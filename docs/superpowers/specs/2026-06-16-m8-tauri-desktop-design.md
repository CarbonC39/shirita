# M8 设计 — Tauri 桌面端（内嵌 Axum）

> 本文档是 M8 的里程碑级设计 spec，承接路线图 `docs/superpowers/specs/2026-06-12-shirita-roadmap-design.md` §M8。
> 后续走独立的 plan → 实现循环。

## 1. 目标与完成标志

把同一套 `shirita-core` + `shirita-web` 包装成原生桌面应用，验证「core 共享、Web/桌面同源」论点。

**完成标志**：本机构建出可运行的 **Linux** 桌面应用（共享同一 core，经进程内嵌的 web 层提供 REST+SSE），且 GitHub Actions 三平台（Linux / macOS / Windows）`tauri build` 全绿、产出 **未签名** 安装包（`.AppImage`/`.deb`、`.dmg`、`.msi`）。

### 已确认的关键决策（brainstorm 结论）

- **传输架构 = 方案 B（进程内嵌 Axum）**。`shirita-tauri` 在进程内起一个 tokio 任务跑现成的 `shirita_web` 路由，绑 `127.0.0.1:<随机端口>`；webview 前端启动时把 `BASE` 指向该本地服务、注入 token，继续用 `fetch` + SSE。**不重写任何业务逻辑**，不引入 Tauri IPC command / 事件流，不引入 `asset://` 自定义协议。（spec review 追加两处后端**附加**：跨域 CORS 层 §5b、优雅关闭 §8.2。）
- **范围 = 三平台全打包**。Linux 本机构建+验证；macOS/Windows 经 GitHub Actions CI 出产物，本机不验证其安装包。
- **签名 = 永不签名（unsigned forever）**。CI 出未签名安装包，零证书、零 secrets。macOS 用户需右键绕过 Gatekeeper，Windows 会被 SmartScreen 警告——接受。
- **数据目录 = Tauri 路径解析器**（系统用户数据目录）。非可选项。
- **provider 配置暂留 env**。打包桌面二进制仍读 `PROVIDER`/`OPENAI_*` env（有则接真实 LLM，无则离线 `EchoProvider`）。「应用内 provider/API key 配置」**不在本轮**，作为独立后续小项。

### 不做（YAGNI / 明确推迟）

- Tauri IPC command 包装、Tauri 事件流式（方案 A，被否决）。
- `asset://` 自定义协议（方案 C 的增量，未采纳；assets 仍走本地 HTTP `/assets`）。
- 应用内 provider/API key 配置 UI、provider 热重建。
- 代码签名 / 公证 / 自动更新（updater）。
- 系统托盘、多窗口、深链接等桌面增强。

## 2. 现状与接口事实（写设计时已核实）

- workspace 成员：`shirita-core`、`shirita-web`（`Cargo.toml` `members = ["shirita-core", "shirita-web"]`）。无 `shirita-tauri`。
- `shirita-web`：`pub fn app(state: AppState) -> Router`（`lib.rs:17`），整层 REST + SSE + Bearer 鉴权。`pub use {Generations, AppState}`。
- `AppState { storage: Arc<dyn Storage>, config: Arc<Config>, provider: Arc<dyn ModelProvider>, token_counter: Arc<dyn TokenCounter>, model: String, generations: Arc<Generations> }`，字段全 pub。
- 鉴权（`auth.rs`）：`require_bearer` 做简单字符串比较 `token == state.config.token_secret`。
- `Config`（`config.rs`）：字段 `database_path / assets_dir / token_secret / openai_base_url / openai_api_key / openai_model`。`Config::new(db, assets, token_secret)` 要求 `token_secret` 非空。`from_env()` 读 env。
- provider 在 `main.rs` 启动时按 `PROVIDER` env 一次性构造为 `Arc<dyn ModelProvider>`（anthropic/ollama/openai/echo）。**不**从 settings DB 读。
- `resolve_asset_url`（`routes/assets.rs:15`）：web 返回 `/assets/<rel>`。已留注释「Tauri 入口在 M8 返回 `asset://localhost/<rel>`」——**本设计不采纳该 asset:// 分支**，保持 `/assets/..`，由前端拼 BASE 指向 localhost 服务。
- 前端 `client.ts`：`const BASE = import.meta.env.VITE_API_BASE ?? ''`，`const TOKEN = import.meta.env.VITE_API_TOKEN ?? ''`，`authHeaders()` 发 `Bearer ${TOKEN}`；所有请求走 `${BASE}/api/...`，SSE 走 fetch ReadableStream。
- 构建环境：Debian 13（trixie），已装 `webkit2gtk-4.1 (2.52.3)`、`gtk+-3.0 (3.24.49)`、`libsoup-3.0 (3.6.5)`、`javascriptcoregtk-4.1`——满足 **Tauri v2** Linux 依赖。`tauri-cli` 未安装（用 `cargo install tauri-cli` 或 build-dependency）。rustc 1.95，node v24。

## 3. 架构总览

```
shirita-tauri (bin, Tauri v2)
  ├─ setup hook（异步启动序列）:
  │    1. 算数据目录（Tauri path resolver: app_data_dir）
  │    2. Config::new(<data>/shirita.db, <data>/assets, <random-token>)
  │       并按 env 叠加 provider 字段（openai_base_url/api_key/model）
  │    3. SqliteStorage::connect + run_migrations + ensure_default_template
  │    4. create_dir_all(assets_dir)
  │    5. 构造 provider（PROVIDER env，无 key → EchoProvider）
  │    6. TcpListener::bind("127.0.0.1:0") → 取 local_addr().port()
  │    7. tokio::spawn(axum::serve(listener, shirita_web::app_with_cors(state))
  │             .with_graceful_shutdown(token.cancelled()))   // §5b CORS / §8.2 优雅关闭
  │    8. initialization_script 注入
  │       window.__SHIRITA_RUNTIME__ = { base:"http://127.0.0.1:<port>", token:"<random>" }
  └─ 窗口加载 frontendDist（shirita-ui/dist 的 Vue 产物）
```

一句话：**Tauri = 原生外壳 + 进程内 Axum + 既有 Vue 前端**。复用 `shirita-web` 整层（含 M7 的 `/import`、`DefaultBodyLimit` 等）与 `shirita-core` 全部业务，**不重写业务逻辑**；后端仅两处**附加**（非改写）：CORS 辅助层（§5b）与优雅关闭接线（§8.2）。

## 4. 后端：`shirita-tauri` 二进制

### 4.1 workspace 接线
- `Cargo.toml` `members` 加 `"shirita-tauri"`。
- `shirita-tauri/Cargo.toml`：依赖 `shirita-core`、`shirita-web`（path）、`tauri` v2、`tokio`、`uuid`、`tauri-plugin-dialog`（启动错误框）。`build-dependencies` `tauri-build`。
- `shirita-tauri/build.rs`：`tauri_build::build()`。
- `shirita-tauri/tauri.conf.json`、`icons/`、`capabilities/`（见 §6 打包）。

### 4.2 启动序列（`setup` hook 内）
按 §3 第 1–8 步执行。要点：
- **token**：`uuid::Uuid::new_v4().to_string()` 作 `Config` 的 `token_secret`（满足非空校验）。桌面进程与前端共享同一随机值，localhost 之外无从得知。
- **provider**：复用 `main.rs` 现有 `PROVIDER` env 选择逻辑（可抽成 `shirita-web` 或 `shirita-core` 的一个 `provider_from_env(&Config)` 辅助函数，避免在 tauri bin 里复制 13 行 match；该抽取**仅移动既有代码**，不改行为）。
- **端口**：`bind("127.0.0.1:0")` 让 OS 分配空闲端口，`local_addr().port()` 读回真实端口，杜绝固定端口冲突。
- **server 任务**：`tokio::spawn(async move { axum::serve(listener, app_with_cors(state)).with_graceful_shutdown(token.cancelled()).await })`（CORS 见 §5b、优雅关闭见 §8.2），与 webview 同进程同 tokio runtime（Tauri v2 用 tokio）。
- **BASE**：`format!("http://127.0.0.1:{port}")`。

### 4.3 注入运行时配置
Tauri `WebviewWindowBuilder` / `app.handle()` 的 `initialization_script`（在页面任何脚本前执行）注入：
```js
window.__SHIRITA_RUNTIME__ = { base: "http://127.0.0.1:<port>", token: "<random>" };
```
注入字符串由 Rust 端 `serde_json` 安全序列化拼接，避免注入破坏。

## 5. 前端：运行时配置注入（唯一前端源码改动）

`client.ts` 顶部改为优先读注入值、回退到既有 env（Web 构建分支行为完全不变）：

```ts
const RT = (globalThis as any).__SHIRITA_RUNTIME__ as { base?: string; token?: string } | undefined
const BASE  = RT?.base  ?? import.meta.env.VITE_API_BASE  ?? ''
const TOKEN = RT?.token ?? import.meta.env.VITE_API_TOKEN ?? ''
```

其余 `fetch` / SSE / `downloadExport` / asset URL 拼接全部沿用 `BASE` / `TOKEN`，**无需改动**。这是前端唯一的源码改动（后端侧的附加改动见 §5b CORS 与 §8.2 优雅关闭）。

- 回归：`client.test.ts`（含 M7 `importFile`、SSE 等）在 `RT` 为 `undefined`（jsdom 无注入）时走 env 回退分支，断言不变。新增一条单测覆盖「`window.__SHIRITA_RUNTIME__` 存在时 BASE/TOKEN 取注入值」。

## 5b. 跨域（CORS）—— 内嵌 server 的必备项（spec review 追加）

方案 B 下 webview 的 origin（Tauri 自有协议）与内嵌 server 的 `http://127.0.0.1:<port>` **不同源**；带 `Authorization` 头的 `fetch` 不是「简单请求」，会先发 **preflight `OPTIONS`**。仅在 `tauri.conf.json` 配 CSP `connect-src` **不够**——CSP 只决定「页面被允许连什么」，CORS 决定「server 是否许可这次跨域响应」。**两者都要**。否则首屏因 preflight 失败而白屏。

设计：

- `shirita-web` 的 `tower-http` 依赖加 `"cors"` feature（已有 `0.6`，仅加 feature）。
- 新增 `pub fn app_with_cors(state: AppState) -> Router`：对 `app(state)` 返回的 router 施加**最外层** `CorsLayer`。`app()` 本身保持无 CORS（Web 部署同源、不需要，也不向公网 server 平白发 CORS 头）。
- **层序是关键**：`CorsLayer` 作为最外层 `.layer()`，preflight `OPTIONS` 由它直接短路应答，**根本不进 `require_bearer`**（auth 是 `protected` 上的内层 `route_layer`）；真实请求则穿过 CORS → auth → handler，响应回程被 CORS 补上 `Access-Control-Allow-Origin`。
- `CorsLayer` 配置：`allow_origin` 白名单 `tauri://localhost`、`https://tauri.localhost`、`http://tauri.localhost`（跨平台覆盖，可调）；`allow_methods` = GET/POST/PUT/DELETE/OPTIONS；`allow_headers` = `AUTHORIZATION`、`CONTENT_TYPE`。Bearer 走 header 而非 cookie，**无需** `allow_credentials`。
- 桌面 bin 用 `app_with_cors(state)` 替代 `app(state)`。
- 测试：`shirita-web` 加 preflight 单测（带 `Origin: tauri://localhost` 的 `OPTIONS` 得 2xx + `access-control-allow-*` 头，且不被 401）。既有端点行为不变（无 `Origin` / 同源请求不受影响），107 测试不变。

## 6. 打包与 CI

### 6.1 `tauri.conf.json`（Tauri v2 schema）
- `build.frontendDist = "../shirita-ui/dist"`。
- `build.beforeBuildCommand = "npm --prefix ../shirita-ui run build"`（即 `vue-tsc -b && vite build`，已核实）。
- `build.devUrl = "http://localhost:5173"`（vite 默认端口，开发模式 `tauri dev`）。
- `app.windows`：标题 `Shirita`、合理初始尺寸。
- `bundle.active = true`，`bundle.targets`：Linux `["appimage","deb"]`、macOS `["dmg"]`、Windows `["msi"]`（或 `"all"`，由各 runner 产对应目标）。
- `identifier`：如 `app.shirita.desktop`。
- `icons/`：占位图标集（各平台所需尺寸）。
- 不配置 `updater`、不配置签名字段。

### 6.2 GitHub Actions（`.github/workflows/desktop.yml`）
- 触发：push 到 `main`、tag、或手动 `workflow_dispatch`（具体在实现时定，倾向 `workflow_dispatch` + push tag，避免每次 push 跑重 CI）。
- 矩阵：`ubuntu-latest` / `macos-latest` / `windows-latest`。
- 步骤：checkout → 装 Rust（`dtolnay/rust-toolchain`）→ 装 Node → （Linux）`apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev ...` → `npm ci`（shirita-ui）→ `tauri build`（经 `tauri-apps/tauri-action` 或直接 cargo）→ 上传安装包 artifacts。
- **未签名**：不注入任何证书 secret。

### 6.3 本机验证
- 仅 **Linux**：`tauri dev` 手动跑通 + `tauri build` 出 AppImage/deb。
- Win/macOS：依赖 CI，本机不产、不验证其安装包。

## 7. 数据目录与资产

- DB 与 assets 落 Tauri **app data dir**：
  - Linux `~/.local/share/<identifier>`、macOS `~/Library/Application Support/<identifier>`、Windows `%APPDATA%\<identifier>`（由 Tauri path API 解析，不手写平台分支）。
- `database_path = <data>/shirita.db`，`assets_dir = <data>/assets`，首启 `create_dir_all`。
- 迁移与 `ensure_default_template` 在启动序列内执行，首启自动建库+seed。
- assets 仍走进程内 `/assets` HTTP（方案 B）；`resolve_asset_url` **不改**，前端拼 BASE 即指向 localhost 服务，头像/背景照常加载。

## 8. 错误处理与生命周期

### 8.1 启动错误
- 数据目录不可写 / `SqliteStorage::connect` 失败 / `run_migrations` 失败：经 `tauri-plugin-dialog` 弹**原生错误对话框**（含简短原因），记 `tracing` 日志后退出。不让 webview 连一个不存在的后端而白屏。
- **`bind 127.0.0.1:0` 失败**（spec review 强化）：虽用随机端口已极大降低冲突概率，仍须强捕获。错误对话框文案除原因外，**明确建议「请检查本地防火墙或杀毒软件的拦截记录」**，降低排查成本。
- token 不符 → 既有 401 链路不变（本地双方共享同一注入值，实际不会触发）。
- 生成取消 / SSE 中断 → 既有 `Generations` 注册表 + `futures::stream::abortable` 机制不变。

### 8.2 优雅关闭（spec review 追加）
SQLite/WAL 本就 per-commit 崩溃安全，已提交数据即便进程被杀也不丢；但显式优雅关闭能 **checkpoint WAL、避免下次启动 `database is locked`、并让 in-flight 写完成**，对桌面应用值得做。

设计（**零 core 改动**——`SqliteStorage` 已暴露 `pub fn pool(&self) -> &SqlitePool`）：

- 启动序列里把 storage 装箱为 `Arc<dyn Storage>` **之前**，先 `let pool = concrete.pool().clone();`（`SqlitePool` 是 `Arc` 包装，clone 廉价）存入 bin 状态。
- 引入 `tokio_util::sync::CancellationToken`（`tokio-util` 仅作 **shirita-tauri** 依赖）；server 任务用 `axum::serve(listener, app_with_cors(state)).with_graceful_shutdown(async move { token.cancelled().await })`——收到信号即停止收新请求、放干在途连接。
- Tauri 生命周期：`builder.build(ctx)?.run(|_, event| ...)` 中拦截 `RunEvent::ExitRequested { .. }`（或 `RunEvent::Exit`），经 `tauri::async_runtime::block_on(async { token.cancel(); pool.close().await; })` 广播关闭并显式关池，再放行退出。
- 测试：smoke 测试覆盖「`token.cancel()` 后 server graceful shutdown 正常返回、`pool.close()` 后池不可再用」。

## 9. 测试

- **core/web 既有业务逻辑不动**（B 方案不改端点行为）；新增**附加**项：`tower-http` `cors` feature + `app_with_cors` 辅助 + CORS preflight 单测（§5b）。既有 107 测试不变。若抽取 `provider_from_env` 辅助函数，补一条其行为单测（env→provider 类型映射）。
- **前端**：`client.test.ts` 回归（`RT` undefined 时 env 回退，断言不变）+ 新增 1 条「注入值优先」单测。`vue-tsc` + `vitest` + `vite build` 全绿。
- **shirita-tauri 单测**：
  - 数据目录/Config 路径构造（给定 base dir 推出 `shirita.db` / `assets` 路径）。
  - **启动 smoke**（纯 Rust、不依赖 webview）：构造 AppState、`bind 127.0.0.1:0`、`spawn` 跑 `app_with_cors`，`GET /health` 得 200，正确 Bearer 不 401、错 token 401，`OPTIONS`+`Origin: tauri://localhost` 得 CORS 头。
  - **优雅关闭**：`token.cancel()` 后 server graceful shutdown 返回、`pool.close()` 后池不可再用。
- **Tauri webview** 难自动化：靠本机 `tauri dev` 手动跑通（建会话、发消息走 SSE、头像/背景 asset 加载）+ **CI 三平台 build 绿** 作为完成证据。

## 10. 风险与缓解

- **Tauri v2 与 webkit2gtk-4.1**：本机已具依赖，但首次接线可能踩 `tauri-build`/capabilities/CSP 坑 → 先以最小 `tauri dev` 跑通空壳，再接 server。
- **同进程双 tokio runtime**：Tauri v2 默认 tokio；server 用同一 runtime `spawn`，避免再起 runtime。验证 SSE 长连接在 webview 内正常。
- **跨域双闸门（CSP + CORS）**：方案 B 的 webview→localhost 是跨域，**两道闸都要过**：(a) `tauri.conf.json` `app.security.csp` 的 `connect-src` 放行 `http://127.0.0.1:*`（页面侧准入）；(b) server 侧 `CorsLayer` 许可 Tauri origin + preflight（响应侧准入，见 §5b）。任一缺失都首屏白屏。**这是方案 B 的头号风险点，列入 Plan 1 首要验证项**。
- **CI 三平台首绿**：matrix 依赖各 runner 系统库；Linux 需手装 webkit2gtk-4.1-dev 等。失败属正常迭代，非阻塞设计。

## 11. 实现计划拆分（交由 writing-plans 细化）

预计 2–3 个 plan：

1. **Plan 1 — `shirita-tauri` 骨架 + 内嵌 server + 前端注入**：workspace 接线、`tauri.conf.json` 最小可跑、`shirita-web` 加 `cors` feature + `app_with_cors`、`provider_from_env` 抽取、启动序列（数据目录/Config/migrations/bind/spawn/优雅关闭 §8.2）、`initialization_script` 注入、`client.ts` 运行时配置、**CSP+CORS 双闸门**；本机 `tauri dev` 跑通端到端；Rust 启动 smoke（含 CORS preflight + 优雅关闭）+ 前端注入单测。
2. **Plan 2 — 打包与 CI**：`bundle` 配置、图标占位、本机 `tauri build` 出 Linux 安装包、GitHub Actions 三平台矩阵（未签名）、artifacts 上传。
3.（可选）**Plan 3 — 桌面便利项 / 收尾**：错误对话框打磨、README 桌面构建说明；若 Plan 1/2 已覆盖则并入。
