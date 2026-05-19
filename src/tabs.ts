import { MessageStream } from "./stream";
import {
  renderMessage,
  buildToolGroup,
  addToToolGroup,
  type JsonlRecord,
  type ToolGroup,
} from "./cards";

export type TabStatus = "live" | "idle" | "archived";

export interface Tab {
  sessionId: string;
  /**
   * Tab 标题。优先级：aiTitle > cwd 项目名 > session_id 前 8 位。
   * aiTitle 一旦出现就锁住，后续 cwd 不再覆盖。
   */
  title: string;
  cwd: string | null;
  /** Claude 给出的语义标题（JSONL 里 `ai-title` 记录的 aiTitle 字段），出现一次就锁定 */
  aiTitle: string | null;
  status: TabStatus;
  streamEl: HTMLElement;
  stream: MessageStream;
  unread: number;
  isSubagent: boolean;
  /** 当前正在累积的工具组（连续 tool-only assistant 消息），出现普通卡时清空 */
  pendingToolGroup: ToolGroup | null;
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

    // ai-title 不进入消息流，只更新 Tab 标题
    if (payload.message.type === "ai-title") {
      this.applyAiTitle(tab, payload.message.aiTitle);
      return;
    }

    const result = renderMessage(payload.message);

    switch (result.kind) {
      case "skip":
        return;
      case "card":
        // 普通卡（user / 含 text 的 assistant）出现就断开工具组累积
        tab.pendingToolGroup = null;
        tab.stream.append(result.element);
        break;
      case "tool-group": {
        if (tab.pendingToolGroup) {
          // 追加到当前组
          addToToolGroup(tab.pendingToolGroup, result.units);
        } else {
          // 新建组卡片并 append 到 stream
          const group = buildToolGroup(result.timestamp);
          addToToolGroup(group, result.units);
          tab.pendingToolGroup = group;
          tab.stream.append(group.root);
        }
        break;
      }
    }

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
        // aiTitle 已锁定时不再回退到 cwd
        if (tab.aiTitle === null) {
          tab.title = projectNameFromCwd(cwd) ?? tab.title;
          this.refreshTabBar();
        }
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
      aiTitle: null,
      status: "live",
      streamEl,
      stream: new MessageStream(streamEl),
      unread: 0,
      isSubagent,
      pendingToolGroup: null,
    };
    this.tabs.set(sessionId, tab);

    if (this.activeId === null) {
      this.switchTo(sessionId, { user: false });
    } else {
      this.refreshTabBar();
    }
    return tab;
  }

  /** 应用 ai-title：锁定 Tab 标题，后续 cwd 变化不再回退 */
  private applyAiTitle(tab: Tab, aiTitle: string): void {
    const trimmed = aiTitle.trim();
    if (!trimmed) return;
    if (tab.aiTitle === trimmed) return;
    tab.aiTitle = trimmed;
    tab.title = trimmed;
    this.refreshTabBar();
  }

  /** session 退出（~/.claude/sessions/<PID>.json 被删）—— 灰显归档，内容保留 */
  archiveTab(sessionId: string): void {
    const tab = this.tabs.get(sessionId);
    if (!tab) return;
    if (tab.status === "archived") return;
    tab.status = "archived";
    tab.pendingToolGroup = null;
    this.refreshTabBar();
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
      if (t.status === "archived") btn.classList.add("archived");

      if (t.status !== "archived") {
        const dot = document.createElement("span");
        dot.className = "live-dot";
        btn.appendChild(dot);
      }

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
