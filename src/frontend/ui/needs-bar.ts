/**
 * 「需手动」的窗口标题（`cc-monitor · 需手动 2`）。消息流底部那一块（等什么、在这里就地答）住 `answer-block.ts`。
 */
import { copyText } from "./copy-table";

/**
 * 每次标签页栏刷新时喂一次（`observe`）：`cc-monitor · 需手动 N`（0 个时只有 `cc-monitor`）；
 * N 同侧栏入口（会话 ＋ 计划项 ＋ 会话外的每一项，`total`）；变了才写。
 */
export class NeedsTitle {
  private title: string | null = null;

  constructor(private readonly setTitle: (title: string) => void) {}

  observe(total: number): void {
    const title = total > 0 ? copyText("needs.windowTitle.count", { n: total }) : copyText("needs.windowTitle.base");
    if (title !== this.title) {
      this.title = title;
      this.setTitle(title);
    }
  }
}
