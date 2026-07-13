pub mod routes;

use axum::Router;
use std::path::PathBuf;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

pub fn router(serve_dir: PathBuf) -> Router {
    let service = ServeDir::new(&serve_dir)
        .append_index_html_on_directories(true);

    Router::new()
        .route("/health", axum::routing::get(routes::public::health))
        .nest_service("/", service)
        .layer(TraceLayer::new_for_http())
}
