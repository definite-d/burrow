use askama::Template;

use crate::share::Share;

pub fn qr_svg(url: &str) -> String {
    use qrcode::QrCode;
    use qrcode::render::svg;
    let code = match QrCode::new(url.as_bytes()) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    code.render::<svg::Color>()
        .min_dimensions(160, 160)
        .max_dimensions(200, 200)
        .build()
}

pub fn full_share_url(tunnel_url: &str, token: &str) -> String {
    if tunnel_url.is_empty() {
        format!("/share/{token}")
    } else {
        format!("{tunnel_url}/share/{token}")
    }
}

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
    pub share_url: String,
    pub share_url_local: String,
    pub qr_svg: String,
}

impl ShareView {
    pub fn from_share(share: &Share, tunnel_url: &str) -> Self {
        let (mode, mode_badge) = match &share.mode {
            crate::share::ShareMode::Download => ("Download", "badge-download"),
            crate::share::ShareMode::Upload => ("Upload", "badge-upload"),
            crate::share::ShareMode::Both => ("Both", "badge-both"),
        };
        let expires = share
            .expires
            .map(|e| e.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "Never".into());
        let max_upload_size = format_bytes(share.max_upload_size);
        let share_url = full_share_url(tunnel_url, &share.token);
        let share_url_local = format!("/share/{}", share.token);
        let qr_svg = qr_svg(&share_url);
        Self {
            id: share.id.clone(),
            token: share.token.clone(),
            mode: mode.into(),
            mode_badge: mode_badge.into(),
            path: share.path.display().to_string(),
            expires,
            allow_archive: share.allow_archive,
            max_upload_size,
            share_url,
            share_url_local,
            qr_svg,
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

#[derive(Clone)]
pub struct Breadcrumb {
    pub name: String,
    pub path: String,
}

#[derive(Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub size_human: String,
    pub mime_type: String,
    pub date_modified: String,
}

#[derive(Template)]
#[template(path = "share.html")]
pub struct ShareTemplate {
    pub share_id: String,
    pub token: String,
    pub mode_label: String,
    pub mode_badge: String,
    pub is_expired: bool,
    pub allows_download: bool,
    pub allows_upload: bool,
    pub allow_archive: bool,
    pub max_upload_size: String,
    pub breadcrumbs: Vec<Breadcrumb>,
    pub files: Vec<FileEntry>,
    pub file_count: usize,
    pub file_count_suffix: String,
}
