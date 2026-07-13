use std::collections::HashMap;
use std::io::{SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Multipart, Path as AxumPath, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::error::AppError;
use crate::share::{Share, ShareStore};

pub async fn health() -> StatusCode {
    StatusCode::OK
}

#[derive(Clone)]
pub struct AppState {
    pub share_store: Arc<ShareStore>,
}

#[derive(Serialize)]
struct FileInfo {
    name: String,
    path: String,
    size: u64,
    is_dir: bool,
    mime_type: String,
}

#[derive(Serialize)]
struct DirListing {
    files: Vec<FileInfo>,
    current_path: String,
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
    FileInfo {
        name,
        path: relative.to_string_lossy().to_string(),
        size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        is_dir: path.is_dir(),
        mime_type: mime,
    }
}

fn expired_response() -> Response {
    (StatusCode::GONE, "share has expired").into_response()
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

    if !share.allows_download() {
        return Err(AppError::BadRequest("download not allowed".into()));
    }

    if let Some(format) = params.get("archive") {
        if share.allow_archive && (format == "zip" || format == "tar.gz") {
            return archive_download(&share, format).await;
        }
    }

    let files = list_dir(&share.path, &share.path).await?;
    Ok(Json(DirListing {
        files,
        current_path: ".".to_string(),
    }).into_response())
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

    if !share.allows_download() {
        return Err(AppError::BadRequest("download not allowed".into()));
    }

    let target = safe_resolve(&share.path, &file_path)?;

    if !target.exists() {
        return Err(AppError::NotFound("file not found".into()));
    }

    if target.is_dir() {
        let files = list_dir(&target, &share.path).await?;
        return Ok(Json(DirListing {
            files,
            current_path: file_path,
        }).into_response());
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
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let share = state.share_store.get(&token).await
        .ok_or_else(|| AppError::NotFound("share not found".into()))?;

    if share.is_expired() {
        return Ok(expired_response());
    }

    if !share.allows_upload() {
        return Err(AppError::from(AppError::BadRequest("upload not allowed".into())));
    }

    let mut uploaded = Vec::new();

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

        fs::write(&target, &data).await?;
        uploaded.push(name);
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
