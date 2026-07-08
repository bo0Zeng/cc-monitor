/**
 * 单个 Tab 的消息流容器。负责按位置插卡 + 自动贴底滚动。
 *
 * 设计：
 *   .stream          — 外层，absolute inset:0，overflow-y:auto（滚动容器）
 *     └ .stream-content — 内层包装，所有卡片挂在这里
 *
 * RecordTimeline 是唯一调用方：`insertNode` 按 seq 把卡插到正确位置。
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
 *   - 重放期「视口上方」的旧内容根本不建 DOM（Batch13-F40a 尾部优先收纳,
 *     TailWindow 账本）——"逐帧上方插入"从源头消失（INVARIANTS § 21.3）。
 */
export class MessageStream {
  private scrollEl: HTMLElement;
  private contentEl: HTMLElement;
  /** 是否粘底（用户向上滚动后变 false） */
  private stickToBottom = true;
  /** F40a S-7:物化大批插卡期间暂停逐卡守卫 snap(见 batchInsert) */
  private snapSuspended = false;
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
    // F40a D 审计 R-2 防御:anchor 可能已被折进 .branch-fold-wrap(增量渲染的洞场景
    // /F40b 补批),直接 insertBefore 会 NotFoundError → 该记录被 drain 的 try/catch
    // 吞掉永久丢失。爬到 contentEl 直接子层再插——卡粒度顺序仍正确(落在包含
    // anchor 的 wrap 之前),折叠归属由下一次 rebuild 自愈;anchor 已不在 DOM 则
    // 降级末尾追加(保数据,顺序由 seq 账本兜底)。
    if (anchor && anchor.parentElement !== this.contentEl) {
      let a: HTMLElement | null = anchor;
      while (a && a.parentElement !== this.contentEl) {
        a = a.parentElement;
      }
      anchor = a;
    }
    if (anchor) {
      this.contentEl.insertBefore(node, anchor);
    } else {
      this.contentEl.appendChild(node);
    }
    if (this.stickToBottom && !this.snapSuspended) {
      this.snap();
    }
  }

  /**
   * F40a S-7:物化/补批大批插卡——期间暂停逐卡守卫 snap(每次 snap 读 scrollHeight
   * 都是一次强制 reflow,150 卡 = 150 次),批末按粘底状态一次贴底。
   */
  batchInsert(fn: () => void): void {
    this.snapSuspended = true;
    try {
      fn();
    } finally {
      this.snapSuspended = false;
    }
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
