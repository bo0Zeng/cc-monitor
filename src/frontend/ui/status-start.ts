/**
 * 状态栏「开始用 · 剩 N 步」：N ＝ 本机后端 `first-run` 的 `left`（只数必做的步；可选的只在设置窗那一块列着，不在这里催）。
 * 0 / 问不到 / 那一块点过「跳过」（后端记着）⇒ 不渲染；点了打开设置窗直达机器页「开始用」那一块。什么时候问：起来时 · 主窗口拿回焦点时（`refresh`）。
 */
import { chip } from "./kit/chip";
import { copyText } from "./copy-table";

export interface StartDeps {
  /** 那一份数：必做的还剩几步 · 跳过过没有；问不到 ⇒ `null`。 */
  read(): Promise<{ left: number; skipped: boolean } | null>;
  open(): void;
}

export class StatusStart {
  readonly el: HTMLElement;
  constructor(private readonly deps: StartDeps) {
    this.el = document.createElement("span");
    this.el.dataset.role = "status-start";
  }

  async refresh(): Promise<void> {
    const got = await this.deps.read();
    const n = got === null || got.skipped ? 0 : got.left;
    if (n <= 0) {
      this.el.replaceChildren();
      return;
    }
    this.el.replaceChildren(chip({ text: copyText("statusBar.start.label", { n }), onClick: () => this.deps.open() }));
  }
}
