# 状态 / STATUS — account-zero（恢复工作的入口，每次先读这里）

> 跨轮对话靠这个文件接着干，不靠记忆。每完成一步就更新。

- **当前阶段**：**B 功能规划**（Phase A 主计划 **2026-07-29 用户已批准**）
- **当前功能**：Z01 账号 0 登记 + 可见
- **当前步骤**：未开始（feature 计划待写）
- **已完成功能**：无
- **下一个功能**：Z01 → Z02 → Z04 → Z03（Z05 独立，任意时点可插）
- **阻塞 / 待用户确认**：
  - **[已批准] 主计划**（用户 2026-07-29「批准主计划 account zero」）
  - **[待确认] 授权动 `~/.claude/skills/cc-acct-iso/`**（上游本体，在用户家目录）。
    **注意 `~/.local/bin/cc-acct-iso` 是 symlink 指向它 ⇒ 改了立刻在 aya 上生效、没有缓冲。**
    未获授权前，Z01/Z04 只改 vendored 副本并生成 diff 给用户自己贴——**但那会造成两份漂移，
    比不做更坏**，所以 Z01 开工前必须先要到这条。
  - **[待确认] `tabs.ts` 红线松不松**（Z02 绕不开，约 14 处「基座」）。Z01 不需要。
  - **[待确认] 账号 0 的显示名**（建议 manifest 存可改 `name`，默认 `"0"`，UI 显示「账号 0（主）」）
  - **[待用户跑] 真机验证三步**（本会话红线禁止我起真实已认证的 claude）：
    ① `cc-acct-iso verify` 看是否已报「`$HOME/.claude.json` 又出现了」那条 vwarn
    ② 不选账号 resume 一个远端会话，看是否要求重新登录
    ③ Z01 做完后再 `verify` 一次，看账号 0 是否出现且标 `loggedIn`
- **最近一次计划回看时间**：2026-07-29（Phase A 落盘 + 用户批准）
- **自动模式（/loop）**：未起 loop。**用户已批主计划，但每个功能计划（Phase B）仍需过审**
  —— 用户本轮说的是「开始制定计划」，没说全自动
- **本轮 loop 目标**：n/a
- **loop 停止条件**：n/a
- **备注**：
  - **本工作区的立论**：一条守不住的不变量，不如一个能表达它的模型。**吸收 > 检测 > 禁止。**
  - **账号 0 的定义（全局约定）**：**账号 0 ≡「不设 `CLAUDE_CONFIG_DIR`」这个状态本身**。
    凭据在 `~/.claude/.credentials.json`，状态在 `$HOME/.claude.json`。起它 = **什么都不设**
    （不是空串、不是 `~/.claude`）。给它一个 `configDir` 路径就是 cc-acct-iso 已有的 V1
    `--default-in-place`，会引入 `.claude.json` 分裂。
  - **Z01 开工第一件事是实测「空串 vs 未设」**（`cc-acct-iso:682`
    `exec env CLAUDE_CONFIG_DIR="$cfgdir" …`），结果决定共享面 2 的形态。
  - **顺手要收的门禁欠账**：`vendor/cc-acct-iso/scripts/` 纳入 shellcheck（BACKLOG **E13**，
    实测今天零告警）+ `vendor/cc-acct-iso/scripts/test/run-tests.sh`（424 行，工具自己的测试）
    接进门禁 —— **既然要改这个工具，没有网不能改**。
  - 关联：BACKLOG **E1/R16**（history 缺基座逃生口）按本模型**不是遗漏**，Z02 时关掉该登记项。
    BACKLOG **E15**（两渲染器 base 不等价）由 Z02 闭合。
