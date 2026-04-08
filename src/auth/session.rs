use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use axum::extract::ConnectInfo;
use axum::response::Response;
use axum::http::StatusCode;
use tokio::sync::RwLock;
use tracing::{info, warn};
use anyhow::Result;

/// Session data for authenticated users
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserSession {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_accessed: chrono::DateTime<chrono::Utc>,
}

impl UserSession {
    pub fn new() -> Self {
        let now = chrono::Utc::now();
        Self {
            id: format!("sess_{}", uuid::Uuid::new_v4()),
            created_at: now,
            last_accessed: now,
        }
    }
}

/// Rate limiting entry
#[derive(Debug)]
struct RateLimitEntry {
    attempts: u32,
    window_start: Instant,
    locked_until: Option<Instant>,
}

impl RateLimitEntry {
    fn new() -> Self {
        Self {
            attempts: 0,
            window_start: Instant::now(),
            locked_until: None,
        }
    }
}

/// Session manager with rate limiting
pub struct SessionAuth {
    password: String,
    sessions: Arc<RwLock<HashMap<String, UserSession>>>,
    rate_limits: Arc<RwLock<HashMap<String, RateLimitEntry>>>,
    max_attempts: u32,
    window_secs: u64,
    lockout_secs: u64,
    session_ttl_mins: u64,
}

impl SessionAuth {
    pub fn new(password: String) -> Self {
        Self {
            password,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            rate_limits: Arc::new(RwLock::new(HashMap::new())),
            max_attempts: 5,
            window_secs: 300, // 5 minutes
            lockout_secs: 900, // 15 minutes
            session_ttl_mins: 60, // 1 hour
        }
    }

    /// Attempt login with rate limiting
    pub async fn login(&self, password: &str, client_ip: &str) -> Result<Option<String>, String> {
        // Check rate limit
        if self.is_rate_limited(client_ip).await {
            return Err("Too many failed attempts. Please try again later.".to_string());
        }

        // Verify password
        if password != self.password {
            self.record_failed_attempt(client_ip).await;
            return Ok(None);
        }

        // Clear rate limit on success
        self.clear_rate_limit(client_ip).await;

        // Create session
        let session = UserSession::new();
        let session_id = session.id.clone();
        
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), session);
        }

        info!("New session created for client {}", client_ip);
        Ok(Some(session_id))
    }

    /// Logout and invalidate session
    pub async fn logout(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if sessions.remove(session_id).is_some() {
            info!("Session {} invalidated", session_id);
        }
    }

    /// Validate session
    pub async fn validate_session(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        
        if let Some(session) = sessions.get_mut(session_id) {
            let now = chrono::Utc::now();
            let last_access = session.last_accessed;
            
            // Check if session expired
            if now.signed_duration_since(last_access).num_minutes() > self.session_ttl_mins as i64 {
                sessions.remove(session_id);
                info!("Session {} expired", session_id);
                return false;
            }
            
            // Update last accessed
            session.last_accessed = now;
            return true;
        }
        
        false
    }

    /// Check if IP is rate limited
    async fn is_rate_limited(&self, client_ip: &str) -> bool {
        let limits = self.rate_limits.read().await;
        
        if let Some(entry) = limits.get(client_ip) {
            // Check if locked out
            if let Some(locked_until) = entry.locked_until {
                if Instant::now() < locked_until {
                    return true;
                }
            }
        }
        
        false
    }

    /// Record a failed login attempt
    async fn record_failed_attempt(&self, client_ip: &str) {
        let mut limits = self.rate_limits.write().await;
        let now = Instant::now();
        
        let entry = limits.entry(client_ip.to_string()).or_insert_with(RateLimitEntry::new);
        
        // Reset if window expired
        if now.duration_since(entry.window_start).as_secs() > self.window_secs {
            entry.attempts = 0;
            entry.window_start = now;
            entry.locked_until = None;
        }
        
        entry.attempts += 1;
        
        // Lock out if max attempts reached
        if entry.attempts >= self.max_attempts {
            entry.locked_until = Some(now + Duration::from_secs(self.lockout_secs));
            warn!(
                "Rate limit triggered for IP {} after {} attempts",
                client_ip,
                entry.attempts
            );
        }
    }

    /// Clear rate limit on successful login
    async fn clear_rate_limit(&self, client_ip: &str) {
        let mut limits = self.rate_limits.write().await;
        limits.remove(client_ip);
    }

    /// Cleanup expired sessions and rate limits (call periodically)
    pub async fn cleanup(&self) {
        let now = chrono::Utc::now();
        
        // Cleanup sessions
        {
            let mut sessions = self.sessions.write().await;
            let expired: Vec<String> = sessions
                .iter()
                .filter(|(_, session)| {
                    now.signed_duration_since(session.last_accessed).num_minutes() > self.session_ttl_mins as i64
                })
                .map(|(id, _)| id.clone())
                .collect();
            
            for id in expired {
                sessions.remove(&id);
            }
        }
        
        // Cleanup rate limits
        {
            let mut limits = self.rate_limits.write().await;
            let now_inst = Instant::now();
            let expired: Vec<String> = limits
                .iter()
                .filter(|(_, entry)| {
                    if let Some(locked_until) = entry.locked_until {
                        now_inst > locked_until + Duration::from_secs(self.window_secs)
                    } else {
                        now_inst.duration_since(entry.window_start).as_secs() > self.window_secs * 2
                    }
                })
                .map(|(ip, _)| ip.clone())
                .collect();
            
            for ip in expired {
                limits.remove(&ip);
            }
        }
    }
}

use axum::{extract::Request, middleware::Next};

/// Middleware to check session authentication
pub async fn session_auth_middleware(
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip auth for certain paths
    let path = request.uri().path();
    if path == "/" || path.starts_with("/static/") || path == "/api/login" || path == "/health" {
        return Ok(next.run(request).await);
    }

    // Check session header
    let session_id = request
        .headers()
        .get("X-Session-ID")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    if session_id.is_none() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Session validation happens in the handler using SessionAuth
    Ok(next.run(request).await)
}
