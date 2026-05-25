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

  /**
   * v2.3.1 (issue #1)：把 DocumentFragment 一次性 prepend 到 stream 顶部。
   * 专门给 replay 切块 prepend 用 —— 每个 older chunk 都贴到当前 contentEl 顶部，
   * 多 chunk 调用后 DOM 顺序自然是 [最老 chunk, 次老 chunk, ..., chunk 0 head]。
   *
   * **不依赖外部 anchor 节点**（之前 `prependBefore(fragment, anchor)` 模式踩坑：
   * BranchFolder rebuild / ensureTab 时机问题会让 anchor 脱离 contentEl 直接子节点，
   * insertBefore 抛 NotFoundError）。直接 `contentEl.firstChild` 当 anchor，
   * 它始终是 contentEl 的 child（或 null 时 insertBefore 自动 append）。
   *
   * **滚动位置保持**：
   * - 用户在底部（stickToBottom=true）：插入老内容后保持滚到底部
   * - 用户向上滚到老内容（stickToBottom=false）：补偿 scrollTop 让视觉位置不变
   */
  prependFragmentAtTop(fragment: DocumentFragment): void {
    if (fragment.childNodes.length === 0) return;

    const beforeHeight = this.contentEl.scrollHeight;
    const beforeScrollTop = this.scrollEl.scrollTop;

    // contentEl.firstChild 为 null 时 insertBefore 等价于 append —— 边界 safe
    this.contentEl.insertBefore(fragment, this.contentEl.firstChild);

    if (this.stickToBottom) {
      // 用户在底部 → 让浏览器自动 layout 后 snap 贴底
      this.snap();
    } else {
      // 用户向上看老内容 → 补偿 scrollTop 保持视觉位置不变
      const afterHeight = this.contentEl.scrollHeight;
      const delta = afterHeight - beforeHeight;
      if (delta > 0) {
        this.scrollEl.scrollTop = beforeScrollTop + delta;
      }
    }
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

  /**
   * issue #8：BranchFolder 需要扫卡片所在的真实容器（.stream-content），不是
   * 外层 scroll container；否则 querySelector :scope > .branch-fold-wrap 找不到。
   */
  get contentElement(): HTMLElement {
    return this.contentEl;
  }
}
