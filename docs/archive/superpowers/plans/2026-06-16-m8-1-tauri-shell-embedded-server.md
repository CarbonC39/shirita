# M8 Plan 1 — Tauri 外壳 + 内嵌 server + CORS/优雅关闭 + 前端注入

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让既有 Vue 前端在一个原生 Tauri v2 桌面壳里运行，后端复用进程内嵌的 `shirita_web` 路由（127.0.0.1 随机端口），端到端跑通对话/SSE/头像。

**Architecture:** 方案 B（内嵌 Axum）。`shirita-tauri` bin 在 `setup` 里算数据目录→建 Config（随机 token）→连 SqliteStorage+迁移+seed→按 env 建 provider→`bind 127.0.0.1:0`→`tokio::spawn` 跑 `app_with_cors` 并挂 graceful shutdown→建带 `initialization_script` 的窗口注入 `window.__SHIRITA_RUNTIME__={base,token}`。前端 `client.ts` 优先读注入值。后端仅两处**附加**：CORS 层、优雅关闭。

**Tech Stack:** Rust（Axum 0.8、tower-http cors、tokio-util CancellationToken、sqlx）、Tauri v2、Vue 3 + Vitest。参见 spec `docs/superpowers/specs/2026-06-16-m8-tauri-desktop-design.md`。

---

## 文件结构

- `shirita-core/src/config.rs` — **修改**：抽出 `pub fn apply_provider_env(&mut Config)`，`from_env` 改为调用它（DRY，供 tauri bin 复用）。
- `shirita-web/src/provider_select.rs` — **新建**：`ProviderKind` + `provider_kind()`（纯决策、可测）+ `provider_from_env(&Config) -> Arc<dyn ModelProvider>`（移自 main.rs）。
- `shirita-web/src/lib.rs` — **修改**：`pub mod provider_select;` + 再导出；新增 `pub fn app_with_cors(state) -> Router`。
- `shirita-web/src/main.rs` — **修改**：provider 选择改调 `provider_from_env`。
- `shirita-web/Cargo.toml` — **修改**：`tower-http` 加 `cors` feature；dev-deps 加 `tokio-util`、`reqwest` 不需要（见 Task 3）。
- `shirita-web/tests/desktop_server_test.rs` — **新建**：CORS preflight/header 测试 + 内嵌 server 优雅关闭 smoke。
- `Cargo.toml`（workspace）— **修改**：`members` 加 `"shirita-tauri"`。
- `shirita-tauri/{Cargo.toml,build.rs,tauri.conf.json,icons/,src/main.rs}` — **新建**：crate 骨架 + `data_paths` 纯函数 + 完整启动接线。
- `shirita-ui/src/api/client.ts` — **修改**：BASE/TOKEN 优先读 `window.__SHIRITA_RUNTIME__`。
- `shirita-ui/src/api/client.test.ts` — **修改**：加「注入值优先」单测。

---

## Task 1：抽出 provider-from-env（core `apply_provider_env` + web `provider_kind`/`provider_from_env`）

**Files:**
- Modify: `shirita-core/src/config.rs`
- Create: `shirita-web/src/provider_select.rs`
- Modify: `shirita-web/src/lib.rs`、`shirita-web/src/main.rs`
- Test: `shirita-core/src/config.rs`（`#[cfg(test)]`）、`shirita-web/src/provider_select.rs`（`#[cfg(test)]`）

- [ ] **Step 1: 写失败测试（core apply_provider_env）**

在 `shirita-core/src/config.rs` 末尾的 `#[cfg(test)] mod tests` 内追加（若无 tests 模块则新建）：

```rust
    #[test]
    fn apply_provider_env_overlays_openai_fields() {
        // SAFETY: 单线程测试内设置/清理 env。
        std::env::set_var("OPENAI_BASE_URL", "http://x/v1");
        std::env::set_var("OPENAI_MODEL", "m-test");
        let mut cfg = Config::new("db", "assets", "tok").unwrap();
        apply_provider_env(&mut cfg);
        assert_eq!(cfg.openai_base_url, "http://x/v1");
        assert_eq!(cfg.openai_model, "m-test");
        std::env::remove_var("OPENAI_BASE_URL");
        std::env::remove_var("OPENAI_MODEL");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p shirita-core apply_provider_env_overlays_openai_fields`
Expected: 编译失败 `cannot find function apply_provider_env`。

- [ ] **Step 3: 实现 `apply_provider_env` 并让 `from_env` 复用**

在 `shirita-core/src/config.rs` 的 `impl Config` 之外（模块级）加：

```rust
/// 把 provider 相关 env（OPENAI_BASE_URL/OPENAI_API_KEY/OPENAI_MODEL）叠加到 cfg。
/// 供 `from_env` 与桌面（Tauri）入口共享，避免重复。
pub fn apply_provider_env(cfg: &mut Config) {
    if let Ok(v) = std::env::var("OPENAI_BASE_URL") {
        cfg.openai_base_url = v;
    }
    cfg.openai_api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
    if let Ok(v) = std::env::var("OPENAI_MODEL") {
        cfg.openai_model = v;
    }
}
```

然后把 `from_env` 里原本内联的这三段替换为 `apply_provider_env(&mut cfg);`（保持行为不变）。确认 `apply_provider_env` 已 `pub` 并在 `lib.rs` 通过 `pub use config::*` 或显式导出（检查 `shirita-core/src/lib.rs` 现有 `Config` 的导出方式，按同样方式加 `apply_provider_env`）。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p shirita-core apply_provider_env_overlays_openai_fields`
Expected: PASS。

- [ ] **Step 5: 写失败测试（web provider_kind）**

新建 `shirita-web/src/provider_select.rs`：

```rust
use std::sync::Arc;

use shirita_core::{AnthropicProvider, Config, EchoProvider, ModelProvider, OpenAiProvider};

/// provider 选择的纯决策结果（便于单测）。
#[derive(Debug, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    Ollama,
    OpenAi,
    Echo,
}

/// 由 `PROVIDER` env 值与 api_key 是否为空，决定适配器种类。纯函数。
pub fn provider_kind(provider_env: &str, api_key_empty: bool) -> ProviderKind {
    match provider_env {
        "anthropic" => ProviderKind::Anthropic,
        "ollama" => ProviderKind::Ollama,
        _ => {
            if api_key_empty {
                ProviderKind::Echo
            } else {
                ProviderKind::OpenAi
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_maps_env_and_key() {
        assert_eq!(provider_kind("anthropic", false), ProviderKind::Anthropic);
        assert_eq!(provider_kind("ollama", true), ProviderKind::Ollama);
        assert_eq!(provider_kind("", false), ProviderKind::OpenAi);
        assert_eq!(provider_kind("", true), ProviderKind::Echo);
        assert_eq!(provider_kind("unknown", true), ProviderKind::Echo);
    }
}
```

- [ ] **Step 6: 跑测试确认失败**

Run: `cargo test -p shirita-web provider_kind_maps_env_and_key`
Expected: 编译失败（模块未在 lib.rs 声明 / 未链接）。

- [ ] **Step 7: 实现 `provider_from_env` 并接线 lib.rs / main.rs**

在 `shirita-web/src/provider_select.rs` 追加（`tests` 模块之上）：

```rust
/// 按 env 构造 provider（移自 main.rs）。读取 PROVIDER 及对应 base url env。
pub fn provider_from_env(config: &Config) -> Arc<dyn ModelProvider> {
    let provider_env = std::env::var("PROVIDER").unwrap_or_default();
    match provider_kind(&provider_env, config.openai_api_key.is_empty()) {
        ProviderKind::Anthropic => {
            let base = std::env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com".into());
            tracing::info!("using AnthropicProvider at {base}");
            Arc::new(AnthropicProvider::new(base, config.openai_api_key.clone()))
        }
        ProviderKind::Ollama => {
            let base = std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434/v1".into());
            tracing::info!("using Ollama (OpenAI-compatible) at {base}");
            Arc::new(OpenAiProvider::new(base, "ollama"))
        }
        ProviderKind::OpenAi => {
            tracing::info!("using OpenAiProvider at {}", config.openai_base_url);
            Arc::new(OpenAiProvider::new(
                config.openai_base_url.clone(),
                config.openai_api_key.clone(),
            ))
        }
        ProviderKind::Echo => {
            tracing::info!("OPENAI_API_KEY empty: using offline EchoProvider");
            Arc::new(EchoProvider)
        }
    }
}
```

在 `shirita-web/src/lib.rs` 顶部模块声明区加 `pub mod provider_select;`，并在 `pub use` 区加 `pub use provider_select::{provider_from_env, provider_kind, ProviderKind};`。

在 `shirita-web/src/main.rs`：删除原先整段 `let provider: Arc<dyn ModelProvider> = match std::env::var("PROVIDER")... { ... };`，替换为：

```rust
    let provider = shirita_web::provider_from_env(&config);
```

并移除 main.rs 中因此不再使用的 imports（`AnthropicProvider`、`EchoProvider`、`OpenAiProvider`、`ModelProvider` 若仅用于该 match）。`model` 仍取 `config.openai_model.clone()`，顺序保持「build config → provider_from_env(&config) → Arc::new(config)」。

- [ ] **Step 8: 跑测试 + 全量编译确认零警告**

Run: `cargo test -p shirita-web provider_kind_maps_env_and_key && cargo build --workspace`
Expected: 测试 PASS；`cargo build` 无 warning、无 error。

- [ ] **Step 9: Commit**

```bash
git add shirita-core/src/config.rs shirita-web/src/provider_select.rs shirita-web/src/lib.rs shirita-web/src/main.rs
git commit -m "$(printf 'refactor(web): extract provider_from_env + core apply_provider_env\n\nProvider selection moved out of main.rs into shirita_web::provider_select\n(provider_kind pure decision + provider_from_env), reusable by the Tauri bin.\nConfig.apply_provider_env shares the env overlay with from_env.\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 2：`app_with_cors` + tower-http cors

**Files:**
- Modify: `shirita-web/Cargo.toml`、`shirita-web/src/lib.rs`
- Test: `shirita-web/tests/desktop_server_test.rs`（新建）

- [ ] **Step 1: 写失败测试（CORS preflight + 真实请求带 CORS 头）**

新建 `shirita-web/tests/desktop_server_test.rs`：

```rust
//! 桌面内嵌 server：CORS（preflight 绕过鉴权）+ 优雅关闭 smoke。

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

use shirita_core::{Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter};
use shirita_web::{app_with_cors, AppState, Generations};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_path_buf();
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(base.join("desk.db").to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let assets = base.join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    let config = Arc::new(Config::new("ignored", assets.to_str().unwrap(), "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "m".into(), generations: Arc::new(Generations::new()) }
}

#[tokio::test]
async fn cors_preflight_bypasses_auth_and_allows_tauri_origin() {
    let req = Request::builder()
        .method("OPTIONS")
        .uri("/api/ping")
        .header(header::ORIGIN, "tauri://localhost")
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "authorization")
        .body(Body::empty())
        .unwrap();
    let res = app_with_cors(test_state().await).oneshot(req).await.unwrap();
    // preflight 不带 Authorization，却不能 401——必须被 CorsLayer 短路应答。
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "tauri://localhost"
    );
}

#[tokio::test]
async fn real_request_carries_cors_header() {
    let req = Request::builder()
        .method("GET")
        .uri("/api/ping")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::ORIGIN, "tauri://localhost")
        .body(Body::empty())
        .unwrap();
    let res = app_with_cors(test_state().await).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "tauri://localhost"
    );
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p shirita-web --test desktop_server_test`
Expected: 编译失败 `cannot find function app_with_cors`。

- [ ] **Step 3: 加 cors feature**

编辑 `shirita-web/Cargo.toml`，把
`tower-http = { version = "0.6", features = ["trace", "fs"] }`
改为
`tower-http = { version = "0.6", features = ["trace", "fs", "cors"] }`。

- [ ] **Step 4: 实现 `app_with_cors`**

在 `shirita-web/src/lib.rs` 顶部 imports 区加：

```rust
use axum::http::{header, Method};
use tower_http::cors::{AllowOrigin, CorsLayer};
```

在 `pub fn app(...)` 之后加：

```rust
/// 桌面（内嵌 server）专用：在 `app()` 外层套 CORS，放行 Tauri webview origin。
/// CorsLayer 作为最外层——preflight `OPTIONS` 由它短路应答，不进 Bearer 鉴权；
/// 真实请求穿过 CORS → auth → handler，响应回程补上 `Access-Control-Allow-Origin`。
pub fn app_with_cors(state: AppState) -> Router {
    let origins = [
        header::HeaderValue::from_static("tauri://localhost"),
        header::HeaderValue::from_static("https://tauri.localhost"),
        header::HeaderValue::from_static("http://tauri.localhost"),
    ];
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
    app(state).layer(cors)
}
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p shirita-web --test desktop_server_test`
Expected: 两个测试 PASS。

- [ ] **Step 6: Commit**

```bash
git add shirita-web/Cargo.toml shirita-web/src/lib.rs shirita-web/tests/desktop_server_test.rs
git commit -m "$(printf 'feat(web): app_with_cors — Tauri-origin CORS for embedded desktop server\n\nOutermost CorsLayer so preflight OPTIONS short-circuits before Bearer auth.\nAllows tauri://localhost (+https/http variants), Authorization/Content-Type.\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 3：内嵌 server 优雅关闭 smoke 测试

**Files:**
- Modify: `shirita-web/Cargo.toml`（dev-deps 加 `tokio-util`）
- Test: `shirita-web/tests/desktop_server_test.rs`（追加）

- [ ] **Step 1: 加 dev-dep `tokio-util`**

编辑 `shirita-web/Cargo.toml` 的 `[dev-dependencies]`，加：
`tokio-util = { version = "0.7", features = ["rt"] }`
（`CancellationToken` 在 `tokio_util::sync`，`rt`/默认 feature 即含 `sync`；若编译报缺 feature，用 `features = ["rt", "time"]` 或显式 `["sync"]`——`sync` 是默认 feature，通常无需指定。）

- [ ] **Step 2: 写测试（先失败）**

在 `shirita-web/tests/desktop_server_test.rs` 末尾追加：

```rust
#[tokio::test]
async fn embedded_server_binds_serves_and_shuts_down_gracefully() {
    use tokio_util::sync::CancellationToken;

    // 单独构造 storage 以保留 pool 句柄（模拟桌面 bin 的优雅关闭）。
    let dir = tempfile::tempdir().unwrap();
    let storage = SqliteStorage::connect(dir.path().join("smoke.db").to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let pool = storage.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let assets = dir.path().join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    let config = Arc::new(Config::new("ignored", assets.to_str().unwrap(), "secret-token").unwrap());
    let state = AppState {
        storage,
        config,
        provider: Arc::new(EchoProvider),
        token_counter: Arc::new(TiktokenCounter::new()),
        model: "m".into(),
        generations: Arc::new(Generations::new()),
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    assert!(port > 0, "OS 应分配一个真实端口");

    let token = CancellationToken::new();
    let child = token.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, app_with_cors(state))
            .with_graceful_shutdown(async move { child.cancelled().await })
            .await
    });

    // 让 server 起来，然后广播关闭。
    tokio::task::yield_now().await;
    token.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .expect("server 应在取消后及时退出")
        .expect("server task join")
        .expect("axum::serve 返回 Ok");

    // 显式关池（桌面 RunEvent::Exit 的行为）。
    pool.close().await;
    assert!(pool.is_closed());
}
```

- [ ] **Step 3: 跑测试确认失败再通过**

Run: `cargo test -p shirita-web --test desktop_server_test embedded_server_binds_serves_and_shuts_down_gracefully`
Expected: 先因缺 `tokio-util` 编译失败 → 加好 dev-dep 后 PASS（若 Step 1 已加则直接 PASS）。

- [ ] **Step 4: 全量回归**

Run: `cargo test -p shirita-web`
Expected: 既有 + 新增全绿。

- [ ] **Step 5: Commit**

```bash
git add shirita-web/Cargo.toml shirita-web/tests/desktop_server_test.rs
git commit -m "$(printf 'test(web): embedded server graceful-shutdown smoke (bind 0 + cancel + pool.close)\n\nProves the desktop embedded-server pattern: bind 127.0.0.1:0 yields a real\nport, axum with_graceful_shutdown returns on CancellationToken cancel, and\nSqlitePool.close() leaves the pool closed.\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 4：`shirita-tauri` crate 骨架 + `data_paths`

**Files:**
- Modify: `Cargo.toml`（workspace members）
- Create: `shirita-tauri/Cargo.toml`、`shirita-tauri/build.rs`、`shirita-tauri/tauri.conf.json`、`shirita-tauri/icons/*`、`shirita-tauri/src/main.rs`

- [ ] **Step 1: 安装 tauri-cli + 构建前端 dist + 生成占位图标**

Run（一次性准备 `generate_context!` 所需资产）：

```bash
cargo install tauri-cli --version "^2" --locked   # 若已装可跳过
npm --prefix shirita-ui run build                  # 产出 shirita-ui/dist
mkdir -p shirita-tauri/icons
magick -size 1024x1024 xc:'#6C5CE7' shirita-tauri/icons/source.png
cargo tauri icon shirita-tauri/icons/source.png -o shirita-tauri/icons
```

Expected: `shirita-ui/dist/index.html` 存在；`shirita-tauri/icons/` 下生成 `32x32.png`、`128x128.png`、`128x128@2x.png`、`icon.icns`、`icon.ico`、`icon.png`。

- [ ] **Step 2: workspace 接线**

编辑根 `Cargo.toml`：`members = ["shirita-core", "shirita-web", "shirita-tauri"]`。

创建 `shirita-tauri/Cargo.toml`：

```toml
[package]
name = "shirita-tauri"
version.workspace = true
edition.workspace = true

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
shirita-core = { path = "../shirita-core" }
shirita-web = { path = "../shirita-web" }
tauri = { version = "2", features = [] }
tauri-plugin-dialog = "2"
tokio = { workspace = true }
tokio-util = { version = "0.7", features = ["rt"] }
uuid = { workspace = true }
serde_json = { workspace = true }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[features]
# 默认启用 Tauri 自定义协议等运行时能力；保持最小。
default = ["custom-protocol"]
custom-protocol = ["tauri/custom-protocol"]
```

创建 `shirita-tauri/build.rs`：

```rust
fn main() {
    tauri_build::build();
}
```

- [ ] **Step 3: 最小 `tauri.conf.json`（含 CSP 放行 127.0.0.1）**

创建 `shirita-tauri/tauri.conf.json`：

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Shirita",
  "version": "0.0.0",
  "identifier": "app.shirita.desktop",
  "build": {
    "frontendDist": "../shirita-ui/dist",
    "devUrl": "http://localhost:5173",
    "beforeBuildCommand": "npm --prefix ../shirita-ui run build"
  },
  "app": {
    "windows": [],
    "security": {
      "csp": "default-src 'self'; connect-src 'self' http://127.0.0.1:* http://localhost:*; img-src 'self' http://127.0.0.1:* data:; style-src 'self' 'unsafe-inline'; script-src 'self'"
    }
  },
  "bundle": {
    "active": true,
    "targets": ["appimage", "deb"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico",
      "icons/icon.png"
    ]
  }
}
```

> 注：`app.windows` 故意留空——窗口在 Task 5 的 `setup` 里用代码创建，以便挂 `initialization_script`。

- [ ] **Step 4: 最小 `src/main.rs`（含 `data_paths` + 单测，能编译能跑空壳）**

创建 `shirita-tauri/src/main.rs`：

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};

/// 由 app data 基目录推出 (db_path, assets_dir)。纯函数，便于单测。
fn data_paths(base: &Path) -> (PathBuf, PathBuf) {
    (base.join("shirita.db"), base.join("assets"))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_paths_derives_db_and_assets() {
        let (db, assets) = data_paths(Path::new("/data"));
        assert_eq!(db, Path::new("/data/shirita.db"));
        assert_eq!(assets, Path::new("/data/assets"));
    }
}
```

> `data_paths` 暂时仅被测试使用，会触发 `dead_code` 警告——Task 5 会真正调用它。为保持「零警告」，本步在 `fn data_paths` 上加 `#[cfg_attr(not(test), allow(dead_code))]`，Task 5 移除该属性。

把上面的 `fn data_paths` 行替换为：

```rust
#[cfg_attr(not(test), allow(dead_code))]
fn data_paths(base: &Path) -> (PathBuf, PathBuf) {
```

- [ ] **Step 5: 编译 + 单测**

Run: `cargo build -p shirita-tauri && cargo test -p shirita-tauri data_paths_derives_db_and_assets`
Expected: 编译成功（webkit2gtk-4.1 已具）、测试 PASS、无 warning。

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml shirita-tauri/Cargo.toml shirita-tauri/build.rs shirita-tauri/tauri.conf.json shirita-tauri/icons shirita-tauri/src/main.rs
git commit -m "$(printf 'feat(tauri): shirita-tauri crate skeleton + data_paths helper\n\nWorkspace member, Tauri v2 conf (empty windows, CSP allowing 127.0.0.1),\nplaceholder icons, dialog plugin. data_paths pure fn unit-tested.\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 5：完整启动接线（setup 序列 + 注入 + 优雅关闭 + 错误对话框）

**Files:**
- Modify: `shirita-tauri/src/main.rs`

本任务以「`cargo build -p shirita-tauri` 通过 + 手动 `cargo tauri dev` 跑通」验证（Tauri glue 无单测）。

- [ ] **Step 1: 重写 `src/main.rs` 为完整启动序列**

把 `shirita-tauri/src/main.rs` 整体替换为：

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use shirita_core::{Config, SqliteStorage, Storage, TiktokenCounter, TokenCounter};
use shirita_web::{app_with_cors, AppState, Generations};
use sqlx::SqlitePool;
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tokio_util::sync::CancellationToken;

/// 由 app data 基目录推出 (db_path, assets_dir)。纯函数，便于单测。
fn data_paths(base: &Path) -> (PathBuf, PathBuf) {
    (base.join("shirita.db"), base.join("assets"))
}

/// 退出时优雅关闭所需的句柄（managed state）。
struct Shutdown {
    token: CancellationToken,
    pool: SqlitePool,
}

/// 启动序列里所有可失败步骤集中于此，返回人类可读错误供对话框展示。
async fn boot(base: PathBuf) -> Result<(AppState, SqlitePool, u16, CancellationToken, String), String> {
    std::fs::create_dir_all(&base).map_err(|e| format!("无法创建数据目录 {}：{e}", base.display()))?;
    let (db_path, assets_dir) = data_paths(&base);
    std::fs::create_dir_all(&assets_dir).map_err(|e| format!("无法创建资源目录：{e}"))?;

    let token_secret = uuid::Uuid::new_v4().to_string();
    let mut config = Config::new(
        db_path.to_string_lossy().to_string(),
        assets_dir.to_string_lossy().to_string(),
        &token_secret,
    )
    .map_err(|e| format!("配置错误：{e}"))?;
    shirita_core::apply_provider_env(&mut config);
    let model = config.openai_model.clone();

    let storage = SqliteStorage::connect(&config.database_path)
        .await
        .map_err(|e| format!("打开数据库失败：{e}"))?;
    storage
        .run_migrations()
        .await
        .map_err(|e| format!("数据库迁移失败：{e}"))?;
    shirita_core::ensure_default_template(&storage)
        .await
        .map_err(|e| format!("初始化默认模板失败：{e}"))?;
    let pool = storage.pool().clone();

    let provider = shirita_web::provider_from_env(&config);
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    let state = AppState {
        storage,
        config: Arc::new(config),
        provider,
        token_counter,
        model,
        generations: Arc::new(Generations::new()),
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("无法绑定本地端口：{e}\n\n请检查本地防火墙或杀毒软件的拦截记录。"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("读取本地端口失败：{e}"))?
        .port();

    let token = CancellationToken::new();
    let child = token.clone();
    let serve_state = state.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = axum::serve(listener, app_with_cors(serve_state))
            .with_graceful_shutdown(async move { child.cancelled().await })
            .await
        {
            tracing::error!("embedded server error: {e}");
        }
    });

    Ok((state, pool, port, token, token_secret))
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let base = handle
                .path()
                .app_data_dir()
                .map_err(|e| format!("无法定位数据目录：{e}"))?;

            let boot_result = tauri::async_runtime::block_on(boot(base));
            let (_state, pool, port, token, token_secret) = match boot_result {
                Ok(v) => v,
                Err(msg) => {
                    handle
                        .dialog()
                        .message(msg)
                        .kind(MessageDialogKind::Error)
                        .title("Shirita 启动失败")
                        .blocking_show();
                    std::process::exit(1);
                }
            };

            // 注入运行时配置（在页面脚本之前执行）。
            let runtime_cfg = serde_json::json!({
                "base": format!("http://127.0.0.1:{port}"),
                "token": token_secret,
            });
            let init_script = format!("window.__SHIRITA_RUNTIME__ = {runtime_cfg};");

            WebviewWindowBuilder::new(&handle, "main", WebviewUrl::default())
                .title("Shirita")
                .inner_size(1100.0, 760.0)
                .initialization_script(&init_script)
                .build()?;

            app.manage(Shutdown { token, pool });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let RunEvent::Exit = event {
            if let Some(sd) = app_handle.try_state::<Shutdown>() {
                let token = sd.token.clone();
                let pool = sd.pool.clone();
                tauri::async_runtime::block_on(async move {
                    token.cancel();
                    pool.close().await;
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_paths_derives_db_and_assets() {
        let (db, assets) = data_paths(Path::new("/data"));
        assert_eq!(db, Path::new("/data/shirita.db"));
        assert_eq!(assets, Path::new("/data/assets"));
    }
}
```

> 说明：`_state` 在 setup 中未直接使用（server 任务已 clone 走 `serve_state`），用 `_` 前缀避免 unused 警告。`sqlx` 需作为 `shirita-tauri` 依赖以命名 `SqlitePool` 类型——见 Step 2。

- [ ] **Step 2: 给 `shirita-tauri` 加 `sqlx` + `axum` 依赖**

编辑 `shirita-tauri/Cargo.toml` 的 `[dependencies]`，追加：

```toml
axum = { version = "0.8" }
sqlx = { workspace = true }
```

（`SqlitePool` 来自 `sqlx`；`axum::serve` 来自 `axum`。`sqlx` 用 workspace 版本含 sqlite/runtime-tokio。）

- [ ] **Step 3: 编译确认**

Run: `cargo build -p shirita-tauri`
Expected: 成功、无 warning。若报 `try_state`/`DialogExt`/`blocking_show` API 名差异，按 `tauri` v2 / `tauri-plugin-dialog` v2 当前签名微调（`cargo doc -p tauri --open` 或 `cargo build` 错误提示给出确切名）。

- [ ] **Step 4: 全量回归**

Run: `cargo test --workspace`
Expected: 全绿（含 Task 1–3 的 web 测试 + tauri data_paths）。

- [ ] **Step 5: Commit**

```bash
git add shirita-tauri/src/main.rs shirita-tauri/Cargo.toml
git commit -m "$(printf 'feat(tauri): full startup wiring — embedded server, runtime injection, graceful shutdown\n\nsetup hook: app_data_dir -> Config(random token) -> SqliteStorage+migrations+seed\n-> provider_from_env -> bind 127.0.0.1:0 -> spawn app_with_cors with graceful\nshutdown -> window with initialization_script injecting window.__SHIRITA_RUNTIME__.\nRunEvent::Exit cancels the token and closes the pool. Boot errors surface in a\nnative dialog; bind failure suggests checking firewall/antivirus.\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 6：前端 `client.ts` 运行时配置注入

**Files:**
- Modify: `shirita-ui/src/api/client.ts`
- Test: `shirita-ui/src/api/client.test.ts`

- [ ] **Step 1: 写失败测试（注入值优先）**

在 `shirita-ui/src/api/client.test.ts` 末尾追加：

```ts
describe('runtime config injection', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('prefers injected window.__SHIRITA_RUNTIME__ for BASE and TOKEN', async () => {
    vi.resetModules()
    vi.stubGlobal('__SHIRITA_RUNTIME__', { base: 'http://127.0.0.1:9999', token: 'inj-tok' })
    const fm = vi.fn().mockResolvedValue({ ok: true, status: 200, json: async () => [] })
    vi.stubGlobal('fetch', fm)
    const { listSessions } = await import('./client')
    await listSessions()
    expect(fm).toHaveBeenCalledWith('http://127.0.0.1:9999/api/sessions', {
      headers: { Authorization: 'Bearer inj-tok' },
    })
  })
})
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd shirita-ui && npx vitest run src/api/client.test.ts`
Expected: 新测试 FAIL（仍走 env 的 `''` base / `test-token`）。

- [ ] **Step 3: 修改 `client.ts` 顶部 BASE/TOKEN**

把 `shirita-ui/src/api/client.ts` 顶部：

```ts
const BASE = import.meta.env.VITE_API_BASE ?? ''
const TOKEN = import.meta.env.VITE_API_TOKEN ?? ''
```

替换为：

```ts
const RT = (globalThis as { __SHIRITA_RUNTIME__?: { base?: string; token?: string } }).__SHIRITA_RUNTIME__
const BASE = RT?.base ?? import.meta.env.VITE_API_BASE ?? ''
const TOKEN = RT?.token ?? import.meta.env.VITE_API_TOKEN ?? ''
```

- [ ] **Step 4: 跑测试确认通过 + 回归**

Run: `cd shirita-ui && npx vitest run src/api/client.test.ts`
Expected: 新测试 PASS，既有 importFile/SSE 等 9 条仍 PASS（`RT` undefined → env 回退）。

- [ ] **Step 5: 类型检查 + 构建**

Run: `cd shirita-ui && npx vue-tsc --noEmit && npx vite build`
Expected: 均通过。

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/api/client.ts shirita-ui/src/api/client.test.ts
git commit -m "$(printf 'feat(ui): client reads window.__SHIRITA_RUNTIME__ for BASE/TOKEN (Tauri inject)\n\nFalls back to import.meta.env when not injected, so Web build is unchanged.\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 7：手动端到端验证（`cargo tauri dev`）

**Files:** 无（验证任务，不提交，除非发现需修复）。

- [ ] **Step 1: 启动桌面开发模式**

Run: `cargo tauri dev`
（本机 `DISPLAY=:0` 可弹窗；vite dev 在 5173，Tauri 加载 devUrl。）
Expected: 弹出 Shirita 窗口，无白屏（CSP+CORS 双闸门生效）。

- [ ] **Step 2: 功能走查（人工）**

在窗口内：
1. 新建会话；
2. 发送一条消息——观察 **SSE 流式**逐字渲染（无 provider key 时 Echo 也会回流）；
3. 若已配置头像/背景，确认 **asset 图片**经 `http://127.0.0.1:<port>/assets/...` 正常加载；
4. 打开浏览器/webview 控制台确认无 CORS/CSP 报错。

- [ ] **Step 3: 关闭窗口验证优雅退出**

关闭窗口，确认进程干净退出（日志无 panic）；重开 `cargo tauri dev` 确认无 `database is locked`。

> 若本机 GUI 无法由自动化 worker 目视确认，请提示用户运行 `cargo tauri dev` 完成 Step 2–3 的人工走查；server 层行为已由 Task 2/3 自动化测试覆盖。

---

## Self-Review 记录

- **Spec 覆盖**：传输 B（Task 4/5）、CORS §5b（Task 2）、优雅关闭 §8.2（Task 3/5）、bind 失败文案 §8.1（Task 5 boot）、前端注入 §5（Task 6）、provider 暂留 env §1（Task 1）、数据目录 §7（Task 5 boot）、CSP §10（Task 4 conf）。打包/CI §6 → Plan 2。
- **占位符**：无 TBD；所有步骤含确切代码/命令/预期。
- **类型一致**：`app_with_cors`/`provider_from_env`/`apply_provider_env`/`data_paths`/`Shutdown`/`__SHIRITA_RUNTIME__` 在定义与使用处命名一致。
- **已知风险**：`tauri` v2 / `tauri-plugin-dialog` v2 的 `try_state`/`DialogExt::message`/`blocking_show`/`MessageDialogKind` 具体签名以当前 crate 版本为准（Task 5 Step 3 注明按编译错误微调）；`AllowOrigin::list` 接受 `IntoIterator<Item=HeaderValue>`。
