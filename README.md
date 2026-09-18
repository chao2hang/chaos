# chaos

**[English](README.en.md)** · 中文

面向 [dae](https://github.com/daeuniverse/dae) 的现代控制面：**Rust API + SvelteKit 控制台 + 内置 dae 数据面**。

一次安装即可使用，无需再单独装 daed / dae-wing。

[![Release](https://img.shields.io/github/v/release/chao2hang/chaos?display_name=tag&sort=semver)](https://github.com/chao2hang/chaos/releases/latest)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](Cargo.toml)

---

## 特性一览

| 模块 | 能力 |
|------|------|
| **节点 / 订阅** | 分享链接导入、订阅拉取与定时刷新、延迟探测 |
| **分流编排** | 可视化画布：规则 → 节点组 / DIRECT，校验、模拟、发布并 Apply |
| **DNS / 网络** | DNS 上游与规则；WAN/LAN 网卡与内核参数选项 |
| **运行时** | Apply / 重载 / 停止、日志、诊断（权限 / eBPF / 内核） |
| **权限** | 首个账号为 **admin**；运行时变更、发布、备份等需管理员 |
| **备份** | 创建 / 列表 / 下载 / 恢复（数据库与 `config.dae`） |
| **安装包** | **amd64** 与 **arm64** 的 `.deb` + FHS `.tar.gz`，tag 推送自动发版 |

默认监听 **`0.0.0.0:2030`**（所有 IPv4 网卡），局域网设备可通过主机 IP 访问；如只允许本机访问，可将 `CHAOS_BIND` 改为 `127.0.0.1:2030`。

---

## 安装（推荐）

从 [Releases](https://github.com/chao2hang/chaos/releases/latest) 下载对应架构包。

### Debian / Ubuntu

```bash
# x86_64
curl -fL -O https://github.com/chao2hang/chaos/releases/download/v0.1.0/chaos_0.1.0_amd64.deb
sudo dpkg -i chaos_0.1.0_amd64.deb
# 若依赖提示，可: sudo apt-get install -f

# aarch64
# curl -fL -O https://github.com/chao2hang/chaos/releases/download/v0.1.0/chaos_0.1.0_arm64.deb
# sudo dpkg -i chaos_0.1.0_arm64.deb

sudo systemctl enable --now chaos
```

### Arch / CachyOS 等（非 Debian 系）

`.deb` 的 `Depends: libc6` 可能无法直接配置，请用 FHS tar：

```bash
curl -fL -O https://github.com/chao2hang/chaos/releases/download/v0.1.0/chaos_0.1.0_linux_amd64.tar.gz
sudo tar -xzf chaos_0.1.0_linux_amd64.tar.gz -C /
sudo systemctl enable --now chaos
```

### 打开控制台

浏览器访问：

- 本机：**http://127.0.0.1:2030**
- 局域网：**`http://<主机局域网 IP>:2030`**

> `0.0.0.0` 会向所有可达网卡开放服务。请配置主机防火墙，不要在完成管理员账号初始化前将端口直接暴露到公网。
>
> 已安装旧版本的用户需将 `/etc/chaos/chaos.env` 中的 `CHAOS_BIND` 改为 `0.0.0.0:2030`，然后执行 `sudo systemctl restart chaos`；升级不会自动覆盖已有的环境配置文件。

1. **首次运行** → 创建管理员账号（永久为 `admin`）
2. 导入节点 / 订阅 → 测延迟
3. **分流编排** 中设计规则 → **发布并应用**
4. **仪表盘** 查看状态、重载 / 停止 dae

```bash
systemctl status chaos
journalctl -u chaos -f
```

| 路径 | 说明 |
|------|------|
| `/etc/chaos/chaos.env` | 环境变量（conffile） |
| `/var/lib/chaos/` | 数据库、JWT、dae 工作目录、备份 |
| `/usr/lib/chaos/bin/` | `chaos-api`、`dae` |
| `/usr/share/chaos/web/` | 静态控制台 |

---

## 自动发版

推送 **`v*`** 标签会触发 GitHub Actions：在 **amd64** 与 **arm64** runner 上打包，并发布到 Release。

```bash
git tag v0.1.1
git push origin v0.1.1
```

工作流：[`.github/workflows/release.yml`](.github/workflows/release.yml)

---

## 开发

### 依赖

- Linux（数据面与 dae 一致）
- Rust stable、Node 20+、**pnpm** 10+

### 启动

```bash
pnpm install
./scripts/fetch-dae.sh          # 可选，真实 Apply 需要
./scripts/dev.sh                # API :2030 + Web :5173
# 或: pnpm dev
```

打开 **http://127.0.0.1:5173**；同一局域网内也可使用 **`http://<主机局域网 IP>:5173`**（开发态由 Vite 代理 `/api`）。

```bash
pnpm dev:api    # cargo run -p chaos-api
pnpm dev:web    # SvelteKit
```

### 仓库结构

```text
apps/web              SvelteKit 控制台
crates/chaos-api      REST（默认 0.0.0.0:2030）
crates/chaos-core     领域逻辑 / 延迟 / 配置渲染
crates/chaos-dae      dae 进程与配置
crates/chaos-store    SQLite
packaging/debian/     deb / tar 构建
scripts/fetch-dae.sh  拉取固定版本 dae
locales/              en + zh-CN 共用文案
```

### 常用环境变量

| 变量 | 默认（开发） | 含义 |
|------|----------------|------|
| `CHAOS_BIND` | `0.0.0.0:2030` | API 监听地址；设为 `127.0.0.1:2030` 可限制为仅本机访问 |
| `CHAOS_WEB_HOST` | `0.0.0.0` | Vite 开发服务器监听地址 |
| `CHAOS_DATABASE_URL` | `sqlite:./data/chaos.db?mode=rwc` | SQLite |
| `CHAOS_JWT_SECRET` | 自动生成 `./data/jwt.secret` | 密钥字符串，或**密钥文件路径** |
| `CHAOS_DAE_BIN` | `third_party/dae/current/dae` | dae 可执行文件 |
| `CHAOS_DAE_WORK_DIR` | `./data/dae` | 配置 / pid / 日志 |
| `CHAOS_DAE_LOG_LEVEL` | `info` | 写入 `config.dae` 的 dae 日志级别（`trace`…`fatal`，非法值回退 `info`） |
| `CHAOS_DAE_LOG_MAX_BYTES` | `33554432`（32 MiB） | `dae.log` 轮转阈值；保留一代历史，最多占两份文件 |
| `CHAOS_WEB_DIR` | （未设置则不托管静态站） | 发布包中的控制台目录 |
| `CHAOS_BACKUP_DIR` | `./data/backups` | 备份目录（包内多为 `/var/lib/chaos/backups`） |
| `CHAOS_AUTOSTART_DAE` | — | 为 `1` 时尝试恢复上次配置 |
| `CHAOS_GEOIP_ENABLED` | 关 | 为 `1` 时才向第三方查 GeoIP |

### 鉴权规则

- 空库时仅 **`POST /api/v1/auth/setup`** 可建首个用户，角色固定为 **admin**
- 之后 `/setup` 关闭；新建用户默认为 `user`
- **Apply / 停止 / 重载、发布编排、节点/分组/订阅的写入、改 DNS/网络、备份** 等需 **admin**（否则 `403 admin_required`）

### Apply 与权限

- 延迟测试为 TCP 探测，**不依赖** dae 已运行
- Apply 写入 `config.dae`（模式 **0600**）并启动 / 重载 dae
- 透明代理通常需要 **root 或 CAP_NET_ADMIN / CAP_BPF** 与合适内核
- 默认不走会卡住的交互式 sudo；可用 root 跑服务，或 `CHAOS_DAE_ALLOW_SUDO=1`（需免密）
- dae 以追加方式写 `dae.log`，超过 `CHAOS_DAE_LOG_MAX_BYTES` 时轮转，因此工作目录最多保留两份日志

### 本地打包

```bash
./scripts/fetch-dae.sh
CHAOS_VERSION=0.1.0 CHAOS_ARCH=amd64 ./packaging/debian/build.sh
# arm64（需交叉或 aarch64 主机）:
# CHAOS_DAE_ARCH=arm64 ./scripts/fetch-dae.sh
# CHAOS_VERSION=0.1.0 CHAOS_ARCH=arm64 ./packaging/debian/build.sh
```

### 测试

```bash
cargo test --workspace
pnpm --dir apps/web check
pnpm --dir apps/web build
```

### 国际化

`locales/en.json` 与 `locales/zh-CN.json` 由 Web 与 `chaos-i18n` 共用；控制台可切换语言，API 错误文案跟随 `Accept-Language`。

---

## 端口对照

| 服务 | 端口 |
|------|------|
| **chaos** | **2030** |
| daed（若本机已装） | 2023 |

chaos **不使用** dae-wing GraphQL。

---

## 平台说明

- 内置 **dae** 数据面为 **Linux**。Windows 上控制面可编译，Apply 会提示数据面不可用，见 [docs/platform/windows-data-plane.md](docs/platform/windows-data-plane.md)
- GeoIP 查询会把节点地址发往第三方服务，**默认关闭**
- 产品设计：[docs/superpowers/specs/2026-07-18-chaos-product-design.md](docs/superpowers/specs/2026-07-18-chaos-product-design.md)

---

## 许可证

当前工作区声明为 **MIT**（见 `Cargo.toml`）。打包内嵌的 **dae** 遵循其上游 **AGPL** 许可，分发时请一并遵守。
