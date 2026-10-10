/**
 * **一个子运行的时间线**：按运行读那台后端的记录（通用命令 `history-run`），用与主运行同一套渲染器画；尾巴上留一块给活卡（宿主画）。
 * agent 窗口里的消息流就是它（`views/agent-window.ts`）。运行表一变（`refresh`）就从上次读到的地方续读，不整份重读；
 * 读失败只在尾巴上挂一句，已画出来的不擦。回退掉的那几条按那份记录的主线外清单折起来（第一页到了冷读一次 `history-branch`）。
 */
import type { LineRecord } from "./generated/LineRecord";
import type { Origin } from "./ipc/origin";
import { loadRunPage, readBranch, type RunPage, type RunWhich } from "./record-reads";
import { BranchFolder } from "./branch-fold";
import { markCardId } from "./render-stream-record";
import { copyText } from "./copy-table";
import { sayFailure } from "./kit/detail";
import { observeForEnhance } from "./render";

/** 渲染器给出来的那一形（`cards/index.ts::renderMessage` 的结果，只取要用的两种）。 */
export type RunRender = (
  rec: LineRecord,
) => { kind: "card"; element: HTMLElement } | { kind: "tool-group"; units: HTMLElement[] } | { kind: "skip" };

/** 一次最多续读几页（一页 ≤ 1 MiB；再多下一次运行表变了再读）。 */
export const RUN_PAGES_PER_REFRESH = 8;

export interface RunTimelineDeps {
  origin: Origin;
  /** 父记录路径（派出它的那个会话的记录）。 */
  parent: string;
  which: RunWhich;
  render: RunRender;
  /** 读到一条带对账键的记录（撤这个子运行同一次应答的活卡）。 */
  onRecord?: (rid: string) => void;
  /** 读法（判据换成假的；缺 ＝ 经通道问那台后端）。 */
  load?: (from: number) => Promise<RunPage>;
  /**
   * 惰路渲染（`RenderContext.lazy`）留下的代码块 / 公式占位：交给这个滚动容器，滚进视口再补（与主窗口 / 查看窗同一套）。
   * 缺 ＝ 渲染器当场补完，这里什么都不做。
   */
  enhanceRoot?: HTMLElement;
  /** 那份记录的主线外清单怎么问（判据换成假的；缺 ＝ 经通道问 `history-branch`）。 */
  branch?: (path: string) => Promise<string[]>;
}

export class RunTimeline {
  readonly element: HTMLElement;
  readonly body: HTMLElement;
  /** 活卡画在这里（由宿主画）。 */
  readonly live: HTMLElement;
  private end = 0;
  private loading: Promise<void> | null = null;
  private again = false;
  /** 上一次读失败挂在尾巴上的那一行（下次读成了就摘；已经画出来的记录不动）。 */
  private errorEl: HTMLElement | null = null;
  /** 读到过的那份记录（查看器整份打开用）。 */
  path: string | null = null;
  private readonly folder: BranchFolder;
  /** 主线外清单问过没有（一份记录问一次）。 */
  private branchAsked = false;

  constructor(private readonly deps: RunTimelineDeps) {
    this.element = document.createElement("div");
    this.element.className = "block-body block-agent-body";
    this.body = document.createElement("div");
    this.live = document.createElement("div");
    this.element.append(this.body, this.live);
    this.body.textContent = copyText("runs.timeline.loading");
    this.folder = new BranchFolder(this.body);
  }

  /** 从上次读到的地方续读（正在读 ⇒ 读完再来一次）。 */
  refresh(): Promise<void> {
    if (this.loading) {
      this.again = true;
      return this.loading;
    }
    this.loading = this.pull().finally(() => {
      this.loading = null;
      if (this.again) {
        this.again = false;
        void this.refresh();
      }
    });
    return this.loading;
  }

  private async pull(): Promise<void> {
    const load = this.deps.load ?? ((from: number) => loadRunPage(this.deps.origin, this.deps.parent, this.deps.which, from));
    try {
      for (let i = 0; i < RUN_PAGES_PER_REFRESH; i++) {
        const page = await load(this.end);
        this.errorEl?.remove();
        this.errorEl = null;
        if (this.end === 0) this.body.replaceChildren(); // 第一页到了：摘掉「正在读」
        this.path = page.path;
        this.end = page.end;
        this.askBranch(page.path);
        for (const row of page.rows) {
          const r = this.deps.render(row.record);
          const root = this.deps.enhanceRoot;
          if (r.kind === "card") {
            markCardId(r.element, row.record);
            this.body.appendChild(r.element);
            if (root) observeForEnhance(r.element, root);
          } else if (r.kind === "tool-group") {
            const wrap = document.createElement("div");
            wrap.className = "block-agent-tool-group";
            for (const u of r.units) wrap.appendChild(u);
            this.body.appendChild(wrap);
            if (root) for (const u of r.units) observeForEnhance(u, root);
          }
          if (row.rid !== undefined) this.deps.onRecord?.(row.rid);
        }
        this.folder.rebuildNow();
        if (!page.more) return;
      }
    } catch (e) {
      this.folder.rebuildNow();
      // 读失败只在尾巴上说一句：之前画出来的留着，下次从读到的地方接着读（运行表再到 / 再展开都会再读）。
      if (this.end === 0) this.body.replaceChildren();
      this.errorEl?.remove();
      const err = document.createElement("div");
      err.className = "block-agent-error";
      sayFailure(err, copyText("runs.timeline.failed"), e);
      this.body.appendChild(err);
      this.errorEl = err;
    }
  }

  /** 那份记录的主线外清单：第一页到了问一次，到了就按它折（读不到 ⇒ 不折）。 */
  private askBranch(path: string): void {
    if (this.branchAsked) return;
    this.branchAsked = true;
    const ask = this.deps.branch ?? ((p: string) => readBranch(this.deps.origin, p).then((b) => b.off));
    void ask(path).then(
      (off) => this.folder.setOff(new Set(off)),
      (e: unknown) => console.warn("[run-timeline] 主线外清单没读到（不折）：", e),
    );
  }
}
