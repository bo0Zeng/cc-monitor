/**
 * 远端后端的装 / 卸：全前端只这一处问本机后端这两条帧命令（`deploy-install` · `deploy-uninstall`，入参只交那台的名字；
 * 带着哪几个槽是本机后端自己的事实，那一句也由它写全）。
 * 换上这一版：机器卡问题行［更新］与主窗口 ↗ 浮层［更新］都经这里 —— 先问会打断什么（有才弹框，确认键「更新」），再部署；
 * 本机那一台的［更新］（口上是上一版 app 留下的后端）也经这里、同一个框：确认后先停（壳拿这一版的落点停口上那一位）再起这一版。
 * 卸载：机器卡「卸载后端」（确认框住机器卡）。做的过程与结局由调用方在原处说。
 */
import { commands } from "./ipc/commands";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { confirmDialog } from "./kit/dialog";
import { copyText } from "./copy-table";
import { isLocalOrigin, LOCAL } from "./ipc/origin";
import { resolveRemoteConfigByOrigin } from "./remote-config";
import { askInterrupts, interruptRows } from "./settings/interrupts";

/** 期限（值归发起方，DL1）：问几次那台 ＋ 推一份 MB 级的字节 ＋ 读回。 */
const INSTALL_BUDGET_MS = 600_000;

/**
 * 名字叫 `origin` 的那台换上这一版（也是问会打断什么时认那台的键），`machine` 是框里显示的名字。
 * 用户在确认框里取消 ⇒ `null`（不做、不叫 `onStart`）；部署了 ⇒ 后端那句人读结果；失败 ⇒ 原样抛。
 * 本机：停口上那一位（`backend_stop`）、再起这一版（`backend_start`）⇒ 空串（结局由本机那一行的状态说）；停没停成就不起。
 */
export async function updateBackend(origin: string, machine: string, onStart?: () => void): Promise<string | null> {
  const rows = interruptRows(await askInterrupts(origin), machine, "update");
  if (
    rows.length > 0 &&
    !(await confirmDialog({ title: copyText("interrupts.update.title", { machine }), action: copyText("front.act.update"), rows }))
  ) {
    return null;
  }
  onStart?.();
  if (!isLocalOrigin(origin)) {
    return (await callTyped(LOCAL, "deploy-install", { machine: origin }, budgetWithin(INSTALL_BUDGET_MS))).said;
  }
  await commands.backend_stop({ origin });
  await commands.backend_start({ origin });
  return "";
}

/** 按 origin 找那台的配置再换（主窗口只有 origin）。设置里没有这台 ⇒ 抛（不拿空配置去部署）；本机不在机器表里、不查。 */
export async function updateBackendOf(origin: string, machine: string, onStart?: () => void): Promise<string | null> {
  const known = isLocalOrigin(origin) || (await resolveRemoteConfigByOrigin(origin)) !== null;
  if (!known) throw new Error(copyText("backendDeploy.update.noMachine", { machine }));
  return updateBackend(origin, machine, onStart);
}

/** 卸掉那台的后端（确认框与结果区住机器卡）。 */
export async function uninstallBackend(origin: string): Promise<string> {
  return (await callTyped(LOCAL, "deploy-uninstall", { machine: origin }, budgetWithin(INSTALL_BUDGET_MS))).said;
}
