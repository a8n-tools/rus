use serde::{Deserialize, Serialize};

/// Request to shorten a URL
#[derive(Serialize, Deserialize)]
pub struct ShortenRequest {
    pub url: String,
}

/// Response after shortening a URL
#[derive(Serialize, Deserialize)]
pub struct ShortenResponse {
    pub short_code: String,
    pub short_url: String,
    pub original_url: String,
}

/// URL entry stored in database
#[derive(Clone, Serialize)]
pub struct UrlEntry {
    pub original_url: String,
    pub short_code: String,
    pub name: Option<String>,
    pub clicks: u64,
}

/// User registration request
#[derive(Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
}

/// User login request
#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Authentication response with tokens
#[derive(Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub refresh_token: String,
    pub username: String,
}

/// Request to update URL name
#[derive(Serialize, Deserialize)]
pub struct UpdateUrlNameRequest {
    pub name: Option<String>,
}

/// Token refresh request
#[derive(Serialize, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

/// Token refresh response
#[derive(Serialize, Deserialize)]
pub struct RefreshResponse {
    pub token: String,
    pub refresh_token: String,
}

/// Click history entry
#[derive(Serialize, Deserialize)]
pub struct ClickHistoryEntry {
    pub clicked_at: String,
}

/// Click statistics for a URL
#[derive(Serialize, Deserialize)]
pub struct ClickStats {
    pub total_clicks: u64,
    pub history: Vec<ClickHistoryEntry>,
}

/// JWT claims structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,      // username
    pub user_id: i64,     // user ID
    pub is_admin: bool,   // admin flag
    pub exp: usize,       // expiration time
}

/// Health check response
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
}

/// Version response
#[derive(Serialize)]
pub struct VersionResponse {
    pub version: String,
}

/// Public configuration response
#[derive(Serialize)]
pub struct ConfigResponse {
    pub host_url: String,
    pub max_url_length: usize,
    pub allow_registration: bool,
}

/// Setup check response
#[derive(Serialize)]
pub struct SetupCheckResponse {
    pub setup_required: bool,
}

/// User information
#[derive(Serialize)]
pub struct UserInfo {
    pub user_id: i64,
    pub username: String,
    pub is_admin: bool,
    pub created_at: String,
    pub url_count: i64,
}

/// Current user response
#[derive(Serialize)]
pub struct CurrentUserResponse {
    pub user_id: i64,
    pub username: String,
    pub is_admin: bool,
}

/// Admin statistics response
#[derive(Serialize)]
pub struct AdminStatsResponse {
    pub total_users: i64,
    pub total_urls: i64,
    pub total_clicks: i64,
}

/// Abuse report submission request
#[derive(Serialize, Deserialize)]
pub struct SubmitReportRequest {
    pub short_code: String,
    pub reporter_email: Option<String>,
    pub reason: String,
    pub description: Option<String>,
}

/// Abuse report details
#[derive(Serialize)]
pub struct AbuseReport {
    pub id: i64,
    pub short_code: String,
    pub reporter_email: Option<String>,
    pub reason: String,
    pub description: Option<String>,
    pub status: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub resolved_by: Option<i64>,
    // Additional info from joins
    pub original_url: Option<String>,
    pub url_owner_username: Option<String>,
    pub url_owner_id: Option<i64>,
}

/// Request to resolve an abuse report
#[derive(Serialize, Deserialize)]
pub struct ResolveReportRequest {
    pub action: String, // "dismiss", "delete_url", "ban_user"
}
