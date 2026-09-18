//! chaos-core — domain helpers (link parse, later latency/config).

#[cfg(not(unix))]
compile_error!(
    "chaos-core is Unix-only: chaos is a Linux dae control plane. Windows support \
     was removed in 0.1.27."
);

pub mod config_parse;
pub mod config_render;
pub mod geoip;
pub mod latency;
pub mod link;
pub mod logtail;
pub mod orchestration;
pub mod subscription;
pub mod traffic;
