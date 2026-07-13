pub mod routes;
pub mod templates;

use axum::Router;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::share::ShareStore;
use routes::public::AppState;

pub fn router(
    serve_dir: PathBuf,
    share_store: Arc<ShareStore>,
    tunnel_url: String,
) -> Router {
    let service = ServeDir::new(&serve_dir)
        .append_index_html_on_directories(true);

    let state = AppState {
        share_store,
        tunnel_url,
        serve_dir: serve_dir.display().to_string(),
    };

    Router::new()
        .route("/", axum::routing::get(routes::admin::dashboard))
        .route("/admin/shares", axum::routing::get(routes::admin::shares_list))
        .route("/admin/shares/new", axum::routing::get(routes::admin::share_form))
        .route("/admin/shares", axum::routing::post(routes::admin::share_create))
        .route("/admin/shares/{id}", axum::routing::get(routes::admin::share_edit))
        .route("/admin/shares/{id}", axum::routing::put(routes::admin::share_update))
        .route("/admin/shares/{id}", axum::routing::delete(routes::admin::share_delete))
        .route("/health", axum::routing::get(routes::public::health))
        .route("/share/{token}", axum::routing::get(routes::public::share_entry))
        .route("/share/{token}/{*path}", axum::routing::get(routes::public::share_file))
        .route("/share/{token}", axum::routing::post(routes::public::share_upload))
        .fallback_service(service)
        .with_state(state)
        .layer(RequestBodyLimitLayer::new(250 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
}
