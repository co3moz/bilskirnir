//! A container's machine identity: the hostname, MAC address and machine ID that software inside
//! the desktop sees, and that node-locked licenses bind to.
//!
//! Docker gives every container it creates a hostname from the container's ID, a MAC address of
//! its own on each network, regenerated at every start, and whatever machine ID the image
//! carries; the catalog images carry one that every container of a release shares. A reset, a
//! settings change that recreates the container and a retry all create a new container, so
//! without this the desktop would become another machine after each of them. The record keeps
//! all three, and every container created for it gets them: the hostname and the MAC address as
//! creation flags, the same MAC address again on the internet network, and the machine ID written
//! into the created container before it starts.
//!
//! The internet network is connected after creation, and only the Engine API takes a MAC address
//! for that: Docker Engine 29 (API 1.54) and later use it, earlier releases accept it and choose
//! their own, and the CLI has no option for it. Both networks get the same address because Docker
//! does not keep the order of a container's interfaces, so `eth0` can be either of them.
use crate::{App, model::*};
use anyhow::{Context, Result, bail};
use http_body_util::BodyExt;
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};

/// One RFC 1123 label: the desktop's name without a domain.
pub fn hostname(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 63
        || !bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
        || bytes[0] == b'-'
        || bytes[bytes.len() - 1] == b'-'
    {
        bail!(
            "Hostname must be 1 to 63 letters, digits and hyphens, and cannot start or end with a hyphen"
        );
    }
    Ok(())
}

/// Six lowercase hex pairs separated by colons, the address of one interface: not a group
/// address, and not all zeros.
pub fn mac_address(value: &str) -> Result<()> {
    let octets = value
        .split(':')
        .map(|pair| {
            (pair.len() == 2 && pair.bytes().all(lower_hex))
                .then(|| u8::from_str_radix(pair, 16).ok())
                .flatten()
        })
        .collect::<Option<Vec<u8>>>()
        .filter(|octets| octets.len() == 6)
        .context(
            "MAC address must be six hex pairs separated by colons, such as 02:42:ac:11:00:02",
        )?;
    if octets[0] & 1 == 1 {
        bail!("MAC address must be a unicast address, with an even first pair");
    }
    if octets.iter().all(|octet| *octet == 0) {
        bail!("MAC address cannot be all zeros");
    }
    Ok(())
}

/// 32 lowercase hex digits, the form of `/etc/machine-id`. All zeros is systemd's null ID.
pub fn machine_id(value: &str) -> Result<()> {
    if value.len() != 32 || !value.bytes().all(lower_hex) {
        bail!("Machine ID must be 32 hex digits");
    }
    if value.bytes().all(|b| b == b'0') {
        bail!("Machine ID cannot be all zeros");
    }
    Ok(())
}

fn lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

/// A MAC address as other tools print it, in capitals or with hyphens, in the form kept.
pub(crate) fn mac_input<'de, D: Deserializer<'de>>(input: D) -> Result<Option<String>, D::Error> {
    let value = Option::<String>::deserialize(input)?;
    Ok(value.map(|mac| mac.to_ascii_lowercase().replace('-', ":")))
}

/// A machine ID written as a UUID, or in capitals, in the form kept.
pub(crate) fn machine_id_input<'de, D: Deserializer<'de>>(
    input: D,
) -> Result<Option<String>, D::Error> {
    let value = Option::<String>::deserialize(input)?;
    Ok(value.map(|id| id.to_ascii_lowercase().replace('-', "")))
}

/// A random MAC address that is locally administered and unicast, so no vendor's device has it.
pub(crate) fn generate_mac() -> String {
    let mut octets = [0; 6];
    octets.copy_from_slice(&uuid::Uuid::new_v4().as_bytes()[..6]);
    octets[0] = (octets[0] & 0xfc) | 0x02;
    octets.map(|octet| format!("{octet:02x}")).join(":")
}

/// A random machine ID, as systemd makes one: a version 4 UUID without hyphens.
pub(crate) fn generate_machine_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Give a new container the parts of its identity its request left out. Its hostname is its
/// Docker container name, unique on the installation and short enough for a label.
pub(crate) fn assign(spec: &mut ContainerSpec, container: &str) {
    spec.hostname.get_or_insert_with(|| container.to_owned());
    spec.mac_address.get_or_insert_with(generate_mac);
    spec.machine_id.get_or_insert_with(generate_machine_id);
}

/// Complete the identity of a record saved before containers kept one, before another container
/// is created for it. It takes the hostname and MAC address of `info`, the container being
/// replaced, when that is still there, so they change no more from then on. Its machine ID stays
/// unset: such a container keeps the one its image or its commit carries, which never changed.
pub(crate) fn adopt(env: &mut Container, info: Option<&Value>) {
    let info = info.filter(|info| {
        info["Config"]["Labels"][LABEL] == "true"
            && info["Config"]["Labels"]["thoric.environment"] == env.id.as_str()
    });
    if env.spec.hostname.is_none() {
        let current = info
            .and_then(|info| info["Config"]["Hostname"].as_str())
            .filter(|name| hostname(name).is_ok());
        env.spec.hostname = Some(current.map_or_else(|| env.container.clone(), str::to_owned));
    }
    if env.spec.mac_address.is_none() {
        // A stopped container has no address: Docker chooses one when it starts.
        let current = info
            .and_then(|info| {
                info["NetworkSettings"]["Networks"][&env.private_network]["MacAddress"].as_str()
            })
            .filter(|mac| mac_address(mac).is_ok());
        env.spec.mac_address = Some(current.map_or_else(generate_mac, str::to_owned));
    }
}

/// The files `docker cp` writes for a machine ID: systemd's and D-Bus's copies, as the desktop
/// images have them, owned by root and readable by everyone. Writing a file replaces a symbolic
/// link in its place rather than following it.
pub(crate) fn machine_id_archive(id: &str) -> Vec<u8> {
    let data = format!("{id}\n");
    let mut archive = Vec::new();
    for name in ["etc/machine-id", "var/lib/dbus/machine-id"] {
        archive.extend(tar_entry(name, data.as_bytes(), now()));
    }
    // Two empty blocks end an archive.
    archive.resize(archive.len() + 1024, 0);
    archive
}

/// A regular file as a USTAR entry, padded to whole blocks.
fn tar_entry(name: &str, data: &[u8], mtime: u64) -> Vec<u8> {
    let mut header = [0u8; 512];
    header[..name.len()].copy_from_slice(name.as_bytes());
    for (field, value) in [
        (100..108, 0o644),
        (108..116, 0),
        (116..124, 0),
        (124..136, data.len() as u64),
        (136..148, mtime),
    ] {
        let digits = format!("{value:0width$o}", width = field.len() - 1);
        header[field.start..field.start + digits.len()].copy_from_slice(digits.as_bytes());
    }
    header[156] = b'0';
    header[257..265].copy_from_slice(b"ustar\x0000");
    header[148..156].fill(b' ');
    let sum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    let mut entry = header.to_vec();
    entry.extend_from_slice(data);
    entry.resize(entry.len().div_ceil(512) * 512, 0);
    entry
}

/// The Engine API request that connects a container to a network with a MAC address.
pub(crate) fn connect_request(container: &str, mac: &str) -> Value {
    json!({"Container": container, "EndpointConfig": {"MacAddress": mac}})
}

/// Send one Engine API request with a JSON body and check its answer.
pub(crate) async fn engine_post<S>(io: S, path: &str, body: &Value) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(io))
            .await
            .context("Docker API handshake failed")?;
    let connection = tokio::spawn(connection);
    let exchange = async {
        let request = hyper::Request::post(path)
            .header(hyper::header::HOST, "docker")
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(http_body_util::Full::new(hyper::body::Bytes::from(
                serde_json::to_vec(body)?,
            )))?;
        let response = sender
            .send_request(request)
            .await
            .context("Docker API request failed")?;
        let status = response.status();
        let body = http_body_util::Limited::new(response.into_body(), 65_536)
            .collect()
            .await
            .map(|body| body.to_bytes())
            .unwrap_or_default();
        if !status.is_success() {
            let message = serde_json::from_slice::<Value>(&body)
                .ok()
                .and_then(|value| value["message"].as_str().map(str::to_owned))
                .unwrap_or_else(|| String::from_utf8_lossy(&body).trim().to_owned());
            bail!("Docker: {message} ({status})");
        }
        anyhow::Ok(())
    };
    let result = tokio::time::timeout(Duration::from_secs(120), exchange)
        .await
        .unwrap_or_else(|_| Err(anyhow::anyhow!("Docker operation timed out")));
    connection.abort();
    result
}

impl App {
    /// Write the record's machine ID into a container before it starts. A record saved before
    /// containers kept one has none, and its container keeps the file it has.
    pub(crate) async fn write_machine_id(&self, env: &Container) -> Result<()> {
        let Some(id) = &env.spec.machine_id else {
            return Ok(());
        };
        let args = ["cp".into(), "-".into(), format!("{}:/", env.container)];
        let out = crate::process::capture_input(
            self.command(&args),
            Duration::from_secs(120),
            65_536,
            crate::process::Overflow::Tail,
            Some(machine_id_archive(id)),
        )
        .await?;
        if !out.status.success() {
            bail!(
                "Docker could not write the machine ID: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    }

    /// Connect a container to one of its networks after creation, with the record's MAC address
    /// where the Engine API is reachable: the internet network, or a private network a retry finds
    /// disconnected. Without the socket the CLI connects it, and Docker chooses the address.
    pub(crate) async fn connect_network(
        &self,
        env: &Container,
        network: &str,
        container: &str,
    ) -> Result<()> {
        if let (Some(mac), Some(socket)) = (&env.spec.mac_address, &self.config.docker_socket) {
            match crate::pulls::connect(socket).await {
                Ok(stream) => {
                    let path = format!("/networks/{}/connect", crate::pulls::query_value(network));
                    return engine_post(stream, &path, &connect_request(container, mac)).await;
                }
                Err(error) => tracing::debug!(
                    "Connecting {container} to {network} through the CLI; {} is unavailable: {error}",
                    socket.display()
                ),
            }
        }
        self.docker(&["network", "connect", network, container])
            .await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
