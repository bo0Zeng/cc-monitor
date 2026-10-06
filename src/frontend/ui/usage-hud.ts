/**
 * 状态栏「上下文」chip：当前会话最新一轮占了上下文上限的多少（`上下文 44%`）；点开浮层看那一轮用了多少、上限多少、上限从哪来。
 *
 * 数据来自后端的会话事实（`history-facts` 的 `usage`：最新一轮的 prompt token ＋ 后端定的上限与它的来源），
 * 经当前 tab 那一格（`TabManager.active`）喂进来；监控板读同一个上限。百分比是排版（`views/context-limit.ts`），永远不超过 100。
 *
 * - 上限判不出 ⇒ 只写用了多少（`上下文 350k`），不算百分比、不上色；浮层里写「上限未知 · 只显示用量」＋［设上限］。
 * - ≥ 80% ⇒ 字用琥珀（需要注意，不是出错）。
 * - 事实要不到 ⇒ `上下文 —`，浮层里一条错误条＋［重试］。
 * - 还没有一轮回复的会话：不渲染。
 * - 再点 chip / Esc / 点外面 ⇒ 关浮层（kit 浮层的关法）。
 */

import { contextPercentOf, contextTokensText, normalizeModel } from "./views/context-limit";
import { chip } from "./kit/chip";
import type { IconName } from "./kit/icon";
import { closePopover, openPopover, popoverOpenOn } from "./kit/popover";
import { button } from "./kit/button";
import { banner } from "./kit/banner";
import s from "./usage-hud.module.css";
import { copyText } from "./copy-table";
import type { UsageFact } from "./session-reads";

/** 上限从哪来（后端那一格；`assumed` ＝ 判不出，不显示来源）。 */
export type LimitFrom = UsageFact["limitFrom"];

/** chip 点开浮层里要做的两件事（宿主给）。 */
export interface UsageHudHost {
  /** 去设置里改上限。 */
  openSettings(): void;
  /** 当前会话的会话事实再问一次。 */
  retry(): void;
}

/** chip 前面那颗图标。 */
const CHIP_ICON: IconName = "context";

/** 到这个百分比字用琥珀。 */
export const CONTEXT_WARN_AT = 80;

function limitFromText(from: LimitFrom): string {
  switch (from) {
    case "relay":
      return copyText("usageHud.from.relay");
    case "setting":
      return copyText("usageHud.from.setting");
    case "model":
      return copyText("usageHud.from.model");
    case "observed":
      return copyText("usageHud.from.observed");
    case "assumed":
      return "";
  }
}

export class UsageHud {
  /** 挂 status-bar 的 chip。 */
  readonly summaryElement: HTMLButtonElement;
  private readonly label: HTMLSpanElement;
  private model: string | null = null;
  private promptTokens: number | null = null;
  /** 上下文上限（后端定的，与监控板同一个数）。 */
  private limit: number | null = null;
  private limitFrom: LimitFrom = "assumed";
  /** 活跃会话的会话事实要不到的原因（`null` = 可用）。非空时压过上面几格（那几格可能是旧的）。 */
  private unavailable: string | null = null;
  host: UsageHudHost | null = null;

  constructor() {
    const btn = chip({ text: "", icon: CHIP_ICON, onClick: () => this.toggle() }) as HTMLButtonElement;
    btn.classList.add(s.hudChip);
    btn.setAttribute("aria-expanded", "false");
    btn.style.display = "none"; // 无活跃会话 / 还没有一轮回复 ⇒ 不渲染
    this.label = btn.querySelector("span") as HTMLSpanElement;
    this.summaryElement = btn;
  }

  /**
   * 更新活跃会话的最新 prompt token、模型、上限与来源。
   * `promptTokens` = null（无活跃会话 / 活跃会话尚无带 usage 的 assistant 记录）→ 不渲染。
   */
  setActive(model: string | null, promptTokens: number | null, limit: number | null, limitFrom: LimitFrom = limit === null ? "assumed" : "model"): void {
    this.model = model;
    this.promptTokens = promptTokens;
    this.limit = limit;
    this.limitFrom = limitFrom;
    this.render();
  }

  /** 活跃会话的会话事实要不到 ⇒ 说出来（`null` = 又可用了 / 切到了可用的会话）。 */
  setUnavailable(reason: string | null): void {
    this.unavailable = reason;
    this.render();
  }

  /** 这一刻的百分比（判不出上限 / 没有用量 ⇒ `null`）。 */
  private percent(): number | null {
    if (this.promptTokens == null) return null;
    const pct = contextPercentOf(this.promptTokens, this.limit);
    return pct == null ? null : Math.round(pct);
  }

  private render(): void {
    const btn = this.summaryElement;
    const shown = this.unavailable !== null || this.promptTokens != null;
    btn.style.display = shown ? "" : "none";
    if (!shown) {
      btn.dataset.intent = "neutral";
      if (popoverOpenOn(btn)) closePopover();
      return;
    }
    const pct = this.percent();
    this.label.textContent =
      this.unavailable !== null
        ? copyText("usageHud.chip.none")
        : pct === null
          ? copyText("usageHud.chip.tokens", { tokens: contextTokensText(this.promptTokens ?? 0) })
          : copyText("usageHud.chip.pct", { n: pct });
    btn.title = this.unavailable ?? "";
    btn.dataset.intent = this.unavailable === null && pct !== null && pct >= CONTEXT_WARN_AT ? "warn" : "neutral";
  }

  private toggle(): void {
    const btn = this.summaryElement;
    if (popoverOpenOn(btn)) {
      closePopover();
      return;
    }
    openPopover(btn, this.panel(), { label: copyText("usageHud.panel.label") });
  }

  /** 浮层里那一块（每次打开时按此刻的数现画）。 */
  private panel(): HTMLElement {
    const root = document.createElement("div");
    root.className = s.panel;
    const line = (text: string): HTMLElement => {
      const el = document.createElement("div");
      el.textContent = text;
      return el;
    };
    const act = (label: string, run: () => void): HTMLButtonElement =>
      button({
        label,
        kind: "ghost",
        size: "compact",
        onClick: () => {
          closePopover();
          run();
        },
      });
    if (this.unavailable !== null) {
      root.appendChild(banner("error", copyText("usageHud.panel.unavailable"), [act(copyText("usageHud.panel.retry"), () => this.host?.retry())]));
      return root;
    }
    const tok = contextTokensText(this.promptTokens ?? 0);
    const model = normalizeModel(this.model);
    const pct = this.percent();
    const settings = (): void => this.host?.openSettings();
    if (pct === null || this.limit === null) {
      const title = line(copyText("usageHud.panel.unknownLimit"));
      title.className = s.panelTitle;
      const sub = line(copyText("usageHud.panel.tokensOnly", { tok, model }));
      sub.className = s.panelLine;
      root.appendChild(title);
      root.append(sub, act(copyText("usageHud.panel.setLimit"), settings));
      return root;
    }
    const warn = pct >= CONTEXT_WARN_AT;
    const title = line(copyText("usageHud.chip.pct", { n: pct }));
    title.className = s.panelTitle;
    const track = document.createElement("div");
    track.className = s.panelTrack;
    const fill = document.createElement("div");
    fill.className = s.panelFill;
    fill.style.transform = `scaleX(${pct / 100})`;
    if (warn) root.dataset.intent = "warn";
    track.appendChild(fill);
    const usage = line(copyText("usageHud.panel.usage", { tok, limit: contextTokensText(this.limit), model }));
    usage.className = s.panelLine;
    const src = line(copyText("usageHud.panel.from", { from: limitFromText(this.limitFrom) }));
    src.className = s.panelSub;
    src.appendChild(act(copyText("usageHud.panel.fix"), settings));
    root.append(title, track, usage, src);
    return root;
  }
}
