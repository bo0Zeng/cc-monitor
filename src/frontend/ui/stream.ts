/**
 * 单个 Tab 的消息流容器。负责按位置插卡 + 自动贴底滚动。
 *
 * 设计：
 *   .stream          — 外层，absolute inset:0，overflow-y:auto（滚动容器）
 *     └ .stream-content — 内层包装，所有卡片挂在这里
 *
 * RecordTimeline 是唯一调用方：`insertNode` 按 seq 把卡插到正确位置。
 *
 * 用 ResizeObserver 观察 .stream-content 与 .stream 自己两根轴：
 *   - 内容那根答「内容长高了吗」→ 贴底；
 *   - 容器那根答「视口变大了吗」→ 报给宿主去补批（`onViewportResize`）。
 *
 * 内容那根的触发面：
 *   - 卡片插入 / 长高、tool 组追加单元、collapsible 展开、Markdown 字体/图片后载
 *   - Tab visibility 切回时（0×0 → 真实尺寸）
 *   都会触发回调 → stickToBottom 时 snap 贴底。
 *
 * 粘底状态（stickToBottom）由用户滚动决定：
 *   - 用户向上滚 → 解开粘底；再回到底部 24px 内 → 恢复
 *   - 粘底时尺寸增长自动贴底；不粘底时不打扰用户
 *
 * **贴底稳定性（INVARIANTS § 21，启动重放消抖的关键）**：
 *   - `snap()` 是「守卫式」的：只在确实落后底部 >1px 时才写 scrollTop。每帧无脑
 *     `scrollTop = scrollHeight` 会在 HiDPI 分数像素下因舍入误差逐帧 ±0.5px 抖。
 *   - 内容插到「视口上方」时不手动补偿 scrollTop，交给浏览器原生 `overflow-anchor`
 *     维持视觉稳定（两者叠加会 double-shift）。
 *   - 重放期视口上方的旧内容不建 DOM（尾部优先收纳，TailWindow 账本）——「逐帧上方插入」从源头没有（INVARIANTS § 21.3）。
 */
export class MessageStream {
  private scrollEl: HTMLElement;
  private contentEl: HTMLElement;
  /** 流尾巴上的那一块（活卡住这里）；第一次要的时候才建（见 `trailerElement`）。 */
  private trailerEl: HTMLElement | null = null;
  /** 是否粘底（用户向上滚动后变 false） */
  private stickToBottom = true;
  /** 物化大批插卡期间暂停逐卡守卫 snap（见 batchInsert） */
  private snapSuspended = false;
  /** 停放：这条流所在的 tab 切走了（`content-visibility: hidden` 收起）—— 收起期间几何不作数（见 `park`）。 */
  private parked = false;
  /** 停放期间容器尺寸变过（漏报给宿主的那一次）：翻出来的下一帧补报。 */
  private missedResize = false;
  private resizeObserver: ResizeObserver;
  private scrollHandler: () => void;
  private disposed = false;

  constructor(root: HTMLElement) {
    this.scrollEl = root;

    this.contentEl = document.createElement("div");
    this.contentEl.className = "stream-content";
    this.scrollEl.appendChild(this.contentEl);

    this.scrollHandler = () => {
      if (this.parked) return; // 收起那一下的 scroll：读数是假的，粘不粘底照切走那一刻的
      const distFromBottom =
        this.scrollEl.scrollHeight -
        this.scrollEl.scrollTop -
        this.scrollEl.clientHeight;
      this.stickToBottom = distFromBottom < 24;
    };
    this.scrollEl.addEventListener("scroll", this.scrollHandler);

    this.resizeObserver = new ResizeObserver((entries) => {
      if (this.stickToBottom) this.snap();
      // 视口自己变大也要有人管：拉高窗口、收起侧栏时变的是 `scrollEl` 自己，内容没动，
      // 而 `fillAbove` 挂在 scroll 事件上（不可滚的元素不产生 scroll）⇒ 观察 `scrollEl`，宿主自己决定补不补。
      if (this.onViewportResize && entries.some((e) => e.target === this.scrollEl)) {
        // 停放期间不报：收起 / 翻出时滚动条没了又有了，容器宽就变一下 —— 每切一下新旧两个 tab 各来一次，
        // 宿主去重估列宽、重排刻度就是逼浏览器排那棵被跳过的子树。记一笔，翻出来的下一帧补报。
        if (this.parked) this.missedResize = true;
        else this.onViewportResize();
      }
    });
    this.resizeObserver.observe(this.contentEl);
    this.resizeObserver.observe(this.scrollEl);
  }

  /**
   * 步 3：滚动容器**自身**尺寸变了（窗口拉高 / 布局变化）。宿主用它重新补批。
   *
   * 🔴 只报事实，不替宿主做决定：这里不知道「该不该补」——那要看 tab 是不是 active、
   * 账本里还有没有东西。`MessageStream` 手上一样都没有。
   */
  onViewportResize?: () => void;

  /**
   * 释放 RO + scroll listener。Tab 被关闭时调用，避免每次关 Tab 累积一个
   * MessageStream 实例 + RO 回调闭包持有 contentEl。
   */
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.resizeObserver.disconnect();
    this.scrollEl.removeEventListener("scroll", this.scrollHandler);
  }

  /**
   * RecordTimeline 按 seq 找到位置后调用：把卡插到 `anchor`（下一个兄弟节点）前；
   * anchor 为 null 时追加到末尾（= 当前最新，贴底跟随的常态）。
   *
   * 贴底跟随只在 stickToBottom 时做，且 `snap()` 自带「已在底部就不重钉」守卫。
   * 内容插到「视口上方」时**不手动补偿 scrollTop** —— 交给浏览器原生 `overflow-anchor`
   * 维持视觉稳定（手动补偿与 anchoring 叠加会 double-shift；每帧重钉则亚像素抖动）。
   * 详见类头注释「贴底稳定性」与 INVARIANTS § 21。
   */
  insertNode(node: HTMLElement, anchor: HTMLElement | null): void {
    // anchor 可能已被折进 .branch-fold-wrap ⇒ 就插在 anchor 前（它在哪层就在哪层）：插在整段之前会错序，而 rebuild 不重排卡。
    // 新卡若其实在主线上，`recordAdded` 排的帧末重算会触发 rebuild 把它摘出段。anchor 不在本流里（理论上走不到）⇒ 末尾追加并出声。
    if (anchor && this.contentEl.contains(anchor) && anchor.parentElement) {
      anchor.parentElement.insertBefore(node, anchor);
    } else {
      if (anchor) console.warn("[stream] insertNode 的锚点不在本流里 —— 退回末尾追加（D4）");
      this.contentEl.appendChild(node);
    }
    if (this.stickToBottom && !this.snapSuspended) {
      this.snap();
    }
  }

  /** 物化 / 补批大批插卡：期间暂停逐卡守卫 snap（每次读 scrollHeight 都是一次强制 reflow），批末按粘底状态一次贴底。 */
  batchInsert(fn: () => void): void {
    this.snapSuspended = true;
    try {
      fn();
    } finally {
      this.snapSuspended = false;
    }
    if (this.stickToBottom) this.snap();
  }

  /**
   * **停放 / 翻出**：这条流所在的 tab 切走（`park(true)`）/ 切回（`park(false)`）。切走的 tab 用 `content-visibility: hidden`
   * 收起（`styles.css` 的 `.stream`）—— 浏览器跳过整棵子树的样式 / 布局 / 绘制、切回来沿用收起前的渲染状态；代价是收起期间
   * 几何不可信（Chromium 读 `scrollTop` 得 0、`scrollHeight` 得视口高，翻出来才还原，收起那一下还来一次 `scroll`）。
   * 停放期间：`scroll` 不改粘底状态，来新卡不贴底（往收起的容器里写 `scrollTop` 是白写，还逼浏览器当场排那棵子树）。
   * 翻出时这里不贴底 —— 宿主切进来的下一帧按 `stuckToBottom` 决定（`tabs.ts::switchTo`）。
   */
  park(on: boolean): void {
    if (this.parked === on) return;
    this.parked = on;
    if (!on && this.missedResize) {
      this.missedResize = false;
      // 调度：一次性 —— 翻出来的下一帧补报停放期间漏掉的那次尺寸变化（同步段不读几何）
      requestAnimationFrame(() => {
        if (!this.parked && !this.disposed) this.onViewportResize?.();
      });
    }
  }

  /** 此刻是不是贴着底（用户往上翻过 ⇒ `false`）。切回一个 tab 时据它决定要不要贴底。 */
  get stuckToBottom(): boolean {
    return this.stickToBottom;
  }

  /** 强制贴底（切回一个本来贴着底的 tab · 点「回到底部」） */
  scrollToBottom(): void {
    this.stickToBottom = true;
    this.snap();
  }

  private snap(): void {
    if (this.parked) return;
    const el = this.scrollEl;
    // 只在确实落后底部 >1px 时才贴底。内容持续在视口上方插入时，原生 overflow-anchor
    // 已把 scrollTop 维持在底部，这里就不再每帧 scrollTop=scrollHeight 重钉 —— 那会在
    // HiDPI 分数像素布局下因整数 scrollHeight 与分数布局的舍入误差每帧不同，整块内容 ±0.5px 抖。
    if (el.scrollHeight - el.clientHeight - el.scrollTop > 1) {
      el.scrollTop = el.scrollHeight;
    }
  }

  /**
   * **流尾巴上的一块**：挂在 `.stream-content` 之后（全部卡之后），不进时间线 —— 活卡（`live-card.ts`）住这里。
   * 第一次要的时候才建；建出来就交给同一个 `ResizeObserver` 观察 ⇒ 它长高时照「粘底才贴底」那条规矩走，不另起一套。
   */
  get trailerElement(): HTMLElement {
    if (!this.trailerEl) {
      this.trailerEl = document.createElement("div");
      this.scrollEl.appendChild(this.trailerEl);
      this.resizeObserver.observe(this.trailerEl);
    }
    return this.trailerEl;
  }

  /** BranchFolder 要扫卡片所在的真实容器（.stream-content），不是外层滚动容器（否则 `:scope > .branch-fold-wrap` 找不到）。 */
  get contentElement(): HTMLElement {
    return this.contentEl;
  }
}
