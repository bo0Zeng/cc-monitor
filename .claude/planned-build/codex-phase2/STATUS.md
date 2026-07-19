# STATUS — cc-monitor Phase 2：Codex 泛化

## 当前阶段
- **Phase A（主规划）完成** = `MASTERPLAN.md`（三 agent 全面调研底座 + 共享面账本 + F1–F7 拆分）。
- 用户 2026-07-18 **直接授权全自动开跑**（Draft masterplan → 直接授权 auto-loop）；关键决策不清 → 开 agent web 搜索。

## 自动模式
- **连续跑（全自动）**：用户预批全程；loop 连续 B→G，只在下列停。
- **停 loop 交用户**：① 跨项目待 aterm 回（wire 共享面在 aterm 2D）② 计划-现实冲突 ③ 同一步失败两次 ④ 全部完成→Phase G。

## 已完成
- Phase 0/事实对齐：`code-picture/codex-vs-claude-事实对照_2026-07-18.md`（两端交叉核对无 diff + 三 agent 深研补充 A–D）。

## 进度
- ✅ **F1 slice 1（monitor adapter 地基）**：`AgentKind`/`SidStrategy`、`SessionLayout.sid_strategy`、
  `CodexAdapter` shell（~/.codex+$CODEX_HOME）、`for_kind()`、`session_id_from_path` 泛化（Claude 零回归/
  Codex 末36 UUID）。monitor 316 pass（原 312 全绿+4 新）、my 文件 clippy 0、fmt 净。**active() 仍 Claude、未接 discovery**。

## Phase F 再排（2026-07-18，用户批准）
- **F1 slice 2 调查发现**：monitor 会话发现**不是单一中央扫**——散在核心 startup（`lib.rs:341` 一 agent 一 watcher 一 SessionMap）+ ~20 处 `active()` scan 点。多 kind = 核心 startup 大重构 + 设计岔口（显式传 kind vs 按路径解 kind）。**且早于 F2 premature**（发现了 Codex 会话也无记录模型可解析/渲染）。
- **再排（用户选「F2 keystone next」）**：**先 F2**（CanonicalRecord + Codex defensive parser，自包含、纯可测、本机 fixture、解锁 turn-end/usage/UI、对齐 aterm 2A）。monitor 发现层大重构**推迟到 F2 后**、专门设计一轮 + 协调。daemon Codex 发现（更自包含）可与之并行/其后。

## 进度（续）
- ✅ **F2a · Codex 防御分类器**（`codex-F2a` 825a15a）：`codex_record.rs` classify(Value)->CodexRecordKind（顶层 5 type + 子型、alias turn↔task 归一、未知→Other 不崩）+ accessor（turn_id/token_usage_last/message_role）+ golden 测（本机真实 shape + 防御 case）。staged `#![allow(dead_code)]`（consumer 在 F3/F5/F7）。monitor 320（原 316 全绿+4）、my clippy 0。
- ✅ **canonical 岔口已定 = 第三条路**（aterm 2A 对齐 2026-07-18）：渲染轴复用现有 `JsonlRecord`（Codex 映射进、零 re-plumb、Claude 零回归）+ turn-end/用量轴 per-kind（event→Unknown 保 rawJson）。taxonomy 两端一致。

## 进度（续）
- ✅ **F2b-1 · 映射 trap-critical 助手**（`codex-F2b1` ce5e9e6）：flatten_text/reasoning_text/tool_input/call_id，守 output 数组坑。
- ✅ **F2b-2 · `to_jsonl_record` 组装**（`codex-F2b2` 642ff86）：Codex→现有 JsonlRecord（message/reasoning/tool→content blocks、event→Unrecognized+raw）。**F2 keystone 记录层完成**。monitor 327、my clippy 0、Claude 零回归。**staged**（parser.rs 派发 = F1 发现层重构时接）。

## F1 design pass 完成（2026-07-18）→ F1a/F1b 拆分
- **发现**：history/read/list 路（`history.rs` 枚举 records_dir+parse_line）与 live watcher+SessionMap startup **可分离**。
- **F1a（bounded/visible/unblocked，现在做）**：多 kind history/list/read → Codex 会话可见+经 to_jsonl_record 渲染。不碰核心 startup。
- **F1b（big/与 F4 耦合，推迟）**：live watcher + Codex 判活（核心 startup、无 pidfile）→ 合进 F1b+F4 slice（届时撞核心 startup 大改 → **停 loop 交用户定范围**）。
- ✅ **F1a-1 · 发现原语**（`codex-F1a1` c6112da）：`enabled_kinds`/`records_dir_for`/`records_roots`（显式传 kind）。monitor 329、staged。

- ✅ **F1a-2 · parse 派发 + read-session 多 kind**（`codex-F1a2` f6c6bd5）：`parse_for_kind`（Claude→parse_line/Codex→to_jsonl_record）+ `kind_of_path` + `stream_read_session_jsonl` 多 kind。**Codex 会话内容现可解析+渲染**（read 路 live）；Claude 字节不变。摘 codex_record 模块 staged allow。monitor 331、总 clippy 不增。

## 下一个（下轮 loop 目标）= F1a-3 · list-sessions 枚举多 kind + Codex 元数据
- **list-sessions/项目列表枚举 `records_roots()`**（`history.rs:135` 项目扫 + `:193` 会话扫）：Claude 走 projects/<cwd> 分组、Codex 走 sessions/日期树。前端拿到 Codex 会话（含 jsonl_path）→ 点开走已接的 read 路渲染。
- **Codex 会话元数据**（title/cwd/首条摘要）：per-kind 抽——Codex cwd 从 `session_meta.payload.cwd`（非 User.cwd）、title 从首条 message 或 session_meta。可能加 `session_meta` accessor。
- **注意**：Codex 无 cwd-项目目录概念 → 用 `session_meta.cwd` 内存分组呈现（口径先想清、别硬套 projects/<enc(cwd)>）。真机 ~/.codex 验证列表。
- 之后 F1a 完 → F3/F5/F7（部分待 aterm 2D）→ F1b+F4 判活（核心 startup，撞大改→停 loop）。
- 之后：F3 turn-end（daemon 侧 codex_record 镜像 + per-kind turn_detect；需 agent_kind wire@aterm-2D 才跨项目联调）→ F5 用量 → F7 UI（第三条路下渲染基本免费，验证为主）→ F1 发现层重构（post-F2 专设计）+ daemon --agent → F4 判活 → F6 resume。
- **潜在停点**：F3+ 的 daemon/wire 联调需 agent_kind wire（aterm 2D）——届时可能停 loop 等 aterm/交用户。

## 待协调（不阻塞本地地基）
- wire 共享面（agent_kind + liveness_confidence + ResumeSpec agent_kind）→ **aterm 2D 联调**。aterm 正推 2A（RecordParser SPI）。
- F4 判活「fd 持开」假设 → 建到 F4 时起真 codex 会话实测坐实（源码已指向持开）。

## 回看
- 2026-07-18 建 STATUS，Phase A 完，指向 F1。
