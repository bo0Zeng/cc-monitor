# STATUS — daemon Codex 泛化（Phase 2D · 与 aterm 联合）

## 当前阶段
- ✅ **Phase A masterplan 用户已审批**（2026-07-19「批准，按计划推进」）。
- ✅ **DG4 完成 + aterm 逐字对拍 verbatim-equivalent**（`daemon-DG4` 679f4c5）：`codex.rs`——Codex turn-end 检测器（is_codex_turn_end/codex_turn_end_uuid，turn_id 缺→envelope timestamp 回退），golden-parity aterm CodexTurnEndDetector（aterm 真读我 codex.rs 对抗核：子型集/uuid 回退链/坏信封/per-kind 隔离一字同）。daemon 99/clippy 0/Claude 零回归。staged（consumer=DG1/DG3）。
- ✅✅ **DG3 wire 已 build 完成**（`daemon-DG3` ea8bd99）：Frame Hello+codex_dir/kinds、SessionAdded+agent_kind/liveness_confidence、SessionStatus+liveness_confidence、ResumeSpec+agentKind（camelCase）。3 producer 现发 None/空（Claude 路，DG1 才填 Codex）。**Claude 帧字节零回归**（skip 省略、单测精确串锁）。daemon 102/clippy 0。**已 cc-send aterm 真序列化精确字节**（present/absent 形），aterm 消费侧（0bb21a1，已 build）据此交叉核 fixture。
- DG3 锁定契约（final spec，全 additive、不 bump PROTO_VERSION）：
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

## 下一个 = DG5 · Codex usage（daemon 侧 · 未阻塞纯解析）
- `codex.rs` 加 usage 助手（token_count 的 last_token_usage 字段 + turn_context model + 全零跳）+ `usage_query` per-kind（镜像 monitor F5：input=input_tokens−cached / cache_read=cached / cache_creation=0 / output=output_tokens，SUM last、跳全零 no-op、model 取 turn_context）。`--usage` 回传行加 agent_kind。
- **★ DG5 golden-parity 对拍点（aterm CodexUsageAggregator 2B 字段，落 DG5 时核）**：
  1. **output 含不含 reasoning**：我 monitor F5 `output=output_tokens`（**含** reasoning，OpenAI 语义 output=visible+reasoning）；aterm 单列 `reasoningOutput`。daemon `--usage` 出 `UsageTotals`（无 reasoning 字段、双写 parity）→ **DG5 须与 aterm 核 aterm 的 `output` 是否也含 reasoning**（含=一致；若 aterm output 剔除 reasoning 则总量口径差、需对齐）。
  2. **context-gauge 字段属 F1b 非 DG5**：aterm `lastContextTokens=last_token_usage.total`(非累计)、`contextWindow=model_context_window` 是**上下文占用**（F1b live 表、trap①），**不进 DG5 用量总量**（DG5 只 SUM last 各字段增量入 UsageTotals）。
  3. `costPartial=true`：Codex 用量只 token 不定价（同 monitor 硬边界），标记部分/仅 token。
  - 落了 cc-send aterm 对 CodexUsageAggregator 逐字核（它读我 Rust）。
- 之后：DG1 发现（接 DG3 agent_kind：watcher per-kind 走 codex_dir/sessions 日期树 + SessionAdded 发 agent_kind=codex + Hello 翻 kinds/codex_dir）→ **DG2 判活（gating：真机实测 fd/sqlite/mtime，aterm 同机交叉核已应承、撞不确定停交用户）** → DG6 resume（ResumeSpec.agentKind→codex resume）→ Phase G。

## 进度总览（2D，2026-07-19）
- ✅ Phase A masterplan（用户批准）· ✅ DG4 turn-end（aterm verbatim 对拍）· ✅ DG3 wire（aterm 消费侧并行 build、真字节已交叉核）
- ⏭ DG5 usage（next，未阻塞）· DG1 发现（接 DG3）· DG2 判活（真机 gating，aterm 同机核）· DG6 resume
- aterm 侧：2C β 全完 + DaemonTransport DG3 消费（0bb21a1）+ CodexUsageAggregator(2B) 待我 DG5 对拍。

## 回看
- 2026-07-19 建（草案）：monitor-local Codex（../codex-phase2 F1a/F5/F7）完成后，2D 联合起步。
