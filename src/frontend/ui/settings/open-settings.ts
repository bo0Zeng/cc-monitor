/**
 * ST1「点设置有反馈」。
 *
 * 原先主窗里六处都是 `void commands.open_settings_window()`：不 await、不 disable、不给任何迹象，
 * 失败就变成一条未捕获 rejection 落到状态栏的 `REJ:`。用户点下去，要等新窗口建好才知道点没点上，
 * 于是再点一次 —— 那一次又是一趟建窗。
 *
 * 这里做三件事：
 * ① **在路上时说在路上**：点的那颗按钮（有的话）`disabled` ＋ `aria-busy`，整页光标换成 `progress`；
 * ② **同一时刻只有一趟**：在路上时再点（按钮、快捷键、命令面板、账号 chip……六个入口任一）
 *    都并进同一趟，不再多建一次；
 * ③ **失败说出来**：落在一条提示上，不再是状态栏里一行 `REJ:`。
 *
 * ⚠ 本模块住 `settings/` 下，但它是给**主窗**用的：**不许 import 设置面板**（`panel.ts` 那一族）——
 *   主窗的模块图里不该有设置面板（`tests/frontend/ui/entry-graphs.vitest.ts` 钉着）。它只碰那一条命令。
 * ⚠ 射程：「在路上」量的是那条命令回来之前（后端把窗建好 / 把藏着的窗 show 出来）。
 *   新窗口里的页面自己加载那一段不在这里 —— 那一段归设置窗自己的骨架（`skeleton.ts`）。
 */
import { commands } from "../ipc/commands";
import { toast } from "../kit/toast";
import { copyText } from "../copy-table";

let inFlight: Promise<void> | null = null;

/** 打开（或把藏着的）设置窗拉出来。`trigger` 是用户点的那颗按钮（快捷键 / 命令面板那几路没有）。 */
export function openSettingsWindow(trigger?: HTMLButtonElement | null): Promise<void> {
  if (inFlight) return inFlight;
  const root = document.documentElement;
  const prevCursor = root.style.cursor;
  root.style.cursor = "progress";
  if (trigger) {
    trigger.disabled = true;
    trigger.setAttribute("aria-busy", "true");
  }
  inFlight = commands
    .open_settings_window()
    .catch((e: unknown) => {
      toast(copyText("openSettings.openSettingsWindow.failed"), String(e), { level: "error" });
    })
    .finally(() => {
      inFlight = null;
      root.style.cursor = prevCursor;
      if (trigger) {
        trigger.disabled = false;
        trigger.removeAttribute("aria-busy");
      }
    });
  return inFlight;
}
