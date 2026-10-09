/**
 * 「现在重启」（重启条与行内那一句共用）：先问本机与各台会打断什么（I12），有才弹框（默认焦点在［取消］），
 * 确认 / 没有要打断的 ⇒ toast 一句、壳重起 cc-monitor（`restart_app`）。
 */
import { commands } from "../ipc/commands";
import { copyText } from "../copy-table";
import { confirmDialog, type ConfirmFn } from "../kit/dialog";
import { toast, failToast } from "../kit/toast";
import { button } from "../kit/button";
import { hostKey, readRemoteConfig } from "../remote-config";
import { askAppExitInterrupts, interruptRows, type Interrupts } from "./interrupts";

export interface RestartDeps {
  ask?: (remotes: readonly string[]) => Promise<Interrupts>;
  confirm?: ConfirmFn;
  restart?: () => Promise<void>;
  remotes?: () => Promise<string[]>;
}

async function enabledRemotes(): Promise<string[]> {
  try {
    return (await readRemoteConfig()).hosts.filter((h) => h.connect).map(hostKey).filter((h) => h.trim() !== "");
  } catch {
    return [];
  }
}

/** 点了［现在重启］。回 `false` ＝ 用户在框里取消了。 */
export async function restartNow(deps: RestartDeps = {}): Promise<boolean> {
  const remotes = await (deps.remotes ?? enabledRemotes)();
  const rows = interruptRows(await (deps.ask ?? askAppExitInterrupts)(remotes), copyText("restartNow.dialog.everywhere"), "restart");
  if (rows.length > 0) {
    const ok = await (deps.confirm ?? confirmDialog)({
      title: copyText("restartNow.dialog.title"),
      action: copyText("backend.restart.action"),
      danger: false,
      rows,
    });
    if (!ok) return false;
  }
  toast(copyText("restartNow.run.toast"), "", { level: "info" });
  try {
    await (deps.restart ?? commands.restart_app)();
  } catch (e) {
    failToast(copyText("restartNow.run.failed"), e, { level: "error" });
  }
  return true;
}

/** 那一颗［现在重启］按钮（重启条与行内那一句用同一颗）。 */
export function restartNowButton(deps: RestartDeps = {}): HTMLButtonElement {
  const b = button({
    label: copyText("restartNow.bar.action"),
    size: "compact",
    onClick: () => {
      b.disabled = true;
      void restartNow(deps).finally(() => {
        b.disabled = false;
      });
    },
  });
  b.classList.add("settings-restart-now");
  b.dataset.action = "restart-now";
  return b;
}
