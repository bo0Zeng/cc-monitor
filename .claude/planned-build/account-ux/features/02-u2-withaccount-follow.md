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
（待填）
