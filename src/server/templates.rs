use askama::Template;

use crate::share::Share;

#[derive(Template)]
#[template(path = "base.html")]
pub struct BaseTemplate {
    pub title: String,
}

#[derive(Clone)]
pub struct ShareView {
    pub id: String,
    pub token: String,
    pub mode: String,
    pub mode_badge: String,
    pub path: String,
    pub expires: String,
    pub allow_archive: bool,
    pub max_upload_size: String,
}

impl From<&Share> for ShareView {
    fn from(share: &Share) -> Self {
        let (mode, mode_badge) = match &share.mode {
            crate::share::ShareMode::Download => ("Download", "badge-green"),
            crate::share::ShareMode::Upload => ("Upload", "badge-yellow"),
            crate::share::ShareMode::Both => ("Both", "badge-gray"),
        };
        let expires = share
            .expires
            .map(|e| e.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "Never".into());
        let max_upload_size = format_bytes(share.max_upload_size);
        Self {
            id: share.id.clone(),
            token: share.token.clone(),
            mode: mode.into(),
            mode_badge: mode_badge.into(),
            path: share.path.display().to_string(),
            expires,
            allow_archive: share.allow_archive,
            max_upload_size,
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[derive(Template)]
#[template(path = "admin/dashboard.html")]
pub struct DashboardTemplate {
    pub version: String,
    pub tunnel_url: String,
    pub has_tunnel: bool,
    pub share_count: usize,
    pub serve_dir: String,
    pub shares: Vec<ShareView>,
}

#[derive(Template)]
#[template(path = "admin/share_form.html")]
pub struct ShareFormTemplate;

#[derive(Template)]
#[template(path = "admin/share_edit.html")]
pub struct ShareEditTemplate {
    pub share: ShareView,
}
