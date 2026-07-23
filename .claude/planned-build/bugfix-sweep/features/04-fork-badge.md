# F-fork-badge(#63① fork 显示成同名独立 tab)

> 类1 feature 4。#63 三症状里的**①**:fork 出来的会话在活 tab 里与原会话**同名、肉眼分不清**(活 tab 层只按 sessionId keyed、`forkedFrom` 此前只在历史树用)。②③ 不在本 feature(见下)。

## DoD / 验收
- fork 会话的活 tab 标题带 **`↳` 血缘徽标** + tooltip「↳ 从 <parent-sid8> fork 而来」→ 与原会话可辨识。
- 镜像 `aiTitle` 范式:`onLine` 从首条带 `forkedFrom` 的记录取 `forkedFrom.sessionId` → 锁定(出现一次即锁,同 aiTitle)→ 重算标题 + refreshTabBar。
- 验证:tsc + 全套 `npm test`。

## Phase D 审计(2 视角)——均**无阻塞、无重要代码问题**
- **正确性+回归**:字段路径 `message.forkedFrom.sessionId` 对照后端 `messages.rs`(serde `sessionId`、`forkedFrom` 无 skip)+ `history.rs`(每条记录都带同一 `forkedFrom.sessionId`、后端也锁首条)核实无误;`onLine` 顺序在双重去重之后(重投不重触);`↳` 恰合成一次(computeTitle 每次从零件重建、不追加)、后到 aiTitle 不叠加;非 fork **字节不变**、零成本;`unknown` cast 有 `typeof` 守卫、无类型洞。判「correct, ship-able」。
- **范围+UX+符合度**:DoD 达成(用计划点名的 `computeTitleFor` 路径 + `↳`+tooltip);②③ 未碰、`styles.css` 未碰(账本预测「大概率改」未命中——纯文本前缀不需 CSS,可接受);`↳` 在 `[origin]` 之外(`↳ [pi] …`)、活过后到 aiTitle 与 tab 切换。

## 处置(采纳的审计项)
- **建议(已采纳)**:把 `applyForkedFrom` 移到 `turnEndNotifier.observe` **之前**(仍在双重去重之后)→ 首条即含徽标的标题也进 turn-end 通知。
- **测试补齐(两审都点的 pin)**:新增 3 条——fork + 后到 aiTitle → **单个 `↳`**(pin doubling)、远端 fork `↳ [pi] …` 顺序、**tooltip 含 `从 <sid8> fork 而来`**(唯一暴露 parent sid 处、原零覆盖)。共 6 条 #63① 测。
- **未采纳/记录**:给前端 `JsonlRecord` 补 `forkedFrom` 字段(类型完整性,cast 已安全,低价值);与历史树 `↳` 的**语义/呈现分歧**(历史树 `↳`=孤儿-only + 专用 styled span;活 tab `↳`=所有 fork + 纯文本前缀)→ **follow-up**,建议后续抽共享 label/helper 统一(非本 feature bug 范畴)。

## carry-forward(真机,用户)
- **远端 fork 徽标依赖 daemon**:`remote_history.rs` 对远端**历史**明确不提取 fork 关系(呈平铺);若 daemon 在**实时行**也不透传 `forkedFrom`,远端 fork tab 会静默**无徽标**——前端修不了、未测。真机验远端 fork 是否带徽标;缺则属 daemon 侧、另开。

## Phase E 工程审计(主线程对账)
- 共享面账本:本 feature **未**改 `styles.css`(账本对 #63① 的「大概率改」预测未命中,已核 `git diff --name-only` 只有 tabs.ts/tabs.vitest.ts/planned-build)——纯文本 `↳` 前缀,无 CSS。账本无需再改;#63① 与 #67 无实际 styles.css 冲突。
- `tab.title` 的所有消费者(turn-notify/grid/tear-off/ghost/label/tooltip)都当不透明显示串,无人解析前缀 → `↳` 安全。唯一 `startsWith("↳")` 是测试。

## 签收:代码审计[x](无阻塞/无重要;建议已采纳+补测) 工程审计[x] 主计划已更新[x]
