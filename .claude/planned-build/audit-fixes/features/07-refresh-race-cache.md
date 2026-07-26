# F07 — 刷新竞态(I4) + 多远端缓存(I5) + onUnselectable

## DoD
- [x] **I4 刷新竞态**：`main.ts refreshSessionAccounts` 加 in-flight 递增序号门——进入 `++refreshSeq` 取 `mySeq`，两处写 `setSessionAccounts` 前 `if (mySeq !== refreshSeq) return`（晚到的旧快照不覆盖新快照，防切号"反向窗口"从并发侧重开）。
- [x] **I5 多远端缓存**：`defaultName` 是**全局单值**，`selectDefault`(account-chip.ts / accounts-section.ts) 切它时从 `invalidateAccountsCache(this.origin)` 改为 `invalidateAccountsCache()` **清全部 origin**（否则非当前 origin ≤30s TTL 仍用旧账号判"不一致"/对齐目标错）。"刷新"按钮仍 per-origin。
- [x] **onUnselectable**：`resumeTab` 显式选号解析不到 → 补 toast（对齐 history.ts:1502），不再静默落基座吞掉用户明点的"用账号 X resume"。

## 测试
- onUnselectable：tabs.vitest 扩既有"账号库不可用"测，断言 `showActionFailureToast("账号不可用", …)` 触发 + 变异验证（改标签→红）。
- I4：在 `main.ts`（全仓零测试覆盖，F09 专项补）→ 序号门是 textbook latest-wins 模式、逐行核；本轮不单测（untestable-without-heavy-setup）。
- I5：改动 = 1 行 `this.origin` → 无参；`invalidateAccountsCache()` 清全部已在 accounts.vitest 覆盖；chip-internal selectDefault 单测 setup 重、低风险不加。

## 审计
- **D**（中风险主线程自审）：I4 序号门覆盖两处写点（early-return + 主）；I5 只改 selectDefault 两处（刷新按钮不动）；onUnselectable 逐字对齐 history.ts；无 daemon/双写点/bashrc/轮询。tsc 0 / npm test 586 / build ✓。
- **E**：I4/I5 是 account-ux 并发/多远端边角，收干净后不拖累 F03.2；主计划自洽。

## 备注
- I4/I5 是 full-audit 审计项（无 GitHub issue）→ 完成不关 issue。

## 签收
- [x] 过 D + E（I4/I5 测试受限已注明，转 F09 补 main.ts 盲区）
- [x] 主计划已更新
- [x] F07 完成
