pub mod routes;

use axum::Router;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::share::ShareStore;
use routes::public::AppState;

pub fn router(serve_dir: PathBuf, share_store: Arc<ShareStore>) -> Router {
    let service = ServeDir::new(&serve_dir)
        .append_index_html_on_directories(true);

    let state = AppState {
        share_store,
    };

    Router::new()
        .route("/health", axum::routing::get(routes::public::health))
        .route("/share/{token}", axum::routing::get(routes::public::share_entry))
        .route("/share/{token}/{*path}", axum::routing::get(routes::public::share_file))
        .route("/share/{token}", axum::routing::post(routes::public::share_upload))
        .nest_service("/", service)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}
