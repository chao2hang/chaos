# Windows 数据面开发 TODO

> 目标：在 Windows 上通过 Wintun + sing-box 实现系统代理，使 chaos 成为跨平台产品。

## 前置条件

- [ ] Windows 10/11 开发机（或 VM）
- [ ] Rust MSVC 工具链：`rustup target add x86_64-pc-windows-msvc`
- [ ] sing-box Windows 二进制（从 GitHub Releases 下载）
- [ ] Wintun DLL（从 https://www.wintun.net/ 下载签名版）
- [ ] 管理员权限（TUN 适配器需要）

---

## Phase 1：sing-box 配置渲染器

**目标**：将 chaos 的 `CompiledRouting` IR 渲染为 sing-box JSON 配置。

- [ ] 在 `crates/chaos-engine/src/` 新建 `singbox_config.rs`
- [ ] 实现 `render_singbox_config(nodes, routing, dns) -> String`
  - [ ] `inbounds`：TUN inbound（auto_route: true, strict_route: true）
  - [ ] `outbounds`：每个节点生成对应协议的 outbound（trojan/ss/vmess/vless/hysteria2/tuic）
  - [ ] `route.rules`：将 `CompiledRoute条件转为 sing-box rule_set
  - [ ] `dns`：渲染 DNS 上游和规则
  - [ ] 直连/阻断 outbound（direct/block）
- [ ] 单元测试：验证渲染输出是合法 JSON 且结构正确
- [ ] 集成测试：用 `sing-box check -c config.json` 验证配置有效性

**参考**：
- sing-box 配置文档：https://sing-box.sagernet.org/configuration/
- 节点协议映射：trojan→trojan, shadowsocks→shadowsocks, vmess→vmess, vless→vless

---

## Phase 2：引擎进程管理

**目标**：启动/停止/监控 sing-box 进程。

- [ ] 在 `crates/chaos-engine/src/` 新建 `process.rs`
- [ ] 实现 `EngineManager`：
  - [ ] `start(config_path)` — 启动 `sing-box run -c config.json`
  - [ ] `stop()` — 优雅终止（先 SIGTERM/ctrl-break，超时后 kill）
  - [ ] `is_running()` — 检查进程存活
  - [ ] `reload(config_path)` — sing-box 不原生支持热重载，用 stop+start
  - [ ] PID 文件管理（`work_dir/engine.pid`）
  - [ ] 日志捕获（stdout/stderr → `work_dir/engine.log`）
- [ ] Windows 特殊处理：
  - [ ] 使用 `CREATE_NO_WINDOW` flag 避免弹出控制台
  - [ ] 使用 Job Object 确保子进程随父进程终止
- [ ] 单元测试：用 fake 可执行文件测试生命周期

---

## Phase 3：Wintun 适配器管理

**目标**：安装/创建/销毁 Wintun TUN 适配器。

- [ ] 在 `crates/chaos-engine/src/` 新建 `wintun.rs`
- [ ] 下载并放置 `wintun.dll`（x86_64）到 `third_party/wintun/`
- [ ] 实现 Wintun FFI 绑定（或使用 `wintun` crate）：
  - [ ] `WintunLoadLibrary(path)` — 加载 DLL
  - [ ] `WintunOpenAdapter(name)` — 打开已有适配器
  - [ ] `WintunCreateAdapter(name, tunnel_type)` — 创建新适配器
  - [ ] `WintunCloseAdapter(handle)` — 关闭适配器
  - [ ] `WintunSetAdapterAddresses()` — 设置 IP/子网
- [ ] 实现 `WintunManager`：
  - [ ] `ensure_adapter()` — 创建或复用名为 "chaos" 的适配器
  - [ ] `remove_adapter()` — 清理
  - [ ] `set_dns(servers)` — 设置适配器 DNS
- [ ] 注意：sing-box 的 TUN inbound 自带 auto_route，可能不需要手动管理 Wintun
  - 先测试 sing-box 内置 TUN 是否足够，再决定是否需要独立 Wintun 管理

---

## Phase 4：平台后端集成

**目标**：让 `chaos-dae` 的 `PlatformBackend` 在 Windows 上可用。

- [ ] 修改 `crates/chaos-dae/src/lib.rs`：
  - [ ] `WindowsWintunBackend::status()` 改为检测 sing-box 二进制是否存在
  - [ ] 返回 `ready: true` 当 sing-box + wintun.dll 均就位
- [ ] 修改 `crates/chaos-api/src/routes/runtime.rs`：
  - [ ] `apply_current_config_locked()` 在 Windows 上调用 `chaos-engine` 而非 `chaos-dae`
  - [ ] 根据 `cfg!(windows)` 选择渲染器（dae config vs sing-box JSON）
- [ ] 新增环境变量：
  - [ ] `CHAOS_ENGINE_BIN`：sing-box 路径（默认 `third_party/sing-box/sing-box.exe`）
  - [ ] `CHAOS_WINTUN_DLL`：wintun.dll 路径（默认 `third_party/wintun/wintun.dll`）

---

## Phase 5：网络边界处理

**目标**：处理 Windows 网络特殊情况。

- [ ] 回环防止：确保 chaos-api 自身的请求不走代理
  - [ ] sing-box route 中排除本机 IP 和 127.0.0.1
- [ ] DNS 拦截：
  - [ ] 验证 sing-box TUN 模式下 DNS 是否自动劫持
  - [ ] 如不自动，需设置适配器 DNS 为 127.0.0.1
- [ ] IPv6 支持：
  - [ ] TUN 适配器同时分配 IPv4 + IPv6 地址
  - [ ] 路由规则覆盖 IPv6 流量
- [ ] 休眠/恢复：
  - [ ] 监听 `WM_WTSSESSION_CHANGE` 或电源事件
  - [ ] 恢复后重建 TUN 路由
- [ ] 多网卡：
  - [ ] 检测活跃网卡，设置正确的出口

---

## Phase 6：安装与权限

**目标**：Windows 下的安装体验。

- [ ] 管理员权限检测：
  - [ ] 启动时检查是否以管理员运行
  - [ ] 非管理员时提示用户（TUN 需要管理员）
- [ ] 可选：注册为 Windows 服务（`windows-service` crate）
  - [ ] 开机自启
  - [ ] 服务恢复策略
- [ ] 可选：NSIS/WiX 安装包
  - [ ] 捆绑 chaos-api.exe + sing-box.exe + wintun.dll + web 资源
  - [ ] 安装时注册服务
  - [ ] 卸载时清理适配器

---

## Phase 7：验证清单

- [ ] `cargo build --target x86_64-pc-windows-msvc` 编译通过
- [ ] 管理员运行 chaos-api，Web UI 可访问
- [ ] 导入节点 → Apply → sing-box 启动
- [ ] 浏览器访问 google.com 走代理
- [ ] 国内网站直连（分流规则生效）
- [ ] Stop → 网络恢复正常
- [ ] 休眠恢复后代理仍工作
- [ ] 非管理员运行时给出明确错误提示

---

## 文件结构预览

```
crates/chaos-engine/
├── Cargo.toml
└── src/
    ├── lib.rs              # 已有：架构定义 + 状态类型
    ├── singbox_config.rs   # Phase 1：配置渲染
    ├── process.rs          # Phase 2：进程管理
    ├── wintun.rs           # Phase 3：Wintun 适配器
    └── platform.rs         # Phase 4：平台集成

third_party/
├── sing-box/
│   └── sing-box.exe        # 手动放置
└── wintun/
    └── wintun.dll          # 手动放置
```

---

## 开发顺序建议

```
Phase 1 (配置渲染) → Phase 2 (进程管理) → Phase 4 (集成)
                                              ↓
                              Phase 5 (网络边界) ← Phase 3 (Wintun，可能不需要)
                                              ↓
                              Phase 6 (安装) → Phase 7 (验证)
```

先跑通最小闭环（Phase 1+2+4），再处理边界情况。
