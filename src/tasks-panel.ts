/**
 * Issue #11: Claude Code CLI 的 task 列表展示。
 *
 * 数据源：后端 `tasks.rs` 监听 `~/.claude/tasks/<sid>/`，emit `task-update`。
 * Tab 创建时 invoke `get_session_tasks` 拿初次快照。
 *
 * UI 形态（v2.3 调整后）：
 *  - **summary chip**：嵌入底部 status bar，显示「N tasks (X done, Y active, Z open)」+ ▶/▼
 *    点击切换 popover 折叠 / 展开。0 task 时灰显不可点。
 *  - **popover**：fixed 浮层贴在 status bar 上方往上展开（最高 50vh），含 task 完整列表
 *
 * 全局单例：TabManager 维护 `Map<sid, TaskEntry[]>`，切 Tab / 收到 update 都同步给单例。
 * 同 sid 只显示自己的 task，跨 Tab 不混淆。
 *
 * 折叠状态写 `localStorage cc-monitor.tasks-panel.collapsed`（全局单例无 per-Tab 必要）。
 *
 * 状态 icon：
 *   pending      □
 *   in_progress  ■
 *   completed    ✓
 *   deleted      ✗
 *   未知值       •
 */

import { invoke } from "@tauri-apps/api/core";
import { dispatcher } from "./keybindings/registry";

export interface TaskEntry {
  id: string;
  subject: string;
  description?: string;
  activeForm?: string;
  status: string;
  blocks: string[];
  blockedBy: string[];
}

const LS_KEY = "cc-monitor.tasks-panel.collapsed";

function loadCollapsed(): boolean {
  try {
    const v = localStorage.getItem(LS_KEY);
    if (v === "1") return true;
    if (v === "0") return false;
  } catch (e) {
    console.warn("[tasks-panel] localStorage read failed:", e);
  }
  return true;
}

function saveCollapsed(collapsed: boolean): void {
  try {
    localStorage.setItem(LS_KEY, collapsed ? "1" : "0");
  } catch (e) {
    console.warn("[tasks-panel] localStorage write failed:", e);
  }
}

export class TasksPanel {
  /** 挂到 status-bar 里当 chip。click 切换 collapsed。 */
  readonly summaryElement: HTMLButtonElement;
  /** 挂到 #app 里当 fixed popover，向上从 status bar 浮出。 */
  readonly popoverElement: HTMLElement;

  private summaryArrow: HTMLElement;
  private summaryText: HTMLElement;
  private list: HTMLUListElement;

  private tasks: TaskEntry[] = [];
  /** 当前显示的 session（来自 TabManager.activeId）。`null` = 无 active Tab。 */
  private activeSid: string | null = null;
  private collapsed: boolean;

  constructor() {
    this.collapsed = loadCollapsed();

    // === summary chip ===
    this.summaryElement = document.createElement("button");
    this.summaryElement.type = "button";
    this.summaryElement.className = "status-tasks";
    this.summaryElement.style.display = "none"; // 默认隐藏，0 task 时一直隐藏

    this.summaryArrow = document.createElement("span");
    this.summaryArrow.className = "status-tasks-arrow";
    this.summaryArrow.textContent = "▶";
    this.summaryElement.appendChild(this.summaryArrow);

    this.summaryText = document.createElement("span");
    this.summaryText.className = "status-tasks-text";
    this.summaryElement.appendChild(this.summaryText);

    this.summaryElement.addEventListener("click", () => this.toggleCollapsed());

    // === popover ===
    this.popoverElement = document.createElement("div");
    this.popoverElement.className = "tasks-popover";
    this.popoverElement.style.display = "none";

    const popHead = document.createElement("div");
    popHead.className = "tasks-popover-head";
    const popTitle = document.createElement("span");
    popTitle.className = "tasks-popover-title";
    popTitle.textContent = "任务列表";
    popHead.appendChild(popTitle);
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "tasks-popover-close";
    closeBtn.textContent = "×";
    closeBtn.title = "关闭";
    closeBtn.addEventListener("click", () => this.setCollapsed(true));
    popHead.appendChild(closeBtn);
    this.popoverElement.appendChild(popHead);

    this.list = document.createElement("ul");
    this.list.className = "tasks-popover-list";
    this.popoverElement.appendChild(this.list);

    this.applyCollapsedClass();

    // issue #5: Esc 通过 KeybindingDispatcher 统一调度。展开时 push、折叠时 pop，
    // 跟设置 / 历史浏览器共享同一个 overlay 栈（LIFO）。
    if (!this.collapsed && this.tasks.length > 0 && this.activeSid !== null) {
      dispatcher.pushOverlay(this);
    }
  }

  /** dispatcher overlay 接口 */
  handleEsc(): void {
    if (!this.collapsed && this.popoverElement.style.display !== "none") {
      this.setCollapsed(true);
    }
  }

  /**
   * 切换当前显示的 session（TabManager.switchTo 或新 Tab 创建时调）。
   * `null` 表示无 active Tab —— summary 隐藏。
   */
  setSession(sid: string | null, tasks: TaskEntry[]): void {
    this.activeSid = sid;
    this.tasks = tasks;
    this.render();
  }

  /**
   * 仅刷新当前 session 的 task 数据（用户在 CLI 里改了 task）。
   * 调用方应该已经判断过 sid 是 active，但 panel 内部再做一次保险确认。
   */
  refreshIfActive(sid: string, tasks: TaskEntry[]): void {
    if (this.activeSid !== sid) return;
    this.tasks = tasks;
    this.render();
  }

  /** 快捷键 (issue #5) 用：折叠 ↔ 展开切换 */
  toggle(): void {
    this.setCollapsed(!this.collapsed);
  }

  dispose(): void {
    dispatcher.popOverlay(this);
    this.summaryElement.remove();
    this.popoverElement.remove();
  }

  private render(): void {
    const total = this.tasks.length;
    if (total === 0 || this.activeSid === null) {
      this.summaryElement.style.display = "none";
      this.popoverElement.style.display = "none";
      return;
    }
    this.summaryElement.style.display = "";
    // popover 显示与否跟随折叠状态
    this.popoverElement.style.display = this.collapsed ? "none" : "";

    let completed = 0;
    let inProgress = 0;
    let pending = 0;
    let other = 0;
    for (const t of this.tasks) {
      switch (t.status) {
        case "completed":
          completed += 1;
          break;
        case "in_progress":
          inProgress += 1;
          break;
        case "pending":
          pending += 1;
          break;
        default:
          other += 1;
      }
    }
    const segments: string[] = [];
    if (completed > 0) segments.push(`${completed} done`);
    if (inProgress > 0) segments.push(`${inProgress} active`);
    if (pending > 0) segments.push(`${pending} open`);
    if (other > 0) segments.push(`${other} other`);
    this.summaryText.textContent =
      segments.length > 0
        ? `${total} tasks (${segments.join(", ")})`
        : `${total} tasks`;

    // 列表全量 replace —— task 数典型 < 30 条
    this.list.replaceChildren();
    for (const t of this.tasks) {
      const row = document.createElement("li");
      row.className = `tasks-popover-item status-${cssStatus(t.status)}`;
      row.setAttribute("data-task-id", t.id);

      const icon = document.createElement("span");
      icon.className = "tasks-popover-icon";
      icon.textContent = statusIcon(t.status);
      row.appendChild(icon);

      const subject = document.createElement("span");
      subject.className = "tasks-popover-subject";
      subject.textContent = t.subject;
      row.appendChild(subject);

      if (t.description || t.activeForm) {
        const parts: string[] = [];
        if (t.activeForm) parts.push(`▶ ${t.activeForm}`);
        if (t.description) parts.push(t.description);
        row.title = parts.join("\n\n");
      }

      this.list.appendChild(row);
    }
  }

  private toggleCollapsed(): void {
    this.setCollapsed(!this.collapsed);
  }

  private setCollapsed(next: boolean): void {
    if (next === this.collapsed) return;
    this.collapsed = next;
    saveCollapsed(next);
    this.applyCollapsedClass();
    // 0 task 时即使被 setCollapsed(false) 也不会 popoverElement 显示，
    // render() 会强制 display:none
    if (this.tasks.length > 0 && this.activeSid !== null) {
      this.popoverElement.style.display = this.collapsed ? "none" : "";
    }
    // issue #5: 同步 dispatcher overlay 栈 —— 展开进栈、折叠出栈
    if (this.collapsed) {
      dispatcher.popOverlay(this);
    } else if (this.tasks.length > 0 && this.activeSid !== null) {
      dispatcher.pushOverlay(this);
    }
  }

  private applyCollapsedClass(): void {
    this.summaryElement.classList.toggle("expanded", !this.collapsed);
    this.summaryArrow.textContent = this.collapsed ? "▶" : "▼";
    this.summaryElement.setAttribute(
      "aria-expanded",
      this.collapsed ? "false" : "true",
    );
  }
}

/** Tab 创建时拉一次初始 task 快照。失败返空数组（panel 自然隐藏）。 */
export async function fetchSessionTasks(
  sessionId: string,
): Promise<TaskEntry[]> {
  try {
    return await invoke<TaskEntry[]>("get_session_tasks", { sessionId });
  } catch (e) {
    console.warn(`[tasks-panel] fetch ${sessionId} failed:`, e);
    return [];
  }
}

function statusIcon(status: string): string {
  switch (status) {
    case "pending":
      return "□";
    case "in_progress":
      return "■";
    case "completed":
      return "✓";
    case "deleted":
      return "✗";
    default:
      return "•";
  }
}

function cssStatus(status: string): string {
  if (!/^[a-z_]+$/.test(status)) return "unknown";
  return status;
}
