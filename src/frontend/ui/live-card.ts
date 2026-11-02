/**
 * **活卡**：壳交来的成品照画（会话流 `session-live-card`，一格 = 那台某会话某运行此刻整份的卡，或某张卡只换了正文的尾巴）。
 * 对账 · 缺口 · 记录到了整轮覆盖 · 子运行归位 · 截头 · 挤谁在后端（`src/backend/stream/live.rs`）；一帧一帧折好在端核心
 * （`src/comms/resident-client/live.rs`，壳与手机核心同一份）。这里不认 `op`、不对号：整份一格换掉那一处手上那一份；
 * 尾巴一格把那张卡的正文前 `keep` 块原样留着（同一批对象）、后面换成交来的那几块 —— 画法照对象认哪几块没变。
 *
 * {@link LiveCards} 把每个 tab 此刻的卡画到它流尾巴上的那一块（`MessageStream.trailerElement`）。活卡**不进时间线**：
 * 不占 seq、不进去重集、不进大纲 / 查找 / 改动集 —— 全会话事实只读 json。主 tab 只画主运行的卡（`run` 缺）。
 */
import type { LiveCardNow } from "./generated/LiveCardNow";
import type { LiveCards as LivePlace } from "./generated/LiveCards";
import type { LiveTail } from "./generated/LiveTail";
import type { MdBlock } from "./generated/MdBlock";
import type { SessionLiveCardsPayload } from "./generated/SessionLiveCardsPayload";
import type { RunInfo } from "./generated/RunInfo";
import { RunBoard, type RunTable } from "./runs";

/** 一张活卡此刻的样子（给视图 / 判据读）：头一行与正文是端核心折卡时写好的（`resident-client` 的活卡板），这里照摆、不按块的种类取字。 */
export interface LiveCardState {
  key: string;
  messageId: string;
  /** `streaming` = 上游还在说；`awaiting` = 上游说完了、那一轮的记录还没到（只挑样子）。 */
  phase: "streaming" | "awaiting";
  head: string;
  /** 正文的顶层块（后端解析好的树；尾巴一格之后，前面没变的那几块还是同一批对象）。活卡不拉高亮。 */
  body: readonly MdBlock[];
}

/** 手上那一处（某台某会话某运行）最新那一份。 */
interface Held {
  origin: string;
  sid: string;
  run: string | null;
  cards: LiveCardNow[];
}

/** 哪一处变了。 */
interface At {
  sid: string;
  run: string | null;
}

const stateOf = (origin: string, c: LiveCardNow): LiveCardState => ({
  key: `${origin}\u0000${c.card}`,
  messageId: c.rid,
  phase: c.phase,
  head: c.head,
  body: c.body,
});

/** 宿主给视图的口：某个 tab 流尾巴上的那一块（没有这个 tab ⇒ `null`）。 */
export type TrailerOf = (sid: string) => HTMLElement | null;

/**
 * 把一个 tab 此刻的活卡画进它流尾巴上那一块（同一张卡沿用节点、正文只重建变了的尾巴）。实现住 `live-card-view.ts`（带 `.module.css`），
 * **由主窗口的入口装进来**（`TabManager.setLivePainter`）：本文件被主窗口与独立查看器两个入口共用（进共享块），
 * 而 `.module.css` 进共享块会让它的规则排在主窗口全局样式之前（`entry-graphs` 的次序判据）—— 画法只有主窗口要
 * （只有它订 `session-live-card`），所以样式跟着画法住主窗口独有的那一块。
 */
export interface LivePainter {
  /** 画几张活卡：主 tab 流尾巴那一块（只有主运行的）· 子运行时间线尾巴上那一截。 */
  cards(host: HTMLElement, cards: LiveCardState[]): void;
}

/**
 * 视图：手上每一处最新那一份（`route(origin, sid)` 由宿主给：这台上有这个 tab ⇒ 留，没有就没处画）；
 * 主运行那一处变了 ⇒ 重画它流尾巴上那一块（同一张卡沿用节点、正文只重建变了的尾巴）。没装画法（独立查看器那一形）⇒ 只记不画。
 */
export class LiveCards {
  private readonly held = new Map<string, Held>();
  /** 每个会话的运行表（后端 `session_runs` 给的成品）。 */
  readonly board = new RunBoard();
  private painter: LivePainter | null = null;

  constructor(
    private readonly route: (origin: string, sid: string) => boolean,
    private readonly trailerOf: TrailerOf,
  ) {}

  setPainter(p: LivePainter): void {
    this.painter = p;
  }

  /** 一格：那一处此刻整份的卡（空 ⇒ 那里没有卡了），或某张卡只换了尾巴。 */
  onCards(p: SessionLiveCardsPayload): void {
    const ch = p.live;
    const at = ch.t === "whole" ? this.onWhole(p.origin, ch.whole) : this.onTail(ch.tail);
    if (!at) return;
    if (at.run === null) this.paint(at.sid);
    else this.onRunLive?.(at.sid, at.run);
  }

  private onWhole(origin: string, w: LivePlace): At | null {
    const { sid, cards } = w;
    const run = w.run ?? null;
    const key = `${sid}\u0000${run ?? ""}`;
    if (cards.length === 0 || !this.route(origin, sid)) {
      if (!this.held.delete(key)) return null;
    } else {
      this.held.set(key, { origin, sid, run, cards });
    }
    return { sid, run };
  }

  /** 尾巴：那张卡前 `keep` 块原样留着，后面换成交来的。手上没有那一处 · 那张卡 ⇒ 不收（壳在丢过格之后先交整份）。 */
  private onTail(t: LiveTail): At | null {
    const run = t.run ?? null;
    const h = this.held.get(`${t.sid}\u0000${run ?? ""}`);
    const at = h ? h.cards.findIndex((c) => c.card === t.card) : -1;
    const was = h?.cards[at];
    if (!h || !was) return null;
    const next: LiveCardNow = { ...was, body: [...was.body.slice(0, t.keep), ...t.blocks] };
    if (t.last === undefined) delete next.last;
    else next.last = t.last;
    h.cards = h.cards.map((c, i) => (i === at ? next : c));
    return { sid: t.sid, run };
  }

  /** 一个 tab 里某个运行此刻的活卡（先来的在前；`run` 缺 ＝ 主运行）。 */
  cardsOf(sid: string, run: string | null = null): LiveCardState[] {
    const h = this.held.get(`${sid}\u0000${run ?? ""}`);
    return h ? h.cards.map((c) => stateOf(h.origin, c)) : [];
  }

  /** 某个子运行那一行的「最近：…」那一截：最新一张带着它的卡上核心写好的那一句；没有 ⇒ `null`。 */
  liveLastOf(sid: string, run: string): string | null {
    const cards = this.held.get(`${sid}\u0000${run}`)?.cards ?? [];
    return [...cards].reverse().find((c) => c.last !== undefined)?.last ?? null;
  }

  /** 一个会话的运行表到了。回：这一次收场的那几个（卡由后端撤）。 */
  onRuns(sid: string, table: RunTable): RunInfo[] {
    return this.board.set(sid, table);
  }

  /** 某个子运行的卡有动静（它的时间线开着的话要重画活卡那一截）。宿主装。 */
  onRunLive: ((sid: string, run: string) => void) | null = null;

  /** 这个 tab 没了 ⇒ 它的卡不留（没处画）。 */
  dropTab(sid: string): void {
    this.board.drop(sid);
    let hit = false;
    for (const [k, h] of this.held) if (h.sid === sid) hit = this.held.delete(k) || hit;
    if (hit) this.paint(sid);
  }

  /** 那台机器的活卡流看不见了 ⇒ 那台的卡全撤（不会再有下文）。 */
  dropOrigin(origin: string): void {
    const touched = new Set<string>();
    for (const [k, h] of this.held) {
      if (h.origin !== origin) continue;
      this.held.delete(k);
      touched.add(h.sid);
    }
    for (const sid of touched) this.paint(sid);
  }

  /** 此刻手上的卡数（判据读）。 */
  get size(): number {
    let n = 0;
    for (const h of this.held.values()) n += h.cards.length;
    return n;
  }

  private paint(sid: string): void {
    const painter = this.painter;
    if (!painter) return;
    const host = this.trailerOf(sid);
    if (host) painter.cards(host, this.cardsOf(sid));
  }
}
