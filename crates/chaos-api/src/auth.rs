//! Auth routes, password hashing, JWT helpers, and AuthUser extractor.

use axum::extract::FromRequestParts;
use axum::extract::State;
use axum::http::request::Parts;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use chaos_store::{count_users, create_user, find_user_by_username};

use crate::error::ApiError;
use crate::state::AppState;

const JWT_TTL_HOURS: i64 = 24;
const MIN_PASSWORD_LEN: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub username: String,
    pub exp: i64,
}

/// Authenticated user extracted from `Authorization: Bearer <jwt>`.
#[derive(Debug, Clone)]
#[allow(dead_code)] // reserved for per-user scoping later
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
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
}

/// Load JWT secret from `CHAOS_JWT_SECRET` or `./data/jwt.secret`.
/// Creates a random 32-byte hex secret file if missing.
pub fn load_or_create_jwt_secret() -> anyhow::Result<String> {
    if let Ok(secret) = std::env::var("CHAOS_JWT_SECRET") {
        let secret = secret.trim().to_string();
        if !secret.is_empty() {
            return Ok(secret);
        }
    }

    let path = Path::new("./data/jwt.secret");
    if path.exists() {
        let secret = fs::read_to_string(path)?.trim().to_string();
        if secret.is_empty() {
            anyhow::bail!("JWT secret file is empty: {}", path.display());
        }
        return Ok(secret);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let secret = random_hex_secret(32);
    fs::write(path, &secret)?;
    Ok(secret)
}

fn random_hex_secret(num_bytes: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; num_bytes];
    OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::internal(format!("password hash failed: {e}")))
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, ApiError> {
    let parsed = PasswordHash::new(password_hash)
        .map_err(|e| ApiError::internal(format!("invalid password hash: {e}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

pub fn issue_token(user_id: &str, username: &str, secret: &str) -> Result<String, ApiError> {
    let exp = (Utc::now() + Duration::hours(JWT_TTL_HOURS)).timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        exp,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ApiError::internal(format!("jwt encode failed: {e}")))
}

pub fn decode_token(token: &str, secret: &str) -> Result<Claims, ApiError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| ApiError::unauthorized("invalid_token", "invalid or expired token"))
}

fn validate_credentials(body: &Credentials) -> Result<(), ApiError> {
    let username = body.username.trim();
    if username.is_empty() {
        return Err(ApiError::bad_request(
            "invalid_request",
            "username is required",
        ));
    }
    if body.password.len() < MIN_PASSWORD_LEN {
        return Err(ApiError::bad_request(
            "invalid_request",
            format!("password must be at least {MIN_PASSWORD_LEN} characters"),
        ));
    }
    Ok(())
}

async fn setup(
    State(state): State<AppState>,
    Json(body): Json<Credentials>,
) -> Result<Json<TokenResponse>, ApiError> {
    validate_credentials(&body)?;

    let n = count_users(&state.pool).await?;
    if n > 0 {
        return Err(ApiError::conflict(
            "already_initialized",
            "admin user already exists",
        ));
    }

    let hash = hash_password(&body.password)?;
    let user = create_user(&state.pool, body.username.trim(), &hash).await?;
    let token = issue_token(&user.id, &user.username, &state.jwt_secret)?;
    Ok(Json(TokenResponse { token }))
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<Credentials>,
) -> Result<Json<TokenResponse>, ApiError> {
    validate_credentials(&body)?;

    let user = find_user_by_username(&state.pool, body.username.trim())
        .await?
        .ok_or_else(|| {
            ApiError::unauthorized("invalid_credentials", "invalid username or password")
        })?;

    if !verify_password(&body.password, &user.password_hash)? {
        return Err(ApiError::unauthorized(
            "invalid_credentials",
            "invalid username or password",
        ));
    }

    let token = issue_token(&user.id, &user.username, &state.jwt_secret)?;
    Ok(Json(TokenResponse { token }))
}

async fn status(State(state): State<AppState>) -> Result<Json<StatusResponse>, ApiError> {
    let n = count_users(&state.pool).await?;
    Ok(Json(StatusResponse {
        initialized: n > 0,
    }))
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                ApiError::unauthorized("unauthorized", "missing Authorization header")
            })?;

        let token = auth
            .strip_prefix("Bearer ")
            .or_else(|| auth.strip_prefix("bearer "))
            .ok_or_else(|| {
                ApiError::unauthorized("unauthorized", "expected Bearer token")
            })?;

        let claims = decode_token(token, &state.jwt_secret)?;
        Ok(AuthUser {
            user_id: claims.sub,
            username: claims.username,
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
                    .body(Body::from(
                        r#"{"username":"admin","password":"password1"}"#,
                    ))
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
                    .body(Body::from(
                        r#"{"username":"admin","password":"password1"}"#,
                    ))
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
                    .body(Body::from(
                        r#"{"username":"admin","password":"password1"}"#,
                    ))
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
                    .body(Body::from(
                        r#"{"username":"admin","password":"wrongpass"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login_bad.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(login_bad).await["error"]["code"], "invalid_credentials");

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
