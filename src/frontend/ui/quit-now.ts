/**
 * 退出 cc-monitor（关主窗口、而进程会随它一起退）之前：先问会打断什么（与「现在重启」同一问 `askAppExitInterrupts`、同一种框），
 * 有才弹（默认焦点在［取消］）；取消 ⇒ 不关。什么都不断 ⇒ 直接关。
 *
 * 进程退不退照壳那一条：主窗口销毁时壳只顺手收掉设置窗，还开着的查看窗 / agent 窗会把进程吊住 ⇒ 那时关主窗不是退出，不问。
 * ⚠ 买不到：主窗口先关、最后一个查看窗后关（那一下才退）这条路不问。
 */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getAllWebviewWindows } from "@tauri-apps/api/webviewWindow";
import { copyText } from "./copy-table";
import { confirmDialog, type ConfirmFn } from "./kit/dialog";
import { askAppExitInterrupts, interruptRows, type Interrupts } from "./settings/interrupts";

export interface QuitDeps {
  ask?: () => Promise<Interrupts>;
  confirm?: ConfirmFn;
  /** 关了主窗口进程会不会退（除主窗口与设置窗之外没有别的窗口）。 */
  exits?: () => Promise<boolean>;
}

/** 壳那份标签（`lib.rs::MAIN_WINDOW_LABEL` · `SETTINGS_WINDOW_LABEL`）：主窗口销毁时设置窗跟着收，不吊进程。 */
const NOT_HOLDING = new Set(["main", "settings"]);

async function closingExits(): Promise<boolean> {
  try {
    return (await getAllWebviewWindows()).every((w) => NOT_HOLDING.has(w.label));
  } catch {
    return true;
  }
}

/** 关主窗口那一下放不放行。回 `false` ＝ 用户在框里取消了。 */
export async function quitNow(deps: QuitDeps = {}): Promise<boolean> {
  if (!(await (deps.exits ?? closingExits)())) return true;
  const rows = interruptRows(await (deps.ask ?? askAppExitInterrupts)(),copyText("restartNow.dialog.everywhere"), "stop");
  if (rows.length === 0) return true;
  return (deps.confirm ?? confirmDialog)({
    title: copyText("quitNow.dialog.title"),
    action: copyText("quitNow.dialog.action"),
    danger: false,
    rows,
  });
}

/** 接管主窗口的关窗（系统标题栏的 X · 窗口管理器的关）：先 [`quitNow`]，放行才销毁。挂不上 ⇒ 关窗照默认（只少了这一问）。 */
export function installQuitGuard(): void {
  try {
    const w = getCurrentWindow();
    let asking = false;
    void w
      .onCloseRequested(async (e) => {
        e.preventDefault();
        if (asking) return;
        asking = true;
        try {
          if (await quitNow()) await w.destroy();
        } finally {
          asking = false;
        }
      })
      .catch((e: unknown) => console.warn("[quit] 挂关窗拦截失败：", e));
  } catch (e) {
    console.warn("[quit] 关窗接管失败：", e);
  }
}
