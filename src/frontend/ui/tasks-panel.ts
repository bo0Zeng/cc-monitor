/**
 * Issue #11: Claude Code CLI 的 task 列表展示。
 *
 * 数据源：那台机器的后端 `tasks-list`（〔RM1b · 第四波〕本机与远端同一条路，按 `origin` 问）。
 * 〔LOC1a · 第四波 4D · C4e 批 4〕Tab 创建时**经通道直接问**（`chan.call(origin, "tasks-list")`），后端出成品 `{tasks}`，
 * 这里按形状严格收（{@link decodeTasks}；字段语义只住后端 `observe/tasks_query.rs::task_entry`）。
 * 〔MIG-3b · `设计/99 §2.1 ㉓②`〕**变更推送本机远端同形**：那台后端自己盯任务目录，变了发 `tasks_changed{sid}` 帧；
 * monitor 交进通道 `subscribe(origin, "session-tasks")`（{@link SESSION_TASKS_KIND}），界面收到那个 sid 就重问一次
 * （{@link tasksChangedItems} 读那一批格）。monitor 自己那条 notify 与 `task-update` 事件删了。
 * 「被切到的那一刻 / 面板被展开的那一刻」现问一次那条照旧（兜住订阅建立之前那一窗）。
 * sid → origin 由 {@link fetchSessionTasks} 记下（Tab 创建时那一次就带着 origin）。
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

import { dispatcher } from "./keybindings/registry";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "./ipc/chan-caller";
import { LS_KEYS, safeGet, safeSet } from "./local-storage";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import type { Tab } from "./tab-model";

import { copyText } from "./copy-table";

/**
 * `tasks-list` 成品里的一个任务（线上形状：跨语言金样 `tests/__fixtures__/tasks-list.golden.json` 钉住，{@link decodeTasks} 严格收）。
 * 〔MIG-3b〕原来是 monitor `tasks.rs::TaskEntry` 的生成物；那份文件随任务 notify 进后端删了，形状手写在这里。
 */
export type TaskEntry = {
  id: string;
  subject: string;
  description?: string;
  activeForm?: string;
  status: string;
  blocks: string[];
  blockedBy: string[];
};

// 〔MIG-3b · ㉓②〕`session-tasks` 那条流的串与读格函数住叶子模块 `src/frontend/ui/tasks-stream.ts`（`events.ts` 也要它，避免 import 成环）。
export { SESSION_TASKS_KIND, SESSION_TASKS_WINDOW, tasksChangedItems } from "./tasks-stream";

function loadCollapsed(): boolean {
  const v = safeGet(LS_KEYS.tasksPanelCollapsed);
  if (v === "1") return true;
  if (v === "0") return false;
  return true;
}

function saveCollapsed(collapsed: boolean): void {
  safeSet(LS_KEYS.tasksPanelCollapsed, collapsed ? "1" : "0");
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
  /** 〔RM1b〕远端现问的代次：慢的那次回来不许盖掉后发的那次（同 `plugins-section.ts` 的 `seq`）。 */
  private refreshSeq = 0;

  constructor() {
    this.collapsed = loadCollapsed();

    // === summary chip ===
    this.summaryElement = document.createElement("button");
    this.summaryElement.type = "button";
    this.summaryElement.className = "status-tasks";
    this.summaryElement.style.display = "none"; // 默认隐藏，0 task 时一直隐藏

    this.summaryArrow = document.createElement("span");
    this.summaryArrow.className = "status-tasks-arrow";
    this.summaryArrow.textContent = copyText("tasksPanel.arrow.collapsed");
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
    popTitle.textContent = copyText("tasksPanel.ctor.title");
    popHead.appendChild(popTitle);
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "tasks-popover-close";
    closeBtn.textContent = copyText("tasksPanel.ctor.close");
    closeBtn.title = copyText("tasksPanel.ctor.closeHint");
    closeBtn.addEventListener("click", () => this.setCollapsed(true));
    popHead.appendChild(closeBtn);
    this.popoverElement.appendChild(popHead);

    this.list = document.createElement("ul");
    this.list.className = "tasks-popover-list";
    this.popoverElement.appendChild(this.list);

    this.applyCollapsedClass();
    // 构造时 tasks=[] / activeSid=null，永远不需要 pushOverlay；
    // setSession 之后用户 toggle 折叠时再由 setCollapsed 路径推入。
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
    // 〔RM1b〕远端 tab 被切到的那一刻现问一次（本机有 watcher 推送，不必）。
    if (sid !== null) void this.refreshIfRemote(sid);
  }

  /**
   * 〔RM1b〕远端会话：向那台机器的后端现问一次，回来时仍是同一个 sid、且没被更晚的一次盖过才换上。
   * 本机会话 / 不知道 origin 的 sid ⇒ 什么都不做（前者有推送，后者没有可问的对象）。
   */
  async refreshIfRemote(sid: string): Promise<void> {
    const origin = originOfSession(sid);
    if (origin === undefined || isLocalOrigin(origin)) return;
    const mine = ++this.refreshSeq;
    const tasks = await fetchSessionTasks(sid, origin);
    if (mine !== this.refreshSeq || this.activeSid !== sid) return;
    this.tasks = tasks;
    this.render();
  }

  /** 快捷键 (issue #5) 用：折叠 ↔ 展开切换 */
  toggle(): void {
    this.setCollapsed(!this.collapsed);
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
    // 〔RM1b〕展开的那一刻，远端会话现问一次（看的就是这一刻的列表）。
    if (!next && this.activeSid !== null) void this.refreshIfRemote(this.activeSid);
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
    this.summaryArrow.textContent = this.collapsed ? copyText("tasksPanel.arrow.collapsed") : copyText("tasksPanel.arrow.expanded");
    this.summaryElement.setAttribute(
      "aria-expanded",
      this.collapsed ? "false" : "true",
    );
  }
}

/** 这一问的期限：与它上一个住址（monitor `frame_query::LINES_BUDGET`）同值 —— 读一个小目录 ＋ 回程。 */
const TASKS_BUDGET_MS = 30_000;

/** 成品里一个任务**恰好**能有的键（可选的两格可缺）。 */
const TASK_REQUIRED_KEYS = ["blockedBy", "blocks", "id", "status", "subject"];
const TASK_OPTIONAL_KEYS = ["activeForm", "description"];

/**
 * 〔LOC1a · 第四波 4D〕`tasks-list` 的成品 → `TaskEntry[]`。**按形状严格收，不解释**：多一格 / 缺一格 / 类型不对 ⇒ 抛
 * （两端契约对不上，不猜、不跳过）。字段语义（哪几格必填、`null` 怎么算）只住后端 `task_entry`；
 * 线上形状由 `tests/__fixtures__/tasks-list.golden.json` 钉住（后端产出 == 金样 · 本解码器读同一份）。
 */
export function decodeTasks(v: unknown): TaskEntry[] {
  const bad = (what: string): never => {
    throw new Error(`tasks-list reply shape mismatch: ${what}`);
  };
  if (v === null || typeof v !== "object" || Array.isArray(v)) return bad("not an object");
  const o = v as Record<string, unknown>;
  if (Object.keys(o).join(",") !== "tasks" || !Array.isArray(o.tasks)) return bad("top level is not exactly one `tasks` array");
  const strs = (x: unknown): x is string[] => Array.isArray(x) && x.every((s) => typeof s === "string");
  return o.tasks.map((raw, i): TaskEntry => {
    if (raw === null || typeof raw !== "object" || Array.isArray(raw)) return bad(`item ${i} is not an object`);
    const t = raw as Record<string, unknown>;
    const keys = Object.keys(t);
    const known = [...TASK_REQUIRED_KEYS, ...TASK_OPTIONAL_KEYS];
    if (!TASK_REQUIRED_KEYS.every((k) => keys.includes(k)) || !keys.every((k) => known.includes(k))) {
      return bad(`item ${i} has keys ${keys.sort().join(",")}`);
    }
    const optOk = TASK_OPTIONAL_KEYS.every((k) => !(k in t) || typeof t[k] === "string");
    if (typeof t.id !== "string" || typeof t.subject !== "string" || typeof t.status !== "string"
      || !strs(t.blocks) || !strs(t.blockedBy) || !optOk) {
      return bad(`item ${i} has a field of the wrong type`);
    }
    return t as unknown as TaskEntry;
  });
}

/** 〔RM1b〕sid → 它住哪台机器。{@link fetchSessionTasks} 每次调用都记一笔（Tab 创建时那一次必带）。 */
const originBySid = new Map<string, Origin>();

/** 这个会话住哪台机器；从没被问过 ⇒ `undefined`（不猜成本机）。 */
export function originOfSession(sid: string): Origin | undefined {
  return originBySid.get(sid);
}

/**
 * 向那台机器的后端拉一次 task 快照（本机逐字 `LOCAL_ORIGIN`）。失败返空数组（panel 自然隐藏）。
 *
 * 〔RM1b〕第二个参数必填：本机与远端同一条路，差别只在问哪台。它收的是 **Tab 自己那一格**
 * （`Tab["origin"]`）而不是另写一份类型。〔合并 C4a〕那一格已收成 `Origin`（本机 = `LOCAL_ORIGIN`），
 * 原样过线 —— 上一版这里那个 `?? LOCAL_ORIGIN` 随之删了。
 */
export async function fetchSessionTasks(
  sessionId: string,
  tabOrigin: Tab["origin"],
): Promise<TaskEntry[]> {
  const origin: Origin = tabOrigin;
  originBySid.set(sessionId, origin);
  try {
    const budget = budgetWithin(TASKS_BUDGET_MS);
    const body = jsonBody({ sid: sessionId });
    const reply = await chan.call(origin, "tasks-list", body, budget);
    return decodeTasks(readJson(reply));
  } catch (e) {
    console.warn(`[tasks-panel] fetch ${sessionId}@${origin} failed:`, e);
    return [];
  }
}

function statusIcon(status: string): string {
  switch (status) {
    case "pending":
      return copyText("tasksPanel.status.pending");
    case "in_progress":
      return copyText("tasksPanel.status.inProgress");
    case "completed":
      return copyText("tasksPanel.status.done");
    case "deleted":
      return copyText("tasksPanel.status.failed");
    default:
      return copyText("tasksPanel.status.other");
  }
}

function cssStatus(status: string): string {
  if (!/^[a-z_]+$/.test(status)) return "unknown";
  return status;
}
