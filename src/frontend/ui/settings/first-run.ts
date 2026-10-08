/**
 * 机器页顶上「开始用」那一块（稿 03 · §4.8）：三步一行一行（圈 · 名字 · 一句 · 修法按钮），哪步做了由后端事实打勾（`first-run`）；
 * 三步都做完 / 点过「跳过」不出现（「跳过」交本机后端记下，跨重启还在）。构造零 I/O：宿主在机器页可见时调 [`loadNow`]。
 */
import { copyText } from "../copy-table";
import { button } from "../kit/button";
import { icon } from "../kit/icon";
import { LOCAL_ORIGIN } from "../ipc/origin";
import type { SettingsTarget } from "./open-settings";
import { readReadiness, skipStart, type Readiness, type ReadinessStep } from "./readiness-reads";
import { toast } from "../kit/toast";

export interface FirstRunHost {
  go(t: SettingsTarget): void;
  addMachine(): void;

}

export class FirstRun {
  readonly element: HTMLElement;
  private remotes: number | undefined;
  constructor(private readonly host: FirstRunHost) {
    this.element = document.createElement("section");
    this.element.className = "first-run";
    this.element.dataset.anchor = "first-run";
    this.element.hidden = true;
  }

  /** `remotes` ＝ 宿主手上那一份机器表的台数（没有 ⇒ 现读一次配置）。 */
  async loadNow(remotes?: number): Promise<void> {
    if (remotes !== undefined) this.remotes = remotes;
    this.paint(await readReadiness(this.remotes));
  }

  private paint(r: Readiness | null): void {
    this.element.hidden = r === null || r.skipped || r.steps.every((s) => s.done);
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
        void skipStart().then(
          () => this.loadNow(),
          (e: unknown) => toast(copyText("firstRun.head.skipFailed"), e instanceof Error ? e.message : String(e), { level: "error" }),
        );
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
