use crate::{
    App,
    error::{Error, Result},
};
use anyhow::{Context, bail};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

#[derive(Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Button {
    #[default]
    Left,
    Middle,
    Right,
}
impl Button {
    fn mask(self) -> u8 {
        match self {
            Self::Left => 1,
            Self::Middle => 2,
            Self::Right => 4,
        }
    }
}
fn one() -> u8 {
    1
}
fn drag_duration() -> u64 {
    300
}
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Move {
        x: u16,
        y: u16,
    },
    Click {
        x: u16,
        y: u16,
        #[serde(default)]
        button: Button,
        #[serde(default = "one")]
        count: u8,
    },
    Drag {
        from_x: u16,
        from_y: u16,
        to_x: u16,
        to_y: u16,
        #[serde(default)]
        button: Button,
        #[serde(default = "drag_duration")]
        duration_ms: u64,
    },
    Scroll {
        x: u16,
        y: u16,
        direction: Direction,
        #[serde(default = "one")]
        steps: u8,
    },
    KeyPress {
        keys: Vec<String>,
    },
    TypeText {
        text: String,
    },
    Wait {
        milliseconds: u64,
    },
}
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub actions: Vec<Action>,
}

pub(crate) fn keysym(key: &str) -> anyhow::Result<u32> {
    if key.chars().count() == 1 {
        let c = key.chars().next().unwrap();
        return match c {
            '\n' | '\r' => Ok(0xff0d),
            '\t' => Ok(0xff09),
            c if !c.is_control() => Ok(if (c as u32) < 256 {
                c as u32
            } else {
                0x01000000 | c as u32
            }),
            _ => Err(anyhow::anyhow!("Unsupported control character")),
        };
    }
    Ok(match key.to_ascii_lowercase().as_str() {
        "enter" | "return" => 0xff0d,
        "tab" => 0xff09,
        "backspace" => 0xff08,
        "escape" | "esc" => 0xff1b,
        "delete" => 0xffff,
        "insert" => 0xff63,
        "home" => 0xff50,
        "end" => 0xff57,
        "pageup" => 0xff55,
        "pagedown" => 0xff56,
        "left" => 0xff51,
        "up" => 0xff52,
        "right" => 0xff53,
        "down" => 0xff54,
        "shift" => 0xffe1,
        "control" | "ctrl" => 0xffe3,
        "alt" => 0xffe9,
        "super" | "meta" | "win" => 0xffeb,
        "space" => 0x20,
        name if name.starts_with('f') => {
            let n: u32 = name[1..].parse().context("Unknown key name")?;
            if !(1..=12).contains(&n) {
                bail!("Function keys must be F1–F12");
            }
            0xffbd + n
        }
        _ => bail!("Unknown key name: {key}"),
    })
}
fn pointer(mask: u8, x: u16, y: u16) -> Vec<u8> {
    let mut bytes = vec![5, mask];
    bytes.extend(x.to_be_bytes());
    bytes.extend(y.to_be_bytes());
    bytes
}
fn key(down: bool, keysym: u32) -> Vec<u8> {
    let mut bytes = vec![4, u8::from(down), 0, 0];
    bytes.extend(keysym.to_be_bytes());
    bytes
}
type Plan = Vec<(Vec<u8>, u64)>;
pub(crate) fn plan(input: &Input, width: u16, height: u16) -> anyhow::Result<Plan> {
    if input.actions.is_empty() || input.actions.len() > 100 {
        bail!("Send 1–100 desktop actions");
    }
    let mut output = Vec::new();
    let (mut delay, mut chars) = (0u64, 0usize);
    let point = |x: u16, y: u16| -> anyhow::Result<()> {
        if x >= width || y >= height {
            bail!("Coordinates ({x}, {y}) are outside the {width}x{height} desktop");
        }
        Ok(())
    };
    for action in &input.actions {
        match action {
            Action::Move { x, y } => {
                point(*x, *y)?;
                output.push((pointer(0, *x, *y), 0));
            }
            Action::Click {
                x,
                y,
                button,
                count,
            } => {
                point(*x, *y)?;
                if !(1..=3).contains(count) {
                    bail!("Click count must be 1–3");
                }
                for _ in 0..*count {
                    output.push((pointer(button.mask(), *x, *y), 40));
                    output.push((pointer(0, *x, *y), 80));
                    delay += 120;
                }
            }
            Action::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
                button,
                duration_ms,
            } => {
                point(*from_x, *from_y)?;
                point(*to_x, *to_y)?;
                if !(100..=2000).contains(duration_ms) {
                    bail!("Drag duration must be 100–2000 milliseconds");
                }
                output.push((pointer(button.mask(), *from_x, *from_y), 0));
                for n in 1..=20 {
                    let interpolate = |a: u16, b: u16| {
                        (i32::from(a) + (i32::from(b) - i32::from(a)) * n / 20) as u16
                    };
                    output.push((
                        pointer(
                            button.mask(),
                            interpolate(*from_x, *to_x),
                            interpolate(*from_y, *to_y),
                        ),
                        duration_ms / 20,
                    ));
                }
                output.push((pointer(0, *to_x, *to_y), 0));
                delay += duration_ms;
            }
            Action::Scroll {
                x,
                y,
                direction,
                steps,
            } => {
                point(*x, *y)?;
                if !(1..=50).contains(steps) {
                    bail!("Scroll steps must be 1–50");
                }
                let mask = match direction {
                    Direction::Up => 8,
                    Direction::Down => 16,
                    Direction::Left => 32,
                    Direction::Right => 64,
                };
                for _ in 0..*steps {
                    output.push((pointer(mask, *x, *y), 10));
                    output.push((pointer(0, *x, *y), 10));
                    delay += 20;
                }
            }
            Action::KeyPress { keys } => {
                if keys.is_empty() || keys.len() > 8 {
                    bail!("A key press needs 1–8 keys");
                }
                let symbols = keys
                    .iter()
                    .map(|k| keysym(k))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                let unique: std::collections::HashSet<_> = symbols.iter().collect();
                if unique.len() != symbols.len() {
                    bail!("Do not repeat the same key in a chord");
                }
                let mut bytes = Vec::new();
                for symbol in &symbols {
                    bytes.extend(key(true, *symbol));
                }
                for symbol in symbols.iter().rev() {
                    bytes.extend(key(false, *symbol));
                }
                output.push((bytes, 0));
            }
            Action::TypeText { text } => {
                chars += text.chars().count();
                if chars > 10000 {
                    bail!("Type at most 10000 characters per request");
                }
                let mut bytes = Vec::new();
                for c in text.chars() {
                    let symbol = keysym(&c.to_string())?;
                    bytes.extend(key(true, symbol));
                    bytes.extend(key(false, symbol));
                }
                output.push((bytes, 0));
            }
            Action::Wait { milliseconds } => {
                if *milliseconds > 1000 {
                    bail!("A wait action is limited to 1000 milliseconds");
                }
                output.push((Vec::new(), *milliseconds));
                delay += milliseconds;
            }
        }
        if delay > 10000 {
            bail!("Desktop actions may wait at most 10 seconds in total");
        }
    }
    Ok(output)
}

async fn connect(address: &str) -> anyhow::Result<(TcpStream, u16, u16)> {
    let mut stream = TcpStream::connect(address).await?;
    stream.set_nodelay(true)?;
    let mut version = [0u8; 12];
    stream.read_exact(&mut version).await?;
    if &version != b"RFB 003.008\n" {
        bail!("Desktop input requires RFB 3.8");
    }
    stream.write_all(b"RFB 003.008\n").await?;
    let count = stream.read_u8().await?;
    if count == 0 {
        bail!("VNC rejected the connection");
    }
    let mut types = vec![0; count as usize];
    stream.read_exact(&mut types).await?;
    if !types.contains(&1) {
        bail!("Managed VNC must support its private no-auth connection");
    }
    stream.write_all(&[1]).await?;
    if stream.read_u32().await? != 0 {
        bail!("VNC handshake failed");
    }
    stream.write_all(&[1]).await?; // Shared session; keep the browser's viewer connected.
    let (width, height) = (stream.read_u16().await?, stream.read_u16().await?);
    let mut pixel_format = [0; 16];
    stream.read_exact(&mut pixel_format).await?;
    let name_len = stream.read_u32().await? as usize;
    if name_len > 65536 {
        bail!("VNC desktop name is too long");
    }
    let mut name = vec![0; name_len];
    stream.read_exact(&mut name).await?;
    // Negotiate Fence to acknowledge that preceding input has been processed.
    let mut encodings = vec![2, 0, 0, 1];
    encodings.extend((-312i32).to_be_bytes());
    stream.write_all(&encodings).await?;
    Ok((stream, width, height))
}
async fn fence(stream: &mut TcpStream) -> anyhow::Result<()> {
    let marker = b"bilskirnir";
    let mut request = vec![248, 0, 0, 0];
    request.extend(0x80000001u32.to_be_bytes());
    request.push(marker.len() as u8);
    request.extend(marker);
    stream.write_all(&request).await?;
    stream.flush().await?;
    loop {
        match stream.read_u8().await? {
            248 => {
                let mut padding = [0; 3];
                stream.read_exact(&mut padding).await?;
                let flags = stream.read_u32().await?;
                let length = stream.read_u8().await?;
                let mut data = vec![0; length as usize];
                stream.read_exact(&mut data).await?;
                if flags & 0x80000000 == 0 && data == marker {
                    return Ok(());
                }
                if flags & 0x80000000 != 0 {
                    let mut response = vec![248, 0, 0, 0];
                    response.extend((flags & 3).to_be_bytes());
                    response.push(length);
                    response.extend(data);
                    stream.write_all(&response).await?;
                }
            }
            2 => {} // Bell.
            3 => {
                let mut padding = [0; 3];
                stream.read_exact(&mut padding).await?;
                let length = stream.read_u32().await? as usize;
                if length > 65536 {
                    bail!("VNC clipboard message exceeds the input limit");
                }
                let mut text = vec![0; length];
                stream.read_exact(&mut text).await?;
            }
            message => bail!("Unexpected VNC message during input acknowledgement: {message}"),
        }
    }
}
pub async fn input(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<Input>,
) -> Result<Json<Value>> {
    let _permit = app.desktop_inputs.try_acquire().map_err(|_| {
        Error(
            StatusCode::TOO_MANY_REQUESTS,
            "Four desktop input requests are already active".into(),
        )
    })?;
    let lock = app.resource_lock(&format!("environment:{id}"));
    let _guard = lock.lock().await;
    let env = app.environment(&id)?;
    let address = app.address(&env).await?;
    let result=tokio::time::timeout(Duration::from_secs(20),async {
        let (mut stream,width,height)=connect(&address).await?;
        let steps=plan(&body,width,height)?;
        for (bytes,delay) in steps {
            stream.write_all(&bytes).await?;
            if delay>0 { tokio::time::sleep(Duration::from_millis(delay)).await; }
        }
        fence(&mut stream).await?;
        stream.shutdown().await?;
        anyhow::Ok(json!({"environment_id":id,"applied_actions":body.actions.len(),"width":width,"height":height}))
    }).await.context("VNC input timed out; inspect the desktop before retrying")??;
    Ok(Json(result))
}
