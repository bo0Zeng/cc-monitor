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

## 下一个（下轮 loop 目标）
- **F1 slice 2**：monitor 多 kind discovery 派发——探 `~/.codex/sessions` 存在则纳入、发现层按会话根传 kind
  （watcher/history/search/usage 的 `active()` 点改按路径 kind），Codex 日期树扫。仍不碰记录模型（F2）。
- 之后 F1 slice 3：daemon `--agent` selector（§26 strippable）+ 日期树 discovery + daemonless SSH find。

## 待协调（不阻塞本地地基）
- wire 共享面（agent_kind + liveness_confidence + ResumeSpec agent_kind）→ **aterm 2D 联调**。aterm 正推 2A（RecordParser SPI）。
- F4 判活「fd 持开」假设 → 建到 F4 时起真 codex 会话实测坐实（源码已指向持开）。

## 回看
- 2026-07-18 建 STATUS，Phase A 完，指向 F1。
