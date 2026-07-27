# F1 — 全局/局部两个切号入口 + 文案统一

## 现状勘定（重要：大部分已存在，F1 主要是减法+relabel+rename）
- **chip 已是 CCSwitcher 式下拉**（`account-chip.ts` toggleMenu：列账号 ●/○ 当前 + 点击 setDefaultName 全局切）。多出的是 ⚠k 计数（mismatchSpan/updateMismatchBadge）+ 菜单顶部「对齐 N 个」批量入口。
- **tab 右键已有 per-session 切号**（`tabs.ts:2214-2240`：每可选账号「用账号 X 重启…」+「…（先压缩上下文）」，archived 给「用账号 X resume」）——正是局部切号，只需按 CCSwitcher 语义 relabel。
- **tab 徽章 ⇄**（align-to-current）留给 F2 删；F1 不动 ⇄。

## DoD
- [ ] chip = 纯全局切换器：下拉列账号 + ●/○ 当前 + 点击设为当前账号；**移除 ⚠k 计数 + 批量对齐菜单入口**（batch align 仍在命令面板，main.ts:552）。
- [ ] tab 右键 per-session 菜单 relabel 成「把此会话切到账号 X（重启）」框架（吸收旧「重启」措辞为「切到 X」直觉版）。
- [ ] 「当前工作账号」全部文案 → 「当前账号」（全仓 src/，含注释/字符串/样式/测试）。
- [ ] main.ts 去掉 chip 的 alignAll 依赖 + 2 处 updateMismatchBadge 调用（onDefaultChanged 保留）。
- [ ] tsc 0 / vitest 全绿（删 ⚠k 测试、留 chip 下拉切号 + tab per-session relabel 断言）。
- **不做**：不删 tab 徽章 ⇄（F2）；不动 account-restart 编排逻辑；不动 command-palette 的 alignAll。

## 与主计划对接（共享面）
- `account-chip.ts`：账本「chip 只读/极简下拉」项——F1 落「极简全局下拉 + 去 ⚠k」最终形态。
- `tabs.ts` 右键菜单：账本「tab 右键 per-session」项——relabel 到最终措辞。
- `account-restart.ts`：逻辑不动（per-session + 命令面板复用），仅被调用方文案变。

## 逐条步骤
1. 全局 rename `当前工作账号`→`当前账号`（sed src/ + styles.css；纯中文串非标识符，安全）；跑 tsc/vitest 兜底。
2. `account-chip.ts`：删 mismatchSpan/updateMismatchBadge/mismatchCount + toggleMenu 里的批量对齐入口 + AccountChipDeps.alignAll；title/文案简化为「当前账号（点击切换 / 管理）」。
3. `main.ts`：删 alignAll dep + 两处 `accountChip.updateMismatchBadge(...)`；保留 onDefaultChanged。
4. `account-chip.vitest.ts`：删 updateMismatchBadge 相关用例，补/留 chip 下拉切号断言。
5. `tabs.ts:2214-2240`：relabel per-account 项——活跃「把此会话切到账号 X（重启）」/「…（先压缩上下文再重启）」，archived「把此会话切到账号 X（resume）」。
6. 门禁 tsc/vitest（pipefail 回盘）。

## 测试策略
- chip：下拉渲染账号行 + 点击 selectDefault 走 setDefaultName（保留/补）；确认无 ⚠k span。
- tab 菜单：relabel 后 label 断言（若有右键菜单测试）。
- rename：tsc + 全量 vitest 兜住文本断言破坏。

## 实现状态（Phase C 完）
- ✓ 全局 rename 当前工作账号→当前账号（13 文件）
- ✓ chip 去 ⚠k（删 mismatchSpan/updateMismatchBadge/mismatchCount + toggleMenu 批量对齐块 + AccountChipDeps.alignAll）
- ✓ main.ts 去 alignAll dep + 2 处 updateMismatchBadge 调用
- ✓ tabs.ts per-account 菜单 relabel「把此会话切到账号 X（重启/resume）」
- ✓ account-chip.vitest 删 U6 ⚠k 测试 + 加 F1「无 ⚠k」断言
- ✓ styles.css 删孤儿 .status-account-mismatch
- **门禁**：tsc 0 / vitest 597。

## 审计结果（Phase D 完）
1 综合 agent，**无阻塞**。确认：mismatch 数据流未断（setSessionAccounts 五参齐 → updateAccountBadge/⇄ 链完好）、命令面板批量对齐完好、chip 去 ⚠k 自洽无悬空、tab relabel id 未变（不破坏按 id 逻辑）、rename 完整（0 残留、不误伤英文标识符）、只做 F1 范围（tab ⇄ 未删，留 F2）、红线全守。修：
- **I1（已修）**：两处 user-facing 提示仍引用旧标签「用 X 重启」/「用账号 X resume」→ 同步成新措辞「把此会话切到账号 X」（account-chip.ts toast、tabs.ts 徽章 tooltip、tabs.ts 无法定位时的错误提示）。
- **S4（已修）**：compact label 补「把此会话」前缀，三条 relabel 措辞平行。
- **S2（已修）**：tabs.ts 两处注释同步新标签。
- **S3（已修）**：补 DoD 要的正路覆盖——chip 下拉列出账号 + 点非当前项 → setDefaultName 全局切号 + onDefaultChanged 回调（account-chip.vitest）；tabs.vitest 徽章 tooltip 断言同步新措辞。
- **S1（移交 F2）**：`countAccountMismatches()` 生产侧已无调用者（唯一调用点随 main.ts 的 updateMismatchBadge 删除而消失），成死代码——F2 撤 mismatch 主 UI 时一并清（连同 tabs.vitest 对它的测试）。
- **复验门禁**：tsc 0 / vitest 598。

## 工程审计结果（Phase E）
主线程对账 §3 账本：chip「极简全局下拉」+ tab 右键「per-session 切号」落最终形态；account-restart 编排零改（tab 右键/命令面板复用）。未引入新耦合债。唯一技术债 = S1 死代码，已明确移交 F2（F2 正是清 mismatch 主 UI 的功能，最自然的清理点，非补丁）。红线全守（无新增轮询/emoji、daemon·remote-launch·rc 零改）。

## 签收
- [x] 过代码审计(D)（无阻塞；I1/S2/S3/S4 已修复验，S1 移交 F2）
- [x] 过工程审计(E)（数据流完整、账本最终形态、死代码移交 F2）
- [x] 主计划已更新(F)
