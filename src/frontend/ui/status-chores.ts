/**
 * 状态栏「待办 N」：N ＝ 本机后端 `chores-merge` 问到各台的 `data-report`、件数合成那一份的 `badge`，与设置窗「文件与数据」左栏角标同一个数
 * （读 `settings/data-reads.ts::readChores`；没问到的那台后端不计）。0 不渲染；点了打开设置窗直达「文件与数据」。
 * 什么时候问：起来时 · 哪台接上时 · 主窗口拿回焦点时（从设置窗处理完回来）各问一遍（`refreshAll`）。
 */
import { chip } from "./kit/chip";
import { copyText } from "./copy-table";
import type { ChoreCounts } from "./generated/ChoreCounts";

export interface ChoresDeps {
  machines(): Promise<string[]>;
  /** 本机后端问到这几台、合一份；只读它的 `badge`。 */
  read(machines: string[]): Promise<{ total: ChoreCounts }>;
  open(): void;
}

export class StatusChores {
  /** 外包：有数时里面一枚 chip，没有时空着。 */
  readonly el: HTMLElement;
  /** 只画最后一问的那一份（先发的晚回来不盖后发的）。 */
  private gen = 0;

  constructor(private readonly deps: ChoresDeps) {
    this.el = document.createElement("span");
    this.el.dataset.role = "status-chores";
  }

  async refreshAll(): Promise<void> {
    const gen = ++this.gen;
    let badge = 0;
    try {
      badge = (await this.deps.read(await this.deps.machines())).total.badge;
    } catch {
      // 机器表 · 本机后端问不到 ⇒ 不出数（不猜）。
    }
    if (gen !== this.gen) return;
    if (badge <= 0) {
      this.el.replaceChildren();
      return;
    }
    this.el.replaceChildren(chip({ text: copyText("statusBar.todo.label", { n: badge }), tone: "warn", onClick: () => this.deps.open() }));
  }
}
