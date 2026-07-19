# STATUS — daemon Codex 泛化（Phase 2D · 与 aterm 联合）

## 当前阶段
- ✅ **Phase A masterplan 用户已审批**（2026-07-19「批准，按计划推进」）。
- **DG3 wire 待 aterm Kotlin 消费侧草案对拍**（wire 值域锁定前不定稿）→ **再排：先做未阻塞纯解析层 DG4/DG5**（blocker 驱动的 Phase F 调整，功能/架构不变、仅顺序）。
- ✅ **DG4 完成**（`daemon-DG4` 679f4c5）：`codex.rs` 模块——Codex turn-end 检测器（is_codex_turn_end/codex_turn_end_uuid，turn_id 缺→envelope timestamp 回退），golden-parity aterm CodexTurnEndDetector，7 测。daemon 99/clippy 0/Claude 零回归。staged（consumer=DG1/DG3）。**已 cc-send aterm 请 Rust↔Kotlin golden-parity 对拍。**

## 自动模式
- **停 loop 交用户**：① masterplan 未审批（现状）② wire 契约待 aterm 对拍 ③ DG2 判活假设未真机坐实 ④ 计划-现实冲突 ⑤ 全完→Phase G。

## 已定（2D 联合起步 @2026-07-19，cc-bus 与 aterm 逐条确认）
- 分工：daemon Codex 逻辑我主（Rust）/ aterm 消费 wire 主（Kotlin）。
- wire additive 面（草案，见 MASTERPLAN 账本）：Hello+codex_dir、SessionAdded+agent_kind+liveness_confidence、ResumeSpec+agentKind、TurnEnd 不变、不 bump PROTO_VERSION。
- liveness 语义：aterm `authoritativeLiveness` ⟺ 我 `liveness_confidence:authoritative|heuristic`。
- 判活 fd 假设**未坐实**、DG2 真机实测再定（两端同机交叉核）。daemon opt-in（不自动部署）两端一致。

## 功能序（详见 MASTERPLAN）
DG3 wire（keystone·契约先行·对拍锁定）→ DG1 发现+Line → DG4 turn-end → DG5 usage → **DG2 判活（gating 调查·最硬）** → DG6 resume → Phase G。

## 下一个 = DG5 · Codex usage（daemon 侧 · 未阻塞纯解析）
- `codex.rs` 加 usage 助手（token_count 的 last_token_usage 字段）+ `usage_query` per-kind 分支（镜像 monitor F5：input=input_tokens−cached / cache_read=cached / cache_creation=0 / output=output_tokens，SUM last、跳全零、model 取 turn_context）。golden-parity aterm 用量 SPI。`--usage` 行加 agent_kind（DG3 wire 面、但 --usage 是独立 stdout 非 Frame，可先出 Codex 行、agent_kind 字段名待 DG3 锁）。
- **DG3 wire 仍待 aterm 消费侧草案**：aterm 回草案 → 对拍字段名/值域/判活接口 → 锁 DG3。
- 之后：DG1 发现（需 DG3 agent_kind 接线）→ **DG2 判活（gating：真机实测 fd/sqlite/mtime，两端同机交叉核、撞不确定停交用户）** → DG6 resume → Phase G。

## 回看
- 2026-07-19 建（草案）：monitor-local Codex（../codex-phase2 F1a/F5/F7）完成后，2D 联合起步。
