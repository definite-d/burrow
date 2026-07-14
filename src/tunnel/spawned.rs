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
        &mut self,
        port: u16,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + '_>>
    {
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
                    "--protocol",
                    "http2",
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
            self.child = Some(child);

            Ok(url)
        })
    }

    fn stop(&mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async {
            if let Some(mut child) = self.child.take() {
                tracing::info!("Stopping cloudflared tunnel");
                let _ = child.kill().await;
            }
        })
    }
}

impl Drop for SpawnedTunnel {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.start_kill();
        }
    }
}

fn parse_tunnel_url(line: &str) -> Option<String> {
    let idx = line.find("https://")?;
    let rest = &line[idx..];
    if !rest.starts_with("https://") {
        return None;
    }
    let after_scheme = &rest["https://".len()..];
    let domain_end = after_scheme.find(|c: char| c == '/' || c == '"' || c == '\'' || c.is_whitespace())?;
    let domain = &after_scheme[..domain_end];
    if !domain.ends_with(".trycloudflare.com") {
        return None;
    }
    let full_end = rest.find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == ',')
        .unwrap_or(rest.len());
    let url = rest[..full_end].to_string();
    if url.contains("/tunnel") || url.contains("api.") {
        return None;
    }
    Some(url)
}
