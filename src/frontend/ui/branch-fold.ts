/**
 * ESC 回退分支的折叠 UI：消息流容器（卡片已按记录顺序摆好）＋ 主线外清单（回退掉的那几条记录的 `id`）⇒
 * 把连续的主线外卡包进 `.branch-fold-wrap`。
 *
 * 清单是那台后端给的成品（实时帧 `branch` 整份 · 冷读 `history-branch`），界面不判谁在主线上、只按清单排版：
 * - 段：`data-id` 在清单里的卡连成一段；不在清单里的卡与没有 `data-id` 的元素断段。
 *   清单一换可能一大段卡转进 / 转出主线，重折按段差量做，见 `rebuild`。
 * - 折叠状态保留：wrap 的展开态记在 `foldExpanded`（key = 段首条 id）。
 * - 增量读到的、在已有清单里的那几条照样折（清单不随读法变；插卡之后 `rebuildNow` 重折一遍）。
 */

import { copyText } from "./copy-table";
import { icon } from "./kit/icon";

const FOLD_WRAP_CLASS = "branch-fold-wrap";
const FOLD_HEADER_CLASS = "branch-fold-header";
const FOLD_ARROW_CLASS = "branch-fold-arrow";
const FOLD_BODY_CLASS = "branch-fold-body";
const FOLD_BODY_INNER_CLASS = "branch-fold-body-inner";

/**
 * 跟随一个具体的 stream 容器，管它的 fold 重建。
 *
 * 每个 Tab / SessionViewer / 子运行窗口持有一个实例。
 */
export class BranchFolder {
  /** stream 容器（卡片直接挂在它的 children 上，可能跟 fold-wrap 混合） */
  private container: HTMLElement;
  /** 此刻的主线外清单（后端给的整份）。 */
  private off: ReadonlySet<string> = new Set();
  /** 折叠 ID（每个 fold 的第一条 id） → 用户是否手动展开了 */
  private foldExpanded = new Map<string, boolean>();
  /** 每个 wrap 包它时有几条（标题上那个数）—— 差量重折判「这个 wrap 还原样可用吗」用 */
  private wrapSize = new WeakMap<Element, number>();
  /** 帧末那一次重折排上了没有（新卡挂进来 ⇒ 帧末重折一次，一帧来多少条都只折一次）。 */
  private scheduled = false;
  private disposed = false;

  constructor(container: HTMLElement) {
    this.container = container;
  }

  /** 此刻的主线外清单有几条（DEV 探针用）。 */
  get offCount(): number {
    return this.off.size;
  }

  /**
   * **换一份主线外清单**（整份）并当场重折。清单与上一份相同 ⇒ 什么都不动。
   */
  setOff(off: ReadonlySet<string>): void {
    if (this.disposed) return;
    if (setsEqual(off, this.off)) return;
    this.off = new Set(off);
    this.rebuild();
  }

  /**
   * 新卡挂进来之后调：清单非空 ⇒ 帧末重折一次（新卡可能正落在回退掉的那一段里 / 夹在两段之间）；清单空 ⇒ 什么都不做。
   */
  cardsAdded(): void {
    if (this.off.size === 0 || this.scheduled || this.disposed) return;
    this.scheduled = true;
    const run = (): void => {
      this.scheduled = false;
      if (!this.disposed) this.rebuild();
    };
    if (typeof requestAnimationFrame === "function") {
      // 调度：合批 —— 新卡挂进来之后的重折排到帧末，排一次位
      requestAnimationFrame(run);
    } else {
      // 调度：合批 —— 上面那一处在没有 rAF 时的兜底，0ms 一次
      setTimeout(run, 0);
    }
  }

  /**
   * 物化 / 上翻补批后的无条件重折：插卡前摊平过的折叠段（`unwrapAll`）要按此刻的清单重新包起来。
   */
  rebuildNow(): void {
    if (this.disposed) return;
    // 清单空、也没有折着的段 ⇒ 无事可做（批末每个 tab 都叫一次，别白扫一遍）。
    if (this.off.size === 0 && !this.container.querySelector(`:scope > .${FOLD_WRAP_CLASS}`)) return;
    this.rebuild();
  }

  /** Tab 销毁时调，断 GC 引用 */
  dispose(): void {
    this.disposed = true;
    this.off = new Set();
    this.foldExpanded.clear();
  }

  // === 内部 DOM 操作 ===

  /**
   * 按主线外清单重折 fold 结构 —— 按段差量，不全量解开重包。
   *
   * 1. 逻辑序列：顶层子节点依次读；遇到 wrap 就读它 inner 里的卡（不搬）。
   * 2. 目标段：逻辑序列里连续的、`data-id` 在清单里的卡（无 `data-id` 的元素断段）。
   * 3. 现存 wrap 若**恰好**等于某个目标段（inner 的卡 == 段成员、同序）⇒ 原地不动（展开态、DOM 都不碰）；
   *    其余 wrap 解开（卡搬回 wrap 所在位置）；没有现成 wrap 的目标段新包一个。
   *
   * DOM 写只落在归属真变了的段上；扫描是 O(顶层子节点 ＋ 折叠卡数) 次读。
   * 等价：差量结果与「平铺容器上从零折一遍」逐字相同（`tests/frontend/ui/branch-fold-batching.vitest.ts`）。
   */
  private rebuild(): void {
    const off = this.off;
    // 第 1 步：逻辑序列（wrap 展开读，不搬）
    const items: Array<{ el: HTMLElement; wrap: HTMLElement | null }> = [];
    const innerCount = new Map<HTMLElement, number>();
    for (const child of Array.from(this.container.children)) {
      const el = child as HTMLElement;
      if (el.classList.contains(FOLD_WRAP_CLASS)) {
        const inner = el.querySelector(`.${FOLD_BODY_INNER_CLASS}`);
        const kids = inner ? Array.from(inner.children) : [];
        innerCount.set(el, kids.length);
        for (const k of kids) items.push({ el: k as HTMLElement, wrap: el });
      } else {
        items.push({ el, wrap: null });
      }
    }

    // 第 2 步：目标段
    const runs: Array<Array<{ el: HTMLElement; wrap: HTMLElement | null; uuid: string }>> = [];
    let cur: (typeof runs)[number] | null = null;
    for (const it of items) {
      const uuid = it.el.getAttribute("data-id");
      if (uuid && off.has(uuid)) {
        if (!cur) {
          cur = [];
          runs.push(cur);
        }
        cur.push({ ...it, uuid });
      } else {
        cur = null;
      }
    }

    // 第 3 步：恰好等于目标段的现存 wrap 留着；其余解开；缺的新包
    const kept = new Set<HTMLElement>();
    const todo: typeof runs = [];
    for (const run of runs) {
      const w = run[0].wrap;
      // 「恰好等于」：段成员全在 w 里、w 里没有别的卡 —— 还要 w 的键（首条 id）与包它时记下的条数都没变：
      // 有卡插进段首（`insertNode` 插在锚点前，锚点在段里就插进段里）会让键过期；段内一增一减会让标题上的条数过期。
      if (
        w &&
        !kept.has(w) &&
        run.every((it) => it.wrap === w) &&
        innerCount.get(w) === run.length &&
        w.getAttribute("data-fold-key") === run[0].uuid &&
        this.wrapSize.get(w) === run.length
      ) {
        kept.add(w);
      } else {
        todo.push(run);
      }
    }
    this.unwrapFolds([...innerCount.keys()].filter((w) => !kept.has(w)));
    for (const run of todo) {
      this.wrapRun(run[0].el, run[run.length - 1].el, run.map((it) => it.uuid));
    }
  }

  /**
   * 把所有现存 fold-wrap 解开。往折叠后的 DOM 里二分插入前先摊平：timeline 的邻居可能已被搬进 fold wrap
   * （不是 container 的直接子节点），insertBefore 会 NotFoundError。插完由 `rebuildNow` 重折。
   */
  unwrapAll(): void {
    this.unwrapFolds(Array.from(this.container.querySelectorAll(`:scope > .${FOLD_WRAP_CLASS}`)));
  }

  /** 解开给定的这几个 wrap（差量重折只解归属变了的那几个）。 */
  private unwrapFolds(wraps: ReadonlyArray<Element>): void {
    for (const wrap of wraps) {
      const inner = wrap.querySelector(`.${FOLD_BODY_INNER_CLASS}`);
      // 记下展开状态，下次重建可继承
      const firstUuid = wrap.getAttribute("data-fold-key");
      if (firstUuid) {
        const expanded = wrap.classList.contains("expanded");
        this.foldExpanded.set(firstUuid, expanded);
      }
      if (inner) {
        // 把 inner 里的卡片 move 回 wrap 的位置（在 container 上）
        const cards = Array.from(inner.children);
        for (const card of cards) {
          this.container.insertBefore(card, wrap);
        }
      }
      wrap.remove();
    }
  }

  /**
   * 把 [start, end] 这一段连续元素包到 fold-wrap 里。
   */
  private wrapRun(start: HTMLElement, end: HTMLElement, uuids: string[]): void {
    const foldKey = uuids[0]; // 用第一条 uuid 当稳定 key
    const expanded = this.foldExpanded.get(foldKey) ?? false; // 默认折叠

    const wrap = document.createElement("div");
    wrap.className = FOLD_WRAP_CLASS;
    wrap.setAttribute("data-fold-key", foldKey);
    this.wrapSize.set(wrap, uuids.length);
    // 折叠态真高 ≈ 34px（header 一行）；兜底 120px 偏大 3 倍。展开后由 content-visibility 的 auto 记住真高。
    wrap.style.setProperty("contain-intrinsic-size", "auto 34px");
    if (expanded) wrap.classList.add("expanded");

    const header = document.createElement("div");
    header.className = FOLD_HEADER_CLASS;
    header.setAttribute("role", "button");
    header.setAttribute("tabindex", "0");
    header.setAttribute("aria-expanded", expanded ? "true" : "false");

    const arrow = document.createElement("span");
    arrow.className = FOLD_ARROW_CLASS;
    arrow.appendChild(icon("caretRight", "compact"));
    header.appendChild(arrow);

    const title = document.createElement("span");
    title.className = "branch-fold-title";
    title.textContent = copyText("branchFold.wrapRun.summary", { n: uuids.length });
    header.appendChild(title);

    const toggleFn = () => {
      const nowExpanded = !wrap.classList.contains("expanded");
      wrap.classList.toggle("expanded", nowExpanded);
      header.setAttribute("aria-expanded", nowExpanded ? "true" : "false");
      this.foldExpanded.set(foldKey, nowExpanded);
    };
    header.addEventListener("click", toggleFn);
    header.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        toggleFn();
      }
    });

    const body = document.createElement("div");
    body.className = FOLD_BODY_CLASS;
    const inner = document.createElement("div");
    inner.className = FOLD_BODY_INNER_CLASS;
    body.appendChild(inner);

    // 把 wrap 插到 start 位置，然后把 [start, end] 全部 move 进 inner
    this.container.insertBefore(wrap, start);
    wrap.appendChild(header);
    wrap.appendChild(body);

    // 收集 start 到 end 之间的所有节点（包含两端）
    const toMove: HTMLElement[] = [];
    let cursor: ChildNode | null = start;
    while (cursor) {
      const next: ChildNode | null = cursor.nextSibling;
      toMove.push(cursor as HTMLElement);
      if (cursor === end) break;
      cursor = next;
    }
    for (const node of toMove) {
      inner.appendChild(node);
    }
  }
}

function setsEqual(a: ReadonlySet<string>, b: ReadonlySet<string>): boolean {
  if (a.size !== b.size) return false;
  for (const x of a) if (!b.has(x)) return false;
  return true;
}
