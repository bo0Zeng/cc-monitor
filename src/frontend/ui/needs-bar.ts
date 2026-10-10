/**
 * **「需手动」钉条**（消息流底部）：当前会话在等你时，一条说清等什么、等了多久、去哪答；答完（状态变了）就消失，不留痕。
 *
 * - 第一行 `等批准 · Bash` ＋ 那一步的主参数（等宽）/ `等回答` ＋ 问题 / `计划 · 等批准` / `需手动`（判不出，不猜）；
 *   第二行 `2m · 在终端里答`。
 * - 右边：Windows 有 ↗ ⇒ ［切到终端］；远端且在 tmux 里 ⇒ ［在终端里打开］；都不行 ⇒ 不给按钮（cc-monitor 只能看，不替你答）。
 *
 * 另两件「需手动」的事也住这里：窗口标题（`cc-monitor · 需手动 2`）· 系统通知（主窗口不在前台时、一个会话**开始**等你就发一条）。
 */
import type { Tab } from "./tab-model";
import type { Needs } from "./session-reads";
import { isRemoteOrigin } from "./ipc/origin";
import { hasTerminal } from "./tab-session-state";
import { terminalFrontAvailable } from "./terminal-front";
import { copyText } from "./copy-table";
import { button } from "./kit/button";
import { statusDot } from "./kit/status-dot";
import { machineOf, needsOf, fullTitle } from "./session-face";
import { waitedNow } from "./cards/step-line";
import s from "./needs-bar.module.css";

export interface NeedsBarHost {
  active(): Tab | null;
  front(sid: string): void;
  attach(sid: string): void;
}

/** 钉条第一行（种类 ＋ 工具名）与等宽那一段（主参数 / 问题）。 */
export function needsHeadline(n: Needs): { label: string; code: string | null } {
  switch (n.kind) {
    case "approve":
      return { label: n.tool ? copyText("needs.bar.approve", { tool: n.tool }) : n.text, code: n.what };
    case "answer":
      return { label: copyText("needs.bar.answer"), code: n.what };
    case "plan":
      return { label: copyText("needs.bar.plan"), code: null };
    case "network":
      return { label: copyText("needs.bar.network"), code: n.what };
    case "worker":
      return { label: copyText("needs.bar.worker"), code: null };
    case "goal":
      return { label: copyText("needs.bar.goal"), code: null };
    case "choose":
      return { label: copyText("needs.bar.choose"), code: null };
    case "unknown":
      return { label: copyText("needs.bar.unknown"), code: null };
  }
}

/** 去哪答：Windows 有 ↗ ⇒ 切到终端；远端在 tmux 里（活着的会话都在那台的某个终端里）⇒ 在终端里打开；都不行 ⇒ 不给。 */
export function answerWhere(tab: Tab): "front" | "attach" | null {
  if (terminalFrontAvailable() && hasTerminal(tab.state)) return "front";
  if (isRemoteOrigin(tab.origin) && tab.state.liveness === "live" && tab.state.recoverability === "attachable") return "attach";
  return null;
}

export class NeedsBar {
  readonly el: HTMLDivElement;
  private drawn: string | null = null;

  constructor(private readonly host: NeedsBarHost) {
    this.el = document.createElement("div");
    this.el.className = s.nbBar;
    this.el.setAttribute("role", "status");
    this.el.style.display = "none";
  }

  render(now: number = Date.now()): void {
    const tab = this.host.active();
    const n = tab ? needsOf(tab) : null;
    if (!tab || !n) {
      if (this.drawn !== "") {
        this.drawn = "";
        this.el.style.display = "none";
        this.el.replaceChildren();
      }
      return;
    }
    const head = needsHeadline(n);
    const waited = waitedNow(n, now);
    const where = answerWhere(tab);
    const drawn = [tab.sessionId, head.label, head.code ?? "", waited ?? "", where ?? ""].join("\u0000");
    if (drawn === this.drawn) return;
    this.drawn = drawn;
    this.el.style.display = "";
    const body = document.createElement("div");
    body.className = s.nbBody;
    const l1 = document.createElement("div");
    l1.className = s.nbLine;
    l1.append(document.createTextNode(head.label));
    if (head.code) {
      // 批准：那一步的主参数等宽；回答：问题原文照正文排。
      const asCode = n.kind === "approve" || n.kind === "network";
      const code = document.createElement(asCode ? "code" : "span");
      code.className = asCode ? s.nbCode : s.nbQuote;
      code.textContent = head.code;
      l1.append(code);
    }
    const l2 = document.createElement("div");
    l2.className = s.nbSub;
    l2.textContent = waited ? copyText("needs.bar.sub", { waited }) : copyText("needs.bar.subBare");
    body.append(l1, l2);
    const sid = tab.sessionId;
    const act =
      where === "front"
        ? button({ label: copyText("needs.bar.front"), icon: "front", size: "compact", onClick: () => this.host.front(sid) })
        : where === "attach"
          ? button({ label: copyText("needs.bar.openTerm"), size: "compact", onClick: () => this.host.attach(sid) })
          : null;
    this.el.replaceChildren(statusDot("needs-you", copyText("sessionFace.dot.needs"), "compact"), body, ...(act ? [act] : []));
  }
}

/**
 * 窗口标题与系统通知：每次标签页栏刷新时喂一次（`observe`）。
 * - 标题：`cc-monitor · 需手动 N`（0 个时只有 `cc-monitor`）；变了才写。
 * - 通知：一个会话**开始**等你（上一次喂的时候它不在等）、主窗口不在前台、设置里开着 ⇒ 发一条：标题 `{title}（{machine}）· 等批准`，正文是那一句。
 *   那一刻会话事实还没到（种类不明）⇒ 等它到了再发（不先发一条「需手动」再补一条）；到不了（老后端）⇒ 最多等 [`NOTIFY_WAIT_MS`] 照发。
 */
export const NOTIFY_WAIT_MS = 3000;

export interface NeedsWatchDeps {
  setTitle(title: string): void;
  isFocused(): boolean;
  enabled(): Promise<boolean>;
  send(title: string, body: string): Promise<void>;
  now(): number;
}

export class NeedsWatch {
  private title: string | null = null;
  /** 在等你的会话 → 这一次等待通知过没有 · 开始等的那一刻（本机时钟）。 */
  private readonly waiting = new Map<string, { notified: boolean; seenAt: number }>();

  constructor(private readonly deps: NeedsWatchDeps) {}

  observe(tabs: Iterable<Tab>): void {
    let n = 0;
    const now = this.deps.now();
    const still = new Set<string>();
    for (const t of tabs) {
      const need = needsOf(t);
      if (!need) continue;
      n++;
      still.add(t.sessionId);
      let w = this.waiting.get(t.sessionId);
      if (!w) {
        w = { notified: false, seenAt: now };
        this.waiting.set(t.sessionId, w);
      }
      if (!w.notified && (t.needs !== null || now - w.seenAt >= NOTIFY_WAIT_MS)) {
        w.notified = true;
        void this.notify(t, need);
      }
    }
    for (const sid of [...this.waiting.keys()]) if (!still.has(sid)) this.waiting.delete(sid);
    const title = n > 0 ? copyText("needs.windowTitle.count", { n }) : copyText("needs.windowTitle.base");
    if (title !== this.title) {
      this.title = title;
      this.deps.setTitle(title);
    }
  }

  private async notify(t: Tab, need: Needs): Promise<void> {
    if (this.deps.isFocused()) return;
    try {
      if (!(await this.deps.enabled())) return;
      await this.deps.send(copyText("needs.notify.title", { title: fullTitle(t), machine: machineOf(t), kind: need.text }), need.what ?? "");
    } catch (e) {
      console.warn("needs-notify: send failed:", e);
    }
  }
}
