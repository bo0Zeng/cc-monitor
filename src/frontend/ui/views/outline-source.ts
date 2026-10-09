/**
 * 大纲的数据源：问后端要，不在前端攒。「什么算一条用户输入」只住后端（`observe/user_inputs.rs`），
 * 实时 tab 与历史查看器都经同一处要（`session-reads.ts::listUserInputs`，帧命令 `history-user-inputs`，`IPC-PROTOCOL.md §10.4`）。
 *
 * # 形状
 *
 * 一个宿主（一个 tab / 一个查看器）一份。它只记三样：上次要到哪个字节（`end`，下一次增量从这里接）·
 * 已经列出来的 uuid（认「文件被重写」用）· 在途与否。**不存正文，不存摘要** —— 摘要在面板的行上。
 *
 * - `refresh()`：从 `end` 接着要一截，追加到面板上。在途时再叫 ⇒ 这一趟回来后**只补一趟**（合并）。
 * - 增量要不到（后端报「越过 EOF」= 文件被截断/重写）或增量里冒出**已有的** uuid（被重写但更长）
 *   ⇒ 从 0 重要一份、整表换掉。
 * - 要不到清单：**只看回包里的种类**（`OutlineFailure`，分档只住 `session-reads.ts::failureOf`），不解析原因文字 ——
 *   · **结构性**（`oldBackend`：对面不认这条命令 / 回的形状不对）⇒ 再要一定还是这样 ⇒ 灰掉说原因、此后不再要；
 *   · **瞬时**（`transport` / `truncated`：进程或连接出错、超时、输出断在半路）⇒ **下一次触发时再要**
 *     （沿用现有触发点，不起定时器）；**连续** [`MAX_TRANSIENT_FAILURES`] 次都是瞬时 ⇒ 按结构性处理。
 *     手上已经有清单时，瞬时失败**不动它**（续点不动、行不动）；一条都还没有时灰掉并挂原因。
 *   `reset`（关 tab / 换会话）清掉这两样状态。
 *
 * # 什么时候要（由宿主决定，本类不自己起定时器）
 *
 * 实时 tab：批结束时的 active tab · 切进来的 tab（有新行才要）· 真用户输入上屏那一刻
 * （渲染那边已有的 `onRealUserInput` 事件 —— 「刚刚发生了什么」留在流上）。
 * 查看器：加载完要一次。
 */
import type { Origin } from "../generated/Origin";
import {
  listUserInputs,
  type OutlineFailure,
  type UserInputEntry,
  type UserInputsResult,
} from "../session-reads";
import type { UserInputPanel } from "./user-input-panel";
import { copyText } from "../copy-table";

/** 这份清单问的是哪台机器上的哪份会话。拿不到（tab 还没收到路径）⇒ `null`，这一趟不要。 */
export type OutlineWhere = () => { origin: Origin; jsonlPath: string } | null;

/** 骨架索引一行里大纲要的那几个键（后端 `IndexRow` 的 `u` / `x` / `ts`，`IPC-PROTOCOL.md §10.3`）。 */
export interface OutlineIndexRow {
  u?: string;
  /** 这一行是一条用户输入 ⇒ 摘要；不是 ⇒ 缺。 */
  x?: string;
  ts?: string;
}

/** 从骨架索引里带回来的一份清单（`[0, end)` 的全量）。 */
export interface OutlineSeed {
  entries: UserInputEntry[];
  end: number;
}

/**
 * 首屏的「索引」与「大纲清单」合成一趟读：从一次从 0 起的骨架索引里把大纲搬出来。
 *
 * 推断是**单向可靠**的（理由逐字在后端 `IndexRow::x` 的头注）：
 * - 有**至少一个** `x` ⇒ 对面是会出它的后端 ⇒ 每一条用户输入都带着（同一个判定逐行跑）⇒ 这就是全量清单；
 * - 一个都没有 ⇒ **分不清**「老后端」还是「真的零条」⇒ `null`，调用方照旧从 0 要一份清单。
 * 索引要不到（`available: false`）同样 `null`。
 */
export function outlineSeedFromIndex(res: {
  available: boolean;
  end: number;
  rows: ReadonlyArray<OutlineIndexRow>;
} | null | undefined): OutlineSeed | null {
  if (!res?.available || !Array.isArray(res.rows)) return null;
  const entries: UserInputEntry[] = [];
  for (const r of res.rows) {
    if (r.x === undefined || !r.u) continue;
    entries.push({ uuid: r.u, excerpt: r.x, timestamp: r.ts ?? "" });
  }
  return entries.length > 0 ? { entries, end: res.end } : null;
}

/**
 * 连续几次**瞬时**失败之后按结构性处理（不再要）。
 *
 * 取 **3**，理由：
 * - 触发点全是**事件**（批结束 · 切进来且有新行 · 真用户输入上屏），不是定时器 ⇒ 3 次失败摊在
 *   3 个用户看得见的时刻上，不是一秒内连打三下 —— ssh 抖一下、本机后端刚被换掉，下一次多半就好了；
 * - 能扛住**连着两次**抖动（一次重连期间正好碰上两个触发点）而不灰；
 * - 又给「真坏了但被分进瞬时档」的那两形（`OutlineFailure` 头注登记的：远端很老的后端回 hello、
 *   本机后端过旧退出 2）封了顶：每个宿主最多白起 3 次本机进程 / 3 次 ssh exec
 *   （远端每次最坏占满 `LIST_TIMEOUT` 30 s，3 次即 90 s 的后台等待，不挡界面）。
 * 成功一次就清零 —— 数的是**连续**。
 */
export const MAX_TRANSIENT_FAILURES = 3;

/** 结构性失败：再要一定还是这样。只有这一种；其余（含缺席 = 调用本身抛了）都按瞬时。 */
function isStructural(f: OutlineFailure | undefined): boolean {
  return f === "oldBackend";
}

export class OutlineSource {
  private end = 0;
  private readonly uuids = new Set<string>();
  private inflight: Promise<void> | null = null;
  private again = false;
  /** `reset()` 一次加一；在途那趟回来发现代数变了 ⇒ 结果作废（换会话 / 关 tab 之后不许回写）。 */
  private gen = 0;
  private fetched = false;
  private stale = false;
  private rows = 0;
  /**
   * 结构性失败（或连续瞬时失败到上限）⇒ 本宿主不再要（`reset` 才清）：
   * 不然每次切 tab、每句输入都起一次注定失败的本机进程 / ssh exec。
   */
  private gaveUp = false;
  /** 连续瞬时失败的次数（成功一次清零）。 */
  private transientFailures = 0;

  constructor(
    private readonly panel: UserInputPanel,
    private readonly where: OutlineWhere,
  ) {}

  /** 列出来的条数（`debugSnapshot` 的读数）。 */
  get count(): number {
    return this.rows;
  }

  /** 要过至少一次、而且之后没有新行进来 ⇒ `false`。宿主据此决定切进来时要不要再要一截。 */
  get needsFetch(): boolean {
    return !this.gaveUp && (!this.fetched || this.stale);
  }

  /** 要到过（或确认要不到）至少一次。 */
  get everFetched(): boolean {
    return this.fetched;
  }

  /** 这份会话又长了（宿主每收一行调一次，O(1)）。只记一笔，不去要。 */
  markStale(): void {
    this.stale = true;
  }

  refresh(): Promise<void> {
    if (this.gaveUp) return Promise.resolve();
    if (this.inflight) {
      this.again = true;
      return this.inflight;
    }
    const run = this.run().finally(() => {
      this.inflight = null;
      if (this.again) {
        this.again = false;
        void this.refresh();
      }
    });
    this.inflight = run;
    return run;
  }

  /**
   * **骨架索引在途：清单先不单独要，等它带回来。**
   *
   * 宿主发「从 0 起的骨架索引」的同一刻调它（`tab-stream-view.ts::requestSkeleton`）。`seed` 兑现成：
   * - 一份清单 ⇒ 整表建好、续点接上它的 `end`（**这一趟不再单独要清单**）；
   * - `null` / 抛了（老后端 / 真的零条 / 索引失败）⇒ 照旧自己从 0 要一份。
   * 等的期间 `refresh()` 并进同一趟（沿用在途合并）。等完：没种上 ⇒ 补一趟从 0 的 `refresh()`；
   * 种上了 ⇒ 只有「等的期间真有新行进来（`markStale`）**而且**有人叫过」才补一趟增量 ——
   * 批结束那一下的 `refresh()` 只是「还没要过」，种上之后它已经不成立，不为它白发一次 IPC。
   * 已经要到过、在途、或已放弃 ⇒ 什么都不做（清单已经有了 / 正在要，索引那份不再用）。
   */
  awaitSeed(seed: Promise<OutlineSeed | null>): void {
    if (this.gaveUp || this.fetched || this.inflight) {
      seed.catch(() => {}); // 不用它也要接住它的失败（否则是一条没人处理的拒绝）
      return;
    }
    const gen = this.gen;
    let seeded = false;
    this.stale = false;
    const run = seed
      .then((s) => {
        if (gen !== this.gen || !s) return;
        this.fetched = true;
        this.transientFailures = 0;
        this.uuids.clear();
        this.panel.setEntries(s.entries);
        for (const e of s.entries) this.uuids.add(e.uuid);
        this.rows = s.entries.length;
        this.end = s.end;
        seeded = true;
      })
      .catch(() => {})
      .finally(() => {
        this.inflight = null;
        const again = this.again;
        this.again = false;
        if (gen !== this.gen) {
          if (again) void this.refresh(); // 换了会话之后有人叫过 ⇒ 那是新会话的，照要
          return;
        }
        if (!seeded || (again && this.stale)) void this.refresh();
      });
    this.inflight = run;
  }

  /** 换会话 / 关 tab：清空、收起，在途那趟作废。 */
  reset(): void {
    this.gen++;
    this.end = 0;
    this.uuids.clear();
    this.rows = 0;
    this.gaveUp = false;
    this.transientFailures = 0;
    this.again = false;
    this.fetched = false;
    this.stale = false;
    this.panel.clear();
  }

  private async run(): Promise<void> {
    const where = this.where();
    if (!where) return;
    const gen = this.gen;
    this.stale = false;
    let res: UserInputsResult;
    let base = this.end;
    try {
      res = await listUserInputs(where.origin, where.jsonlPath, this.end);
      if (gen !== this.gen) return;
      if (!res) throw new Error(copyText("outlineSource.run.noReply"));
      // 增量要不到（多半是越过 EOF = 截断/重写）或冒出已有的 uuid（重写但更长）⇒ 从 0 重要一份
      if (this.end > 0 && (!res.available || res.entries.some((e) => this.uuids.has(e.uuid)))) {
        res = await listUserInputs(where.origin, where.jsonlPath, 0);
        if (gen !== this.gen) return;
        if (!res) throw new Error(copyText("outlineSource.run.noReply"));
        base = 0;
      }
    } catch (e) {
      if (gen !== this.gen) return;
      // 调用本身抛了（IPC / 预检）⇒ 没有种类 ⇒ 按瞬时
      res = { available: false, reason: String(e), from: this.end, end: this.end, entries: [] };
    }
    this.fetched = true;
    if (!res.available) {
      this.failed(res.failure, res.reason ?? "");
      return;
    }
    this.transientFailures = 0;
    if (base === 0) {
      this.uuids.clear();
      this.rows = 0;
      this.panel.setEntries(res.entries);
    } else {
      this.panel.appendEntries(res.entries);
    }
    for (const e of res.entries) this.uuids.add(e.uuid);
    this.rows += res.entries.length;
    this.end = res.end;
  }

  /** 要不到：结构性 ⇒ 灰掉、不再要；瞬时 ⇒ 记一次、下一次触发再要（到上限按结构性）。 */
  private failed(failure: OutlineFailure | undefined, reason: string): void {
    if (!isStructural(failure)) {
      this.transientFailures++;
      if (this.transientFailures < MAX_TRANSIENT_FAILURES) {
        this.stale = true; // 下一次「切进来」也算触发点（`needsFetch`）
        // 手上已经有清单 ⇒ 不动它（续点、行、标记都不动）；一条都没有 ⇒ 灰着，原因挂提示上
        if (this.rows === 0) this.panel.setUnavailable(reason);
        return;
      }
    }
    this.end = 0;
    this.uuids.clear();
    this.rows = 0;
    this.gaveUp = true;
    this.panel.setUnavailable(reason);
  }
}
