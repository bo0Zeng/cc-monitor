# DG1 · daemon Codex 发现 + Line 流 + turn-end（设计草案，待用户审 + DG2 判活实测）

> **状态 = 设计草案**（Phase B）。用户 2026-07-19 定：先起草设计交审、fd 判活实测待用户方便起真 codex。
> 审批 + DG2 判活实测坐实前**不写码**。

## DoD（做什么）
- daemon watcher **发现 + tail 本机 Codex 会话**（`<codex_dir>/sessions/YYYY/MM/DD/rollout-*.jsonl`），发 `Line` 帧（逐行 raw + byte_offset，同 Claude）。
- 发现的 Codex 会话发 `SessionAdded{agent_kind:"codex", liveness_confidence:"heuristic", cwd, path, sid}`；退场发 `SessionRemoved`。
- **⚠必办 gotcha**：Codex 会话的 turn-end 走 `codex::is_codex_turn_end`/`codex_turn_end_uuid`（**非** Claude `turn_detect`）发 `TurnEnd` 帧——否则 aterm（若门控 emits）等永不到的帧。
- `Hello` 翻 `codex_dir:Some(resolved)` + `kinds:["claude","codex"]`（现服务 Codex）。
- **Claude 路字节零回归**（pidfile 发现/判活/turn_detect/Line 全不动）。

## 不做（DG1 范围外）
- **精确判活 = DG2**（fd 持开 + logs_2.sqlite process_uuid，real-test-gated）。DG1 只用 **mtime 近似**（rollout 近期被写=活）判「流哪些会话」；DG2 精化退场判定。
- resume（DG6，已完成）、usage（DG5，已完成、走 `--usage` 一次性查询非 live 流）。

## 核心难点：Claude 模型 ≠ Codex 模型
| 轴 | Claude（现 watcher） | Codex（DG1） |
|---|---|---|
| 会话记录 | `projects/<enc-cwd>/<sid>.jsonl` | `sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`（日期树） |
| 判活源 | **pidfile** `sessions/<PID>.json` + `/proc/<pid>` starttime | **无 pidfile** → mtime（DG1）/ fd+sqlite（DG2） |
| 发现 | 扫/监视 `sessions/` pidfile add/remove | 监视 `<codex_dir>/sessions/` rollout 文件 add/append |
| 退场 | pidfile 删 或 PID 死（2s poll） | rollout mtime 陈旧（DG1）/ fd 关（DG2） |
| turn-end | assistant+end_turn（`turn_detect`） | task_complete 事件（`codex::is_codex_turn_end`） |

现 `watch_loop`/`ReaderState`/`process_*` 全 **pidfile-centric**（`sessions.HashMap<pidfile→SessionEntry>`、`session_alive(pid,start)`）。Codex 无 pidfile → 不能套。

## ★ 架构选型（**请用户定**）
### 方案 A（推荐）：平行 Codex 路、Claude 字节不动
- `watch_loop` 除 Claude（projects+sessions pidfile）外，**再加一条 Codex 发现**：监视 `<codex_dir>/sessions/`（递归、日期树），rollout 文件事件 → Codex 处理。
- `ReaderState` 加 Codex 簿记（`codex_sessions: HashMap<rollout_path→CodexEntry{sid,cwd,last_mtime}>` + 复用 `offsets`/`seqs`/`active_sids`）。
- **复用共享原语**：`read_new_lines`/`ReadCursor`/`SeqCounter`/`FrameSink`/`process_jsonl` 的 Line 发射核——Codex 与 Claude tail 同一套字节/offset/seq 逻辑（byte_offset 语义对齐 aterm LineFramer 不变）。
- **per-kind turn-end**：`process_jsonl` 现固定调 `turn_detect::turn_end_uuid`；Codex 路改调 `codex::codex_turn_end_uuid`。做法 = `process_jsonl` 加 `kind` 参数（或抽 `process_jsonl_line(kind, ...)`），Claude 传 ClaudeKind（行为字节不变）、Codex 传 CodexKind。
- **判活 poll**：2s tick 除 Claude pidfile 判活外，加 Codex mtime 判活（rollout mtime 陈旧 → SessionRemoved）。
- **优点**：Claude 路零回归（红线）、mirror monitor「第三条路」哲学、增量可控。**缺点**：两套发现/判活簿记，少量重复。

### 方案 B：泛化 watch_loop 到多 kind
- 重构 `watch_loop`/`ReaderState`/`process_*` 为 kind-参数化（per-kind 发现/判活策略、共享 tail）。
- **优点**：长远更干净、无重复。**缺点**：**动 Claude 路**（100KB 核心 live 循环）→ 回归风险高、审计面大。

→ **我推荐 A**（Claude 零回归红线优先、增量安全、与 monitor 第三条路一致）。**请你拍 A / B / 或别的范围。**

## DG1 设计（按方案 A）
1. **codex_dir 解析 + watch**：`codex::resolve_codex_dir()`（DG5 已建）→ `<codex_dir>/sessions` 存在则加 `RecursiveMode::Recursive` watch（日期树）。初扫（Phase 1）walk 近期日期分区的 rollout（避免全史——只近 N 天/近期 mtime）。
2. **发现 + 流判定（mtime 近似）**：rollout 文件 append 事件（mtime 刷新）→ 若未跟踪且 mtime 在窗口 W 内 → 新 Codex 会话：读首行 session_meta 取 cwd、`codex_sid_from_path` 取 sid（DG5 已建、畸形跳）、发 SessionAdded{codex/heuristic/cwd/path}、rescan tail 现有行。已跟踪 → tail 增量。
3. **tail**：复用 `read_new_lines`（ReadCursor/SeqCounter）→ Line 帧（byte_offset 同 Claude）。**tail_only/with_bg** 语义：Codex 无 bg 概念（with_bg 忽略）；tail_only 同 Claude（prime cursor 不重放，monitor 走 --read-session 快照）。
4. **turn-end（gotcha）**：Codex 路每行过 `codex::codex_turn_end_uuid` → 非 None 发 `TurnEnd{sid,uuid}`（uuid=turn_id 缺→timestamp）。**与 DG4 detector 接线**（DG4 的 staged 函数在此接 consumer、摘 dead_code allow）。
5. **判活/退场（mtime 近似 → DG2 精化）**：2s poll 检 Codex 会话 rollout mtime：陈旧超阈 T → 视为结束 → SessionRemoved。**DG1 的 mtime 判活是近似**（空闲但活的 codex 会 mtime 老→误判死）；**DG2 用 fd 持开/sqlite process_uuid 精化**（fd 开=真活即便 mtime 老）。判活窗 W/阈 T 值 = DG2 实测后定（先用保守大窗、别误杀）。
6. **Hello 翻**：`codex_dir:Some(...)`, `kinds` 加 "codex"（现服务 Codex 发现）。

## 待用户/DG2 定的岔口
- **① 方案 A vs B**（上）。
- **② mtime 判活窗 W/阈 T**：DG2 实测 fd/sqlite 后定；DG1 先保守（宽窗、宁流多勿杀活）。
- **③ 初扫范围**：Phase 1 扫多少历史 rollout？建议只近期（近 N 天 or mtime 窗内），非全日期树（避免把整史当 live）。
- **④ 判活精度依赖 DG2**：DG1 可先 mtime-only 落地（可 demo Codex live 流+turn-end），DG2 补 fd/sqlite 精度——**或**等 DG2 实测坐实再一并做（两者耦合）。

## 步骤（审批 + DG2 定后）
C1 codex_dir watch 接线（初扫近期 + live watch）· C2 Codex 发现→SessionAdded(codex/heuristic) · C3 tail 复用→Line · C4 **turn-end gotcha 接线（DG4 detector）** · C5 mtime 判活 poll→SessionRemoved · C6 Hello 翻 codex_dir/kinds · 每步 golden 对拍 aterm CodexSessionCatalog + 真机验。

## 零回归红线
- Claude pidfile 发现/判活/turn_detect/Line 字节不动（方案 A 保证）。
- 不 bump PROTO_VERSION（DG3 wire 已 additive）。
- gate：daemon cargo test 不回归（基线 108）+ clippy --all-targets 0 + fmt。
- **DG2 判活假设未真机坐实前不当既定**；mtime 窗保守。

## 变更记录
- 2026-07-19 建（草案）：用户选「起草 DG1 设计、实测待方便」后拟。待审 A/B + DG2 实测。
