use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;

use crate::error::AppError;
use crate::share::ShareMode;
use crate::server::templates::{DashboardTemplate, ShareEditTemplate, ShareFormTemplate, ShareView};
use crate::server::routes::public::AppState;

pub async fn dashboard(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let shares = state.share_store.list().await;
    let share_views: Vec<ShareView> = shares.iter()
        .map(|s| ShareView::from_share(s, &state.tunnel_url))
        .collect();

    let tpl = DashboardTemplate {
        version: env!("CARGO_PKG_VERSION").into(),
        tunnel_url: state.tunnel_url.clone(),
        has_tunnel: !state.tunnel_url.is_empty(),
        share_count: share_views.len(),
        serve_dir: state.serve_dir.clone(),
        shares: share_views,
    };

    let html = tpl.render().map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Html(html))
}

pub async fn shares_list(
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let shares = state.share_store.list().await;
    let share_views: Vec<ShareView> = shares.iter()
        .map(|s| ShareView::from_share(s, &state.tunnel_url))
        .collect();

    let tpl = DashboardTemplate {
        version: env!("CARGO_PKG_VERSION").into(),
        tunnel_url: state.tunnel_url.clone(),
        has_tunnel: !state.tunnel_url.is_empty(),
        share_count: share_views.len(),
        serve_dir: state.serve_dir.clone(),
        shares: share_views,
    };

    let html = tpl.render().map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Html(html).into_response())
}

pub async fn share_form() -> Result<Html<String>, AppError> {
    let tpl = ShareFormTemplate;
    let html = tpl.render().map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Html(html))
}

#[derive(Deserialize)]
pub struct CreateShareForm {
    pub id: String,
    pub mode: String,
    pub path: String,
    pub expires: Option<String>,
    pub max_upload_size: Option<String>,
    pub allow_archive: Option<String>,
}

pub async fn share_create(
    State(state): State<AppState>,
    Form(form): Form<CreateShareForm>,
) -> Result<Html<String>, AppError> {
    let mode = match form.mode.as_str() {
        "upload" => ShareMode::Upload,
        "both" => ShareMode::Both,
        _ => ShareMode::Download,
    };

    let path = std::path::PathBuf::from(&form.path);
    let path = if path.is_relative() {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    } else {
        path
    };
    let path = path.canonicalize().map_err(|e| {
        AppError::BadRequest(format!("Could not resolve path {}: {e}", form.path))
    })?;

    let expires = form.expires.as_deref().and_then(parse_duration_display);

    let max_upload_size = form
        .max_upload_size
        .as_deref()
        .and_then(|s| crate::filter::parse_size(s).ok())
        .unwrap_or(100 * 1024 * 1024);

    let allow_archive = form.allow_archive.as_deref() == Some("true");

    match state
        .share_store
        .create(
            form.id,
            path,
            mode,
            expires,
            allow_archive,
            max_upload_size,
        )
        .await
    {
        Ok(_share) => Ok(Html(
            "<p style='color:green'>Share created. <a href='/admin/shares'>View all</a></p>".into(),
        )),
        Err(e) => Ok(Html(format!("<p style='color:red'>Error: {e}</p>"))),
    }
}

pub async fn share_edit(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Html<String>, AppError> {
    let share = state
        .share_store
        .get_by_id(&id)
        .await
        .ok_or_else(|| AppError::NotFound(format!("share '{id}' not found")))?;

    let view = ShareView::from_share(&share, &state.tunnel_url);
    let tpl = ShareEditTemplate { share: view };
    let html = tpl.render().map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Html(html))
}

#[derive(Deserialize)]
pub struct UpdateShareForm {
    pub mode: Option<String>,
    pub allow_archive: Option<String>,
}

pub async fn share_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(form): Form<UpdateShareForm>,
) -> Result<Html<String>, AppError> {
    let mut share = state
        .share_store
        .get_by_id(&id)
        .await
        .ok_or_else(|| AppError::NotFound(format!("share '{id}' not found")))?;

    if let Some(mode_str) = &form.mode {
        share.mode = match mode_str.as_str() {
            "upload" => ShareMode::Upload,
            "both" => ShareMode::Both,
            _ => ShareMode::Download,
        };
    }
    share.allow_archive = form.allow_archive.as_deref() == Some("true");

    state.share_store.replace(&id, share).await;

    Ok(Html(
        "<p style='color:green'>Share updated. <a href='/admin/shares'>View all</a></p>".into(),
    ))
}

pub async fn share_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let deleted = state.share_store.delete(&id).await;
    if deleted {
        Ok(StatusCode::OK)
    } else {
        Err(AppError::NotFound(format!("share '{id}' not found")))
    }
}

fn parse_duration_display(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let s = s.trim().to_lowercase();
    let seconds: i64 = if let Some(v) = s.strip_suffix('s') {
        v.trim().parse().ok()?
    } else if let Some(v) = s.strip_suffix('m') {
        v.trim().parse::<i64>().ok()? * 60
    } else if let Some(v) = s.strip_suffix('h') {
        v.trim().parse::<i64>().ok()? * 3600
    } else if let Some(v) = s.strip_suffix('d') {
        v.trim().parse::<i64>().ok()? * 86400
    } else {
        return None;
    };
    Some(chrono::Utc::now() + chrono::Duration::seconds(seconds))
}
