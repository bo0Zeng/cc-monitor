/**
 * 窗口标题（`cc-monitor · 待答请求 2 · 需手动 1`）：整句由核心写好（`NeedsMerged.windowTitle`），这里一个字不拼。消息流底部那一块（等什么、在这里就地答）住 `answer-block.ts`。
 */

/**
 * 每次标签页栏刷新时喂一次（`observe`）：照抄核心合好的那一句（非零那几列各带数、待答请求在前；都 0 时只有名字）；变了才写，还没合到（`null`）不写。
 */
export class NeedsTitle {
  private title: string | null = null;

  constructor(private readonly setTitle: (title: string) => void) {}

  observe(title: string | null): void {
    // 第一份还没合到 ⇒ 不写（窗口照页面自带的标题）。
    if (title !== null && title !== this.title) {
      this.title = title;
      this.setTitle(title);
    }
  }
}
