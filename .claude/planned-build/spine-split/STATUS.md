# 状态 / STATUS — spine-split 工作区（每次先读这里）

> 独立 planned-build 工作区。分支 `account-ux`。**脊柱拆分**：把两个「上帝文件」
> （前端 `src/tabs.ts` ~3178 行 / 后端 `src-tauri/src/ssh_source.rs` ~4512 行）安全分解为小模块。
> 从 audit-fixes 的 F13 **单独拆出来做深**（用户 2026-07-26 定：单独开 planned-build、先全面评估再拆）。

## 当前
- **阶段**：**Phase A 主规划（深度评估中）** —— 未动任何代码。
- **评估方式**：3 并行 agent 全面评估（① tabs.ts 可拆单元清单「能拆什么」② 拆后安全 + 功能保全 ③ ssh_source.rs 模块化可行性）。收齐 → 综合成 MASTERPLAN（可拆单元账本 + 风险分级 + 依赖顺序 + 安全/验证策略）。
- **门禁**：Phase A 主计划**须过用户确认**（planned-build 铁律 #7）才进 B/C。**先评估再拆，别硬拆**（用户明令）。

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
