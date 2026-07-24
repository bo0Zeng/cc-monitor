# 账号用量：抓取 `/usage`（plan 额度）方案（设计草案，待用户定）

> 用户 2026-07-24 问「账号 usage 能不能直接新开窗口跑 `/usage`、再把输出读出来/截屏抓出来」→ 摸清后出方案。**未写码。**
> 结论先行：**机制可行、且是唯一可行路**（无非交互 API/命令）——但 `/usage` 是**交互式多屏面板**，抓取 = `capture-pane` 拿渲染后的屏幕文本（人读快照可靠，结构化解析脆）。需要一个**该账号已登录的 claude 会话**（推荐一次性临时会话）。**只读边界不破、不碰凭据 token、daemon 不参与。**

---

## 0. 为什么不是现有 usage 功能能覆盖的

cc-monitor 已有的 usage（#52，F88a/F88b）**只做 jsonl 里的 token 会计**，和 `/usage` 显示的**订阅额度**是两回事：

| | 现有 usage（#52） | 本方案要抓的 `/usage` |
|---|---|---|
| 数据 | 各会话 token（input/cache/output）+ 活跃会话 context% | **plan 额度**：5 小时滚动窗口 % + 周窗口 % + 重置时间 + 按 skill/subagent/plugin/MCP 拆分 |
| 来源 | 扫 `projects/**/*.jsonl`（`usage_query.rs` 服务端聚合 / `usage-hud.ts` live 流） | **Anthropic API**（`/usage` 面板 fetch，可能被限流） |
| 位置 | 本地/远端文件里都有 | 磁盘上没有，只能问 API |

⇒ 想在 cc-monitor 里看「这个号还剩多少额度 / 离重置多久」，**除了让 claude 自己渲染 `/usage` 再抓屏，没有别的路**（见 §1）。

## 1. 已核实的事实（claude-code-guide 查官方文档，2026-07-24）

- **无非交互取用量的路**：没有 `claude usage` 子命令、没有 `--usage`/`-p` 输出；`/usage` 是**唯一**文档化的取 plan 用量方式，且仅交互态（`/usage-credits` 明确不支持 `-p`）。
- **`/usage` 是交互式多屏面板**（非纯文本、也非全屏 modal）：会话块（token/本地估算成本/时长/改动）+ plan 块（5h/周窗口条 + 按 skill/subagent/plugin/MCP 拆分 + >10% 行为标记）；导航键 `d`/`w` 切 24h/7d、`r` 重试限流、`Esc` 关。
- **plan 条 fetch 自 Anthropic API**（可被限流；失败时显示过去 60 分钟的「last-known usage」快照）；会话成本本地估算。
- **新会话即可用**（无需先对话），但**必须已登录**。
- **无公开的个人账号用量 API**（只有 Enterprise/Analytics API 需 admin key）——故**不能绕开 `/usage` 直接调 API**（也不该：碰 OAuth token 违反只读/不碰凭据红线）。

## 2. 已有可复用积木（都在盘上，**无需新增 daemon**）

- **`tmux_send_keys(origin, target, keys, enter?)`**（`src-tauri/src/tmux.rs`，A5/A5+）：向 `cc-*` 会话发 `/usage`（`enter:true`）/ `Escape`（`enter:false`）。`is_ccm_tmux_name` 白名单只发本工具会话。
- **`capture_remote_pane(origin, target)`**（`src-tauri/src/tmux.rs`，F60）：`tmux capture-pane -p -t <target>` 抓屏幕文本；前端 `src/views/pane-preview.ts` 已有只读预览 overlay 用它。
- **临时会话起法**：`remote-launch.ts` 的 `tmux new-session -d -s <名>[ -c cwd] && send-keys '<载荷>'` 范式；载荷 = `CLAUDE_CONFIG_DIR='<该号 configDir>' claude`（A4 的 `buildEnvPrefix` + A6 的 `acct-deploy` 已有 configDir 注入/校验）。
- **账号库**：A3 的 `accounts.ts`（`fetchAccounts`→`accountConfigDir`、`isSelectable` 判已登录）。

## 3. 推荐方案：一次性临时会话抓快照（on-demand）

对某个**已登录**账号，点「查用量」：

1. **起临时会话**：`tmux new-session -d -s cc-usage-<号>[-N]`，send-keys 载荷 `CLAUDE_CONFIG_DIR='<configDir>' claude`（该号 config-dir 起一个 headless claude TUI）。
2. **等 TUI ready**：轮询 `capture_remote_pane` 直到出现输入框提示符（复用 A5+ `awaitExitFor` 的轮询骨架，判据换成「ready」）。
3. **发 `/usage`**：`tmux_send_keys(..., "/usage", enter:true)`。
4. **等 plan 条加载**：轮询 `capture_remote_pane` 直到抓到 plan 块标记（如「5-hour」「weekly」字样 / 用量条字符）；有界超时（API 可能限流 → 抓到「last-known usage」也算到）。
5. **抓屏**：`capture_remote_pane` 拿渲染文本 → 展示（**MVP：复用 `pane-preview.ts` 的等宽 `<pre>` overlay，原样给人看**；结构化解析留后续，见 §5）。
6. **收尾**：`tmux_send_keys(..., "Escape", enter:false)` 关面板 → `kill_remote_tmux` 收掉临时会话（临时会话，直接 kill 无副作用）。

> **为何用临时会话而非注入用户在跑的会话**：注入 = 在用户**正在干活的会话**上弹 `/usage` overlay（打断 + `capture-pane` 抓到的是他当前屏而非用量面板 + 得替他 Esc 回去）——太侵入。临时会话隔离、可随手 kill、不碰用户现场。代价：多一次进程起停 + 一次 API 往返。

## 4. 改动面 / 工作量 / 风险 / 安全

- **改动面**：纯前端新模块（编排：起临时会话→等 ready→/usage→等加载→capture→展示→Esc+kill）+ 复用 `tmux_send_keys`/`capture_remote_pane`/`pane-preview` overlay/`accounts` store。**零新增 daemon/Rust 命令**（capture_remote_pane/tmux_send_keys 均已注册）。设置「账号」组每行加个「查用量」按钮（挨着 A6 的「登录终端」）。
- **工作量**：中（编排 + 两处轮询 ready/loaded + overlay 复用 + 纯逻辑单测）。
- **风险**：中——① `/usage` 面板布局/字样随 CC 版本变，抓屏文本**能给人看、但结构化解析脆**（MVP 只展示原文规避）；② **API 限流**：`/usage` plan 条会被限流，故只做**手动 on-demand**、别轮询；③ ready/loaded 的可判定信号是启发式（同 DESIGN §1 V3 的 TUI-ready 难题）——超时兜底 + 直接展示当前屏。
- **安全**：`capture-pane` 抓的是**用量 %/条**，`/usage` **不显凭据 token** → 无泄漏；临时会话用该号既有凭据登录（**不搬/不读/不回显 token**）；send-keys/capture/kill 全走一次性 ssh、**daemon 不参与**（只读铁律不破）；不碰用户 `~/.claude`（临时会话的 config-dir 是账号库里已存在的，只读起进程）。

## 5. 后续（非 MVP）

- **结构化解析**：从抓屏文本正则/解析出 5h%、周%、重置时间 → 结构化展示 + 逼近上限预警（脆、随版本变，价值验证后再做）。
- **缓存**：一次抓的快照缓存几分钟（API 限流 + 面板本就显示 60 分钟内 last-known），避免重复起会话。
- **合并到账号面板**：每个账号行直接显「5h: 42% · 周: 18%」（需结构化解析）。

## 6. 决策点（请用户定）

1. **做 / 不做 / 缓一缓？**（是补足「看不到 plan 额度」这个真缺口的唯一路。）
2. 若做：**承载形态** = MVP 原样抓屏 overlay（复用 pane-preview，快）/ 还是直接上结构化解析（脆、慢）？
3. **触发**：设置「账号」组每行「查用量」按钮（手动 on-demand，避开限流）——认可否？
4. 归属：作 account 家族的小追加（A8?）/ 独立小功能？走 planned-build（plan→实现→D 审计）。**本方案不碰 daemon、不发版**（同 A6/A5+）。

---

**状态**：草案，待用户拍板。**GitHub issue：#73**（`bo0Zeng/cc-monitor#73`）。相关：`doc/远端支持方案-agent查看器与代码全景图.md`（同类「远端能力缺口」草案，issue #64）、`.claude/planned-build/account-isolation/`（账号功能族 A0–A6+A5+ 已交付，#68）。
