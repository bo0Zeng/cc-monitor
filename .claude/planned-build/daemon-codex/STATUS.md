# STATUS — daemon Codex 泛化（Phase 2D · 与 aterm 联合）

## 当前阶段
- **Phase A（masterplan）= 草案完成，待两道门**：① 用户审批 ② 与 aterm Kotlin 消费侧草案对拍锁 wire 值域。
- **审批前不写码**（planned-build 铁律 #7 + #10：masterplan 是与用户的硬门禁）。

## 自动模式
- **停 loop 交用户**：① masterplan 未审批（现状）② wire 契约待 aterm 对拍 ③ DG2 判活假设未真机坐实 ④ 计划-现实冲突 ⑤ 全完→Phase G。

## 已定（2D 联合起步 @2026-07-19，cc-bus 与 aterm 逐条确认）
- 分工：daemon Codex 逻辑我主（Rust）/ aterm 消费 wire 主（Kotlin）。
- wire additive 面（草案，见 MASTERPLAN 账本）：Hello+codex_dir、SessionAdded+agent_kind+liveness_confidence、ResumeSpec+agentKind、TurnEnd 不变、不 bump PROTO_VERSION。
- liveness 语义：aterm `authoritativeLiveness` ⟺ 我 `liveness_confidence:authoritative|heuristic`。
- 判活 fd 假设**未坐实**、DG2 真机实测再定（两端同机交叉核）。daemon opt-in（不自动部署）两端一致。

## 功能序（详见 MASTERPLAN）
DG3 wire（keystone·契约先行·对拍锁定）→ DG1 发现+Line → DG4 turn-end → DG5 usage → **DG2 判活（gating 调查·最硬）** → DG6 resume → Phase G。

## 下一个
1. **待用户审批 masterplan**（本 workspace Phase A 门禁）。
2. **待 aterm 带 Kotlin 消费侧草案** → 对拍 DG3 wire 字段名/值域/判活接口。
3. 双门过 → DG3 起（批 D 节奏 build + 对抗互审）。

## 回看
- 2026-07-19 建（草案）：monitor-local Codex（../codex-phase2 F1a/F5/F7）完成后，2D 联合起步。
