# M8 Plan 2 — 打包与 CI（三平台未签名）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 本机构建出可运行的 Linux 桌面安装包，并配好 GitHub Actions 三平台矩阵产出 **未签名** `.AppImage`/`.deb`/`.dmg`/`.msi` artifacts。

**Architecture:** 复用 Plan 1 的 `shirita-tauri` crate 与 `tauri.conf.json`；本机仅验证 Linux；Win/macOS 由 CI runner（`tauri-apps/tauri-action`）构建。永不签名 → 零证书、零 secrets。

**Tech Stack:** Tauri v2 bundler、GitHub Actions、`tauri-apps/tauri-action`。前置：Plan 1 已完成（crate 可 `cargo tauri build`）。参见 spec §6。

---

## 文件结构

- `shirita-tauri/tauri.conf.json` — **修改**：确认 `bundle` 配置完整（Plan 1 已含 targets/icon，本计划核对并按需补全 `category`/`shortDescription`）。
- `.github/workflows/desktop.yml` — **新建**：三平台矩阵 build + 上传 artifacts。
- `README.md` — **修改**：新增「桌面构建」一节。

---

## Task 1：本机构建 Linux 安装包

**Files:**
- Modify: `shirita-tauri/tauri.conf.json`（按需补全 bundle 元数据）

- [ ] **Step 1: 核对 bundle 元数据**

确认 `shirita-tauri/tauri.conf.json` 的 `bundle` 段含：

```json
  "bundle": {
    "active": true,
    "targets": ["appimage", "deb"],
    "category": "Utility",
    "shortDescription": "Shirita desktop",
    "longDescription": "Shirita — a local AI chat desktop app.",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico",
      "icons/icon.png"
    ]
  }
```

（`targets` 在本机仅出 Linux；CI 各 runner 用 `--bundles` 覆盖或由 tauri-action 自动选当平台目标——见 Task 2。）

- [ ] **Step 2: 本机构建**

Run: `cargo tauri build --bundles appimage,deb`
Expected: 构建成功，产物位于 `target/release/bundle/appimage/*.AppImage` 与 `target/release/bundle/deb/*.deb`。

- [ ] **Step 3: 冒烟运行 AppImage（可选，若本机 GUI 可用）**

Run: `./target/release/bundle/appimage/*.AppImage`
Expected: 弹出 Shirita 窗口、不白屏。若 worker 无法目视，提示用户验证；构建成功本身即满足本任务的自动化判据。

- [ ] **Step 4: Commit（仅当 conf 有改动）**

```bash
git add shirita-tauri/tauri.conf.json
git commit -m "$(printf 'build(tauri): bundle metadata for Linux packaging (appimage/deb)\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

> 若 Step 1 未改动文件（Plan 1 已完整），跳过本步。

---

## Task 2：GitHub Actions 三平台矩阵（未签名）

**Files:**
- Create: `.github/workflows/desktop.yml`

- [ ] **Step 1: 写 workflow**

创建 `.github/workflows/desktop.yml`：

```yaml
name: desktop-build

on:
  workflow_dispatch: {}
  push:
    tags:
      - "v*"

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: ubuntu-latest
          - platform: macos-latest
          - platform: windows-latest
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v4

      - name: Install Linux deps (webkit2gtk 4.1)
        if: matrix.platform == 'ubuntu-latest'
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libwebkit2gtk-4.1-dev \
            libgtk-3-dev \
            libsoup-3.0-dev \
            librsvg2-dev \
            libayatana-appindicator3-dev \
            patchelf

      - uses: dtolnay/rust-toolchain@stable

      - uses: actions/setup-node@v4
        with:
          node-version: 20

      - name: Install frontend deps
        run: npm ci
        working-directory: shirita-ui

      - name: Build frontend
        run: npm run build
        working-directory: shirita-ui

      - name: Tauri build (unsigned)
        uses: tauri-apps/tauri-action@v0
        with:
          projectPath: shirita-tauri
        env:
          # 永不签名：不注入任何证书 secret。

      - name: Upload installers
        uses: actions/upload-artifact@v4
        with:
          name: shirita-${{ matrix.platform }}
          path: |
            target/release/bundle/appimage/*.AppImage
            target/release/bundle/deb/*.deb
            target/release/bundle/dmg/*.dmg
            target/release/bundle/msi/*.msi
          if-no-files-found: ignore
```

> 说明：
> - 触发限定 `workflow_dispatch` + tag `v*`，避免每次 push 跑重 CI（spec §6.2）。
> - `tauri-action` 自动在各 runner 上构建该平台对应 bundle（macOS→dmg、Windows→msi、Linux→appimage/deb）；`projectPath` 指向含 `tauri.conf.json` 的 crate。
> - `beforeBuildCommand` 已在 `tauri.conf.json` 设为构建前端，但 workflow 仍显式 `npm ci`/`npm run build` 以确保 `node_modules` 与 `dist` 就绪（tauri-action 在干净 runner 上需要）。
> - 未注入签名 secret → 产物未签名（接受）。

- [ ] **Step 2: 本地静态校验 YAML**

Run: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/desktop.yml')); print('yaml ok')"`
Expected: 输出 `yaml ok`（无语法错）。

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/desktop.yml
git commit -m "$(printf 'ci(desktop): GitHub Actions matrix builds unsigned 3-platform installers\n\nworkflow_dispatch + v* tags. ubuntu/macos/windows runners via tauri-action;\nuploads .AppImage/.deb/.dmg/.msi artifacts. No signing secrets.\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

- [ ] **Step 4: 触发并核验 CI（推送后）**

推送分支/打 tag 后，到 GitHub Actions 手动 `Run workflow`（`workflow_dispatch`）。
Expected: 三个 job 均绿；artifacts 含三平台安装包。**Win/macOS 产物本机不验证，CI 绿 + artifact 存在即为完成证据**（spec §1 完成标志）。若某 runner 因系统库/版本失败，按报错迭代（spec §10 视为正常迭代）。

---

## Task 3：README 桌面构建说明

**Files:**
- Modify: `README.md`（若不存在则新建）

- [ ] **Step 1: 追加「桌面（Tauri）」一节**

在 `README.md` 末尾追加：

```markdown
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
cargo tauri dev          # 起 vite dev + 桌面窗口
```

### 构建安装包

```bash
cargo tauri build --bundles appimage,deb   # 本机 Linux
```

Windows(.msi) / macOS(.dmg) 由 GitHub Actions（`.github/workflows/desktop.yml`，
`workflow_dispatch` 或 `v*` tag 触发）构建，产物为 **未签名** 安装包——macOS 需
右键「打开」绕过 Gatekeeper，Windows 会有 SmartScreen 警告。

### provider 配置（桌面）

当前桌面版仍读环境变量：`PROVIDER`（`anthropic`/`ollama`/留空=OpenAI 兼容）、
`OPENAI_API_KEY`/`OPENAI_BASE_URL`/`OPENAI_MODEL` 等。未设 key 时使用离线 Echo。
应用内 provider 配置为后续小项。
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "$(printf 'docs(readme): desktop (Tauri) build + run instructions\n\nCo-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>')"
```

---

## Task 4：全量回归收尾

- [ ] **Step 1: 后端 + 前端全绿**

Run: `cargo test --workspace && cd shirita-ui && npx vue-tsc --noEmit && npx vitest run && npx vite build`
Expected: 全绿、零警告。

- [ ] **Step 2: 零警告构建**

Run: `cargo build --workspace`
Expected: 无 warning、无 error。

> 通过后，M8 实现完成 → 走 `superpowers:finishing-a-development-branch` 决定合并方式（分支 `m8-tauri-desktop`）。CI 三平台绿由 Task 2 Step 4 在远端核验。

---

## Self-Review 记录

- **Spec 覆盖**：打包 §6.1（Task 1）、CI §6.2（Task 2）、本机仅验证 Linux §6.3（Task 1/2）、README/收尾（Task 3/4）。传输/CORS/优雅关闭/注入由 Plan 1 覆盖。
- **占位符**：无 TBD；workflow、conf、README 均为完整内容。
- **类型一致**：bundle targets 与 upload-artifact 路径一致（appimage/deb/dmg/msi）；`projectPath: shirita-tauri` 与 crate 位置一致。
- **已知风险**：`tauri-action@v0` 与 Tauri v2 的兼容、各 runner 系统库版本——首绿可能需迭代（spec §10，非阻塞）。
