# F1 · AgentKind + Codex 定位/发现

## DoD（可验证）
- [ ] `AgentKind{ClaudeCode, Codex}` enum（monitor 先落；daemon/frontend 后续 slice）。
- [ ] `SessionLayout` 泛化：`sid_from_stem: bool` → `sid_strategy: SidStrategy{Stem, CodexRollout}`（表达 Codex `rollout-<ts>-<uuid>` 文件名取 UUID）。
- [ ] `CodexAdapter` shell（impl 全 `AgentAdapter`；F1 相关字段真实：data_root `~/.codex`(+`$CODEX_HOME` override)、layout；F4/F6 字段占位标注）。
- [ ] `for_kind(AgentKind)` 派发；`active()` 仍 = Claude（**Claude 零回归**——所有现有 caller 走 active() 不变）。
- [ ] Codex sid 提取：`rollout-<ts>-<uuid>.jsonl` → 末 36 字符 UUID（本机语料实测对拍）。
- [ ] gate 绿：monitor cargo 不回归（`claude_layout_locked` 更新后仍绿）、fmt、clippy。

## 不做（本 slice）
- 不碰记录模型（`CanonicalRecord` = F2 keystone）。
- 不让 discovery 真 multi-kind（active() 仍 Claude；按会话根 per-kind 派发 = F1b/后续 slice）。
- 不动 daemon（daemon `--agent` selector = 后续 slice）。
- 不碰 liveness（`liveness_subdir` 对 Codex 占位；F4 引入 per-kind liveness 时泛化）。
- .jsonl.zst（冷会话压缩）发现 = F2/history。

## 与主计划对接（共享面账本）
- **AgentKind enum**（账本）：monitor 先定义；daemon/frontend 各自镜像（后续 slice）。
- **CanonicalRecord**：本 slice **不碰**（F2）。
- 遵循零回归红线：`claude_layout_locked`（更新断言到 `sid_strategy::Stem`，语义不变）、`session_id_from_path` 对 Claude 结果字节一致。

## 接口
- `SidStrategy{Stem, CodexRollout}`。
- `session_id_from_path(p)` = `session_id_from_path_with(active().layout(), p)`（公开 API 零回归）；`_with` 供 per-kind 测 + 后续 multi-kind 派发。
- `codex_sid_from_rollout(&Path) -> Option<String>`（末 36 字符 UUID 校验）。
- `for_kind(AgentKind) -> &'static dyn AgentAdapter`。

## 步骤
1. adapter.rs：加 `AgentKind` + `SidStrategy` enum；`SessionLayout.sid_from_stem`→`sid_strategy`。
2. adapter.rs：`session_id_from_path` 拆 `_with(layout,p)` + match strategy；加 `codex_sid_from_rollout` + `is_uuid` 助手。
3. adapter.rs：`for_kind(kind)`（Claude/Codex 两 static）；`active()`=for_kind(ClaudeCode)。
4. claude_code.rs：CLAUDE_LAYOUT `sid_strategy: Stem`；更新 `claude_layout_locked` 断言。
5. 新 codex.rs：`CodexAdapter` + CODEX_LAYOUT + `resolve_codex_dir`（`$CODEX_HOME`|`~/.codex`）；全 trait 方法（F4/F6 占位注）。
6. 测：`codex_sid_from_rollout` 本机语料（`rollout-2026-07-18T20-25-05-019f7867-...` → `019f7867-...`）、非 rollout/短名/坏 UUID → None；`for_kind` 派发；claude sid 不变。

## 测试策略
monitor `cargo test`（新 codex sid 测 + 更新的 claude_layout_locked + 现有全绿=零回归）；fmt + clippy。

## 进度：slice 1 完成（monitor adapter 地基）
本 slice 落 monitor 侧：`AgentKind`+`SidStrategy` enum、`SessionLayout.sid_from_stem`→`sid_strategy`、
`CodexAdapter` shell（`~/.codex`+`$CODEX_HOME`、CODEX_LAYOUT）、`for_kind()`、`session_id_from_path`
泛化（Claude Stem 零回归 / Codex 末 36 UUID）。**active() 仍 Claude、未接 discovery 派发**。
剩余 slice：F1b monitor 多 kind discovery 派发（探 `~/.codex` + 按会话根派发）；F1c daemon `--agent`
selector + 日期树 discovery + daemonless SSH find。

## 审计结果（slice 1 · 低风险自审）
- **Claude 零回归**：`session_id_from_path` 走 `active()`=Claude、Stem 策略与原 `file_stem` 字节一致；
  `claude_layout_locked` 更新到 `SidStrategy::Stem`（语义不变）；monitor 316 pass（原 312 全绿 + 4 新）。
- **Codex sid 正确性**：真实 rollout 文件名 → `019f7867-...`（时间戳内 `-` 不误切，取末 36 校验 UUID）；
  非 rollout/短名/坏 UUID → None（不臆造）。`is_uuid` 8-4-4-4-12 位校验经 bad-case 覆盖。
- **gate**：my 三文件 clippy 0 / fmt 净（monitor 其余 31 clippy = 既有 tech debt、非本 slice、隔离在别处）。
- 0 阻塞。

## 工程审计结果（slice 1）
- 隔离在 adapter 模块、沿用既有 seam 模式（`SessionLayout`/`AgentAdapter`/`active()`）；无新耦合。
- `AgentKind::Codex` 变体 + placeholder 字段（liveness/resume）均 `#[allow(dead_code)]`/doc 明标 F1b/F4/F6
  接线点——诚实收窄、不假装做更多。主计划自洽（F1 拆 slice 记进 feature/STATUS）。

## 签收
- [x] 代码审计（slice 1 自审 0 阻塞）· [x] 工程审计（slice 1）· [x] 主计划已更新（STATUS + 本 feature）
