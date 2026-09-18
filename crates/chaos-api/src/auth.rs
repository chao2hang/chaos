//! Auth routes, password hashing, JWT helpers, and AuthUser extractor.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::extract::State;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chaos_i18n::Locale;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use chaos_store::{count_users, create_first_admin_user, find_user_by_id, find_user_by_username};

use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::state::AppState;

const JWT_TTL_HOURS: i64 = 24;
const MIN_PASSWORD_LEN: usize = 8;
const MAX_PASSWORD_LEN: usize = 256;
const MAX_USERNAME_LEN: usize = 128;
const MIN_JWT_SECRET_BYTES: usize = 32;

// Rate limiting constants
const MAX_LOGIN_ATTEMPTS: u32 = 5;
const LOCKOUT_DURATION_SECS: u64 = 900; // 15 minutes

/// Cap on tracked keys. Both the peer address and the submitted username are
/// attacker-influenced, so the map must not be allowed to grow without bound.
const MAX_TRACKED_KEYS: usize = 10_000;

struct Attempt {
    count: u32,
    last_attempt: Instant,
}

/// Simple in-memory rate limiter for login attempts.
///
/// Entries are keyed by [`rate_limit_key`] — the peer address when the server is
/// run with connect info, otherwise the submitted username. Note that keying by
/// address means callers sharing one address (a NAT, a VPN egress) share a
/// budget; that is the intended trade for not letting one source spray attempts
/// across unlimited usernames.
struct LoginRateLimiter {
    attempts: HashMap<String, Attempt>,
}

impl LoginRateLimiter {
    fn new() -> Self {
        Self {
            attempts: HashMap::new(),
        }
    }

    /// Check whether `key` is currently locked out.
    /// Returns (is_limited, remaining_attempts).
    fn check(&mut self, key: &str) -> (bool, u32) {
        self.sweep();
        let now = Instant::now();
        let Some(attempt) = self.attempts.get_mut(key) else {
            return (false, MAX_LOGIN_ATTEMPTS);
        };
        // Reset once the lockout window has passed.
        if now.duration_since(attempt.last_attempt).as_secs() > LOCKOUT_DURATION_SECS {
            attempt.count = 0;
            attempt.last_attempt = now;
            return (false, MAX_LOGIN_ATTEMPTS);
        }
        if attempt.count >= MAX_LOGIN_ATTEMPTS {
            return (true, 0);
        }
        (false, MAX_LOGIN_ATTEMPTS - attempt.count)
    }

    /// Record a failed login attempt.
    fn record_failure(&mut self, key: &str) {
        let now = Instant::now();
        let attempt = self.attempts.entry(key.to_string()).or_insert(Attempt {
            count: 0,
            last_attempt: now,
        });
        attempt.count += 1;
        attempt.last_attempt = now;
    }

    /// Clear attempts on successful login.
    fn clear(&mut self, key: &str) {
        self.attempts.remove(key);
    }

    /// Expire stale entries, then keep the map under [`MAX_TRACKED_KEYS`].
    ///
    /// Expiry used to be evaluated only for a key that was looked up again, so
    /// failures recorded for arbitrary usernames were never released.
    fn sweep(&mut self) {
        let now = Instant::now();
        self.attempts.retain(|_, attempt| {
            now.duration_since(attempt.last_attempt).as_secs() <= LOCKOUT_DURATION_SECS
        });
        if self.attempts.len() < MAX_TRACKED_KEYS {
            return;
        }
        // Still at the cap after expiring, so something has to go. Evict the
        // least recently active keys, but prefer unlocked ones: dropping a key
        // that is currently locked out would hand that attacker a fresh budget
        // on demand, since they only need to push `MAX_TRACKED_KEYS / 2` newer
        // keys in to clear their own lockout.
        let mut candidates: Vec<(String, Instant, bool)> = self
            .attempts
            .iter()
            .map(|(key, attempt)| {
                (
                    key.clone(),
                    attempt.last_attempt,
                    attempt.count >= MAX_LOGIN_ATTEMPTS,
                )
            })
            .collect();
        candidates.sort_by_key(|(_, last_attempt, locked_out)| (*locked_out, *last_attempt));
        for (key, _, _) in candidates.into_iter().take(MAX_TRACKED_KEYS / 2) {
            self.attempts.remove(&key);
        }
    }
}

static RATE_LIMITER: LazyLock<Mutex<LoginRateLimiter>> =
    LazyLock::new(|| Mutex::new(LoginRateLimiter::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub username: String,
    /// `admin` for the install/setup account; other roles reserved for later.
    #[serde(default = "default_role")]
    pub role: String,
    pub exp: i64,
}

fn default_role() -> String {
    "user".into()
}

/// Authenticated user extracted from `Authorization: Bearer <jwt>`.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub role: String,
}

/// Authenticated caller for automation endpoints. JWT callers retain the
/// existing behaviour; API-key callers are additionally checked against the
/// requested scope by the endpoint.
#[derive(Debug, Clone)]
pub struct ExternalUser {
    pub user: AuthUser,
    pub scopes: HashSet<String>,
    pub api_key: bool,
}

impl ExternalUser {
    pub fn has_scope(&self, scope: &str) -> bool {
        !self.api_key || self.scopes.contains(scope)
    }

    pub fn require_scope(&self, scope: &'static str, locale: Locale) -> Result<(), ApiError> {
        if self.has_scope(scope) {
            Ok(())
        } else {
            Err(ApiError::forbidden("api_key_scope_required", locale))
        }
    }

    pub fn require_admin(&self, locale: Locale) -> Result<(), ApiError> {
        if self.user.is_admin() {
            Ok(())
        } else {
            Err(ApiError::forbidden("admin_required", locale))
        }
    }
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

/// Admin-only extractor. Rejects non-admin users with 403.
#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthUser);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if !user.is_admin() {
            let locale = Locale::from_accept_language(
                parts
                    .headers
                    .get(axum::http::header::ACCEPT_LANGUAGE)
                    .and_then(|v| v.to_str().ok()),
            );
            return Err(ApiError::forbidden("admin_required", locale));
        }
        Ok(AdminUser(user))
    }
}

#[derive(Debug, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub initialized: bool,
}

pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/setup", post(setup))
        .route("/login", post(login))
        .route("/status", get(status))
        .route("/api-keys", get(list_keys).post(create_key))
        .route("/api-keys/{id}", delete(revoke_key))
}

const API_KEY_PREFIX: &str = "chaos_sk_";
const API_KEY_SCOPES: [&str; 6] = [
    "orchestration:read",
    "orchestration:validate",
    "orchestration:simulate",
    "orchestration:plan",
    "orchestration:publish",
    "diagnostics:read",
];

#[derive(Debug, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
    #[serde(default = "default_api_key_scopes")]
    scopes: Vec<String>,
}

fn default_api_key_scopes() -> Vec<String> {
    vec![
        "orchestration:read".into(),
        "orchestration:validate".into(),
        "orchestration:simulate".into(),
        "orchestration:plan".into(),
        "diagnostics:read".into(),
    ]
}

#[derive(Debug, Serialize)]
struct ApiKeyResponse {
    id: String,
    name: String,
    key_prefix: String,
    scopes: Vec<String>,
    created_at: String,
    last_used_at: Option<String>,
    revoked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
}

fn api_key_response(key: chaos_store::ApiKey, secret: Option<String>) -> ApiKeyResponse {
    let scopes = serde_json::from_str(&key.scopes).unwrap_or_default();
    ApiKeyResponse {
        id: key.id,
        name: key.name,
        key_prefix: key.key_prefix,
        scopes,
        created_at: key.created_at,
        last_used_at: key.last_used_at,
        revoked_at: key.revoked_at,
        key: secret,
    }
}

async fn create_key(
    admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<Json<ApiKeyResponse>, ApiError> {
    let name = body.name.trim().to_string();
    if name.is_empty() || name.len() > 128 || body.scopes.is_empty() {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let mut scopes = body.scopes;
    scopes.sort();
    scopes.dedup();
    if scopes
        .iter()
        .any(|scope| !API_KEY_SCOPES.contains(&scope.as_str()))
    {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let secret = format!("{API_KEY_PREFIX}{}", hex::encode(bytes));
    let hash = Sha256::digest(secret.as_bytes());
    let key = chaos_store::create_api_key(
        &state.pool,
        &admin.0.user_id,
        &name,
        &secret[..API_KEY_PREFIX.len() + 8],
        &hex::encode(hash),
        &serde_json::to_string(&scopes).map_err(|e| ApiError::internal_logged(locale, e))?,
    )
    .await?;
    Ok(Json(api_key_response(key, Some(secret))))
}

async fn list_keys(
    admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<ApiKeyResponse>>, ApiError> {
    let keys = chaos_store::list_api_keys(&state.pool, &admin.0.user_id).await?;
    Ok(Json(
        keys.into_iter()
            .map(|key| api_key_response(key, None))
            .collect(),
    ))
}

async fn revoke_key(
    admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    RequestLocale(locale): RequestLocale,
) -> Result<StatusCode, ApiError> {
    let keys = chaos_store::list_api_keys(&state.pool, &admin.0.user_id).await?;
    if !keys.iter().any(|key| key.id == id) {
        return Err(ApiError::not_found("not_found", locale));
    }
    chaos_store::revoke_api_key(&state.pool, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Load JWT secret from `CHAOS_JWT_SECRET` or `./data/jwt.secret`.
///
/// `CHAOS_JWT_SECRET` may be either:
///
/// - a raw secret string (≥ 32 bytes), or
/// - a filesystem path (contains `/` or `\\`, or exists as a file) whose contents are the secret.
///
/// Packaging sets a path under `/var/lib/chaos/jwt.secret`.
pub fn load_or_create_jwt_secret() -> anyhow::Result<String> {
    if let Ok(raw) = std::env::var("CHAOS_JWT_SECRET") {
        let raw = raw.trim().to_string();
        let as_path = Path::new(&raw);
        let looks_like_path = raw.contains('/')
            || raw.contains('\\')
            || as_path.exists()
            || raw.starts_with('.')
            || raw.starts_with('~');
        if looks_like_path {
            return load_or_create_jwt_secret_file(as_path);
        }
        if raw.len() >= MIN_JWT_SECRET_BYTES {
            return Ok(raw);
        }
        anyhow::bail!(
            "CHAOS_JWT_SECRET must be a secret of at least {MIN_JWT_SECRET_BYTES} bytes or a path to a secret file"
        );
    }

    load_or_create_jwt_secret_file(Path::new("./data/jwt.secret"))
}

fn load_or_create_jwt_secret_file(path: &Path) -> anyhow::Result<String> {
    if path.exists() {
        let secret = fs::read_to_string(path)?.trim().to_string();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        if secret.len() < MIN_JWT_SECRET_BYTES {
            anyhow::bail!(
                "JWT secret file must contain at least {MIN_JWT_SECRET_BYTES} bytes: {}",
                path.display()
            );
        }
        return Ok(secret);
    }

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }
    }

    let secret = random_hex_secret(32);
    fs::write(path, &secret)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(secret)
}

fn random_hex_secret(num_bytes: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; num_bytes];
    OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

#[cfg(test)]
pub fn hash_password(password: &str) -> Result<String, ApiError> {
    hash_password_locale(password, Locale::En)
}

pub fn hash_password_locale(password: &str, locale: Locale) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::internal_logged(locale, format!("password hash failed: {e}")))
}

#[cfg(test)]
pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, ApiError> {
    verify_password_locale(password, password_hash, Locale::En)
}

fn verify_password_locale(
    password: &str,
    password_hash: &str,
    locale: Locale,
) -> Result<bool, ApiError> {
    let parsed = PasswordHash::new(password_hash)
        .map_err(|e| ApiError::internal_logged(locale, format!("invalid password hash: {e}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

#[cfg(test)]
pub fn issue_token(user_id: &str, username: &str, secret: &str) -> Result<String, ApiError> {
    issue_token_with_role(user_id, username, "admin", secret, Locale::En)
}

/// Issue a JWT with an explicit role (used by login/setup and tests).
#[cfg(test)]
pub fn issue_token_role(
    user_id: &str,
    username: &str,
    role: &str,
    secret: &str,
) -> Result<String, ApiError> {
    issue_token_with_role(user_id, username, role, secret, Locale::En)
}

fn issue_token_with_role(
    user_id: &str,
    username: &str,
    role: &str,
    secret: &str,
    locale: Locale,
) -> Result<String, ApiError> {
    let exp = (Utc::now() + Duration::hours(JWT_TTL_HOURS)).timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        role: role.to_string(),
        exp,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ApiError::internal_logged(locale, format!("jwt encode failed: {e}")))
}

#[cfg(test)]
pub fn decode_token(token: &str, secret: &str) -> Result<Claims, ApiError> {
    decode_token_locale(token, secret, Locale::En)
}

fn decode_token_locale(token: &str, secret: &str, locale: Locale) -> Result<Claims, ApiError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| ApiError::unauthorized("invalid_token", locale))
}

fn validate_credentials(body: &Credentials, locale: Locale) -> Result<(), ApiError> {
    let username = body.username.trim();
    if username.is_empty() || username.len() > MAX_USERNAME_LEN {
        return Err(ApiError::bad_request("invalid_request", locale));
    }
    if body.password.len() < MIN_PASSWORD_LEN || body.password.len() > MAX_PASSWORD_LEN {
        return Err(ApiError::bad_request("invalid_password", locale));
    }
    Ok(())
}

/// Install bootstrap: only when **zero** users exist.
/// The created account is always **admin** (system rule).
async fn setup(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<Credentials>,
) -> Result<Json<TokenResponse>, ApiError> {
    validate_credentials(&body, locale)?;

    let hash = hash_password_locale(&body.password, locale)?;
    let user = create_first_admin_user(&state.pool, body.username.trim(), &hash)
        .await?
        .ok_or_else(|| ApiError::conflict("already_initialized", locale))?;
    debug_assert!(user.is_admin());
    let token = issue_token_with_role(
        &user.id,
        &user.username,
        &user.role,
        &state.jwt_secret,
        locale,
    )?;
    Ok(Json(TokenResponse { token }))
}

/// Peer address of the request, when the server was started with connect info.
///
/// Read out of the request extensions rather than through `ConnectInfo` so the
/// handler keeps working where the extension is absent (unit tests, and any
/// transport that does not supply it); login then falls back to a username key.
struct ClientIp(Option<std::net::SocketAddr>);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .extensions
                .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                .map(|info| info.0),
        ))
    }
}

/// Identity a login attempt is rate limited under.
///
/// The peer address is preferred: keying on the username alone let an attacker
/// try one password across unlimited usernames, each drawing a fresh budget.
/// Only the IP is used, never the port — the source port changes with every new
/// connection, so including it would hand each attempt its own budget.
fn rate_limit_key(client_ip: Option<std::net::SocketAddr>, username: &str) -> String {
    match client_ip {
        Some(addr) => format!("ip:{}", addr.ip()),
        None => format!("user:{username}"),
    }
}

async fn login(
    State(state): State<AppState>,
    ClientIp(client_ip): ClientIp,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<Credentials>,
) -> Result<Json<TokenResponse>, ApiError> {
    validate_credentials(&body, locale)?;

    let username = body.username.trim();
    let rate_key = rate_limit_key(client_ip, username);

    // Check rate limiting
    {
        let mut limiter = RATE_LIMITER.lock().unwrap();
        let (is_limited, _remaining) = limiter.check(&rate_key);
        if is_limited {
            return Err(ApiError::coded(
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                "too_many_attempts",
                locale,
            ));
        }
    }

    let user = find_user_by_username(&state.pool, username)
        .await?
        .ok_or_else(|| {
            // Record failed attempt
            RATE_LIMITER.lock().unwrap().record_failure(&rate_key);
            ApiError::unauthorized("invalid_credentials", locale)
        })?;

    if !verify_password_locale(&body.password, &user.password_hash, locale)? {
        // Record failed attempt
        RATE_LIMITER.lock().unwrap().record_failure(&rate_key);
        return Err(ApiError::unauthorized("invalid_credentials", locale));
    }

    // Clear rate limit on successful login
    RATE_LIMITER.lock().unwrap().clear(&rate_key);

    let token = issue_token_with_role(
        &user.id,
        &user.username,
        &user.role,
        &state.jwt_secret,
        locale,
    )?;
    Ok(Json(TokenResponse { token }))
}

async fn status(State(state): State<AppState>) -> Result<Json<StatusResponse>, ApiError> {
    let n = count_users(&state.pool).await?;
    Ok(Json(StatusResponse { initialized: n > 0 }))
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let locale = Locale::from_accept_language(
            parts
                .headers
                .get(axum::http::header::ACCEPT_LANGUAGE)
                .and_then(|v| v.to_str().ok()),
        );

        let auth = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("unauthorized", locale))?;

        let token = auth
            .strip_prefix("Bearer ")
            .or_else(|| auth.strip_prefix("bearer "))
            .ok_or_else(|| ApiError::unauthorized("unauthorized", locale))?;

        let claims = decode_token_locale(token, &state.jwt_secret, locale)?;
        let user = find_user_by_id(&state.pool, &claims.sub)
            .await?
            .ok_or_else(|| ApiError::unauthorized("invalid_token", locale))?;
        Ok(AuthUser {
            user_id: user.id,
            role: user.role,
        })
    }
}

impl FromRequestParts<AppState> for ExternalUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let locale = Locale::from_accept_language(
            parts
                .headers
                .get(axum::http::header::ACCEPT_LANGUAGE)
                .and_then(|v| v.to_str().ok()),
        );
        let auth = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("unauthorized", locale))?;
        let token = auth
            .strip_prefix("Bearer ")
            .or_else(|| auth.strip_prefix("bearer "))
            .ok_or_else(|| ApiError::unauthorized("unauthorized", locale))?;

        if !token.starts_with(API_KEY_PREFIX) {
            return Ok(Self {
                user: AuthUser::from_request_parts(parts, state).await?,
                scopes: API_KEY_SCOPES.iter().map(|s| (*s).to_string()).collect(),
                api_key: false,
            });
        }
        if token.len() != API_KEY_PREFIX.len() + 64 {
            return Err(ApiError::unauthorized("invalid_token", locale));
        }
        let hash = Sha256::digest(token.as_bytes());
        let key = chaos_store::find_active_api_key(&state.pool, &hex::encode(hash))
            .await?
            .ok_or_else(|| ApiError::unauthorized("invalid_token", locale))?;
        let user = chaos_store::find_user_by_id(&state.pool, &key.owner_user_id)
            .await?
            .ok_or_else(|| ApiError::unauthorized("invalid_token", locale))?;
        let scopes = serde_json::from_str::<Vec<String>>(&key.scopes)
            .unwrap_or_default()
            .into_iter()
            .collect();
        chaos_store::touch_api_key(&state.pool, &key.id).await?;
        Ok(Self {
            user: AuthUser {
                user_id: user.id,
                role: user.role,
            },
            scopes,
            api_key: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chaos_store::{connect, migrate};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn test_app() -> (Router, AppState) {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let state = AppState::new(pool, "test-secret-key-for-jwt-hs256".to_string());
        let app = Router::new()
            .nest("/api/v1/auth", auth_router())
            .with_state(state.clone());
        (app, state)
    }

    async fn json_body(res: axum::response::Response) -> serde_json::Value {
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn rate_limit_key_prefers_peer_address_over_username() {
        let addr: std::net::SocketAddr = "203.0.113.7:51234".parse().unwrap();
        // Address-keyed: one source spraying many usernames shares a budget.
        assert_eq!(
            rate_limit_key(Some(addr), "alice"),
            rate_limit_key(Some(addr), "bob")
        );
        assert_eq!(rate_limit_key(Some(addr), "alice"), "ip:203.0.113.7");
        // The source port must not split the budget: a new connection from the
        // same host has a different port but is still the same identity.
        let other_port: std::net::SocketAddr = "203.0.113.7:4096".parse().unwrap();
        assert_eq!(
            rate_limit_key(Some(addr), "alice"),
            rate_limit_key(Some(other_port), "alice")
        );
        // Without connect info we fall back to the submitted username.
        assert_eq!(rate_limit_key(None, "alice"), "user:alice");
        assert_ne!(rate_limit_key(None, "alice"), rate_limit_key(None, "bob"));
    }

    #[test]
    fn rate_limiter_expires_stale_keys_and_bounds_growth() {
        let mut limiter = LoginRateLimiter::new();
        limiter.attempts.insert(
            "user:stale".to_string(),
            Attempt {
                count: MAX_LOGIN_ATTEMPTS,
                last_attempt: Instant::now()
                    - std::time::Duration::from_secs(LOCKOUT_DURATION_SECS + 60),
            },
        );
        // The sweep inside `check` must drop the expired entry even though the
        // caller never asks about that key again.
        assert!(!limiter.check("user:fresh").0);
        assert!(!limiter.attempts.contains_key("user:stale"));

        // A spray of distinct keys must not pin memory.
        for i in 0..(MAX_TRACKED_KEYS + 250) {
            limiter.record_failure(&format!("user:spray-{i}"));
        }
        assert!(limiter.attempts.len() > MAX_TRACKED_KEYS);
        let _ = limiter.check("user:someone");
        assert!(limiter.attempts.len() <= MAX_TRACKED_KEYS);
    }

    #[test]
    fn eviction_keeps_locked_out_keys() {
        let mut limiter = LoginRateLimiter::new();

        // Lock out a key first, so its entry is the least recently active one —
        // which used to make it the first eviction candidate.
        let victim = "user:victim";
        for _ in 0..MAX_LOGIN_ATTEMPTS {
            limiter.record_failure(victim);
        }

        // Spray fresh keys well past the cap to force an eviction pass.
        for i in 0..(MAX_TRACKED_KEYS * 2) {
            limiter.record_failure(&format!("user:spray-{i}"));
        }
        assert!(limiter.attempts.len() > MAX_TRACKED_KEYS);

        // The eviction runs inside `check`, which also answers for `victim`.
        let before = limiter.attempts.len();
        assert!(
            limiter.check(victim).0,
            "eviction dropped a locked-out key and reset its budget"
        );
        assert!(
            limiter.attempts.len() < before,
            "sweep did not shed any entries: {}",
            limiter.attempts.len()
        );
    }

    #[test]
    fn rate_limiter_locks_out_one_identity_only() {
        let mut limiter = LoginRateLimiter::new();
        for _ in 0..MAX_LOGIN_ATTEMPTS {
            assert!(!limiter.check("ip:198.51.100.4:1000").0);
            limiter.record_failure("ip:198.51.100.4:1000");
        }
        assert!(limiter.check("ip:198.51.100.4:1000").0);
        // A different identity keeps its own budget.
        assert!(!limiter.check("ip:198.51.100.5:1000").0);
        limiter.clear("ip:198.51.100.4:1000");
        assert!(!limiter.check("ip:198.51.100.4:1000").0);
    }

    #[tokio::test]
    async fn setup_then_second_setup_conflict() {
        let (app, _) = test_app().await;

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/setup")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"username":"admin","password":"password1"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = json_body(res).await;
        assert!(body.get("token").and_then(|t| t.as_str()).is_some());

        let res2 = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/setup")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"username":"admin2","password":"password1"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res2.status(), StatusCode::CONFLICT);
        let err = json_body(res2).await;
        assert_eq!(err["error"]["code"], "already_initialized");
    }

    #[tokio::test]
    async fn concurrent_setup_creates_exactly_one_admin() {
        let (app, state) = test_app().await;
        let request = |username: &str| {
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/setup")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"username":"{username}","password":"password1"}}"#
                )))
                .unwrap()
        };
        let (first, second) = tokio::join!(
            app.clone().oneshot(request("first")),
            app.oneshot(request("second"))
        );
        let mut statuses = [first.unwrap().status(), second.unwrap().status()];
        statuses.sort();
        assert_eq!(statuses, [StatusCode::OK, StatusCode::CONFLICT]);
        assert_eq!(count_users(&state.pool).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn login_and_status() {
        let (app, _) = test_app().await;

        let status0 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/auth/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(status0.status(), StatusCode::OK);
        assert_eq!(json_body(status0).await["initialized"], false);

        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/setup")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"username":"admin","password":"password1"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        let login_ok = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"username":"admin","password":"password1"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login_ok.status(), StatusCode::OK);
        assert!(json_body(login_ok).await.get("token").is_some());

        let login_bad = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"username":"admin","password":"wrongpass"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login_bad.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            json_body(login_bad).await["error"]["code"],
            "invalid_credentials"
        );

        let status1 = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/auth/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(json_body(status1).await["initialized"], true);
    }

    #[tokio::test]
    async fn login_error_localizes_with_accept_language() {
        let (app, _) = test_app().await;
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/setup")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"username":"admin","password":"password1"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        let login_bad = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/login")
                    .header("content-type", "application/json")
                    .header("accept-language", "zh-CN")
                    .body(Body::from(r#"{"username":"admin","password":"wrongpass"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login_bad.status(), StatusCode::UNAUTHORIZED);
        let err = json_body(login_bad).await;
        assert_eq!(err["error"]["code"], "invalid_credentials");
        assert_eq!(err["error"]["message"], "用户名或密码错误");
    }

    #[test]
    fn password_hash_roundtrip() {
        let hash = hash_password("password1").unwrap();
        assert!(verify_password("password1", &hash).unwrap());
        assert!(!verify_password("password2", &hash).unwrap());
    }

    #[test]
    fn jwt_roundtrip() {
        let secret = "test-secret";
        let token = issue_token("uid-1", "admin", secret).unwrap();
        let claims = decode_token(&token, secret).unwrap();
        assert_eq!(claims.sub, "uid-1");
        assert_eq!(claims.username, "admin");
    }
}
