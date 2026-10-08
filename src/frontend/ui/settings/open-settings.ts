/**
 * 打开设置窗（主窗口那几个入口都走这里），可带一个目的地。
 *
 * - 在路上时：点的那颗按钮禁用 ＋ `aria-busy`，整页光标 `progress`；同一时刻只一趟，再点并进去。
 * - 失败落在一条提示上。
 *
 * # 目的地 `SettingsTarget`（唯一定义处）
 *
 * 四格都可空，认不出的值一律忽略（照常打开、停在上次那页）：
 * - `page`：`machines` · `data`（文件与数据）· `ext` · `appearance` · `general` · `logs`；给了 `machine` 时可省。
 * - `machine`：机器的 origin（本机 `"<local>"`，远端是它在列表里的名字）⇒ 那台的页。
 * - `tab`：机器页里的栏，`acct`（账号）· `config`（别名与配置文件）。
 * - `anchor`：页里带 `data-anchor="<值>"` 的那一节；滚进视野并高亮 1.5 秒。没给 ⇒ 高亮页头。
 *
 * 壳把它原样交给设置窗（已开着的收 `settings-target` 事件，新建的从初始化脚本里读）；
 * 设置窗那一侧由 `parseSettingsTarget` 收。
 *
 * ⚠ 本模块是给主窗用的：不许 import 设置面板。
 */
import { commands } from "../ipc/commands";
import { toast } from "../kit/toast";
import { copyText } from "../copy-table";
import { detailOf } from "../kit/detail";

export interface SettingsTarget {
  page?: string;
  machine?: string;
  tab?: string;
  anchor?: string;
}

/** 设置窗收到的目的地：只留四格里是非空串的。不是对象 / 解析不了 ⇒ `null`。 */
export function parseSettingsTarget(raw: unknown): SettingsTarget | null {
  let v: unknown = raw;
  if (typeof v === "string") {
    try {
      v = JSON.parse(v);
    } catch {
      return null;
    }
  }
  if (!v || typeof v !== "object" || Array.isArray(v)) return null;
  const o = v as Record<string, unknown>;
  const out: SettingsTarget = {};
  for (const k of ["page", "machine", "tab", "anchor"] as const) {
    const x = o[k];
    if (typeof x === "string" && x.trim() !== "") out[k] = x.trim();
  }
  return out;
}

let inFlight: Promise<void> | null = null;

/** 打开（或把藏着的）设置窗拉出来。`trigger` 是用户点的那颗按钮；`target` 见头注。 */
export function openSettingsWindow(trigger?: HTMLButtonElement | null, target?: SettingsTarget): Promise<void> {
  if (inFlight) return inFlight;
  const root = document.documentElement;
  const prevCursor = root.style.cursor;
  root.style.cursor = "progress";
  if (trigger) {
    trigger.disabled = true;
    trigger.setAttribute("aria-busy", "true");
  }
  inFlight = commands
    .open_settings_window(target ? JSON.stringify(target) : null)
    .catch((e: unknown) => {
      toast(copyText("openSettings.openSettingsWindow.failed"), String(e), { detail: detailOf(e), level: "error" });
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
