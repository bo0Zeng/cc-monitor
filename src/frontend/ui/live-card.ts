/**
 * **活卡**：后端拼好的成品（会话流 `session-live-card`，帧 `live_card`）照画。对账 · 缺口 · 记录到了整轮覆盖 · 子运行归位 · 截头 ·
 * 挤谁，全在后端（`src/backend/stream/live.rs`）；这里只认几种 `op`：整卡换、加块、加字、改相、撤卡。
 *
 * 两层：
 * - {@link LiveBoard}：**纯状态**（无 DOM、无定时器）。吃一帧，回哪个 tab 的卡变了；唯一一条规矩：一张卡的 `n` 不接上一帧 ⇒ 先不画它，
 *   等后端下一张整卡（`open`）或撤卡（`close`）。
 * - {@link LiveCards}：把状态画到每个 tab 流尾巴上的那一块（`MessageStream.trailerElement`）。活卡**不进时间线**：
 *   不占 seq、不进去重集、不进大纲 / 查找 / 改动集 —— 全会话事实只读 json。主 tab 只画主运行的卡（`run` 缺）。
 */
import { copyText } from "./copy-table";
import type { LiveBlock as WireBlock } from "./generated/LiveBlock";
import type { LiveCard } from "./generated/LiveCard";
import type { SessionLiveCardPayload } from "./generated/SessionLiveCardPayload";
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

interface Card {
  origin: string;
  sid: string;
  run: string | null;
  /** 下一帧该是第几号。 */
  next: number;
  rid: string;
  phase: "streaming" | "awaiting";
  clipped: boolean;
  blocks: LiveBlock[];
  born: number;
}

const blockOf = (b: WireBlock): LiveBlock => ({ kind: b.kind, text: b.text ?? "", tool: b.tool });

/**
 * 纯状态：照后端的帧换卡、长卡、撤卡。`route(origin, sid)` 由宿主给：这台上有这个 tab ⇒ `true`（没有就不留，没处画）。
 */
export class LiveBoard {
  private readonly cards = new Map<string, Card>();
  private born = 0;

  constructor(private readonly route: (origin: string, sid: string) => boolean = () => true) {}

  /** 一帧。回：哪个 tab 的卡变了（没变 ⇒ `null`）。 */
  apply(origin: string, f: LiveCard): string | null {
    const key = `${origin}\u0000${f.card}`;
    if (f.op === "open") {
      if (!this.route(origin, f.sid)) return null;
      this.cards.set(key, {
        origin,
        sid: f.sid,
        run: f.run ?? null,
        next: f.n + 1,
        rid: f.rid,
        phase: f.phase,
        clipped: f.clipped,
        blocks: f.blocks.map(blockOf),
        born: this.cards.get(key)?.born ?? this.born++,
      });
      return f.sid;
    }
    const c = this.cards.get(key);
    if (!c) return null;
    if (f.n !== c.next || f.op === "close") {
      // 撤卡 · 接不上（壳那一跳没 credit 丢过）⇒ 这张先不画。
      this.cards.delete(key);
      return c.sid;
    }
    c.next += 1;
    switch (f.op) {
      case "block":
        while (c.blocks.length <= f.i) c.blocks.push({ kind: "other", text: "" });
        c.blocks[f.i] = { kind: f.block, text: "", tool: f.tool };
        break;
      case "text": {
        const b = c.blocks[f.i];
        if (b) b.text += f.s;
        break;
      }
      case "awaiting":
        c.phase = "awaiting";
        break;
    }
    return c.sid;
  }

  /** 一个 tab 里某个运行此刻的活卡（先来的在前；`run` 缺 ＝ 主运行）。 */
  cardsOf(sid: string, run: string | null = null): LiveCardState[] {
    return [...this.cards.entries()]
      .filter(([, c]) => c.sid === sid && c.run === run)
      .sort(([, a], [, b]) => a.born - b.born)
      .map(([key, c]) => ({
        key,
        messageId: c.rid,
        phase: c.phase,
        blocks: c.blocks.map((b) => ({ ...b })),
        clipped: c.clipped,
      }));
  }

  /** 某个子运行此刻在生成的那一块（给它那一行的「最近：…」）：最新那张正在说的卡的最后一块；没有 ⇒ `null`。 */
  liveBlockOf(sid: string, run: string): LiveBlock | null {
    const cs = [...this.cards.values()]
      .filter((c) => c.sid === sid && c.run === run && c.phase === "streaming")
      .sort((a, b) => b.born - a.born);
    const last = cs[0]?.blocks[cs[0].blocks.length - 1];
    return last ? { ...last } : null;
  }

  /** 这个 tab 没了 ⇒ 它的卡不留（没处画）。回：变没变。 */
  dropTab(sid: string): boolean {
    let hit = false;
    for (const [k, c] of this.cards) if (c.sid === sid) hit = this.cards.delete(k) || hit;
    return hit;
  }

  /** 那台机器的活卡流看不见了 ⇒ 那台的卡全撤（不会再有下文）。回：哪几个 tab 变了。 */
  dropOrigin(origin: string): Set<string> {
    const touched = new Set<string>();
    for (const [k, c] of this.cards) {
      if (c.origin !== origin) continue;
      this.cards.delete(k);
      touched.add(c.sid);
    }
    return touched;
  }

  /** 此刻在画的卡数（判据读）。 */
  get size(): number {
    return this.cards.size;
  }
}

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
 * 视图：每个被改过的 tab，把它流尾巴上那一块整块重画成此刻的主运行活卡。没装画法（独立查看器那一形）⇒ 只记账不画。
 */
export class LiveCards {
  readonly cards: LiveBoard;
  /** 每个会话的运行表（后端 `session_runs` 给的成品）。 */
  readonly board = new RunBoard();
  private painter: LivePainter | null = null;

  constructor(
    route: (origin: string, sid: string) => boolean,
    private readonly trailerOf: TrailerOf,
  ) {
    this.cards = new LiveBoard(route);
  }

  setPainter(p: LivePainter): void {
    this.painter = p;
  }

  onCard(p: SessionLiveCardPayload): void {
    const sid = this.cards.apply(p.origin, p.card);
    if (sid === null) return;
    if (p.card.run === undefined) this.paint(sid);
    else this.onRunLive?.(sid, p.card.run);
  }

  /** 一个会话的运行表到了。回：这一次收场的那几个（卡由后端撤）。 */
  onRuns(sid: string, runs: RunInfo[]): RunInfo[] {
    return this.board.set(sid, runs);
  }

  /** 某个子运行的卡有动静（它的时间线开着的话要重画活卡那一截）。宿主装。 */
  onRunLive: ((sid: string, run: string) => void) | null = null;

  dropTab(sid: string): void {
    this.board.drop(sid);
    if (this.cards.dropTab(sid)) this.paint(sid);
  }

  dropOrigin(origin: string): void {
    for (const sid of this.cards.dropOrigin(origin)) this.paint(sid);
  }

  private paint(sid: string): void {
    const painter = this.painter;
    if (!painter) return;
    const host = this.trailerOf(sid);
    if (host) painter.cards(host, this.cards.cardsOf(sid));
  }
}
