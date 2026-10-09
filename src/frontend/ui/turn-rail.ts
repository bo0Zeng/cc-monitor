/**
 * **轮次刻度**：正文区右缘一列，每轮一格（3px 短线），按这一轮在整段会话里的序号均匀排。
 * 视口所在的那一轮 `--accent` 加长；在等你的那一轮（这个会话此刻「需要你」⇒ 最后一轮）琥珀。超过 60 轮相邻的并成一格。
 * 悬停一格 ⇒ 左侧小卡「第 9 轮 · 02:05 · 工具 ×2」· 你那句的第一行 · 回复头三行（kit 悬停卡）。点一格 / `Alt+↑↓` ⇒ 跳到那一轮开头。
 *
 * 轮的数据与过程行、大纲同一份（`turn-fold.ts` 手上的 `history-turns` 成品），本模块只排版；跳经宿主（与查找 / 大纲同一个住址）。
 * 列宽 ≤ 900px 不出（CSS 容器查询，`.stream` 是尺寸容器）。不排定时器：宿主转来 scroll。
 */
import type { TurnSummary } from "./session-reads";
import { copyText } from "./copy-table";
import { delegateTooltip } from "./kit/tooltip";

/** 超过这么多轮，相邻的并成一格。 */
export const RAIL_MAX_TICKS = 60;
/** 正文列不宽于这个（px）就不出刻度。 */
export const RAIL_MIN_COLUMN = 900;
/** 悬停卡的宽（px）。 */
const TIP_WIDTH = 300;

export interface RailHost {
  turns(): readonly TurnSummary[];
  /** 这个会话此刻在等你（「需要你」）。 */
  waiting(): boolean;
  /** 跳到那一轮开头（没加载的由宿主取到再跳）。 */
  jump(uuid: string): void;
  /** 说话那一方叫什么（那一家的短名）；还不知道是哪一家 ⇒ `null`。 */
  speaker?(): string | null;
  /** 这条刻度此刻在不在眼前（它那个 tab 是不是当前的）；不给 ⇒ 一直在。不在眼前不量（量就是逼浏览器排那棵收起的子树），翻出来时宿主调 `shown`。 */
  inFront?(): boolean;
}

/** 轮 ⇒ 格：每格 `[from, to]`（轮的下标，含两头）。不超过上限一轮一格。 */
export function railGroups(n: number, max = RAIL_MAX_TICKS): [number, number][] {
  const per = Math.max(1, Math.ceil(n / max));
  const out: [number, number][] = [];
  for (let i = 0; i < n; i += per) out.push([i, Math.min(n, i + per) - 1]);
  return out;
}

export class TurnRail {
  readonly el: HTMLElement;
  private ticks: HTMLElement[] = [];
  private groups: [number, number][] = [];
  private current = -1;
  /** 这一帧已经排过量刻度了（滚动一帧来好几个 `scroll`，合成一次）。 */
  private scheduled = false;
  /** 「uuid → 第几轮」：轮那一份换了（`turns()` 换了一个数组）才重建。 */
  private indexOf: { turns: readonly TurnSummary[]; map: Map<string, number> } | null = null;

  constructor(
    private readonly scroller: HTMLElement,
    private readonly content: HTMLElement,
    private readonly host: RailHost,
  ) {
    this.el = document.createElement("div");
    this.el.className = "turn-rail";
    this.el.setAttribute("aria-hidden", "true");
    this.el.addEventListener("click", (e) => {
      const tick = (e.target as Element | null)?.closest<HTMLElement>(".turn-tick");
      const turn = tick ? this.host.turns()[Number(tick.dataset.from)] : undefined;
      if (turn) this.host.jump(turn.uuid);
    });
    delegateTooltip(this.el, ".turn-tick", (t) => this.tip(t), { placement: "left", width: () => TIP_WIDTH });
  }

  /** 轮变了 / 「需要你」变了：重排每一格。 */
  render(): void {
    const turns = this.host.turns();
    this.groups = railGroups(turns.length);
    if (this.ticks.length !== this.groups.length) {
      this.el.replaceChildren();
      this.ticks = this.groups.map(() => {
        const t = document.createElement("div");
        t.className = "turn-tick";
        this.el.appendChild(t);
        return t;
      });
    }
    const waiting = this.host.waiting();
    this.groups.forEach(([from, to], i) => {
      const t = this.ticks[i];
      t.dataset.from = String(from);
      t.dataset.to = String(to);
      t.style.top = `${((i + 0.5) / this.groups.length) * 100}%`;
      t.dataset.waiting = String(waiting && to === turns.length - 1);
    });
    this.el.hidden = turns.length === 0 || this.scroller.clientWidth <= RAIL_MIN_COLUMN;
    this.current = -1;
    this.mark();
  }

  /**
   * 宿主转来的 `scroll`：只排一帧，帧里量一次（`mark`）。滚动一帧来好几个 `scroll`；骨架接上 / 补批时的程序化滚动
   * 还夹在增删卡之间 —— 每个都当场量，就是每一下逼浏览器当场排版。
   */
  readonly onScroll = (): void => {
    if (this.host.inFront?.() === false) return;
    this.schedule();
  };

  /** 排一帧量一次（这一帧已排过就不再排）；帧里再看在不在眼前。 */
  private schedule(): void {
    if (this.el.hidden || this.scheduled) return;
    this.scheduled = true;
    // 调度：合批 —— 一帧里的几个 scroll 合成一次量
    requestAnimationFrame(() => {
      this.scheduled = false;
      if (this.el.isConnected) this.mark();
    });
  }

  /** 这条刻度翻到眼前了（它那个 tab 切进来）：下一帧量一次（收起期间的 scroll / 轮的更新都没量）。 */
  shown(): void {
    this.current = -1;
    this.schedule(); // 宿主可能还没把「当前是谁」改过来（`switchTo` 先翻出、后改 activeId）⇒ 帧里再判
  }

  /** 视口所在的那一轮：开头已在视口上沿之上（或就在视口里）的最后一轮 —— 标到它那一格上。不在眼前不量。 */
  private mark(): void {
    if (this.el.hidden || this.host.inFront?.() === false) return;
    const at = this.viewportTurn();
    if (at === this.current) return;
    this.current = at;
    this.groups.forEach(([from, to], i) => (this.ticks[i].dataset.viewport = String(at >= from && at <= to)));
  }

  /** `Alt+↑` / `Alt+↓`：上 / 下一轮。 */
  step(dir: -1 | 1): void {
    const turns = this.host.turns();
    if (turns.length === 0) return;
    const at = this.viewportTurn();
    const next = Math.max(0, Math.min(turns.length - 1, (at < 0 ? (dir > 0 ? -1 : turns.length) : at) + dir));
    this.host.jump(turns[next].uuid);
  }

  /**
   * 开头已过视口上沿（＋8）的最后一轮；一个都没过 ⇒ 第一个已建出来的那一轮。
   * 轮的开头按文档顺序上下排着 ⇒ 二分（量对数个位置；逐个量到视口上沿，贴底看长会话时是几百次）。
   */
  private viewportTurn(): number {
    const turns = this.host.turns();
    if (turns.length === 0) return -1;
    if (this.indexOf?.turns !== turns) this.indexOf = { turns, map: new Map(turns.map((t, i) => [t.uuid, i])) };
    const index = this.indexOf.map;
    const heads: Array<[Element, number]> = [];
    for (const el of Array.from(this.content.children)) {
      const i = index.get(el.getAttribute("data-uuid") ?? "");
      if (i !== undefined) heads.push([el, i]);
    }
    if (heads.length === 0) return -1;
    const top = this.scroller.getBoundingClientRect().top + 8;
    let lo = 0;
    let hi = heads.length - 1;
    let at = -1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (heads[mid][0].getBoundingClientRect().top > top) hi = mid - 1;
      else {
        at = heads[mid][1];
        lo = mid + 1;
      }
    }
    return at < 0 ? heads[0][1] : at;
  }


  private tip(tick: HTMLElement): HTMLElement | null {
    const turns = this.host.turns();
    const from = Number(tick.dataset.from);
    const to = Number(tick.dataset.to);
    const t = turns[from];
    if (!t) return null;
    const card = document.createElement("div");
    card.className = "turn-tip";
    const head = document.createElement("div");
    head.className = "turn-tip-head";
    head.textContent =
      from === to
        ? copyText("stream.rail.tip", { n: from + 1, time: t.startText, tools: t.tools })
        : copyText("stream.rail.tipRange", { from: from + 1, to: to + 1, time: t.startText });
    card.appendChild(head);
    if (t.said) card.appendChild(line("turn-tip-said", copyText("stream.rail.you", { text: t.said })));
    if (from === to && t.reply) {
      // 说话那一方的名字（那一家的短名）；还不知道是哪一家 ⇒ 只写回复本身。
      const who = this.host.speaker?.() ?? null;
      card.appendChild(line("turn-tip-reply", who ? copyText("stream.rail.reply", { who, text: t.reply }) : t.reply));
    }
    return card;
  }
}

function line(cls: string, text: string): HTMLElement {
  const d = document.createElement("div");
  d.className = cls;
  d.textContent = text;
  return d;
}
