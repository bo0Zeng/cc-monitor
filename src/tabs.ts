import { MessageStream } from "./stream";
import { renderMessage, type JsonlRecord } from "./cards";

export type TabStatus = "live" | "idle" | "archived";

export interface Tab {
  sessionId: string;
  /** Tab 标题（M1: cwd 项目名最后一段，缺失时退化为 session_id 前 8 位） */
  title: string;
  cwd: string | null;
  status: TabStatus;
  streamEl: HTMLElement;
  stream: MessageStream;
  unread: number;
  isSubagent: boolean;
}

export class TabManager {
  private tabs = new Map<string, Tab>();
  private activeId: string | null = null;
  private locked = false;
  private lockTimer: number | null = null;

  constructor(
    private barEl: HTMLElement,
    private streamRootEl: HTMLElement,
  ) {}

  /** 收到一行 JSONL 时调用 */
  onLine(payload: {
    session_id: string;
    cwd: string | null;
    path: string;
    message: JsonlRecord;
  }): void {
    const tab = this.ensureTab(payload.session_id, payload.cwd, payload.path);
    const card = renderMessage(payload.message);
    if (card) tab.stream.append(card);

    if (this.activeId !== tab.sessionId) {
      tab.unread += 1;
      this.refreshTabBar();
    }
  }

  ensureTab(sessionId: string, cwd: string | null, sourcePath: string): Tab {
    let tab = this.tabs.get(sessionId);
    if (tab) {
      if (!tab.cwd && cwd) {
        tab.cwd = cwd;
        tab.title = projectNameFromCwd(cwd) ?? tab.title;
        this.refreshTabBar();
      }
      return tab;
    }

    const isSubagent = sourcePath.replace(/\\/g, "/").includes("/subagents/");
    const title =
      (cwd && projectNameFromCwd(cwd)) ??
      (isSubagent ? `↳ ${sessionId.slice(0, 8)}` : sessionId.slice(0, 8));

    const streamEl = document.createElement("div");
    streamEl.className = "stream";
    streamEl.style.display = "none";
    this.streamRootEl.appendChild(streamEl);

    tab = {
      sessionId,
      title,
      cwd,
      status: "live",
      streamEl,
      stream: new MessageStream(streamEl),
      unread: 0,
      isSubagent,
    };
    this.tabs.set(sessionId, tab);

    if (this.activeId === null) {
      this.switchTo(sessionId, { user: false });
    } else {
      this.refreshTabBar();
    }
    return tab;
  }

  switchTo(sessionId: string, options?: { user?: boolean }): void {
    if (!this.tabs.has(sessionId)) return;
    if (options?.user) this.lock();
    if (this.locked && !options?.user && this.activeId !== null) return;

    for (const [sid, t] of this.tabs) {
      t.streamEl.style.display = sid === sessionId ? "block" : "none";
    }
    const next = this.tabs.get(sessionId);
    if (next) {
      next.unread = 0;
      next.stream.scrollToBottom();
    }
    this.activeId = sessionId;
    this.refreshTabBar();
  }

  private lock(): void {
    this.locked = true;
    if (this.lockTimer !== null) window.clearTimeout(this.lockTimer);
    this.lockTimer = window.setTimeout(() => {
      this.locked = false;
    }, 5000);
  }

  private refreshTabBar(): void {
    this.barEl.innerHTML = "";
    for (const [sid, t] of this.tabs) {
      const btn = document.createElement("button");
      btn.className = "tab" + (sid === this.activeId ? " active" : "");
      if (t.isSubagent) btn.classList.add("subagent");

      const dot = document.createElement("span");
      dot.className = "live-dot";
      btn.appendChild(dot);

      const label = document.createElement("span");
      label.className = "tab-title";
      label.textContent = t.title;
      btn.appendChild(label);

      if (t.unread > 0 && sid !== this.activeId) {
        const badge = document.createElement("span");
        badge.className = "tab-badge";
        badge.textContent = t.unread > 99 ? "99+" : String(t.unread);
        btn.appendChild(badge);
      }

      btn.addEventListener("click", () =>
        this.switchTo(sid, { user: true }),
      );
      this.barEl.appendChild(btn);
    }
  }
}

function projectNameFromCwd(cwd: string): string | null {
  const normalized = cwd.replace(/\\/g, "/").replace(/\/+$/, "");
  const last = normalized.split("/").filter(Boolean).pop();
  return last ?? null;
}
