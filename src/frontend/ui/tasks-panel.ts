/**
 * Issue #11: Claude Code CLI 的 task 列表展示。
 *
 * 数据源：那台机器的后端 `tasks-list`（本机与远端同一条路，按 `origin` 问）。
 * Tab 创建时**经通道直接问**（`chan.call(origin, "tasks-list")`），后端出成品 `{tasks}`，
 * 这里按形状严格收（{@link decodeTasks}；字段语义只住后端 `observe/tasks_query.rs::task_entry`）。
 * **变更推送本机远端同形**：那台后端自己盯任务目录，变了发 `tasks_changed{sid}` 帧；
 * monitor 交进通道 `subscribe(origin, "session-tasks")`（{@link SESSION_TASKS_KIND}），界面收到那个 sid 就重问一次
 * （{@link tasksChangedItems} 读那一批格）。monitor 自己那条 notify 与 `task-update` 事件删了。
 * 「被切到的那一刻 / 抽屉开到这一页的那一刻」现问一次那条照旧（兜住订阅建立之前那一窗）。
 * sid → origin 由 {@link fetchSessionTasks} 记下（Tab 创建时那一次就带着 origin）。
 *
 * 界面：
 *  - **chip**：状态栏里 `任务 {做完}/{全部}`，悬停分项与键位；点了交宿主开 / 收底部抽屉的「任务」页。这个会话没有任务 ⇒ 不渲染。
 *  - **抽屉页**：一个任务一行（状态图标 · 主题 · 在做的那一句），按 id 原地更新；这个会话没有任务 ⇒ 页里写空态。
 *
 * 全局单例：TabManager 维护 `Map<sid, TaskEntry[]>`，切 Tab / 收到 update 都同步给单例。
 * 同 sid 只显示自己的 task，跨 Tab 不混淆。
 */

import { dispatcher, KeybindingDispatcher } from "./keybindings/registry";
import { chip } from "./kit/chip";
import { attachTooltip } from "./kit/tooltip";
import { icon, type IconName } from "./kit/icon";
import { emptyState } from "./kit/empty";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "./ipc/chan-caller";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import type { Tab } from "./tab-model";

import { copyText } from "./copy-table";

/**
 * `tasks-list` 成品里的一个任务（线上形状：跨语言金样 `tests/__fixtures__/tasks-list.golden.json` 钉住，{@link decodeTasks} 严格收）。
 * 原来是 monitor `tasks.rs::TaskEntry` 的生成物；那份文件随任务 notify 进后端删了，形状手写在这里。
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

// `session-tasks` 那条流的串与读格函数住叶子模块 `src/frontend/ui/tasks-stream.ts`（`events.ts` 也要它，避免 import 成环）。
export { SESSION_TASKS_KIND, SESSION_TASKS_WINDOW, tasksChangedItems } from "./tasks-stream";

/** 一个任务行前面那颗图标（状态 ⇒ 图标名）。 */
function statusIconName(status: string): IconName {
  switch (status) {
    case "completed":
      return "check";
    case "in_progress":
      return "inProgress";
    case "deleted":
      return "failed";
    default:
      return "pending";
  }
}

/** chip 前面那颗图标。 */
const CHIP_ICON: IconName = "tasks";

export class TasksPanel {
  /** 挂到 status-bar 里当 chip：点了交宿主（开 / 收底部抽屉的「任务」页）。 */
  readonly summaryElement: HTMLButtonElement;
  /** 底部抽屉「任务」页的内容。 */
  readonly pageElement: HTMLElement;
  /** chip 被点了（宿主接到抽屉上）。 */
  onChip: (() => void) | null = null;
  /** chip 上的字变了（宿主同步到抽屉页签）。 */
  onLabel: ((label: string) => void) | null = null;

  private summaryText: HTMLElement;
  /** chip 悬停那一句（每次画时按此刻的分项现拼）。 */
  private hint = "";
  private list: HTMLUListElement;
  private empty: HTMLElement;
  /** 画出来的行按任务 id 留着：任务表又来一份时原地改，不整表重建。 */
  private readonly rows = new Map<string, HTMLLIElement>();

  private tasks: TaskEntry[] = [];
  /** 当前显示的 session（来自 TabManager.activeId）。`null` = 无 active Tab。 */
  private activeSid: string | null = null;
  /** 远端现问的代次：慢的那次回来不许盖掉后发的那次（同 `plugins-section.ts` 的 `seq`）。 */
  private refreshSeq = 0;
  /** 抽屉此刻开在这一页。 */
  private visible = false;

  constructor() {
    // === summary chip（C6）：`任务 2/4`；悬停分项与键位 ===
    this.summaryElement = chip({ text: "", icon: CHIP_ICON, onClick: () => this.onChip?.() }) as HTMLButtonElement;
    this.summaryElement.classList.add("status-tasks");
    this.summaryElement.style.display = "none"; // 这个会话没有任务 ⇒ 不渲染
    this.summaryElement.setAttribute("aria-expanded", "false");
    this.summaryText = this.summaryElement.querySelector("span") as HTMLElement;
    attachTooltip(this.summaryElement, () => this.hint);

    // === 抽屉页 ===
    this.pageElement = document.createElement("div");
    this.pageElement.className = "tasks-page";
    this.list = document.createElement("ul");
    this.list.className = "tasks-list";
    this.empty = emptyState({ icon: "tasks", text: copyText("tasksPanel.page.empty") });
    this.render();
  }

  /** 页签上的字（有任务 ⇒ 与 chip 同字；没有 ⇒ 页名）。 */
  get label(): string {
    return this.tasks.length > 0 && this.activeSid !== null ? (this.summaryText.textContent ?? "") : copyText("tasksPanel.page.title");
  }

  /** 抽屉开到 / 离开这一页（宿主调）。开到的那一刻远端会话现问一次（看的就是这一刻的列表）。 */
  setVisible(on: boolean): void {
    if (on === this.visible) return;
    this.visible = on;
    this.summaryElement.setAttribute("aria-expanded", String(on));
    if (on && this.activeSid !== null) void this.refreshIfRemote(this.activeSid);
  }

  /**
   * 切换当前显示的 session（TabManager.switchTo 或新 Tab 创建时调）。
   * `null` 表示无 active Tab —— summary 隐藏。
   */
  setSession(sid: string | null, tasks: TaskEntry[]): void {
    this.activeSid = sid;
    this.tasks = tasks;
    this.render();
    // 远端 tab 被切到的那一刻现问一次（本机有 watcher 推送，不必）。
    if (sid !== null) void this.refreshIfRemote(sid);
  }

  /**
   * 远端会话：向那台机器的后端现问一次，回来时仍是同一个 sid、且没被更晚的一次盖过才换上。
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

  private render(): void {
    const total = this.activeSid === null ? 0 : this.tasks.length;
    this.summaryElement.style.display = total === 0 ? "none" : "";
    const body = total > 0 ? this.list : this.empty;
    if (this.pageElement.firstChild !== body) this.pageElement.replaceChildren(body);
    if (total === 0) {
      this.place([]);
      this.onLabel?.(this.label);
      return;
    }

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
    this.summaryText.textContent = copyText("statusBar.tasks.chip", { done: completed, total });
    const chord = dispatcher.effectiveChord("panel.toggle-tasks");
    const parts = copyText("statusBar.tasks.hint", { done: completed, active: inProgress, open: pending + other });
    this.hint = chord ? `${parts} · ${KeybindingDispatcher.prettyChord(chord)}` : parts;
    this.onLabel?.(this.label);
    this.place(this.tasks.map((t) => this.row(t)));
  }

  /** 列表换成 `items` 这一串：留着的行原地不动（只在次序不对时挪），没了的摘掉。 */
  private place(items: HTMLLIElement[]): void {
    let cur: ChildNode | null = this.list.firstChild;
    for (const n of items) {
      if (n === cur) cur = cur.nextSibling;
      else this.list.insertBefore(n, cur);
    }
    while (cur) {
      const next: ChildNode | null = cur.nextSibling;
      cur.remove();
      cur = next;
    }
    const keep = new Set(items);
    for (const [id, el] of this.rows) if (!keep.has(el)) this.rows.delete(id);
  }

  /** 一个任务一行（按 id 留着，字与状态原地换）。 */
  private row(t: TaskEntry): HTMLLIElement {
    let row = this.rows.get(t.id);
    if (!row) {
      row = document.createElement("li");
      row.dataset.taskId = t.id;
      this.rows.set(t.id, row);
    }
    row.className = `tasks-item status-${cssStatus(t.status)}`;
    const sig = JSON.stringify([t.status, t.subject, t.activeForm ?? null, t.description ?? null]);
    if (row.dataset.sig === sig) return row;
    row.dataset.sig = sig;
    const ic = icon(statusIconName(t.status), "compact");
    ic.setAttribute("aria-hidden", "false");
    ic.setAttribute("role", "img");
    ic.setAttribute("aria-label", statusText(t.status));
    const subject = document.createElement("span");
    subject.className = "tasks-subject";
    subject.textContent = t.subject;
    row.replaceChildren(ic, subject);
    if (t.status === "in_progress" && t.activeForm) {
      const now = document.createElement("span");
      now.className = "tasks-now";
      now.textContent = t.activeForm;
      row.appendChild(now);
    }
    if (t.description) row.title = t.description;
    else row.removeAttribute("title");
    return row;
  }
}

/** 这一问的期限：与它上一个住址（monitor `frame_query::LINES_BUDGET`）同值 —— 读一个小目录 ＋ 回程。 */
const TASKS_BUDGET_MS = 30_000;

/** 成品里一个任务**恰好**能有的键（可选的两格可缺）。 */
const TASK_REQUIRED_KEYS = ["blockedBy", "blocks", "id", "status", "subject"];
const TASK_OPTIONAL_KEYS = ["activeForm", "description"];

/**
 * `tasks-list` 的成品 → `TaskEntry[]`。**按形状严格收，不解释**：多一格 / 缺一格 / 类型不对 ⇒ 抛
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

/** sid → 它住哪台机器。{@link fetchSessionTasks} 每次调用都记一笔（Tab 创建时那一次必带）。 */
const originBySid = new Map<string, Origin>();

/** 这个会话住哪台机器；从没被问过 ⇒ `undefined`（不猜成本机）。 */
export function originOfSession(sid: string): Origin | undefined {
  return originBySid.get(sid);
}

/**
 * 向那台机器的后端拉一次 task 快照（本机逐字 `LOCAL_ORIGIN`）。失败返空数组（panel 自然隐藏）。
 *
 * 第二个参数必填：本机与远端同一条路，差别只在问哪台。它收的是 **Tab 自己那一格**
 * （`Tab["origin"]`）而不是另写一份类型。那一格已收成 `Origin`（本机 = `LOCAL_ORIGIN`），
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

/** 读屏念的状态名。 */
function statusText(status: string): string {
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
