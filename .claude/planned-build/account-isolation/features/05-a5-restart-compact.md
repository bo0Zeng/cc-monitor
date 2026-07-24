# A5 — 换号破坏性重启会话 + （可选）compact

切账号语义③（DESIGN §2）：让**正在跑**的会话改用别的号 → 杀旧进程 + 用新号 resume 同一 sid。
破坏性（中断当前回合）。远端优先。核心编排 = DESIGN §5。**在 A4 的 `withAccount` 骨架上扩展**。

## 可行性（Phase B 已核实机制）
- ④ kill 旧进程：`kill_remote_tmux`（tmux.rs:140，headless ssh-exec）**已有**。
- ⑤ 带账号 resume：`buildResumeTmuxCmd(sid,cwd,launcher,name,configDir)` + `runRemoteResumeTmux` **已有**。
- ③ compact 的 send `/compact`：**需新增 headless `tmux_send_keys(origin,target,keys)` 命令**（照 kill_remote_tmux 范式，走 `ssh_source::connect_and_exec_cmd`，`tmux send-keys -t <name> <keys> Enter`）。daemon 只读、不参与（send-keys 走一次性 ssh，非 daemon）。
- compact 完成检测：`isCompactSummary(text)`（cards/compact.ts:15）**已有**——优先从**活 tab 已有的 daemon 流**里检测（tab 开着就无需额外轮询）；tab 未观测则有界超时后放弃、照 §5.2 不阻断。
- ① 预检 trust：`checkTrust`（accounts.ts）**已有**（只警告不阻断）。

## DoD（可勾选）
- [ ] **新 Rust 命令 `tmux_send_keys(origin, target, keys)`**（headless，照 kill_remote_tmux；keys 经 shell_quote；只允许发到 `cc-*` 目标名，防误发）+ 注册 lib.rs + 单测（命令构造 / NO_TMUX 降级）。**daemon 只读边界不破**（send-keys 走一次性 ssh，不经 daemon）。
- [ ] **A5 编排器**（扩展 `withAccount` 或新 `restartWithAccount`）：① 预检（X selectable + 远端 + daemon 够新；trust 只警告）→ ② `window.confirm` 破坏性确认（文案写清中断+耗时+compact 顺序）→ ③[可选默认关] send `/compact` + 有界等完成 → ④ `kill_remote_tmux`（失败即**中止不续**⑤，防双进程抢会话）→ ⑤ `runRemoteResumeTmux(...,configDir=X)` → ⑥ `recordLastAccount(sid,X)` + toast。
- [ ] **失败语义照 §5.2**：预检失败不动手给可操作提示；compact 超时/错**不阻断**续④；优雅退出超时降级 kill；kill 失败**中止**；⑤ 失败走既有剪贴板回退。
- [ ] **compact 顺序正确**（§5.1）：勾上时在**旧号**上 compact（命中旧缓存）**再**换号——顺序写进按钮 tooltip。
- [ ] **compact 默认关**（用户拍板）。承载形态（§5 note）：**MVP=两条菜单项**「用账号 X 重启…」/「用账号 X 重启（先压缩上下文）」（避开无 checkbox 的 confirm）；进度用 toast（"正在压缩…/正在重启…"）。
- [ ] **落点**：tabs.ts **活远端 tab** 右键加「用账号 X 重启…」(danger)（每可选账号；活 tab 才出——归档 tab 用 A4 的「用账号 X resume」）。
- [ ] **顺带收敛技术债**（A4 遗留）：tabs 账号菜单项从**同步 peek 改异步追加**（复用 tabs `resolveAttachMenuItem`(tabs.ts:1837) 的菜单开后异步挂项模式），消除冷缓存分裂——A4 的「resume」项与 A5 的「重启」项都走这套异步骨架。
- [ ] **全量验证**：tsc 0 / npm test 全绿 / cargo test --lib / build ✓。真机零改动（只 send-keys + kill + resume + 写本机 metadata，不碰用户 `~/.claude`）。
- **不做**：本地会话换号（A7）；部署 apply（A6）；优雅退出序列的精细 V3（先直接 kill，优雅退出留 TODO）。

## 对接主计划 / 共享面
- 新增：`tmux_send_keys`（tmux.rs + lib.rs）。改：tabs.ts（异步菜单追加 + 活 tab 重启项 + 编排调用）、accounts.ts（可能扩 withAccount 或加 restartWithAccount 编排）。
- 复用 A4：`withAccount`（resolve+record 骨架，A5 在中间插 compact/kill/resume）、`peekSelectableAccounts`→改异步、`accountConfigDir`、`isCompactSummary`、`kill_remote_tmux`、`runRemoteResumeTmux`、`checkTrust`。

## 逐条实现步骤（Phase C）
1. Rust `tmux_send_keys` + 注册 + 单测 → cargo 绿。
2. tabs 菜单**异步追加**重构（A4 resume 项 + A5 重启项共用；收敛冷缓存债）→ tsc/vitest 绿。
3. A5 编排器（预检→确认→[compact]→kill→resume→record）+ 失败语义 → 单测编排纯逻辑。
4. tabs 活 tab 右键接编排 + 两条 compact 菜单项 + toast 进度。
5. compact 完成检测（活 tab 流优先 + 有界超时兜底）。
6. 全量验证 + 真机零改动核查。

## 测试策略
- Rust：tmux_send_keys 命令构造 / NO_TMUX 降级（照既有 tmux 测）。
- 前端：编排器纯逻辑单测（mock kill/resume/sendkeys/checkTrust：预检失败不动手、compact 超时不阻断、kill 失败中止不续⑤、成功记 lastAccount）；菜单异步追加守卫（复用 A4 appendAccountResumeItems 守卫范式）。
- 破坏性动作 + 远端交互无法真机自动测 → 靠纯逻辑单测 + tsc + 手动真机验证（用户空闲时）。

## 审计结果（D，2026-07-24，三视角并行）
- **正确性/安全**：报 **1 阻塞**——`restartTabWithAccount` 缺 `live.sid === sid` 精确守卫，降级远端（无 @ccm_sid）+ 同 cwd 多 claude 时 findClaudeTmux 走 cwd 回退→可 kill 错会话 + 对目标 sid 起新进程 = 双进程/jsonl 双写。**已修**：加 `!live || live.sid !== sid` 即拒（对齐 resumeTabTmux，破坏性操作不"按目录猜"）+ 3 条守卫锁定测（cwd 回退拒 / 精确命中放行 / 不在 tmux 拒）。其余全绿：kill 失败中止、compact 不阻断、send-keys cc-* 白名单、菜单代次守卫、waiter 不泄漏均正确。
- **计划符合度**：**零阻塞、零谎报**（tsc0/vitest/cargo 亲验为真）。7 项 DoD 逐条磁盘核到真身。留 R1（tmux_send_keys 命令构造/NO_TMUX 测缺，命令内联难测，同 kill_remote_tmux 惯例）、R2（appendAccountMenuItems 直接测缺，已补 restartTabWithAccount 守卫 3 测）为跟进项。
- **架构/耦合**：**零阻塞**。裁定 **withAccount 与 restartWithAccount 应分离**（语义不兼容：不可选降级 vs 中止 / 无条件记账 vs 仅全成功记）——**已修**：account-restart.ts 顶注补"为何不合并"。清理：删死代码 `peekSelectableAccounts`+4 测（A5 异步重构后零生产调用）、删 `_cwd` 死参、confirm 带 tmux 名、④ 加 S2 优雅退出 TODO。
- **建议（未做，记账）**：抽 `resolveLiveTmux(origin,sid,cwd)→{sessions,live,viaCwd}` DRY 三处 tmux 解析（resolveAttachMenuItem/resumeTabTmux/restartTabWithAccount）——纯 DRY 重构，correctness 已由守卫覆盖，A6 不会扩展此处，作可选后续；`is_ccm_tmux_name`↔`deriveTmuxName` 跨语言契约补进 INVARIANTS（已补，见下）。

## 工程审计（E）
- 模块三层分明：`account-restart.ts` 纯编排（不 import UI）/ tabs.ts UI 胶水（菜单/tmux 解析/awaitCompactFor）/ accounts.ts store。**不拖累 A6**（A6 依赖 A3、与 restart 正交）。daemon 只读边界守住（send-keys/kill/resume 全走一次性 ssh，不经 daemon）。account store 消费经 withAccount/restartWithAccount 共用同一批原语，无逻辑漂移。A4 冷缓存债确收敛（同步 peek 已从生产路径彻底移除并删净）。

## 签收
- [x] 过代码审计(D，三视角，1 阻塞已修 + 建议/清理已做) · [x] 过工程审计(E) · [x] 主计划已更新(F) · [x] 测试绿（433 vitest + 351 cargo + build）
