# MASTERPLAN — cc-monitor Phase 2：Codex 泛化（AgentKind 第二刀）

- **性质**：planned-build 主计划。用户 2026-07-18「两产品一起适配 Codex，全面调研 → Draft masterplan → 直接授权全自动开跑；每个关键决策点不清楚就开 agent web 搜索」。
- **协作**：与 android-terminal（aterm）并行——aterm 侧 SPI masterplan 已过其用户门禁（2A→2D），**wire 共享面落 aterm 2D 联调**。跨项目走 cc-bus `#codex`，事实源 = `code-picture/codex-vs-claude-事实对照_2026-07-18.md`。
- **调研底座**：三 agent 全面调研（架构缝 / openai/codex 源码+web / 本机 ~/.codex sqlite 深挖）——见事实对照 doc「深研补充」A–D。

## 北极星
把 cc-monitor 从 **Claude-only** 泛化到 **also 支持 Codex CLI 会话**，通过**激活既有的 `AgentKind` 延迟缝**（`adapter.rs`/`agent-profile.ts` 明写「第二个 agent 落地才动记录模型」——Codex 就是这第二个）。**Claude 零回归是硬红线。**

## 关键调研结论（决定架构）
1. **缝已在**：monitor `src-tauri/src/adapter.rs`（`AgentAdapter` trait + `SessionLayout` + 单一 `ClaudeCodeAdapter`）、frontend `src/agent-profile.ts`（`AGENT_PROFILE`）是刻意留的浅缝；**daemon `remote-daemon-proto/` 无缝**（独立 crate、双写 parity）。
2. **keystone = 记录解析**：引入中立 `CanonicalRecord` + per-kind 解码器；turn-end/usage/UI 全坐其上。
3. **判活有解但软**：Codex 无 pidfile，但 `logs_2.sqlite` 的 `process_uuid=pid:<PID>:<uuid>` 给 session→PID，`/proc/*/fd` 开着 rollout（源码坐实全程持开）= 活。DEAD 高信心 / ALIVE 中信心。**wire 带 per-kind liveness confidence**。
4. **格式 churn 高**：rollout 未文档、每几版变（`task_*` 是 `turn_*` 的 v1 别名）→ **maximally defensive parser**（逐行不崩、字段全 optional、alias 归一、吃 `.jsonl`+`.jsonl.zst`、call_id 配对、token_count 当噪音）。
5. **别信 sqlite 当权威**（state_5 已知 stale）→ 枚举扫 `sessions/YYYY/MM/DD/` 目录，sqlite 只当 fast-path。

## 架构（三层各缝、双写 parity，同现状纪律）
- **monitor**（`src-tauri/`）：扩 `AgentAdapter` trait 补延迟方法（`decode(line)->CanonicalRecord` / `is_turn_end` / `usage_of` / `liveness`）；`active()`→`for_kind(AgentKind)`；加 `CodexAdapter` + Codex `SessionLayout`。引入中立 `CanonicalRecord`。
- **daemon**（`remote-daemon-proto/`）：加它缺的缝——`--agent codex` selector（**受 §26 strippable 护栏** + `every_capability_token_is_strippable` 覆盖），切 `resolve_dir`/layout/`turn_detect`/`usage_query`/liveness 半。`CanonicalRecord` 跨 crate 镜像 + parity 测（同 `turn_detect`/`usage`/`SeqCounter` 现有双写）。
- **frontend**（`src/`）：`AGENT_PROFILE`→per-kind lookup；Codex record 解码器喂现有 `renderMessage`（Responses-API `message/reasoning/custom_tool_call` → 中立单元）。

## ★ 共享面账本（≥2 功能改到 · 朝最终形态实现、禁打补丁）
| 共享面 | 触及功能 | 最终形态 |
|---|---|---|
| **`AgentKind{Claude,Codex}` enum** | 全部 | 三层各定义（daemon/monitor/frontend）；monitor 从 config/discovery 选，daemon 从 `--agent` argv，frontend 从帧的 agent_kind |
| **`CanonicalRecord`（keystone）** | F2/F3/F5/F7 | 中立记录模型：{kind, role, content-units, uuid, turn markers, usage}；monitor + daemon 镜像 + parity 测；Claude/Codex 各解码到它。Claude 侧**不改 parse_line 的 F63 零信息损失**、在其上加 canonical 投影 |
| **wire：SessionAdded/SessionStatus + `agent_kind` + `liveness_confidence`** | F1/F4 | additive 字段（旧 client 忽略）；`agent_kind:"claude"\|"codex"`、`liveness_confidence:"authoritative"\|"heuristic"`。**落 aterm 2D 联调互审**、不 bump PROTO_VERSION |
| **wire：ResumeSpec/CommandPlan + `agent_kind`** | F6 | `--resolve` 按 kind 出命令（`claude --resume`/`codex resume <uuid>`）。additive |
| **Codex `SessionLayout`/定位** | F1（daemon+monitor+daemonless SSH） | 日期分区扫 `sessions/YYYY/MM/DD/rollout-*.jsonl(.zst)`；sid 从文件名 UUID（非 stem）；无 cwd-项目目录概念 → `list_sessions` 走日期树 |
| **Codex defensive parser** | F2/F5/F7 | 单一解码器：alias 归一、field-optional、.zst、call_id 配对、容缺 world_state。monitor + daemon 双写 |
| **Codex liveness provider** | F4 | `logs_2.process_uuid`→PID + `/proc/<pid>`(comm≈codex) + rollout mtime + in-progress-turn；daemon 远端读远端 ~/.codex；confidence=heuristic |

## 功能拆分（依赖序 · 每个走 B→F 一圈）
- **F1 · AgentKind + Codex 定位/发现**（Axis1，独立、先做）：`AgentKind` enum 三层；`CodexAdapter`+Codex `SessionLayout`；`active()`→multi-kind；daemon `--agent` selector（§26）；daemonless SSH find 日期树；monitor discovery 走 sessions/日期。**不碰记录模型**（只定位）。
- **F2 · CanonicalRecord + Codex defensive parser**（Axis2，keystone）：定义 `CanonicalRecord`（monitor+daemon 镜像+parity）；Codex 解码器（envelope unwrap、alias、.zst、全 taxonomy、call_id）；Claude 侧 canonical 投影（parse_line 之上、零回归）。
- **F3 · Codex turn-end**（Axis3，on F2）：per-kind `turn_detect`——Codex `task_complete`→TurnEnd(uuid=turn_id)、`turn_aborted`→静默（aterm 决策）；daemon 发帧 + frontend `turn-notify` per-kind；Claude 谓词 golden 不变。
- **F4 · Codex 判活**（Axis4，最险、可与 F2 并行）：liveness provider（logs_2 PID / /proc-fd / mtime / in-progress-turn，confidence=heuristic）；SessionStatus 数据源 per-kind（Codex 无 pidfile status/waitingFor）；wire 带 liveness_confidence。**判活假设（fd 持开）建到此步起真 codex 会话 `lsof`/`/proc/fd` 实测坐实**（源码已指向持开）。
- **F5 · Codex 用量**（Axis5，on F2）：per-kind usage extractor——Codex `token_count`（total/last，含 reasoning_output）；monitor `usage.rs` + daemon `usage_query.rs` 双写加 Codex 分支；`UsageTotals`/`SessionUsageRow` 复用。
- **F6 · Codex resume/resolve**（Axis6，近完成）：daemon `resolve_query` per-kind 出命令（`codex resume <uuid>`，subcommand 非 flag）；退 `cc-<sid8>` Claude 品牌名 or per-kind；adapter `resume_flag` 支持 subcommand 形；ResumeSpec/CommandPlan 加 agent_kind。
- **F7 · Codex UI 渲染**（UI，on F2）：Codex Responses-API 解码器（message.output_text/reasoning/custom_tool_call）→ 中立渲染单元；`renderMessage` per-kind 分支或 canonical 前置归一；工具配对按 call_id（Codex 无 Claude tool_use↔tool_result 结构）。
- **（联调）wire 共享面**：F1 的 agent_kind + F4 的 liveness_confidence + F6 的 ResumeSpec agent_kind → **aterm 2D 按批 D 节奏联调 + 对抗性互审**。

**顺序理由**：F1 独立先行解锁定位；F2 keystone 第二（F3/F5/F7 坍缩成「分类/抽取 canonical」）；F4 险、与 F2 并行推进；F6 近完成可早收；F7 依赖 F2。

## 门槛 / 零回归红线（每功能 gate 绿）
- **Claude 零回归**：`claude_layout_locked` / `parse_line` F63 零信息损失 / `turn_detect` aterm golden 逐字 / usage 双写 parity（requestId MAX 不变）/ `cc-monitor-unrecognized` wire 契约 / line framing（byte_offset/seq）agent-agnostic 复用不 fork。
- **wire additive**：不 bump PROTO_VERSION；`--agent` 需 `split_stream_flags` 剥离分支 + `every_capability_token_is_strippable` 覆盖（§26 防重连死循环）。
- **gate**：daemon `cargo test` + `cargo clippy --all-targets` + fmt；monitor cargo 不回归；vitest + tsc + build；每功能 DoD 含实际验证。
- **Codex parser**：maximally defensive（format churn 高）；加跨版本回归语料。
- **决策纪律**：关键决策点不清 → 开 agent web 搜索（不臆测）；Codex 事实一律本机实测/源码，不训练回忆。

## loop（自动驱动）
用户直接授权全自动。每轮推进一个功能 C→F（实现→代码审计→工程审计→回看）；停 loop 条件（planned-build 铁律）：跨项目待 aterm 回（wire 共享面在 aterm 2D）/ 计划-现实冲突 / 同一步失败两次 / 全部完成→Phase G。**F4 判活真机实测**、**wire 共享面 aterm 2D 联调**是天然协调点。

## 变更记录
- 2026-07-18 建，源自三 agent 全面调研 + 用户直接授权全自动开跑。
