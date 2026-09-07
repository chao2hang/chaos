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

/// Simple in-memory rate limiter for login attempts.
struct LoginRateLimiter {
    attempts: HashMap<String, (u32, Instant)>,
}

impl LoginRateLimiter {
    fn new() -> Self {
        Self {
            attempts: HashMap::new(),
        }
    }

    /// Check if an IP is rate limited. Returns (is_limited, remaining_attempts).
    fn check(&mut self, key: &str) -> (bool, u32) {
        let now = Instant::now();
        if let Some((count, last_attempt)) = self.attempts.get_mut(key) {
            // Reset if lockout period has passed
            if now.duration_since(*last_attempt).as_secs() > LOCKOUT_DURATION_SECS {
                *count = 0;
                *last_attempt = now;
                return (false, MAX_LOGIN_ATTEMPTS);
            }
            if *count >= MAX_LOGIN_ATTEMPTS {
                return (true, 0);
            }
            (false, MAX_LOGIN_ATTEMPTS - *count)
        } else {
            (false, MAX_LOGIN_ATTEMPTS)
        }
    }

    /// Record a failed login attempt.
    fn record_failure(&mut self, key: &str) {
        let now = Instant::now();
        let entry = self.attempts.entry(key.to_string()).or_insert((0, now));
        entry.0 += 1;
        entry.1 = now;
    }

    /// Clear attempts on successful login.
    fn clear(&mut self, key: &str) {
        self.attempts.remove(key);
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
    pub username: String,
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
/// - a raw secret string (≥ 32 bytes), or
/// - a filesystem path (contains `/` or `\\`, or exists as a file) whose contents are the secret.
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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
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
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
            }
        }
    }

    let secret = random_hex_secret(32);
    fs::write(path, &secret)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
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

async fn login(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<Credentials>,
) -> Result<Json<TokenResponse>, ApiError> {
    validate_credentials(&body, locale)?;

    let username = body.username.trim();

    // Check rate limiting
    {
        let mut limiter = RATE_LIMITER.lock().unwrap();
        let (is_limited, _remaining) = limiter.check(username);
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
            RATE_LIMITER.lock().unwrap().record_failure(username);
            ApiError::unauthorized("invalid_credentials", locale)
        })?;

    if !verify_password_locale(&body.password, &user.password_hash, locale)? {
        // Record failed attempt
        RATE_LIMITER.lock().unwrap().record_failure(username);
        return Err(ApiError::unauthorized("invalid_credentials", locale));
    }

    // Clear rate limit on successful login
    RATE_LIMITER.lock().unwrap().clear(username);

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
            username: user.username,
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
                username: user.username,
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
