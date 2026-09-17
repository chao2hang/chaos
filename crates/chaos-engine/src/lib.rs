//! chaos-engine — Windows data plane backend (Wintun + sing-box/mihomo).
//!
//! This crate provides the Windows-specific data plane implementation that
//! replaces dae's Linux eBPF-based transparent proxy with a Wintun TUN
//! adapter and a user-mode proxy engine (sing-box or mihomo).
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    chaos-engine (Windows)                    │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Backend IR  →  Engine Config Renderer  →  Process Manager  │
//! │      ↓                                        ↓             │
//! │  CompiledRouting                         sing-box/mihomo    │
//! │      ↓                                        ↓             │
//! │  Wintun Adapter  ←──────────────────────  TUN routing       │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Platform Support
//!
//! - **Linux**: Uses dae (see `chaos-dae` crate)
//! - **Windows**: Uses this crate with Wintun + sing-box/mihomo
//! - **macOS**: Not yet supported (future: utun + sing-box)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Windows data plane status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsDataPlaneStatus {
    /// Whether the Windows data plane is available.
    pub available: bool,
    /// Reason if not available.
    pub reason: String,
    /// Wintun driver status.
    pub wintun: WintunStatus,
    /// Proxy engine status.
    pub engine: EngineStatus,
}

/// Wintun driver status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WintunStatus {
    /// Whether Wintun driver is installed.
    pub installed: bool,
    /// Driver version if available.
    pub version: Option<String>,
    /// Adapter name if created.
    pub adapter_name: Option<String>,
}

/// Proxy engine status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatus {
    /// Engine type: "sing-box" or "mihomo".
    pub engine_type: String,
    /// Whether the engine binary is available.
    pub available: bool,
    /// Engine version if available.
    pub version: Option<String>,
    /// Whether the engine is currently running.
    pub running: bool,
}

/// Configuration for the Windows data plane.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WindowsDataPlaneConfig {
    /// Path to the Wintun DLL.
    pub wintun_dll: Option<PathBuf>,
    /// Path to the proxy engine binary (sing-box or mihomo).
    pub engine_bin: Option<PathBuf>,
    /// Engine type: "sing-box" or "mihomo".
    pub engine_type: EngineType,
    /// TUN adapter name.
    pub adapter_name: String,
    /// TUN IP address.
    pub tun_ip: String,
    /// DNS server address.
    pub dns_server: String,
}

/// Supported proxy engine types.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EngineType {
    #[default]
    SingBox,
    Mihomo,
}

impl EngineType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EngineType::SingBox => "sing-box",
            EngineType::Mihomo => "mihomo",
        }
    }
}

/// Backend-neutral routing rule for Windows engine config rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsRoutingRule {
    /// Rule type: "domain", "ip", "process", etc.
    pub rule_type: String,
    /// Rule pattern/value.
    pub pattern: String,
    /// Outbound: "direct", "proxy", "block", or group name.
    pub outbound: String,
}

/// Windows data plane manager.
#[derive(Debug)]
pub struct WindowsDataPlane {
    config: WindowsDataPlaneConfig,
}

impl WindowsDataPlane {
    pub fn new(config: WindowsDataPlaneConfig) -> Self {
        Self { config }
    }

    /// Check if the Windows data plane is available.
    pub fn status(&self) -> WindowsDataPlaneStatus {
        #[cfg(windows)]
        {
            self.check_windows_status()
        }
        #[cfg(not(windows))]
        {
            WindowsDataPlaneStatus {
                available: false,
                reason: "Windows data plane is only available on Windows".to_string(),
                wintun: WintunStatus {
                    installed: false,
                    version: None,
                    adapter_name: None,
                },
                engine: EngineStatus {
                    engine_type: self.config.engine_type.as_str().to_string(),
                    available: false,
                    version: None,
                    running: false,
                },
            }
        }
    }

    #[cfg(windows)]
    fn check_windows_status(&self) -> WindowsDataPlaneStatus {
        // TODO: Implement actual Windows status checks:
        // 1. Check if Wintun DLL exists and is valid
        // 2. Check if engine binary exists
        // 3. Check if adapter is created
        // 4. Check if engine process is running
        WindowsDataPlaneStatus {
            available: false,
            reason: "Windows data plane implementation in progress".to_string(),
            wintun: WintunStatus {
                installed: false,
                version: None,
                adapter_name: None,
            },
            engine: EngineStatus {
                engine_type: self.config.engine_type.as_str().to_string(),
                available: self
                    .config
                    .engine_bin
                    .as_ref()
                    .map(|p| p.is_file())
                    .unwrap_or(false),
                version: None,
                running: false,
            },
        }
    }

    /// Render sing-box config from routing rules.
    pub fn render_singbox_config(&self, rules: &[WindowsRoutingRule]) -> String {
        // TODO: Implement sing-box config rendering
        serde_json::json!({
            "log": {
                "level": "info"
            },
            "inbounds": [
                {
                    "type": "tun",
                    "tag": "tun-in",
                    "interface_name": self.config.adapter_name,
                    "inet4_address": self.config.tun_ip,
                    "auto_route": true,
                    "strict_route": true
                }
            ],
            "outbounds": [
                {
                    "type": "direct",
                    "tag": "direct"
                }
            ],
            "route": {
                "rules": rules.iter().map(|r| {
                    serde_json::json!({
                        "type": r.rule_type,
                        "value": r.pattern,
                        "outbound": r.outbound
                    })
                }).collect::<Vec<_>>()
            }
        })
        .to_string()
    }

    /// Render mihomo (clash) config from routing rules.
    pub fn render_mihomo_config(&self, rules: &[WindowsRoutingRule]) -> String {
        // TODO: Implement mihomo config rendering
        format!(
            r#"
mixed-port: 7890
allow-lan: false
mode: rule
log-level: info

dns:
  enable: true
  listen: 0.0.0.0:53
  default-nameserver:
    - {}

tun:
  enable: true
  stack: system
  dns-hijack:
    - any:53

rules:
{}
"#,
            self.config.dns_server,
            rules
                .iter()
                .map(|r| format!(
                    "  - {},{},{}",
                    r.rule_type.to_uppercase(),
                    r.pattern,
                    r.outbound
                ))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_type_as_str() {
        assert_eq!(EngineType::SingBox.as_str(), "sing-box");
        assert_eq!(EngineType::Mihomo.as_str(), "mihomo");
    }

    #[test]
    fn test_default_config() {
        let config = WindowsDataPlaneConfig::default();
        assert_eq!(config.engine_type, EngineType::SingBox);
        assert!(config.wintun_dll.is_none());
        assert!(config.engine_bin.is_none());
    }

    #[test]
    fn test_status_on_non_windows() {
        let config = WindowsDataPlaneConfig::default();
        let dp = WindowsDataPlane::new(config);
        let status = dp.status();
        #[cfg(not(windows))]
        {
            assert!(!status.available);
            assert!(status.reason.contains("Windows"));
        }
    }
}
