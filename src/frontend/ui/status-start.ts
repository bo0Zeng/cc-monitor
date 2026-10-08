/**
 * 状态栏「开始用 · 剩 N 步」：N ＝ 本机后端 `readiness` 的 `left`（设置窗机器页「开始用」那一块同一个数，`settings/readiness-reads.ts`）。
 * 0 / 问不到 / 那一块点过「跳过」⇒ 不渲染；点了打开设置窗直达机器页「开始用」那一块。什么时候问：起来时 · 主窗口拿回焦点时（`refresh`）。
 */
import { chip } from "./kit/chip";
import { copyText } from "./copy-table";

export interface StartDeps {
  left(): Promise<number | null>;
  skipped(): boolean;
  open(): void;
}

export class StatusStart {
  readonly el: HTMLElement;
  constructor(private readonly deps: StartDeps) {
    this.el = document.createElement("span");
    this.el.dataset.role = "status-start";
  }

  async refresh(): Promise<void> {
    const n = this.deps.skipped() ? null : await this.deps.left();
    if (n === null || n <= 0) {
      this.el.replaceChildren();
      return;
    }
    this.el.replaceChildren(chip({ text: copyText("statusBar.start.label", { n }), onClick: () => this.deps.open() }));
  }
}
