# F05 — 手动清理真孤儿会话（#76 残留）

> 用户拍板：孤儿**仅手动**回收，不自动 kill。

## DoD
- [x] 纯函数 `findOrphanTmux(sessions, tabSids)` + `isCcmTmuxName(name)`（tabs.ts，导出可测）：**真孤儿 = `cc-*`(过 isCcmTmuxName) + 带 @ccm_sid + 该 sid 无对应活 tab**。保守：只认带 @ccm_sid 的（身份确凿，绝不误杀有 tab / 非本工具会话）；`<project>_cc`(cc-bus)经 isCcmTmuxName 天然排除。
- [x] `TabManager.cleanupOrphanTmux(origin)`：点击才查 `list_remote_tmux` → 算孤儿(this.tabs.keys() 为活 sid 集) → 无则 info toast → 有则 `window.confirm` 列出 → 逐个 `kill_remote_tmux`(F02 白名单)→ 结果 toast。
- [x] 入口：账号 chip 菜单「清理孤儿会话…」(deps.cleanupOrphans → main.ts → tabs.cleanupOrphanTmux(origin))。
- [x] 回归测(6 例：isCcmTmuxName + 孤儿/有tab/无sid/项目_cc/空) + 变异验证(去"无 tab"判据 → has-tab 测红)。

## 不做（防蔓延）
- **不自动 kill**（用户拍板）。
- 不清 `<project>_cc`（cc-bus 资产、且 isCcmTmuxName 天然排除）。
- 不清**无 @ccm_sid** 的 cc-*（身份不确凿，保守不误杀）——这类靠 F03.4 甲′/丙 让新会话带身份后自然减少。

## 与主计划对接
- 共享面 `tmux.rs`：复用 F02 的 `is_ccm_tmux_name`（后端 kill 白名单）——前端 `isCcmTmuxName` 镜像同判据，kill 双保险。

## 审计
- **D**（低风险主线程自审）：findOrphanTmux 保守（只杀 @ccm_sid 确凿 + 无 tab）；kill 走 F02 白名单后端拒非 cc-*；confirm 明列不可恢复 + "不碰有 tab/非本工具会话"；无 daemon/双写点/bashrc/轮询/自动。tsc 0 / npm test 586 / build ✓。
- **E**：复用 F02 判据（账本一致）；per-origin chip 入口；主计划自洽。#76 命名/回收部分本步 + F03.1(复用防新孤儿)共同缓解。

## 签收
- [x] 过 D + E
- [x] 主计划已更新
- [x] F05 完成
