/**
 * 机器页顶上「开始用」那一块（稿 03 · §4.8）：三步一行一行（圈 · 名字 · 一句 · 修法按钮），哪步做了由后端事实打勾（`readiness`）；
 * 全做完不出现；「跳过」收起、记住。构造零 I/O：宿主在机器页可见时调 [`loadNow`]。
 */
import { copyText } from "../copy-table";
import { button } from "../kit/button";
import { icon } from "../kit/icon";
import { LOCAL_ORIGIN } from "../ipc/origin";
import type { SettingsTarget } from "./open-settings";
import { firstRunSkipped, readReadiness, skipFirstRun, type Readiness, type ReadinessStep } from "./readiness-reads";

export interface FirstRunHost {
  go(t: SettingsTarget): void;
  addMachine(): void;
  /** 那份数变了（主窗口状态栏那一枚跟着）。 */
  onLeft?(left: number | null): void;
}

export class FirstRun {
  readonly element: HTMLElement;
  constructor(private readonly host: FirstRunHost) {
    this.element = document.createElement("section");
    this.element.className = "first-run";
    this.element.dataset.anchor = "first-run";
    this.element.hidden = true;
  }

  async loadNow(): Promise<void> {
    const got = await readReadiness();
    this.paint(got);
    this.host.onLeft?.(got?.left ?? null);
  }

  private paint(r: Readiness | null): void {
    this.element.hidden = r === null || r.left === 0 || firstRunSkipped();
    if (this.element.hidden || r === null) {
      this.element.replaceChildren();
      return;
    }
    const head = document.createElement("div");
    head.className = "first-run-head";
    const title = document.createElement("span");
    title.className = "first-run-title";
    title.textContent = copyText("firstRun.head.title");
    const sub = document.createElement("span");
    sub.className = "settings-hint";
    sub.textContent = copyText("firstRun.head.sub", { n: r.steps.length });
    const skip = button({
      label: copyText("firstRun.head.skip"),
      kind: "ghost",
      size: "compact",
      onClick: () => {
        skipFirstRun();
        this.paint(r);
        this.host.onLeft?.(null);
      },
    });
    skip.dataset.action = "skip";
    head.append(title, sub, skip);
    const rows = r.steps.map((s) => this.row(s.id, s.done));
    this.element.replaceChildren(head, ...rows);
  }

  private row(id: ReadinessStep, done: boolean): HTMLElement {
    const row = document.createElement("div");
    row.className = "first-run-row";
    row.dataset.step = id;
    row.dataset.done = String(done);
    const mark = document.createElement("span");
    mark.className = "first-run-mark";
    if (done) mark.appendChild(icon("check", "compact"));
    const text = document.createElement("div");
    text.className = "first-run-text";
    const name = document.createElement("div");
    name.className = "first-run-name";
    const hint = document.createElement("div");
    hint.className = "settings-hint";
    let act: { label: string; run: () => void; primary: boolean };
    if (id === "terminal") {
      name.textContent = copyText("firstRun.terminal.name");
      hint.textContent = copyText("firstRun.terminal.hint");
      act = { label: copyText("firstRun.terminal.action"), run: () => this.host.go({ machine: LOCAL_ORIGIN, tab: "config", anchor: "connect-terminal" }), primary: true };
    } else if (id === "named") {
      name.textContent = copyText("firstRun.named.name");
      hint.textContent = copyText("firstRun.named.hint");
      act = { label: copyText("firstRun.named.action"), run: () => this.host.go({ machine: LOCAL_ORIGIN, tab: "acct" }), primary: false };
    } else {
      name.textContent = copyText("firstRun.remote.name");
      hint.textContent = copyText("firstRun.remote.hint");
      act = { label: copyText("firstRun.remote.action"), run: () => this.host.addMachine(), primary: false };
    }
    text.append(name, hint);
    row.append(mark, text);
    if (!done) {
      const b = button({ label: act.label, kind: act.primary ? "primary" : "secondary", size: "compact", onClick: act.run });
      b.dataset.action = id;
      row.appendChild(b);
    }
    return row;
  }
}
