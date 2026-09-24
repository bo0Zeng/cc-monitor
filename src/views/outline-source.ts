/**
 * 〔SE1 · `设计/10 §2.2b ⑥`〕**大纲的数据源：问后端要，不在前端攒。**
 *
 * # 它顶掉了什么
 *
 * 实时 tab 从前在 `onLine` 的旁路里一条一条攒「我说过的每一句」（`Tab.userInputs`，
 * 喂它的是 `toUserInputEntry`）—— **到达序不是对话序**（重放是尾块先到），monitor 起得晚
 * 清单就不全，每个 tab 各攒一份。历史查看器那边再拿同一份 TS 判定扫全量 payloads。
 * 两样都删了：「什么算一条用户输入」只住后端（`observe/user_inputs.rs`），
 * 两个宿主都经 `list_user_inputs` 来要（`IPC-PROTOCOL.md §10.4`）。
 *
 * # 形状
 *
 * 一个宿主（一个 tab / 一个查看器）一份。它只记三样：上次要到哪个字节（`end`，下一次增量从这里接）·
 * 已经列出来的 uuid（认「文件被重写」用）· 在途与否。**不存正文，不存摘要** —— 摘要在面板的行上。
 *
 * - `refresh()`：从 `end` 接着要一截，追加到面板上。在途时再叫 ⇒ 这一趟回来后**只补一趟**（合并）。
 * - 增量要不到（后端报「越过 EOF」= 文件被截断/重写）或增量里冒出**已有的** uuid（被重写但更长）
 *   ⇒ 从 0 重要一份、整表换掉。
 * - 要不到清单（老后端 / 本机后端不在 / 截断）⇒ 面板灰掉、原因挂在开关的提示上（**不是错误**），
 *   本宿主此后不再要（`gaveUp`；关 tab / 换会话 `reset` 才再试）。
 *
 * # 什么时候要（由宿主决定，本类不自己起定时器）
 *
 * 实时 tab：批结束时的 active tab · 切进来的 tab（有新行才要）· 真用户输入上屏那一刻
 * （渲染那边已有的 `onRealUserInput` 事件 —— 「刚刚发生了什么」留在流上，`设计/10 §2.2`）。
 * 查看器：加载完要一次。
 */
import { commands } from "../ipc/commands";
import type { Origin } from "../generated/Origin";
import type { UserInputsResult } from "../generated/UserInputsResult";
import type { UserInputPanel } from "./user-input-panel";

/** 这份清单问的是哪台机器上的哪份会话。拿不到（tab 还没收到路径）⇒ `null`，这一趟不要。 */
export type OutlineWhere = () => { origin: Origin; jsonlPath: string } | null;

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
   * 上一趟**要不到**（老后端 / 本机后端不在 / 截断）⇒ 本宿主这一辈子不再要（`reset` 才清）。
   * 同骨架索引「成不成都不重拉」：不然每次切 tab、每句输入都起一次注定失败的本机进程 / ssh exec。
   */
  private gaveUp = false;

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

  /** 换会话 / 关 tab：清空、收起，在途那趟作废。 */
  reset(): void {
    this.gen++;
    this.end = 0;
    this.uuids.clear();
    this.rows = 0;
    this.gaveUp = false;
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
    try {
      res = await commands.list_user_inputs({ ...where, fromOffset: this.end });
      if (gen !== this.gen) return;
      if (!res) throw new Error("没有回包");
      const rewritten =
        this.end > 0 && (!res.available || res.entries.some((e) => this.uuids.has(e.uuid)));
      if (rewritten) {
        res = await commands.list_user_inputs({ ...where, fromOffset: 0 });
        if (gen !== this.gen) return;
        this.end = 0;
        this.uuids.clear();
        this.rows = 0;
      }
    } catch (e) {
      if (gen !== this.gen) return;
      res = { available: false, reason: String(e), from: this.end, end: this.end, entries: [] };
    }
    this.fetched = true;
    if (!res.available) {
      this.end = 0;
      this.uuids.clear();
      this.rows = 0;
      this.gaveUp = true;
      this.panel.setUnavailable(res.reason ?? "");
      return;
    }
    if (this.end === 0) this.panel.setEntries(res.entries);
    else this.panel.appendEntries(res.entries);
    for (const e of res.entries) this.uuids.add(e.uuid);
    this.rows += res.entries.length;
    this.end = res.end;
  }
}
