pub mod routes;

use axum::Router;
use std::path::PathBuf;
use tower_http::services::ServeDir;

pub fn router(serve_dir: PathBuf) -> Router {
    let service = ServeDir::new(&serve_dir)
        .append_index_html_on_directories(true);

    Router::new()
        .nest_service("/", service)
}
