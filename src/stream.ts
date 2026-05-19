/**
 * 单个 Tab 的消息流容器。负责 append + 贴底滚动。
 * 虚拟滚动留到 M6。
 */
export class MessageStream {
  private el: HTMLElement;
  /** 是否粘底（用户向上滚动后变 false） */
  private stickToBottom = true;

  constructor(root: HTMLElement) {
    this.el = root;
    this.el.addEventListener("scroll", () => {
      const distFromBottom =
        this.el.scrollHeight - this.el.scrollTop - this.el.clientHeight;
      this.stickToBottom = distFromBottom < 24;
    });
  }

  append(node: HTMLElement): void {
    this.el.appendChild(node);
    if (this.stickToBottom) {
      this.scrollToBottom();
    }
  }

  /** rAF 包装：display 切 none→block 那帧 scrollHeight 还没算出来 */
  scrollToBottom(): void {
    requestAnimationFrame(() => {
      this.el.scrollTop = this.el.scrollHeight;
      // 内嵌大图/字体加载完后高度还会变；再补一帧以兜底
      requestAnimationFrame(() => {
        this.el.scrollTop = this.el.scrollHeight;
      });
    });
  }

  clear(): void {
    this.el.innerHTML = "";
  }

  get element(): HTMLElement {
    return this.el;
  }
}
