/**
 * 额度与账号几处界面共用的一个叫法：采样那台在界面上叫什么（那台叫什么是这一侧的事）。
 * 状态栏那颗按钮与它的悬停卡照会话那台写好的 `session-account` 画（`account-chip.ts`），这里不判、不合。
 */
import { copyText } from "./copy-table";
import { isLocalOrigin, type Origin } from "./ipc/origin";

/** 采样那台在界面上叫什么。 */
export function machineLabel(origin: Origin): string {
  return isLocalOrigin(origin) ? copyText("acct.machine.here") : origin;
}
