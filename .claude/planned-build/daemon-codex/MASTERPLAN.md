# MASTERPLAN — daemon Codex 泛化（Phase 2D · 与 aterm 联合）

> **状态 = 草案（待用户审批 + 待与 aterm Kotlin 消费侧草案对拍）。** 审批前不写码。
> 本 workspace 承载 daemon（`remote-daemon-proto/`，我方仓 Rust）的 Codex 支持；monitor-local
> Codex（历史/用量/渲染）已在 `../codex-phase2/` 完成。与 aterm（`android-terminal`，只读）经 cc-bus 联合。

## 北极星
daemon 在**会话主机本地**发现 + 流式 + 判活 + per-kind 解析（turn-end/usage）**Codex 会话**，经 additive
wire 交 monitor / aterm 消费。**Claude 路字节零回归**（daemon-01~09 已上线的 Claude 行为不变）；**不 bump
`PROTO_VERSION`**（全 additive/skip_if_none/旧 client 忽略）。

## 关键调研结论（决定架构，均本机实测/读码核实）
1. **daemon 现状 = Claude-only 零 Codex**（daemon-01~09 骨架）：wire.rs `Frame`{Hello(claude_dir)/Line(byte_offset)/SessionAdded(pidfile 元)/SessionStatus/SessionRemoved/TurnEnd/Overflow}；watcher.rs 判活=Claude pidfile（`sessions/<PID>.json`+/proc starttime）；turn_detect=assistant+end_turn（golden-parity aterm）；usage/history/resolve 均 Claude。
2. **daemon↔monitor 不共享代码**（deliberate：daemon 在 `serde_json::Value` 上 golden-parity 重实现、无 typed model）→ Codex 逻辑 daemon **独立重镜像** monitor F2/F5（`codex_record`/`usage`），非复用。
3. **Codex 事实**（codex-cli 0.144.6，`../code-picture/codex-vs-claude-事实对照`）：sessions 日期分区 `<codex_dir>/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`；信封 `{timestamp,type,payload}`；turn-end=显式 `task_complete`/`turn_complete` 事件（uuid=turn_id，缺→envelope timestamp 回退）；usage=`token_count` 事件（last_token_usage 增量）；**无 pidfile → 判活退化 mtime 启发**（最硬缺口）。
4. **判活「fd 持开」= 未坐实假设**：/proc/<pid>/fd 指 rollout + logs_2.sqlite process_uuid + mtime 窗兜底——**必起真 codex 会话实测再定**（aterm 同机可交叉核）。DG2 的 gating 调查。

## 架构（per-kind，镜像 monitor 三层各缝纪律）
- **发现层**：watcher 按 kind 走各自记录根（Claude=`projects/**`、Codex=`sessions/YYYY/MM/DD/rollout-*`）。
- **判活层**：per-kind——Claude=pidfile（authoritative）、Codex=mtime 启发（heuristic）。wire 带 `liveness_confidence`。
- **解析层**：per-kind turn-end/usage 在 Value 上抽（golden-parity aterm 的 CodexTurnEndDetector / 用量 SPI）。
- **Line 转发不变**：daemon 仍逐行 raw 转发每一条 Line（不因 kind/分类丢行）；turn-end/usage 是 raw 之外额外算的信号。

## ★ 共享面账本（wire = 与 aterm 的跨产品契约 · 朝最终形态 · 禁打补丁）
**全 additive、不 bump PROTO_VERSION。我出 Rust 草案 → aterm Kotlin 消费侧审 → 锁字段名/值域。**（下方为**草案**，待对拍）

| wire 面 | 最终形态（草案） | 消费方 | 备注 |
|---|---|---|---|
| `Hello.codex_dir` | `Option<String>`，skip_if_none | monitor/aterm | 对称 claude_dir；Codex 未启用→缺省 |
| `Hello.emits` | 追加 Codex 专属帧 kind（若有） | aterm 门控 | 现 emits 机制不变（§26 外） |
| `SessionAdded.agent_kind` | `"claude"\|"codex"`，skip_if_none（缺=claude 兼容） | monitor/aterm | 会话属哪 kind |
| `SessionAdded.liveness_confidence` | `"authoritative"\|"heuristic"`，skip_if_none | aterm `authoritativeLiveness` | Claude=authoritative/Codex=heuristic |
| `SessionStatus.liveness_confidence` | 同上（状态变化时带） | aterm | mtime 判活的置信度 |
| `TurnEnd` | **不变**（`{session_id,uuid}`）；Codex uuid=turn_id（缺→envelope timestamp 回退） | aterm rolling+debounce | 与 aterm CodexTurnEndDetector 一字同 |
| `ResumeSpec.agentKind`（stdin） | `"claude"\|"codex"`，缺=claude | daemon resolve | → `codex resume <uuid>` vs `claude --resume` |
| `CommandPlan`（stdout） | 复用；command 按 agentKind 构建 | aterm ResumePlan | mode 仍 PtyInject |

## 功能拆分（依赖序 · 每个走 B→F 一圈 · daemon gate 含 `cargo clippy --all-targets`）
1. **DG3 · wire agent_kind/liveness_confidence/codex_dir（keystone·契约先行）**：定 additive 字段（账本最终形态），单测序列化/skip_if_none/旧 client 忽略。**先与 aterm 消费侧对拍锁定**再往下。产 Claude 侧仍缺省=零回归。
2. **DG1 · Codex 发现 + Line 流**：watcher per-kind——枚举/监视 `<codex_dir>/sessions` 日期树、tail rollout-*.jsonl、SessionAdded 带 agent_kind=codex + path/cwd（cwd 来自 session_meta）。（`.zst` 压缩会话 = 与 aterm 2C-3 对齐点，见 STATUS。）
3. **DG4 · Codex turn-end**：daemon turn_detect per-kind——`task_complete`/`turn_complete`（alias 归一）→ TurnEnd{uuid=turn_id 缺→timestamp}。golden-parity aterm CodexTurnEndDetector。
4. **DG5 · Codex usage**：daemon usage_query per-kind——token_count 的 last_token_usage 按 (model,天) SUM（镜像 monitor F5 字段映射 input−cached/cache_read/output）。`--usage` 回传行加 agent_kind。
5. **DG2 · Codex 判活（核心·gating 调查）**：**先起真 codex 会话实测** fd/sqlite/mtime 三路（aterm 同机交叉核）→ 定判活算法 → SessionAdded/SessionStatus 带 liveness_confidence=heuristic + alive 判定。撞不确定 → 停交用户/与 aterm 定。
6. **DG6 · Codex resume**：resolve_query per-kind——ResumeSpec.agentKind → 构 `codex resume <uuid>` command（子命令、无 unset）。golden-parity aterm ResumePlan。

## 依赖与顺序理由
- **DG3 wire 先行**（契约、双侧都依赖；先与 aterm 对拍锁定值域，避免返工）。
- **DG1 发现**是活体基座（先能发现+流 Codex 会话，turn-end/usage 才有 live 数据）。
- DG4/DG5 是纯解析（Value 上抽、可独立单测、golden-parity）。
- **DG2 判活**排后（依赖真机实测坐实假设；最硬、最可能停点）。
- DG6 resume 独立、可最后。

## 门槛 / 零回归红线（每功能 gate 绿）
- **Claude 路字节零回归**：daemon-01~09 的 Claude 发现/判活/turn-end/usage/resolve 行为不变（agent_kind 缺省=claude）。
- **不 bump PROTO_VERSION**（全 additive/skip_if_none）。§26 死循环护栏：capabilities/emits 规律不破。
- gate：daemon `cargo test` 不回归（基线待测）+ `cargo clippy --all-targets` 0 + fmt；wire 变更单测序列化 parity。
- **golden-parity 双写**：daemon 的 Codex turn-end/usage 与 aterm 对应 detector/SPI **逐字对拍**（同 turn_detect/usage_query 套路）。
- **Codex 事实本机实测/源码不臆测**；判活假设未坐实前不当既定。

## 联合协作（与 aterm · 批 D 节奏）
- **分工**：daemon Codex 逻辑我主（Rust）/ aterm 消费 wire 主（Kotlin DaemonTransport）。
- **wire 契约对拍**：我出 Rust 草案 → aterm Kotlin 消费侧草案 → 对拍字段名/值域/判活接口锁定（DG3 门禁）。
- **对抗性互审**：每功能双侧 golden-parity 对拍 + 不互相盖章（judue 判活假设、turn-end 边沿、usage 字段）。
- **判活实测**：DG2 起真 codex 会话，aterm 同机（同 `~/.codex`）交叉核 fd/sqlite/mtime。

## 变更记录
- 2026-07-19 建（草案）：2D 联合起步后，据「读 daemon 现码核实 + 与 aterm 4 问对齐」拟。待用户审批 + aterm 消费侧草案对拍。
