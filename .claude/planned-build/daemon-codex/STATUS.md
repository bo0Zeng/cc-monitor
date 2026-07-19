# STATUS — daemon Codex 泛化（Phase 2D · 与 aterm 联合）

## 当前阶段
- ✅ **Phase A masterplan 用户已审批**（2026-07-19「批准，按计划推进」）。
- ✅ **DG4 完成 + aterm 逐字对拍 verbatim-equivalent**（`daemon-DG4` 679f4c5）：`codex.rs`——Codex turn-end 检测器（is_codex_turn_end/codex_turn_end_uuid，turn_id 缺→envelope timestamp 回退），golden-parity aterm CodexTurnEndDetector（aterm 真读我 codex.rs 对抗核：子型集/uuid 回退链/坏信封/per-kind 隔离一字同）。daemon 99/clippy 0/Claude 零回归。staged（consumer=DG1/DG3）。
- ✅ **DG3 wire 已与 aterm 对拍锁定**（final spec 见下）——DG3 现**解锁可 build**。
  - Hello: +`codex_dir:Option<String>`(skip_if_none) +`kinds:Vec<String>`(skip_if_empty，如 ["claude","codex"])
  - SessionAdded: +`agent_kind:Option<String>`(Codex 发 "codex"；Claude 省→缺=claude) +`liveness_confidence:Option<String>`(Codex 发 "heuristic"；Claude 省→缺=authoritative)
  - SessionStatus: +`liveness_confidence:Option<String>`
  - ResumeSpec(stdin): +`agentKind`(**camelCase**——resolve I/O 面全 rename_all=camelCase、对齐 aterm 数据类；Frame wire 才 snake_case) 缺=claude → daemon 构 `codex resume <uuid>`
  - TurnEnd 不变；liveness 映射 authoritative↔true/heuristic↔false/**缺↔true**（向后兼容）；**全 additive、不 bump PROTO_VERSION**。

## 自动模式
- **停 loop 交用户**：① masterplan 未审批（现状）② wire 契约待 aterm 对拍 ③ DG2 判活假设未真机坐实 ④ 计划-现实冲突 ⑤ 全完→Phase G。

## 已定（2D 联合起步 @2026-07-19，cc-bus 与 aterm 逐条确认）
- 分工：daemon Codex 逻辑我主（Rust）/ aterm 消费 wire 主（Kotlin）。
- wire additive 面（草案，见 MASTERPLAN 账本）：Hello+codex_dir、SessionAdded+agent_kind+liveness_confidence、ResumeSpec+agentKind、TurnEnd 不变、不 bump PROTO_VERSION。
- liveness 语义：aterm `authoritativeLiveness` ⟺ 我 `liveness_confidence:authoritative|heuristic`。
- 判活 fd 假设**未坐实**、DG2 真机实测再定（两端同机交叉核）。daemon opt-in（不自动部署）两端一致。

## 功能序（详见 MASTERPLAN）
DG3 wire（keystone·契约先行·对拍锁定）→ DG1 发现+Line → DG4 turn-end → DG5 usage → **DG2 判活（gating 调查·最硬）** → DG6 resume → Phase G。

## 下一个 = DG3 wire build（已锁·解锁）→ 然后 DG5 usage
- **DG3**：wire.rs 加 Hello.codex_dir/kinds + SessionAdded.agent_kind/liveness_confidence + SessionStatus.liveness_confidence（全 additive skip_if_none/empty）；resolve_query.rs ResumeSpec +agentKind（camelCase）。单测序列化/skip 缺省/旧 client 忽略 parity。**不 bump PROTO_VERSION**。Claude 帧字节不变（Codex 字段省→等价旧帧）。好了 cc-send aterm → 它 build 消费侧。
- **DG5**：`codex.rs` 加 usage 助手（token_count last_token_usage）+ `usage_query` per-kind（镜像 monitor F5：input−cached/cache_read/cache_creation=0/output，SUM last、跳全零、model 取 turn_context）。golden-parity aterm CodexUsageAggregator(2B)——落了 cc-send aterm 对拍。`--usage` 行加 agent_kind。
- 之后：DG1 发现（接 DG3 agent_kind：watcher per-kind 走 codex_dir/sessions 日期树 + SessionAdded 发 agent_kind=codex）→ **DG2 判活（gating：真机实测 fd/sqlite/mtime，aterm 同机交叉核已应承、撞不确定停交用户）** → DG6 resume（ResumeSpec.agentKind→codex resume）→ Phase G。

## 回看
- 2026-07-19 建（草案）：monitor-local Codex（../codex-phase2 F1a/F5/F7）完成后，2D 联合起步。
