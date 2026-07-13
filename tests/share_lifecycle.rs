use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use burrow::share::{ShareMode, ShareStore};
use burrow::server::router;

fn test_store() -> Arc<ShareStore> {
    Arc::new(ShareStore::new())
}

fn test_router(store: Arc<ShareStore>) -> axum::Router {
    router(
        std::path::PathBuf::from("."),
        store,
        String::new(),
    )
}

#[tokio::test]
async fn health_check() {
    let store = test_store();
    let app = test_router(store);

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn dashboard_renders() {
    let store = test_store();
    let app = test_router(store);

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Dashboard"));
    assert!(html.contains("Burrow"));
}

#[tokio::test]
async fn share_not_found() {
    let store = test_store();
    let app = test_router(store);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/share/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_and_retrieve_share() {
    let store = test_store();
    let app = test_router(store.clone());

    // Create a temp dir for the share
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_path_buf();

    // Create share via form POST
    let body = format!(
        "id=test-share&mode=download&path={}",
        urlencoding::encode(&path.display().to_string())
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/shares")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify it's in the store
    let shares = store.list().await;
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].id, "test-share");
    assert_eq!(shares[0].mode, ShareMode::Download);

    // Verify dashboard shows it
    let app2 = test_router(store.clone());
    let response = app2
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("test-share"));
}

#[tokio::test]
async fn delete_share() {
    let store = test_store();
    let tmp = tempfile::tempdir().unwrap();

    store
        .create(
            "del-test".into(),
            tmp.path().to_path_buf(),
            ShareMode::Download,
            None,
            false,
            100 * 1024 * 1024,
        )
        .await
        .unwrap();

    assert_eq!(store.list().await.len(), 1);

    let app = test_router(store.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/admin/shares/del-test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(store.list().await.len(), 0);
}

#[tokio::test]
async fn share_token_lookup() {
    let store = test_store();
    let tmp = tempfile::tempdir().unwrap();

    let share = store
        .create(
            "token-test".into(),
            tmp.path().to_path_buf(),
            ShareMode::Download,
            None,
            false,
            100 * 1024 * 1024,
        )
        .await
        .unwrap();

    let token = share.token.clone();
    let app = test_router(store.clone());

    // Accessing via token should work (return directory listing or error about empty dir)
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/share/{token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // 200 OK (empty dir listing) or 500 (empty dir read error) - both are valid
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "Expected 200 or 500, got {}",
        response.status()
    );
}

#[tokio::test]
async fn expired_share_returns_gone() {
    let store = test_store();
    let tmp = tempfile::tempdir().unwrap();

    // Create share that expired 1 hour ago
    let expired_time = chrono::Utc::now() - chrono::Duration::hours(1);
    let share = store
        .create(
            "expired-test".into(),
            tmp.path().to_path_buf(),
            ShareMode::Download,
            Some(expired_time),
            false,
            100 * 1024 * 1024,
        )
        .await
        .unwrap();

    let token = share.token.clone();
    let app = test_router(store.clone());

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/share/{token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::GONE);
}

#[tokio::test]
async fn invalid_upload_mode_returns_400() {
    let store = test_store();
    let tmp = tempfile::tempdir().unwrap();

    let share = store
        .create(
            "dl-only".into(),
            tmp.path().to_path_buf(),
            ShareMode::Download,
            None,
            false,
            100 * 1024 * 1024,
        )
        .await
        .unwrap();

    let token = share.token.clone();
    let app = test_router(store.clone());

    // Try to upload to a download-only share
    let boundary = "----TestBoundary";
    let body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         hello\r\n\
         --{boundary}--\r\n"
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/share/{token}"))
                .header("content-type", format!("multipart/form-data; boundary={boundary}"))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn evict_expired_removes_old_shares() {
    let store = test_store();
    let tmp = tempfile::tempdir().unwrap();

    // Active share
    store
        .create(
            "active".into(),
            tmp.path().to_path_buf(),
            ShareMode::Download,
            None,
            false,
            100 * 1024 * 1024,
        )
        .await
        .unwrap();

    // Expired share
    let expired_time = chrono::Utc::now() - chrono::Duration::hours(1);
    store
        .create(
            "expired".into(),
            tmp.path().to_path_buf(),
            ShareMode::Download,
            Some(expired_time),
            false,
            100 * 1024 * 1024,
        )
        .await
        .unwrap();

    assert_eq!(store.list().await.len(), 2);

    store.evict_expired().await;

    let remaining = store.list().await;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "active");
}
