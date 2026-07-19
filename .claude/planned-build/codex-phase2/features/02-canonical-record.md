# F2 · Codex 记录层（keystone）

## 设计岔口 → **已定：第三条路（aterm 2A 对齐，2026-07-18）**
调查（读 messages.rs `JsonlRecord`）发现 Claude 记录模型深度 CLI 专属、紧耦合前端。原 A（全统一新
CanonicalRecord）大重构、B（per-kind 两套渲染路）。**aterm 2A 给了更好的第三条路、cc-monitor 采纳**：
- **渲染轴 = 复用现有 `JsonlRecord`**：加 `CodexRecordParser` 把 Codex **映射进现有 JsonlRecord**（不建新
  中立模型）→ 下游 catalog/MainBranch/前端 `renderMessage` 消费 JsonlRecord、**不分 kind、零 re-plumb、
  Claude 零回归**（`parse_line` body 不动、加 Codex 分支）。省两套渲染路。
- **turn-end/用量轴 = per-kind**：Codex `event_msg`/`token_count` 是**事件非消息记录**、不映射进
  JsonlRecord → 落 `Unknown`(保 rawJson)；turn-end/用量走 per-kind（F2a accessor `turn_id`/
  `token_usage_last` 从 rawJson 读；F3/F5 接）。
- **taxonomy 两端一致**（aterm 确认）：Message/Reasoning/ToolCall/ToolResult ↔ ContentBlock Text/
  Thinking/ToolUse(id=call_id)/ToolResult(toolUseId=call_id)；role user/assistant→User/Assistant+Text、
  developer→User(isMeta)、reasoning→Thinking。利互审 + 将来共享 golden。

## DoD（分 slice）
- [x] **F2a · Codex 防御分类器**（`codex_record.rs`）：`classify(Value)->CodexRecordKind`（顶层 5 type +
  event_msg/response_item 子型，alias `turn_*`↔`task_*` 归一，未知→Other 不崩）+ accessor（turn_id/
  token_usage_last/message_role）+ golden 测（本机真实 shape + 防御 case）。**staged `#![allow(dead_code)]`**。
- [x] canonical 岔口决定 + aterm 对齐 = **第三条路**（见上）。
- [ ] F2b · **`CodexRecordParser`：Codex → 现有 `JsonlRecord` 映射器**（message→User/Assistant+ContentBlock、reasoning→Thinking、custom_tool_call→ToolUse(id=call_id)、custom_tool_call_output→ToolResult(toolUseId=call_id)、developer→User isMeta；event_msg/token_count→Unknown 保 rawJson）。接进 `parser.rs` 的 Codex 分支（Claude 分支不动、零回归）。本机 rollout 作 fixture、映射口径与 aterm 互审。

### ⚠️ F2b 真数据坑（aterm Phase D 审计真 ~/.codex 抓出、我建时必守，别用 String fixture 自欺）
1. **`custom_tool_call_output.output` 恒数组** `[{type:input_text,text:...}]`（aterm 9/9 实测、无一 String/object）→ 必须**按数组拼 `content[].text`**（对齐 Claude `flattenToolResult` 的 List 分支）。当 String 处理会 else→"" **静默丢全部工具返回文本**（且 String fixture 会绿着骗过）。**fixture 用真机数组 shape。**
2. **`message.content` 也是数组** `[{type,text}]` → 抽 text 从数组项；`input_image` 等非文本项 → text 空。
3. **`reasoning.summary` 恒 []**（仅 encrypted_content）→ 空文本时**给空 blocks、不产 `Thinking("")`**（避空 Thinking 渲染噪音）。有 summary text 才产 Thinking。
4. **developer → User(isMeta=true)**（系统指令/元、渲染隐藏，同 Claude isMeta）。
5. **Codex 无 parentUuid；无 payload.id 的记录 uuid=null**（user/developer/tool_output）→ **MainBranch parentUuid 串链对 Codex 失效** → 定位/排序改**文件序 + timestamp**（F1 发现层重构 + F3 都按此，勿套 Claude 链）。
6. **role:user 但正文 `<environment_context>` 注入上下文** → meta 去噪（cc-monitor 有类似 noise 过滤、对齐）。
- **互审**：先只读参照 aterm `CodexRecordParser.kt`(c03e46f)+14 测对齐口径，再两端对拍 golden（共享 fixture）。

## 不做（本 slice / 本 feature）
- 不碰 `parse_line`/`JsonlRecord`（Claude 零回归）。
- 不接 consumer（F3 turn-end / F5 usage / F7 UI 各自接）。
- 不建 rigid typed enum（format churn 高 → Value 宽容抽取）。

## 与主计划对接
- 事实源：`code-picture/codex-vs-claude-事实对照`（本机实测 + openai/codex 源码）。
- 双写：`codex_record` 逻辑将镜像到 daemon（`remote-daemon-proto`，F3/F5 daemon 侧 turn-end/usage 用）+
  frontend（F7 TS 渲染）——同 turn_detect/usage 现状。本 slice 先落 monitor 实例 + golden 契约。

## 审计结果（F2a · 低风险自审）
- 新模块、零 Claude 影响（monitor 320 = 原 316 全绿 + 4 新）。防御性经 malformed/unknown/alias case 覆盖。
- classify 对本机真实 record shape（session_meta/turn_context/world_state/task_complete/token_count/
  response_item message-reasoning-tool）全对；未知顶层/子型 → Other/OtherEvent 不崩、不误判。
- my 文件 clippy 0、fmt 净。0 阻塞。

## 进度：F2 记录层完成
- ✅ F2a 分类器（825a15a）· ✅ F2b-1 trap-critical 助手（ce5e9e6）· ✅ F2b-2 `to_jsonl_record` 组装（642ff86）。
- 第三条路落地：Codex→现有 JsonlRecord（message/reasoning/tool→content blocks、event→Unrecognized+raw）。
- monitor 327、my clippy 0、Claude 零回归。**staged**（parser.rs Codex 分支派发 = F1 发现层重构时接）。
- aterm 真数据坑全守（output/content 数组、reasoning 空、developer isMeta、无 parentUuid、call_id 配对）。

## 签收（F2）
- [x] 代码审计（F2a/b 自审 + aterm 互审口径对齐）· [x] 工程审计（新模块无耦合、staged 诚实标注、双写将镜像 daemon/frontend）· [x] 主计划已更新
