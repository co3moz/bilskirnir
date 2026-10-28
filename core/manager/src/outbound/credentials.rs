//! The password of a container's own proxy: a record of its own kind, by container, which no
//! route returns. The container's `proxy` says only whether there is one.
use super::{ProxyChoice, choice};
use crate::App;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The database kind of the passwords.
pub(crate) const CREDENTIALS: &str = "proxy-credentials";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Credentials {
    pub password: String,
}

/// What a request does with the password: `proxy_password` with a value sets it, empty or `null`
/// removes it, and a request without it keeps the one there is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PasswordChange {
    Keep,
    Set(String),
    Clear,
}

impl PasswordChange {
    /// The change a create or settings request asks for.
    pub(crate) fn requested(request: &Map<String, Value>) -> Result<Self> {
        Self::from_value(request.get("proxy_password"))
    }

    pub(crate) fn from_value(value: Option<&Value>) -> Result<Self> {
        match value {
            None => Ok(Self::Keep),
            Some(Value::Null) => Ok(Self::Clear),
            Some(Value::String(text)) if text.is_empty() => Ok(Self::Clear),
            Some(Value::String(text)) => {
                choice::password(text)?;
                Ok(Self::Set(text.clone()))
            }
            Some(_) => bail!("proxy_password must be text"),
        }
    }

    /// Mark whether the new settings' own proxy keeps a password: one set now, or the one it
    /// had while it still names the same host, since a password belongs to the proxy it was
    /// given for.
    pub(crate) fn settle(
        &self,
        old: Option<&ProxyChoice>,
        new: &mut Option<ProxyChoice>,
    ) -> Result<()> {
        let host = new.as_ref().and_then(ProxyChoice::host);
        let Some(ProxyChoice::Custom { password, .. }) = new else {
            if matches!(self, Self::Set(_)) {
                bail!("proxy_password needs a proxy of the container's own (mode custom)");
            }
            return Ok(());
        };
        *password = match self {
            Self::Set(_) => true,
            Self::Clear => false,
            Self::Keep => {
                matches!(old, Some(ProxyChoice::Custom { password: true, .. }))
                    && old.and_then(ProxyChoice::host) == host
            }
        };
        Ok(())
    }
}

impl App {
    /// The password of a container's own proxy, if it keeps one.
    pub(crate) fn proxy_password(&self, container_id: &str) -> Option<String> {
        self.db
            .get::<Credentials>(CREDENTIALS, container_id)
            .ok()
            .map(|credentials| credentials.password)
    }

    /// Keep the password the settings say there is, set by `change` or kept, and remove it
    /// otherwise.
    pub(crate) fn save_proxy_password(
        &self,
        container_id: &str,
        proxy: Option<&ProxyChoice>,
        change: &PasswordChange,
    ) -> Result<()> {
        match (proxy, change) {
            (Some(ProxyChoice::Custom { password: true, .. }), PasswordChange::Set(password)) => {
                self.db.put(
                    CREDENTIALS,
                    container_id,
                    &Credentials {
                        password: password.clone(),
                    },
                )
            }
            (Some(ProxyChoice::Custom { password: true, .. }), _) => Ok(()),
            _ => self.forget_proxy_password(container_id),
        }
    }

    pub(crate) fn forget_proxy_password(&self, container_id: &str) -> Result<()> {
        self.db.remove(CREDENTIALS, container_id)
    }
}
