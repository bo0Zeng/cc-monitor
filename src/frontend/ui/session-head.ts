/**
 * **会话头**（主区顶上 40px）：状态点 · 标题全名 · 「机器 · 目录」· 状态一句；右边：打开工作目录（本机）· 切到终端（Windows）· 看它的终端 ·
 * 在会话里找 · 更多（＝这个标签页的右键菜单）。按状态多一颗：已结束［恢复 ▾］· Claude 已退出（远端）［在终端里打开］·
 * 状态不明［重新连接］。窄时先藏目录、再藏状态一句（CSS 容器查询）。
 *
 * 「看它的终端」开底部抽屉的终端页（会话还有终端可去才出）。
 *
 * 只排版：一句话怎么写从 `session-face.ts` 取；做事的都交宿主（`TabManager` 那几条）。
 */
import type { Tab } from "./tab-model";
import { isRemoteOrigin } from "./ipc/origin";
import { canResume, hasTerminal } from "./tab-session-state";
import { terminalFrontAvailable } from "./terminal-front";
import { dispatcher, KeybindingDispatcher } from "./keybindings/registry";
import { copyText } from "./copy-table";
import { icon, type IconName } from "./kit/icon";
import { statusDot, setDot } from "./kit/status-dot";
import { attachTooltip } from "./kit/tooltip";
import { button } from "./kit/button";
import { dotLabel, dotOf, fullTitle, machineOf, stateLine } from "./session-face";
import s from "./session-head.module.css";

export interface SessionHeadHost {
  active(): Tab | null;
  /** 开底部抽屉的终端页（跟着当前标签页走）。 */
  viewTerminal(): void;
  openCwd(sid: string): void;
  front(sid: string): void;
  find(): void;
  more(anchor: HTMLElement, sid: string): void;
  resume(anchor: HTMLElement, sid: string): void;
  attach(sid: string): void;
  reconnect(origin: string): void;
}

/** 悬停「名字 · 当前键位」（按 `config.json.keybindings` 现拼）。 */
function keyed(name: string, action: Parameters<typeof dispatcher.effectiveChord>[0] | null): () => string {
  return () => {
    const chord = action === null ? null : dispatcher.effectiveChord(action);
    return chord ? `${name} · ${KeybindingDispatcher.prettyChord(chord)}` : name;
  };
}

export class SessionHead {
  readonly el: HTMLElement;
  private readonly dot: HTMLSpanElement;
  private readonly title: HTMLSpanElement;
  private readonly where: HTMLSpanElement;
  private readonly dir: HTMLSpanElement;
  private readonly state: HTMLSpanElement;
  private readonly extra: HTMLSpanElement;
  private readonly acts: HTMLSpanElement;
  private drawn: string | null = null;

  /** 宿主拿 `el` 顶掉 `index.html` 里那一格占位（`#session-head`，认领 `#app` 网格的 `head` 区）。 */
  constructor(private readonly host: SessionHeadHost) {
    this.el = document.createElement("div");
    this.el.id = "session-head";
    this.el.className = s.shHead;
    this.dot = statusDot("running", copyText("sessionFace.dot.running"), "compact");
    this.title = document.createElement("span");
    this.title.className = s.shTitle;
    attachTooltip(this.title, () => (this.title.scrollWidth > this.title.clientWidth ? (this.title.textContent ?? "") : null));
    this.where = document.createElement("span");
    this.where.className = s.shWhere;
    this.dir = document.createElement("span");
    this.dir.className = s.shDir;
    this.state = document.createElement("span");
    this.state.className = s.shState;
    const sp = document.createElement("span");
    sp.className = s.shSp;
    this.extra = document.createElement("span");
    this.extra.className = s.shExtra;
    this.acts = document.createElement("span");
    this.acts.className = s.shActs;
    this.el.append(this.dot, this.title, this.where, this.dir, this.state, sp, this.extra, this.acts);
    this.el.style.display = "none";
  }

  private iconButton(name: IconName, label: string, hint: () => string, onClick: (b: HTMLButtonElement) => void): HTMLButtonElement {
    const b = document.createElement("button");
    b.type = "button";
    b.className = s.shIb;
    b.setAttribute("aria-label", label);
    b.appendChild(icon(name));
    attachTooltip(b, hint);
    b.addEventListener("click", () => onClick(b));
    return b;
  }

  /** 当前 tab 变了 / 它的状态变了 ⇒ 重画（与上次画出去的一样就一个 DOM 都不写）。 */
  render(now: number = Date.now()): void {
    const tab = this.host.active();
    if (!tab) {
      if (this.drawn !== "") {
        this.drawn = "";
        this.el.style.display = "none";
      }
      return;
    }
    const d = dotOf(tab);
    const st = stateLine(tab, now);
    const remote = isRemoteOrigin(tab.origin);
    const front = terminalFrontAvailable() && hasTerminal(tab.state);
    // 看它的终端：会话还有终端可去才出（已结束的没有）。
    const term = hasTerminal(tab.state);
    const extra = canResume(tab.state) ? "resume" : d === "exited" && remote ? "attach" : d === "unknown" ? "reconnect" : "";
    const drawn = [tab.sessionId, d, fullTitle(tab), machineOf(tab), tab.projectDir ?? "", st.text, st.needs ? 1 : 0, remote ? 1 : 0, front ? 1 : 0, term ? 1 : 0, extra].join("\u0000");
    if (drawn === this.drawn) return;
    const sameSession = this.drawn?.split("\u0000")[0] === tab.sessionId;
    this.drawn = drawn;
    this.el.style.display = "";
    setDot(this.dot, d, dotLabel(d));
    this.title.textContent = fullTitle(tab);
    this.where.textContent = machineOf(tab);
    this.dir.textContent = tab.projectDir ?? "";
    this.dir.style.display = tab.projectDir === null ? "none" : "";
    this.state.textContent = st.text;
    this.state.className = st.needs ? `${s.shState} ${s.shNeed}` : s.shState;
    const sid = tab.sessionId;
    const extraBtn =
      extra === "resume"
        ? button({ label: copyText("sessionHead.act.resume"), size: "compact", icon: "caretDown", onClick: (e) => this.host.resume(e.currentTarget as HTMLElement, sid) })
        : extra === "attach"
          ? button({ label: copyText("sessionHead.act.openTerm"), size: "compact", onClick: () => this.host.attach(sid) })
          : extra === "reconnect"
            ? button({ label: copyText("sessionHead.act.reconnect"), size: "compact", onClick: () => this.host.reconnect(tab.origin) })
            : null;
    this.extra.replaceChildren(...(extraBtn ? [extraBtn] : []));
    // 右边一排随会话种类变（本机才有目录、Windows 才有 ↗）；同一个会话只在这两样变了时重建。
    const shape = `${remote ? 1 : 0}${front ? 1 : 0}${term ? 1 : 0}`;
    if (!sameSession || this.acts.dataset.shape !== shape) {
      this.acts.dataset.shape = shape;
      const btns: HTMLButtonElement[] = [];
      if (!remote && tab.projectDir) btns.push(this.iconButton("folder", copyText("tabBarView.tab.cwdHint"), keyed(copyText("tabBarView.tab.cwdHint"), "tab.open-cwd"), () => this.host.openCwd(sid)));
      if (front) {
        const b = this.iconButton("front", copyText("tabBarView.tab.terminalHint"), keyed(copyText("tabBarView.tab.terminalHint"), "terminal.bring-front"), () => this.host.front(sid));
        // ↗ 的结果浮层锚在这一颗上（按会话找）。
        b.dataset.role = "head-front";
        b.dataset.sid = sid;
        btns.push(b);
      }
      if (term) btns.push(this.iconButton("terminal", copyText("sessionHead.act.terminal"), keyed(copyText("sessionHead.act.terminal"), "panel.toggle-terminal"), () => this.host.viewTerminal()));
      btns.push(this.iconButton("search", copyText("sessionHead.act.find"), keyed(copyText("sessionHead.act.find"), "session.find"), () => this.host.find()));
      btns.push(this.iconButton("more", copyText("tabBarView.tab.moreHint"), () => copyText("tabBarView.tab.moreHint"), (b) => this.host.more(b, sid)));
      this.acts.replaceChildren(...btns);
    }
  }
}
