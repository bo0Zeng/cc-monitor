/**
 * config.json 的 `contextLimits`（状态栏 ctx% 用的模型上限覆盖表）在设置页露出来，可读可改 ——
 * 「每个配置键要么有入口要么删」。住「应用 → 外观」下面「高级」那一折（它改的是状态栏那一格怎么算）。
 *
 * 一行一条「模型名片段 = 上限 tokens」；空 ⇒ 删掉那一键（不覆盖）。读回走 `views/context-limit.ts::readContextLimits`（与状态栏同一份），
 * 写只经 `config.ts::patchConfig`；存好之后广播 `SETTINGS_APPLIED_EVENT`，主窗口的 chip 重读。
 */
import { emit } from "@tauri-apps/api/event";
import { loadConfig, patchConfig, removeAt, setAt } from "../config";
import { copyText } from "../copy-table";
import { readContextLimits, type ContextLimitOverrides } from "../views/context-limit";
import { SETTINGS_APPLIED_EVENT } from "./events";

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

  constructor() {
    const el = document.createElement("details");
    el.className = "settings-group";
    const sum = document.createElement("summary");
    sum.textContent = copyText("contextLimits.editor.title");
    el.appendChild(sum);
    const intro = document.createElement("div");
    intro.className = "settings-hint";
    intro.textContent = copyText("contextLimits.editor.intro");
    el.appendChild(intro);
    const label = document.createElement("label");
    label.className = "settings-label";
    label.textContent = copyText("contextLimits.editor.field");
    this.box = document.createElement("textarea");
    this.box.className = "settings-input";
    this.box.rows = 4;
    label.appendChild(this.box);
    el.appendChild(label);
    // 设置页全即时（没有「保存」钮）：离开这一格（`change`）就存。
    this.box.addEventListener("change", () => void this.save());
    this.status = document.createElement("div");
    this.status.className = "settings-hint";
    el.appendChild(this.status);
    this.element = el;
    void this.load();
  }

  async load(): Promise<void> {
    try {
      const cfg = (await loadConfig()) as Record<string, unknown>;
      this.box.value = limitsToText(readContextLimits(cfg["contextLimits"]));
    } catch (e) {
      this.status.textContent = copyText("contextLimits.editor.loadFailed", { e: String(e) });
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
      emit(SETTINGS_APPLIED_EVENT).catch((e: unknown) => console.warn("[context-limits] 通知不到主窗口：", e));
    } catch (e) {
      this.status.textContent = copyText("contextLimits.editor.saveFailed", { e: String(e) });
    }
  }
}
