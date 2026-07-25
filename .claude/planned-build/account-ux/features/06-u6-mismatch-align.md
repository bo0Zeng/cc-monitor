# U6 — 不一致检测 + 一键对齐

> 风险:**中高(破坏性路径)**。依赖 U1、U4、U5。
> 补记说明:U3–U6 此前未逐个建 feature 文件(偏离 MASTERPLAN「每功能 DoD 硬门」)。本文件在
> Phase D 审计后补齐 U6 这一份,并把审计结论落盘;U3–U5 的结论仍在 STATUS.md 的进度行里。

## DoD(对照 MASTERPLAN §Features U6)

| 计划要求 | 状态 | 落点 |
|---|---|---|
| tab **hover** `⇄`(仅活会话) | ✅ | `.tab:hover .tab-align-btn.is-eligible`(CSS 管露面)+ JS 管够格 |
| 精确 `@ccm_sid` 才动手 | ✅(点击时守卫) | `restartTabWithAccount` 的 `live.sid === sid`;**显示层不预判**,见「已知取舍」 |
| chip `⚠k` | ✅ | `.status-account-mismatch`,push 模型 `updateMismatchBadge(count)` |
| 汇总浮层 | ⏭ **未做** | 现用两步 `window.confirm`,已逐行列出「会话 → 目标账号」。**原写"顺延 U8",但 U8 收官时它在任何文件里都没再出现过(承诺断链,Phase G 文档审计揪出)** → 已改登记进 `.claude/planned-build/BACKLOG.md` §A,待用户定"做 / 正式取消" |
| 批量对齐(空闲/回合中两步确认) | ✅ | `alignAllToCurrentAccount` |
| 均复用 `restartWithAccount` | ✅ | 零新编排;仅加 `confirm` 透传 + 返回值 |
| 「current 不可选不对齐」测 | ✅ | 新纯函数 `alignableCurrentAccount` + 5 个 vitest |

**不做**:per-会话逐行取舍(浮层的事,U8);`compactFirst` 变体(右键菜单已有,新入口硬编码 false);
本地会话(A7);键盘可达的 `⇄`(与既有 📂/↗/× 同构,U8 快捷键统一解决)。

## 实现要点

- **单一谓词** `alignableCurrentAccount(sid, tab)`:⇄ 显隐 / ⚠k 计数 / 批量枚举 / U8 的 Ctrl+K 全走它。
  门控 = 远端 + 非归档 + 非 in-flight + readyOrigins + current 可用 + live 存活 + 确知不一致。
- **U8-ready public API**(账本预定最终形态,U8 不必再改):`alignSessionToCurrentAccount(sid): Promise<boolean>`、
  `accountMismatchSids(): string[]`、`countAccountMismatches()`、`alignAllToCurrentAccount()`。
- **重入防护**:`restartingSids`(per-sid,拦所有入口)+ `aligningBatch`(批量)。
- **确认分层**:单会话 = `restartWithAccount` 自带的破坏性二次确认;批量 = 批量层两步确认后传
  `confirm: () => true`,不再逐会话弹 N 次。

## 代码审计结果(Phase D:3 个对抗 agent 并行)

**阻塞 1 项(已修)**
- `idle` 判据用了 `activity?.status !== "running"`。会话状态枚举实为 `busy/idle/shell/waiting/null`
  (`bridge.rs` 透传 CC 官方值);`"running"` 是 **subagent** 的状态串。→ 判据恒真 ⇒ busy 桶恒空 ⇒
  用户拍板的决策④「回合中二步确认」**在生产中是死代码**,且确认框把跑着回合的会话称作
  「空闲…几乎无感」。**三个 agent 独立命中同一条。** 修:白名单 `idle`/`shell`,`busy`/`waiting`/`null`
  一律落第二步确认(未知即保守)。
- **元教训**:门禁 490 全绿没挡住它——我自己的测试照着实现抄了同一个错枚举。绿的是错的契约。

**重要 6 项(已修)**
1. `current` 不可选(未登录/in-place/目录缺失)时 ⚠k 常亮、⇄ 是死按钮、批量 N 个全败还报成功 →
   抽出纯函数 `alignableCurrentAccount` 过 `isSelectable`,并补齐 MASTERPLAN 点名要的那条测。
2. 末尾汇总按**发起数**报成功(最坏「0 成功 + 一句成功汇总」)→ `restartWithAccount` 加布尔返回值,
   逐个统计,文案改「已重启对齐 x 个;y 个未执行」。
3. 批量无重入防护:批量跑数分钟而 ⚠k 要等 10s 轮询才降,用户极易再点一次,第二批拿陈旧列表
   并发 kill/resume,可能把第一批刚 resume 的新进程杀掉;且批量已关掉逐会话确认 ⇒ 全程无拦截。
4. 切号后 ≤10s 反向窗口:`selectDefault` 不刷 `currentByOrigin` ⇒ chip 显新账号,而「对齐」把会话
   打回**刚被切走**的旧账号(与意图相反)→ 加 `onDefaultChanged` 回调即时重算。
5. 批量循环无 per-iteration `try/catch`,靠被调方自律 → 加上,单会话失败不中断整批。
6. 确认文案不实:对齐会**为每个会话新开终端窗口**(旧窗口 `-NoExit` 不自动关)、进程内状态
   (队列输入/后台任务/`/model`/MCP)会丢、正 attach 的终端会断 → 文案照实重写。

**架构(已修)**
- `AccountChipDeps` 反向拉 `TabManager`,且 chip 构造在 `tabs` 之前、靠「中间恰好没有 await」这条
  隐式不变量避开 TDZ(我原注释给的理由是错的)。→ 改**推**模型(与 `tabs.onActiveUsageChanged →
  usageHud.setActive` 同惯例)+ chip 构造移到 `tabs` 之后,不变量消失。
- 非 ready(未启用/需更新)时 ⚠k 仍显但菜单无对齐入口 = 死胡同 → `updateMismatchBadge` 加 ready 门控。

**建议(已办)**:⇄ 改 hover(同时解决布局:默认 150px tab 栏里固定图标已占 ~132px,标题只剩一两个字)、
`role`/`aria-label`、幽灵徽章 tooltip 补回「右键 resume 可对齐」的指路、⚠k 文案去掉「当前账号」单数、
CSS 去掉冗余 fallback、测试 `afterEach` 收 mockImplementation(防渗给后续 describe)。

**已知取舍(不修,记档)**
- **⇄ 不预判 tmux/@ccm_sid**:预判要对每个 tab 打一次 `list_remote_tmux`(SSH 往返),代价远大于
  收益;点击后守卫会拒并给出可操作提示(「先归档后用『用账号 X resume』」)。故 ⚠k 在
  「会话不在本工具 tmux」时可能虚高——U8 做浮层时可用 `tmuxCache` 顺手降噪。
- 跨 origin:计数跨所有远端、各按各自远端的当前账号对齐(行为正确),chip 只绑主远端 → 多远端下
  文案已改为「账号不一致的会话」不提单数账号。单远端(本项目实况)无感。
- 折叠态 tab 栏隐藏 ⇄(与 📂/↗/× 一致),此时走 chip 菜单。

## 测试结果

- **门禁**:`tsc --noEmit` 0 · `npm test` **522 通过**(U6 前 490 → 修复后 522,+32)· `npm run build` ✓。
- **变异验证**(D3 指出原 9 个用例是「CSS 显隐外壳」:删掉整个 ⇄ 点击监听、偷偷取消破坏性二次确认、
  对齐到 `WRONG-ACCT`,三个突变**全部照样绿**)。重写后逐个复验:

  | 突变 | 重写前 | 重写后 |
  |---|---|---|
  | 单会话偷偷注入 `confirm:()=>true` | 103/103 绿 | **1 failed** ✅ |
  | 删掉 ⇄ 的 click 监听 | 103/103 绿 | **2 failed** ✅ |
  | 单会话对齐到 `WRONG-ACCT` | 103/103 绿 | **1 failed** ✅ |
  | 退回 `idle: st !== "running"`(本次阻塞 bug) | — | **4 failed** ✅ |

- 新增覆盖:⇄ 正路(账号正确 / 不注入 confirm / 不切 tab)、5 档 activity 分桶(真枚举 + 断言弹的是
  哪个框而非只数次数)、文案说真话、批量部分失败的真实计数、批量中途抛异常不断批、
  per-sid 与批量两级重入、chip ⚠k 的 5 个 DOM 用例(此前零覆盖)、`alignableCurrentAccount` 5 条。

## 签收

- [x] 过代码审计(Phase D,3 agent 并行 + 阻塞项已修并复验)
- [x] 过工程审计(Phase E,见 STATUS/MASTERPLAN 账本回写)
- [x] 主计划已更新(账本 + 变更记录)
