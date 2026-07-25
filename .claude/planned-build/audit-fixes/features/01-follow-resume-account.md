# F01 — follow-resume 账号安全 + resume 选账号

> 修 full-audit 阻塞 B1（pin 内存脏读覆盖磁盘真 pin）+ issue #75 主因（跟随注入错账号）。

## DoD
- [x] **步骤 1**：跟随 resume 的 pin **现读磁盘**（`list_last_accounts`），不读内存镜像 `accountLastByS`；回归测 + 变异验证。
- [ ] **步骤 2**：账号解析瀑布里「基座/不隔离」是可显式选中的一项（老会话能选它 resume）。
- [ ] **步骤 3**：resume 入口加账号下拉（含基座），默认全局当前工作账号，显式选号重钉。picker 组件为 F03/F04 共享面。

## 不做（防蔓延）
- 不改 withAccount 的解析语义（瀑布不变：显式 > pin > 全局账号 > 基座）。
- 不自动探测"会话属于哪个账号"（靠用户选 + pin 记忆）。
- 新建/重启入口的后端+账号选择 → F03/F04（本功能只做 resume 入口）。

## 与主计划对接
- 共享面「`src/tabs.ts` pin 读取」→ 落 `readSessionPin(sid)` 现读 helper（账本最终形态），resumeTab/resumeTabTmux 共用，与 history.ts:1489 三处一致。
- 共享面「resume/起会话入口 UI」→ 步骤 3 落 resume 版 picker，F03/F04 复用。

## 实现步骤
1. ✅ `tabs.ts` 加 `readSessionPin(sid)`（fresh `invoke("list_last_accounts")` + catch→undefined）；`resumeTab`(直连) 与 `resumeTabTmux`(tmux) 两处 follow.lastAccount 改 `await this.readSessionPin(sid)`。验证：tsc 0 + 3 回归测 + 变异（退回内存镜像→2 测红）。
2. ⏳ 基座作为可选项（accounts 模型 + 选择器语义）。
3. ⏳ resume 下拉（右键菜单 / 历史入口）+ 重钉。

## 测试策略
- 单元/DOM：`tabs.vitest.ts` 新增「F01 follow-resume pin 现读磁盘」套件（resumeTab + resumeTabTmux 现读；显式选号不读）。
- 回归纪律：先写复现失败测试（内存镜像脏读）→ 修 → 变异验证锚点。

## 审计结果
- **代码审计(D)**（低风险主线程自审）：变更 = 新 helper + 2 调用点 + 3 测；`readSessionPin` 逐字对齐 history.ts 现读；async 链 tsc 通过；`accountLastByS` 仍作徽章数据源保留（未误删）；无 daemon/cc-<sid8>/bashrc 触碰。无阻塞。
- **工程审计(E)**：readSessionPin 是账本预定共享面，为 F13 铺路，无新增耦合；主计划仍自洽；全量 569 测绿 + build ✓。

## 签收
- [x] 步骤 1 过代码审计（D）
- [x] 步骤 1 过工程审计（E）
- [x] 主计划已更新（rev 02）
- [ ] 步骤 2/3 未做（下轮）
