/**
 * **机器条**（A2-17-6-1）：钉在标签页栏底、往上长（标签页不跳位），一行一台、只列没连上的那几台。
 * 行上照那台的状态成品摆（`MachineState`：后端 `deploy::machine` 判的态 · 名字旁那个词 · 那一句 · 修法），界面不按 SSH 层自推、不去抖；
 * 连上那一下那一行当场摘掉，不弹提示。多于 3 台收成一行「未连上（n）」，点开逐台列。窄栏只剩点（悬停是那一句）。
 */
import type { MachineState } from "./generated/MachineState";
import type { MachineStateKind } from "./generated/MachineStateKind";
import type { MachineFix } from "./generated/MachineFix";
import { machineDot } from "./machine-dot";
import { statusDot } from "./kit/status-dot";
import { attachTooltip } from "./kit/tooltip";
import { copyText } from "./copy-table";

/** 修法交给谁做（重新连接 · 连接设置 · 更新 …）。 */
export interface StripHost {
  fix(origin: string, fix: MachineFix): void;
}

/** 不进条的几态：连着的 · 比这一版新的（也连着）· 用户停用的（不是没连上，是不连）· 还不知道的。 */
const NOT_LISTED: ReadonlySet<MachineStateKind> = new Set(["up", "newer", "disabled", "unknown"]);

/** 多于这几台收成一行。 */
const MAX_ROWS = 3;

export class MachineStrip {
  readonly el: HTMLDivElement;
  private readonly machines = new Map<string, MachineState>();
  /** 收起那一行点开过。 */
  private open = false;

  constructor(private readonly host: StripHost) {
    this.el = document.createElement("div");
    this.el.className = "machine-strip";
  }

  /** 一台的成品来了：没连上 ⇒ 记下、画；连上了 ⇒ 摘掉。 */
  set(origin: string, m: MachineState): void {
    if (NOT_LISTED.has(m.state)) this.machines.delete(origin);
    else this.machines.set(origin, m);
    this.paint();
  }

  private paint(): void {
    const list = [...this.machines];
    if (list.length > MAX_ROWS && !this.open) {
      const more = document.createElement("button");
      more.type = "button";
      more.className = "machine-strip-more";
      const text = document.createElement("span");
      text.className = "machine-strip-more-text";
      text.textContent = copyText("tabBar.strip.more", { n: list.length });
      more.append(statusDot("failed", null, "compact"), text);
      attachTooltip(more, () => text.textContent ?? "");
      more.addEventListener("click", () => {
        this.open = true;
        this.paint();
      });
      this.el.replaceChildren(more);
      return;
    }
    if (list.length <= MAX_ROWS) this.open = false;
    this.el.replaceChildren(...list.map(([origin, m]) => this.row(origin, m)));
  }

  private row(origin: string, m: MachineState): HTMLElement {
    const row = document.createElement("div");
    row.className = "machine-strip-row";
    row.dataset.machine = origin;
    const dot = statusDot(machineDot(m.state), m.word, "compact");
    attachTooltip(dot, () => m.said);
    const name = document.createElement("span");
    name.className = "machine-strip-name one-line";
    name.textContent = m.name;
    const word = document.createElement("span");
    word.className = "machine-strip-word";
    word.textContent = m.word;
    const said = document.createElement("span");
    said.className = "machine-strip-said one-line";
    said.textContent = m.said;
    const body = document.createElement("span");
    body.className = "machine-strip-body";
    const head = document.createElement("span");
    head.className = "machine-strip-head";
    head.append(name, word);
    body.append(head, said);
    row.append(dot, body);
    const first = m.fixes[0];
    if (first) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "machine-strip-fix";
      b.dataset.fix = first.fix;
      b.textContent = first.label;
      b.addEventListener("click", () => this.host.fix(origin, first.fix));
      row.appendChild(b);
    }
    return row;
  }
}
