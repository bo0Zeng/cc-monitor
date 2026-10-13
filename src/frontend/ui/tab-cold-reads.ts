/**
 * **每个 tab 的第一趟会话事实 ＋ 轮次什么时候问**（切到的先拉，没有哪一步等全部 tab）。
 *
 * 第一趟是冷读：`history-facts` 不带上一份成品、`history-turns` 从 0 —— 后端要把那份记录从头扫一遍。
 * F5 之后 N 个 tab 一起要，就是 N 份同时扫、当前 tab 那一份夹在中间。这里排成：
 *
 * - **前台**（当前 tab）当场要，记成「前台那一趟」；它落地（两问都回来，成没成都算）之前，后台一份都不发。
 * - **后台**排队；前台那一趟落地后，空闲时一个一个要，同时至多一份在途。
 * - 切进来停住的（`TabStreamView.activate` 那一下停够了）当场按前台要，从队里摘掉。
 * - 要过第一趟之后的再问（facts 带上一份成品、turns 从还没收尾那一轮）不是冷读，随时照旧要（[`want`]）。
 *
 * 只排「什么时候问」：问什么、回来怎么落都是 `FactsSource` / `TurnFold` 的事；本类不判会话事实。
 */
import type { Tab } from "./tab-model";
import type { TabStore } from "./tab-store";

export class ColdReads {
  /** 第一趟已经发过的 tab。 */
  private readonly warmed = new WeakSet<Tab>();
  /** 排着的后台 tab（sid，先来先要）。 */
  private queue: string[] = [];
  /** 前台那一趟（落地之前后台不发）。 */
  private front: Promise<unknown> | null = null;
  /** 在途的那一份后台（同时至多一份）。 */
  private running: Promise<unknown> | null = null;
  /** 已经排了一个空闲回调。 */
  private scheduled = false;

  constructor(private readonly store: TabStore) {}

  /** 这个 tab 的第一趟发过了。 */
  isWarm(tab: Tab): boolean {
    return this.warmed.has(tab);
  }

  /**
   * 这个 tab 的会话事实要再问一次（又长了 · 状态变了 · 那台说需手动变了 · 上限表换了 ……）：
   * 发过第一趟的照旧续读；没发过的 —— 当前 tab ⇒ 当场按前台要；后台 ⇒ 只记一笔、排队，轮到再要。
   */
  want(tab: Tab): void {
    tab.facts.markStale();
    if (this.warmed.has(tab)) {
      void tab.facts.refresh();
      return;
    }
    if (this.store.activeId === tab.sessionId) this.toFront(tab);
    else this.later(tab);
  }

  /** 又来了一行（非批期）：发过第一趟的照旧续读两样；没发过的同 [`want`]（当前 ⇒ 当场要；后台 ⇒ 排着）。 */
  grew(tab: Tab): void {
    if (!this.warmed.has(tab)) {
      this.want(tab);
      return;
    }
    void tab.facts.refresh();
    void tab.turnFold.refresh();
  }

  /**
   * 当前 tab（批结束 · 切进来停住了）：没发过第一趟 ⇒ 当场要、记成前台那一趟、从队里摘掉；
   * 发过 ⇒ 照旧续读（有新写的才要）。
   */
  toFront(tab: Tab): void {
    if (this.warmed.has(tab)) {
      if (tab.facts.needsFetch) void tab.facts.refresh();
      return;
    }
    this.queue = this.queue.filter((sid) => sid !== tab.sessionId);
    const p = this.warm(tab);
    if (!p) return;
    this.front = p;
    void p.finally(() => {
      if (this.front === p) this.front = null;
      this.pump();
    });
  }

  /** 后台 tab：没发过第一趟 ⇒ 排队（已排着的不重排）；发过 ⇒ 照旧续读。 */
  later(tab: Tab): void {
    if (this.warmed.has(tab)) {
      if (tab.facts.needsFetch) void tab.facts.refresh();
      void tab.turnFold.refresh();
      return;
    }
    if (!this.queue.includes(tab.sessionId)) this.queue.push(tab.sessionId);
    this.pump();
  }

  /** 关掉的 tab 出队。 */
  drop(sid: string): void {
    this.queue = this.queue.filter((s) => s !== sid);
  }

  /** 发第一趟（facts ＋ turns），两问都回来（成没成都算）才落地。路径还没到（tab 先于首条行建出来）⇒ 不算发过，下次再说。 */
  private warm(tab: Tab): Promise<unknown> | null {
    if (!tab.parentPath) return null;
    this.warmed.add(tab);
    return Promise.allSettled([tab.facts.refresh(), tab.turnFold.refresh()]);
  }

  /** 前台那一趟落地了、没有在途的后台 ⇒ 空闲时要队首那一个。 */
  private pump(): void {
    if (this.front || this.running || this.scheduled || this.queue.length === 0) return;
    this.scheduled = true;
    const run = (): void => {
      this.scheduled = false;
      if (this.front || this.running) return; // 等的时候来了前台那一趟：它落地后再排
      for (;;) {
        const sid = this.queue.shift();
        if (sid === undefined) return;
        const tab = this.store.tabs.get(sid);
        if (!tab || this.warmed.has(tab)) continue;
        const p = this.warm(tab);
        if (!p) continue; // 还没有路径：首条行到了会再来（`want`）
        this.running = p;
        void p.finally(() => {
          if (this.running === p) this.running = null;
          this.pump();
        });
        return;
      }
    };
    if (typeof window.requestIdleCallback === "function") {
      // 调度：自链 —— 后台 tab 的第一趟会话事实与轮次：空闲时要一个，回来了再排下一个，队空即停
      window.requestIdleCallback(run, { timeout: 2000 });
    } else {
      // 调度：自链 —— 上面那条在没有 rIC 的引擎（WebKitGTK）上的兜底，同一条链
      window.setTimeout(run, 200);
    }
  }
}
