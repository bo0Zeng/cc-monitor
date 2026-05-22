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
  /**
   * tool_use_id → tool_use 折叠条 DOM。tool_result 直接注入对应 tool_use
   * 内部，不再产生独立折叠条。详见 cards/index.ts 的 injectOrBuildToolResult。
   */
  toolUseElements: Map<string, HTMLElement>;
}

/** Tab 数量摘要，发给宿主用于状态栏 / empty-state 等外部 UI */
export interface TabsSummary {
  total: number;
  live: number;
  archived: number;
}

/** TabButton 的 DOM 引用：refreshTabBar 局部更新依赖这些 ref 避免重新创建 button */
interface TabButtonRefs {
  root: HTMLButtonElement;
  label: HTMLSpanElement;
  badge: HTMLSpanElement;
}

export class TabManager {
  private tabs = new Map<string, Tab>();
  /** 按插入顺序的 sessionId 数组，与 this.tabs.keys() 顺序一致但避免每次 Array.from */
  private orderedIds: string[] = [];
  /** sessionId → button DOM refs，避免 refreshTabBar 每次重建整个 bar */
  private tabButtons = new Map<string, TabButtonRefs>();
  private activeId: string | null = null;
  // 早期版本曾用 user lock（用户主动点 Tab 后 5s 内拒 focus-switch）防自动焦点撞回去，
  // 实测 5s 太长导致用户切焦点窗口后 Tab 不跟着切（看上去焦点同步失效）。砍掉：用户
  // 切焦点窗口本身就是明确意图，应该尊重。

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
      toolUseElements: tab.toolUseElements,
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
      toolUseElements: new Map(),
    };
    this.tabs.set(sessionId, tab);
    this.orderedIds.push(sessionId);

    if (this.activeId === null) {
      this.switchTo(sessionId);
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
    const idx = this.orderedIds.indexOf(sessionId);
    // 优先切到后一个 Tab，否则前一个
    const fallbackId =
      this.orderedIds[idx + 1] ?? this.orderedIds[idx - 1] ?? null;

    tab.stream.dispose();
    tab.streamEl.remove();
    // 显式清 Map：释放对已卸载 DOM 节点的强引用，让 GC 可早回收
    // （Map 本身也会随 Tab 对象一起回收，但显式 clear 让 DOM 引用计数立即归零）
    tab.toolUseNames.clear();
    tab.toolUseElements.clear();
    tab.pendingToolGroup = null;
    this.tabs.delete(sessionId);
    if (idx >= 0) this.orderedIds.splice(idx, 1);

    // 让后端 event_replay 把这个 session 的历史也丢掉
    void invoke("forget_session", { sessionId }).catch((e) => {
      console.warn(`forget_session ${sessionId} failed:`, e);
    });

    if (wasActive) {
      if (fallbackId !== null) {
        this.switchTo(fallbackId);
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
    const ids = this.orderedIds;
    if (ids.length === 0) return;
    const idx = this.activeId ? ids.indexOf(this.activeId) : -1;
    const nextIdx = ((idx + delta) % ids.length + ids.length) % ids.length;
    const targetId = ids[nextIdx];
    if (targetId && targetId !== this.activeId) {
      this.switchTo(targetId);
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

  /** 快捷键 Ctrl+` ：把当前活跃 Tab 对应的终端窗口调到前台（仅 live） */
  bringActiveTerminalToFront(): void {
    if (!this.activeId) return;
    const tab = this.tabs.get(this.activeId);
    if (tab && tab.status !== "archived") {
      void bringTerminalToFront(this.activeId);
    }
  }

  switchTo(sessionId: string): void {
    if (!this.tabs.has(sessionId)) return;
    if (this.activeId === sessionId) return;

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

  /**
   * 局部更新策略（避免每次 onLine 都 replaceChildren）：
   *   1. 删除：tabButtons 缓存里有但 orderedIds 已没的 sid → 摘 DOM + 清缓存
   *   2. 创建：orderedIds 里有但缓存没的 sid → createTabButton 一次（含所有 5 个子
   *      元素 + 事件 listener），visibility 全交 CSS 控制
   *   3. 更新：updateTabButton 同步 active / archived / has-unread class + label/badge 文本
   *   4. 排序：iterate orderedIds + insertBefore，确保 DOM 顺序 = orderedIds 顺序
   *
   * CSS 配合（styles.css）：
   *   .tab.archived .live-dot/.tab-focus { display: none }
   *   .tab:not(.archived) .tab-close { display: none }
   *   .tab .tab-badge { display: none }
   *   .tab.has-unread:not(.active) .tab-badge { display: inline-block }
   */
  private refreshTabBar(): void {
    // 1. 删
    const wanted = new Set(this.orderedIds);
    for (const sid of Array.from(this.tabButtons.keys())) {
      if (!wanted.has(sid)) {
        const refs = this.tabButtons.get(sid)!;
        refs.root.remove();
        this.tabButtons.delete(sid);
      }
    }

    // 2 + 3 + 4. 创建 / 更新 / 排序
    let prev: ChildNode | null = null;
    for (const sid of this.orderedIds) {
      const tab = this.tabs.get(sid);
      if (!tab) continue;
      let refs = this.tabButtons.get(sid);
      if (!refs) {
        refs = this.createTabButton(sid);
        this.tabButtons.set(sid, refs);
      }
      this.updateTabButton(refs, sid, tab);
      // 排序：希望此 button 出现在 prev 之后
      const targetNext: ChildNode | null = prev
        ? prev.nextSibling
        : this.barEl.firstChild;
      if (refs.root !== targetNext) {
        this.barEl.insertBefore(refs.root, targetNext);
      }
      prev = refs.root;
    }

    this.notifyChanged();
  }

  private createTabButton(sid: string): TabButtonRefs {
    const root = document.createElement("button");
    root.className = "tab";

    const dot = document.createElement("span");
    dot.className = "live-dot";
    root.appendChild(dot);

    const label = document.createElement("span");
    label.className = "tab-title";
    root.appendChild(label);

    const badge = document.createElement("span");
    badge.className = "tab-badge";
    root.appendChild(badge);

    const focusBtn = document.createElement("span");
    focusBtn.className = "tab-focus";
    focusBtn.textContent = "↗";
    focusBtn.title = "调出对应终端 (Ctrl+`)";
    focusBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      void bringTerminalToFront(sid);
    });
    root.appendChild(focusBtn);

    const closeBtn = document.createElement("span");
    closeBtn.className = "tab-close";
    closeBtn.textContent = "×";
    closeBtn.title = "关闭 Tab";
    closeBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      this.closeTab(sid);
    });
    root.appendChild(closeBtn);

    root.addEventListener("click", () => this.switchTo(sid));
    // 中键点击归档 Tab 也关闭（常见 UX）
    root.addEventListener("mousedown", (e) => {
      if (e.button !== 1) return;
      const t = this.tabs.get(sid);
      if (t?.status === "archived") {
        e.preventDefault();
        this.closeTab(sid);
      }
    });

    return { root, label, badge };
  }

  private updateTabButton(refs: TabButtonRefs, sid: string, tab: Tab): void {
    refs.root.classList.toggle("active", sid === this.activeId);
    refs.root.classList.toggle("archived", tab.status === "archived");
    const unread = tab.unread > 0 && sid !== this.activeId;
    refs.root.classList.toggle("has-unread", unread);

    if (refs.label.textContent !== tab.title) {
      refs.label.textContent = tab.title;
    }
    if (unread) {
      const text = tab.unread > 99 ? "99+" : String(tab.unread);
      if (refs.badge.textContent !== text) {
        refs.badge.textContent = text;
      }
    }
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

function bringTerminalToFront(sessionId: string): Promise<void> {
  return invoke<void>("bring_terminal_to_front", { sessionId }).catch((e) => {
    console.warn(`bring_terminal_to_front ${sessionId} failed:`, e);
    showStatusError(String(e));
  });
}

/**
 * 后端 bring_terminal_to_front 失败时把详细错误抬到状态栏几秒；
 * 之前只 console.warn 用户看不见，"拉不起来"找不到原因。
 * 5 秒后让其他 status 更新自然覆盖（TabsSummary callback / 全局事件）。
 */
function showStatusError(msg: string): void {
  const statusMsg = document.querySelector<HTMLElement>("#status-bar .status-msg");
  if (!statusMsg) return;
  const truncated = msg.length > 240 ? msg.slice(0, 240) + "…" : msg;
  statusMsg.textContent = `⚠ ${truncated}`;
  statusMsg.title = msg;
  statusMsg.classList.add("status-error");
  window.setTimeout(() => {
    statusMsg.classList.remove("status-error");
    statusMsg.title = "";
  }, 8000);
}
