/**
 * 设计/97 §8（主会话 09-28 裁 FIX4）「全景小程序卸口：给（受管工具都应可卸，照 SU1 装卸账）」。
 *
 * 机器页「工具」栏一格：卸掉这台机器上的代码全景小程序。问的是**那台机器的后端**（`panorama-uninstall`，本机远端同一条 `chan.call`）：
 * 它先认身份、再经自己的文件管理面只删装时放下的那一份；索引不删、说在哪。破坏性 ⇒ 先问一句。
 * 构造零 I/O（这一格只有按钮），切机器跟着 `machine-context` 走。
 */
import { askConfirm, type ConfirmFn } from "../ask-dialog";
import { copyText } from "../copy-table";
import { chan } from "../ipc/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "../ipc/chan-caller";
import type { Origin } from "../ipc/origin";
import { getCurrentMachine, subscribeMachine } from "./machine-context";

const UNINSTALL_BUDGET_MS = 30_000;

/** 那台后端的成品 ⇒ 一句话。形状不对 ⇒ 抛（两边版本对不上）。 */
export function saidOfUninstall(v: unknown): string {
  const o = v as { removed?: unknown; path?: unknown; index?: unknown } | null;
  if (!o || typeof o.removed !== "boolean" || typeof o.path !== "string" || typeof o.index !== "string") {
    throw new Error(copyText("panorama.uninstall.unreadable"));
  }
  return o.removed
    ? copyText("panorama.uninstall.done", { path: o.path, index: o.index })
    : copyText("panorama.uninstall.absent", { path: o.path });
}

/** 卸 `origin` 上的全景小程序；失败抛一句人话。 */
export async function uninstallPanorama(origin: Origin): Promise<string> {
  try {
    const body = jsonBody({});
    const budget = budgetWithin(UNINSTALL_BUDGET_MS);
    const reply = await chan.call(origin, "panorama-uninstall", body, budget);
    return saidOfUninstall(readJson(reply));
  } catch (e) {
    throw new Error(saidOf(e, copyText("panorama.uninstall.oldBackend")));
  }
}

export class PanoramaSection {
  readonly element: HTMLElement;
  private origin: Origin = getCurrentMachine();
  private readonly status: HTMLElement;

  constructor(private readonly confirm: ConfirmFn = askConfirm) {
    const root = document.createElement("div");
    root.className = "settings-group settings-headless";
    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent = copyText("panorama.uninstall.intro");
    root.appendChild(hint);
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "btn";
    btn.textContent = copyText("panorama.uninstall.action");
    btn.addEventListener("click", () => void this.run(btn));
    root.appendChild(btn);
    this.status = document.createElement("div");
    this.status.className = "settings-hint";
    root.appendChild(this.status);
    this.element = root;
    subscribeMachine((o) => {
      this.origin = o;
      this.status.textContent = "";
    });
  }

  /** 这一格不预读（只有一颗按钮）。 */
  loadNow(): void {}

  async run(btn: HTMLButtonElement): Promise<void> {
    if (!(await this.confirm(copyText("panorama.uninstall.confirm")))) return;
    btn.disabled = true;
    try {
      this.status.textContent = await uninstallPanorama(this.origin);
    } catch (e) {
      this.status.textContent = e instanceof Error ? e.message : String(e);
    } finally {
      btn.disabled = false;
    }
  }
}
