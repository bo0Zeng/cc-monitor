# F2 · Codex 记录层（keystone）

## 背景 · 一个待定的设计岔口
调查（读 messages.rs `JsonlRecord`）发现：Claude 记录模型**深度 CLI 专属**（8+ 变体、isMeta/api_error/
queue-op/attachment、紧耦合前端渲染+分支检测+turn-end+usage）。所以「中立 CanonicalRecord 统一所有
consumer」是**大重构、高回归风险**。**岔口**（接 consumer 时定，非现在）：
- (A) **全统一 CanonicalRecord**：JsonlRecord + Codex 都投影到中立模型，consumer 全改。干净、但重构大。
- (B) **per-kind adapter 方法**：`AgentAdapter` 加 `turn_end_of/usage_of/render_units_of`，Claude 走
  现路（零回归）、Codex 走 Codex 解析。改动小、低回归，但两套渲染路。
- 倾向：**(B) 偏向**（低回归 + 增量），中立模型只在**新 consumer 共享逻辑**处引入、不替 JsonlRecord。
  **待接 F3/F5/F7 consumer 时按实际需要定，并与 aterm 2A canonical 模型互审对齐。**

## DoD（分 slice）
- [x] **F2a · Codex 防御分类器**（`codex_record.rs`）：`classify(Value)->CodexRecordKind`（顶层 5 type +
  event_msg/response_item 子型，alias `turn_*`↔`task_*` 归一，未知→Other 不崩）+ accessor（turn_id/
  token_usage_last/message_role）+ golden 测（本机真实 shape + 防御 case）。**staged `#![allow(dead_code)]`**。
- [ ] F2b · Codex message content 抽取（response_item.message.content[] → 渲染单元；reasoning；tool call_id 配对）——F7 UI 用。
- [ ] canonical 岔口 (A/B) 决定 + aterm 对齐（接 F3 consumer 前）。

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

## 签收
- [x] 代码审计（F2a 自审）· [x] 工程审计（F2a：新模块无耦合、staged 诚实标注）· [x] 主计划已更新
