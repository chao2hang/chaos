# Windows data plane boundary

The control plane and orchestration compiler are platform-neutral, but the
current Linux data plane is not. `dae` depends on Linux TUN/eBPF primitives and
must never be launched from a Windows build.

The supported Windows shape is an independent backend:

1. A signed [Wintun](https://www.wintun.net/) driver provides a layer-3 virtual
   adapter and packet I/O.
2. A user-mode proxy engine, currently targeted at sing-box or mihomo, owns TUN
   routing, DNS interception, sniffing, UDP, IPv4/IPv6, and outbound health
   checks.
3. Chaos renders a backend-neutral routing IR. The Windows backend translates
   that IR to the selected engine's JSON/YAML; it does not consume `config.dae`.

`chaos-dae` now exposes a `DataPlaneBackend` boundary. Linux reports the dae
backend; Windows reports `windows-wintun-engine` as unavailable until the
signed adapter, engine process supervisor, route/DNS rollback, and installer
are implemented. The API returns an explicit `windows_data_plane_unavailable`
error instead of trying to execute Linux dae arguments.

## Required implementation phases

- **Backend IR:** represent ordered rules, terminal direct, node groups, and
  future hop chains without dae-specific expressions. The hop-chain contract is
  specified separately in [orchestration-v3-hops.md](orchestration-v3-hops.md).
- **Engine supervisor:** validate engine config, launch it with a private
  working directory, capture the process identity, and restore the previous
  config on failed reload.
- **Wintun lifecycle:** install/verify the signed DLL, request elevation, set
  routes and DNS, and restore them on stop/crash.
- **Network edge cases:** endpoint bypass, loop prevention, IPv4/IPv6, UDP,
  DNS interception, and sleep/resume recovery.
- **Native verification:** run the Windows job in a Windows VM/runner with a
  real Wintun adapter and sing-box/mihomo binary. Linux cross-compilation is
  not evidence that the data plane works.

Do not set a Windows engine environment variable to bypass the capability check;
the placeholder backend intentionally remains unavailable until those phases
are complete.
