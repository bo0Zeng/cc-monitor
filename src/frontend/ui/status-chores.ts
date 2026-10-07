/**
 * 状态栏「要你动手 N」：N ＝ 各台 `data-report` 的 `chores` 相加，与设置窗「文件与数据」左栏角标同一个数
 * （读 `settings/data-reads.ts::choresOf`，问不到的那台不算、不当 0）。0 不渲染；点了打开设置窗直达「文件与数据」。
 * 什么时候问：起来时各台问一遍 · 主窗口拿回焦点时各台再问一遍（从设置窗处理完回来）（`refreshAll`）；`refresh` 只问一台。
 */
import { chip } from "./kit/chip";
import { copyText } from "./copy-table";

export interface ChoresDeps {
  machines(): Promise<string[]>;
  chores(origin: string): Promise<number | null>;
  open(): void;
}

export class StatusChores {
  /** 外包：有数时里面一枚 chip，没有时空着。 */
  readonly el: HTMLElement;
  private readonly counts = new Map<string, number>();

  constructor(private readonly deps: ChoresDeps) {
    this.el = document.createElement("span");
    this.el.dataset.role = "status-chores";
  }

  async refreshAll(): Promise<void> {
    let origins: string[];
    try {
      origins = await this.deps.machines();
    } catch {
      return;
    }
    for (const o of [...this.counts.keys()]) if (!origins.includes(o)) this.counts.delete(o);
    await Promise.all(origins.map((o) => this.refresh(o)));
  }

  async refresh(origin: string): Promise<void> {
    const n = await this.deps.chores(origin);
    if (n === null) this.counts.delete(origin);
    else this.counts.set(origin, n);
    this.paint();
  }

  private paint(): void {
    let total = 0;
    for (const n of this.counts.values()) total += n;
    if (total <= 0) {
      this.el.replaceChildren();
      return;
    }
    this.el.replaceChildren(chip({ text: copyText("statusBar.todo.label", { n: total }), tone: "warn", onClick: () => this.deps.open() }));
  }
}
