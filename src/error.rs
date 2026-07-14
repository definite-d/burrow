use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Forbidden(String),
    Internal(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound(msg) => write!(f, "Not Found: {msg}"),
            AppError::BadRequest(msg) => write!(f, "Bad Request: {msg}"),
            AppError::Forbidden(msg) => write!(f, "Forbidden: {msg}"),
            AppError::Internal(msg) => write!(f, "Internal Error: {msg}"),
        }
    }
}

impl std::error::Error for AppError {}

fn error_page(status: StatusCode, title: &str, message: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} — Burrow</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Instrument+Serif&family=Outfit:wght@400;500;600&family=Fira+Code:wght@400;500&display=swap" rel="stylesheet">
<style>
  :root {{
    --bg: #f8f5f0; --surface: #fff; --border: #e8e2d9;
    --text: #2c2416; --text-muted: #a89d8e; --accent: #c2752e;
    --danger: #b44a3f; --danger-bg: #fdf0ee; --danger-border: #e8b5b0;
    --font-display: 'Instrument Serif', Georgia, serif;
    --font-body: 'Outfit', system-ui, sans-serif;
    --font-mono: 'Fira Code', monospace;
    --radius: 6px;
  }}
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{
    font-family: var(--font-body); background: var(--bg); color: var(--text);
    display: flex; align-items: center; justify-content: center;
    min-height: 100vh; padding: 2rem;
  }}
  .error-card {{
    background: var(--surface); border: 1px solid var(--border);
    border-radius: var(--radius); padding: 3rem 2.5rem; max-width: 440px;
    text-align: center;
  }}
  .error-code {{
    font-family: var(--font-mono); font-size: 3rem; font-weight: 500;
    color: var(--danger); margin-bottom: 0.5rem; letter-spacing: -0.02em;
  }}
  .error-title {{
    font-family: var(--font-display); font-size: 1.5rem;
    margin-bottom: 0.75rem;
  }}
  .error-msg {{
    color: var(--text-muted); font-size: 0.9rem; line-height: 1.6;
    margin-bottom: 1.5rem;
  }}
  .error-link {{
    display: inline-block; padding: 0.5rem 1.2rem; border-radius: var(--radius);
    background: var(--accent); color: #fff; text-decoration: none;
    font-weight: 500; font-size: 0.9rem; transition: background 0.15s;
  }}
  .error-link:hover {{ background: #a8631f; }}
</style>
</head>
<body>
  <div class="error-card">
    <div class="error-code">{status}</div>
    <div class="error-title">{title}</div>
    <div class="error-msg">{message}</div>
    <a href="/" class="error-link">Back to Burrow</a>
  </div>
</body>
</html>"#
    )
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, title, message) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "Not Found", msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "Bad Request", msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, "Forbidden", msg.clone()),
            AppError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Server Error",
                msg.clone(),
            ),
        };
        let html = error_page(status, title, &message);
        (status, [("content-type", "text/html; charset=utf-8")], html).into_response()
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}
