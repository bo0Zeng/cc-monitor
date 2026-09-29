# 贡献者操作手册

给 cc-monitor 添砖加瓦前先读这份。包含：
- **§ 1 撤回 / 修改 checklist** — 删功能时必跑的项
- **§ 2 添加新东西 cookbook** — 加 IPC / jsonl 类型 / 设置项 / 快捷键 / 跨进程协议的食谱
- **§ 3 发版 / CHANGELOG 规范** — 详见 [RELEASING.md](RELEASING.md)

不变量清单 → [INVARIANTS.md](INVARIANTS.md)。架构总览 → [ARCHITECTURE.md](ARCHITECTURE.md)。

---

## 1. 撤回 / 修改 checklist

### 1.1 撤回某个特性 / IPC 命令 / 跨进程协议

**全套必做**：

```bash
# 假设撤掉 BindRegistry State + aliases_read IPC（它是 BindRegistry 今天在命令面上的消费者之一）
cd src/bridge

# 1. State 消费者全 grep
grep -rn 'State<.*Arc<BindRegistry>>' src/
grep -rn 'BindRegistry' src/

# 2. app.manage 调用全 grep
grep -rn 'app.manage(bind_registry' src/

# 3. IPC handler 注册全 grep
grep -rn 'aliases_read' src/

# 4. 前端 invoke 依赖全 grep
cd .. && grep -rn 'invoke<.*"aliases_read"' src/

# 5. 跨进程文件 IO 全 grep（如果撤的是文件协议）
grep -rn 'ps-await\|ps-registry' src/bridge/src/ src/

# 6. 删完跑：
cd src/bridge && cargo check && cargo test --workspace
cd .. && npm run build

# !! cargo check 不能挡 State 漏 manage 的运行时 panic !!
# 必须额外起 dev mode 实测每个会消费 X 的 IPC 命令的前端入口
powershell -NoProfile -File scripts\run.ps1 dev
```

**为什么不能光靠 cargo check**：详 [STATE-MATRIX.md § 4.1](STATE-MATRIX.md#41-撤回某个-state-类型如删-bindregistry) — 漏 `manage` 是运行时 panic，类型系统抓不住。

### 1.2 修改跨进程协议（ps-await / ps-registry / sid-hwnd-cache / auto-launch）

修改 [IPC-PROTOCOL.md](IPC-PROTOCOL.md) 定义的任一文件 schema 都要：

- [ ] 改 **写入方** 代码（PS 端模板 `src/shared/cc.ps1.tpl` 或 Rust 端 `bind.rs` / `profile_installer.rs`）
- [ ] 改 **读取方** 代码（serde struct）
- [ ] 更新 [IPC-PROTOCOL.md](IPC-PROTOCOL.md) 字段定义
- [ ] **向后兼容性**：旧文件应能被新版本读取（serde `#[serde(default)]` 字段新增 OK，删字段需 RFC）
- [ ] 编码必须 UTF-8 无 BOM（[INVARIANTS § 3](INVARIANTS.md#3-所有跨进程-json-文件--utf-8-无-bom)）
- [ ] 双端原子写

### 1.3 修改 jsonl 解析（后端 `agents/claudecode/schema.rs` / `parse.rs` · 前端 `cards/index.ts`）

- [ ] 后端 `JsonlRecord` enum 加 variant 用 `#[serde(rename)]` + `#[serde(default)]`
- [ ] `JsonlRecord::is_displayable()` 决定是否显示
- [ ] 前端 `cards/index.ts` `renderMessage` dispatch 表加分支
- [ ] 测试覆盖至少一个真实样本（放 `tests/backend/agents/claudecode/parse_tests.rs`）

### 1.4 改 Tauri capability / permission

- [ ] 改 `src/bridge/capabilities/default.json`
- [ ] cargo build 后看 `src/bridge/gen/schemas/acl-manifests.json` 实际 permission set 内容确认
- [ ] dev mode 实测涉及的 IPC 不报 `Permission xxx not allowed`

**警示**：plugin 的 `<plugin>:default` permission set 通常**不包含所有** `allow-*`；某些 allow 默认空 scope 需要 inline 给 path/url pattern。详 [DEVELOPMENT.md § 查 capability 报错](DEVELOPMENT.md#查-capability-报错)。

### 1.5 发版前

- [ ] 改 **版本号三处对齐**（必做）：
  - `package.json::version`
  - `src/bridge/Cargo.toml::[package].version`
  - `src/bridge/tauri.conf.json::version`
- [ ] `Cargo.lock` 提交（Rust 应用必须锁版本）
- [ ] 若改过后端：`src/backend/lib.rs::BUILD_ID` 已 bump（手工标签非哈希！）+ 内嵌二进制一致（tag 发版 CI 自动重编；本地打包须先重编 —— 🔴 **`K-R70`（09-12）起不再需要「同步 `.build_id` 清单」那一步**，身份跟着字节走）
      > **这条 2026-08-01 起是机器强制的**，不再靠自觉：`src/bridge/build.rs` 在「内嵌二进制的
      > **字节里问不出身份戳**」「字节自报的身份 ≠ 源码 `BUILD_ID`」「抠不到源码 `BUILD_ID`」三种情况
      > 直接 **panic 掉编译**（原来只有一条比 mtime 的 warning，漏掉了真实发生过的半 bump）。
      > 三条都以「`src/bridge/embedded-backends/` 里真有二进制」为前提；该目录不存在（干净 clone / CI 常态）
      > 时是优雅降级，那一档由 `ssh_source_stream_flag_gate_tests.rs::embedded_build_id_single_source_wired` 兜。
      > 详见 [REMOTE-PHASE0-DEPLOY.md § 发版构建](REMOTE-PHASE0-DEPLOY.md#发版构建交叉编译--内嵌-backend-二进制f08b)。
- [ ] [CHANGELOG.md](../../CHANGELOG.md) 加新版本段（写法见 [RELEASING.md](RELEASING.md)）
- [ ] `cargo fmt --all --check + cargo check + cargo test --workspace + npm run build` 全绿
      （`.github/workflows/ci.yml` 第一步就是 `cargo fmt --check` 严格 verify；
      本地写完代码先 `cargo fmt` 一次再发版，避免 tag 推完才发现 CI 红需要补
      style commit 的尴尬。v2.0.0 就踩过这个坑）
- [ ] **手测关键 UI 入口**：
  - [ ] 启动 monitor，Tab 自动出现
  - [ ] 点 Tab ↗ 拉对应终端窗口
  - [ ] \` 同上（v2.x 起单键，快捷键表见根 README）
  - [ ] **H 历史浏览器打开**
  - [ ] , 设置面板打开，PowerShell 集成区扫描出 profile
  - [ ] 设置面板 hover 各个 `?` 图标，tooltip 在 viewport 内可见
  - [ ] 设置面板 [打开 profile]，资源管理器或编辑器弹出
  - [ ] /resume 一个历史会话
  - [ ] 用 cc 跑 claude，PowerShell 端到端，看 ps-await → ps-registry → sid-hwnd-cache
  - [ ] 装 cc 集成到一个**有自定义内容的 profile**，确认用户原内容保留 + 生成 `.ccm-backup-<ts>` 备份

### 1.6 Git 操作

- [ ] `git commit -m "release: vX.Y.Z"`（**不加 Claude coauthor**）
- [ ] `git tag vX.Y.Z`
- [ ] `git push origin main && git push origin vX.Y.Z`
- [ ] release.yml CI 跑过（约 6-8 min），产 NSIS + MSI + monitor.exe + SHA256SUMS

---

## 2. 添加新东西 cookbook

每个食谱给"动哪些文件 + 测试什么 + 检查清单"。

### 2.1 添加新 IPC 命令

**目标**：加一个 `monitor_get_active_ids() -> Vec<String>` 返回当前活跃 session id 列表。

**步骤**：

1. **后端** `src/bridge/src/lib.rs`（或独立 module 如 `stats.rs`）：

```rust
#[tauri::command]
async fn monitor_get_active_ids() -> Result<Vec<String>, String> {
    // 本机活会话表（本机后端帧喂的，进程级一张）提供哪些公开 API 见 src/bridge/src/session_map.rs。
    Ok(session_map::local()
        .read()
        .snapshot_active()
        .into_iter()
        .map(|a| a.session_id)
        .collect())
}
```

⚠️ **示例是说明性的**，落地前必须 `cargo check`。本机活会话表（`session_map::LocalTable`）的公开方法见 `documentSymbol src/bridge/src/session_map.rs` 或 `pub fn` grep。

2. **注册到 invoke_handler**（`lib.rs::run()` 内）：

```rust
.invoke_handler(tauri::generate_handler![
    /* ... 现有命令 ... */
    monitor_get_active_ids,    // ← 加这行
])
```

3. **如果用了 State**：去 [STATE-MATRIX.md § 2](STATE-MATRIX.md#2-消费者矩阵ipc-命令) 对应 State 下加一行 `lib.rs::monitor_get_active_ids(...)`。

4. **前端调用**：

```ts
const activeIds = await invoke<string[]>("monitor_get_active_ids");
```

5. **检查**：
- [ ] `cargo check + cargo test --workspace`
- [ ] `npm run build` TS 编译过
- [ ] dev mode 实测命令真的能从前端 invoke 到（State 漏 manage 才能挡住）
- [ ] [STATE-MATRIX.md § 2](STATE-MATRIX.md) 表已更新

### 2.2 添加新 jsonl 记录类型

**目标**：claude 后续新加 `type=memory_recall` 记录，cc-monitor 要解析 + 渲染。

**步骤**：

1. **后端** `src/backend/agents/claudecode/schema.rs::JsonlRecord` enum 加 variant（〔MOD〕记录解释只住后端，monitor 只转交成品）：

```rust
#[serde(rename = "memory_recall")]
MemoryRecall {
    uuid: String,
    timestamp: String,
    #[serde(default)]
    content: serde_json::Value,
    #[serde(rename = "sessionId", default)]
    session_id: Option<String>,
},
```

2. **决定是否 displayable**：`impl JsonlRecord::is_displayable()` 加 match arm 返回 true（如果要渲染）或 false（如果只是 metadata 不显示）。

   > ⚠️ **若新类型带 `uuid`+`parentUuid`（参与 parent 链）**：`is_displayable()` **必须**返回 true，且**必须同时**加进前端 `branching.ts::extractBranchRecord` 白名单 + `cards/index.ts` 的 `JsonlRecord` 镜像。否则前端 parent 链断在这条记录处 → 它的后续消息被误判孤儿 root → **整段被错误折叠为「已被 ESC 回退」**（`branching.ts:24` 预警、2026-06-13 咬过一次、F63 补的正是这条）。若只是不带链身份的会话级 metadata（如 `mode`/`pr-link`），可返回 false 不进链——但记住 F63 起未知 type 一律被 `parse_line` 抢救成 `Unrecognized` 保底，**别退回静默丢弃**（见 INVARIANTS § 18.1）。

3. **`parse.rs` 测试**（`tests/backend/agents/claudecode/parse_tests.rs`）：加一行真实样本断言能 parse 成功；`npm run gen:types` 重生成 `src/generated/JsonlRecord.ts`。

4. **前端类型** `src/cards/index.ts` 或对应 type 文件加 TS 类型 + dispatch：

```ts
// 在 renderMessage 的 switch 里
case "memory_recall":
  return renderMemoryRecall(record);
```

5. **写渲染逻辑**：通常新建 `src/cards/memory-recall.ts` 包装成折叠卡 / 普通卡。

6. **检查**：
- [ ] `cargo test --lib parser` 通过
- [ ] 跑一个含该新类型的 jsonl 文件 → 前端能正常显示

### 2.3 添加新跨进程协议文件

如想加 `cc-monitor 状态心跳` 文件给 PS 端查 monitor 是否在跑：

详细步骤 → [IPC-PROTOCOL.md § 添加新的跨进程协议文件](IPC-PROTOCOL.md#添加新的跨进程协议文件)。

**关键**：
- 路径必须在 `~/.claude/work/` 下
- UTF-8 无 BOM
- 原子写
- 反序列化容错（`#[serde(default)]` + `#[serde(other)]`）
- **必须声明它是「真相」还是「缓存」**（INVARIANTS § 2.1）：在 `data_paths.rs` 的枚举里登记 + description 写清。真相（用户手写/意图，删了丢东西）要格式迁移友好；缓存（能重建）允许随手删。**新 data dir 文件一律先答这一问**——身份类 id 另见 § 28。

### 2.4 添加新外观设置项

**目标**：在设置面板"颜色"分组下加一个 `--info` token。

**步骤**：

1. **CSS** `src/styles.css::root` 加 `--info: #6699cc;` 默认值 + 引用处替换字面量。
2. **TS 类型** `src/theme.ts::ThemeConfig` interface 加 `"info"?: string`。
3. **`TOKENS` 数组**加 `{ key: "info", category: "color" }`。
4. **设置面板** `src/settings/panel.ts::FIELDS` 加 `{ key: "info", label: "信息色", type: "color", group: "color" }`。
5. **检查**：
- [ ] 设置面板能看到新 token 字段
- [ ] 拖 color picker 实时预览生效
- [ ] 关闭 monitor 后重启，颜色保留

### 2.5 添加新全局快捷键

> issue #5 起所有 chord 走 `src/keybindings/` 的 dispatch table（`actions.ts` = 单一真相源 + `registry.ts` = dispatcher）。**别再往 `main.ts` 加 keydown case**（旧写法，已废弃）。

**目标**（真实例：F84 命令栏 `Ctrl+K`）：

1. **`src/keybindings/actions.ts`** 的 `ACTIONS` 加一行（id / label / category / default chord / available）：

```ts
{ id: "app.open-command-bar", label: "打开命令栏（命令面板）", category: "App", default: "Ctrl+KeyK", available: true },
```
（chord 规范：`normalizeChord` 用 `KeyboardEvent.code`、modifier 固定序 `Ctrl+Shift+Alt+Meta+<code>`；预留未上线的 action 设 `available: false` + `comingSoon` 文案。）

2. **`src/main.ts`** `dispatcher.bind("<id>", cb)`：

```ts
dispatcher.bind("app.open-command-bar", () => commandBar.toggle());
```

3. **冲突检查**：`whoOwns("Ctrl+KeyK")` 应返回 null（`ACTIONS` 里 grep `default:` 确认无占用）；单键 chord 在可编辑目标聚焦时自动失效（`registry.ts::isEditableTarget`，除 `overlay.close`）。

4. 编辑器 UI + 持久化 schema 自动从 `ACTIONS` 收敛，无需别处改；用户可在「设置 → 快捷键」改绑。

### 2.6 添加新 Tauri capability permission

**目标**：用 plugin-X 的某个 IPC，capability 没默认授权。

**步骤**：

1. **cargo build 一次** 让 `src/bridge/gen/schemas/acl-manifests.json` 重新生成。
2. **看 plugin-X 的 `permissions`** 找具体 `allow-foo` 的定义，看 description 是否需要 scope。
3. **`capabilities/default.json` 加 permission**：

简单（无 scope）：
```json
{ "permissions": [ "plugin-x:allow-foo", ... ] }
```

带 scope（多见，default 通常空 scope）：
```json
{ "permissions": [
  { "identifier": "plugin-x:allow-foo", "allow": [{ "scope_field": "pattern" }] },
  ...
] }
```

4. **dev mode 实测**：调用涉及 IPC 不报 `Permission xxx not allowed`。

### 2.7 改远端会话后端命令（tmux attach / new / send-keys）

> ⚠⚠ **G3 订正（2026-08-04）：本节的「阶段②」已经是现在时了。**
> 下面第 4 步把「取命令方式转后端 RPC」写成**未来动作**，而 **F04b（kill）与 F04c（send-keys）
> 已经把生产主路切到后端 RPC**：当年是 monitor 的两个发送端（杀会话 · 送键，分流判定在 `backend_route.rs`，三态而非二态）；
> 〔C4e · 第四波 4C〕两个发送端连同 Tauri 命令迁到界面，今天是 `src/tmux-control.ts` 经通道直接说后端的 `kill` / `launch`。
>
> 🔴 **订正二（`K-R106` 2026-09-13 现打）：这一段原来那两句今天都假了。**
> 原文逐字是「`src/session-backend.ts` 那条 shell 串**已降级为 C7 过渡期**的第二条路
> （`tmux_backend_gate_guard.rs` 反过来钉着**它必须还在**）」——
> ① `C7` 那条一次性 SSH 的第二条路 **`K-R72`（2026-09-12）整块删了**（`K-R54` 裁定表第 1 · 2 处）；
> ② `tmux_backend_gate_guard.rs` 今天钉的是**反向**（回潮闸）：那两条命令的生产段里
> **再出现** `connect_and_exec_cmd` 就红。两次翻面方向相反，别读成只改了措辞。
> ③ 而且 `session-backend.ts` 今天**与 kill / send-keys 无关** ——
> 它只有 `createRunAttach` / `attach` / `runInExistingAttach` 三个方法。
>
> ⇒ **今天要改 kill / send-keys 的行为，盘上只有 Rust 那一条路可改。**
> ⚠ `attach` / `new-session` 那两条**没有整条搬完**，而它今天要**分两半**读
> —— 所以本节不是整节作废，是「按命令分叉」：
>
> | 命令 | 今天的主路 | 改哪里 |
> |---|---|---|
> | `kill` | 后端 RPC（F04b；〔C4e〕界面经通道直接说） | `src/tmux-control.ts::killSession`（门在后端 `src/backend/control/gate.rs`）；**盘上没有第二条路** |
> | `send-keys` | 后端 RPC（F04c；〔C4e〕界面经通道直接说） | `src/tmux-control.ts::sendKeys`（两个 mode 名的理由在它头注里）；同上 |
> | `attach`（**本机**） | 🔴 **本机后端**〔`K-R106` 2026-09-13，用户逐字「归本机后端就好了啊」〕 | 〔MIG-2〕本机后端 `local.rs::plan` 的接回那一格（帧命令 `launch-local`，原 Tauri 命令 `render_local_attach`）⇒ `ccm -- --attach <名>`；前端 `runLocalResumeIntoExistingTmux` 问它要 |
> | `attach` / `new-session`（**远端兜底**） | 🔴 **后端渲染器**〔步 22b·B 2026-09-20，`设计/90 §4 E` 收官〕 | `src/backend/control/launch_render/payload.rs::render_tmux_outer`（外层三格）＋ `render_payload`（内层载荷），同一条帧命令 `launch-render-payload`（〔MIG-2〕原 tauri 命令 `render_launch_payload` 退役，界面经通道问那台后端）（`outer` 缺席 = `container:"none"`，带 `outer` = tmux 那三格）。**改完必须改用例表的手写期望并重生成入库夹具**：`npm run gen:payload-golden`。〔LR2 2026-09-25〕TS 那份（`session-backend.ts` ＋ `launch-render-fallback.ts`）已删，这是唯一一份 |
>
> 🔴🔴 **订正三（步 22b·B 2026-09-20）：下面那四条「步骤」整段过期了，别照着做。**
> 它们写的是「改命令语法 → 只改 `src/session-backend.ts`」，而那条路
> **2026-09-20 起零生产调用** —— 照着改只会改到两份入库夹具的**左边**，
> 而生产上跑的那一串一个字节都不会变（然后夹具对拍当场红，告诉你改错了地方）。
>
> **今天要改「在远端起/接会话」的命令，按这四步：**
>
> 1. **改命令语法** → `src/backend/control/launch_render/payload.rs`（〔MIG-2〕搬进后端）
>    （`render_tmux_outer` 外层三格 · `render_payload` 内层载荷）。
>    ⚠ **不许单开模块**：`设计/00 §2.5 ④` 要的是「5 个渲染实现 → 2 个」，
>    单开一个就让盘上从 5 变 6，方向是反的（`the_launch_renderers_on_disk_are_exactly_these` 钉着）。
> 2. **改用例表里的手写期望**（`tests/test-support/launch-payload-golden.ts` / `tests/test-support/launch-tmux-outer-golden.ts` 的 `payload` / `cmd`）——
>    〔LR2 2026-09-25〕原来这一步是「跟着改 TS 那一份」（`session-backend.ts` ＋ `launch-render-fallback.ts`），
>    那一族零生产调用、按 `设计/00 §2.5 ④` 删了，夹具左边换成手写期望。期望与 Rust 产出不同步 ⇒ 夹具对拍红。
> 3. **重生成入库夹具** → `npm run gen:payload-golden`
>    （产 `payload-golden.json` 与 `tmux-outer-golden.json` 两份）。
>    ⚠ **不重生成会红，那是设计**：`tests/launch-tmux-outer-golden.vitest.ts` 断「入库的 == 现场渲染的」。
> 4. **回归** → `npx vitest run tests/remote-launch-run.vitest.ts`（含 `W22B` 那五条生产接线判据）
>    ＋ `cargo test -p monitor --lib launch_tmux_outer_parity`。
>
> ⚠ **§31 最终形态第①条一个字没松**：前端仍然绝不硬编码后端命令字面量
>（〔LR2〕`tests/launch-no-shell-in-ts.vitest.ts`，`设计/90 §3` 条 1：`src/**/*.ts` 生产段零 `tmux <动词> -` / `&&` 字面量、不开例外）。变的是「问谁要」——
> 从「问前端座要」变成「问后端要」，那正是第①条括号里写的**阶段②**。


**目标**：改「在远端起/接会话」的命令（resume/launcher/attach）。守 **INVARIANTS §31（SS-12）**：
前端**绝不硬编码后端命令字面量**，命令语法一律问后端要（`render_launch_payload` / `render_ccm_launch`）。

〔以下四步 **2026-09-20 起留档**，〔LR2 2026-09-25〕起**整段作废** —— 它们说的座（`src/session-backend.ts`）
与它的判据（`tests/session-backend-gate.vitest.ts` · `tests/session-backend.test.ts`）都删了。今天的版本在上面那块「订正三」里。〕

**步骤**：

1. **改命令语法** → 只改 `src/session-backend.ts`（`TMUX_BACKEND` 方法）；`remote-launch.ts` 只负责校验/转义/载荷/编排。
2. **grep 门禁**（remote-launch 正文不许出现可执行的 tmux 命令字面量，只许 doc 注释命中）：
```bash
grep -nE "tmux (new-session|send-keys|attach)" src/remote-launch.ts   # 命中的必须全是 ` * ` 注释行
```
3. **保形回归** → `node tests/remote-launch.test.ts`（改命令串则同步更新其逐串断言）+ `node tests/session-backend.test.ts`。
4. **加后端**（阶段②，后端在场）：先过 §31 最终形态第②③条——**abduco/dtach 没有 send-keys，取命令方式转后端 RPC**，不是往座里再加一个返回 shell 串的 const（见 `session-backend.ts` 顶注）。

---

## 3. PR 流程

1. fork → branch（命名 `feat/<short-desc>` / `fix/<short-desc>`）
2. 改代码 + 测试 + 文档（参照本文档对应 cookbook）
3. `cargo fmt + cargo clippy + cargo test --workspace + cargo test -p code-picture-core〔在 src/panorama-engine 里跑〕 + npm test + npm run coverage + npm run build` 全绿。
   ⚠ **`--all` 只是 `--workspace` 的弃用别名**（audit-0805 F18 订正）。〔TL1 · 4C 拍板 ③〕RM1f 起 monitor 不再依赖 vendor `code-picture-core`（链它的只剩 `src/panorama-engine`）⇒ 它不再是 `src/bridge` workspace 的成员，这条 `--exclude` 在那里只剩一条 cargo warning（`excluded package(s) not found`）⇒ 删了；裸 `--workspace` 现打就是那 9 个成员（`monitor` ＋ 8 个共享 crate）。vendor 再被拉回来会让成员数变 10：monitor 清单零 vendor 依赖那条判据（`shared_crate_registry`）与门禁 `cargo` 那一格的包数相等当场红。vendor 自测要到 `src/panorama-engine` 里跑（`ci.yml` 那一步同）。
   ⚠ **各项条数与 CI job 数刻意不写在这里**：那些数在仓里曾有 4-5 份拷贝、全部漂成假的。
   分工照旧：`npm test` = node 纯函数 + vitest DOM = **前端那个 CI job**；本机后端 / 远端后端 / e2e 冒烟是**各自独立的 job**，`npm test` 不含它们；动滚动/渲染管线另跑 `tests/e2e/f40-suite.sh`（见 tests/e2e/README.md）
4. PR 描述：
   - 解决什么问题（链到 issue）
   - 怎么解决（一句话）
   - 涉及的文件 + 变动量
   - 手测过哪些路径
5. 提 PR → 等 CI → review → merge

CHANGELOG / 版本号 / tag 由 maintainer 在 release 时统一处理，PR 不需要碰这些。
