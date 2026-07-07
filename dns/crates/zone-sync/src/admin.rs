use std::net::SocketAddr;
use std::sync::Arc;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use crate::model::SyncStatus;

#[derive(Clone)]
pub struct AdminState {
    trigger: Arc<dyn Fn() + Send + Sync>,
    status: Arc<dyn Fn() -> SyncStatus + Send + Sync>,
    token: Option<String>,
}

impl AdminState {
    pub fn new(
        trigger: Arc<dyn Fn() + Send + Sync>,
        status: Arc<dyn Fn() -> SyncStatus + Send + Sync>,
        token: Option<String>,
    ) -> Self {
        AdminState { trigger, status, token }
    }
}

fn authorized(expected: Option<&str>, provided: Option<&str>) -> bool {
    match (expected, provided) {
        (Some(expected), Some(provided)) => !expected.is_empty() && provided == expected,
        _ => false,
    }
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
}

async fn healthz(State(state): State<AdminState>) -> Json<SyncStatus> {
    Json((state.status)())
}

async fn reload(State(state): State<AdminState>, headers: HeaderMap) -> impl IntoResponse {
    if !authorized(state.token.as_deref(), bearer(&headers)) {
        return (StatusCode::UNAUTHORIZED, "invalid or missing admin token").into_response();
    }
    
    (state.trigger)();
    (StatusCode::ACCEPTED, "reload triggered").into_response()
}

pub fn router(state: AdminState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/admin/reload", post(reload))
        .with_state(state)
}

pub async fn serve(addr: SocketAddr, state: AdminState) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "admin endpoint listening (POST /admin/reload, GET /healthz)");
    
    axum::serve(listener, router(state)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_requires_a_matching_token() {
        assert!(authorized(Some("secret"), Some("secret")));
        assert!(!authorized(Some("secret"), Some("wrong")));
        assert!(!authorized(Some("secret"), None));
    }

    #[test]
    fn reload_is_closed_when_no_token_is_configured() {
        // Fail safe: an unconfigured token must not mean "anyone may reload".
        assert!(!authorized(None, None));
        assert!(!authorized(None, Some("anything")));
        assert!(!authorized(Some(""), Some("")));
    }

    #[test]
    fn parses_bearer_header() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "Bearer abc123".parse().unwrap());
        assert_eq!(bearer(&headers), Some("abc123"));

        let mut basic = HeaderMap::new();
        basic.insert(AUTHORIZATION, "Basic abc123".parse().unwrap());
        assert_eq!(bearer(&basic), None);
    }
}
