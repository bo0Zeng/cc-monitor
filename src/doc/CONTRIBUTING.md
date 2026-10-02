# 贡献者手册

给 cc-monitor 加东西、撤东西之前先读这份：先认清一件事该放在哪一层，再照对应的做法动手；漏了哪一步，多半有一条判据会红并点名。

架构 → [ARCHITECTURE.md](ARCHITECTURE.md)。不变量 → [INVARIANTS.md](INVARIANTS.md)。开发环境与测试 → [DEVELOPMENT.md](DEVELOPMENT.md)。发版 → [RELEASING.md](RELEASING.md)。

---

## 1. 动手前先答的几问

- **它是判定还是排版？** 判定（口径 · 命令串 · 全会话事实 · 这一发走哪）一律在后端；前端只把后端给的成品排版成像素、收手势。
- **本机与远端是不是同一条路？** 本机就是不走 ssh 的远端：同一个后端、同一条帧命令，只差 `origin`。别为本机单写一条。
- **后端不在时怎么办？** 不在就说清楚、不发生，不许「问不到就自己干一遍」（那会让同一件事有两份实现，写操作还会有两道宽窄不同的围栏）。
- **这个判定 / 这个数有没有第二个家？** 一个判定只许一个家，一个数只许一个住址；要两侧共用的只放契约（`src/common/` 的共享 crate），不放判定。
- **会不会写用户的文件？** 会 ⇒ 只经那台后端的文件管理面，并在足迹里看得见。

---

## 2. 加东西

### 2.1 加一条后端帧命令（界面要一件新成品）

界面要的新东西几乎都是这一形：那台后端算好，界面 `call(origin, op, …)` 拿成品。

1. **本体**放进它该在的那一层：产出观测的读进 `observe/`，改状态、或只喂控制决策的查询进 `control/`，写用户文件经 `control/files_write` 那一族；入参出参用结构，不拼 shell 串。
2. **登记**进 `src/backend/stream/inbound.rs`：名单 `COMMANDS` 一行 ＋ `REGISTRY` 一条 `CommandSpec`（名字 · 协议文档锚点 · 错误码 · 输出字段 · 有没有入参 · 跑法）。两边对不上由 `inbound_structure_guards.rs::the_commands_mirror_matches_the_registry` 报。只读查询的帧面宿主住 `faces/`。
3. **协议文档**：[IPC-PROTOCOL.md](IPC-PROTOCOL.md) 里给它一节 `` #### `命令名` ``，写载荷与答话的形状；带载荷的命令没有这一节会红（`inbound_structure_guards.rs::a_command_with_a_payload_must_own_a_doc_section`）。
4. **CLI 面**：帧命令默认派生同名的一次性子命令；只在流面上有意义的（读本进程里的 watcher、中转或转发账）登记进 `src/backend/control/cli_control.rs` 的 `STREAM_ONLY` 并写理由。子命令表一变 `build_id_guard` 就红 ⇒ bump `src/backend/lib.rs` 的 `BUILD_ID` 并重铺内嵌字节（[BUILDING.md § 内嵌字节与 BUILD_ID](BUILDING.md#内嵌字节与-build_id)）。
5. **界面**：`chan.call(origin, "命令名", 载荷, 期限)`，期限由发起那件事的一方给一个绝对时刻。vitest 里用 `tests/test-support/chan-fake.ts` 的通道替身。
6. **检查**：后端 `cargo test`（在 `src/backend`）· `npm test` · `npx tsc --noEmit`。

### 2.2 加一条 Tauri 命令（只限 monitor 自己的事）

只有 monitor 自己的事才开 Tauri 命令：窗口 · 拉前 · 本机 monitor 配置 · 日志与数据位置 · 重放缓冲 · 本机后端起停与引导 · 通道本身 · 放字节 · 足迹里 monitor 自己的事实。碰后端的一律走 §2.1。

1. `#[tauri::command]` 写在它那一域的模块里（窗口类进 `lib.rs`，开终端进 `launch.rs` …）；要调 Win32 同步 API 的走 `spawn_blocking`。
2. 注册进 `lib.rs::run` 里的 `generate_handler!`。
3. 登记进 `tests/frontend/shell/command_home_registry_tests.rs` 的 `MONITOR_OWN`（命令名 · 类 · 理由）；没登记、或实现碰到了后端，都会红。
4. 前端只经 `src/frontend/ui/ipc/commands.ts` 的包装调它（全仓只有这一份直接调 `invoke`，`tests/frontend/ui/ipc/commands.vitest.ts` 对拍名字与键名）。
5. 用了 `State<…>` ⇒ 在 [STATE-MATRIX.md § 2](STATE-MATRIX.md#2-消费者矩阵ipc-命令) 对应 State 下加一行。
6. **检查**：`cargo test --workspace`（在 `src/frontend/shell`）· `npx tsc --noEmit` · dev 模式里真点一次那个入口——State 漏了 `manage` 是运行时 panic，`cargo check` 抓不住。

### 2.3 加一种 jsonl 记录类型

以 claude 新加一种 `type=memory_recall` 记录为例。

1. 后端 `src/backend/agents/claudecode/schema.rs` 的 `JsonlRecord` 加变体（`#[serde(rename = "memory_recall")]`，字段一律 `#[serde(default)]`）。记录解释只住后端，界面只收成品。
2. `is_displayable()` 决定它进不进渲染。⚠ 带 `uuid` ＋ `parentUuid`（参与 parent 链）的必须返回 true，并且同时进前端 `src/frontend/ui/branching.ts` 的 `extractBranchRecord` 白名单——否则 parent 链断在这条记录上，它后面的消息被判成孤儿 root，整段误折成「已被 ESC 回退」。只是会话级元数据、不带链身份的可以返回 false。未知 type 由 `parse_line` 抢救成 `Unrecognized`，别退回静默丢弃（INVARIANTS §18.1）。
3. 在 `tests/backend/agents/claudecode/parse_tests.rs` 加一条能解析成功的样本。样本只采结构、不采真会话正文。
4. `npm run gen:types` 重生成 `src/frontend/ui/generated/JsonlRecord.ts`（`npm run check:types` 会查它与 Rust 一致）。
5. 前端 `src/frontend/ui/cards/index.ts` 的 `renderMessage` 加分支，卡片本身写在 `src/frontend/ui/cards/` 下。
6. **检查**：后端 `cargo test`（在 `src/backend`）· `npm run test:dom` · 拿一份含这种记录的 jsonl 看显示与折叠。

### 2.4 加一个外观设置项

以在「颜色」分组下加一个 `--info` token 为例。

1. `src/frontend/ui/styles.css` 的 `:root` 加 `--info` 的默认值，引用处换成变量。
2. `src/frontend/ui/theme.ts`：`ThemeConfig` 加 `"info"?: string`，`TOKENS` 加一行（`key` · `cssVar`，数值类带 `unit`）。
3. `src/frontend/ui/settings/panel.ts` 的 `FIELDS` 加一行（`type: "color"`，`group: "color"`），标签走文案表 `copyText("settingsPanel.field.…")`（见 §2.8）。
4. **检查**：设置里看得到这一项、拖色板实时预览、重启后保留。

### 2.5 加一个快捷键

所有快捷键走 `src/frontend/ui/keybindings/` 的派发表：`actions.ts` 是单一事实源，`registry.ts` 是派发器。别往 `main.ts` 里加 keydown 分支。

1. `actions.ts` 的 `ACTIONS` 加一行，例（命令栏 `Ctrl+K`）：

   ```ts
   { id: "app.open-command-bar", label: copyText("keybindingActions.app.commandBar"), category: "App", default: "Ctrl+KeyK", available: true },
   ```

   chord 按 `registry.ts::normalizeChord` 的规范写：用 `KeyboardEvent.code`，修饰键固定顺序 `Ctrl+Shift+Alt+Meta+<code>`。预留还没上线的动作设 `available: false`。
2. `main.ts` 里 `dispatcher.bind("<id>", cb)`。
3. 冲突检查：`ACTIONS` 里没有别的动作占着同一个 `default`（编辑器里 `whoOwns` 会拒）；单键 chord 在可编辑目标聚焦时自动失效（`isEditableTarget`）。
4. 编辑器与持久化自动从 `ACTIONS` 收敛，用户可以在「设置 → 快捷键」改绑。

### 2.6 改 Tauri capability

用某个 plugin 的 IPC 而 capability 没授权时：

1. cargo build 一次，让 `src/frontend/shell/gen/schemas/acl-manifests.json` 重新生成；
2. 在里面找那个 plugin 的 `allow-*`，看它是否需要 scope；
3. 改 `src/frontend/shell/capabilities/default.json`：

   ```json
   { "permissions": [ "plugin-x:allow-foo" ] }
   ```

   带 scope 的（多数 `default` 是空 scope）：

   ```json
   { "permissions": [ { "identifier": "plugin-x:allow-foo", "allow": [{ "path": "…" }] } ] }
   ```

4. dev 模式实测涉及的调用不再报 `Permission xxx not allowed`。

⚠ plugin 的 `<plugin>:default` 通常不含全部 `allow-*`。独立窗口要关窗得有 `core:window:allow-close`（`core:window:default` 不含），capability 的 `windows` 要列进 `settings` 与 `viewer-*`。

### 2.7 改「起 / 接会话」的命令串

每一条起会话路径交给终端的都只是一行 `ccm [交给 agent 的…] -- [ccm 自己的…]`：那一行只住后端 `src/backend/control/launch_render/ccm_invocation.rs`（远端经帧命令 `launch-render-cli`，本机经 `launch-local`）。环境、中转地址、身份标记、预信任由那台机器上的 `ccm`（`src/backend/control/ccm/`）在最终 exec 那一处定，别在别处渲。外层容器（建 tmux 会话 / 键进已有 pane）只包这一行。前端零命令串字面量（`tests/frontend/ui/launch-no-shell-in-ts.vitest.ts`：生产段零 `tmux <动词> -` / `&&`）；后端生产段除 `ccm` 自己的最终 exec 外零处渲直接起 agent 的命令（`ccm_tests.rs::nothing_but_ccm_renders_a_command_that_starts_an_agent`）。

1. 改后端那一份渲染（缺的选项先在 `ccm` 里加：`control/ccm/argv.rs` 那张表）；
2. 改用例表里的手写期望：`tests/test-support/launch-cli-golden.ts`；
3. `npm run gen:cli-golden` 重生成入库夹具 `src/backend/control/launch_render/fixtures/cli-golden.json` —— 不重生成会红，那是设计：`launch_cli_parity_tests.rs` 断「Rust 渲染的 == 入库的」，`every_monitor_launch_path_hands_over_one_ccm_line` 断每条路径都以 `ccm ` 开头；
4. 回归：`npx vitest run tests/frontend/ui/remote-launch-run.vitest.ts tests/frontend/ui/remote-launch.test.ts` ＋ 后端 `cargo test --lib launch_render ccm`（在 `src/backend`）＋ 改了 `ccm` 的行为就跑 `tests/e2e/restart-suite.sh` · `resume-suite.sh` · `tmux-target-acceptance.sh`（单跑前缀 `env -u CLAUDE_CONFIG_DIR -u TMUX -u TMUX_PANE`；这几套用真后端二进制当 `ccm`，先 `cargo build`）。

### 2.8 界面文字

- 新写的界面文字走文案表：TS 用 `src/frontend/ui/copy-table.ts` 的 `copyText(key)`，Rust 用 `copy_text`，文案住 `src/shared/copy/table.json`。只说现在是什么、用户能做什么，不讲演进、不露内部名，一句能说清不写两句。对外拼法是「API key」。
- 改了或加了任何界面 / 报错文字 ⇒ `python3 tests/evidence/CP1-copy-verdicts.py --json` 必须 `ok:true`，缺的在 `tests/evidence/CP1-copy-verdicts.tsv` 补一行；新词要在 `src/shared/copy/terms.json` 给出处置（`tests/copy/copy-terms.vitest.ts`）。

### 2.9 加跨进程文件

做法见 [IPC-PROTOCOL.md § 添加新的跨进程协议文件](IPC-PROTOCOL.md#添加新的跨进程协议文件)。要点：放在 `~/.cc-monitor/` 下 · UTF-8 无 BOM · 原子写（临时件 ＋ 改名） · 反序列化容错（`#[serde(default)]`） · 先答它是「真相」还是「缓存」，在 `data_paths.rs` 登记并写清（INVARIANTS §2.1）。后端自己的状态文件另有写者登记（`readonly_guard.rs` 的 `OWN_STATE_WRITERS`）。

---

## 3. 撤东西 / 改名

### 3.1 撤一条 Tauri 命令或一个 State

以撤掉 `BindRegistry` 与它的消费者之一 `bound_terminal_count` 为例：

```bash
cd src/frontend/shell
grep -rn 'State<.*BindRegistry>' src/                        # State 的全部消费者
grep -rn 'app.manage(bind_registry' src/lib.rs                # manage 调用
grep -rln 'bound_terminal_count' src/ ../ui/ ../../../tests/  # 注册 · 包装 · 调用处 · 测试
cargo test --workspace
```

再同拍改 `command_home_registry_tests.rs` 的 `MONITOR_OWN`、`src/frontend/ui/ipc/commands.ts` 的包装与 [STATE-MATRIX.md](STATE-MATRIX.md)。漏 `manage` 是运行时 panic，删完要在 dev 模式里把每个会消费它的入口真点一次（[STATE-MATRIX.md § 4.1](STATE-MATRIX.md#41-撤回某个-state-类型如删-bindregistry)）。

### 3.2 撤一条后端帧命令

从 `COMMANDS` 与 `REGISTRY` 里摘掉、删 IPC-PROTOCOL.md 那一节、处理 CLI 面（派生的子命令跟着没了，登记过的从 `STREAM_ONLY` 摘）、删界面的调用处与通道替身里的那一格，bump `BUILD_ID`。

### 3.3 改跨进程文件的格式（`ps-await` · `ps-registry` · `sid-hwnd-cache` · `auto-launch`）

- 写入方（PS 模板 `src/shared/cc.ps1.tpl`，或 Rust 的 `bind.rs` 等）与读取方（serde 结构）同拍改；
- 更新 [IPC-PROTOCOL.md](IPC-PROTOCOL.md) 的字段定义；
- 编码 UTF-8 无 BOM（[INVARIANTS § 3](INVARIANTS.md#3-所有跨进程-json-文件--utf-8-无-bom)），双端原子写；
- 新增字段 `#[serde(default)]`。老 profile 里的 PS 模板不会自动更新，monitor 那一侧找窗口的重试就是留给它们的。

### 3.4 删掉或改名一个符号，而散文还提着它

- 散文（文档与注释）点符号写成 `文件.rs::函数名`，不写 `文件:行号`；`doc/` 与源码里的这种地址各有判据核它还在不在。
- 删 / 改名之后，把提到它的每一句话改成现状：说今天是什么、在哪儿。不写「它为什么不在了」，不挂墓碑标记，也不登记。
- 已有的墓碑标记（字面见 `structural_scan.rs` 的 `PROSE_NAME_TOMBSTONE`）冻结，只许减：改到一句挂着墓碑的话，就把它改成现状、去掉标记，在 `tests/frontend/shell/structural_scan_tests.rs` 的 `REGISTERED`（按文件计数）与 `TOMBSTONED` 里减掉那一格，并把同一处的冻结数往下改。两张表加行或加数会红。

---

## 4. 提交与 PR

1. fork → 分支（`feat/<简述>` / `fix/<简述>`）。
2. 改代码 ＋ 测试 ＋ 文档（照本篇对应的做法）。测试夹具只采结构，不放真会话正文；令牌与钥匙不进日志、不走 argv / env。
3. 本机跑绿再提：两处 cargo（壳 workspace · 后端）的 fmt / clippy / test、`npm test`、`npx tsc --noEmit`、`npm run build`；`npm run gate` 一趟跑完 cargo test、fmt、`npm test` 与各类判据，看见 `GATE: OK` 再提交。跑法见 [DEVELOPMENT.md](DEVELOPMENT.md)，CI 有哪些 job 以 `.github/workflows/ci.yml` 为准。
4. PR 描述写：解决什么问题（链 issue）· 怎么解决（一句话）· 动了哪些文件 · 手测过哪些路径。
5. 提 PR → 等 CI → review → merge。

CHANGELOG、版本号与 tag 由维护者发版时统一处理，PR 不用碰。
