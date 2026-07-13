use std::process::Stdio;
use tokio::process::{Child, Command};

use super::Tunnel;

pub struct SpawnedTunnel {
    child: Option<Child>,
}

impl SpawnedTunnel {
    pub fn new() -> Self {
        Self { child: None }
    }
}

impl Default for SpawnedTunnel {
    fn default() -> Self {
        Self::new()
    }
}

impl Tunnel for SpawnedTunnel {
    fn start(
        &self,
        port: u16,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>>
    {
        let port = port;
        Box::pin(async move {
            let which = Command::new("which")
                .arg("cloudflared")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await;

            match which {
                Ok(out) if !out.status.success() => {
                    return Err(
                        "cloudflared not found in PATH.\n\
                         Install: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/"
                            .into(),
                    );
                }
                Err(e) => {
                    return Err(format!("failed to check for cloudflared: {e}"));
                }
                _ => {}
            }

            let mut child = Command::new("cloudflared")
                .args([
                    "tunnel",
                    "--url",
                    &format!("http://localhost:{port}"),
                    "--no-autoupdate",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| format!("failed to spawn cloudflared: {e}"))?;

            let stderr = child.stderr.take().unwrap();
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = tokio::io::AsyncBufReadExt::lines(reader);

            let url = loop {
                if let Some(line) = lines
                    .next_line()
                    .await
                    .map_err(|e| format!("failed to read cloudflared output: {e}"))?
                {
                    tracing::debug!("cloudflared: {line}");
                    if let Some(url) = parse_tunnel_url(&line) {
                        break url;
                    }
                } else {
                    return Err("cloudflared exited before printing tunnel URL".into());
                }
            };

            let _ = child.stdout.take();

            Ok(url)
        })
    }

    fn stop(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        Box::pin(async {})
    }
}

fn parse_tunnel_url(line: &str) -> Option<String> {
    let idx = line.find("https://")?;
    let rest = &line[idx..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}
