# 开发环境与调试

本机起 cc-monitor 的 dev 模式、跑测试、看日志，以及几类常见问题怎么查。

生产构建与打包 → [BUILDING.md](BUILDING.md)。发版 → [RELEASING.md](RELEASING.md)。架构 → [ARCHITECTURE.md](ARCHITECTURE.md)。

---

## 前置依赖

| 工具 | 版本 | 检查 |
|---|---|---|
| Node.js | ≥ 22.18（`package.json` 的 `engines`） | `node -v` |
| Rust | stable | `rustc --version` |
| Windows：MSVC Build Tools 2022 | 含 VCTools workload | Developer PowerShell 里 `where link.exe` 找得到 MSVC 的 link |
| Windows：WebView2 Runtime | Windows 11 自带，Windows 10 [手装](https://developer.microsoft.com/microsoft-edge/webview2/) | — |
| Linux：WebKitGTK 等 | `libwebkit2gtk-4.1-dev` · `libgtk-3-dev` · `libayatana-appindicator3-dev` · `librsvg2-dev` | `pkg-config --modversion webkit2gtk-4.1` |
| e2e 另需 | `tmux` · `jq` · `Xvfb`（GUI 那几套） | — |

---

## 起 dev

```bash
npm install                                  # 一次性
bash tests/scripts/re-embed.sh --native      # 编本机原生后端与全景小程序，铺进内嵌目录
npx tauri dev                                # 在仓根跑：起 vite、编 monitor、弹 1100×800 窗口
```

- **本机后端要先铺**：monitor 的每条主路都走本机常驻后端，开发构建与发版走同一条路——`--native` 编出本机那一份，`build.rs` 把它编进 exe，运行时自释放再起。没铺就起不了本机后端（界面会明说），不存在「开发构建里先凑合」的退路。判起不起得来用 `bash tests/scripts/re-embed.sh --check-dev`（真起一趟那份字节、读它的 hello 帧）。要连远端机器，另跑一次 `bash tests/scripts/re-embed.sh` 铺 musl 那两份（要 `cargo-zigbuild`）。
- **Windows**：在 Developer PowerShell for VS 2022 里跑 `npx tauri dev`；或用 `powershell -NoProfile -File tests\scripts\run.ps1 dev`，它用 `vswhere.exe` 找 MSVC、注入环境后再跑（子命令 `dev` · `build` · `check` · `clean`，见 [`tests/scripts/README.md`](../../tests/scripts/README.md)）。没注入 vcvars 时 `link.exe` 会找到 Git Bash 带的同名工具，编译挂在链接阶段。
- 别从一个 Claude Code 会话里的 shell 起 dev 却指望它继承的环境干净：monitor 启动最先清掉嵌套会话标记（见下文「常见问题」）。

### dev 模式的行为

- **自动打开 DevTools**：debug 构建在 setup 里 `open_devtools()`；设 `CCM_NO_DEVTOOLS=1` 不开（远程实测 / e2e 时省半屏）。生产构建没有 DevTools。
- **HMR**：改 `src/frontend/ui/` 下的 TS / CSS 由 vite 热更新；改 Rust 会增量重编并重启 monitor。
- **主窗整页重载**：主窗入口遇到任何 TS 热更新都 `location.reload()`，不做局部替换——长跑时旧 listener 与新代码并存会让消息重复、重放状态错乱。

---

## 跑测试

### Rust

```bash
cd src/frontend/shell
cargo test --workspace                         # monitor ＋ 共享 crate，与 CI 的 rust job 逐字相同
cargo test -p monitor --lib <过滤串>            # 只跑名字里带这个串的
cargo test -p monitor --lib <过滤串> -- --nocapture
cargo fmt --all --check
cargo clippy --workspace --all-targets

cd src/backend                                 # 后端不是 workspace 成员，单独跑
cargo test
cargo fmt --check
cargo clippy --all-targets

cd src/panorama-engine                         # 全景小程序也自成一份
cargo test
```

三处要分别跑：在 `src/frontend/shell` 里跑的任何 cargo 命令都覆不到后端与全景小程序（理由见 [ARCHITECTURE.md § 2.7](ARCHITECTURE.md#27-共享-crate-与-workspace)）。每条 cargo 命令前带 `CARGO_BUILD_JOBS=4` 可以压住并行编译的内存；跑会碰 tmux 的测试前摘掉 `TMUX` / `TMUX_PANE`（`env -u TMUX -u TMUX_PANE …`），测试起的 tmux 与后端一律隔离，不碰用户那一个。

### 前端

| 层 | 跑法 | 覆盖 |
|---|---|---|
| node 纯函数套件（`tests/**/*.test.ts`） | `npm test` 的前段（每套一个 `test:*` 脚本，`tsx` 跑） | diff · branching · 报错卡 · bash 卡 · 格式化 · 历史缓存等纯逻辑。清单的单一事实源是 `tests/frontend/ui/node-suite-registry-guard.vitest.ts` 的 `NODE_SUITES` |
| vitest ＋ jsdom（`tests/**/*.vitest.ts`） | `npm run test:dom`；单跑一份 `npx vitest run <文件>`；覆盖率 `npm run coverage` | DOM · 生命周期 · 通道替身协作：tab 门控与物化、账本、估高、设置面板、全景视图等 |
| 类型 | `npx tsc --noEmit`；`npm run check:types`（重生成 Rust 导出的类型并要求 `src/frontend/ui/generated/` 无差异） | 全部源码与测试文件 |
| lint | `npm run lint` · `npm run lint:css` | CI 里只作参考；eslint 基线由 `tests/frontend/ui/eslint-baseline.vitest.ts` 钉着 |

`npm test` ＝ 全部 node 套件 ＋ vitest，即 CI frontend job 里的单测那一步；覆盖率的逐文件地板另跑 `node tests/scripts/assert-coverage-floors.mjs`。各套测试的条数以实跑为准，这里不抄。

### e2e

- CI 跑的那几套在 `.github/workflows/ci.yml` 里，每套一行 `bash tests/e2e/assert-pass-floor.sh <套件> <断言数地板>`；本机跑得动的那几套以 `shared_crate_registry_tests.rs` 的 `LOCALLY_RUNNABLE` 为准（例：`npm run test:ccm-cli`）。
- 要 GUI 的两套：`npm run test:f40`（滚动 / 渲染管线）与 `npm run test:graylight`，前置是 Xvfb 上跑着一个 `npx tauri dev`，见 [`tests/e2e/README.md`](../../tests/e2e/README.md)。⚠ `test:f40` 会往 `~/.claude/` 写 fixture，受限环境别跑。
- WebView2（生产）的滚动行为没有自动化覆盖：动过滚动锚定的改动，发版前在 Windows 真机按 `tests/e2e/README.md` 的「人工场景」复核。

### 门禁

- `npm run gate`（＝ `bash tests/scripts/gate.sh`）：出货前的唯一闸门，逐格跑 cargo（三处）· npm · 各类判据，末尾只吐一行 `GATE: OK` / `GATE: FAIL …`。先跑它、看见 OK，再单独提交。
- `bash tests/scripts/verify-committed-state.sh`：从提交状态（不是工作树）编一次。本仓不 push，CI 见不到本机的提交，这道门只能在本机跑。

### 这些数不抄在文档里

- 主题 token 有哪些：以 `src/frontend/ui/theme.ts` 的 `TOKENS` 为准。
- 设置面板每页有哪几个折叠分组：家在 `tests/frontend/ui/settings/panel-groups.vitest.ts`（逐页完整相等断言）。
- CI 有哪些 job：`.github/workflows/ci.yml` 本身。

---

## 日志

- **dev**：跑 `npx tauri dev` 的那个终端里有 vite 日志、cargo 进度与 monitor 自己的 `tracing` 输出（默认 INFO）。调级别：

  ```bash
  RUST_LOG=debug npx tauri dev
  RUST_LOG="monitor=debug,tauri=warn" npx tauri dev
  ```

- **生产**：release 版没有 stdout，日志写到 `<monitor 数据目录>/logs/monitor/monitor.YYYY-MM-DD.log`，按天滚动；设置 → 诊断里可以不重启地改级别、打开日志，ERROR 级会弹提示。
- **后端**：本机后端的 stderr 接进 monitor 的日志；脱离起的后端写 `<monitor 数据目录>/logs/backend/`，设置页的「日志」经通道读那台后端的尾部。

---

## 端口冲突

dev 端口默认 24174。启动报：

```
Error: listen EACCES: permission denied ::1:24174
```

是 Windows 的 Hyper-V / WSL2 / WinNAT 把一段段端口列进了动态保留范围：netstat 看不到占用进程，但 listen 失败；保留段重启后会重排，所以「昨天能跑今天不行」很正常。24174 选在保留段（通常约 1000–12500）之上、ephemeral 段（49152 起）之下，最不容易被占。

确认与看保留段：

```powershell
netsh int ipv4 show excludedportrange protocol=tcp
```

临时换端口（不改任何提交的文件）：

```powershell
$env:VITE_PORT = "24500"          # vite 端口；HMR 端口自动是 VITE_PORT + 1
'{ "build": { "devUrl": "http://localhost:24500" } }' | Set-Content -Encoding utf8 "$env:TEMP\ccm-dev.json"
npx tauri dev --config "$env:TEMP\ccm-dev.json"
```

根治：重启电脑，或管理员 PowerShell 里 `net stop winnat; net start winnat`（会影响 Docker / WSL 网络）。

---

## 调试

### 看 IPC

DevTools 的 Network 看不到 Tauri IPC。界面对后端的请求都经通道（`chan.call` / `chan.subscribe`）；会话流的异常（读不懂的格 · 那台看不见了 · 流关了）由 `events.ts` 以 `[events]` 前缀打在 console 里。Rust 一侧加 `tracing::info!`，dev 终端里看。

### capability 报错

报 `Permission xxx not allowed`：

1. 看 `src/frontend/shell/gen/schemas/acl-manifests.json`（cargo build 之后生成）里那个 plugin 的 permission set 实际内容；
2. 看 `src/frontend/shell/capabilities/default.json` 现在授了哪些；
3. 通常要加一条带 scope 的 inline permission，做法见 [CONTRIBUTING.md § 改 Tauri capability](CONTRIBUTING.md#改-tauri-capability)。

---

## 常见问题

### tab 不出现

1. 跑 `claude` 之后，`~/.claude/sessions/` 里应当多一个 `<PID>.json`；没有就是 claude 自己没起来。
2. 有文件但没 tab：DevTools console 里找 `[events] 会话流 …` 那几行（看不见 / 关了）；再看机器页本机后端是不是已连上。

### 从 Claude Code 会话里起的 dev monitor，resume 出的会话不落盘

Claude Code 给它 shell 里起的子进程注入 `CLAUDECODE=1` / `CLAUDE_CODE_CHILD_SESSION=1` 等嵌套会话标记；子进程默认继承环境，经 monitor 起的 `claude --resume` 拿到这些标记会判定自己是嵌套子会话，不注册 pidfile、不写 jsonl。monitor 在 `lib.rs::run` 最前面（任何线程起来之前）清掉这组标记（保留 `CLAUDE_CONFIG_DIR`），日志留一行 `scrubbed inherited claude nested-session env markers: …`。

边界：它只管 monitor 自己起的进程链。Windows Terminal 设成「附着到已有窗口」时，新 tab 的 shell 继承的是那个 WT 进程的环境；那个 WT 若本身是从 claude 会话里起的，resume 出的 claude 照样带着标记。判别法：`sessions/` 里没有那个 claude 的 `<PID>.json`，jsonl 的修改时间冻结，而同一条 `claude --resume` 在自己开的终端里正常。

### `cc` 集成握手不成功（Windows）

- `~/.cc-monitor/ps-await/<PID>.json` 写了又被删 ⇒ monitor 收到了；写了没被删 ⇒ monitor 没看到或解析失败。
- dev 终端里找 `bind: parse … failed`。握手顺序与时序见 [IPC-PROTOCOL.md § 跨进程握手时序图](IPC-PROTOCOL.md)。

### 会话内容在底部整段重复

记录文件被截短重写时，后端先发 `session_file_reread`、行号从 0 重数，前端收到后那个 tab 整份重来（`TabManager.onRecordFileReread`），入口只按 seq 去重。复现：对一个正在被盯的 jsonl 手动截短再追加回去，后端日志出 `jsonl truncated … full re-read`，tab 内容不应翻倍。行投递是至少一次（INVARIANTS §25）。

### 大段消息被误折成「已被 ESC 回退」

重复的 uuid 会毒化分支拓扑的计数，让折叠信号全错。四道防线：`computeMainBranch` 入口按 uuid 去重；`BranchFolder` 拒重；console 出现 `[branching] Kahn leftover` warn（带嫌疑 uuid）说明遇到了新形态的异常输入；后端日志的 `jsonl truncated … full re-read` 说明发生过截断重投。回归用 `npm run test:branching`。

### 启动重放时最新消息上下微抖

查三道防线有没有被破坏：`stream.ts` 的 `snap()` 还是守卫式（落后底部超过 1px 才贴）；`.stream` 没被加上 `overflow-anchor: none`；`TabManager.onLine` 对尾块之前的旧记录仍只收进 `TailWindow`、不建卡（INVARIANTS §21）。`scrollTop` 本身单调增长，只测它发现不了抖动——要测可见元素 `getBoundingClientRect().top` 的逐帧方向反转。

### 字体 / 图片 / KaTeX 后载导致贴底跟随失灵

`stream.ts` 的 `MessageStream` 用 `ResizeObserver` ＋ 守卫式 `snap()`：内容后载长高时若仍贴底就跟到底部。失灵时在 DevTools 看 ResizeObserver 回调有没有触发、`snap()` 的守卫条件是否满足。

### 设置里的提示框不显示或错位

DevTools Elements 看 `.settings-info-tooltip` 是否挂在 `<body>` 末尾，inline 的 `left` / `top` 是否在视口内。为什么挂 body 见 [ARCHITECTURE.md § 5](ARCHITECTURE.md#5-关键设计选择与理由)。
