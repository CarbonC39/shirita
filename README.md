# Shirita

本地优先的 AI 对话引擎（Rust 后端 + Vue 前端），Web 与桌面（Tauri）共享同一套
`shirita-core`。

- `shirita-core` — 领域模型、存储（SQLite/sqlx）、上下文组装、对话、预算/总结、provider 适配。
- `shirita-web` — Axum REST + SSE + Bearer 鉴权适配层。
- `shirita-ui` — Vue 3 + Vite 前端（仅视图层）。
- `shirita-tauri` — Tauri v2 桌面外壳，进程内嵌 `shirita-web`。

## Web

```bash
# 前端
npm --prefix shirita-ui run dev
# 后端（需要 TOKEN_SECRET）
TOKEN_SECRET=dev cargo run -p shirita-web
```

## 桌面端（Tauri）

Shirita 桌面版复用同一套 `shirita-core` + `shirita-web`：Tauri 进程内嵌 Axum
服务（绑 `127.0.0.1` 随机端口），前端经注入的运行时配置走本地 HTTP + SSE。

### 本机依赖（Linux / Debian 13）

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
  librsvg2-dev libayatana-appindicator3-dev patchelf
cargo install tauri-cli --version "^2" --locked
```

### 开发

```bash
cargo tauri dev          # 自动起 vite dev + 桌面窗口
```

### 构建安装包

```bash
npm --prefix shirita-ui run build      # 先产出前端 dist（生产构建不走 beforeBuildCommand）
cargo tauri build --bundles deb        # 本机 Linux（.deb，无需 FUSE）
```

> 生产构建**不配** `beforeBuildCommand`：其 CWD 在本机（配置目录）与 CI 的
> `tauri-action`（仓库根）下不一致，相对路径无法两端通吃。CI 在 `tauri build` 前已显式
> `npm run build`，本机手动先构建前端即可。`beforeDevCommand` 仅 `tauri dev` 用，保留。

> AppImage（`--bundles appimage`）依赖 `linuxdeploy`：需要 FUSE，且其 GTK 插件在
> Debian trixie 上对 `librsvg-2.0` 存在 `libdir` 兼容问题。本机若无 FUSE/遇该问题，
> 用 `.deb` 即可；`.AppImage` 由 CI（ubuntu-latest）产出。

Windows(.msi) / macOS(.dmg) / Linux(.AppImage) 由 GitHub Actions
（`.github/workflows/desktop.yml`，`workflow_dispatch` 或 `v*` tag 触发）构建，产物为
**未签名** 安装包——macOS 需右键「打开」绕过 Gatekeeper，Windows 会有 SmartScreen 警告。

### provider 配置（桌面）

当前桌面版仍读环境变量：`PROVIDER`（`anthropic` / `ollama` / 留空 = OpenAI 兼容）、
`OPENAI_API_KEY` / `OPENAI_BASE_URL` / `OPENAI_MODEL` 等。未设 key 时使用离线 Echo。
应用内 provider 配置为后续小项。
