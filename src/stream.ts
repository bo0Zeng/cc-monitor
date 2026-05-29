/**
 * 单个 Tab 的消息流容器。负责按位置插卡 + 自动贴底滚动。
 *
 * 设计：
 *   .stream          — 外层，absolute inset:0，overflow-y:auto（滚动容器）
 *     └ .stream-content — 内层包装，所有卡片挂在这里
 *
 * RecordTimeline 是唯一调用方：`insertNode` 按 seq 把卡插到正确位置，
 * `attachBatch` 在启动重放结束时把延后的"视口上方"旧内容一次性挂回。
 *
 * 用 ResizeObserver 观察 .stream-content 的尺寸变化以维持贴底：
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
 *   - 重放期「视口上方」的旧内容由 RecordTimeline 延后、经 `attachBatch` 一次性挂，
 *     把"逐帧上方插入"压成一帧，避免持续重排 + 重锚定的抖动。
 */
export class MessageStream {
  private scrollEl: HTMLElement;
  private contentEl: HTMLElement;
  /** 是否粘底（用户向上滚动后变 false） */
  private stickToBottom = true;
  private resizeObserver: ResizeObserver;
  private scrollHandler: () => void;
  private disposed = false;

  constructor(root: HTMLElement) {
    this.scrollEl = root;

    this.contentEl = document.createElement("div");
    this.contentEl.className = "stream-content";
    this.scrollEl.appendChild(this.contentEl);

    this.scrollHandler = () => {
      const distFromBottom =
        this.scrollEl.scrollHeight -
        this.scrollEl.scrollTop -
        this.scrollEl.clientHeight;
      this.stickToBottom = distFromBottom < 24;
    };
    this.scrollEl.addEventListener("scroll", this.scrollHandler);

    this.resizeObserver = new ResizeObserver(() => {
      if (this.stickToBottom) this.snap();
    });
    this.resizeObserver.observe(this.contentEl);
  }

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
    if (anchor) {
      this.contentEl.insertBefore(node, anchor);
    } else {
      this.contentEl.appendChild(node);
    }
    if (this.stickToBottom) {
      this.snap();
    }
  }

  /**
   * 批量挂载：把一段已渲染但未挂载的节点（DocumentFragment）一次性插到 anchor 前，
   * 然后只做一次 snap。给 RecordTimeline 的 flushDeferred 用 —— 重放期"视口上方"的
   * 旧内容延后到这里一次性插入，把"逐帧上方插入→每帧重排+重锚定±0.5px 抖"压成一帧。
   */
  attachBatch(fragment: DocumentFragment, anchor: HTMLElement | null): void {
    this.contentEl.insertBefore(fragment, anchor);
    if (this.stickToBottom) this.snap();
  }

  /** 强制贴底（Tab 切换时调用） */
  scrollToBottom(): void {
    this.stickToBottom = true;
    this.snap();
  }

  private snap(): void {
    const el = this.scrollEl;
    // 只在确实落后底部 >1px 时才贴底。内容持续在视口上方插入时，原生 overflow-anchor
    // 已把 scrollTop 维持在底部，这里就不再每帧 scrollTop=scrollHeight 重钉 —— 那会在
    // HiDPI 分数像素布局下因整数 scrollHeight 与分数布局的舍入误差每帧不同，造成整块
    // 内容 ±0.5px 高频重绘（"整行一起上下抖"的根因，已实测定位）。
    if (el.scrollHeight - el.clientHeight - el.scrollTop > 1) {
      el.scrollTop = el.scrollHeight;
    }
  }

  /**
   * issue #8：BranchFolder 需要扫卡片所在的真实容器（.stream-content），不是
   * 外层 scroll container；否则 querySelector :scope > .branch-fold-wrap 找不到。
   */
  get contentElement(): HTMLElement {
    return this.contentEl;
  }
}
