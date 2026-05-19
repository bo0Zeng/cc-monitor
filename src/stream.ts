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

  scrollToBottom(): void {
    this.el.scrollTop = this.el.scrollHeight;
  }

  clear(): void {
    this.el.innerHTML = "";
  }

  get element(): HTMLElement {
    return this.el;
  }
}
