import { invoke } from "@tauri-apps/api/core";
import { MessageStream } from "./stream";
import {
  renderMessage,
  buildToolGroup,
  addToToolGroup,
  type JsonlRecord,
  type ToolGroup,
  type RenderContext,
} from "./cards";

/**
 * Tab 生命周期：
 * - `live`：session 进程还在跑（`~/.claude/sessions/<PID>.json` 存在且 PID 探活通过）
 * - `archived`：session 进程退出，Tab 灰显但保留内容；用户可主动关
 *
 * 历史：设计文档原本规划过 `idle`（5min 无消息变灰），但实际未落地，
 * 移除以免误用。
 */
export type TabStatus = "live" | "archived";

export interface Tab {
  sessionId: string;
  /**
   * Tab 标题。优先级：[项目] aiTitle > 项目名 > session_id 前 8 位。
   * aiTitle 一旦出现就锁住，后续 cwd 不再回退。
   */
  title: string;
  cwd: string | null;
  /** Claude 给出的语义标题（JSONL 里 `ai-title` 记录的 aiTitle 字段），出现一次就锁定 */
  aiTitle: string | null;
  status: TabStatus;
  streamEl: HTMLElement;
  stream: MessageStream;
  /** 父 JSONL 路径（subagent 加载需要） */
  parentPath: string;
  unread: number;
  /** 当前正在累积的工具组（连续 tool-only assistant 消息），出现普通卡时清空 */
  pendingToolGroup: ToolGroup | null;
  /**
   * tool_use_id → tool_name 缓存。tool_use 在 assistant 消息出现时记下，
   * 下一条 user 消息的 tool_result 反查显示工具名。
   */
  toolUseNames: Map<string, string>;
}

/** Tab 数量摘要，发给宿主用于状态栏 / empty-state 等外部 UI */
export interface TabsSummary {
  total: number;
  live: number;
  archived: number;
}

export class TabManager {
  private tabs = new Map<string, Tab>();
  private activeId: string | null = null;
  private locked = false;
  private lockTimer: number | null = null;

  constructor(
    private barEl: HTMLElement,
    private streamRootEl: HTMLElement,
    /** 任何 Tab 增/减/状态变化后回调；宿主用它驱动状态栏等外部 UI */
    private onTabsChanged?: (summary: TabsSummary) => void,
  ) {}

  private notifyChanged(): void {
    if (!this.onTabsChanged) return;
    let live = 0;
    let archived = 0;
    for (const t of this.tabs.values()) {
      if (t.status === "archived") archived += 1;
      else live += 1;
    }
    this.onTabsChanged({ total: this.tabs.size, live, archived });
  }

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

    const ctx: RenderContext = {
      parentPath: tab.parentPath,
      toolUseNames: tab.toolUseNames,
    };
    const result = renderMessage(payload.message, ctx);

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
        tab.title = this.computeTitle(tab);
        this.refreshTabBar();
      }
      return tab;
    }

    const title = computeTitleFor(sessionId, cwd, null);

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
      parentPath: sourcePath,
      unread: 0,
      pendingToolGroup: null,
      toolUseNames: new Map(),
    };
    this.tabs.set(sessionId, tab);

    if (this.activeId === null) {
      this.switchTo(sessionId, { user: false });
    } else {
      this.refreshTabBar();
    }
    return tab;
  }

  /** 应用 ai-title：锁定语义标题，并按 [项目名] aiTitle 格式更新 Tab 标题 */
  private applyAiTitle(tab: Tab, aiTitle: string): void {
    const trimmed = aiTitle.trim();
    if (!trimmed) return;
    if (tab.aiTitle === trimmed) return;
    tab.aiTitle = trimmed;
    tab.title = this.computeTitle(tab);
    this.refreshTabBar();
  }

  /** 根据 tab.cwd + tab.aiTitle + sessionId 算出展示标题 */
  private computeTitle(tab: Tab): string {
    return computeTitleFor(tab.sessionId, tab.cwd, tab.aiTitle);
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

  /**
   * 关闭 Tab：销毁 stream DOM、从 Map 中移除、通知后端 forget 历史、必要时切到相邻 Tab。
   * 仅允许关闭 archived 状态的 Tab，避免误关运行中的会话。
   * forget 后该 session 不会在下次 F5 刷新时被 event_replay 重放复活。
   */
  closeTab(sessionId: string): void {
    const tab = this.tabs.get(sessionId);
    if (!tab) return;
    if (tab.status !== "archived") return;

    const wasActive = this.activeId === sessionId;
    const orderedIds = Array.from(this.tabs.keys());
    const idx = orderedIds.indexOf(sessionId);
    // 优先切到后一个 Tab，否则前一个
    const fallbackId = orderedIds[idx + 1] ?? orderedIds[idx - 1] ?? null;

    tab.stream.dispose();
    tab.streamEl.remove();
    this.tabs.delete(sessionId);

    // 让后端 event_replay 把这个 session 的历史也丢掉
    void invoke("forget_session", { sessionId }).catch((e) => {
      console.warn(`forget_session ${sessionId} failed:`, e);
    });

    if (wasActive) {
      if (fallbackId !== null) {
        this.switchTo(fallbackId, { user: true });
      } else {
        this.activeId = null;
        this.refreshTabBar();
      }
    } else {
      this.refreshTabBar();
    }
  }

  /**
   * 切到上 / 下一个 Tab。delta=+1 下一个、-1 上一个。环回。
   * 快捷键 Ctrl+Tab / Ctrl+Shift+Tab 用。
   */
  cycleActive(delta: 1 | -1): void {
    const ids = Array.from(this.tabs.keys());
    if (ids.length === 0) return;
    const idx = this.activeId ? ids.indexOf(this.activeId) : -1;
    const nextIdx = ((idx + delta) % ids.length + ids.length) % ids.length;
    const targetId = ids[nextIdx];
    if (targetId && targetId !== this.activeId) {
      this.switchTo(targetId, { user: true });
    }
  }

  /** 快捷键 Ctrl+W：当前活跃 Tab 是 archived 才关，live 不动 */
  closeActiveIfArchived(): void {
    if (!this.activeId) return;
    const tab = this.tabs.get(this.activeId);
    if (tab && tab.status === "archived") {
      this.closeTab(this.activeId);
    }
  }

  switchTo(sessionId: string, options?: { user?: boolean }): void {
    if (!this.tabs.has(sessionId)) {
      if (!options?.user) {
        console.info(`[focus] switchTo ${sessionId} ignored: tab not present`);
      }
      return;
    }
    if (options?.user) this.lock();
    if (this.locked && !options?.user && this.activeId !== null) {
      console.info(`[focus] switchTo ${sessionId} blocked by user lock`);
      return;
    }

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
    this.barEl.replaceChildren();
    for (const [sid, t] of this.tabs) {
      const btn = document.createElement("button");
      btn.className = "tab" + (sid === this.activeId ? " active" : "");
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

      // 归档 Tab 才显示关闭按钮 —— 防止误关运行中的会话
      if (t.status === "archived") {
        const close = document.createElement("span");
        close.className = "tab-close";
        close.textContent = "×";
        close.title = "关闭 Tab";
        close.addEventListener("click", (e) => {
          e.stopPropagation();
          this.closeTab(sid);
        });
        btn.appendChild(close);
      }

      btn.addEventListener("click", () =>
        this.switchTo(sid, { user: true }),
      );
      // 中键点击归档 Tab 也关闭（常见 UX）
      btn.addEventListener("mousedown", (e) => {
        if (e.button === 1 && t.status === "archived") {
          e.preventDefault();
          this.closeTab(sid);
        }
      });
      this.barEl.appendChild(btn);
    }
    this.notifyChanged();
  }
}

function projectNameFromCwd(cwd: string): string | null {
  const normalized = cwd.replace(/\\/g, "/").replace(/\/+$/, "");
  const last = normalized.split("/").filter(Boolean).pop();
  return last ?? null;
}

/**
 * 标题格式（决策见 project_monitor_decisions.md）：
 *   aiTitle 有 + cwd 有 → `[项目] aiTitle`
 *   aiTitle 有 + cwd 无 → `aiTitle`
 *   aiTitle 无 + cwd 有 → `项目`
 *   都没有 → `<sid 前 8 位>`
 *
 * Subagent 不再独立 Tab（嵌入到父 session 的 Task 折叠卡），所以没有 `↳` 前缀分支。
 */
function computeTitleFor(
  sessionId: string,
  cwd: string | null,
  aiTitle: string | null,
): string {
  const project = cwd ? projectNameFromCwd(cwd) : null;
  if (aiTitle) {
    return project ? `[${project}] ${aiTitle}` : aiTitle;
  }
  if (project) return project;
  return sessionId.slice(0, 8);
}
