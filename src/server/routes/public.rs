use std::collections::HashMap;
use std::io::{SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Multipart, Path as AxumPath, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::Json;
use askama::Template as _;
use serde::Serialize;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::error::AppError;
use crate::server::templates::{Breadcrumb, FileEntry, ShareTemplate};
use crate::share::{Share, ShareStore};

pub async fn health() -> StatusCode {
    StatusCode::OK
}

#[derive(Clone)]
pub struct AppState {
    pub share_store: Arc<ShareStore>,
    pub tunnel_url: String,
    pub serve_dir: String,
}

#[derive(Serialize)]
struct FileInfo {
    name: String,
    path: String,
    size: u64,
    is_dir: bool,
    mime_type: String,
    date_modified: String,
}

fn safe_resolve(base: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let cleaned = relative.replace('\\', "/");
    let parts: Vec<&str> = cleaned.split('/').filter(|s| !s.is_empty() && *s != ".").collect();
    let mut result = base.to_path_buf();
    for part in parts {
        if part == ".." {
            return Err(AppError::BadRequest("path traversal not allowed".into()));
        }
        result.push(part);
    }
    if !result.starts_with(base) {
        return Err(AppError::BadRequest("path traversal not allowed".into()));
    }
    Ok(result)
}

fn file_info(path: &Path, base: &Path) -> FileInfo {
    let name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let relative = path.strip_prefix(base).unwrap_or(path);
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    let date_modified = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            Some(dt.format("%Y-%m-%d %H:%M").to_string())
        })
        .unwrap_or_else(|| String::from("—"));
    FileInfo {
        name,
        path: relative.to_string_lossy().to_string(),
        size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        is_dir: path.is_dir(),
        mime_type: mime,
        date_modified,
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

fn expired_response() -> Response {
    let html = error_page_html(410, "Share Expired", "This share has expired and is no longer accessible.");
    (StatusCode::GONE, [("content-type", "text/html; charset=utf-8")], html).into_response()
}

fn error_page_html(status: u16, title: &str, message: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} — Burrow</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Pangolin&family=Fira+Code:wght@400;500&display=swap" rel="stylesheet">
<style>
  :root {{ --bg: #f8f5f0; --surface: #fff; --border: #e8e2d9; --text: #2c2416; --text-muted: #a89d8e; --accent: #c2752e; --danger: #b44a3f; --font-display: 'Pangolin', system-ui, sans-serif; --font-body: 'Pangolin', system-ui, sans-serif; --radius: 6px; }}
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ font-family: var(--font-body); background: var(--bg); color: var(--text); display: flex; align-items: center; justify-content: center; min-height: 100vh; padding: 2rem; }}
  .card {{ background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 3rem 2.5rem; max-width: 440px; text-align: center; }}
  .code {{ font-family: var(--font-body); font-size: 3rem; font-weight: 600; color: var(--danger); margin-bottom: 0.5rem; }}
  .title {{ font-family: var(--font-display); font-size: 1.5rem; margin-bottom: 0.75rem; }}
  .msg {{ color: var(--text-muted); font-size: 0.9rem; line-height: 1.6; margin-bottom: 1.5rem; }}
  .link {{ display: inline-block; padding: 0.5rem 1.2rem; border-radius: var(--radius); background: var(--accent); color: #fff; text-decoration: none; font-weight: 500; font-size: 0.9rem; }}
  .link:hover {{ background: #a8631f; }}
</style>
</head>
<body>
  <div class="card">
    <div class="code">{status}</div>
    <div class="title">{title}</div>
    <div class="msg">{message}</div>
    <a href="/" class="link">Back to Burrow</a>
  </div>
</body>
</html>"#
    )
}

fn build_breadcrumbs(_token: &str, current_path: &str) -> Vec<Breadcrumb> {
    if current_path == "." || current_path.is_empty() {
        return Vec::new();
    }
    let parts: Vec<&str> = current_path.split('/').filter(|s| !s.is_empty()).collect();
    let mut breadcrumbs = Vec::new();
    let mut accumulated = String::new();
    for part in parts {
        if !accumulated.is_empty() {
            accumulated.push('/');
        }
        accumulated.push_str(part);
        breadcrumbs.push(Breadcrumb {
            name: part.to_string(),
            path: accumulated.clone(),
        });
    }
    breadcrumbs
}

fn build_file_entries(files: &[FileInfo]) -> Vec<FileEntry> {
    files.iter().map(|f| FileEntry {
        name: f.name.clone(),
        path: f.path.clone(),
        is_dir: f.is_dir,
        size: f.size,
        size_human: format_bytes(f.size),
        mime_type: f.mime_type.clone(),
        date_modified: f.date_modified.clone(),
    }).collect()
}

async fn render_share_page(
    share: &Share,
    current_path: &str,
    files: Vec<FileInfo>,
) -> Result<Html<String>, AppError> {
    let (mode_label, mode_badge) = match &share.mode {
        crate::share::ShareMode::Download => ("Download", "badge-green"),
        crate::share::ShareMode::Upload => ("Upload", "badge-yellow"),
        crate::share::ShareMode::Both => ("Both", "badge-gray"),
    };

    let breadcrumbs = build_breadcrumbs(&share.token, current_path);
    let file_count = files.len();
    let file_entries = build_file_entries(&files);

    let tpl = ShareTemplate {
        share_id: share.id.clone(),
        token: share.token.clone(),
        mode_label: mode_label.to_string(),
        mode_badge: mode_badge.to_string(),
        is_expired: share.is_expired(),
        allows_download: share.allows_download(),
        allows_upload: share.allows_upload(),
        allow_archive: share.allow_archive,
        max_upload_size: format_bytes(share.max_upload_size),
        breadcrumbs,
        files: file_entries,
        file_count,
        file_count_suffix: if file_count == 1 { String::new() } else { "s".to_string() },
    };

    let html = tpl.render().map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Html(html))
}

pub async fn share_entry(
    State(state): State<AppState>,
    AxumPath(token): AxumPath<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let share = state.share_store.get(&token).await
        .ok_or_else(|| AppError::NotFound("share not found".into()))?;

    if share.is_expired() {
        return Ok(expired_response());
    }

    if let Some(format) = params.get("archive") {
        if share.allow_archive && (format == "zip" || format == "tar.gz") {
            if share.allows_download() {
                return archive_download(&share, format).await;
            }
        }
    }

    if !share.allows_download() && !share.allows_upload() {
        return Err(AppError::BadRequest("share not accessible".into()));
    }

    let files = list_dir(&share.path, &share.path).await?;
    let html = render_share_page(&share, ".", files).await?;
    Ok(html.into_response())
}

pub async fn share_file(
    State(state): State<AppState>,
    AxumPath((token, file_path)): AxumPath<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let share = state.share_store.get(&token).await
        .ok_or_else(|| AppError::NotFound("share not found".into()))?;

    if share.is_expired() {
        return Ok(expired_response());
    }

    if !share.allows_download() && !share.allows_upload() {
        return Err(AppError::BadRequest("share not accessible".into()));
    }

    let target = safe_resolve(&share.path, &file_path)?;

    if !target.exists() {
        return Err(AppError::NotFound("file not found".into()));
    }

    if target.is_dir() {
        if !share.allows_download() {
            return Err(AppError::BadRequest("download not allowed".into()));
        }
        let files = list_dir(&target, &share.path).await?;
        let html = render_share_page(&share, &file_path, files).await?;
        return Ok(html.into_response());
    }

    if !share.allows_download() {
        return Err(AppError::BadRequest("download not allowed".into()));
    }

    let mime = mime_guess::from_path(&target)
        .first_or_octet_stream()
        .to_string();

    let file_size = fs::metadata(&target).await?.len();

    if let Some(range) = headers.get(header::RANGE) {
        let range_str = range.to_str().unwrap_or("");
        if let Some(range) = parse_range(range_str, file_size) {
            return serve_range(&target, range, &mime, file_size).await;
        }
    }

    let file = fs::File::open(&target).await?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut response = Response::new(body);
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(&mime).unwrap());
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from_str(&file_size.to_string()).unwrap());
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));

    Ok(response)
}

pub async fn share_upload(
    State(state): State<AppState>,
    AxumPath(token): AxumPath<String>,
    Query(params): Query<HashMap<String, String>>,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let share = state.share_store.get(&token).await
        .ok_or_else(|| AppError::NotFound("share not found".into()))?;

    if share.is_expired() {
        return Ok(expired_response());
    }

    if !share.allows_upload() {
        return Err(AppError::BadRequest("upload not allowed".into()));
    }

    let overwrite = params.get("overwrite").map(|v| v == "true").unwrap_or(false);

    let mut fields: Vec<(String, Vec<u8>)> = Vec::new();
    while let Some(field) = multipart.next_field().await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        let name = field.file_name()
            .unwrap_or("unknown")
            .to_string();
        let target = share.path.join(&name);
        if !target.starts_with(&share.path) {
            return Err(AppError::BadRequest("path traversal not allowed".into()));
        }
        let data = field.bytes().await
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
        if data.len() as u64 > share.max_upload_size {
            return Err(AppError::BadRequest(format!(
                "file too large: {} bytes (max: {} bytes)",
                data.len(),
                share.max_upload_size
            )));
        }
        fields.push((name, data.to_vec()));
    }

    if !overwrite {
        let conflicts: Vec<&str> = fields.iter()
            .filter(|(name, _)| share.path.join(name).exists())
            .map(|(name, _)| name.as_str())
            .collect();
        if !conflicts.is_empty() {
            return Ok((
                StatusCode::CONFLICT,
                [("content-type", "application/json")],
                serde_json::json!({
                    "error": "files_exist",
                    "conflicts": conflicts,
                }).to_string(),
            ).into_response());
        }
    }

    let mut uploaded = Vec::new();
    for (name, data) in &fields {
        let target = share.path.join(name);
        fs::write(&target, data).await?;
        uploaded.push(name.clone());
    }

    Ok(Json(serde_json::json!({
        "uploaded": uploaded
    })).into_response())
}

async fn list_dir(dir: &Path, base: &Path) -> Result<Vec<FileInfo>, AppError> {
    let mut entries = fs::read_dir(dir).await?;
    let mut files = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        files.push(file_info(&path, base));
    }
    files.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(files)
}

async fn archive_download(
    share: &Share,
    format: &str,
) -> Result<Response, AppError> {
    let share = share.clone();
    let format_owned = format.to_string();
    let format_clone = format_owned.clone();

    let stream = async_stream::stream! {
        let mut buffer = Vec::new();

        if format_owned == "zip" {
            let cursor = std::io::Cursor::new(&mut buffer);
            let mut zip = zip::ZipWriter::new(cursor);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("archive/", options).ok();

            let _ = add_dir_to_zip(&mut zip, &share.path, &share.path, &share).await;
            drop(zip);
        } else {
            yield Ok::<_, std::io::Error>(Vec::new());
            return;
        }

        yield Ok(buffer);
    };

    let content_type = if format_clone == "zip" {
        "application/zip"
    } else {
        "application/gzip"
    };

    let filename = format!("archive.{}", format_clone.replace('.', "_"));
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).unwrap(),
    );

    let body = Body::from_stream(stream);

    let mut response = Response::new(body);
    *response.headers_mut() = headers;
    Ok(response)
}

async fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::io::Cursor<&mut Vec<u8>>>,
    dir: &Path,
    base: &Path,
    share: &Share,
) -> Result<(), AppError> {
    let mut entries = fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let relative = path.strip_prefix(base).unwrap_or(&path);
        let name = relative.to_string_lossy().to_string();

        if path.is_dir() {
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file(format!("{name}/"), options).ok();
            Box::pin(add_dir_to_zip(zip, &path, base, share)).await?;
        } else {
            let data = fs::read(&path).await?;
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file(&name, options).ok();
            zip.write_all(&data).ok();
        }
    }
    Ok(())
}

struct ByteRange {
    start: u64,
    end: u64,
}

fn parse_range(range_str: &str, file_size: u64) -> Option<ByteRange> {
    let range_str = range_str.strip_prefix("bytes=")?;
    let (start_str, end_str) = range_str.split_once('-')?;
    let start: u64 = start_str.parse().ok()?;
    let end: u64 = if end_str.is_empty() {
        file_size - 1
    } else {
        end_str.parse().ok()?
    };
    if start <= end && end < file_size {
        Some(ByteRange { start, end })
    } else {
        None
    }
}

async fn serve_range(
    path: &Path,
    range: ByteRange,
    mime: &str,
    file_size: u64,
) -> Result<Response, AppError> {
    let mut file = fs::File::open(path).await?;
    file.seek(SeekFrom::Start(range.start)).await?;

    let len = range.end - range.start + 1;
    let stream = async_stream::stream! {
        let mut remaining = len;
        let mut buf = vec![0u8; 8192];
        loop {
            if remaining == 0 {
                break;
            }
            let to_read = std::cmp::min(remaining, buf.len() as u64) as usize;
            match file.read(&mut buf[..to_read]).await {
                Ok(0) => break,
                Ok(n) => {
                    remaining -= n as u64;
                    yield Ok::<_, std::io::Error>(buf[..n].to_vec());
                }
                Err(e) => yield Err(e),
            }
        }
    };

    let body = Body::from_stream(stream);
    let content_range = format!("bytes {}-{}/{}", range.start, range.end, file_size);

    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::PARTIAL_CONTENT;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(mime).unwrap());
    headers.insert(header::CONTENT_RANGE, HeaderValue::from_str(&content_range).unwrap());
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from_str(&len.to_string()).unwrap());
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));

    Ok(response)
}
