/**
 * **活卡**：壳交来的成品照画（会话流 `session-live-card`，一格 = 那台某会话某运行此刻整份的卡）。对账 · 缺口 · 记录到了整轮覆盖 ·
 * 子运行归位 · 截头 · 挤谁在后端（`src/backend/stream/live.rs`）；一帧一帧折成整份在端核心（`src/comms/resident-client/live.rs`，
 * 壳与手机核心同一份）。这里不认 `op`、不对号：每一格换掉那一处手上那一份。
 *
 * {@link LiveCards} 把每个 tab 此刻的卡画到它流尾巴上的那一块（`MessageStream.trailerElement`）。活卡**不进时间线**：
 * 不占 seq、不进去重集、不进大纲 / 查找 / 改动集 —— 全会话事实只读 json。主 tab 只画主运行的卡（`run` 缺）。
 */
import { copyText } from "./copy-table";
import type { LiveCardNow } from "./generated/LiveCardNow";
import type { SessionLiveCardsPayload } from "./generated/SessionLiveCardsPayload";
import type { RunInfo } from "./generated/RunInfo";
import { RunBoard } from "./runs";

type BlockKind = import("./generated/BlockKind").BlockKind;

/** 活卡里的一块。 */
export interface LiveBlock {
  kind: BlockKind;
  /** `text` 块已到的正文；其余块没有正文（思考不展开、工具入参不显示）。 */
  text: string;
  /** 工具块的工具名。 */
  tool?: string;
}

/** 一张活卡此刻的样子（给视图 / 判据读）。 */
export interface LiveCardState {
  key: string;
  messageId: string;
  /** `streaming` = 上游还在说；`awaiting` = 上游说完了、那一轮的记录还没到。 */
  phase: "streaming" | "awaiting";
  blocks: LiveBlock[];
  /** 正文被截过头（后端只留了尾巴）。 */
  clipped: boolean;
}

/** 手上那一处（某台某会话某运行）最新那一份。 */
interface Held {
  origin: string;
  sid: string;
  run: string | null;
  cards: LiveCardNow[];
}

const stateOf = (origin: string, c: LiveCardNow): LiveCardState => ({
  key: `${origin}\u0000${c.card}`,
  messageId: c.rid,
  phase: c.phase,
  blocks: c.blocks.map((b) => ({ kind: b.kind, text: b.text ?? "", tool: b.tool })),
  clipped: c.clipped,
});

/** 把一张活卡画成纯文本行（视图与判据共用；不走 Markdown / 高亮 —— 定稿那一张卡才走渲染管线）。 */
export function renderCardText(card: LiveCardState): { head: string; body: string } {
  const head =
    card.phase === "streaming"
      ? copyText("liveCard.state.streaming")
      : copyText("liveCard.state.awaitingRecord");
  const parts: string[] = [];
  if (card.clipped) parts.push(copyText("liveCard.body.clipped"));
  for (const b of card.blocks) {
    if (b.kind === "text") parts.push(b.text);
    else if (b.kind === "thinking") parts.push(copyText("liveCard.block.thinking"));
    else if (b.kind === "tool") parts.push(copyText("liveCard.block.toolUse", { tool: b.tool ?? "?" }));
  }
  return { head, body: parts.filter((x) => x.length > 0).join("\n") };
}

/** 宿主给视图的口：某个 tab 流尾巴上的那一块（没有这个 tab ⇒ `null`）。 */
export type TrailerOf = (sid: string) => HTMLElement | null;

/**
 * 把一个 tab 此刻的活卡画进它流尾巴上那一块（整块重画）。实现住 `live-card-view.ts`（带 `.module.css`），
 * **由主窗口的入口装进来**（`TabManager.setLivePainter`）：本文件被主窗口与独立查看器两个入口共用（进共享块），
 * 而 `.module.css` 进共享块会让它的规则排在主窗口全局样式之前（`entry-graphs` 的次序判据）—— 画法只有主窗口要
 * （只有它订 `session-live-card`），所以样式跟着画法住主窗口独有的那一块。
 */
export interface LivePainter {
  /** 画几张活卡：主 tab 流尾巴那一块（只有主运行的）· 子运行时间线尾巴上那一截。 */
  cards(host: HTMLElement, cards: LiveCardState[]): void;
}

/**
 * 视图：手上每一处最新那一份整份（`route(origin, sid)` 由宿主给：这台上有这个 tab ⇒ 留，没有就没处画）；
 * 主运行那一处变了 ⇒ 它流尾巴上那一块整块重画。没装画法（独立查看器那一形）⇒ 只记不画。
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

  /** 一格：那一处此刻整份的卡（空 ⇒ 那里没有卡了）。 */
  onCards(p: SessionLiveCardsPayload): void {
    const { sid, cards } = p.live;
    const run = p.live.run ?? null;
    const key = `${sid}\u0000${run ?? ""}`;
    if (cards.length === 0 || !this.route(p.origin, sid)) {
      if (!this.held.delete(key)) return;
    } else {
      this.held.set(key, { origin: p.origin, sid, run, cards });
    }
    if (run === null) this.paint(sid);
    else this.onRunLive?.(sid, run);
  }

  /** 一个 tab 里某个运行此刻的活卡（先来的在前；`run` 缺 ＝ 主运行）。 */
  cardsOf(sid: string, run: string | null = null): LiveCardState[] {
    const h = this.held.get(`${sid}\u0000${run ?? ""}`);
    return h ? h.cards.map((c) => stateOf(h.origin, c)) : [];
  }

  /** 某个子运行此刻在生成的那一块（给它那一行的「最近：…」）：最新那张正在说的卡的最后一块；没有 ⇒ `null`。 */
  liveBlockOf(sid: string, run: string): LiveBlock | null {
    const speaking = this.cardsOf(sid, run).filter((c) => c.phase === "streaming");
    const last = speaking[speaking.length - 1];
    return last?.blocks[last.blocks.length - 1] ?? null;
  }

  /** 一个会话的运行表到了。回：这一次收场的那几个（卡由后端撤）。 */
  onRuns(sid: string, runs: RunInfo[]): RunInfo[] {
    return this.board.set(sid, runs);
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
