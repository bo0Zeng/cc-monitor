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

## 下一个（下轮 loop 目标）
- **F2 · CanonicalRecord + Codex defensive parser**（keystone）。先写 `features/02-canonical-record.md`（Phase B），设计中立 `CanonicalRecord`（**cc-bus 与 aterm 对齐 canonical 概念模型 + 互审**，非硬 wire 依赖）；Codex 解码器 maximally defensive（envelope unwrap / alias task↔turn / 全 taxonomy / call_id 配对 / 容缺 world_state / .jsonl.zst）；Claude canonical 投影（parse_line 之上、零回归）。本机 Codex rollout 作 fixture 语料。

## 待协调（不阻塞本地地基）
- wire 共享面（agent_kind + liveness_confidence + ResumeSpec agent_kind）→ **aterm 2D 联调**。aterm 正推 2A（RecordParser SPI）。
- F4 判活「fd 持开」假设 → 建到 F4 时起真 codex 会话实测坐实（源码已指向持开）。

## 回看
- 2026-07-18 建 STATUS，Phase A 完，指向 F1。
