# Changelog

本文档记录 cc-monitor 用户**可感知**的功能 / 修复 / 行为变更。
内部重构与文档调整通常不入。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。
版本遵循 [SemVer](https://semver.org/)。

---

## [未发布]

### 修复 — 大段历史被误折成「已被 ESC 回退」 (issue #25)

jsonl 行偶发被重复投递（watcher 截断重读会换新 seq 重投整个文件，此前完全静默）时，**一条**重复记录即可毒化主线折叠算法——最坏整段历史（4000+ 条）被误折成「已被 ESC 回退」，且每次重算都复现、F5 不自愈（实锤两例：尾段 5 条 / 整段 pre-compact 树）。现在：
- 主线算法对重复输入**幂等**（`computeMainBranch` 入口按 uuid 去重 + `BranchFolder` 拒重双层防御，INVARIANTS 新增 § 25 投递契约），整类重投路径一次消失；
- watcher 截断重读补 tracing warn、Kahn 异常输入补 console.warn（带嫌疑 uuid）——不再有静默路径，再出问题有第一证人；
- 渲染层对换 seq 重投的幂等是已知残留，另开 issue #26 跟踪。

### 新增 — 会话红绿灯 (issue #23)

每个本地 Tab 的状态圆点现在反映 Claude 的实时状态（信号直连 Claude Code 官方
`sessions/<PID>.json` 的 `status` 字段，仅状态转换时更新，延迟 <1s）：
- 🟢 **绿** = AI 运行中（busy）
- 🟡 **黄**（呼吸闪烁）= 等你**决定**——权限确认 / 弹窗选择（waiting，悬停 Tab 可见细分原因）
- 🔴 **红** = 答完了，等你下一条输入（idle/shell）
- 旧版 Claude Code（无 status 字段）与远端 Tab 维持原绿点（远端 status 透传另行跟进）。

**Agents 展开区**：status bar 新增 `🤖 N agents (M 运行中)` chip（task 面板同形态，0 agent 隐藏），
展开后每个 subagent 一行：自己的状态灯（🟢 运行中呼吸 / ✓ 完成 / ✗ 中止）+ [类型] + 描述。
数据纯前端从 jsonl 流配对 Task/Agent 的 tool_use↔tool_result（零后端改动）；会话变 idle/归档时
仍在跑的标中止（ESC 打断不留僵尸绿灯）。回答了"agent 在跑时主灯也是绿（官方 busy 不区分）、
怎么看清单"的问题。

### 新增 — AskUserQuestion 选项与 API 报错直接可见 (issue #21)

两类"折叠后让用户误以为 LLM 还在输出"的内容改为默认展开、显眼呈现：
- **提问卡**：`AskUserQuestion` 不再折进 🔧 工具组——问题 + 全部选项（label/说明/可多选标记）直接可见；用户答复后卡片降噪、选中项打 ✓ 绿色高亮。`ExitPlanMode` 同理，plan 正文直接以 markdown 展示（📋 计划待批准）。
- **API 报错可见化**（实测两种真实形态）：重试耗尽/不可重试的**最终失败**（`isApiErrorMessage`）从"被当普通 Claude 回复渲染"改为红色 ⛔ 报错卡（含分类/状态码）；单次失败**将重试**的中间态（`system` `api_error`，此前完全不可见）渲染为 ⚠ 单行细条（含 重试 N/M）。

### 修复 — F5/HMR 重载后已结束的远端会话残留为关不掉的 live Tab (issue #20)

补上 v2.9.5 (#19) 留下的远端缺口：远端 sid 不在 `session_map`，#19 的本地对账覆盖不到。
- **后端**：维护「远端当前活跃 sid 集」（随 daemon 的 session added/removed 与断连 flush 增删），frontend-ready 重放后按它对账、对已结束的远端 sid 补发 session-ended 归档。
- **前端**：session-ended 改与行事件**同队列同序**处理——此前同步派发会抢在积压重放行之前执行，归档随即被后续远端行的 un-archive 复活（审计发现，纯后端方案在典型场景必然失效）。
- 断连窗口期重载会把仍活着的远端会话一并归档，重连重放后自动复活（同 #17 行为）。顺手把启动重放的块间 pause 从 `std::thread::sleep` 改为 `tokio::time::sleep`（不再压住 tokio worker）。

### 修复 — ESC 回撤废弃的「首条消息」不再误显 (issue #22)

新开会话时，第一条消息发出后又 ESC 回撤（claude 回复前回撤、或打断后回撤、连环回撤多次），被回撤的废弃首条/重发**没有被折叠**、照常渲染。根因：首条 user 是 `parentUuid=null` 的 root，回撤产生第二个 root 而非同父兄弟，旧 fork 检测（同 parent 多 child）抓不到。
- `computeMainBranch` 多 root 时分类：当前活跃分支（latestDescTs 最大的 root）永远保留；其余 plain `user` root 若子树是死胡同（无 assistant 后代，或**最新会话叶子**是 `[Request interrupted by user…]` 打断；忽略末尾尾随的 `/model` 等 system 命令）→ 整棵折叠。
- `/compact`（system 边界 root）、`/clear`、链断祖先、pre-compact 历史等合法多 root 一律保留，不误折。

## [2.9.5] — 2026-06-12

### 新增 — 远端 Tab 终端拉前 ↗ (issue #18)

远端 `[host]` Tab 的 ↗ 按钮现在能把对应的本地 ssh 终端窗口拉到前台（对标本地 ↗）。
- **按 sessionId 精确绑定**：在远端 `.bashrc`/`.zshrc` 加 `ccm` wrapper（设置面板「远端 (SSH)」→「远端 ↗ 拉前」展示片段供复制）、用 `ccm` 代替 `claude` 启动；多个独立 ssh 窗口各拉各的。
- **opt-in、零远端侵入**：cc-monitor 只扫本地终端窗口标题，不写你的远端机器；marker 走终端标题转义经 ssh 透传到本地。
- **兼容 `/resume`**：wrapper 跟踪 claude 当前 sid（`sessions/<PID>.json` 变了就重刷 marker）+ cc-monitor 点 ↗ 时若未绑定则**现扫一次**兜底，故 `cc` 启动后再 `/resume` 切到别的会话也能正确拉前。
- **限制**：多个 ssh 会话开在同一 Windows Terminal 窗口的不同 tab 时，↗ 只能拉起该窗口、无法切到具体 tab（OS 限制，本地 ↗ 也一样）——建议每会话单独开窗。

### 修复 — F5/HMR 重载后已结束的本地会话残留为关不掉的 live Tab (issue #19)

前端是纯事件增量模型（Tab 见行即建 live，只有一次性的 session-ended 能归档）。F5/HMR 重载后 event_replay 把 buffer 里已结束会话的行重放成 live Tab，而归档信号不在 buffer、不会重发 → 僵尸 live Tab（还因 closeTab 门控 archived 而**关不掉**）。同终端反复 `/resume` 换 sid 会高频放大此问题。
- **后端**：frontend-ready 重放后按 `session_map` 当前活跃集对账，对已结束的**本地** sid 补发 session-ended（复用前端 archiveTab）。
- **前端**：加 `pendingArchive` 集合——归档信号若早于 replay 建 Tab 到达就记下、建 Tab 时落实，消除 `jsonl-batch` 异步 drain 与 `session-ended` 同步派发之间的时序竞争。
- 远端同类缺口（重载后已结束远端 Tab 残留）另行处理。

## [2.9.4] — 2026-06-09

### 新增 — 远端 SSH 断线自动重连 + 主窗口按 seq 去重 (issue #17)

之前远端 SSH 连接掉线（笔记本休眠 / 网络切换 / Tailscale 抖动）后**不会重连**——远端数据源静默假死，新会话再也不显示，直到重启 cc-monitor。现在：

- **自动重连**：掉线后指数退避（2s→30s 封顶）自动重连，收到 daemon hello 即重置退避。
- **重连不重复内容**：新 daemon 会从 seq 0 重放整个活跃会话；主窗口加 per-Tab `seenSeqs` 去重（覆盖 attachment / isMeta 等有 seq 但不入 timeline 的记录），重放的旧行丢弃、断线期间新增的行正常补上——Tab 内容不翻倍，相当于轻量 catch-up。
- **重连后会话复活**：掉线时被归档的远端 Tab，重连重放到达后翻回 live（真正结束的会话保持归档）。
- **本地路径完全不变**：本地 seq 全程唯一，去重为 no-op。

## [2.9.3] — 2026-06-09

### 修复 — skill / 命令注入的 prompt 被当成用户消息渲染

Claude Code 会把 skill（如 `/code-review`、`/full-audit`）展开的 prompt、`/` 命令、
system-reminder、本地命令 caveat 等以 `isMeta:true` 的 user 记录注入对话 —— 这些不是
用户真正输入。此前 monitor 把它们当普通用户气泡渲染，导致整段 skill 指令混进对话框。
现在带 `isMeta` 的记录不再建卡（仍保留在时间线里，维持 ESC 回退主线检测的 parent 链
完整，同 attachment 的处理）。本地 / 远端、历史浏览、独立窗口、subagent 全路径生效。

## [2.9.2] — 2026-06-06

### 修复 — 单键快捷键在用过设置面板后全部失效

v2.9.1 改单键默认快捷键 + 加输入框守卫后引入的回归：设置面板关闭时只是隐藏、并未释放焦点，残留在隐藏输入框（远端 host/port、外观字段等）上的焦点让守卫误判成「正在打字」，于是 `H` / `W` / 数字等**所有单键快捷键被吞掉**。

- 设置面板关闭时主动 `blur()` 面板内聚焦元素，焦点回到主视图。
- 守卫不再把**不可见**的输入（已隐藏 / 脱离渲染树）当作「正在打字」——通用兜底。
- 顺带：设置 / 历史 / 终端 / 工作目录按钮上残留的 `(Ctrl+…)` 提示气泡改成对应单键。

## [2.9.1] — 2026-06-06

### 新增 — 拖拽 Tab 撕离成独立窗口（tear-off）

按住任意 Tab 往**标签栏下方**一拖、松手，就把该会话弹成一个独立的只读窗口（与原来右键「在新窗口打开」/ `N` 是同一个实时同步窗口），并在鼠标落点处打开，方便双屏 / 并排看。

- 拖动时有跟随光标的虚影；越过标签栏下边界后提示变「松开 → 独立窗口」，此时松手即弹出。
- 不影响点击切 Tab、📂/↗/× 子按钮、中键关、右键菜单——拖拽只是多出来的一条路径（移动不超过 6px 仍算普通点击）。
- 源 Tab 仍留在主窗口（主窗口是管理器，弹出窗口是它的实时只读镜像）。

### 变更 — 默认快捷键改为单键（去掉 Ctrl / Shift）

cc-monitor 是只读监视窗口，主视图不接受文本输入，组合键多余。默认快捷键全部改成**单键**：

- 切 Tab：`]` / `[`（下一个 / 上一个）；跳 Tab：`1`–`9`；关归档 Tab：`W`
- 打开工作目录 `E`；独立窗口 `N`；调出终端 反引号键；历史 `H`；设置 `,`；最小化 `M`；Task 面板 `T`；关弹层 `Esc`
- **输入框守卫**：在历史搜索 / 设置输入 / 重命名等可编辑处聚焦时，单键快捷键自动让位给打字，不会误触发（checkbox / 取色器等非文本控件不受影响，导航照常）。
- 仍可在 **设置 → 快捷键** 编辑器里改成任意组合键；之前自定义过的绑定不受影响。

## [2.9.0] — 2026-06-05

### 新增 — SSH 远端模式（实验性 / opt-in）：远端 Linux 跑 claude，本地渲染（issue #15）

让 cc-monitor 连到远端 Linux 主机，把那台机器上 claude 的**活跃会话**实时渲染到本地——本地与远端会话**同屏显示**，远端 Tab 带 `[host]` 前缀区分。

- **聚合**：本地 jsonl-watcher 照常跑，远端是**附加**数据源（不替代本地）；只显示**活跃**会话（远端 `sessions/<PID>.json` + 进程存活，镜像本地判活逻辑），不拉历史会话。
- **设置 → 远端 (SSH)**：从 `~/.ssh/config` **下拉选主机别名**自动填 host/port/user/key（走 `ssh -G`）；**测试连接**按钮（显示 host key 指纹 + 一键固化为严格校验）；支持 **ssh-agent**（免填私钥路径）。
- 远端需一个轻量 daemon（在目标机原生 `cargo build`，见 [`doc/REMOTE-PHASE0-DEPLOY.md`](doc/REMOTE-PHASE0-DEPLOY.md)）。**只读、零侵入**，仅 publickey / agent 认证。
- **实验性边界**：断线不自动重连（断开后远端 Tab 自动归档，需重启重连）；远端的历史浏览 / 全文搜索暂仍读**本地**数据；不支持密码登录。后续版本补齐（见 issue #15 的 Phase 1 backlog）。

### 新增 — Edit/Write/MultiEdit 工具调用渲染为行级 diff 卡（issue #14）

展开 `Edit` / `Write` / `MultiEdit` 工具折叠条，不再是原始 JSON（两坨转义的 `old_string` / `new_string`），而是**行级 diff**：红色删除行 + 绿色新增行 + 灰色上下文行，一眼看出 claude 改了哪些行。

- **Edit**：`old_string` → `new_string` 行级红删绿增；**Write**：整块内容当新增（全绿）；**MultiEdit**：逐条 diff 叠加，标 `Edit 1 / 2 …`。
- 超长 diff 自动截断 + 「↕ 显示完整 diff（+N −M）」按钮（点击展开全部）。
- 颜色复用外观主题的成功 / 错误色（改取色器即同步重染），不新增设置项。
- 任何异常 / 畸形输入 / `NotebookEdit` 一律**优雅回退**到原 JSON 视图，绝不空白。
- 纯前端、只读，不改任何 Claude Code 数据；CRLF / 中文 / emoji / 三引号代码均按字面安全渲染。

### 修复 — 历史会话点进去空白

**症状**：历史浏览器（Ctrl+H）点开任一会话，顶部状态栏显示「N 条记录 · 只读历史视图」，但消息区一片空白。

**根因**：只读查看器 `SessionViewer` 的消息流元素 class 是 `stream session-viewer-stream`，缺多 Tab 机制用的 `.active` 类。而基类 `.stream` 默认 `visibility: hidden`（只有 `.active` 的 tab 流可见）→ 卡片全部渲染进 DOM 却不可见。

**修法**：给 `.session-viewer-stream` 显式 `visibility: visible`（独立查看器永远是唯一可见流，不该借用 tab 的 `.active` 开关）。另加渲染韧性：逐条 `try/catch`，单条记录渲染失败不再让整个查看器空白，并把首个错误显示在状态栏。

### 改进 — 历史「↺ 恢复」改用 PowerShell + `cc`（读取 profile，代理生效）

**症状**：历史浏览器 ↺ 恢复会话时，启动的是 `cmd /K claude --resume`：(1) 跑的是 `claude` 而非用户的 `cc` wrapper，`cc` 里的代理 / env 设置不生效；(2) 那是 cmd.exe 不是 PowerShell，从没加载用户 profile，退出 claude 后敲 `cc` 也不工作。

**修法**：改用系统自带 `powershell.exe -NoExit -EncodedCommand <base64>`（**加载用户 profile** → 代理生效），命令为 `if (Get-Command cc) { cc --resume <sid> } else { claude --resume <sid> }`——装了 `cc` wrapper 就走 `cc`（含 `__ccm_bind`），没装才回退 `claude`，且回退也在加载了 profile 的真 PowerShell 里。`-NoExit` 让 claude 退出后窗口保留、`cc` 可继续用。命令用 `-EncodedCommand` 透过 wt.exe / cmd 多层 shell（绕开引号 / `;` 转义），并对 `session_id` 做注入校验（仅 `[A-Za-z0-9_-]`）。

---

## [2.8.0] — 2026-05-31

### 新增 — Tab 在独立窗口打开（多窗口 / 双屏，issue #10）

可把某个会话拉到**独立只读窗口**，与主窗口实时同步——双屏并排看、长任务单独挂一个窗口常驻。

- **入口**：Tab 右键「在新窗口打开」/ 快捷键 `Ctrl+Shift+N`（可在设置里改）。
- 独立窗口只读、无 tab 栏 / 设置 / 历史，专注镜像渲染该会话；与主窗口实时同步增量。
- 顶部 slim 栏：项目名标题 + **↗ 调出对应终端**（`Ctrl+``）+ **📂 打开工作目录**（`Ctrl+Shift+E`）。
- 关独立窗口不影响主窗口的 Tab；同一会话重复打开会聚焦已有窗口。

### 修复 — Tab「打开工作目录」开到漂移后的子目录

**症状**：会话过程中若工作目录变过（如从项目根切到子目录），Tab 的 📂 打开的是**最后**的目录而非项目根。

**根因**：tab 的 cwd 取「第一个到达的记录」，而启动重放末块先发（最新优先）→ 抓到的是最新记录的 cwd（漂移后的子目录）。

**修法**：tab.cwd 改取**最小 seq（最早记录）**的 cwd = 项目根，与历史浏览器口径一致。

实现（方案 A）：后端 `async` 命令新建 `viewer-<sid>` WebviewWindow 加载 `index.html?viewer=<sid>`，前端检测参数走精简 bootstrap（复用渲染管线，继承分支折叠 / 滚动消抖 / tool-group 合并）。历史经 `replay_session_to_window` 用 `emit_to(webview_window)` **定向** emit + 前端窗口作用域监听（与实时同 seq 空间），实时 `jsonl-line` 广播按 sid 过滤 + seq 去重。设计细节与四个踩坑见 ARCHITECTURE §5 / INVARIANT §22。

---

## [2.7.1] — 2026-05-30

### 改进 — 全文搜索加范围 / 时间筛选（issue #6 后续）

历史浏览器「全文」模式新增两个筛选下拉：

- **范围**：全部 / 只我的输入（user）/ 只 Claude（assistant）。
- **时间**：全部 / 近 7 天 / 近 30 天（按消息时间戳筛）。

与已有「含工具内容」复选框正交组合；改任一筛选即时重搜。后端 `search_history` 加 `scope` / `afterMs` 参数，在内存索引扫描时按记录类型 + ts_ms 过滤（不影响两级匹配性能）。

---

## [2.7.0] — 2026-05-30

### 新增 — 历史会话全文搜索（issue #6）

历史浏览器（Ctrl+H）原本只能按项目名 / 会话标题过滤，现可搜会话**内容**。

- 顶栏新增「项目 / 全文」模式切换。"全文"模式输入关键词回车 → 搜索所有会话的消息内容，结果按会话分组 + 命中片段高亮，点击任一命中进入只读视图并**自动滚动定位 + 高亮**该消息。
- **默认只搜有用内容**：user 输入 + Claude 回复文本；CLI 注入的包装（`<system-reminder>` / `[Request interrupted by user]` 等）自动剥除。
- **可选「含工具内容」**复选框：附加搜索工具调用 / 工具结果 / thinking。
- 大小写不敏感，中文 / 英文混排正常。
- 索引在启动后台构建（不阻塞首屏），就绪前显示"索引中"；可手动「重新索引」。

实现：后端 `search.rs` 内存索引（按 session 分组、原文+小写两份、两级匹配、文本截断封顶）+ `search_history` / `get_search_index_status` / `rebuild_search_index` IPC；前端历史浏览器全文模式 UI + viewer `scrollToUuid`。实测 156 会话 / 3.4 万条消息索引构建约 4.7s（后台并行）。

---

## [2.6.0] — 2026-05-29

### 修复 — 启动消息乱序（B 重构核心目标）

**症状**：v2.5 用户反馈"刚启动软件时第一个 tab 出现消息乱序；F5 刷新后顺序正确"。

**根因**：v2.3 引入 chunked emit + v2.4 加 PayloadSource batch/live 分流 + v2.5 加 replaying flag + catch-up tail，累积 5 个互相耦合的状态字段（PayloadSource / inPrependMode / pendingPrependFragment / pendingToolGroup / chunkIndex / EventReplay.replaying）。每对状态机相位差都是潜在 bug，5 个具体相位 bug 已知。

**修法**：B 重构——把"恢复顺序"从"多 flag 协调"换成"单 seq 字段排序"：
- 后端 watcher 给每行分配 per-file 单调 `seq: u64`，写进 `JsonlLinePayload` wire
- 前端新模块 `RecordTimeline` 按 seq binary search 插入 DOM
- tool-group 合并改后处理算法（看 timeline 左邻居），删 pendingToolGroup 状态字段
- EventReplay 删 replaying flag + catch-up tail——chunked emit 期间 watcher 真新行直接 emit，前端按 seq 自动放对位置

**回归性**：5 个状态字段全删；83 cargo test passed；任何 emit 顺序乱序到达都能自动恢复（不再担心第二批 batch / catch-up live 顺序问题）。

详 [外部 v2.6-b-refactor-notes.md](../doc/v2.6-b-refactor-notes.md) + DECISIONS.md ADR-021..024（仓库外）。

### 修复 — HistoryView 期间新 session 显示空白

**症状**：打开历史浏览器（Ctrl+H）期间，到一个新终端跑 `claude`，关掉历史后新 session 的 Tab 显示空白不可切换。

**根因**：HistoryView.open() 用 `container.replaceChildren(this.root)` 接管 streamRoot；期间 ensureTab 把新 streamEl `appendChild` 到 streamRoot 成 sibling；close 时 `replaceChildren(...savedChildren)` 只恢复打开时的那批 children，**新建的 streamEl 被丢弃**。

**修法**：HistoryView 改 `position: fixed; inset: 0; z-index: 150` 自挂 `document.body` 作 overlay；不再接管 streamRoot。

**回归性**：打开 history 期间能正常 ensureTab；关 history 后切到新 tab 内容正确。

### 修复 — PowerShell profile 安装把 CRLF 改成 LF

**症状**：用户的 PowerShell profile 是 Windows 默认 CRLF 行尾（notepad/VSCode/git autocrlf=true 三大来源），cc-monitor [安装] 后整文件被改成 LF；下次 notepad 打开会被警告 / git diff 整文件标红 / 团队共享 profile 触发行尾战争。

**根因**：`profile_installer.rs::replace_or_append_block` / `strip_block` 用 `existing.lines().join("\n")` —— `str::lines()` 同时吃 `\n` 和 `\r\n` 终止符，`.join("\n")` 只用 LF 重组。长度校验也检不出（两边都已 LF）。

**修法**：用 `detect_eol(existing)` 探测原 EOL 风格 + `split_inclusive('\n')` 保留终止符 + 新 block 按 detected EOL 重写。补 3 个 CRLF 单测守护。

**回归性**：用户 CRLF profile 安装后仍 CRLF；LF profile 仍 LF；新插入的 ccm BEGIN/END 块跟随 detected EOL。

### 修复 — 14 处 alert 改 toast

**症状**：INVARIANT § 12 早就规定"关键失败必须 toast 不能 alert"，但 14 处生产代码仍在用 `alert(...)` —— 用户没看清就关掉，以为按钮坏了。

**修法**：`error-toast.ts` export `showActionFailureToast(headline, body, opts?)`；改 views/history.ts / settings/cc_integration.ts / settings/diagnostics-section.ts / settings/data-section.ts 共 14 处 alert → toast。tabs.ts 拉前失败的 `#bring-terminal-toast` 单例也改用统一 toast stack。

### 修复 — ESC 中断 regex 误吞合法消息

**症状**：cards/index.ts:835 用 `/^\[Request interrupted by user[^\]]*\]\s*$/gim` 剥 CLI 中断标记；`gim` 中的 `m` flag 让 `^...$` 锚到每一行 —— 多行 user 消息里夹一行符合该模式会被静默删除。

**修法**：去掉 `gim` flag，让 `^...$` 只匹配整字符串。CLI 实际把中断标记作为整条 user message 的唯一文本写入，整文本 trim 完正好是该模式才归零。

### 修复 — bring_monitor_to_front 同步阻塞 IPC 派发

**症状**：v2.4.0 加 `bring_monitor_to_front` IPC 是 `fn` 非 async，违反 INVARIANT § 10。Win32 同步调用（keybd_event / AttachThreadInput / SetForegroundWindow 等）数十 ms 阻塞期间其他 IPC（拉前 / 切设置 / 切 Tab）全部排队。

**修法**：改 `async fn` + `tokio::task::spawn_blocking` 隔离 Win32 调用；HWND 走 `as isize` 跨 windows crate 版本（INVARIANT § 19）。

### 修复 — 启动重放期最新消息整行高频上下微抖

**症状**：刚启动软件、滚动条已贴底显示最新消息，但在其余历史消息加载的那一两秒里，最新消息区域整行高频小幅上下抖动；加载结束即停。HiDPI / 高刷新率屏幕尤其明显。

**根因**：重放"末块先发"，最新一段先到贴底，更老的内容随后逐条（B 重构后的 per-record binary insert）插到**视口上方**，持续约 60 帧。每次上方插入都触发浏览器重排 + 重做原生 scroll anchoring，而分数像素布局下整数 `scrollHeight` 与分数布局的舍入误差**每帧不同** → 整块内容逐帧 ±0.5px 重绘。注意 `scrollTop` 本身单调增长不震荡，故极难定位。

**修法**（三道防线，详 INVARIANT § 21）：(1) `snap()` 改守卫式，只在落后底部 >1px 才贴；(2) 上方插入不手动补偿 scrollTop，交给原生 `overflow-anchor`；(3) `RecordTimeline` 加 deferMode，重放期"视口上方"旧内容延后到 `onBatchEnd` 用 `attachBatch` 一次性挂回。渲染仍按 40/帧推进（首屏不变慢）。

**回归性**：实测抖动帧数 66 → 1；中途同时引入又回退的 `.block-body` 去 containment 改动（会导致 tab 可横向滚动）已还原。

详 [外部 scroll-jitter-investigation.md](../doc/scroll-jitter-investigation.md)（排查复盘）。

### 内部清理（不影响用户）

- 删 8 处死代码 + 2 死 IPC（`read_session_jsonl` / `list_history_sessions_in_project`，仅留 stream 版）
- 加 `utils.rs` 新 helper：NetTicks/FileTime newtype（procStart 单位隔离）+ scan_dir_jsons 泛型 + atomic_write_json（统一 ReplaceFileW）+ parse_iso8601_ms/now_ms/systime_to_ms（归并散落实现）
- 加 `local-storage.ts`（LS_KEYS 集中 + safeGet/safeSet）+ `format.ts`（formatTime 合并）
- HistorySessionEntry/HistoryProject 全 camelCase + contract test 守护
- release.yml 加版本号一致性 guard（防 v2.4.2 漂移事故复发）
- 84 + 4 = 83 cargo test passed（删 1 旧 catch-up 测试 + 加 4 新切块/utils 测试）

---

## [2.5.0] — 2026-05-26

### 新功能 — 全部快捷键可自定义（issue #5）

所有快捷键现在都能在设置面板里改。点设置 → 「快捷键」分组 → [打开快捷键编辑器] → 弹出 modal 编辑器（modal overlay，仿小窗口风格）。

**支持的 action（17 个）**：

| 类别 | 动作 | 默认 |
|---|---|---|
| Tab | 切到下一个 / 上一个 | Ctrl+Tab / Ctrl+Shift+Tab |
| Tab | 跳到第 1-9 个 Tab | Ctrl+1..9 |
| Tab | 关闭已归档 Tab | Ctrl+W |
| Tab | 打开当前 Tab 的工作目录 | Ctrl+Shift+E（新） |
| Tab | 拖出为独立窗口 | 未上线（预留，issue #10） |
| 终端 | 把对应终端窗口拉到前台 | Ctrl+` |
| 应用 | 打开设置 | Ctrl+, |
| 应用 | 打开 / 关闭历史浏览器 | Ctrl+H |
| 应用 | 历史浏览器全文搜索 | 未上线（预留，issue #6） |
| 应用 | 最小化主窗口 | Ctrl+M（新） |
| 应用 | 关闭弹层 / 历史 / 设置 | Esc |
| 行为 | 切换「自动跟随用户输入」 | 未绑（新） |
| 行为 | 切换「自动拉前 monitor」 | 未绑（新） |
| 面板 | Task 面板开 / 关 | Ctrl+T（新） |

**关键设计**：

- **chord 用 `KeyboardEvent.code`** 而非 `key`，避免键盘布局差异（法语键盘 `Ctrl+,` 永远触发不了，因为 `,` 是 Shift+逗号——`e.code === "Comma"` 才稳）
- **冲突弹覆盖确认**：要把 Ctrl+W 改绑别的 action，弹「Ctrl+W 当前是 XXX，要覆盖吗？」点确认后旧 action 自动变"未绑"
- **Esc 改时强警告**：解绑 / 改成非 Esc 都弹 confirm，避免用户被锁在弹层里
- **未上线 action 灰显**：tab.pop-out (issue #10) / app.search-history (issue #6) 显示但不可绑，让你知道路线图
- **改即生效，无需重启**：保存即 unbind 旧 chord + bind 新 chord
- **统一 overlay 栈**：Esc 关闭 settings / 历史 / tasks-panel / 快捷键编辑器 走同一个 dispatcher 维护的 LIFO 栈，多弹层按打开顺序逐个关
- **持久化在 `config.json` 顶层 `keybindings` 字段**，缺失字段走默认，`null` = 显式解绑

### 新功能 — Tab 上一键打开工作目录

每个 live Tab 多出 📂 按钮（在 ↗ 拉终端按钮左边），点击 → 系统默认文件管理器打开该 session 的工作目录。也可用快捷键 `Ctrl+Shift+E`（"Explorer"）打开当前活跃 Tab 的 cwd。

复用 `@tauri-apps/plugin-opener.openPath`，archived Tab / 无 cwd 的 Tab 按钮自动隐藏。

### 性能修复 — 切 Tab 长对话不再卡顿

**症状**：切到含千条消息的长对话 Tab 时 monitor 卡 100-500ms。

**根因**：`display: none → block` 触发整棵子树重建 layout tree。`display: none` 时浏览器把那个子树完全踢出 layout tree（不算几何、不分配 box），切回 `block` 必须重建几万 DOM 节点的 layout tree + 解析每个 `<pre>` / `<code>` / KaTeX 元素的 box —— 同步阻塞主线程。

**修复**：改成 `visibility: hidden ↔ visible` 类切换。元素永久留在 layout tree 里，切 active 只改 attribute，**0 reflow**。

- `src/styles.css` `.stream` 默认 `visibility: hidden + pointer-events: none`；`.stream.active` 翻转
- `src/tabs.ts` 删 `streamEl.style.display = "none"`，switchTo 改 `classList.toggle("active", ...)`
- 代价：所有 Tab layout box 常驻内存，10 tab × ~5MB ≈ 可忽略

### UI 改进 — 设置面板重组为 5 大模块

7 个分散分组合并成 5 个折叠模块，除「行为」全部默认折叠：

```
设置
├── ▼ 行为                  (默认展开)
├── ▶ 快捷键
├── ▶ 数据源 & 集成    （← Claude 数据目录 + PowerShell cc 集成）
├── ▶ 外观
└── ▶ 诊断 & 存储      （← 诊断 toggle + monitor 数据路径展示）
```

每个分组标题旁加 ? 图标，hover 看长描述—— 原来散在表单里的 `.settings-hint` 文字全部收纳。表单本体更紧凑。

### 杂项

- 设置面板顶部标题「外观设置」→「设置」（涵盖范围已远超外观）；齿轮按钮 tooltip / aria-label 同步

---

## [2.4.3] — 2026-05-26

### 新功能 — 卡片内外链调用系统默认浏览器打开 (issue #13)

assistant 消息渲染出的 markdown 链接（`https://` / `http://` / `mailto:`）之前点击会让 Tauri WebView2 直接导航，把 monitor UI 整个替换成外站页面。修复后点击外链由系统默认浏览器（Chrome / Edge / Firefox）打开，monitor 窗口保持不变。

**实现**：
- `src/render.ts` marked link renderer 重写 —— 协议链接渲染成 `target="_blank" rel="noopener noreferrer" data-external`
- `src/main.ts` 全局 click 事件代理 (capture 阶段) —— 命中 `a[data-external]` → `preventDefault()` + `openUrl(href)`
- 复用现有 `@tauri-apps/plugin-opener` 插件，`opener:default` 权限已含 `allow-default-urls`（mailto/tel/https/http），无需新加 capability
- 锚点 / 站内相对链接不打 `data-external` 标记，保留默认行为

### 修复 — Tab 标题在 Claude Code v2.1.x 起空白

**症状**：v2.1 后开的新 Claude session，monitor 的 Tab 标题只显示项目名（如 `claudecode-frontend`），而不像 Claude Code CLI 终端那样显示完整的会话语义标题。

**根因**：Claude Code v2.1.x 把 JSONL 里的标题记录从 `"type":"ai-title"` / `aiTitle` 字段改成了 `"type":"custom-title"` / `customTitle`。monitor 后端 `JsonlRecord` 枚举只认旧名字，全部 custom-title 记录被 fallthrough 到 `Unknown` 变体 → 不 emit → 前端永远拿不到标题。

**修复**：保留旧 ai-title 兼容路径（旧 jsonl 历史文件仍可能有），同时新增 custom-title 路径，两者共用同一个 Tab 标题字段（`tab.aiTitle`）：
- `src-tauri/src/messages.rs` — `JsonlRecord::CustomTitle { custom_title, session_id }` 变体 + `is_displayable()` 收录
- `src-tauri/src/history.rs` — 历史扫描 match 加 CustomTitle 分支，与 AiTitle 同样写到 `ai_title` 字段（让历史浏览器列表里 v2.1.x 之后的会话也有标题）
- `src/cards/index.ts` — JsonlRecord union 加 custom-title 类型
- `src/tabs.ts` — onLine 加 custom-title → applyAiTitle 分支

### 版本号同步

本次同时把 `package.json` / `tauri.conf.json` / `Cargo.toml` 一起升到 `2.4.3`。之前 `release: v2.4.2` commit 漏升版本号文件（tag 已 push 但产物 metadata 仍是 2.4.1），本次一并修正。

---

## [2.4.1] — 2026-05-26

### 修复 — 「拉前 monitor 窗口」toggle 实际只闪任务栏不抢焦

**症状**：v2.4.0 开启「自动切 Tab 时同时把 monitor 窗口拉到前台」后，用户在终端敲键回车 → monitor 切了对应 Tab，但**窗口没真浮上来**，只是任务栏图标闪了下。

**根因**：Windows 的 [SetForegroundWindow 限制](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow)——只允许调用方进程本身**在前台焦点链上**时合法拉前别的窗口；其他情况 OS 直接拒绝（防恶意软件偷焦点），仅返回 false + 闪任务栏。

v2.4.0 的 `bring_monitor_to_front` 走 Tauri 的 `WebviewWindow::set_focus()` 内部就是 `SetForegroundWindow`，被这条限制必拦：

- `bring_terminal_to_front`（v1.7 既有，monitor→PS 拉前）能成功，是因为**用户点 monitor 按钮的瞬间** monitor 就是前台
- `bring_monitor_to_front` 场景反过来——用户敲终端时**前台是 PS/WT**，monitor 没权抢

**修复 = AttachThreadInput hack**：

```
fg_thread = GetWindowThreadProcessId(GetForegroundWindow())
AttachThreadInput(fg_thread, current_thread, true)   ← 临时附加到前台线程输入队列
SetForegroundWindow(monitor_hwnd)                     ← 借用其权限合法拉前
AttachThreadInput(fg_thread, current_thread, false)  ← 立刻 detach
```

OS 把附加期间这两线程视作"同输入上下文"，前台限制自动通过。Visual Studio / 各 IDE / 切窗工具广泛使用，被 OS 视为合规（非恶意）。

实施细节：
- `windows` crate 0.56 中 `AttachThreadInput` 位于 `Win32::System::Threading::AttachThreadInput`（feature `Win32_System_Threading` 已有，无需新加）
- Tauri 2 内部用 windows crate 0.61（HWND 内部是 `*mut c_void`），我们用 0.56（HWND 内部是 isize）—— 通过 `tauri_hwnd.0 as isize` 跨版本兼容
- 加 `BringWindowToTop` 一步调整 Z 序辅助 SetForegroundWindow 在边界情况下成功
- 非 Windows 平台保留 v2.4.0 的 set_focus 兜底实现

### 备注

非常少数情况下 OS 仍可能拒绝（hack 也不是 100% 万能；检测到滥用 / lock 时序竞争）。此时仍闪任务栏给用户提示。

---

## [2.4.0] — 2026-05-26

### 新功能 — 终端 active session 自动同步到 monitor Tab（issue #2）

你在多个 PowerShell tab 里跑多个 claude session，切到某个 tab 在 claude 里敲回车 → monitor 自动切到对应的 Tab，不用再手动到 monitor 窗口点。

**信号源：watcher 反推 `type=user`**

- 不依赖 OS 焦点检测（Windows Terminal 单进程多 tab，`GetForegroundWindow` 永远拿 WT 主进程 HWND 无法区分 tab —— v1 早期 `FOCUS_SWITCH` 就是因此废弃）
- 不依赖 ConPTY FOCUS_EVENT（PSReadLine 独占 console input，第三方进程抢句柄不稳）
- 不依赖 claude code hooks（违反零侵入）
- 不依赖 Windows Terminal 公开 API（[microsoft/terminal#19818](https://github.com/microsoft/terminal/issues/19818) 仍在 Backlog，"Spec Needed"，无 ETA）

watcher 反推天然零侵入：jsonl 已经在监听，用户在终端敲回车 → claude 写一行 `type=user` 到对应 jsonl → monitor 立即识别 → 切对应 Tab。

**严格分辨"真用户输入" vs "工具回灌"**

Claude Code JSONL 里 `type=user` 实际三种形态：
1. 真用户敲键的文本 → ✓ 触发自动切
2. CLI 内部 prompt 包装（被 `stripInternalNoise` 剥光的）→ ✗ 不触发
3. 工具结果回灌（`content: [{type: "tool_result", ...}]`，Anthropic API schema 把工具返回挂在 user role 上）→ ✗ 不触发

判别复用前端 `cards/index.ts::renderMessage` 既有逻辑——`result.kind === "card"` 已经过滤了 noise + tool_result。**无后端新事件**，复用既有 `jsonl-line` 路径在 `tabs.onLine` 末尾加判断。

**Manual override：5 秒不抢回**

你手动点 monitor 的 Tab Bar / Ctrl+Tab 切到别的 Tab 后，**5 秒内**任何 user-active 信号都不会抢回。给"我现在主动看另一个 tab"的意图留缓冲。5 秒后才恢复自动跟随。

**两个独立 toggle**

设置面板新增「行为」分组：

- **「用户在终端里输入时自动切到对应 Tab」**（默认开）
- **「自动切 Tab 时同时把 monitor 窗口拉到前台」**（默认关）—— 默认不抢焦避免打断浏览器/IDE 工作；想让 monitor 主动浮上来时勾上

第二个 toggle 仅当第一个开启时生效（前者关时灰显）。

### 新增 IPC
- `bring_monitor_to_front` — `unminimize + show + set_focus` 主窗口，复用 single-instance plugin 回调同款逻辑。受 `AllowSetForegroundWindow` 限制，某些场景 OS 仅闪任务栏不真拉前（Windows 设计）。

### 新增 config 字段
- `autoFollowUserActive: bool`（默认 true）
- `bringMonitorToFrontOnUserActive: bool`（默认 false）

均与 `theme` / `claudeDir` / `diagnostics` 字段平级。**运行时热更**（不像 claudeDir 需要重启）。

---

## [2.3.1] — 2026-05-26

### 修复 — 首次启动消息乱序（必须按 F5 才正常）

**症状**：dev mode / 安装后首次启动 monitor，stream 里消息顺序错乱；用户必须按 F5 刷新一次才显示正确顺序。

**根因双重**：

1. **后端 watcher 异步初始扫与 frontend-ready 竞态**：v2.3 架构下，`spawn_watcher` 把全量扫扔进独立线程异步执行，setup() 立刻返回。同时另起一个 `tauri::async_runtime::spawn(while rx.recv())` async task 慢慢从 mpsc channel 把行 drain 给 `EventReplay::record`。前端 emit("frontend-ready") 时（~T+450ms）watcher 还没扫完 + async task 还没 drain 完 → `replay_and_mark_ready` 持锁 snapshot 看到的 history 不完整 → 部分历史漏到 ready=true 后的 live emit 路径，跟 chunked replay 的 chunks 错位到达前端。

2. **前端 inPrependMode 误捕获 live emit**：`tabs.appendCardOrBuffer` 检查全局 `inPrependMode`，对**所有** payload 一视同仁。chunked replay 进入 prepend 模式（chunk index > 0）时，任何 `jsonl-line` live emit（不管是漏出来的旧历史还是用户实时敲的新行）都被错误丢进 `pendingPrependFragment`，最终被推到 stream 顶部。

F5 不出 bug：刷新时 backend 已稳定几秒，history 完整，replay 一次成型，无 live emit 干扰。

**修复**：

后端：
- `watcher.rs::spawn_watcher` 改接 `on_line: LineHandler` 闭包，**干掉 mpsc 中间层**。watcher 线程内同步调 record()，history buffer 在 watcher 线程内同步落盘。
- `WatcherHandle` 增加 `initial_scan_done: Arc<AtomicBool>`，watcher 同步全量扫完才置 true 然后进 debouncer 监听阶段。
- `lib.rs` frontend-ready listener 在 async task 里 spin-wait `initial_scan_done`（10ms poll，10s timeout 兜底），就绪才调 replay。保证 snapshot 时 history 完整。

前端：
- `events.ts` 新增 `PayloadSource = "batch" | "live"` 类型。`jsonl-batch` 拆出的 payload 标 `source: "batch"`，`jsonl-line` 标 `source: "live"`。
- `tabs.ts::appendCardOrBuffer(tab, element, source)`：source==="batch" 且 inPrependMode 时才走 prepend fragment buffer，**source==="live" 永远 stream.append 贴底**。

**用户体验**：首次启动跟 F5 后行为完全一致，无需手动刷新。加载期间用户在终端敲的新消息仍然实时贴到 stream 底部（绕开切块 prepend 逻辑）。

### 内部
- 删除 `tauri::async_runtime::spawn(while rx.recv())` async drain task。
- watcher 模块的 `mpsc::UnboundedReceiver` import 移除。

---

## [2.3.0] — 2026-05-25

里程碑：启动加速 10× 量级 + 三个 feat 同发 + tool-result UI 全面重做。

### 改进 — 启动加载速度 10× 提速（issue #1）

之前 v2.2 启动重放 ~3920 条 record 前端 drain + 渲染管线耗时 **22s** 才完全可交互。

本版本通过 **历史切块 + DOM prepend + lazy 代码高亮** 三层联防：

#### 历史切块 emit

后端 `event_replay::replay_and_mark_ready` 按 history 数量切块：

- N < 200 → 单次 emit（保持 v2.2 行为，无切块开销）
- N ≥ 200 → 按 session 分组取每 session 最新 N 条到 head 块，剩余按 600 条切 mid 块
- **chunk 0** (head 最新) → 前端 append 到 stream 底部 → 用户**立刻可见最新消息**
- **chunk 1..N** (older) → 前端 prepend 到 stream 顶部 → 后台默默 prepend 老内容
- 块之间释放锁停顿 10ms，让 watcher 真新消息能 live emit 并行插入

最终 DOM 顺序：`[最老 ... 次新 ... head 最新]` 时间升序保持。

#### Lazy 代码高亮 (highlight.js)

batch 重放期间 markdown 渲染走 lazy 路径：marked + DOMPurify + KaTeX 同步出 HTML，但 hljs **不跑** —— 留 `<div class="code-block code-pending">` 占位。

全局 IntersectionObserver（`rootMargin: 300px`）观察卡片进可视区时调 `enhanceCard` 补跑 hljs。

每条 record 渲染管线 5.6ms → 1.5ms（砍 ~70%）。KaTeX 保留同步（耗时占比小 + 拆开复杂度高收益小）。

#### 实测结果

| 阶段 | v2.2 | v2.3.0 |
|---|---|---|
| 首屏 head 可见 | ~22s | **~600ms** ⚡ |
| 全部 drain 完毕 | ~22s | ~1.7s |
| 用户可交互 | ~22s | **~600ms** |

详 [v2.3 启动加速学习笔记](https://github.com/bo0Zeng/cc-monitor/blob/main/CHANGELOG.md#230---2026-05-25)。

### 新增 — Tab Task 面板（issue #11）

Claude Code CLI 终端底部的 task tracker（`TaskCreate` / `TaskUpdate` / `TaskStop` 工具维护）现在能直接在 monitor 看到，再不用切回终端确认任务进度。

- 每个 Tab 的消息流顶部多一个 **sticky 折叠卡**：「N tasks (X done, Y in progress, Z open)」摘要 + 展开看完整列表（subject + 状态 icon），跟终端视觉对齐
  - `□ pending` / `■ in_progress` / `✓ completed` / `✗ deleted`，未知值兜底 `•`
  - 已完成的任务删除线 + 60% opacity，进行中的高亮一点背景色
  - `description` / `activeForm` 进 hover tooltip（点行不展开，省视觉空间）
- **默认折叠**；折叠状态写 `localStorage cc-monitor.tasks-panel.collapsed` **全局**持久（所有 Tab 同步偏好，重启 monitor 保留）
- **0 task 时 panel 完全隐藏**（display:none），不占视觉空间
- **实时同步**：CLI 跑 `TaskCreate` / `TaskUpdate` → ~100ms 内 monitor 对应 Tab 更新

### 实现

后端新增 `src-tauri/src/tasks.rs`：
- `read_session_tasks(tasks_root, sid)` 扫 `<claude_dir>/tasks/<sid>/<id>.json`
  - 跳过 `.lock` / `.highwatermark` / 非 `<digits>.json` 命名
  - 半截 JSON 单条 catch 跳过（写者持锁中途读到 → 下次 debounce 自然修正），不会冻死整次重读
  - 按 id 数字升序排序
- `spawn_task_watcher(tasks_root, app)` 用 `notify-debouncer-mini` 监听 `tasks/` 递归
  - 100ms debounce → 同批次按 sid `HashSet` dedup → 每 sid 重读整目录 → emit `task-update`（**完整重发**，无 diff）
  - `tasks_root` 不存在时静默不 spawn（用户从没用过 task tracker 的兼容态）
- `get_session_tasks` IPC（`async fn` + `spawn_blocking`）给 Tab 创建时拿初次快照
- 新事件 `task-update` + `TasksUpdatePayload { sessionId, tasks }`
- 9 个单元测试（empty / skip .lock / sort by numeric id / partial JSON tolerance / camelCase serde 契约 / session_id 路径反推 etc.）

前端新增 `src/tasks-panel.ts`：
- `TasksPanel` 组件（sticky 面板 + 摘要 header + 列表 body）
- 全 panel 实例共享 `LS_KEY = cc-monitor.tasks-panel.collapsed`，一个 Tab 折叠所有同步
- 完整 replace 渲染（task ≤ ~30 条无 diff 必要）
- `tabs.ts` 在 ensureTab 时挂 panel 到 stream 顶部 + 异步 `fetchSessionTasks`；`updateTasks(sid, tasks)` 路由 `task-update`；closeTab 时 dispose
- `events.ts` 加 `onTasksUpdate` 句柄；`task-update` 直接同步派发不进批量队列（稀疏事件）

### 新增 — 数据存储透明化（issue #3 A 阶段）

设置面板加「**数据存储**」折叠分组，列出 monitor 所有持久化数据位置 + WebView2 用户数据 + localStorage keys，每项配 [打开] 按钮直接进资源管理器查看。**纯展示，不动数据**。

- monitor 持久化目录（`~/.claude/claudecode-frontend/`）的所有文件：`config.json` / `sid-hwnd-cache.json` / `auto-launch.json` / `history-metadata.json` / `ps-await/` / `ps-registry/` / `logs/`
- WebView2 用户数据目录（`%LOCALAPPDATA%\<bundle>\EBWebView\`）— cache / localStorage / IndexedDB / cookies
- PowerShell profile 备份（v1.7.10+ 自动备份的 `.ccm-backup-<时间戳>` 文件，仅在装过 cc 集成时显示）
- 前端 localStorage 所有 `cc-monitor.*` keys + value（折叠 / 渲染模式 / profile 选项 / task panel 状态等）
- 卸载说明：NSIS 默认不清这些数据，想彻底清除手动删

后端新模块 `src-tauri/src/data_paths.rs`（4 个单元测试）+ IPC `get_data_paths`（async + spawn_blocking，stat 不递归算大小避免大目录卡 IPC）。前端 `src/settings/data-section.ts` 渲染分类卡片。

### 新增 — Tool result 渲染模式切换 + 长 output 性能修复

Tool 调用结果展开后顶部新加 [文本 | Markdown] 切换 toolbar：

- **Markdown 模式**复用 `renderMarkdown`（marked + DOMPurify + KaTeX + hljs），含 LaTeX / 代码高亮
- **行号前缀启发式 strip**：Read / Grep 等工具输出带 `<n>\t<content>` 或 `<n>:<content>` 行号前缀，MD 模式渲染前自动 strip 让 `#` 标题等结构暴露
- **per-tool-name 偏好持久** `localStorage cc-monitor.tool-render.<toolName>`；`Read` / `Grep` / `WebFetch` / `NotebookRead` / `TodoWrite` 默认 MD，其他默认 text

性能改进：

- `.block-body` 加 `content-visibility: auto` + `contain: layout style paint` + `contain-intrinsic-size`，浏览器跳过 viewport 外的 layout/paint，长 output 滚动不再卡
- 大 output (>200 KB) 默认只渲染前 800 行 + `[显示完整内容 (N KB)]` 按钮，避免一次性 marked 解析卡死主线程

### 单元测试

后端 44 → 57（+9 tasks tests +4 data_paths tests）。

---

## [2.2.0] — 2026-05-25

里程碑：历史浏览器从「全量同步加载、UI 死锁等几秒」升级到「流式 + 不阻塞 + fork 树形」。重放路径同步打通 batch 模式，启动 monitor 速度数量级提升。

### 新增 — 历史 fork 树形组织（issue #12）

- Claude Code CLI 的 `/branch` 命令分叉出新 session 时，jsonl 顶层有 `forkedFrom: { sessionId, messageUuid }` 字段。本版本在历史浏览器把 fork 关系展现成 **child 缩进显示在 parent 下** 的树。
- 项目内独立树（跨项目 fork 不连接 —— child 当 root 显示「↳ 原 session 不见了」marker）。
- 默认折叠；点 ▶ 展开看 children；折叠/展开状态本地持久 (`localStorage cc-monitor.history.expanded-forks`)，重启 monitor 保留。
- 后端 `messages.rs::JsonlRecord` 的 User / Assistant 加 `forked_from: Option<ForkedFrom>` 字段；`history.rs::HistorySessionEntry` 加 `forkedFromSessionId` / `forkedFromMessageUuid`。

### 新增 — 历史浏览器流式加载（issue #12）

- **session 列表流式**：用 Tauri 2 [Channel API](https://v2.tauri.app/develop/calling-rust/#channel)，后端 `stream_history_sessions_in_project` 边解析边发，前端 onmessage 增量插入到 fork 树。大项目（几十个 session）首条 < 100ms 出现，不再等齐。
- **session 内容流式**：`stream_read_session_jsonl` 按 100 行一 chunk emit，前端边收边 `renderMessage`。10MB 大文件几百毫秒看到首屏。
- **取消**：用户中途关闭历史视图 / 切走 → JS Channel 被 GC → 后端下次 `send()` 返 Err → 自动 break 出读取循环，不浪费 IO。
- 进度提示：「加载中 · 已 N 条…」/「继续加载中…」实时更新；完成后切到「N 条记录 · 只读历史视图」（修了 channel resolve vs onmessage 的竞态）。

### 改进 — 历史 IPC 异步化

历史浏览器全部 IPC（`list_history_projects` / `list_history_sessions_in_project` / `read_session_jsonl`）改 `async fn` + `tokio::task::spawn_blocking` 包同步 IO。**加载期间其他 IPC（拉前 / 切设置 / 切 Tab 等）能正常响应**，不再整个 UI 死锁。

### 改进 — 启动重放速度大幅提升（同 issue #12 路径）

之前启动 monitor 重放历史 jsonl 时，前端 `tabs.ts::onLine` 对**每一条** record 都调 BranchFolder.recordAdded → computeMainBranch（O(N)）→ 可能 DOM rebuild。2000 条 record = O(N²) ≈ 4M ops + 数十次重排 + events.ts 每 40 条让出主线程 1 帧（50 帧起步）→ 总耗时几秒。

本版本引入 **batch 模式**：

- `BranchFolder` 加 `setBatchMode(bool)` + `flushPending()`：batch 模式下 `recordAdded` 只 push 不算，flushPending 一次性 compute + rebuild。
- `events.ts` 的 `jsonl-batch` 事件包裹 `batch-start` ... payloads ... `batch-end` 哨兵；drain 按 kind 派发。
- `TabManager.onBatchStart` / `onBatchEnd` 在重放期把所有 Tab 的 BranchFolder 切 batch，结束时 flush + 切回 live。重放期新建的 Tab 也自动进 batch 模式。
- **结果**：2000 条重放从 ~3-5s 降到 ~200ms（**15-20× 加速**），跟历史只读视图同量级。真实时新消息照旧 per-record 走 live 模式不变。

## [2.1.1] — 2026-05-25

### 修复

- **长 session 启动只渲染头几条消息 + console RangeError**（v2.1.0 起的回归）：
  - 症状：~1000+ 条记录的 session 打开 monitor 后，前端只显示前几条消息，再就停了；F12 console 看到 `RangeError: Maximum call stack size exceeded`。
  - 根因：v2.1.0 issue #8 的 `computeMainBranch` 用真递归 (`dfsLatest` + `walkMain`) 算主线。Claude session 的 parent 链典型几乎线性，递归深度 = 链长度。WebView2 (Chromium) 默认 JS stack 在 ~1000 frames 附近触底 → BranchFolder.recordAdded 抛 RangeError → events.ts 的 drain 异常逃逸 → 后续 record 永久滞留 queue 不渲染。
  - 修法：
    1. `src/branching.ts`：两个 DFS 都改迭代。`latestDescTs` 用 Kahn 拓扑序自底向上累加 O(N) 无递归；`walkMain` 本来就是 tail-recursive，改 `while` 循环深度 1 帧。
    2. `src/events.ts`：drain 加 try/catch 包单条 `onLine` —— 防御未来类似的单条记录处理异常冻死整个 replay queue（详 [`doc/INVARIANTS.md § 17`](doc/INVARIANTS.md)）。

## [2.1.0] — 2026-05-25

### 新增

- **ESC 回退分支折叠**（issue #8）：Claude Code CLI 双击 ESC 回退到之前某条 user 重新发送，jsonl 里产生 `parentUuid` 分叉。本版本识别这种分叉并把"被回退"的连续消息段折叠到「已被 ESC 回退（含 N 条消息）」可展开容器里，主线显示一气呵成。
  - **算法**："只在 fork 点选 latest-descendant 赢家"。单链 / 多 root（含 /compact 切断的多树）/ 无分叉的会话**完全不折叠**；只有真正的 ESC 回退（同 parent 多个 child）才把被抛弃的兄弟子树折叠。
  - **链完整性**：jsonl 链里 attachment 和 system 记录夹在 user→assistant 之间（实测占 8% parent 指向）。后端把 attachment 也 emit 给前端（虽然不渲染卡片），系统级别保证 parent 链不断 → 主线判定正确。
  - **工具组**：tool-group 卡（连续 tool-only assistant）也写 data-uuid（取首条贡献者），跟 user/assistant 一起参与折叠 → 被回退段不会因 tool-group 被切成碎片。
  - 历史浏览器（Ctrl+H 进只读视图）同样支持。
  - 折叠/展开状态本地持有，刷新（F5）不丢。
  - 详 [`src/branching.ts`](src/branching.ts)、[`src/branch-fold.ts`](src/branch-fold.ts)。

- **single-instance lock**（issue #9）：同一个用户同一台机器只允许一个 cc-monitor 进程。第二次双击 `cc-monitor.exe`（或装多份 exe 双击别处那份）→ 第二个实例立即退出，第一个窗口被 unminimize + show + set_focus 拉到前台。修复历史上"两个 monitor 同时跑导致双重渲染 + cc 集成 race"的混乱。底层走 Tauri 官方 [`tauri-plugin-single-instance`](https://v2.tauri.app/plugin/single-instance/)，user-scoped mutex，跨用户登录不冲突。详 [`doc/INVARIANTS.md § 16`](doc/INVARIANTS.md)。

### 改进

- **设置面板折叠分组**（issue #7）：低频 section（外观 = 字体 + 颜色 13 字段；诊断）默认折叠到可展开 ▶ 分组里。设置面板纵向缩短超过 1 屏，找 PowerShell 集成不用再滚很远。展开/折叠状态本地持久（localStorage `cc-monitor.settings.collapsed.<id>`），重启 monitor 保留。展开动画用 grid-template-rows 0fr↔1fr 技巧，200ms 平滑过渡。高频 section（数据 / PowerShell 集成）保持默认展开。

---

## [2.0.0] — 2026-05-25

里程碑：补齐 GUI app 诊断短板。v1.7 系列结尾的 BOM bug "7 个版本带病发布无人察觉" 暴露的结构性问题（`windows_subsystem = "windows"` 无 stderr → tracing 输出不可见）在本版本彻底解决。

### 新增 — 诊断 / log 文件可视化（issue #4）

**背景**：cc-monitor 用 `windows_subsystem = "windows"` 编译，**没有 stderr 控制台**。所有 `tracing::warn!` / `tracing::error!` 用户和开发者都看不到。v1.7.0-1.7.7 的 BOM 解析失败（`bind: parse ... failed`）一直在打 warn，但 7 个版本带病发布无人察觉，cc 集成 "装上没用" —— 用户和开发者都没有任何反馈渠道。

本版本补齐这个结构性短板：

#### 滚动 log 文件
- 写到 `~/.claude/claudecode-frontend/logs/monitor.YYYY-MM-DD.log`
- 按天滚动，默认保留最近 3 天
- `tracing-appender` `non_blocking` writer，不阻塞业务线程
- log 文件失败时自动 fallback 到 stdout-only —— **monitor 启动绝不会被 log 卡住**

#### 设置面板「诊断」区
- ☑ 启用 log 文件（默认开；切换需要重启 monitor）
- 日志级别 [info ▼]（trace / debug / info / warn / error / off；**切换立即生效，无需重启**）
- ☑ 后端 ERROR 时显示右下角 toast（默认开；立即生效）
- log 文件路径 + 当前大小显示
- [打开 log 文件] / [打开 log 目录] / [刷新信息] 三个按钮

#### 后端 ERROR 红色 toast
- 任何 `tracing::error!` → 右下角红色 toast（headline = tracing target 如 `bind`，body = 完整 message）
- 多条 ERROR 垂直堆叠（不互相覆盖）
- 6 秒自动消失；**点击 toast 直接打开 log 文件**
- 限频 60 秒 / 20 条，避免错误风暴时屏幕被刷满

#### Config schema 扩展
`~/.claude/claudecode-frontend/config.json` 顶层新增 `diagnostics`：
```json
{
  "diagnostics": {
    "log_enabled": true,
    "log_level": "info",
    "error_toast": true,
    "max_files": 3
  }
}
```
所有字段 `#[serde(default)]` —— 老 v1.7.x 用户 config.json 无 diagnostics 字段时自动 fallback 到默认值，**完全向后兼容**。

### 新增 IPC

| 命令 | 参数 | 返回 | 说明 |
|---|---|---|---|
| `get_diagnostics_config` | — | `DiagnosticsConfig` | 设置面板拉当前配置 |
| `set_diagnostics_config` | `{ cfg }` | `RestartHint` | 写新配置；返回是否需要重启 |
| `get_log_file_info` | — | `LogFileInfo` | dir / current_file / size / all_files |
| `open_log_file` | — | `()` | 用系统默认编辑器打开当前 log |
| `open_log_dir` | — | `()` | 用资源管理器打开 log 目录 |

### 新增前端 / 后端模块
- `src-tauri/src/logging.rs` —— tracing init + 滚动 appender + EnvFilter reload + ErrorEmitterLayer + DiagnosticsConfig R/W（含 8 个单元测试）
- `src/error-toast.ts` —— listen `monitor-error` 弹堆叠 toast
- `src/settings/diagnostics-section.ts` —— 设置面板「诊断」区

### CSS
- 新增通用 `.ccm-toast-stack` / `.ccm-toast` / `.ccm-toast-error` 类（落实 INVARIANT § 12）
- 旧的 `#bring-terminal-toast` 保留作向后兼容；后续可重构复用 `.ccm-toast`

### 项目管理
- 新依赖：`tracing-appender = "0.2"`
- 单元测试 36 → 44（+8 logging tests）
- `tracing-subscriber` 现 init 走 `logging::init()` 而非 lib.rs 直接 `fmt().init()`

### 不破坏现有行为
- 不勾选诊断任何选项 → 行为跟 v1.7.13 一样（log 仍写但用户感知不到）
- 关掉 log 文件 → 老 log 文件保留，新内容不写
- 关掉 error toast → ERROR 仍写 log 文件，只是不弹 toast

---

## [1.7.13] — 2026-05-24

### 修复 — 设置面板 `?` tooltip 完全看不到

v1.7.12 改 right-anchored 后，靠左的 `?`（如"PowerShell 集成"标题旁）hover 时 tooltip 向左溢出 panel 左边界被裁。换 `position: fixed` + JS 算 viewport 坐标后**仍然看不到** — DevTools 显示 inline style 完全正确（`display: block; left: 735.946px; top: 161.223px; visibility: visible`），但 `getBoundingClientRect()` 实际给的 left 是 1476.5（viewport 外）。

**根因**：`.settings-panel` 有 `transform: translateX(0)` 做 slide-in 动画。CSS spec 规定：**祖先有 transform 时，position: fixed 后代的 containing block 从 viewport 重置到那个祖先**。我设的 `left: 735.946px` 不再相对 viewport，是相对 panel —— 视觉上跑到屏幕外。

**修法**：tooltip DOM 改成挂 `document.body`（不是 `?` icon 的子节点），脱离 .settings-panel 的 transform 子树，`position: fixed` 才真相对 viewport。把 makeInfoIcon + swapFileName 也拆到独立模块 `src/settings/info-icon.ts`，未来其他设置区可复用。

### 改进 — 启动 batch emit

`event_replay::replay_and_mark_ready` 之前对 history 里每条 jsonl 单独 `emit(JSONL_LINE, p)`，N=3000 时累计 ~400ms Tauri IPC 序列化 + 派发 overhead，阻塞主线程导致 F5/冷启动后白屏 + tabs 突然涌出的延迟感。

加 `JSONL_BATCH` 事件，replay 时单次 `emit(JSONL_BATCH, Vec<JsonlLinePayload>)` —— 一次序列化整个 Vec，前端 listener 拿到 array 后 push 进原批量 drain queue。实测启动到可交互省 200-400ms。`record()`（实时单条）走 JSONL_LINE 不变。

### 项目管理

- **删未用依赖**：`Cargo.toml` 的 `anyhow` + `thiserror` 全仓 grep 0 引用，纯死依赖。删了减编译时间 + 包体积。
- **`opener:allow-open-path` scope** 维持 `**`：考虑过收紧到 `$DOCUMENT/WindowsPowerShell/**` 但会破坏"Custom 路径"功能（用户可能选 Documents 外的位置）。
- 文档更新：发版后另起一次文档大重整（doc/CONTRIBUTING.md / ARCHITECTURE.md / IPC-PROTOCOL.md / INVARIANTS.md / STATE-MATRIX.md / DEVELOPMENT.md / BUILDING.md / RELEASING.md 等），覆盖测试列表 + 关键设计理由 + 跨进程协议 schema + 全局不变量。

## [1.7.12] — 2026-05-24

### 改动 — 设置面板 / PowerShell 集成 UX 修复 + 概念修正

#### Tooltip 溢出修复
- `?` 图标 hover tooltip 之前 `left: 50% + translateX(-50%)` 居中 + 320px 宽，靠右的 `?` 会让 tooltip 右半部分超出 360px 宽的设置面板被 `overflow-y: auto` 裁切。改成 right-anchored（`right: -4px`），宽度收到 240px max，永远向左展开不出 panel 右边界。

#### Legacy 文案中性化（修正概念错误）
- v1.7.2 起设置面板检测到 `profile.ps1` 有 cc-monitor 块时会弹"⚠ v1.7.0-1.7.1 旧位置遗留，PowerShell 启动时不读，实际无效"。**这个文案是错的**：`profile.ps1` = CurrentUserAllHosts，PowerShell 启动**会读它**（所有 host 都读）。
- 改成中性文案"ℹ 在 profile.ps1 (AllHosts) 也检测到 cc-monitor 块"，把判断权给用户：故意装那的话保留，重复安装的话清理一份。

#### Profile 路径下拉新增 AllHosts 选项
- 之前下拉只有 `PS 5.1 / PS 7.x / Custom`，默认指向 `Microsoft.PowerShell_profile.ps1`（CurrentUserCurrentHost，只有 powershell.exe 控制台读）。
- 新下拉 5 项：
  - `PowerShell 5.1 - $PROFILE（默认）` → Microsoft.PowerShell_profile.ps1
  - `PowerShell 5.1 - 所有 host（profile.ps1）` ⭐ 推荐：VSCode 终端 / ISE / SSH 都生效
  - `PowerShell 7.x - $PROFILE`
  - `PowerShell 7.x - 所有 host`
  - `自定义路径...`
- 旁边 `?` tooltip 解释 AllHosts vs CurrentHost 的实际差别。

#### 路径选择持久化
- 之前用户手动改 Profile 路径，关闭面板下次打开就被默认 PS 5.1 - $PROFILE 覆盖。
- 现在用 `localStorage` 持久化用户选的下拉项 + 自定义路径，下次打开恢复。

#### 备份机制说明
- [安装] 按钮加 hover 提示："v1.7.10+ 写入前自动备份原 profile 到 `<profile>.ccm-backup-<时间戳>`，写入失败自动回滚，用 Win32 ReplaceFileW 保留原 ACL"。

## [1.7.11] — 2026-05-24

### 修复 — [打开 profile] 按钮无效

设置面板 PowerShell 集成区的 [打开 profile] 按钮点了无效，alert 报：
```
打开失败: opener.open_path not allowed. Permissions associated with this command: opener:allow-open-path
```

**根因**：`src-tauri/capabilities/default.json` 里只有 `opener:default`，而它**不含** `allow-open-path`（实测 `gen/schemas/acl-manifests.json` 中 default permission set 是 `["allow-open-url", "allow-reveal-item-in-dir", "allow-default-urls"]`）。Tauri runtime 在 invoke `plugin:opener|open_path` 时直接拒。

**进一步坑**：单独加 `"opener:allow-open-path"` 仍不工作——`allow-open-path` 的 description 写明 "Enables the open_path command **without any pre-configured scope**"，默认 scope 为空 = 没有任何路径被允许打开。

**修复**：capability 用 inline scoped permission entry：

```json
{
  "identifier": "opener:allow-open-path",
  "allow": [{ "path": "**" }]
}
```

Tauri dev 模式实测：第一版改动后 alert 文本完全相同（permission denied），加 scope 后 [打开 profile] 直接用默认编辑器打开 .ps1（notepad / VSCode 等）。

## [1.7.10] — 2026-05-24 🚨 **紧急修复**

### 修复 — 严重事故：profile_installer 可能写坏用户 profile

v1.7.9 及更早版本在用户**已有内容的 PowerShell profile** 上点 [安装] 时存在两个事故路径，可能导致 profile 变 0 字节 / 普通用户读不了。**症状**：PowerShell 启动卡在 `Access to the path 'X' is denied` 报错，用户的别名/函数等全部失效。

### 两个根因

1. **非原子写**：`atomic_write_string` 走 `write(tmp) → remove(path) → rename(tmp, path)` 三步——如果 rename 因为 OneDrive 同步占用、杀软介入等失败，**原文件已被 remove** → profile 永久丢失。

2. **ACL 被覆盖**：即使 rename 成功，**tmp 文件 ACL（继承父目录）会替换掉 dst 上原有的 explicit ACE**。如果用户把 Documents 重定向到非默认盘（如 `E:\<user>\Documents`），父目录 ACL 通常只给 Administrators + Everyone 部分权限，没有当前用户的 explicit ACE——atomic replace 后用户自己都读不了自己的 profile。这是 v1.7.0–1.7.9 在某些机器上"装上后 PS 启动全报 Access denied"的真凶。

### 修复

1. **`atomic_write_string` 改用 Win32 `ReplaceFileW`** —— 这个 API 专门做"原子替换内容但**保留 dst 的 ACL / ADS / 创建时间**"。MoveFileExW 不保留 ACL，所以 v1.7.10 早期尝试用 MoveFileExW 修也不够。dst 不存在时 fallback 到 rename（首次安装，没东西可保留）。
2. **写之前必做 backup**：把原 profile 复制到 `<path>.ccm-backup-<ms>`，写入失败自动从 backup 恢复。备份文件保留给用户做最后手段。
3. **写之后回读校验长度**：不匹配从 backup 回滚并报错。
4. **`path.exists() == true` 但读到 `""` 时直接 abort**：不再用空字符串覆盖磁盘上有内容的文件（OneDrive placeholder / 文件锁等罕见场景）。
5. **`uninstall_from_profile` 加同样保护**：backup + 校验 + 回滚。
6. **新增 5 个端到端测试**：包括 `install_preserves_existing_user_content` 验证用户原内容不丢、`install_preserves_explicit_acl_entries`（Windows-only）验证 explicit ACE 被 ReplaceFileW 保留、`reinstall_replaces_block_keeps_user_content` 验证重装幂等。

### 受影响用户的应急步骤

如果你在 v1.7.0–1.7.9 装过 cc 集成后 PowerShell 启动报 `Access to the path … is denied`：

**情况 A — profile 完全无法读（普通用户和管理员都报错）**：
1. 用**文件资源管理器**（不要用 PowerShell）打开 profile 所在目录
2. 把 `Microsoft.PowerShell_profile.ps1` 和 `profile.ps1` 改名加 `.broken-bak` 后缀
3. 重启 PowerShell，错误消失；用 cmd `type` 看 `.broken-bak` 内容，抢救你自己的脚本

**情况 B — 管理员能读、普通用户报 access denied**（ACL bug）：
1. 用**管理员 PowerShell** 跑：`icacls "你的 profile 路径" /grant "$env:USERDOMAIN\$env:USERNAME:(F)"`
2. 这一条给你自己加一个 explicit Full Control ACE，普通 PS 立即能读 profile
3. 然后装 v1.7.10 再 [安装] cc 集成，新 ReplaceFileW 不会再吃 ACL

## [1.7.9] — 2026-05-24

### 改动 — 设置面板 / PowerShell 集成 UI 清理

- **"Wrapper 命令名"输入框移除**，命令名固定 `cc`：避免用户填错（最坑：填 `claude` → PowerShell function 跟 `claude.exe` 同名导致无限递归）。需要其他名字的用户直接编辑 profile。
- **新增 "同时安装 cc wrapper" 复选框**，**默认不勾选**：默认只装 `__ccm_bind` helper，不动用户已有的命令；勾选才装 `function cc { __ccm_bind; & claude $args }`。这样新用户的 profile 不会被无意中覆盖。
- **UI 干净化**：所有冗长 hint 改成 `?` 图标 hover 显示 tooltip。说明 = 想看才看；面板不再被 5-6 段说明文字塞满。
- 状态行加 `?` 图标解释"已注册 PowerShell session" 的语义（很多人误以为 0 就是没装好）。

### 文档

- 新增 `doc/ARCHITECTURE.md`：数据流图 + State 矩阵摘要 + 跨进程文件 IPC 协议表 + 设计分层 + 历史踩坑表。新贡献者第一站。
- `README.md` 新增"PowerShell 集成（可选）"章节，写清楚装 / 不装的影响，反映 v1.7.9 默认不勾选 wrapper 的新行为。
- `README.md` 安装包名示例从 `1.5.0` 改成 `<version>` 占位，避免每次 bump 都得改 README。
- `src-tauri/README.md` IPC 清单补全 v1.7 的 7 个命令（`bring_terminal_to_front` / `cc_integration_*` / `cc_*auto_launch`）；模块表加 `bind.rs` / `profile_installer.rs` / `auto_launch.rs`；不变量节加握手协议 + UTF-8 无 BOM 约束；工程坑节补 v1.7.0–1.7.1 profile.ps1 错位 + v1.7.8 BOM。
- `scripts/README.md` 提 `src-tauri/scripts/cc.ps1.tpl` 模板的存在。

## [1.7.8] — 2026-05-24

### 修复（v1.7.0–1.7.7 一直没修对的真凶）

- **PS 5.1 `Out-File -Encoding utf8` 写 UTF-8 BOM，serde_json 解析失败** ——
  这才是 cc 集成"装上没用"的真正根因。从 v1.7.0 起所有"修了又没用"的发版本质都是这个 bug，
  之前 v1.7.5 / v1.7.7 改的 `GW_OWNER` / `GetWindowTextLengthW` 都在 EnumWindows
  那一层，但**根本走不到那里**——`process_await_file` 在 `serde_json::from_str`
  那一步就 fail 了，直接删 await + return。
  - 实测：PS 5.1 `Out-File -Encoding utf8` 输出的文件前 3 字节是 `EF BB BF`（UTF-8 BOM）。
    `serde_json::from_str` 看到非 `{` 字符开头直接 `Err`。
  - 现象完美吻合：用户跑 cc 后 ps-await 被删（解析失败也删，避免重试）但 ps-registry
    永远不生成（fn 早 return 了）。
  - **修法 A**（核心）：`bind.rs::process_await_file` 读文件后
    `raw.trim_start_matches('\u{feff}')` 喂给 serde_json。一行兜底，任何 BOM/无 BOM
    UTF-8 输入都吃下。**已装 cc 集成的用户装 v1.7.8 monitor 立即 work，不需要重装 cc**。
  - **修法 B**（源头清洁）：`cc.ps1.tpl` 改用 `[System.IO.File]::WriteAllText` +
    `UTF8Encoding($false)` 显式无 BOM 写入。新装 cc 的用户拿到正确模板。
- 这是 v1.7.x 系列的最后一根稻草。**至此 4 层 bug 全部找出**：

| 版本 | bug | 实际"装上没用"原因 |
|---|---|---|
| v1.7.0–7.4 | 不知道 cc 没 work | 不知道 |
| v1.7.5 | 以为是 `GW_OWNER` 过滤过紧 | 修了但还没用——因为根本到不了那一步 |
| v1.7.7 | 以为是 `GetWindowTextLengthW` 对 WinUI 返 0 | 修了但还没用——同上 |
| v1.7.8 | **PS Out-File 写 BOM + serde_json 不剥 BOM** | **真凶**，修了立即 work |

### 教训

`tracing::warn!("bind: parse ... failed")` 在 GUI app（windows-subsystem = "windows"）
里**用户看不到**——v1.7.0 起这个 warn 一直在打，但没人能看到。下次必须给 GUI 加
本地 log 文件或者 IPC log 命令。**已加入** `doc/CONTRIBUTING.md` § 1.5 发版前 checklist。

## [1.7.7] — 2026-05-24

### 修复（接 v1.7.5 GW_OWNER 修复后发现的第二层 bug）

- **`GetWindowTextLengthW` 对 WinUI / Microsoft.UI.Xaml.Controls 控件返回 0** ——
  Windows Terminal 用的 XAML 控件（WT 1.18+ tab 容器之类）兼容 Win32 API 时有
  quirk：`GetWindowTextLengthW` 报"长度 0"（说"无 title"），但**实际有 title**——
  直接调 `GetWindowTextW(hwnd, buf, 512)` 给固定 buffer 能拿到。
  - v1.7.5 去掉 `GW_OWNER` 过滤后 monitor 能枚举到 WT XAML 子窗口，但
    `GetWindowTextLengthW` 返回 0 → title 当空字符串 → 永远 marker_match=false →
    跳过 → ps-registry 仍不生成。
  - 用户端诊断脚本用 `StringBuilder 512` 固定 buffer 调，能拿到 title，所以诊断
    脚本能找到 marker，但 monitor 找不到。两者行为不一致就是这里。
  - 修法：`find_window_for_marker` 的 callback **不再用 `GetWindowTextLengthW`**，
    直接固定 512 buffer 调 `GetWindowTextW`。marker 长度 ≤ 50 字符，512 buffer
    肯定够。

### v1.7.x cc 集成回顾

至此 cc 集成 4 个层级 bug 全部修完：
- v1.7.2：profile 文件名错（`profile.ps1` vs `Microsoft.PowerShell_profile.ps1`）
- v1.7.3：一键安装覆盖用户已有 `function cc`
- v1.7.5：`GW_OWNER` 过滤拒绝 WT XAML 子窗口
- v1.7.7：`GetWindowTextLengthW` 对 WinUI 控件 quirk 让 title 拿不到

## [1.7.6] — 2026-05-24

### 改动

- **Wrapper 命令名默认值改回 `cc`**（v1.7.5 改空又改回来）。
  placeholder 提示 "cc / ccm / 留空只装 helper"，仍允许留空 / 改别的。
  留空 + 已有同名 cc function 时的"只装 helper"逻辑保留。
  填 `claude` 阻止（防无限递归）保留。

## [1.7.5] — 2026-05-24

### 新增

- **"打开 profile"按钮** —— 设置面板 PowerShell 集成区加按钮，调系统默认编辑器
  打开当前路径的 profile（用 `tauri-plugin-opener`）。方便用户手动编辑 profile
  加 `__ccm_bind` 调用。

### 改动（UI 默认值调整）

- **Wrapper 命令名默认留空** —— 之前默认 `cc`，但 `cc` 是用户自己常用的别名，
  cc-monitor 不该默认抢这个名字。**新默认：留空**，placeholder "留空只装 helper（推荐）"。
  - 留空：只装 `__ccm_bind` helper，**不装任何 wrapper function**。
    用户在自己的 wrapper（如自定义 `cc` / `mc` / 直接在 prompt 里）调 `__ccm_bind` 即可。
  - 填名字（如 `ccm`）：装 `function 名字 { __ccm_bind; & claude $args }`。
  - **填 `claude` 时阻止**：弹 alert 警告——PowerShell function 跟 exe 同名时
    function 优先，会**无限递归**。
- 移除 v1.7.3 加的"也装默认 function cc"复选框——逻辑改成"命令名是否非空"，
  UI 更简洁。
- 介绍文案重写：不再假设用户用 `cc` 命令，引导用户"自己有 wrapper 就在里面调 `__ccm_bind`"。

### 修复（release-blocker，v1.7.0 起的）

- **cc 命令握手成功但 ps-registry 不生成** —— monitor 处理 ps-await 文件
  但 `find_window_for_marker` 返回 None，导致绑定永远建立不起来，Tab ↗
  始终报"未绑定窗口"。
  - 根因：`bind.rs::find_window_for_marker` 的 `EnumWindows` callback 过滤
    `GetWindow(hwnd, GW_OWNER) != 0` 的窗口（只看顶层无 owner 窗口）。
    这是从 v1.6.x 4-tier 算法继承的——当时为了排除 popup/dialog。
  - 实测：用户的 PS 是从 explorer 启动的，Windows Terminal 接管 console。
    `$Host.UI.RawUI.WindowTitle = $marker` 设的 title 同步到 **WT 内的
    Microsoft.UI.Xaml.* 子窗口（owner != 0，owner = WT 主窗口）**，
    而**不是** WT 主窗口本身。monitor 因为 owner 过滤直接跳过这些窗口。
  - 影响版本：v1.7.0 / v1.7.1 / v1.7.2 / v1.7.3 / v1.7.4 全部带病——
    cc 集成实际上从来没在 WT 接管 console 的常见场景下 work 过。
    单测全过 + 终端流程跑通 + 文件 trace 正确，但**窗口找不到**，binding
    永不生成。
  - 修法：去掉 `GW_OWNER` 过滤。marker 字符串 = `ccm-bind-{PID}-{UUID 8 char}`
    极独特，不需要 owner=0 这个"防 popup 误命中"的保险。

### 诊断脚本（如本次复现）

附 `ccm-diag.ps1`（本仓库外）可在用户 PS 跑：模拟 cc 握手并对比 PS 端 vs
monitor 端 `EnumWindows` 看到的窗口差异。本 bug 就是这样定位的——PS 端能找到
marker，monitor 端找不到 → 一定是过滤条件差异。

### v1.7.x 教训

v1.7.0-1.7.4 看似都"装上能用"，实际除非用户是从 WT 内开新 tab 启动 PS
（owner=0 那种），否则握手永远失败。这次 bug 之所以拖到 v1.7.5 才发现：
1. 自动化测试全是纯函数单测，没法测真实窗口枚举
2. monitor 处理 await 后 silent drop（没写 ps-registry 也没报错日志可见）

---

## [1.7.4] — 2026-05-24

### 修复（release-blocker，v1.6.7 起的回归）

- **历史浏览器打不开**："加载失败：state not managed for field `map` on command
  `list_history_projects`. You must call `.manage()` before using this command"。
  - 根因：v1.6.7 撤 `bring_terminal_to_front` 时把 `app.manage(session_map.clone())`
    一并删了，但 `history.rs::list_history_projects` 和
    `list_history_sessions_in_project` 也接 `State<Arc<SessionMap>>`，没补回去就 dead。
  - v1.6.7 / 1.7.0 / 1.7.1 / 1.7.2 / 1.7.3 都带这个 bug——单测过（不跑 IPC dispatch），
    我也没实测过历史浏览器。
  - 修法：lib.rs setup 补 `app.manage(session_map.clone())`。

## [1.7.3] — 2026-05-23

### 修复

- **v1.7.2 一键安装会覆盖用户已有的 `function cc`** —— 模板默认包含完整
  `function cc { __ccm_bind; & claude $args }`，安装到 profile 时由于
  PowerShell **后定义同名 function 覆盖前面**的机制，用户在 profile 中已有的
  自定义 `function cc`（含 cd / 代理 / 自定义参数处理等逻辑）会被无声覆盖。
  虽然 BEGIN/END 块外的代码本身没被改，但运行时实际生效的是 cc-monitor 的版本。

### 改动

- **模板拆成 `__ccm_bind` helper + 可选 `function cc` 两部分**
  - `cc.ps1.tpl` 用 `{{CC_FUNCTION_BLOCK}}` placeholder，`render_cc_code`
    根据 `include_cc_function` 决定是否填充
  - `__ccm_bind` 永远装（cc 集成的核心）
  - `function cc` 现在是**可选**部分
- **UI 智能默认值**：扫描结果发现 profile 已含自定义 `function {命令名}` 时
  自动取消勾选"也装默认 function cc"复选框，安装时跳过 cc function 段
- 用户已有 cc 时的指引：在 cc 开头加一行 `__ccm_bind` 即可。例如：
  ```powershell
  function cc {
      __ccm_bind                    # ← 加这一行
      if ((Get-Location).Path -eq $env:USERPROFILE) {
          Set-Location 'D:\Sync\文档\claude-conversation'
      }
      # ... 用户自定义代理 / 其他逻辑 ...
      claude @args
  }
  ```

### IPC 改动

- `cc_integration_preview({command_name, include_cc_function})` ← 新增 bool 参数
- `cc_integration_install({path, command_name, include_cc_function})` ← 新增 bool 参数

### 用户操作

v1.7.2 已安装 + 自定义 cc 被覆盖的用户：
1. 装 v1.7.3 → 启动 monitor
2. 设置面板 → PowerShell 集成
3. 扫描会发现你已有 `function cc` → 复选框自动取消勾选
4. 点"安装" → 只装 `__ccm_bind` helper（不动你的 cc）
5. 编辑 profile，在你的 `function cc` 开头加一行 `__ccm_bind`
6. 重启 PS

## [1.7.2] — 2026-05-22

### 修复（release-blocker）

- **v1.7.0/1.7.1 装错 profile 文件名导致 cc 集成形同虚设** ——
  - 错的：`Documents/WindowsPowerShell/profile.ps1`（CurrentUserAllHosts，PS 启动**不**自动读）
  - 对的：`Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1`（CurrentUserCurrentHost，即默认 `$PROFILE`）
  - 用户在 PS 里跑 `$PROFILE` 看到的就是后者。v1.7.0/1.7.1 装到前者 PowerShell 启动根本不加载，整个 cc 集成无效。
  - v1.7.2 `profile_installer::discover_profiles` 改用正确文件名。
  - 新增 `scan_legacy_profiles()` 检测 v1.7.0/1.7.1 错位的 profile.ps1 中是否含
    cc-monitor 块。UI 在状态扫描时显示警告 + 列出文件路径，引导用户手动清理。

### 改动（UX 大改）

- **设置面板"PowerShell 集成"区单卡片重构**：
  - PowerShell 版本下拉（Windows PowerShell 5.1 [默认] / PowerShell 7.x / 自定义路径）
  - profile 路径**可编辑输入框**（默认按版本下拉自动填充 `Microsoft.PowerShell_profile.ps1`，
    用户可手动改成任意路径——比如非标准的 OneDrive 同步路径、portable PowerShell、
    或者特殊 host 的 profile）
  - 选"自定义路径..."后路径输入框获焦让用户填
  - "重新扫描"按钮配合 flash 视觉反馈（之前点了没反应的设计 bug）
  - 状态徽章 (未安装/已安装/文件不存在)
  - 旧位置遗留警告框（紧贴主操作下方）
- **自动识别**：PS 5.1 永远显示（Windows 自带）；PS 7.x **只在 `Documents/PowerShell/` 目录存在时**才作为可选项展示，否则隐藏（绝大多数用户没装 7.x，UI 不再误导）

### 重构（后端 IPC）

- `cc_integration_install({path, command_name})` ← 之前 `{kind, command_name}` 改成接受路径直接
- `cc_integration_uninstall({path})` ← 同上
- 新增 `cc_integration_scan_path({path, command_name})` —— 用户改路径后扫描那个路径
- `cc_integration_status` response 新增 `legacy_profile_paths_with_block` 字段
- `ProfileKind` 加 `Custom` 变体

### 用户操作流（v1.7.2 安装）

1. 装 v1.7.2 后**首次启动 monitor**（auto-launch.json 会自动更新 monitor_exe_path）
2. 设置面板 → PowerShell 集成
3. 版本下拉默认 **PS 5.1**，路径已自动填 `Microsoft.PowerShell_profile.ps1`
4. 如果有 v1.7.0/1.7.1 遗留块，会看到"⚠ 检测到旧位置遗留" + 路径列表 → 手动用编辑器
   打开那个 profile.ps1 删除 BEGIN/END 之间内容（或整个文件删掉）
5. 点"预览代码"看完整内容
6. 点"安装" → 把 cc function 写到正确的 `Microsoft.PowerShell_profile.ps1`
7. **重启 PowerShell**
8. 跑 `cc` → 应该自动握手成功，Tab ↗ 能拉对应 WT 窗口

## [1.7.1] — 2026-05-22

### 新增

- **cc → 自动启动 monitor**（可选 toggle）—— v1.7.0 要求先开 monitor 后跑 cc，
  顺序反了 cc 会 fail-open（仍能启 claude，但没绑定）。v1.7.1 让 cc function
  能主动启动 monitor，但**不硬编码安装路径**（保持 portable exe 特性）：
  - monitor 每次启动调 `std::env::current_exe()` 写自身路径到
    `<monitor_data_dir>/auto-launch.json` 的 `monitor_exe_path` 字段
  - 用户移动 exe 后下次启动会自动更新（不需要重新装 cc function）
  - 设置面板新加 toggle "用 cc 启动 claude 时自动打开 monitor"
  - cc function 读 auto-launch.json：
    - `auto_launch_enabled` = true 且 monitor 没在跑且记录的路径存在 →
      `Start-Process` 启动 + `Start-Sleep -Milliseconds 2000` 等 watcher 起来
    - 已在跑（按绝对路径比对 Get-Process 的 .Path）→ 跳过启动
    - 任何检查失败 → fail-open（仍走握手，超时后 fail-open 启动 claude）
- 新 IPC：`cc_get_auto_launch` / `cc_set_auto_launch`
- 新模块 `src-tauri/src/auto_launch.rs`（含 3 个单测）

### 改动

- `scripts/cc.ps1.tpl` 加 auto-launch 段（读 auto-launch.json + Start-Process）
- 设置面板 PowerShell 集成区底部新增 toggle + monitor 路径显示

### 用户操作

第一次启用 auto-launch：
1. 至少启动一次 v1.7.1 monitor（让它记录自身路径到 auto-launch.json）
2. 设置面板 → PowerShell 集成 → 勾选 "用 cc 启动 claude 时自动打开 monitor"
3. 之后即使 monitor 没在跑，跑 cc 时会自动启动 monitor + 等 ~2s + 正常握手

## [1.7.0] — 2026-05-22

### 新增

- **cc 命令注入式绑定 Tab ↔ 终端窗口**——v1.6.x 的 4-tier 启发式算法在
  explorer 启 PowerShell + WT DefTerm 接管 console 的常见架构下不可靠（claude
  祖先链与 WT 窗口完全脱节）。v1.7 改成 PS 主动跟 monitor 握手：
  - 用户用 `cc` 命令替代 `claude` 启动会话（cc 是 PS function，包装 claude）
  - cc function 写 `ps-await/<PID>.json` + 设独特 WindowTitle marker
  - monitor 后台 watcher 调 EnumWindows 找含 marker 的窗口 → 拿到 hwnd
  - 写 `ps-registry/<PID>.json`（PS_PID ↔ hwnd 映射）→ 解除 PS 阻塞
  - 之后 claude 启动写 `sessions/<PID>.json`，monitor 用 ToolHelp 查
    claude.exe 的 parent_pid 反推 PS_PID → ps-registry → 拿 hwnd
  - 写永久 `sid-hwnd-cache.json`（含复合指纹：hwnd + owner_pid + procStart）
  - Tab ↗ / Ctrl+\` 查缓存 + 校验指纹 + SetForegroundWindow

- **设置面板"PowerShell 集成"区** —— 一键扫描 + 安装 + 卸载 cc function
  到 PS profile：
  - 同时扫描 PS 5.1 (`Documents/WindowsPowerShell/profile.ps1`) + PS 7.x
    (`Documents/PowerShell/profile.ps1`) 两个 profile 路径
  - 检测命令名冲突（profile 已有同名 function 时 UI 警告，建议改名）
  - 命令名可自定义（默认 `cc`，用户可输入 `ccm` / `monclaude` 等）
  - "预览代码"按钮弹 modal 展示完整将要写入的代码（含 BEGIN/END marker）
  - 块标记隔离：`# === cc-monitor BEGIN v1 ===` / `# === cc-monitor END ===`
    重装时整块替换、卸载时整块删除，用户在块外任何内容不动
  - 实时显示当前活跃 PS 注册数

- **rust 后端新增模块**：
  - `bind.rs`：BindRegistry（ps-await 监听 + EnumWindows + ps-registry 持久化）
    + SidHwndCache（sid → hwnd 持久化）+ verify_binding / activate 拉前
    + 心跳 10s 清死 PS 注册
  - `profile_installer.rs`：profile 路径解析 + 块插入/卸载 + 命令名冲突检测
  - `scripts/cc.ps1.tpl`：cc function 模板（include_str! 嵌入二进制）

- **rust 后端新增 4 个 Tauri IPC 命令**：
  - `cc_integration_status` — 扫描两个 profile 状态
  - `cc_integration_preview` — 渲染将要写入的代码（不修改文件）
  - `cc_integration_install` — 写入指定 profile（PS 5.1 或 PS 7.x）
  - `cc_integration_uninstall` — 移除 BEGIN/END 块
  - `bring_terminal_to_front` — 拉前命令（v1.6.7 删除后恢复，但实现完全重写）

### 改动

- Cargo.toml 恢复 `Win32_System_Diagnostics_ToolHelp`（用于 claude.exe →
  parent_pid 查询）+ `Win32_UI_WindowsAndMessaging`（EnumWindows / GetWindowTextW /
  SetForegroundWindow）feature
- 前端恢复 Tab ↗ 按钮 + Ctrl+\` 快捷键 + 失败时右下角 fixed toast

### 关键决策

| 决策 | 选择 | 理由 |
|---|---|---|
| profile 修改方式 | 一键安装 + 预览 + 卸载 | 默认便利但完全透明，BEGIN/END 块隔离不动用户其他内容 |
| 默认命令名 | `cc` | 短易记；UI 可改 |
| 没装 cc 时 | 报"未绑定窗口"不 fallback | 老 4-tier 算法已彻底删除 |
| 复合指纹 | hwnd + owner_pid + owner_proc_start + ps_proc_start | 防 HWND 复用 + PID 复用 |

### 用户操作流（首次安装）

1. 设置面板（Ctrl+,）→ 滚到"PowerShell 集成"区
2. 点 PS 5.1 或 PS 7.x 卡片的"安装"
3. 重启 PowerShell
4. 新 session 启动时 PS function 自动跟 monitor 握手（< 100ms，无感知）
5. 用 `cc` 替代 `claude` 启动会话
6. 之后 Tab ↗ / Ctrl+\` 直接拉对应 WT 窗口

### 设计文档

`D:/Sync/文档/cc-monitor-v1.7-cc-integration-plan.md`（plan + 时序图 + 数据结构）

## [1.6.7] — 2026-05-22

### 移除

- **`bring_terminal_to_front` 整条链路撤回**（v1.6.0–1.6.6 的"Tab ↗ 拉对应
  终端窗口"功能）。在 explorer 启 PowerShell + Windows Terminal DefTerm 接管
  console 的常见架构下，claude.exe 的祖先链与 WT 窗口完全脱节，4-tier
  启发式（祖先链 / 终端类进程 + title 匹配）无法可靠定位"哪个 WT 窗口跑了
  这个 session"。Ambiguous 报错让用户疲于配置独特 title，"Claude Code"
  fallback 又引入新歧义（误命中无 ai-title session 的同名窗口）。算法层修不
  动这个问题——需要 OS API 不暴露的"PowerShell PID ↔ WT HWND"映射。
  - Rust：删 `session_map.rs` 里 `bring_terminal_to_front` 方法 + 整个
    WindowMatcher（`SelectResult` / `MatchTier` / `build_ancestors` /
    `build_search_terms` / `classify_window` / `select_best_window` /
    `ProcInfo` / `WindowSnap` / `is_system_shell_process` /
    `is_terminal_process` / `process_info_snapshot` /
    `enumerate_top_level_windows` / `activate_window`）+ 14 个对应单测
  - `lib.rs`：删 `bring_terminal_to_front` Tauri 命令注册
  - `Cargo.toml`：删 `Win32_System_Diagnostics_ToolHelp` /
    `Win32_System_ProcessStatus` / `Win32_UI_WindowsAndMessaging` 三个 feature
  - 前端：删 `tabs.ts` 的 `bringActiveTerminalToFront` / `bringTerminalToFront` /
    `showBringTerminalToast` + Tab 上的 ↗ 按钮 + `main.ts` 的 Ctrl+\` 快捷键 +
    `styles.css` 的 `.tab-focus` / `#bring-terminal-toast` /
    `.status-msg.status-error`
  - 文档：删 `src-tauri/src/README.md`（专讲拉终端机制的设计文档）
- 保留 `SessionInfo.name` 字段（标记 `#[allow(dead_code)]`），为 v1.7 注入式
  绑定方案准备。

### 保留

- session_map.rs 心跳（2s 探活清死 session，v1.6.3 引入）
- watcher.rs force_rescan 通道 + SessionChange.added 字段（v1.6.3 引入，修
  /resume 竞态的 session 新增鲁棒重扫，跟拉终端无关）

### 下一步

v1.7 通过 `cc` 命令注入式绑定实现拉终端：用户用包装后的 `cc` 启动 claude，
wrapper 主动把 (sid, hwnd) 映射注册给 monitor，绕开"无法从进程树定位窗口"
的 OS 限制。

## [1.6.6] — 2026-05-22

### 修复

- **无 ai-title 的 session 拉前歧义** —— claude CLI 启动时默认 console title
  是 "Claude Code"，要等会话生成 ai-title 后才改成项目语义名。**没生成 ai-title
  之前**，对应的 WT 窗口 title 就是 "✳ Claude Code"。之前 `build_search_terms`
  只用 cwd / 项目名做匹配，没有任何 term 能命中 "Claude Code"——所有终端类
  窗口都 tier D，select 报歧义。
  - `build_search_terms`：当 `ai_title is None` 时把 `"Claude Code"` 加入 terms。
    "Claude Code" 窗口 title_match → 升 tier C (TerminalWithTitle)，唯一命中
    时 select 取它。其他有 ai-title 的窗口（"filter-active..." / "Analyze
    shengwu..."）仍 tier D，不参与竞争。
  - 角落情况：多个无 ai-title 的 session 并存 → 所有 "Claude Code" 窗口同 tier
    C 多候选 → 仍歧义，需要用户配独特 title（toast 提示）。

### 测试

- 单元测试 34 → 36。新增 2 个：`search_terms_include_claude_code_fallback_when_no_ai_title`
  + `search_terms_skip_claude_code_fallback_when_ai_title_present`。

## [1.6.5] — 2026-05-22

### 修复

- **点 ↗ 按钮 monitor 假死 + 消息区域被挤位**（强烈关联 Bug 1 "拉不起来"）——
  根因有两个，一起修：
  1. `bring_terminal_to_front` 是 sync `#[tauri::command]`。Tauri 2 sync 命令
     在 main IPC thread 跑（不是 spawn_blocking），命令期间整个 webview 假死
     不响应任何输入。改 `async` + 显式 `tokio::task::spawn_blocking` 包 Win32
     调用，IPC 主线程立即返回，webview 全程可点。
  2. v1.6.4 把错误写进状态栏文字（`statusMsg.textContent`）会触发 flex 重排，
     长错误字符让 `.status-msg` 内部 layout 变化，间接挤压上面的 message stream
     区域 → 用户看到"消息往右移动"。改 fixed 定位的 `#bring-terminal-toast`
     固定在右下角，完全脱离文档流，绝对不影响其他 element。
- **前端 invoke 加 5s timeout** —— 若极端情况下后端仍卡（如 EnumWindows
  callback 撞上 hung window），5s 后强制 reject 显示"invoke 超时"toast，
  不再让 monitor 看上去假死。

### 已废弃

- `.status-msg.status-error` CSS 规则保留但不再使用（v1.6.4 引入 + v1.6.5 替换）。

## [1.6.4] — 2026-05-22

### 修复

- **`bring_terminal_to_front` 失败时用户看不到原因** —— v1.6.3 加了
  Ambiguous / NoMatch 详细错误，但前端只 `console.warn` 没人开 DevTools。
  这次把后端 Err 字符串抬到状态栏显示 8s（红色 `⚠` 前缀 + `title` 属性
  保留完整文本，hover 可看截断前的全文）。现在"拉不起来"能直接读到
  "歧义：A 命中 4 个终端窗口 (sid=..., terms=[...])；候选: [...]；
  修复：在 PowerShell startup 给当前会话窗口设独特 title"。

## [1.6.3] — 2026-05-22

### 修复

- **多 Tab 拉错终端（同一窗口被反复选中）** —— Windows Terminal 单进程多窗口
  共享同一个 PID，所有 WT 窗口都"在 claude 的祖先链上"——`classify_window`
  把它们全部归到 tier A 或 tier B，而 `select_best_window` 旧实现"同 tier 只
  记第一个候选" 导致多个 session 撞同一窗口（EnumWindows Z-order 的第一个）。
  - 新增 `SelectResult { Single | Ambiguous | NoMatch }`：tier 内多候选时返
    `Ambiguous`，调用方报详细错（含命中 tier + 候选 hwnd/title + 配置建议）
    而非随机选一个。**拉错 → 拉不到，但用户得知该如何修**。
  - `build_search_terms` 加完整 cwd 路径作 term（含反斜杠 / 正斜杠两个版本）：
    用户在 PS startup 设 `$Host.UI.RawUI.WindowTitle = $PWD` 时能精确匹配
    每个会话独有的窗口。

- **关闭终端窗口后 Tab 不归档** —— Claude Code 异常退出时
  `~/.claude/sessions/<PID>.json` 可能不会被删，session_map 仅靠文件事件触发
  扫描 → 死 session 永远不发 `session-ended`。`session_map::run_watcher`
  加 **2 秒心跳**：`recv_timeout(2s)`，timeout 分支主动 `is_process_alive`
  探活所有 by_id 条目，死的自动 remove + emit removed → 前端 Tab 在 ≤2s 内灰显。

- **/resume 历史会话偶发不出现 Tab（多个并发时尤明显）** —— jsonl 行可能
  在 `sessions/<PID>.json` 之前到达 watcher；此时 `active(sid)` 返 false →
  `process_file` early return，且无任何机制重新触发该文件的扫描。新增
  **session-added → 强制重扫**安全网：
  - `SessionChange` 加 `added: Vec<String>`
  - `watcher::spawn_watcher` 返回 `WatcherHandle { rx, force_rescan_tx }`
  - lib.rs 收到 session_map 的 added 列表 → 通过 `force_rescan_tx` 通知
    jsonl-watcher 主动重扫该 session 的所有 jsonl 文件
  - jsonl-watcher 主循环改 `recv_timeout(100ms)` 兼容 rescan 通道（jsonl-line
    总延迟从 ~100ms 上升到 ~200ms，对流式渲染可接受）

### 测试

- 单元测试 29 → 34。新增 5 个：tier A 多候选 → Ambiguous、tier D 多候选 →
  Ambiguous、低 tier 唯一命中 → Single、完整 cwd 加入 terms、短 cwd 跳过完整路径。

## [1.6.2] — 2026-05-21

### 修复

- **`/compact` 等本地命令的 stdout 漏到 user 消息里渲染** —— Claude Code CLI
  把 `/compact` 写进 JSONL 时格式是 `<command-name>/compact</command-name>
  <command-message>compact</command-message><command-args></command-args>
  <local-command-stdout>Compacted...</local-command-stdout>`。v1.5 已过滤
  `<local-command-caveat>` 等 3 个标签但漏了 `<local-command-stdout>`，
  整条 user 消息因尾部多了一段无法匹配 slash 紧凑卡正则，回落到普通
  user 气泡把整段连同 stdout 一起渲染出来。这次：
  - 前端 `isInternalUserNoise` 重构为 `stripInternalNoise(text): string`
    返回剥过的文本（而非 boolean）；剥噪声列表补 `local-command-stdout`；
    user 分支用剥过的文本喂下游 `parseSlashCommand` / `buildUserCard`，
    `/compact` 现正确识别为 "⌘ /compact" 紧凑卡。
  - 后端 `history.rs::clean_user_text` 历史预览的 tag 列表同步补一项。

## [1.6.1] — 2026-05-21

### 修复

- **设置面板拖 color picker 卡顿** —— 每次 `input` 事件原本调 `applyTheme()`
  全量遍历 14 个 token 调 `setProperty`，60Hz 拖动下整棵 :root 子树重算被
  压垮。新增 `applyThemeToken(key, value)` 只更单 token；`onFieldChange`
  改调它。重算量降到 1/14。

### 新增

- **设置面板每行 "↺ 恢复默认" 按钮** —— 24×24 单项重置，仅回退该字段到
  styles.css :root 默认值。底栏的全量 "恢复默认" 按钮保留。

## [1.6.0] — 2026-05-21

v1.5.0 的迭代版。首次通过 `release.yml` 自动发布（v1.5.0 tag 指向的 commit
当时 release.yml 还未引入，无法触发自动 build → 跳过 v1.5.0 release）。

### 新增

- **历史浏览器"全量加载"按钮** —— 顶栏新增；点击后并发（max 4）拉取所有项目的会话详情进缓存。完成后搜索可命中 session 内容（ai-title / 自定义标题 / 首条消息 / sessionId）。状态条显示进度 `加载 N/M …`。

### 变更

- **图标改为纯字符**（去 emoji，避免跨平台字体差异）：
  - 顶栏历史按钮 `📜` → `◷` (U+25F7 时钟样圆形)
  - 重命名 `✏️` → `✎` (U+270E pencil)
  - 隐藏 `🙈` → `–` / 取消隐藏 `👁️` → `+`
  - 恢复 `↩️` → `↺` (U+21BA anticlockwise circle arrow)
  - 删除 `🗑️` → `✕` (U+2715 X)
  - 项目组前的 `📁` 移除（折叠指示器 `▸` 已够，多余）
  - **星标 `★/☆` 保留**（颜色高亮区分状态，且没有跨平台问题）
- **GitHub Actions CI** —— `.github/workflows/ci.yml`（push/PR 触发：rust fmt + clippy + test + frontend tsc + vite build）+ `release.yml`（`v*` tag 触发：tauri build + SHA256 + 自动 GitHub Release 发布）。
- **关键路径 tracing 埋点** —— `list_history_projects` / `list_history_sessions_in_project` / `read_session_jsonl` / `replay_and_mark_ready` 各加 elapsed_ms 日志，便于生产诊断慢点。

### 变更

- **TabBar 局部更新（refreshTabBar 差量 DOM）** —— 引入 `TabManager.tabButtons` 缓存：每个 Tab button 只创建一次，refresh 时只同步 class（active/archived/has-unread）+ 文本，按 `orderedIds` 顺序用 `insertBefore` 排序。Visibility 全交 CSS 控制。长 session 每秒数十次 `onLine` 时 DOM thrash 减少约 80%。
- **`TabManager.orderedIds: string[]`** —— 与 `tabs.keys()` 顺序一致的稳定数组，避免 `cycleActive` / `closeTab` 每次 `Array.from` O(N) 分配。
- **`session_map.bring_terminal_to_front` 重构** —— 160 行内嵌逻辑拆为 4 个纯函数（`build_ancestors` / `build_search_terms` / `classify_window` / `select_best_window`）+ `enum MatchTier`。主函数缩到 ~40 行做 orchestration。
- **`utils::days_from_civil`** —— `subagent.rs` 与 `history.rs` 各自的副本合并到新 `utils.rs`，单源。

### 修复

- `session_map.SessionInfo.status` / `SessionMap::load` / `SessionMap::get` / `SessionChange.added` / `messages::ContentBlock` 等死代码清理。cargo check 0 warnings。

### 测试

- 单元测试 15 → 29。新增 14 个覆盖 `build_ancestors`（链 / 环 / 缺失 parent）、`build_search_terms`（边界）、`classify_window`（5 个 tier 分支 + explorer 排除 + unrelated）、`select_best_window`（多 tier 共存 + 全无命中）。

---

## [1.5.0] — 2026-05-20

首个发 exe 的 release。

### 新增

- **历史会话浏览器**（顶栏 📜 / `Ctrl+H` / Esc）
  - 按**工作目录分组**展示所有历史 jsonl，项目默认折叠
  - **两级懒加载**：初次打开仅读项目级元数据（< 100ms，500 项目）；展开某项目才读其下会话详情；同项目再次展开秒开（缓存）
  - 操作：`★` 标星 · `✎` 重命名（中文 OK）· `–` 隐藏 · `↺` 恢复（拉起 wt.exe / cmd 跑 `claude --resume`）· `✕` 物理删除（二次确认）（v1.5 时是 emoji，v1.6 改纯字符）
  - 点击会话行进入**只读消息查看器**（复用实时 Tab 的渲染管线：Markdown / KaTeX / 代码高亮 / 折叠卡）；Esc 二级关闭（先关查看器再关视图）
  - 搜索框：匹配项目名 / 路径；已缓存项目附加匹配 ai_title / customTitle / first_user_excerpt
  - 用户元数据存 `<monitor_data_dir>/history-metadata.json`（永远在默认位置，不随 claudeDir 切换）

- **Claude 数据目录可配置**（设置面板 → 数据 → Claude 数据目录）
  - 三级回退：① 设置面板配置 `claudeDir` → ② `$CLAUDE_CONFIG_DIR` 环境变量 → ③ `~/.claude` 默认
  - 改后弹"需要重启 monitor"提示
  - 支持文件夹选择对话框（`tauri-plugin-dialog`）

- **vite 端口可配** —— `VITE_PORT` 环境变量覆盖默认 1420，HMR 端口自动 = port + 1

### 修复

- **鼠标光标卡死**（选中文本 / 关闭终端后偶发"鼠标卡为手型、点击无响应、滚动可用"）
  - 根因：jsonl-line 事件大量积压时主线程被 `marked.parse` + `hljs.highlightAuto` 同步渲染压垮
  - 修复：`events.ts` 改批量调度（≤40 条/批，≤8ms/批，`setTimeout(0)` 让出主线程）；`render.ts` 砍 `hljs.highlightAuto`（无 lang 时直接 escape，10kB 代码块 30-50ms → ~0ms）

- **resume 报错 0x80070002**（ERROR_FILE_NOT_FOUND）
  - 根因：旧代码 `wt.exe -d <cwd> pwsh -NoExit -Command "..."`，但 `pwsh.exe` 是 PowerShell Core 独立安装包，不是 Windows 自带
  - 修复：改用 `cmd /K "claude --resume <id>"`，`cmd.exe` 永远在系统目录可用；Plan B 用 `CREATE_NEW_CONSOLE` flag 兜底

- **关闭 Tab 后 DOM 引用残留**：`closeTab` 显式 `clear()` toolUseNames / toolUseElements Map，加速 GC

- **跨电脑硬编码路径**：`paths.rs` 抽出 `resolve_claude_dir()` 三级回退；`session_map.rs:147` 把 `cwd.rsplit(['\\','/'])` 换成 `Path::file_name()`

- **生产 panic 路径**：`watcher.rs:32` 把 `.expect()` 改成日志降级；`session_map.rs:78` `.ok()` 吞错改成 `tracing::error!`

### 变更

- **event_replay 取消 5000 条 cap** —— 历史塞全部，重启清
- **watcher 取消初始 1500 行截断** —— 全量读，由 event_replay 持锁保证顺序
- **HMR 强制 `window.location.reload()`** —— 避免部分热替换导致状态错乱
- **过滤 Claude Code CLI 内部 prompt 包装** —— `<task-notification>` / `<system-reminder>` / `<local-command-caveat>` / `<synthetic>` 不入消息流

### 打包

- `productName` 从 `Claude Code` 改为 `cc-monitor`（避免与 Anthropic 官方品牌冲突）
- `identifier` 从 `com.local.monitor` 改为 `com.ccmonitor.app`（稳定反域名）
- 新增 `publisher` / `copyright` / `longDescription` / NSIS `installMode: perMachine` + 中英双语
- 新增项目根 `LICENSE` (MIT)；`Cargo.toml` / `package.json` 补 metadata
- 删除 `tray-icon` feature（实际未使用）

---

## v1.5.0 之前 — pre-release dev 阶段（无独立 tag）

第一个公开发布是 v1.5.0（2026-05-20）。在那之前，所有功能（实时渲染 / 多 Tab / SessionMap 进程探活 / LaTeX + 代码高亮 / tool_use 折叠卡 / subagent / 设置面板 GUI / `bring_terminal_to_front` 4 阶段 HWND 启发式 / 撤销 SessionStart hook 路线 / 撤销 U2 焦点同步等）都在 dev 阶段内完成，由一个 Initial scaffold + 数十个 feat/fix commit 累积成 v1.5.0 的初始功能集。

详细的关键转向与设计演化叙事见 [`../doc/HISTORY.md`](../doc/HISTORY.md)（不在 git 仓库的工作目录文档）。
