/**
 * 额度与账号几处界面共用的一个叫法：采样那台在界面上叫什么（照那台成品的名字，`machineName`）。
 * 状态栏那颗按钮与它的悬停卡照会话那台写好的 `session-account` 画（`account-chip.ts`），这里不判、不合。
 */
import { machineName } from "./ipc/chan-caller";
import type { Machine } from "./ipc/origin";

/** 采样那台在界面上叫什么。 */
export function machineLabel(origin: Machine): string {
  return machineName(origin);
}
