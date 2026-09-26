/**
 * 〔TAP · V124〕**活卡**：中转抄出来的 SSE 事件（`session-tap`）先上屏，jsonl 那一轮到了就整轮覆盖。
 *
 * 要求出处：`设计/20 §8`（V24「SSE 确保快，落盘确保对」—— SSE 只保快、jsonl 到了整轮覆盖、对账键 `message.id`、
 * 不做记录级合并；按 sid 对账，对不上的只能当匿名流）· `设计/10 §2.3`（「活卡 → 定稿」）· `设计/05 §3.3.4`（缺口原位、纯算术）。
 * 设计与上界表住仓外 `调研/第四波记录/TAP.md §1.3 · §2 · §4 · §5`。
 *
 * 两层：
 * - {@link LiveCore}：**纯状态机**（无 DOM、无定时器）。吃 tap 事件与「一条 jsonl 记录过了去重」，产出每个 tab 此刻该显示的活卡。
 * - {@link LiveCards}：把状态画到每个 tab 流尾巴上的那一块（`MessageStream.trailerElement`）。活卡**不进时间线**：
 *   不占 seq、不进去重集、不进大纲 / 查找 / 改动集 —— 全会话事实只读 json（`设计/10 §2.2`）。
 */
import { copyText } from "./copy-table";
import s from "./live-card.module.css";

/** `session-tap` 的载荷（与 `src/generated/SessionTapPayload.ts` 同形；这里只取要用的几格）。 */
export interface TapPayload {
  origin: string;
  stream: string;
  resp: number;
  n: number;
  data?: string;
  end?: string;
}

/** 每个 tab 最多几张活卡（主线程那一个 ＋ 并发的一个）。 */
export const LIVE_PER_TAB = 2;
/** 单张活卡正文最多留几个字符（只留尾巴）。 */
export const LIVE_TEXT_KEEP = 32_768;
/** 全部 tab 合起来，同时在攒的响应最多几个。 */
export const LIVE_STREAMS_KEEP = 16;
/** 每个 tab 记住几个已定稿的 `message.id`（先进先出）。 */
export const TOMB_KEEP = 64;
/** 记住几个「不再收」的响应（断了 / 匿名 / 已覆盖），先进先出。 */
const DEAD_KEEP = 256;

type BlockKind = "text" | "thinking" | "tool_use" | "other";

/** 活卡里的一块（按 SSE 的 `index`）。 */
export interface LiveBlock {
  kind: BlockKind;
  /** `text` 块的正文；其余块不留正文（思考不展开、工具入参不显示）。 */
  text: string;
  /** `tool_use` 块的工具名。 */
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
  sid: string;
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
      // 头一件就不是 0 号 ⇒ 开头丢了（message_start 在里面），这个响应认不全 ⇒ 不收。
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
        sid,
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
    if (p.data !== undefined) this.apply(r, p.data, touched);
    return touched;
  }

  /**
   * 一条 jsonl 记录**过了双重去重**（`TabManager.onLine` 那一处）。`message` 是那条记录本身。
   * 同 `message.id` 的活卡 ⇒ 整张撤（定稿，该 id 进墓碑）；已收尾而 id 不同的活卡 ⇒ 撤（它的 id 不会再落进这份 jsonl 了）。
   */
  record(sid: string, message: unknown): Touched {
    const touched: Touched = new Set();
    const id = messageIdOf(message);
    if (id !== null) this.tombOf(sid).add(id);
    for (const r of [...this.resps.values()]) {
      if (r.sid !== sid) continue;
      if (id !== null && r.messageId === id) {
        this.kill(r, touched);
      } else if (r.stopped) {
        this.kill(r, touched);
      }
    }
    return touched;
  }

  /** 这个 tab 结束 / 转灰 / 关了 ⇒ 它的活卡全撤。 */
  dropTab(sid: string): Touched {
    const touched: Touched = new Set();
    for (const r of [...this.resps.values()]) if (r.sid === sid) this.kill(r, touched);
    this.tombs.delete(sid);
    return touched;
  }

  /** 一个 tab 此刻的活卡（先来的在前）。 */
  cardsOf(sid: string): LiveCardState[] {
    return [...this.resps.values()]
      .filter((r) => r.sid === sid && r.messageId !== null)
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
    // 同 tab 满了 ⇒ 先挤最老的已收尾那个，没有就挤最老的；全局满了 ⇒ 挤全局最老的。
    const mine = [...this.resps.values()].filter((x) => x.sid === r.sid).sort((a, b) => a.born - b.born);
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

  /** 认几种 Anthropic SSE 事件；其余（`ping` · `message_delta` · 不认识的新事件 · 读不懂的原文）不理。 */
  private apply(r: Resp, data: string, touched: Touched): void {
    let ev: unknown;
    try {
      ev = JSON.parse(data);
    } catch {
      return;
    }
    if (typeof ev !== "object" || ev === null) return;
    const e = ev as Record<string, unknown>;
    switch (e.type) {
      case "message_start": {
        const m = e.message as Record<string, unknown> | undefined;
        const id = typeof m?.id === "string" ? m.id : null;
        if (id === null) return;
        // 同 id 已经定稿过（jsonl 先到了）⇒ 这张卡不该出现。
        if (this.tombOf(r.sid).has(id)) {
          this.kill(r, touched);
          return;
        }
        r.messageId = id;
        touched.add(r.sid);
        return;
      }
      case "content_block_start": {
        const i = e.index;
        const b = e.content_block as Record<string, unknown> | undefined;
        if (typeof i !== "number" || !b) return;
        const kind: BlockKind =
          b.type === "text" || b.type === "thinking" || b.type === "tool_use" ? b.type : "other";
        r.blocks[i] = { kind, text: "", tool: typeof b.name === "string" ? b.name : undefined };
        touched.add(r.sid);
        return;
      }
      case "content_block_delta": {
        const i = e.index;
        const d = e.delta as Record<string, unknown> | undefined;
        if (typeof i !== "number" || !d) return;
        const b = r.blocks[i];
        if (!b || b.kind !== "text" || d.type !== "text_delta" || typeof d.text !== "string") return;
        b.text += d.text;
        this.clip(r);
        touched.add(r.sid);
        return;
      }
      case "message_stop":
        r.stopped = true;
        touched.add(r.sid);
        return;
      case "error":
        this.kill(r, touched);
        return;
      default:
        return;
    }
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

/** 一条 jsonl 记录的 `message.id`（assistant 记录才有；没有 ⇒ `null`）。 */
export function messageIdOf(record: unknown): string | null {
  if (typeof record !== "object" || record === null) return null;
  const m = (record as { message?: unknown }).message;
  if (typeof m !== "object" || m === null) return null;
  const id = (m as { id?: unknown }).id;
  return typeof id === "string" && id.length > 0 ? id : null;
}

/** 把一张活卡画成纯文本行（视图与判据共用；不走 Markdown / 高亮 —— 定稿那一张卡才走渲染管线）。 */
export function renderCardText(card: LiveCardState): { head: string; body: string } {
  const head =
    card.phase === "streaming"
      ? copyText("liveCard.state.streaming")
      : copyText("liveCard.state.awaitingRecord");
  const parts: string[] = [];
  if (card.clipped) parts.push("…");
  for (const b of card.blocks) {
    if (!b) continue;
    if (b.kind === "text") parts.push(b.text);
    else if (b.kind === "thinking") parts.push(copyText("liveCard.block.thinking"));
    else if (b.kind === "tool_use") parts.push(copyText("liveCard.block.toolUse", { tool: b.tool ?? "?" }));
  }
  return { head, body: parts.filter((x) => x.length > 0).join("\n") };
}

/** 宿主给视图的口：某个 tab 流尾巴上的那一块（没有这个 tab ⇒ `null`）。 */
export type TrailerOf = (sid: string) => HTMLElement | null;

/**
 * 视图：每个被改过的 tab，把它流尾巴上那一块整块重画成此刻的活卡（张数 ≤ `LIVE_PER_TAB`，正文 ≤ `LIVE_TEXT_KEEP`）。
 */
export class LiveCards {
  readonly core: LiveCore;

  constructor(
    route: (origin: string, stream: string) => string | null,
    private readonly trailerOf: TrailerOf,
  ) {
    this.core = new LiveCore(route);
  }

  onTap(p: TapPayload): void {
    this.paint(this.core.tap(p));
  }

  onRecord(sid: string, message: unknown): void {
    this.paint(this.core.record(sid, message));
  }

  dropTab(sid: string): void {
    this.paint(this.core.dropTab(sid));
  }

  private paint(touched: Touched): void {
    for (const sid of touched) {
      const host = this.trailerOf(sid);
      if (!host) continue;
      const cards = this.core.cardsOf(sid);
      host.replaceChildren(
        ...cards.map((c) => {
          const { head, body } = renderCardText(c);
          const el = document.createElement("div");
          el.className = c.phase === "streaming" ? s.card : `${s.card} ${s.awaiting}`;
          el.dataset.liveKey = c.key;
          const h = document.createElement("div");
          h.className = s.head;
          h.textContent = head;
          const b = document.createElement("div");
          b.className = s.body;
          b.textContent = body;
          el.append(h, b);
          return el;
        }),
      );
    }
  }
}
