mod api;
mod apps;
mod audio;
mod auth;
mod capture;
mod config;
mod db;
mod desktop_input;
mod docker;
mod error;
mod exec;
mod files;
mod gamepad;
mod jobs;
mod mcp;
mod media;
mod model;
mod process;
mod profiles;
mod pulls;
mod reconfigure;
mod recordings;
mod terminal;
mod uploads;
mod waits;
mod windows;

use axum::{
    Router,
    http::{HeaderValue, header},
    middleware,
    routing::{get, post},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};
use tower_http::{services::ServeDir, trace::TraceLayer};

pub struct App {
    config: config::Config,
    db: db::Db,
    sessions: Mutex<HashMap<String, Instant>>,
    login_attempts: Mutex<Vec<Instant>>,
    operations: Arc<tokio::sync::Mutex<()>>,
    resource_locks: Mutex<HashMap<String, std::sync::Weak<tokio::sync::Mutex<()>>>>,
    session_events: tokio::sync::Notify,
    screenshots: tokio::sync::Semaphore,
    uploads: tokio::sync::Semaphore,
    desktop_inputs: tokio::sync::Semaphore,
}
fn router(app: Arc<App>) -> Router {
    let protected = Router::new()
        .route("/api/logout", post(auth::logout))
        .route("/api/settings", get(api::settings))
        .route("/api/profiles", get(api::profiles))
        .route("/api/profiles/{id}/build", post(api::build_profile))
        .route("/api/profiles/{id}/pull", post(api::pull_profile))
        .route("/api/system", get(api::system))
        .route(
            "/api/environments",
            get(api::environments).post(api::create),
        )
        .route(
            "/api/environments/{id}",
            get(api::environment).delete(api::remove),
        )
        .route("/api/environments/{id}/action", post(api::action))
        .route("/api/environments/{id}/retry", post(api::retry))
        .route("/api/environments/{id}/internet", post(api::internet))
        .route(
            "/api/environments/{id}/settings",
            post(api::update_settings),
        )
        .route("/api/environments/{id}/snapshot", post(api::snapshot))
        .route("/api/environments/{id}/exec", post(exec::execute))
        .route("/api/environments/{id}/files/read", post(files::read))
        .route("/api/environments/{id}/files/write", post(files::write))
        .route("/api/environments/{id}/files/list", post(files::list))
        .route("/api/environments/{id}/files/move", post(files::move_))
        .route("/api/environments/{id}/files/delete", post(files::delete))
        .route(
            "/api/environments/{id}/files/download",
            get(files::download),
        )
        .route("/api/environments/{id}/audio-capture", post(audio::capture))
        .route("/api/environments/{id}/audio-inject", post(audio::inject))
        .route(
            "/api/environments/{id}/terminals",
            get(terminal::list).post(terminal::open),
        )
        .route(
            "/api/environments/{id}/terminals/{terminal}/input",
            post(terminal::input),
        )
        .route(
            "/api/environments/{id}/terminals/{terminal}/output",
            get(terminal::output),
        )
        .route(
            "/api/environments/{id}/terminals/{terminal}/close",
            post(terminal::close),
        )
        .route("/api/environments/{id}/ui", get(windows::inspect))
        .route("/api/environments/{id}/gamepad", post(gamepad::input))
        .route("/api/environments/{id}/input", post(desktop_input::input))
        .route("/api/environments/{id}/act-and-observe", post(capture::act))
        .route("/api/environments/{id}/screenshots", post(capture::save))
        .route(
            "/api/environments/{id}/applications",
            get(apps::list).post(apps::launch),
        )
        .route(
            "/api/environments/{id}/applications/{application}",
            get(apps::get),
        )
        .route(
            "/api/environments/{id}/applications/{application}/logs",
            get(apps::logs),
        )
        .route(
            "/api/environments/{id}/applications/{application}/stop",
            post(apps::stop),
        )
        .route(
            "/api/environments/{id}/applications/{application}/diagnostics",
            get(apps::diagnostics),
        )
        .route(
            "/api/environments/{id}/applications/{application}/metrics",
            get(apps::metrics),
        )
        .route("/api/environments/{id}/wait", post(waits::wait))
        .route(
            "/api/environments/{id}/recordings",
            get(recordings::list).post(recordings::start),
        )
        .route(
            "/api/environments/{id}/recordings/{recording}/stop",
            post(recordings::stop),
        )
        .route("/api/environments/{id}/artifacts", post(recordings::export))
        .route("/api/environments/{id}/reset", post(api::reset))
        .route("/api/environments/{id}/windows", get(windows::list))
        .route(
            "/api/environments/{id}/windows/{window}/focus",
            post(windows::focus),
        )
        .route(
            "/api/environments/{id}/windows/{window}/geometry",
            post(windows::geometry),
        )
        .route("/api/environments/{id}/logs", get(api::logs))
        .route("/api/environments/{id}/diagnostics", get(api::diagnostics))
        .route("/api/environments/{id}/screenshot", get(api::screenshot))
        .route(
            "/api/environments/{id}/files",
            post(uploads::upload).layer(tower_http::limit::RequestBodyLimitLayer::new(
                app.config.upload_limit as usize,
            )),
        )
        .route("/api/environments/{id}/vnc", get(media::vnc))
        .route(
            "/api/environments/{id}/audio/{direction}",
            get(media::audio),
        )
        .route("/api/images", get(api::images).post(api::register_image))
        .route("/api/images/build", post(api::build))
        .route("/api/volumes", get(api::volumes).post(api::create_volume))
        .route("/api/jobs", get(api::jobs))
        .route("/api/jobs/{id}", get(api::job))
        .route_layer(middleware::from_fn_with_state(app.clone(), auth::protect));
    Router::new().merge(protected).merge(mcp::router(app.clone()))
        .route("/api/session",get(api::session))
        .route("/api/login",post(auth::login))
        .route("/healthz",get(||async { "ok" }))
        .fallback_service(ServeDir::new(&app.config.web).append_index_html_on_directories(true))
        .layer(axum::extract::DefaultBodyLimit::max(1_100_000))
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn_with_state(app.clone(), auth::check_host))
        .layer(middleware::from_fn(|req:axum::extract::Request,next:middleware::Next|async move {
            let api=req.uri().path().starts_with("/api/") || req.uri().path().starts_with("/mcp");
            // The interface build names every file under /assets after its content, so a found
            // one never changes. Everything else, index.html above all, is revalidated on each
            // load: after an upgrade a cached page would ask for assets that no longer exist.
            let assets=req.uri().path().starts_with("/assets/");
            let mut res=next.run(req).await;
            let found=res.status().is_success();
            let h=res.headers_mut();
            h.insert(header::X_CONTENT_TYPE_OPTIONS,HeaderValue::from_static("nosniff"));
            h.insert(header::REFERRER_POLICY,HeaderValue::from_static("same-origin"));
            h.insert(header::CONTENT_SECURITY_POLICY,HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; worker-src 'self' blob:; frame-ancestors 'self'"));
            if api { h.insert(header::CACHE_CONTROL,HeaderValue::from_static("no-store")); }
            else if assets && found { h.insert(header::CACHE_CONTROL,HeaderValue::from_static("public, max-age=31536000, immutable")); }
            else { h.insert(header::CACHE_CONTROL,HeaderValue::from_static("no-cache")); }
            res
        }))
        .with_state(app)
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "bilskirnir=info,tower_http=info".into()),
        )
        .init();
    let config = config::Config::load()?;
    if config.mcp_enabled && config.mcp_token.is_none() {
        tracing::warn!(
            "MCP_AUTH_MODE=none: /mcp accepts requests without a token from any allowed host"
        );
    }
    tokio::fs::create_dir_all(&config.data).await?;
    uploads::cleanup_temporary_files(&config.data).await?;
    let db = db::Db::open(&config.data.join("state.sqlite"))?;
    // Interrupted work is explicit. Never silently retry a user script or remove a container.
    for mut job in db.list::<model::Job>("job")? {
        if job.status == "queued" || job.status == "running" {
            job.status = "interrupted".into();
            job.finished_at = Some(model::now());
            job.progress = None;
            job.log.push_str("\nService restarted. The Docker operation may have continued; check resources before retrying.\n");
            job.trim_log();
            db.put("job", &job.id, &job)?;
        }
    }
    let listen = config.listen.clone();
    let app = Arc::new(App {
        config,
        db,
        sessions: Mutex::new(HashMap::new()),
        login_attempts: Mutex::new(Vec::new()),
        operations: Arc::new(tokio::sync::Mutex::new(())),
        resource_locks: Mutex::new(HashMap::new()),
        session_events: tokio::sync::Notify::new(),
        screenshots: tokio::sync::Semaphore::new(2),
        uploads: tokio::sync::Semaphore::new(2),
        desktop_inputs: tokio::sync::Semaphore::new(4),
    });
    let reconcile = app.clone();
    let cleanup = app.clone();
    tokio::spawn(async move {
        if let Ok(jobs) = cleanup.db.list::<model::Job>("job") {
            for job in jobs
                .into_iter()
                .filter(|j| j.kind == "build" && j.status != "running" && j.status != "queued")
            {
                if let Err(e) = cleanup.cleanup_build(&job.id).await {
                    tracing::debug!("Previous build cleanup {}: {e:#}", job.id);
                }
            }
        }
    });
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(20));
        loop {
            tick.tick().await;
            if let Err(e) = reconcile_networks(&reconcile).await {
                tracing::debug!("Docker reconciliation: {e}");
            }
        }
    });
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    tracing::info!("Bilskirnir listening: {listen}");
    axum::serve(listener, router(app))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
async fn reconcile_networks(app: &App) -> anyhow::Result<()> {
    let Some(manager) = &app.config.manager else {
        return Ok(());
    };
    let m = app.inspect("container", manager).await?;
    if m["Config"]["Labels"]["io.wsl-desktop.manager"] != "true" {
        anyhow::bail!("Missing manager label");
    }
    for mut env in app.db.list::<model::Environment>("environment")? {
        let lock = app.resource_lock(&format!("environment:{}", env.id));
        let Ok(_guard) = lock.try_lock() else {
            continue;
        };
        // Reload after taking the lock; a queued mutation may have removed/updated this record.
        env = match app.environment(&env.id) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let result = async {
            let info = app.owned(&env).await?;
            let network = app.inspect("network", &env.private_network).await?;
            App::check_network(&env, &network, true)?;
            let egress = app.inspect("network", &env.egress_network).await?;
            App::check_network(&env, &egress, false)?;
            if m["NetworkSettings"]["Networks"]
                .get(&env.private_network)
                .is_none()
            {
                app.docker(&[
                    "network",
                    "connect",
                    network["Id"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Missing network ID"))?,
                    m["Id"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Missing manager ID"))?,
                ])
                .await?;
            }
            let actual = info["NetworkSettings"]["Networks"]
                .get(&env.egress_network)
                .is_some();
            if env.spec.internet != actual {
                env.spec.internet = actual;
                app.db.put("environment", &env.id, &env)?;
            }
            anyhow::Ok(())
        }
        .await;
        if let Err(e) = result {
            tracing::debug!("Environment {} network reconciliation: {e:#}", env.id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
