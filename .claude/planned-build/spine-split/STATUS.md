# 状态 / STATUS — spine-split 工作区（每次先读这里）

> 独立 planned-build 工作区。分支 `account-ux`。**脊柱拆分**：把两个「上帝文件」
> （前端 `src/tabs.ts` ~3178 行 / 后端 `src-tauri/src/ssh_source.rs` ~4512 行）安全分解为小模块。
> 从 audit-fixes 的 F13 **单独拆出来做深**（用户 2026-07-26 定：单独开 planned-build、先全面评估再拆）。

## 当前
- **阶段**：**关闭——评估后决定不拆、维持现状**（用户 2026-07-26 拍板）。未动任何代码。见 MASTERPLAN.md（决定 + 理由 + 3 agent 报告摘要）。
- **一句话**：拆分由「具体架构病」证成、不由行数证成；两个 god file 都没有能压过「拆分引入的新风险（可见性放宽/测试重分区/static 复制误伤 §24）」的具体收益 → 维持现状。唯一命中架构病的 F12（分层倒挂）已在 audit-fixes 修完。
- **重开条件**：未来出现**具体**架构病（错向依赖/没法测/真实耦合痛点，非「文件大」）再按判据单独立项。

## 硬约束（继承 audit-fixes + 本工作区）
- **行为逐字节等价**：拆分是重构，运行时行为 + UI 功能不许变。
- **红线**：daemon 零行为改动 · 不改 TMUX_LS_FMT 双写点 · 不碰 ~/.bashrc · 不改 cc-<sid8> 语义 · 不 push/发版/bump · 不要用 emoji。
- **§24 单写者不变量**：ssh_source 若拆，REMOTE_IDLE/mark_idle/find_tmux_origin 单写者（emitter）绝不能破。
- **安全先行**：最安全的 cut（纯函数 verbatim move）先做；interface-extraction 高风险的先补 characterization 测再拆；entangled 的评估后可判「不拆」。

## 已知起点（audit-fixes F13 摸底结论，待 agent 复核）
- **tabs.ts 最安全 cut**：模块级 tmux 匹配纯函数（`findClaudeTmux`/`findIdleTmux`/`isCcmTmuxName`/`findOrphanTmux`/`isCwdFallbackMatch`/`claudeExited` + `TmuxSession` 类型，~L240-345）→ `src/tmux-match.ts`。verbatim move、有测、零 `this` 耦合。
- **账号子系统**：疑 entangled（`alignableCurrent` 横跨账号态+tab 态+重启执行态，被徽章/mismatch/restart 三类共用）——待 agent 定性。
- **ssh_source.rs**：审计早标「高危、可能延后」——待 agent 裁决。

## 门禁纪律
结果重定向到文件 + Read/grep 核实 + pipefail；每步 tsc + vitest（前端）/ cargo（后端）绿。
