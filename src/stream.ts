/**
 * 单个 Tab 的消息流容器。负责 append + 自动贴底滚动。
 *
 * 设计：
 *   .stream          — 外层，absolute inset:0，overflow-y:auto（滚动容器）
 *     └ .stream-content — 内层包装，所有卡片追加到这里
 *
 * 用 ResizeObserver 观察 .stream-content 的尺寸变化：
 *   - 新卡片追加 → 子树长高 → 触发回调
 *   - tool 组追加新单元、collapsible 展开、Markdown 字体/图片后载 → 同样触发
 *   - Tab display:none→block 切回时也会触发（0×0 → 真实尺寸）
 *
 * 粘底状态（stickToBottom）由用户滚动决定：
 *   - 用户向上滚 → 解开粘底，再到底部 24px 内 → 恢复
 *   - 粘底时任何尺寸增长都自动贴底；不粘底时尺寸增长不打扰用户
 *
 * 旧实现用 requestAnimationFrame 兜底是硬编码，扛不过晚于 2 帧的布局变化
 * （图片加载、Web 字体），导致"新消息后滚到中间"的现象。新实现不再依赖 rAF。
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

  append(node: HTMLElement): void {
    this.contentEl.appendChild(node);
    // ResizeObserver 回调下一帧才到，先同步贴一下避免视觉跳动
    if (this.stickToBottom) this.snap();
  }

  /** 强制贴底（Tab 切换时调用） */
  scrollToBottom(): void {
    this.stickToBottom = true;
    this.snap();
  }

  clear(): void {
    this.contentEl.replaceChildren();
  }

  private snap(): void {
    this.scrollEl.scrollTop = this.scrollEl.scrollHeight;
  }

  get element(): HTMLElement {
    return this.scrollEl;
  }
}
