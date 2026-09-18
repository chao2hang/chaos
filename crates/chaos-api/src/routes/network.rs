//! Network document GET/PUT and host interface inventory.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use chaos_core::config_render::{
    normalize_interface_list, normalize_interface_name, NetworkConfig,
};
use serde::{Deserialize, Serialize};

use crate::auth::{AdminUser, AuthUser};
use crate::error::ApiError;
use crate::locale::RequestLocale;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkDocument {
    pub wan_interfaces: Vec<String>,
    pub lan_interfaces: Vec<String>,
    pub auto_config_kernel_parameter: bool,
}

impl From<NetworkConfig> for NetworkDocument {
    fn from(value: NetworkConfig) -> Self {
        Self {
            wan_interfaces: value.wan_interfaces,
            lan_interfaces: value.lan_interfaces,
            auto_config_kernel_parameter: value.auto_config_kernel_parameter,
        }
    }
}

impl From<NetworkDocument> for NetworkConfig {
    fn from(value: NetworkDocument) -> Self {
        Self {
            wan_interfaces: value.wan_interfaces,
            lan_interfaces: value.lan_interfaces,
            auto_config_kernel_parameter: value.auto_config_kernel_parameter,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub ips: Vec<String>,
    pub up: bool,
    pub is_default_route: bool,
}

#[derive(Debug, Serialize)]
pub struct NetworkInterfacesResponse {
    pub interfaces: Vec<NetworkInterfaceInfo>,
}

pub fn network_router() -> Router<AppState> {
    Router::new()
        .route("/network", get(get_network).put(put_network))
        .route("/network/interfaces", get(get_network_interfaces))
}

pub(crate) async fn load_network_config(state: &AppState) -> Result<NetworkConfig, ApiError> {
    let Some(raw) = chaos_store::get_meta(&state.pool, chaos_store::META_NETWORK_DOCUMENT).await?
    else {
        return Ok(NetworkConfig::default());
    };
    Ok(parse_stored_network_document(&raw))
}

fn parse_stored_network_document(raw: &str) -> NetworkConfig {
    match serde_json::from_str::<NetworkDocument>(raw) {
        Ok(doc) => normalize_document(doc).into(),
        Err(_) => NetworkConfig::default(),
    }
}

fn normalize_document(doc: NetworkDocument) -> NetworkDocument {
    let mut wan = normalize_interface_list(&doc.wan_interfaces);
    let lan: Vec<String> = normalize_interface_list(&doc.lan_interfaces)
        .into_iter()
        .filter(|name| name != "auto")
        .collect();
    if wan.is_empty() {
        wan = vec!["auto".into()];
    }
    NetworkDocument {
        wan_interfaces: wan,
        lan_interfaces: lan,
        auto_config_kernel_parameter: doc.auto_config_kernel_parameter,
    }
}

fn validate_put_document(
    body: NetworkDocument,
    locale: chaos_i18n::Locale,
) -> Result<NetworkDocument, ApiError> {
    for raw in body.wan_interfaces.iter().chain(body.lan_interfaces.iter()) {
        if raw.trim().is_empty() {
            continue;
        }
        if normalize_interface_name(raw).is_none() {
            return Err(ApiError::bad_request("network_invalid_interface", locale));
        }
    }
    for raw in &body.lan_interfaces {
        if raw.trim().eq_ignore_ascii_case("auto") {
            return Err(ApiError::bad_request("network_lan_auto_forbidden", locale));
        }
    }
    let wan = normalize_interface_list(&body.wan_interfaces);
    if wan.is_empty() {
        return Err(ApiError::bad_request("network_wan_required", locale));
    }
    let lan = normalize_interface_list(&body.lan_interfaces)
        .into_iter()
        .filter(|name| name != "auto")
        .collect();
    Ok(NetworkDocument {
        wan_interfaces: wan,
        lan_interfaces: lan,
        auto_config_kernel_parameter: body.auto_config_kernel_parameter,
    })
}

async fn get_network(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<NetworkDocument>, ApiError> {
    let config = load_network_config(&state).await?;
    Ok(Json(NetworkDocument::from(config)))
}

async fn put_network(
    _admin: AdminUser,
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Json(body): Json<NetworkDocument>,
) -> Result<Json<NetworkDocument>, ApiError> {
    let document = validate_put_document(body, locale)?;
    let raw = serde_json::to_string(&document)
        .map_err(|error| ApiError::internal_logged(locale, format!("encode network: {error}")))?;
    chaos_store::set_meta(&state.pool, chaos_store::META_NETWORK_DOCUMENT, &raw).await?;
    Ok(Json(document))
}

async fn get_network_interfaces(
    _user: AuthUser,
) -> Result<Json<NetworkInterfacesResponse>, ApiError> {
    Ok(Json(NetworkInterfacesResponse {
        interfaces: list_network_interfaces(),
    }))
}

/// Enumerate host interfaces for the Network UI (Linux via sysfs + /proc).
/// Non-Linux or unreadable hosts return an empty list without error.
pub fn list_network_interfaces() -> Vec<NetworkInterfaceInfo> {
    let net_dir = Path::new("/sys/class/net");
    if !net_dir.is_dir() {
        return Vec::new();
    }
    let default_ifaces = default_route_interfaces();
    let ips_by_iface = interface_ips();
    let mut names: Vec<String> = match fs::read_dir(net_dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name != "lo")
            .collect(),
        Err(_) => return Vec::new(),
    };
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let up = fs::read_to_string(net_dir.join(&name).join("operstate"))
                .map(|value| value.trim().eq_ignore_ascii_case("up"))
                .unwrap_or(false);
            let ips = ips_by_iface.get(&name).cloned().unwrap_or_default();
            let is_default_route = default_ifaces.contains(&name);
            NetworkInterfaceInfo {
                name,
                ips,
                up,
                is_default_route,
            }
        })
        .collect()
}

fn default_route_interfaces() -> HashSet<String> {
    let mut out = HashSet::new();
    // IPv4: /proc/net/route — destination 00000000 is default.
    if let Ok(content) = fs::read_to_string("/proc/net/route") {
        for line in content.lines().skip(1) {
            let mut cols = line.split_whitespace();
            let Some(iface) = cols.next() else {
                continue;
            };
            let Some(dest) = cols.next() else {
                continue;
            };
            if dest == "00000000" {
                out.insert(iface.to_string());
            }
        }
    }
    // IPv6: /proc/net/ipv6_route — first field is dest; all-zero means default.
    if let Ok(content) = fs::read_to_string("/proc/net/ipv6_route") {
        for line in content.lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            // Format: dest dest_prefix src ... device (last field is iface)
            if cols.len() < 10 {
                continue;
            }
            let dest = cols[0];
            if dest.chars().all(|c| c == '0') {
                if let Some(iface) = cols.last() {
                    out.insert((*iface).to_string());
                }
            }
        }
    }
    out
}

fn interface_ips() -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    // IPv4 addresses from /proc/net/fib_trie are awkward, so ask
    // `getifaddrs(3)` instead. Non-Unix hosts are not supported at all (the
    // crates fail to compile there).
    collect_ips_getifaddrs(&mut map);

    for ips in map.values_mut() {
        ips.sort();
        ips.dedup();
    }
    map
}

/// Collect every interface address into `map`, keyed by interface name.
fn collect_ips_getifaddrs(map: &mut HashMap<String, Vec<String>>) {
    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifap) != 0 {
            return;
        }
        let mut cursor = ifap;
        while !cursor.is_null() {
            let entry = &*cursor;
            let name = if entry.ifa_name.is_null() {
                cursor = entry.ifa_next;
                continue;
            } else {
                std::ffi::CStr::from_ptr(entry.ifa_name)
                    .to_string_lossy()
                    .into_owned()
            };
            if name == "lo" {
                cursor = entry.ifa_next;
                continue;
            }
            if let Some(addr) = sockaddr_to_ip_string(entry.ifa_addr) {
                map.entry(name).or_default().push(addr);
            }
            cursor = entry.ifa_next;
        }
        libc::freeifaddrs(ifap);
    }
}

unsafe fn sockaddr_to_ip_string(addr: *const libc::sockaddr) -> Option<String> {
    if addr.is_null() {
        return None;
    }
    let family = (*addr).sa_family as i32;
    match family {
        libc::AF_INET => {
            let sin = &*(addr as *const libc::sockaddr_in);
            let octets = u32::from_be(sin.sin_addr.s_addr).to_be_bytes();
            Some(format!(
                "{}.{}.{}.{}",
                octets[0], octets[1], octets[2], octets[3]
            ))
        }
        libc::AF_INET6 => {
            let sin6 = &*(addr as *const libc::sockaddr_in6);
            let bytes = sin6.sin6_addr.s6_addr;
            // Skip link-local noise optional — keep all for parity with daed.
            let segments: Vec<String> = bytes
                .chunks(2)
                .map(|chunk| format!("{:x}", u16::from_be_bytes([chunk[0], chunk[1]])))
                .collect();
            Some(segments.join(":"))
        }
        _ => None,
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

    use crate::auth::{auth_router, issue_token};
    use crate::state::AppState;

    async fn test_app() -> (Router, AppState) {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, created_at, role) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("u1")
        .bind("admin")
        .bind("test-hash")
        .bind("now")
        .bind("admin")
        .execute(&pool)
        .await
        .unwrap();
        let state = AppState::new(pool, "test-secret-key-for-jwt-hs256".to_string());
        let app = Router::new()
            .nest("/api/v1/auth", auth_router())
            .nest("/api/v1", network_router())
            .with_state(state.clone());
        (app, state)
    }

    async fn json_body(res: axum::response::Response) -> serde_json::Value {
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn network_get_defaults_and_put_round_trip() {
        let (app, state) = test_app().await;
        let token = issue_token("u1", "admin", &state.jwt_secret).unwrap();

        let get_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/network")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_res.status(), StatusCode::OK);
        let body = json_body(get_res).await;
        assert_eq!(body["wan_interfaces"][0], "auto");
        assert!(body["lan_interfaces"].as_array().unwrap().is_empty());
        assert_eq!(body["auto_config_kernel_parameter"], true);

        let put_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/v1/network")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"wan_interfaces":["eth0"],"lan_interfaces":["docker0"],"auto_config_kernel_parameter":false}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put_res.status(), StatusCode::OK);
        let put_body = json_body(put_res).await;
        assert_eq!(put_body["wan_interfaces"][0], "eth0");
        assert_eq!(put_body["lan_interfaces"][0], "docker0");
        assert_eq!(put_body["auto_config_kernel_parameter"], false);

        let get2 = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/network")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body2 = json_body(get2).await;
        assert_eq!(body2["wan_interfaces"][0], "eth0");
        assert_eq!(body2["lan_interfaces"][0], "docker0");
        assert_eq!(body2["auto_config_kernel_parameter"], false);
    }

    #[tokio::test]
    async fn network_put_rejects_empty_wan() {
        let (app, state) = test_app().await;
        let token = issue_token("u1", "admin", &state.jwt_secret).unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/v1/network")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"wan_interfaces":[],"lan_interfaces":[],"auto_config_kernel_parameter":true}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_body(res).await;
        assert_eq!(body["error"]["code"], "network_wan_required");
    }

    #[tokio::test]
    async fn network_put_rejects_auto_on_lan() {
        let (app, state) = test_app().await;
        let token = issue_token("u1", "admin", &state.jwt_secret).unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/v1/network")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"wan_interfaces":["auto"],"lan_interfaces":["auto"],"auto_config_kernel_parameter":true}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_body(res).await;
        assert_eq!(body["error"]["code"], "network_lan_auto_forbidden");
    }

    #[tokio::test]
    async fn network_interfaces_list_does_not_panic() {
        let (app, state) = test_app().await;
        let token = issue_token("u1", "admin", &state.jwt_secret).unwrap();
        let res = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/network/interfaces")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = json_body(res).await;
        assert!(body["interfaces"].is_array());
    }

    #[tokio::test]
    async fn network_put_rejects_non_admin() {
        let (app, state) = test_app().await;
        // Insert a non-admin user row (role is loaded from DB by AuthUser).
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, created_at, role) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("u2")
        .bind("viewer")
        .bind("test-hash")
        .bind("now")
        .bind("user")
        .execute(&state.pool)
        .await
        .unwrap();
        let token =
            crate::auth::issue_token_role("u2", "viewer", "user", &state.jwt_secret).unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/v1/network")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"wan_interfaces":["eth0"],"lan_interfaces":[],"auto_config_kernel_parameter":true}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        let body = json_body(res).await;
        assert_eq!(body["error"]["code"], "admin_required");
    }

    #[test]
    fn parse_corrupt_meta_falls_back_to_default() {
        let config = parse_stored_network_document("not-json");
        assert_eq!(config, NetworkConfig::default());
    }
}
