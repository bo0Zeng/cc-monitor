/**
 * **按轮折叠**：完成的轮把过程折成一行「过程 · 工具 ×8 · 思考 ×1 · 失败 ×1 · 02:01–02:04 · 3m02s」，
 * 结论常显。一轮的边界、工具 / 思考 / 失败数、结论是哪几条，**都是后端的 `history-turns`**（`session-reads.ts::readTurns`）；
 * 本模块只做两件事：要那份成品（续取从还没收尾的那一轮的 `at` 起，整轮重算），以及按它给流里已建好的卡排版。
 *
 * # 排版的口径（界面不判谁是过程）
 *
 * - 顺着流的顶层子节点走：遇到 `data-uuid` 等于某一轮 `uuid` 的那张卡 ＝ 那一轮开头；之后的卡属于这一轮，直到下一轮开头。
 * - 只有三种卡会被折：Claude 的正文卡（不是结论那几条）· 工具组 · 重试细条。人说的、事件条、报错卡、压缩摘要一律不折。
 * - 遇到骨架占位或 ESC 回退的折叠段 ⇒ 不知道里面有没有下一轮的开头 ⇒ 之后不再归轮，直到认出下一轮开头（宁可不折，不折错）。
 * - 过程行插在那一轮开头那张卡后面；它不是时间线条目（`RecordTimeline` 按后继锚插入，不受它影响），
 *   `BranchFolder` 认的是 `data-uuid`，过程行没有。
 * - 正在跑的那一轮：过程展开（工具组也展开、不再另起一层「过程 · 工具 ×N」）；收尾时视口正落在它的过程里 ⇒ 不当面收起，
 *   等滚出去（宿主转来的 scroll）或切走再收（I6）。
 *
 * # 展开状态
 *
 * 默认展开与否是每扇窗一个开关（会话头「⋯」·`Ctrl+O`，`LS_KEYS.processExpanded`）；点过程行 ⇒ 这一轮单独记住，实时更新不重置。
 * 切开关 ⇒ 各轮单独记的作废（`Ctrl+O` ＝ 全部收起 / 全部展开）。
 *
 * 本模块不排定时器（`polling_registry`）：触发全靠宿主（新记录到了 · 批结束 · scroll · 切 tab）与 DOM 变动（`MutationObserver`）。
 */
import { readTurns, type TurnSummary, type TurnsResult } from "./session-reads";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { durBetween, fmtStepDur } from "./cards/step-line";
import { icon } from "./kit/icon";
import { SKELETON_GAP_CLASS } from "./skeleton-view";
import { LS_KEYS, safeGet, safeSet } from "./local-storage";

export const PROC_LINE_CLASS = "proc-line";
/** 折起来的过程卡（`display:none`）。 */
const FOLD_WRAP_CLASS = "branch-fold-wrap";
/** 会被折进过程的卡型（白名单：认不出的卡一律常显）。 */
const FOLDABLE = ["card-assistant", "card-tool-group", "card-api-retry", "card-injected"] as const;
/** 连续要不到几次就不再要（同大纲 / 事实的口径）。 */
const MAX_FAILURES = 3;

/** 这份会话在哪（路径要等首条行回填 ⇒ 每次现取；拿不到 ⇒ 这一趟不要）。 */
export type TurnsWhere = () => { origin: Origin; jsonlPath: string } | null;
type Read = (origin: Origin, path: string, from: number) => Promise<TurnsResult>;

/** 「显示系统注入」这扇窗的开关（缺省不露）。 */
export function injectedShownDefault(): boolean {
  return safeGet(LS_KEYS.showInjected) === "1";
}

export function setInjectedShownDefault(on: boolean): void {
  safeSet(LS_KEYS.showInjected, on ? "1" : "0");
}

/** 「过程默认展开」这扇窗的开关（缺省收起）。 */
export function processExpandedDefault(): boolean {
  return safeGet(LS_KEYS.processExpanded) === "1";
}

export function setProcessExpandedDefault(on: boolean): void {
  safeSet(LS_KEYS.processExpanded, on ? "1" : "0");
}

const hasProcess = (t: TurnSummary): boolean => t.tools > 0 || t.thinking > 0;
const foldable = (el: Element): boolean => FOLDABLE.some((c) => el.classList.contains(c));

export class TurnFold {
  private turns: TurnSummary[] = [];
  private byUuid = new Map<string, TurnSummary>();
  private lines = new Map<string, HTMLButtonElement>();
  /** 这一轮单独点过：`true` 展开 / `false` 收起。 */
  private overrides = new Map<string, boolean>();
  /** 收尾了、但收尾那一刻视口落在它的过程里 ⇒ 先不收（等滚出去 / 切走）。 */
  private held = new Set<string>();
  /** 上一次排版时还在跑的那几轮（据此认出「刚收尾」）。 */
  private running = new Set<string>();
  private inflight = false;
  private again = false;
  private gen = 0;
  private failures = 0;
  private gaveUp = false;
  private readonly mo: MutationObserver;
  private expandedDefault: boolean;

  constructor(
    private readonly content: HTMLElement,
    private readonly scroller: HTMLElement,
    private readonly where: TurnsWhere,
    private readonly read: Read = readTurns,
  ) {
    this.expandedDefault = processExpandedDefault();
    // 卡进出流（实时到达 · 上翻补批 · 骨架物化 · ESC 折叠重排）⇒ 重排一遍；排版自己插的过程行不算。
    this.mo = new MutationObserver((recs) => {
      if (recs.some((r) => [...r.addedNodes, ...r.removedNodes].some((n) => !(n instanceof HTMLElement && n.classList.contains(PROC_LINE_CLASS))))) this.apply();
    });
    this.mo.observe(content, { childList: true });
  }

  /** 轮变了（刻度据此重排）。 */
  onTurns: (() => void) | null = null;

  /** 已知的轮（刻度 · 大纲用同一份）。 */
  get all(): readonly TurnSummary[] {
    return this.turns;
  }

  /** 又长了：从还没收尾的那一轮（都收尾了 ⇒ 最后一轮）的开头再要一次；在途 ⇒ 回来后只补一趟。 */
  async refresh(): Promise<void> {
    if (this.gaveUp) return;
    if (this.inflight) {
      this.again = true;
      return;
    }
    const where = this.where();
    if (!where) return;
    this.inflight = true;
    const gen = this.gen;
    try {
      const open = this.turns.find((t) => !t.done) ?? this.turns[this.turns.length - 1];
      const from = open?.at ?? 0;
      let res = await this.read(where.origin, where.jsonlPath, from);
      if (gen !== this.gen) return;
      // 续点不在了（文件被截断 / 重写）⇒ 从 0 整份重要。
      if (!res.available && from > 0) {
        res = await this.read(where.origin, where.jsonlPath, 0);
        if (gen !== this.gen) return;
        if (res.available) this.turns = [];
      }
      if (!res.available) {
        if (++this.failures >= MAX_FAILURES) this.gaveUp = true;
        return;
      }
      this.failures = 0;
      this.turns = [...this.turns.filter((t) => t.at < res.from), ...res.turns];
      this.byUuid = new Map(this.turns.map((t) => [t.uuid, t]));
      this.apply();
      this.onTurns?.();
    } finally {
      if (gen === this.gen) {
        this.inflight = false;
        if (this.again) {
          this.again = false;
          void this.refresh();
        }
      }
    }
  }

  /** `Ctrl+O` / 会话头开关：默认展开与否；各轮单独记的作废。 */
  setDefault(expanded: boolean): void {
    this.expandedDefault = expanded;
    this.overrides.clear();
    this.held.clear();
    this.apply();
  }

  /** scroll 监听（宿主挂 / 摘同一个引用）。 */
  readonly releaseOnScroll = (): void => this.release();

  /** 宿主转来的 scroll / 切走：先不收的那几轮，视口已经不在它的过程里 ⇒ 收。 */
  release(force = false): void {
    if (this.held.size === 0) return;
    for (const uuid of [...this.held]) if (force || !this.inView(uuid)) this.held.delete(uuid);
    this.apply();
  }

  /** 按手上的那份成品给流里的卡排版（O(顶层卡数)）。 */
  apply(): void {
    const seen = new Set<string>();
    const nowRunning = new Set<string>();
    let cur: TurnSummary | null = null;
    let conclusion = new Set<string>();
    for (const el of Array.from(this.content.children)) {
      if (!(el instanceof HTMLElement) || el.classList.contains(PROC_LINE_CLASS)) continue;
      const uuid = el.getAttribute("data-uuid");
      const turn = uuid ? this.byUuid.get(uuid) : undefined;
      if (turn) {
        cur = turn;
        conclusion = new Set(turn.conclusion);
        if (!turn.done) nowRunning.add(turn.uuid);
        else if (this.running.has(turn.uuid) && this.inView(turn.uuid)) this.held.add(turn.uuid);
        if (turn.done && hasProcess(turn)) {
          seen.add(turn.uuid);
          this.placeLine(el, turn);
        }
        this.unmark(el);
        continue;
      }
      if (el.classList.contains(SKELETON_GAP_CLASS) || el.classList.contains(FOLD_WRAP_CLASS)) cur = null;
      if (!cur || !foldable(el) || (uuid !== null && conclusion.has(uuid))) {
        this.unmark(el);
        continue;
      }
      this.mark(el, cur);
    }
    for (const [uuid, line] of this.lines) {
      if (seen.has(uuid)) continue;
      line.remove();
      this.lines.delete(uuid);
    }
    this.running = nowRunning;
    this.mo.takeRecords();
  }

  /** 关 tab / 内容整份重来：断观察、在途那趟作废。 */
  dispose(): void {
    this.gen++;
    this.mo.disconnect();
    this.lines.clear();
  }

  private expanded(turn: TurnSummary): boolean {
    if (!turn.done || this.held.has(turn.uuid)) return true;
    return this.overrides.get(turn.uuid) ?? this.expandedDefault;
  }

  private mark(el: HTMLElement, turn: TurnSummary): void {
    el.dataset.procOf = turn.uuid;
    const open = this.expanded(turn);
    el.classList.toggle("proc-hidden", !open);
    // 过程里的工具组不再另起一层「过程 · 工具 ×N」：展开的过程里它整组摊开（收着那一行由 CSS 藏掉）。
    if (open && el instanceof HTMLDetailsElement && el.classList.contains("card-tool-group")) el.open = true;
  }

  private unmark(el: HTMLElement): void {
    if (el.dataset.procOf === undefined) return;
    delete el.dataset.procOf;
    el.classList.remove("proc-hidden");
  }

  /** 过程行紧跟在那一轮开头那张卡后面（已在就原地重写字）。 */
  private placeLine(head: HTMLElement, turn: TurnSummary): void {
    let line = this.lines.get(turn.uuid);
    if (!line) {
      line = document.createElement("button");
      line.type = "button";
      line.className = PROC_LINE_CLASS;
      line.dataset.turn = turn.uuid;
      line.addEventListener("click", () => this.toggle(turn.uuid));
      this.lines.set(turn.uuid, line);
    }
    if (head.nextElementSibling !== line) head.after(line);
    paintLine(line, turn, this.expanded(turn));
  }

  private toggle(uuid: string): void {
    const turn = this.byUuid.get(uuid);
    if (!turn) return;
    this.held.delete(uuid);
    this.overrides.set(uuid, !this.expanded(turn));
    this.apply();
  }

  /** 这一轮的过程有一张卡与视口相交。 */
  private inView(uuid: string): boolean {
    const box = this.scroller.getBoundingClientRect();
    if (box.height === 0) return false; // 后台 tab：不在眼前
    for (const el of Array.from(this.content.children)) {
      if (!(el instanceof HTMLElement) || el.dataset.procOf !== uuid) continue;
      const r = el.getBoundingClientRect();
      if (r.bottom > box.top && r.top < box.bottom) return true;
    }
    return false;
  }
}

/** 过程行的字：`› 过程 · 工具 ×8 · 思考 ×1 · 失败 ×1`（失败那一段 `--error`）＋ 靠右 `02:01–02:04 · 3m02s`。 */
export function paintLine(line: HTMLElement, turn: TurnSummary, open: boolean): void {
  line.setAttribute("aria-expanded", String(open));
  line.replaceChildren();
  line.appendChild(icon(open ? "caretDown" : "caretRight", "compact"));
  const text = document.createElement("span");
  text.className = "proc-text";
  line.appendChild(text);
  const part = (t: string, cls?: string): void => {
    if (text.childNodes.length > 0) text.append(copyText("kit.text.sep"));
    const s = document.createElement("span");
    if (cls) s.className = cls;
    s.textContent = t;
    text.appendChild(s);
  };
  part(copyText("stream.proc.head"));
  if (turn.tools > 0) part(copyText("stream.proc.tools", { n: turn.tools }));
  if (turn.thinking > 0) part(copyText("stream.proc.thinking", { n: turn.thinking }));
  if (turn.fails > 0) part(copyText("stream.proc.fails", { n: turn.fails }), "proc-fails");
  // 起止 · 用时：靠右（稿 01 / 10）。
  const span = document.createElement("span");
  span.className = "proc-span";
  const from = turn.startText;
  const to = turn.endText;
  span.textContent = from === to ? from : `${from}–${to}`;
  const dur = durBetween(turn.start, turn.end);
  if (dur !== null && dur >= 1000) span.append(copyText("kit.text.sep"), fmtStepDur(dur));
  line.appendChild(span);
}
