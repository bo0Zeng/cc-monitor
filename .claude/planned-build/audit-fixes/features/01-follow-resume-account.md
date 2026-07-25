# F01 — follow-resume 账号安全 + resume 选账号

> 修 full-audit 阻塞 B1（pin 内存脏读覆盖磁盘真 pin）+ issue #75 主因（跟随注入错账号）。

## DoD
- [x] **步骤 1**：跟随 resume 的 pin **现读磁盘**（`list_last_accounts`），不读内存镜像 `accountLastByS`；回归测 + 变异验证。
- [x] **步骤 2**：显式「用基座 resume（不隔离）」逃生口——`resumeTab` 加 `useBase`（不跟随、不注入、不读 pin），归档远端 tab 菜单有 ≥1 可选账号时追加该项；回归测 + 变异验证。
- [x] **步骤 3**：resume 账号下拉——**per-account「用账号 X resume」项 A4/A5 已存在**（`appendAccountMenuItems`），步骤 2 补齐「基座」项后，归档远端 tab 的 direct-resume 选号矩阵完整。默认仍全局当前工作账号，显式选号重钉（withAccount 既有语义）。
  - **残留（转 F04）**：tmux 版 base（`resumeTabTmux` 的 useBase）+ 历史页 resume 入口的基座项 → 归入 F04「统一直连/tmux 管线 + 每入口后端×账号矩阵」，避免两处各做一遍。

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
- [x] 过代码审计（D，主线程低风险自审）：step2 变更 = resumeTab 加 useBase 参 + follow 条件加 `|| useBase` + 菜单加 base 项；`useBase` 走 withAccount(null, follow undefined)=不注入=旧默认行为，additive 不改既有路径；tsc 0；变异验证(去 `|| useBase` → base 测红)。
- [x] 过工程审计（E）：base 项 additive、无新耦合；per-account picker 复用既有 appendAccountMenuItems；主计划自洽；全量 570 测绿 + build ✓。tmux/history 一致性显式转 F04（账本记明，非补丁）。
- [x] 主计划已更新（rev 03）
- [x] F01 完成（步骤 1/2/3 全签收）
