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

- ✅ **F1a-3a · session_meta accessor**（`codex-F1a3a` d08d339）：`session_meta_cwd`/`session_meta_timestamp`（list cwd-grouping prep）。monitor 332、staged。

- ✅ **F1a-3b · Codex 枚举 + 项目入 list**（`codex-F1a3b` 6f3a613）：`enumerate_codex_sessions`（walk 日期树 + 读 session_meta cwd）+ `codex_projects_from`（cwd 分组 → HistoryProject，键 `codex:<cwd>`）+ list_history_projects 追加。**真机验证 13 会话→3 合成项目**。monitor 333、clippy 不增、零回归。

- ✅ **F1a-3c · Codex 项目会话列接线**（`codex-F1a3c` 8b896fc）：`stream_history_sessions_in_project` 认 `codex:<cwd>` 键 + `codex_session_entry` + `codex_first_user_excerpt`（跳 env_context 噪音）。

## ✅✅ F1a 完成（Codex 历史会话在 monitor 可浏览：项目分组 → 会话列 → 内容渲染，全 backend、前端零改、Claude 零回归）
里程碑：F1s1 adapter 地基 + F2 记录层 keystone（第三条路：Codex→JsonlRecord）+ F1a-1/2/3（发现原语/read 路/list 枚举）。monitor 334、总 clippy 31 不增。**F1b（live watcher+判活）推迟**（核心 startup，与 F4 合并、撞大改→停 loop 交用户定范围）。

- ✅ **F5 · monitor 用量 per-kind（Codex）**（`codex-F5` eba6b40）：`usage.rs` 加 Codex 分支——枚举 Codex 会话→逐行 raw JSON→`token_count` 的 `last_token_usage` 增量按 (model,天) SUM 归桶。字段映射 input=input_tokens−cached / cache_read=cached / cache_creation=0 / output=output_tokens（防重复计，input+cache_read=总 prompt）。model 取 turn_context.model。**Phase D 自审**修全零 no-op 事件 ghost 桶。**真机对账**（跨午夜/total 不可靠会话均 SUM last 对上）。monitor 341、clippy 31 不增、Claude 零回归。daemon --usage 的 Codex（远端）属 daemon --agent 片（待 aterm 2D）、本片不含。

- ✅ **F7 · UI 验证 + 注入去噪**（`codex-F7` 2562057）：读前端渲染路坐实 Codex 经第三条路**渲染 largely 免费、零前端改**——消息 blocks 走 renderMessage、event→Unrecognized 命 default→skip(零事件噪音)、空 uuid 优雅跳、usage-pivot 对 Codex model/cache_creation=0 通用不崩。唯一代码改 = **注入上下文去噪**（role=user 正文 `<environment_context>`/`<recommended_plugins>` → isMeta=true 隐藏，对齐 doc §63 + aterm）。monitor 342、clippy 31 不增、Claude 渲染零回归。

## ✅✅✅ 本地可独立完成的未阻塞 slice 全部完成（F1a 历史 / F5 用量 / F7 渲染+去噪）
Codex 在 monitor **历史可浏览 + 内容正确渲染 + 用量计入**，全 backend·零前端改·Claude 零回归。monitor 342、clippy 31 不增。

## ⛔ Loop 停在计划决策点（2026-07-19）——剩余全部命停点
- **F1b+F4 判活**（live watcher + Codex 判活，无 pidfile）= **核心 startup 大改** → 计划早定「撞大改停 loop **交用户定范围**」。这是下一步、但需用户决定改造范围。
- **F3 turn-end / F6 resume / daemon --agent** = 跨项目，**待 aterm 2D `agent_kind` wire** + 两阶段计划的 daemon 联合开发（用户定：aterm 到 daemon 阶段找我共同做）。
→ 无本地未阻塞 slice 可续，**停 loop 交用户**（符合停止条件①③）。

## ⬇ 传入约束（aterm 2B Phase D 真机审计同步 @00:47，都真机 codex 0.144.6【核】；记此供 F1b/F3 落地）
1. **F1b 上下文表**：占用用 `last_token_usage.total_tokens`（**别用累计 `total_token_usage.total_tokens`**——单调增会把上下文卡死~100%）；上限直接读 `info.model_context_window`（真机 258400），**别拿 GPT model 套 Anthropic 200K/1M 档**。
2. **F5 用量 input 语义**：`input_tokens` 含 `cached_input_tokens` → 新鲜输入 = input−cached（否则与 cacheRead 重复计）。**✅ 我 F5 已独立收敛到同解、真机对账无重复计**（互证）。
3. **F3 turn-end**：`turn_id` 缺（v1 alias 路最可能）→ 回退键 = envelope timestamp（否则 null 被当"非 end"漏报最新完成轮）。我 `turn_id` accessor 现 staged、F3 接线时加回退。

## 待协调（不阻塞已完成的本地地基）
- wire 共享面（agent_kind + liveness_confidence + ResumeSpec agent_kind）→ **aterm 2D 联调**。aterm 正推 2B 用量 SPI。
- **渲染去噪集对齐**：我已去噪 `<environment_context>`/`<recommended_plugins>`；`# AGENTS.md instructions`/`You have an MCP server…` 形态模糊、已 cc-send aterm 提议对齐 denoise 集 + 更新 doc §63。
- F4 判活「fd 持开」假设 → 建到 F1b+F4 时起真 codex 会话实测坐实（源码已指向持开）。

## 回看
- 2026-07-18 建 STATUS，Phase A 完，指向 F1。
