/**
 * 那台的上下文上限表（状态栏 ctx% 用的模型上限覆盖表）：住**那台**后端（`prefs.json` 的 `contextLimits`，`history-facts` 在那台现读），
 * 界面经 `prefs-read` / `prefs-set` 问那台、不存副本。住机器页「这台上的 cc-monitor」里「上下文上限」那一折（标题行右侧「已改 N」），
 * 主窗口［设上限］落到当前标签页那台的这一折（`data-anchor="context-limits"`）。
 *
 * 一行一条「模型名片段 = 上限 tokens」；全清空 ⇒ 交一张空表。那台读不懂 ⇒ 文本框禁用、说那台写好的那一句（那台也拒写）。
 * 交好之后广播 `SETTINGS_APPLIED_EVENT`，主窗口的 chip 重问。
 */
import { emit } from "@tauri-apps/api/event";
import { copyText } from "../copy-table";
import { fold, setFoldSummary } from "../kit/fold";
import { readPrefs, setPrefs } from "../machine-prefs";
import type { Origin } from "../ipc/origin";
import { SETTINGS_APPLIED_EVENT } from "./events";
import { sayFailure, sayWithDetail } from "../kit/detail";

/** 模型上限覆盖表：模型串**子串**（大小写不敏感）→ 上限 tokens。 */
export type ContextLimitOverrides = Record<string, number>;

/** 纯函数：覆盖表 ⇒ 文本框里的几行。 */
export function limitsToText(t: ContextLimitOverrides): string {
  return Object.entries(t)
    .map(([k, v]) => `${k} = ${v}`)
    .join("\n");
}

/** 纯函数：文本框 ⇒ 覆盖表，或第一处读不懂的那一行（从 1 数）。空行跳过。 */
export function textToLimits(text: string): { ok: ContextLimitOverrides } | { badLine: number } {
  const ok: ContextLimitOverrides = {};
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trim();
    if (line === "") continue;
    const m = /^(.+?)\s*=\s*(\d+)$/.exec(line);
    const n = m ? Number(m[2]) : NaN;
    if (!m || m[1].trim() === "" || !Number.isSafeInteger(n) || n <= 0) return { badLine: i + 1 };
    ok[m[1].trim()] = n;
  }
  return { ok };
}

export class ContextLimitsSection {
  readonly element: HTMLElement;
  private readonly box: HTMLTextAreaElement;
  private readonly status: HTMLElement;
  private open = false;
  private readonly origin: Origin;

  constructor(origin: Origin) {
    this.origin = origin;
    const body = document.createElement("div");
    body.className = "ctx-limits-body";
    const intro = document.createElement("div");
    intro.className = "settings-hint";
    intro.textContent = copyText("contextLimits.editor.intro");
    this.box = document.createElement("textarea");
    this.box.className = "settings-input settings-input-mono ctx-limits-box";
    this.box.rows = 4;
    this.box.spellcheck = false;
    this.box.setAttribute("aria-label", copyText("contextLimits.editor.field"));
    // 设置页全即时（没有「保存」钮）：离开这一格（`change`）就存。
    this.box.addEventListener("change", () => void this.save());
    this.status = document.createElement("div");
    this.status.className = "settings-hint";
    body.append(intro, this.box, this.status);
    this.element = fold({
      title: copyText("contextLimits.editor.title"),
      open: this.open,
      body,
      onToggle: (o) => (this.open = o),
    });
    this.element.classList.add("ctx-limits");
    this.element.dataset.anchor = "context-limits";
    this.box.disabled = true;
  }

  /** 带目的地跳过来（主窗口［设上限］）：展开。 */
  reveal(): void {
    const head = this.element.querySelector<HTMLElement>("[aria-expanded]");
    if (head?.getAttribute("aria-expanded") === "false") head.click();
  }

  /** 标题行右侧「已改 N」（覆盖几条；0 不写）。 */
  private paintCount(t: ContextLimitOverrides): void {
    const n = Object.keys(t).length;
    setFoldSummary(this.element, n > 0 ? copyText("contextLimits.editor.count", { n }) : "");
  }

  /** 问那台（建好不问：机器页第一次可见时宿主调）。 */
  async load(): Promise<void> {
    try {
      const r = await readPrefs(this.origin);
      const t = r.prefs.contextLimits;
      this.box.value = limitsToText(t);
      this.paintCount(t);
      if (r.state === "unreadable") {
        sayWithDetail(this.status, r.reason ?? copyText("machinePrefs.read.failed"), r.detail ?? "");
        return;
      }
      this.box.disabled = false;
    } catch (e) {
      sayFailure(this.status, copyText("machinePrefs.read.failed"), e);
    }
  }

  async save(): Promise<void> {
    if (this.box.disabled) return;
    const parsed = textToLimits(this.box.value);
    if ("badLine" in parsed) {
      this.status.textContent = copyText("contextLimits.editor.badLine", { n: parsed.badLine });
      return;
    }
    const empty = Object.keys(parsed.ok).length === 0;
    try {
      const r = await setPrefs(this.origin, { contextLimits: parsed.ok });
      this.status.textContent = empty ? copyText("contextLimits.editor.cleared") : copyText("contextLimits.editor.saved");
      this.paintCount(r.prefs.contextLimits);
      emit(SETTINGS_APPLIED_EVENT).catch((e: unknown) => console.warn("[context-limits] 通知不到主窗口：", e));
    } catch (e) {
      sayFailure(this.status, copyText("contextLimits.editor.saveFailed"), e);
    }
  }
}
