use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use tracing::{debug, warn};

/// Authentication middleware state
#[derive(Clone)]
pub struct AuthState {
    token: Arc<String>,
}

impl AuthState {
    pub fn new(token: String) -> Self {
        Self {
            token: Arc::new(token),
        }
    }

    pub fn verify_token(&self, provided: &str) -> bool {
        // Simple string comparison
        // For production, consider using subtle::ConstantTimeEq for constant-time comparison
        self.token.as_ref() == provided
    }
}

/// HTTP authentication middleware
pub async fn auth_middleware(
    state: axum::extract::State<AuthState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract Authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    match auth_header {
        Some(header) => {
            // Check for Bearer token
            if let Some(token) = header.strip_prefix("Bearer ") {
                if state.verify_token(token) {
                    debug!("Authentication successful");
                    Ok(next.run(request).await)
                } else {
                    warn!("Invalid token provided");
                    Err(StatusCode::UNAUTHORIZED)
                }
            } else {
                warn!("Invalid authorization format");
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        None => {
            warn!("No authorization header");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

/// Public endpoint middleware (no auth required)
pub async fn public_middleware(request: Request, next: Next) -> Response {
    next.run(request).await
}

/// Generate a secure random token
pub fn generate_token() -> String {
    use rand::Rng;
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

    let mut bytes = [0u8; 32];
    rand::thread_rng().fill(&mut bytes);
    format!("arec_{}", URL_SAFE_NO_PAD.encode(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_generation() {
        let token1 = generate_token();
        let token2 = generate_token();
        
        assert_ne!(token1, token2);
        assert!(token1.starts_with("arec_"));
        assert!(token1.len() > 40);
    }

    #[test]
    fn test_token_verification() {
        let token = "test_token_123".to_string();
        let state = AuthState::new(token.clone());
        
        assert!(state.verify_token(&token));
        assert!(!state.verify_token("wrong_token"));
    }
}
