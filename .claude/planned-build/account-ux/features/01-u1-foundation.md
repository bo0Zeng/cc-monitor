# U1 — 地基:纯函数层(解析器 + 账号色 + 徽章 source)

> 本轮的地基。全部纯函数 + vitest 锁死,零 UI/零副作用,后续 U2–U9 全建在其上。**风险:低**。

## DoD(可验证)
- [ ] `resolveFollowAccount(state, {lastAccount?, current?}): string | null` — 优先级 `lastAccount(可选)→ current(可选)→ null`;每级过 `isSelectable` 否则下沉;终点 null。纯函数 + table-driven vitest。
- [ ] `currentWorkingAccount(state): Account | null` — `effectiveDefault(state)` 的语义别名(值完全一致),供 follow 解析与 mismatch 比对共用,命名对齐"当前工作账号"新概念。+ 一致性测。
- [ ] `detectAccountMismatch(liveAccount: string|null, current: string|null): boolean` — `live!=null && current!=null && live!==current`。纯函数 + 三态测(相等 false / 不等 true / 任一 null false)。
- [ ] 新文件 `src/account-color.ts`:`accountColorSlot(name: string): number`(FNV-1a(name) % 8,确定性、稳定 key);纯函数 + vitest(确定性 + 分布 + 空串兜底 + CJK)。
- [ ] `sessionBadge(...)` 返回值加 `source: 'live' | 'last' | 'unknown'` 字段(不改现有 text/known/tooltip 语义,纯增字段);现有 sessionBadge 测适配 + 新增 source 断言。
- [ ] **回归硬门**:`accounts.vitest.ts` 现有全部用例保持绿(只增不改语义);`remote-launch.test.ts` 一行不改保持绿。

## 不做(防蔓延)
- 不接任何 UI/调用点(那是 U3/U4/U5);不改 withAccount(U2);不动 CSS(色 token 在 U4/U5 用到时加)。
- `accountColorSlot` 只返回 slot 序号(0–7),不返回 hex/CSS——CSS token 映射留 U4/U5(视觉落地时)。

## 对接主计划(共享面账本)
- 触及共享面:`accounts.ts` 纯函数集(**纯增** resolveFollowAccount/currentWorkingAccount/detectAccountMismatch)+ `sessionBadge` 加 source 字段 + 新增 `account-color.ts`。均在账本"最终形态"内,无新共享面。
- 不碰 builder 层、不碰 withAccount(本功能只备好纯函数,U2 才用)。

## 实现步骤
1. `accounts.ts` 加 `currentWorkingAccount`(直接 `return effectiveDefault(state)`,带 doc 说明是语义别名)。
2. `accounts.ts` 加 `resolveFollowAccount`(纯函数,用 `isSelectable` 经 `state.accounts.find`)。
3. `accounts.ts` 加 `detectAccountMismatch`(纯函数)。
4. 新建 `src/account-color.ts`:`accountColorSlot`(FNV-1a 32bit → %8)。
5. `accounts.ts` `SessionBadge` 接口加 `source`;`sessionBadge` 三分支各返回对应 source(live/last/unknown)。
6. 测试:`accounts.vitest.ts` 加 resolveFollowAccount/currentWorkingAccount/detectAccountMismatch/source 套件;新建 `account-color.vitest.ts`。
7. 门禁:`tsc --noEmit` + `CI=true vitest run`(重定向文件 + grep 计数)+ 现有回归全绿。

## 验证(实测门禁)
- tsc 0 err;vitest 全绿(新增用例数记录);`remote-launch.test.ts` 未改仍绿。
- 每步 Read 回盘;测试输出重定向文件再 Read + grep,绝不信内联绿。

## 代码审计结果(D)
（待填）

## 工程审计结果(E)
（待填)

## 签收
- [ ] 过代码审计(D)
- [ ] 过工程审计(E)
- [ ] 主计划已更新(F)
