/**
 * config.json 的 `contextLimits`（状态栏 ctx% 用的模型上限覆盖表）在设置页露出来，可读可改 ——
 * 「每个配置键要么有入口要么删」。住「通用」页末「高级：上下文上限」那一折（标题行右侧「已改 N」）。
 *
 * 一行一条「模型名片段 = 上限 tokens」；空 ⇒ 删掉那一键（不覆盖）。读回走 `views/context-limit.ts::readContextLimits`（与状态栏同一份），
 * 写只经 `config.ts::patchConfig`；存好之后广播 `SETTINGS_APPLIED_EVENT`，主窗口的 chip 重读。
 */
import { emit } from "@tauri-apps/api/event";
import { loadConfig, patchConfig, removeAt, setAt } from "../config";
import { copyText } from "../copy-table";
import { fold, setFoldSummary } from "../kit/fold";
import { readContextLimits, type ContextLimitOverrides } from "../views/context-limit";
import { SETTINGS_APPLIED_EVENT } from "./events";
import { sayFailure } from "../kit/detail";

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

  constructor() {
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
    void this.load();
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

  async load(): Promise<void> {
    try {
      const cfg = (await loadConfig()) as Record<string, unknown>;
      const t = readContextLimits(cfg["contextLimits"]);
      this.box.value = limitsToText(t);
      this.paintCount(t);
    } catch (e) {
      sayFailure(this.status, copyText("contextLimits.editor.loadFailed"), e);
    }
  }

  async save(): Promise<void> {
    const parsed = textToLimits(this.box.value);
    if ("badLine" in parsed) {
      this.status.textContent = copyText("contextLimits.editor.badLine", { n: parsed.badLine });
      return;
    }
    const empty = Object.keys(parsed.ok).length === 0;
    try {
      await patchConfig([empty ? removeAt(["contextLimits"]) : setAt(["contextLimits"], parsed.ok)]);
      this.status.textContent = empty ? copyText("contextLimits.editor.cleared") : copyText("contextLimits.editor.saved");
      this.paintCount(parsed.ok);
      emit(SETTINGS_APPLIED_EVENT).catch((e: unknown) => console.warn("[context-limits] 通知不到主窗口：", e));
    } catch (e) {
      sayFailure(this.status, copyText("contextLimits.editor.saveFailed"), e);
    }
  }
}
