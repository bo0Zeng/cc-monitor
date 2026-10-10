/**
 * 状态栏「上下文」chip：当前会话最新一轮占了上下文上限的多少（`上下文 44%`）；点开浮层看那一轮用了多少、上限多少、上限从哪来。
 *
 * 数据是后端的会话事实（`history-facts` 的 `usage`：数 ＋ 核心写好的字与语气），经当前 tab 那一格（`TabManager.active`）喂进来；
 * 监控板读同一份。这里只排版：字照抄（判得出上限写百分比、判不出只写用了多少），颜色按核心的语气（快满 ⇒ `warn`）。
 *
 * - 上限判不出（没有百分比）⇒ 浮层里写「上限未知 · 只显示用量」＋［设上限］。
 * - 事实要不到 ⇒ `上下文 —`，浮层里一条错误条＋［重试］。
 * - 还没有一轮回复的会话：不渲染。
 * - 再点 chip / Esc / 点外面 ⇒ 关浮层（kit 浮层的关法）。
 */

import { normalizeModel } from "./views/context-limit";
import { chip } from "./kit/chip";
import type { IconName } from "./kit/icon";
import { closePopover, openPopover, popoverOpenOn } from "./kit/popover";
import { button } from "./kit/button";
import { banner } from "./kit/banner";
import s from "./usage-hud.module.css";
import { copyText } from "./copy-table";
import type { UsageFact } from "./session-reads";

/** chip 点开浮层里要做的两件事（宿主给）。 */
export interface UsageHudHost {
  /** 去设置里改上限。 */
  openSettings(): void;
  /** 当前会话的会话事实再问一次。 */
  retry(): void;
}

/** chip 前面那颗图标。 */
const CHIP_ICON: IconName = "context";

export class UsageHud {
  /** 挂 status-bar 的 chip。 */
  readonly summaryElement: HTMLButtonElement;
  private readonly label: HTMLSpanElement;
  /** 活跃会话的用量成品（后端那一份，照抄）；`null` ＝ 无活跃会话 / 还没有一轮回复。 */
  private usage: UsageFact | null = null;
  /** 活跃会话的会话事实要不到的原因（`null` = 可用）。非空时压过上面那一格（那一格可能是旧的）。 */
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

  /** 换成活跃会话的那一份用量（`null` ⇒ 不渲染）。 */
  setActive(usage: UsageFact | null): void {
    this.usage = usage;
    this.render();
  }

  /** 活跃会话的会话事实要不到 ⇒ 说出来（`null` = 又可用了 / 切到了可用的会话）。 */
  setUnavailable(reason: string | null): void {
    this.unavailable = reason;
    this.render();
  }

  private render(): void {
    const btn = this.summaryElement;
    const shown = this.unavailable !== null || this.usage !== null;
    btn.style.display = shown ? "" : "none";
    if (!shown) {
      btn.dataset.intent = "neutral";
      if (popoverOpenOn(btn)) closePopover();
      return;
    }
    const u = this.usage;
    this.label.textContent = this.unavailable !== null || u === null ? copyText("usageHud.chip.none") : copyText("usageHud.chip.ctx", { ctx: u.contextText });
    btn.title = this.unavailable ?? "";
    btn.dataset.intent = this.unavailable === null && u?.contextTone === "warn" ? "warn" : "neutral";
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
    const u = this.usage;
    if (this.unavailable !== null || u === null) {
      root.appendChild(banner("error", copyText("usageHud.panel.unavailable"), [act(copyText("usageHud.panel.retry"), () => this.host?.retry())]));
      return root;
    }
    const tok = u.promptTokensText;
    const model = normalizeModel(u.model);
    const pct = u.percent;
    const settings = (): void => this.host?.openSettings();
    if (pct === null) {
      const title = line(copyText("usageHud.panel.unknownLimit"));
      title.className = s.panelTitle;
      const sub = line(copyText("usageHud.panel.tokensOnly", { tok, model }));
      sub.className = s.panelLine;
      root.appendChild(title);
      root.append(sub, act(copyText("usageHud.panel.setLimit"), settings));
      return root;
    }
    const title = line(copyText("usageHud.chip.ctx", { ctx: u.contextText }));
    title.className = s.panelTitle;
    const track = document.createElement("div");
    track.className = s.panelTrack;
    const fill = document.createElement("div");
    fill.className = s.panelFill;
    fill.style.transform = `scaleX(${pct / 100})`;
    if (u.contextTone === "warn") root.dataset.intent = "warn";
    track.appendChild(fill);
    const usage = line(copyText("usageHud.panel.usage", { tok, limit: u.limitText, model }));
    usage.className = s.panelLine;
    const src = line(copyText("usageHud.panel.from", { from: u.limitFromText ?? "" }));
    src.className = s.panelSub;
    src.appendChild(act(copyText("usageHud.panel.fix"), settings));
    root.append(title, track, usage, src);
    return root;
  }
}
