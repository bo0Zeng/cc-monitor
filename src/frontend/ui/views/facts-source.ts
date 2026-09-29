/**
 * 〔STC · `设计/90 §4` 阶段 C · `设计/10 §2.2`〕**会话事实的数据源：问后端要，不在前端攒。**
 *
 * # 它顶掉了什么
 *
 * 分叉血缘 · 改动文件集 · agent 列表 · 最新 usage 这四样，活 tab 从前在 `onLine` 的旁路上一条一条攒
 * （`tab-session-facts.ts` 里的四个抽取器，已删）：**到达序不是对话序**、重放缓冲只留尾部 ⇒ F5 之后**不完整**、
 * 每个 tab 各攒一份（`10 §2.2` 那三个病）。今天它们由后端读一遍文件出成品（`history-facts`，`session-reads.ts` 第五问），
 * 判定与累加**只在后端**；本类只做调用方那一侧的事。
 *
 * # 形状（形照大纲的 `OutlineSource`，`10 §2.2b ⑥`）
 *
 * 一个 tab 一份。它只记：上一份成品（**原样**当续传令牌交回去 —— 不读、不改、不合并）· 在途与否 · 失败了几次。
 *
 * - `refresh()`：把上一份成品当 `prior` 交回去，要回累加了新写那一截的整份成品。在途时再叫 ⇒ 这一趟回来后**只补一趟**。
 * - 带着 `prior` 要不到（后端说续点越过文件尾 / 不在行边界 = 文件被截断或重写，或者别的原因）⇒ **不带 `prior` 从 0 重要一趟**。
 * - 要不到：结构性（`oldBackend`：对面不认这条命令）⇒ 不再要；瞬时 ⇒ 下一次触发再要，连续 [`MAX_TRANSIENT_FAILURES`] 次按结构性。
 *   手上已经有成品时瞬时失败**不动它**；放弃了、或一份都还没要到 ⇒ `unavailableReason` 非空（宿主据此出声）。
 *
 * # 什么时候要（由宿主决定，本类不起定时器）
 *
 * 宿主（`tab-stream-view.ts`）：一行新记录到了且不在批期 ⇒ 要；批结束 ⇒ 凡是「没要过或又长了」的 tab 都要
 * （不只 active：分叉 `↳` 在 tab 栏上、监控板每格都显示 context% 与 agent 数）。
 */
import type { Origin } from "../ipc/origin";
import { readSessionFacts, type FactsResult, type SessionFacts } from "../session-reads";
import type { OutlineFailure } from "../session-reads";
import { MAX_TRANSIENT_FAILURES } from "./outline-source";

/** 这份事实问的是哪台机器上的哪份会话。拿不到（tab 还没收到路径）⇒ `null`，这一趟不要。 */
export type FactsWhere = () => { origin: Origin; jsonlPath: string } | null;

/** 事实到了 / 可不可用变了 —— 宿主据此落到 tab 上、刷界面。 */
export interface FactsSink {
  /** 要到了一份新的成品（整份，不是增量）。`first` = 这是本 tab 要到的第一份（`reset` 之后重新算）。 */
  facts(f: SessionFacts, first: boolean): void;
  /** 可不可用变了：`null` = 可用；非空 = 要不到的原因（放弃了，或一份都还没要到）。 */
  availability(reason: string | null): void;
}

export class FactsSource {
  private last: SessionFacts | null = null;
  private inflight: Promise<void> | null = null;
  private again = false;
  /** `reset()` 一次加一；在途那趟回来发现代数变了 ⇒ 结果作废（关 tab 之后不许回写）。 */
  private gen = 0;
  private fetched = false;
  private stale = false;
  private gaveUp = false;
  private transientFailures = 0;
  private reason: string | null = null;

  constructor(
    private readonly where: FactsWhere,
    private readonly sink: FactsSink,
  ) {}

  /** 没要过、或之后又长了（且没放弃）⇒ `true`。 */
  get needsFetch(): boolean {
    return !this.gaveUp && (!this.fetched || this.stale);
  }

  /** 要到过至少一份成品。 */
  get everArrived(): boolean {
    return this.last !== null;
  }

  /** 要不到的原因（`null` = 可用或还没问过）。 */
  get unavailableReason(): string | null {
    return this.reason;
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

  /** 关 tab：清空，在途那趟作废。 */
  reset(): void {
    this.gen++;
    this.last = null;
    this.fetched = false;
    this.stale = false;
    this.gaveUp = false;
    this.transientFailures = 0;
    this.again = false;
    this.reason = null;
  }

  private async run(): Promise<void> {
    const where = this.where();
    if (!where) return;
    const gen = this.gen;
    this.stale = false;
    let res: FactsResult = await readSessionFacts(where.origin, where.jsonlPath, this.last);
    if (gen !== this.gen) return;
    if (!res.available && this.last) {
      res = await readSessionFacts(where.origin, where.jsonlPath, null);
      if (gen !== this.gen) return;
    }
    this.fetched = true;
    if (!res.available) {
      this.failed(res.failure, res.reason);
      return;
    }
    this.transientFailures = 0;
    const first = this.last === null;
    this.last = res.facts;
    this.setReason(null);
    this.sink.facts(res.facts, first);
  }

  /** 要不到：结构性 ⇒ 放弃；瞬时 ⇒ 记一次、下一次触发再要（到上限按结构性）。 */
  private failed(failure: OutlineFailure, reason: string): void {
    if (failure !== "oldBackend") {
      this.transientFailures++;
      if (this.transientFailures < MAX_TRANSIENT_FAILURES) {
        this.stale = true; // 下一个触发点还算「要」
        if (this.last === null) this.setReason(reason); // 手上已有成品 ⇒ 不动它、不出声
        return;
      }
    }
    this.gaveUp = true;
    this.setReason(reason);
  }

  private setReason(reason: string | null): void {
    if (this.reason === reason) return;
    this.reason = reason;
    this.sink.availability(reason);
  }
}
