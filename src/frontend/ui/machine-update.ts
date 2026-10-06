/**
 * 主窗口里报错的［更新］（那台的 cc-monitor 旧）：不开设置，就地把这一版换到那台。
 * 先问会打断什么（有才弹框，与设置窗机器卡那一颗同一份问法 `settings/interrupts.ts`），再交壳部署；
 * 结果一条提示（壳回的那句人话 / 失败原因）。
 */
import { askInterrupts, interruptRows } from "./settings/interrupts";
import { confirmDialog } from "./kit/dialog";
import { toast } from "./kit/toast";
import { commands } from "./ipc/commands";
import { resolveRemoteConfigByOrigin } from "./remote-config";
import { machineName } from "./control-said";
import { copyText } from "./copy-table";
import type { Origin } from "./ipc/origin";

export async function updateMachine(origin: Origin): Promise<void> {
  const machine = machineName(origin);
  const cfg = await resolveRemoteConfigByOrigin(origin);
  if (!cfg) {
    toast(copyText("machineUpdate.result.failed", { machine }), copyText("machineUpdate.result.noConfig"), { level: "error" });
    return;
  }
  const rows = interruptRows(await askInterrupts(origin), machine, "update");
  if (rows.length > 0 && !(await confirmDialog({ title: copyText("interrupts.update.title", { machine }), action: copyText("interrupts.update.action"), rows }))) return;
  try {
    const said = await commands.deploy_remote_backend({ cfg });
    toast(copyText("machineUpdate.result.done", { machine }), said, { level: "success" });
  } catch (e) {
    toast(copyText("machineUpdate.result.failed", { machine }), String(e), { level: "error" });
  }
}
