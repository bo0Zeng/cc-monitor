/**
 * 〔TAP · V124〕**活卡**：中转抄出来的流（`session-tap`）先上屏，记录那一轮到了就整轮覆盖。
 *
 * SSE 只保快、记录到了整轮覆盖、对账键是后端给的 `rid`（记录那一侧是 `line.rid`，流那一侧是归一事件 `start` 带的），不做记录级合并；
 * 按 sid 对账，对不上的只能当匿名流；缺口原位、纯算术。界面只收**归一事件**（后端按上游协议折好、归好位），不认任何一家的事件名。
 *
 * 两层：
 * - {@link LiveCore}：**纯状态机**（无 DOM、无定时器）。吃 tap 事件与「一条记录过了去重」，产出每个 tab 此刻该显示的活卡；
 *   归到子运行的那几段（`run` 有值）不进主 tab 的活卡，只给那个子运行的行与它的时间线。
 * - {@link LiveCards}：把状态画到每个 tab 流尾巴上的那一块（`MessageStream.trailerElement`）。活卡**不进时间线**：
 *   不占 seq、不进去重集、不进大纲 / 查找 / 改动集 —— 全会话事实只读 json（`设计/10 §2.2`）。
 */
import { copyText } from "./copy-table";
import type { StreamEv } from "./generated/StreamEv";
import type { RunInfo } from "./generated/RunInfo";
import { RunBoard, runRowText, type RunRow } from "./runs";

/** `session-tap` 的载荷（与 `src/frontend/ui/generated/SessionTapPayload.ts` 同形；这里只取要用的几格）。 */
export interface TapPayload {
  origin: string;
  stream: string;
  /** 归哪个子运行（缺 ＝ 主运行）。 */
  run?: string;
  resp: number;
  n: number;
  ev?: StreamEv;
  end?: string;
}

/** 每个运行（主运行 ＋ 每个子运行各算一个）最多几张活卡（主线程那一个 ＋ 并发的一个）。 */
export const LIVE_PER_TAB = 2;
/** 单张活卡正文最多留几个字符（只留尾巴）。 */
export const LIVE_TEXT_KEEP = 32_768;
/** 全部 tab 合起来，同时在攒的响应最多几个。 */
export const LIVE_STREAMS_KEEP = 16;
/** 每个 tab 记住几个已定稿的 `message.id`（先进先出）。 */
export const TOMB_KEEP = 64;
/** 记住几个「不再收」的响应（断了 / 匿名 / 已覆盖），先进先出。 */
const DEAD_KEEP = 256;

type BlockKind = import("./generated/BlockKind").BlockKind;

/** 活卡里的一块（按 SSE 的 `index`）。 */
export interface LiveBlock {
  kind: BlockKind;
  /** `text` 块的正文；其余块不留正文（思考不展开、工具入参不显示）。 */
  text: string;
  /** 工具块的工具名。 */
  tool?: string;
}

/** 一张活卡此刻的样子（给视图 / 判据读）。 */
export interface LiveCardState {
  key: string;
  messageId: string | null;
  /** `streaming` = 上游还在说；`awaiting` = 上游说完了、同 id 的 jsonl 还没到。 */
  phase: "streaming" | "awaiting";
  blocks: LiveBlock[];
  /** 正文被截过头（超 `LIVE_TEXT_KEEP`）。 */
  clipped: boolean;
}

interface Resp {
  key: string;
  origin: string;
  sid: string;
  /** 归哪个子运行；`null` ＝ 主运行。 */
  run: string | null;
  next: number;
  messageId: string | null;
  blocks: LiveBlock[];
  stopped: boolean;
  clipped: boolean;
  /** 进来的先后（挤出时用）。 */
  born: number;
}

/** 一件事改了哪几个 tab 的活卡（视图只重画这几个）。 */
export type Touched = Set<string>;

const keyOf = (p: { origin: string; stream: string; resp: number }): string =>
  `${p.origin}\u0000${p.stream}\u0000${p.resp}`;

/** 先进先出、有上界的集合。 */
class BoundedSet {
  private readonly order: string[] = [];
  private readonly set = new Set<string>();
  constructor(private readonly keep: number) {}
  has(k: string): boolean {
    return this.set.has(k);
  }
  add(k: string): void {
    if (this.set.has(k)) return;
    this.set.add(k);
    this.order.push(k);
    while (this.order.length > this.keep) {
      const old = this.order.shift();
      if (old !== undefined) this.set.delete(old);
    }
  }
  get size(): number {
    return this.set.size;
  }
}

/**
 * 纯状态机。`route(origin, stream)` 由宿主给：`stream` 就是某个 tab 的 sid（且机器对得上）⇒ 回那个 sid；否则 `null`（匿名流）。
 */
export class LiveCore {
  private readonly resps = new Map<string, Resp>();
  private readonly dead = new BoundedSet(DEAD_KEEP);
  private readonly tombs = new Map<string, BoundedSet>();
  private born = 0;

  constructor(private readonly route: (origin: string, stream: string) => string | null) {}

  /** 一个 tap 事件。回：哪几个 tab 的活卡变了。 */
  tap(p: TapPayload): Touched {
    const touched: Touched = new Set();
    const key = keyOf(p);
    if (this.dead.has(key)) return touched;
    let r = this.resps.get(key);
    if (!r) {
      // 头一件就不是 0 号 ⇒ 开头丢了（对账键在里面），这个响应认不全 ⇒ 不收。
      if (p.n !== 0) {
        this.dead.add(key);
        return touched;
      }
      const sid = this.route(p.origin, p.stream);
      if (sid === null) {
        // 对不上 tab ⇒ 匿名流：不显示、不留（`20 §8`）。
        this.dead.add(key);
        return touched;
      }
      r = {
        key,
        origin: p.origin,
        sid,
        run: p.run ?? null,
        next: 0,
        messageId: null,
        blocks: [],
        stopped: false,
        clipped: false,
        born: this.born++,
      };
      this.admit(r, touched);
    }
    // 缺口：号连不上 ⇒ 这个响应断了。不补（补是 jsonl 的事，V24），撤卡。
    if (p.n !== r.next) {
      this.kill(r, touched);
      return touched;
    }
    r.next += 1;
    if (p.end !== undefined) {
      // 上游说完了（或转发断了但 message_stop 已经见过 —— 内容是全的）⇒ 等落盘；
      // 转发断在半路（claude 被 Esc 打断 / 上游断了）⇒ 撤（jsonl 到了自然补上）。
      if (p.end === "done" || r.stopped) {
        r.stopped = true;
        touched.add(r.sid);
      } else {
        this.kill(r, touched);
      }
      return touched;
    }
    if (p.ev !== undefined) this.apply(r, p.ev, touched);
    return touched;
  }

  /**
   * 一条记录**过了双重去重**（`TabManager.onLine` 那一处；子运行的时间线读到的记录也走这里，`run` 说是哪个）。`rid` 是它的对账键。
   * 同 `rid` 的活卡 ⇒ 整张撤（定稿，该 id 进墓碑）；同一个运行里已收尾而 id 不同的活卡 ⇒ 撤（它的 id 不会再落进这份记录了）。
   */
  record(sid: string, rid: string | null | undefined, run: string | null = null): Touched {
    const touched: Touched = new Set();
    const id = rid ?? null;
    if (id !== null) this.tombOf(sid).add(id);
    for (const r of [...this.resps.values()]) {
      if (r.sid !== sid) continue;
      if (id !== null && r.messageId === id) {
        this.kill(r, touched);
      } else if (r.stopped && r.run === run) {
        this.kill(r, touched);
      }
    }
    return touched;
  }

  /** 这个子运行收场了 ⇒ 它在攒的几段全撤。 */
  dropRun(sid: string, run: string): Touched {
    const touched: Touched = new Set();
    for (const r of [...this.resps.values()]) if (r.sid === sid && r.run === run) this.kill(r, touched);
    return touched;
  }

  /** 这个 tab 结束 / 转灰 / 关了 ⇒ 它的活卡全撤。 */
  dropTab(sid: string): Touched {
    const touched: Touched = new Set();
    for (const r of [...this.resps.values()]) if (r.sid === sid) this.kill(r, touched);
    this.tombs.delete(sid);
    return touched;
  }

  /** 那台机器的 tap 流看不见了 ⇒ 那台的全部响应撤掉（开着的不会再有下文）。 */
  dropOrigin(origin: string): Touched {
    const touched: Touched = new Set();
    for (const r of [...this.resps.values()]) if (r.origin === origin) this.kill(r, touched);
    return touched;
  }

  /** 一个 tab 里某个运行此刻的活卡（先来的在前；`run` 缺 ＝ 主运行）。 */
  cardsOf(sid: string, run: string | null = null): LiveCardState[] {
    return [...this.resps.values()]
      .filter((r) => r.sid === sid && r.run === run && r.messageId !== null)
      .sort((a, b) => a.born - b.born)
      .map((r) => ({
        key: r.key,
        messageId: r.messageId,
        phase: r.stopped ? "awaiting" : "streaming",
        blocks: r.blocks.map((b) => ({ ...b })),
        clipped: r.clipped,
      }));
  }

  /** 此刻在攒的响应数（判据读上界用）。 */
  get size(): number {
    return this.resps.size;
  }

  private admit(r: Resp, touched: Touched): void {
    // 同一个运行满了 ⇒ 先挤最老的已收尾那个，没有就挤最老的；全局满了 ⇒ 挤全局最老的。
    const mine = [...this.resps.values()]
      .filter((x) => x.sid === r.sid && x.run === r.run)
      .sort((a, b) => a.born - b.born);
    if (mine.length >= LIVE_PER_TAB) {
      const victim = mine.find((x) => x.stopped) ?? mine[0];
      if (victim) this.kill(victim, touched);
    }
    if (this.resps.size >= LIVE_STREAMS_KEEP) {
      const oldest = [...this.resps.values()].sort((a, b) => a.born - b.born)[0];
      if (oldest) this.kill(oldest, touched);
    }
    this.resps.set(r.key, r);
  }

  private kill(r: Resp, touched: Touched): void {
    this.resps.delete(r.key);
    this.dead.add(r.key);
    touched.add(r.sid);
  }

  private tombOf(sid: string): BoundedSet {
    let t = this.tombs.get(sid);
    if (!t) {
      t = new BoundedSet(TOMB_KEEP);
      this.tombs.set(sid, t);
    }
    return t;
  }

  /** 一件归一事件：开始（对账键）· 一块开始 · 文字增量 · 收尾（报错收尾 ⇒ 撤）。 */
  private apply(r: Resp, ev: StreamEv, touched: Touched): void {
    switch (ev.t) {
      case "start": {
        // 同 id 已经定稿过（记录先到了）⇒ 这张卡不该出现。
        if (this.tombOf(r.sid).has(ev.rid)) {
          this.kill(r, touched);
          return;
        }
        r.messageId = ev.rid;
        touched.add(r.sid);
        return;
      }
      case "block": {
        r.blocks[ev.i] = { kind: ev.kind, text: "", tool: ev.tool };
        touched.add(r.sid);
        return;
      }
      case "text": {
        const b = r.blocks[ev.i];
        if (!b || b.kind !== "text") return;
        b.text += ev.s;
        this.clip(r);
        touched.add(r.sid);
        return;
      }
      case "stop":
        if (!ev.ok) {
          this.kill(r, touched);
          return;
        }
        r.stopped = true;
        touched.add(r.sid);
        return;
      default:
        return;
    }
  }

  /** 某个子运行此刻在生成的那一块（给它那一行的「最近：…」）：最新那一段的最后一块；没有在生成的 ⇒ `null`。 */
  liveBlockOf(sid: string, run: string): LiveBlock | null {
    const rs = [...this.resps.values()]
      .filter((r) => r.sid === sid && r.run === run && !r.stopped && r.messageId !== null)
      .sort((a, b) => b.born - a.born);
    const blocks = rs[0]?.blocks.filter((b) => b !== undefined) ?? [];
    const last = blocks[blocks.length - 1];
    return last ? { ...last } : null;
  }

  /** 正文总长超 `LIVE_TEXT_KEEP` ⇒ 从最前面的块开始截头。 */
  private clip(r: Resp): void {
    let total = r.blocks.reduce((n, b) => n + (b ? b.text.length : 0), 0);
    for (const b of r.blocks) {
      if (total <= LIVE_TEXT_KEEP) return;
      if (!b || b.text.length === 0) continue;
      const cut = Math.min(b.text.length, total - LIVE_TEXT_KEEP);
      b.text = b.text.slice(cut);
      total -= cut;
      r.clipped = true;
    }
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
    if (!b) continue;
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
 * （只有它订 `session-tap`），所以样式跟着画法住主窗口独有的那一块。
 */
export interface LivePainter {
  /** 流尾巴那一整块：子运行的行 ＋ 主运行的活卡。 */
  trailer(host: HTMLElement, sid: string, cards: LiveCardState[], rows: RunRow[]): void;
  /** 只画几张活卡（子运行时间线尾巴上那一截）。 */
  cards(host: HTMLElement, cards: LiveCardState[]): void;
}

/**
 * 视图：每个被改过的 tab，把它流尾巴上那一块整块重画成此刻的活卡（张数 ≤ `LIVE_PER_TAB`，正文 ≤ `LIVE_TEXT_KEEP`）。
 * 没装画法（独立查看器那一形）⇒ 只记账不画。
 */
export class LiveCards {
  readonly core: LiveCore;
  /** 每个会话的运行表（后端 `session_runs` 给的成品）。 */
  readonly board = new RunBoard();
  private painter: LivePainter | null = null;

  constructor(
    route: (origin: string, stream: string) => string | null,
    private readonly trailerOf: TrailerOf,
  ) {
    this.core = new LiveCore(route);
  }

  setPainter(p: LivePainter): void {
    this.painter = p;
  }

  onTap(p: TapPayload): void {
    const touched = this.core.tap(p);
    this.paint(touched);
    if (p.run !== undefined) for (const sid of touched) this.onRunLive?.(sid, p.run);
  }

  /** 一个会话的运行表到了：主 tab 上的行跟着变；收场的子运行撤掉它在攒的那几段。回：这一次收场的那几个。 */
  onRuns(sid: string, runs: RunInfo[]): RunInfo[] {
    const finished = this.board.set(sid, runs);
    for (const r of finished) this.core.dropRun(sid, r.run);
    this.paint(new Set([sid]));
    return finished;
  }

  /** 把某个子运行此刻的活卡画进 `host`（没装画法 ⇒ 不画）。 */
  paintCards(host: HTMLElement, sid: string, run: string): void {
    this.painter?.cards(host, this.core.cardsOf(sid, run));
  }

  /** 主 tab 上此刻每个在跑的子运行那一行。 */
  rowsOf(sid: string): RunRow[] {
    return this.board.running(sid).map((r) => ({ run: r.run, text: runRowText(r, this.core.liveBlockOf(sid, r.run)) }));
  }

  /** 某个子运行的流有动静（它的时间线开着的话要重画活卡那一截）。宿主装。 */
  onRunLive: ((sid: string, run: string) => void) | null = null;

  onRecord(sid: string, rid: string | null | undefined, run: string | null = null): void {
    this.paint(this.core.record(sid, rid, run));
  }

  dropRun(sid: string, run: string): void {
    this.paint(this.core.dropRun(sid, run));
  }

  dropTab(sid: string): void {
    this.board.drop(sid);
    this.paint(this.core.dropTab(sid));
  }

  dropOrigin(origin: string): void {
    this.paint(this.core.dropOrigin(origin));
  }

  private paint(touched: Touched): void {
    const painter = this.painter;
    if (!painter) return;
    for (const sid of touched) {
      const host = this.trailerOf(sid);
      if (host) painter.trailer(host, sid, this.core.cardsOf(sid), this.rowsOf(sid));
    }
  }
}
