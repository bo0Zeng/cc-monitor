# U2 — withAccount follow 模式(additive 第三态)

> 把 U1 的 `resolveFollowAccount` 接进统一编排 `withAccount`,新增 opt-in「跟随」路径。**风险:中**(必须证 A4 零回归)。

## DoD
- [ ] `withAccount` 签名 additive 增 `opts.follow?: { lastAccount?: string|null }`;`accountName: string|null` 语义**逐字节不变**(string=显式 A4 / null 无 follow=基座 A4)。
- [ ] `accountName===null && opts.follow` → 新路径:`fetchAccounts` → `resolveFollowAccount(last→当前工作账号→null)` → 命中则注入其 configDir + 记 lastAccount(sticky 自增强);null 则落基座。**下沉静默不 toast**(用户没显式点)。
- [ ] **A4 零回归硬门**:`accounts.vitest.ts` withAccount 现有 5 用例**一行不改保持绿**(尤其"null→不 fetch、invoke 未调用")。
- [ ] follow 新套件 + **迁移守卫测**(老 config 仅 defaultName + follow → 解析出当前账号)。
- [ ] `remote-launch.test.ts` builder 契约未动仍绿。

## 不做
- 不改任何调用点(U3 才把 resumeTab/history 等接上 follow);不动 builder;不加 UI。

## 实现步骤
1. `withAccount` opts 类型加 `follow?`;函数体加 `else if (opts.follow)` 分支(fetch → resolveFollow → 注入/记账);用 `recordName` 变量统一显式/跟随两路的记账名。
2. 更新 doc 注释:null 分支区分"无 follow=基座 / 有 follow=跟随"。
3. 测试:withAccount describe 追加 follow 5–6 用例(last 胜 current / last 不可选下沉 / 无 last 跟 current 迁移守卫 / 都不可选落基座不 toast 不记账 / 新会话无 sid 不记账)。
4. 门禁:tsc + 全量 vitest + remote-launch 回归。

## 验证
tsc 0 / vitest 全绿(A4 老套件不改) / remote-launch 全绿。每步回盘核实。

## 代码审计(D) / 工程审计(E) / 签收
U1+U2 纯地基无 live 调用点 → 主线程复核:全分支单测 + A4 老 5 用例逐字节回归证 + remote-launch builder 契约绿。零回归。已签收(59012b5)。

---

# U3 实现笔记(已核实,给实现提速 —— 2026-07-24)
调用点与 runner 签名(全部读码确认):
- **runner 都已收 `configDir?`**:`runRemoteResume(origin,sid,cwd,launcher,configDir?)`(history:1487 已传)/ `runRemoteResumeTmux(origin,sid,cwd,launcher,name,configDir?)`(tabs:1857 **未传**,加第 6 参)/ `runNewSessionRemote(origin,cwd,command,configDir?)`(history:1518 **未传**,加第 4 参)。builder 层不动。
- **lastAccount 来源**:
  - `tabs.ts` 有 `this.accountLastByS: Map<string,string>`(setSessionAccounts 第 3 参喂) → `resumeTab`/`resumeTabTmux` 归档分支 follow 传 `lastAccount: this.accountLastByS.get(sid)` = **完整 sticky**。
  - `history.ts`:`HistorySessionEntry` **不带 lastAccount**(只 starred/customTitle/hidden)。**裁定**:history 行 `runResume` 默认(无 ctx.account)follow 传 `lastAccount: undefined` → 落**当前工作账号**(比旧的落基座是净改进);行级 sticky 需 entry 带该字段或查 list_last_accounts,**记为 U3 已知裁剪**(后续可补,不阻塞;tabs 路径已覆盖 sticky 主用例)。
- **接线**(U3):
  1. `tabs.resumeTab(sid)` 默认分支:`withAccount(origin, null, run, { sessionId: sid, follow: { lastAccount: this.accountLastByS.get(sid) } })`。显式(带 accountName)分支不变。
  2. `tabs.resumeTabTmux(sid)` ② 归档新起分支:把 `runRemoteResumeTmux(...,name)` 包进 `withAccount(origin, null, (cd)=>runRemoteResumeTmux(...,name,cd), { sessionId: sid, follow:{ lastAccount: this.accountLastByS.get(sid) } })`。① attach 活会话分支**不动**(账号焊死)。
  3. `history.runResume`:`withAccount(origin, ctx.account ?? null, run, { sessionId, onUnselectable, follow: ctx.account ? undefined : {} })`——有显式 account 不给 follow(维持 A4);无则 follow:{}(→ 当前工作账号)。
  4. `history.runNewSession` 远端:`withAccount(origin, null, (cd)=>runNewSessionRemote(ctx.origin,ctx.cwd,behavior.resumeCommandRemote,cd), { follow:{} })`(新会话无 sid,不记账)。
  5. `remote-section.ts` 新会话对话框:预选谓词 `effectiveDefault`→`currentWorkingAccount`(值同,语义对齐);"不指定"仍 = null 无 follow = 基座(显式 opt-out)。
- **测试**:tabs.vitest 现有"默认 resume configDir=undefined"两条(约 :760/:941)需**按契约演进改写**为 follow 断言 + 注释缘由;新增 tmux 归档 follow、history runResume follow / runNewSession follow 正路测。
- **Phase D**:U3 是首个改用户可见行为的功能 → 开对抗审计 agent(正确性/回归 + 计划符合)。
