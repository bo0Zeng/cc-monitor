# F13 — 脊柱拆分（tabs AccountBadgeController / ssh_source 评估）

> **状态：评估完成 → 停 loop 交回用户决策**（本功能是主计划/loop 明定的「撞到停」高风险项）。
> 未动任何代码。以下是边界分析 + 建议。

## 摸底结论：账号子系统与 TabManager 的耦合结构

`tabs.ts`（3178 行）的账号族 = **5 状态 map + ~11 方法**，横跨**三个状态域**：
- **账号状态**：`sessionAccountsByS` / `accountEmailByName` / `accountLastByS` / `accountReadyOrigins` / `currentByOrigin`（main.ts 定期喂）。
- **tab 状态**：`tab.origin` / `tab.status` / `tab.activity.status`（TabManager 的 `tabs` map 拥有）。
- **重启执行状态**：`restartingSids` / `aligningBatch` / **per-sid compact-await 回调（`onLine` 见 compact 摘要行即 resolve）**（会话生命周期耦合）。

**核心纠缠点 = `alignableCurrent(sid, tab)`**：它**同时读三域**（账号 5 map + tab.origin/status + `restartingSids`），且被**三类调用方**共用——① 徽章渲染 `updateAccountBadge`、② 不一致查询 `accountMismatches`/`accountMismatchSids`/`countAccountMismatches`、③ 重启执行 `restartTabWithAccount*`/`alignSession`/`alignAll`。重启族又经 `onLine` compact 回调织进会话生命周期（重启主体原语其实已在 `account-restart.ts`，tabs 里是 TabManager 侧粘合）。

## 拆分可行性判断
- **可拆的部分**：账号 5 状态 map + `setSessionAccounts`（喂数据）+ `updateAccountBadge`（徽章视图）可移入 `AccountBadgeController`，做成**单向依赖**（TabManager→Controller，把 `alignable` 当参数传入，Controller 不回调 TabManager，无环）。
- **拆不干净的部分**：`alignableCurrent`（三域纠缠）+ 不一致查询（迭代 `this.tabs`）+ 重启执行（`restartingSids` + onLine compact 回调 + 会话 kill/resume）**必须留 TabManager**。于是 Controller 需暴露 ~6 个 getter 供 `alignableCurrent` 读账号态。
- **净值评估**：移出 ~80–100 行（5 字段 + 2 方法），但**新增 ~6 getter 面 + 控制器样板**；`alignableCurrent`/mismatch/restart 仍在 TabManager。**复杂度净降有限，而徽章「信息才显」逻辑（U5/U8/detectAccountMismatch/源②③兜底）极微妙**，自动重构的行为等价回归风险落在**全仓最高风险文件**上。

## 决策（交回用户，别硬拆）
**建议：不在无人值守 loop 里硬拆**。这是主计划 §6「F13 最高风险、撞到停」+ loop 明令「拆不干净→交回用户」的正命中场景。可选：
1. **接受现状**（推荐默认）：把 god-object 债记档，`account-ux/MASTERPLAN` 已有 ADR 式注释；tabs.ts 有测试兜底、非阻塞。
2. **你在场时交互式拆**：我按上面「单向 Controller」方案做（Controller=账号态+徽章视图；TabManager 保 alignableCurrent+mismatch+restart），你盯着行为等价 + 全视角 D 审计。
3. **只做极小状态袋**：5 map 收进一个 `AccountBadgeState` 持有者（仅降字段数、逻辑不动），风险最低、价值也最小。

## ssh_source.rs（4512 行）
主计划早已定「高危、可能只做 tabs controller 抽取」。既然连 tabs controller 都判定为交回用户，**ssh_source 分模块本轮不做**（Rust 传输/快照/协议/导入 ~10 职责交织，风险更高），记档留专门重构批次。

## 签收
- [x] **F13 评估完成**：账号子系统经 `alignableCurrent` 三域纠缠、拆分净值有限而风险高 → **停 loop 交回用户**（未动代码）。ssh_source 分模块本轮不做。
