//! A container's two networks, the internal private one it shares with the manager alone and the
//! one that reaches the internet, and its addresses there.
use crate::{App, model::*};
use anyhow::{Context, Result, bail};
use serde_json::Value;

impl App {
    pub fn check_network(env: &Container, info: &Value, internal: bool) -> Result<()> {
        if info["Labels"][LABEL] != "true"
            || info["Labels"]["thoric.environment"] != env.id
            || info["Internal"] != internal
            || info["Driver"] != "bridge"
        {
            bail!("Network ownership or type does not match");
        }
        Ok(())
    }
    pub(super) async fn ensure_network(
        &self,
        env: &Container,
        name: &str,
        internal: bool,
    ) -> Result<Value> {
        if let Some(info) = self.optional_inspect("network", name).await? {
            Self::check_network(env, &info, internal)?;
            return Ok(info);
        }
        let mut args = vec![
            "network",
            "create",
            "--label",
            "thoric.managed=true",
            "--label",
        ];
        let owner = format!("thoric.environment={}", env.id);
        args.push(&owner);
        if internal {
            args.push("--internal");
        }
        args.push(name);
        let id = self.docker(&args).await?;
        self.inspect("network", String::from_utf8_lossy(&id).trim())
            .await
    }
    pub async fn set_internet(&self, env: &mut Container, enabled: bool) -> Result<()> {
        let info = self.owned(env).await?;
        let connected = info["NetworkSettings"]["Networks"]
            .get(&env.egress_network)
            .is_some();
        let network = self.inspect("network", &env.egress_network).await?;
        Self::check_network(env, &network, false)?;
        let container_id = info["Id"].as_str().context("Missing container ID")?;
        let network_id = network["Id"].as_str().context("Missing network ID")?;
        if enabled && !connected {
            self.connect_network(env, network_id, container_id).await?;
        } else if !enabled && connected {
            self.docker(&["network", "disconnect", network_id, container_id])
                .await?;
        }
        env.spec.internet = enabled;
        self.db.put(CONTAINERS, &env.id, env)?;
        Ok(())
    }
    pub async fn address(&self, env: &Container) -> Result<String> {
        let info = self.owned(env).await?;
        if info["State"]["Running"] != true {
            bail!("Container is not running");
        }
        Ok(format!("{}:5900", private_ip(env, &info)?))
    }
}

/// The desktop's address on its private network, which only the manager shares with it, from
/// `docker inspect` of its container.
pub(crate) fn private_ip(env: &Container, info: &Value) -> Result<std::net::Ipv4Addr> {
    info["NetworkSettings"]["Networks"][&env.private_network]["IPAddress"]
        .as_str()
        .context("Missing private network address")?
        .parse()
        .context("Invalid Docker IP address")
}
/// The IPv4 subnet of a desktop's private network, from `docker network inspect`. The desktop's
/// VNC server listens only on its address there, since VNC asks no password and the private
/// network is the one only the manager shares with it.
pub(crate) fn private_subnet(network: &Value) -> Option<&str> {
    network["IPAM"]["Config"]
        .as_array()?
        .iter()
        .filter_map(|config| config["Subnet"].as_str())
        .find(|subnet| {
            subnet
                .split_once('/')
                .is_some_and(|(address, _)| address.parse::<std::net::Ipv4Addr>().is_ok())
        })
}
