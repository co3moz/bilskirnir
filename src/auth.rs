use crate::{
    App,
    error::{Error, Result},
};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    Json,
    extract::{FromRequestParts, Request, State},
    http::{HeaderMap, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

/// Failed sign-ins one address may make in a minute.
const LOGIN_ATTEMPTS: usize = 20;
/// Failed sign-ins kept in all, so a flood from many addresses stays bounded in memory and is
/// refused as a whole.
const LOGIN_ATTEMPTS_TOTAL: usize = 500;

/// The address a request came from, when the listener records one. A request made without a
/// connection, as the tests do, has none, and such requests share one bucket.
pub struct Peer(pub Option<IpAddr>);
impl<S: Send + Sync> FromRequestParts<S> for Peer {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        Ok(Peer(
            parts
                .extensions
                .get::<axum::extract::ConnectInfo<SocketAddr>>()
                .map(|info| info.0.ip()),
        ))
    }
}

pub fn cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|p| {
            p.trim()
                .strip_prefix("bilskirnir_session=")
                .map(str::to_owned)
        })
}
/// What a long transfer answers to once it has begun. A dashboard request's session cookie ends
/// it when the session signs out or expires. A one-time transfer link was checked when its
/// request arrived and belongs to no session, so nothing revokes it. No request extracts `Link`;
/// only the transfer route passes it.
#[derive(Clone, Debug, PartialEq)]
pub enum Session {
    Cookie(Option<String>),
    Link,
}
impl From<Option<String>> for Session {
    fn from(token: Option<String>) -> Self {
        Self::Cookie(token)
    }
}
impl<S: Send + Sync> FromRequestParts<S> for Session {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        Ok(Self::Cookie(cookie(&parts.headers)))
    }
}
pub fn origin_matches(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    let Ok(uri) = origin.parse::<axum::http::Uri>() else {
        return false;
    };
    matches!(uri.scheme_str(), Some("http" | "https"))
        && uri.path() == "/"
        && uri.query().is_none()
        && uri
            .authority()
            .is_some_and(|a| a.as_str().eq_ignore_ascii_case(host))
}
impl App {
    pub fn host_allowed(&self, headers: &HeaderMap) -> bool {
        let Some(host) = headers
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.parse::<axum::http::uri::Authority>().ok())
        else {
            return false;
        };
        if host.as_str().contains('@') {
            return false;
        }
        let name = host
            .host()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase();
        if name == "localhost" || self.config.host_allowed(&name) {
            return true;
        }
        match name.parse::<std::net::IpAddr>() {
            Ok(std::net::IpAddr::V4(ip)) => {
                ip.is_loopback() || ip.is_private() || ip.is_link_local()
            }
            Ok(std::net::IpAddr::V6(ip)) => {
                ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
            }
            _ => false,
        }
    }
    /// Resolves once `session` no longer authorizes the work it started: at once for a request
    /// without a session cookie, never while sign-in is off or for a transfer link.
    pub async fn session_revoked(&self, session: impl Into<Session>) {
        let token = match session.into() {
            _ if !self.config.auth => None,
            Session::Link => None,
            Session::Cookie(None) => return,
            Session::Cookie(Some(token)) => Some(token),
        };
        let Some(token) = token else {
            std::future::pending::<()>().await;
            return;
        };
        loop {
            let notified = self.session_events.notified();
            tokio::pin!(notified);
            // Register before checking: logout cannot fall into a check/wait gap.
            notified.as_mut().enable();
            let expiry = self.sessions.lock().unwrap().get(&token).copied();
            let Some(expiry) = expiry.filter(|t| *t > Instant::now()) else {
                return;
            };
            tokio::select! { _=tokio::time::sleep_until(expiry.into())=>return, _=&mut notified=>{} }
        }
    }
    pub fn authenticated(&self, headers: &HeaderMap) -> bool {
        if !self.config.auth {
            return true;
        }
        let Some(token) = cookie(headers) else {
            return false;
        };
        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain(|_, expiry| *expiry > Instant::now());
        sessions.contains_key(&token)
    }
}
pub async fn check_host(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    if !app.host_allowed(req.headers()) {
        return Error(
            StatusCode::FORBIDDEN,
            "Host not allowed. Add your hostname to ALLOWED_HOSTS.".into(),
        )
        .into_response();
    }
    next.run(req).await
}
pub async fn protect(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    if !app.authenticated(req.headers()) {
        return Error(StatusCode::UNAUTHORIZED, "Sign in required".into()).into_response();
    }
    if !matches!(
        *req.method(),
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    ) && (req
        .headers()
        .get("x-bilskirnir-request")
        .is_none_or(|v| v != "1")
        || (req.headers().contains_key(header::ORIGIN) && !origin_matches(req.headers())))
    {
        return Error(StatusCode::FORBIDDEN, "Request origin rejected".into()).into_response();
    }
    next.run(req).await
}
#[derive(Deserialize)]
pub struct Login {
    pub username: String,
    pub password: String,
}
pub async fn login(
    State(app): State<Arc<App>>,
    Peer(peer): Peer,
    headers: HeaderMap,
    Json(body): Json<Login>,
) -> Result<Response> {
    if headers.get("x-bilskirnir-request").is_none_or(|v| v != "1")
        || (headers.contains_key(header::ORIGIN) && !origin_matches(&headers))
    {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Request origin rejected".into(),
        ));
    }
    if !app.config.auth {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    if body.username.len() > 256 || body.password.len() > 4096 {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "Credentials are too long".into(),
        ));
    }
    {
        // Failures count per address, so one guessing neighbour does not lock the administrator
        // out from another machine; the total bounds what is kept.
        let mut attempts = app.login_attempts.lock().unwrap();
        attempts.retain(|(_, at)| at.elapsed() < Duration::from_secs(60));
        let from_peer = attempts.iter().filter(|(from, _)| *from == peer).count();
        if from_peer >= LOGIN_ATTEMPTS || attempts.len() >= LOGIN_ATTEMPTS_TOTAL {
            return Err(Error(
                StatusCode::TOO_MANY_REQUESTS,
                "Try again in one minute".into(),
            ));
        }
        attempts.push((peer, Instant::now()));
    }
    let hash = app.config.password_hash.clone().unwrap();
    let valid_user = body.username == app.config.username;
    let valid = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).is_ok_and(|h| {
            Argon2::default()
                .verify_password(body.password.as_bytes(), &h)
                .is_ok()
        })
    })
    .await
    .map_err(|e| anyhow::anyhow!(e))?;
    if !valid || !valid_user {
        return Err(Error(
            StatusCode::UNAUTHORIZED,
            "Invalid username or password".into(),
        ));
    }
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    if let Some(old) = cookie(&headers) {
        app.sessions.lock().unwrap().remove(&old);
        app.session_events.notify_waiters();
    }
    app.sessions
        .lock()
        .unwrap()
        .insert(token.clone(), Instant::now() + Duration::from_secs(43200));
    let secure = if app.config.secure_cookie {
        "; Secure"
    } else {
        ""
    };
    Ok((
        [(
            header::SET_COOKIE,
            format!(
                "bilskirnir_session={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=43200{secure}"
            ),
        )],
        StatusCode::NO_CONTENT,
    )
        .into_response())
}
pub async fn logout(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if let Some(token) = cookie(&headers) {
        app.sessions.lock().unwrap().remove(&token);
        app.session_events.notify_waiters();
    }
    (
        [(
            header::SET_COOKIE,
            "bilskirnir_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0",
        )],
        StatusCode::NO_CONTENT,
    )
        .into_response()
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
