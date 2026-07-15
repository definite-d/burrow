use std::process::Stdio;
use tokio::process::{Child, Command};
use tokio::sync::oneshot;

pub struct SpawnedTunnel {
    child: Option<Child>,
}

impl SpawnedTunnel {
    pub fn new() -> Self {
        Self { child: None }
    }

    pub async fn start(&mut self, port: u16) -> Result<String, String> {
        let cfd = find_cloudflared().await?;

        let mut child = Command::new(cfd)
            .args([
                "tunnel",
                "--url",
                &format!("http://localhost:{port}"),
                "--no-autoupdate",
                "--protocol",
                "http2",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn cloudflared: {e}"))?;

        let stderr = child.stderr.take().unwrap();
        let reader = tokio::io::BufReader::new(stderr);
        let mut lines = tokio::io::AsyncBufReadExt::lines(reader);

        let (tx, rx) = oneshot::channel();

        // Background task: drain stderr (keeps pipe open) and forward the URL.
        tokio::spawn(async move {
            let mut url_tx = Some(tx);
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!("cloudflared: {line}");
                if let Some(url) = parse_tunnel_url(&line) {
                    if let Some(tx) = url_tx.take() {
                        let _ = tx.send(url);
                    }
                }
            }
        });

        let url = rx.await.map_err(|_| "cloudflared exited before printing tunnel URL")?;
        self.child = Some(child);
        Ok(url)
    }

    pub async fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            tracing::info!("Stopping cloudflared tunnel");
            let _ = child.kill().await;
        }
    }
}

impl Default for SpawnedTunnel {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SpawnedTunnel {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.start_kill();
        }
    }
}

async fn find_cloudflared() -> Result<String, String> {
    // Try PATH first (standard behavior)
    if Command::new("cloudflared")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok()
    {
        return Ok("cloudflared".into());
    }

    // Search candidates: next to binary, then CWD
    let candidates = [
        std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())),
        std::env::current_dir().ok(),
    ];

    for dir in candidates.into_iter().flatten() {
        // Exact match first
        let exact = if cfg!(windows) { "cloudflared.exe" } else { "cloudflared" };
        let path = dir.join(exact);
        if path.exists() {
            return Ok(path.to_string_lossy().into_owned());
        }
        // Glob fallback for renamed binaries (e.g. cloudflared-windows-amd64.exe)
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let s = entry.file_name().to_string_lossy().to_lowercase();
                if s.starts_with("cloudflared") && (s.ends_with(".exe") || !cfg!(windows)) {
                    return Ok(entry.path().to_string_lossy().into_owned());
                }
            }
        }
    }

    Err(
        "cloudflared not found in PATH or next to the binary.\n\
         Install: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/"
            .into(),
    )
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
