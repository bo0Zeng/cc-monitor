/**
 * Issue #11: Claude Code CLI 的 task 列表展示。
 *
 * 数据源：那台机器的后端 `tasks-list`（本机与远端同一条路，按 `origin` 问）。
 * Tab 创建时**经通道按类型问**（`callTyped(origin, "tasks-list")`），后端出成品 `{tasks}`（字段语义只住后端 `observe/tasks_query.rs::task_entry`）；
 * 帧里推来的那一份同 `tasks-list` 的回包，按同一个类型用。
 * **变更推送本机远端同形**：那台后端自己盯任务目录，变了发 `changed {tasks, key: sid}` 帧；
 * monitor 交进通道 `subscribe(origin, "changed/tasks")`（`changed-stream.ts`），界面收到那个 sid 就重问一次
 * （`changed-stream.ts::changedItems` 读那一批格）。monitor 自己那条 notify 与 `task-update` 事件删了。
 * 「被切到的那一刻 / 抽屉开到这一页的那一刻」现问一次那条照旧（兜住订阅建立之前那一窗）。
 * sid → origin 由 {@link fetchSessionTasks} 记下（Tab 创建时那一次就带着 origin）。
 *
 * 界面：
 *  - **chip**：状态栏里那一枚照摆后端写好的 `chip`，悬停照摆 `hint` ＋ 键位；点了交宿主开 / 收底部抽屉的「任务」页。这个会话没有任务 ⇒ 不渲染。
 *  - **抽屉页**：一个任务一行（状态图标 · 主题 · 在做的那一句；读屏字照摆 `statusText`），按 id 原地更新；这个会话没有任务 ⇒ 页里写空态。
 *
 * 全局单例：TabManager 维护 `Map<sid, TasksList>`，切 Tab / 收到 update 都同步给单例。
 * 同 sid 只显示自己的 task，跨 Tab 不混淆。
 */

import { dispatcher, KeybindingDispatcher } from "./keybindings/registry";
import { chip } from "./kit/chip";
import { attachTooltip } from "./kit/tooltip";
import { icon, type IconName } from "./kit/icon";
import { emptyState } from "./kit/empty";
import { place } from "./kit/morph";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Task } from "./generated/Task";
import type { TasksList } from "./generated/TasksList";
import { isLocalOrigin, type Machine } from "./ipc/origin";
import type { Tab } from "./tab-model";

import { copyText } from "./copy-table";

/**
 * `tasks-list` 成品里的一个任务：形状从后端类型生成（`observe/tasks_query.rs::Task`）。
 * 问回来的与帧里推来的同一个类型（形状由握手指纹管）。
 */
export type TaskEntry = Task;

/** 没有任务（还没问到 · 问不到 · 没有活动 Tab）那一份。 */
export const NO_TASKS: TasksList = { tasks: [], chip: "", hint: "" };


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
  /** chip 悬停那一句（后端的 `hint` ＋ 此刻的键位）。 */
  private hint = "";
  private list: HTMLUListElement;
  private empty: HTMLElement;
  /** 画出来的行按任务 id 留着：任务表又来一份时原地改，不整表重建。 */
  private readonly rows = new Map<string, HTMLLIElement>();

  private got: TasksList = NO_TASKS;
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
    return this.got.tasks.length > 0 && this.activeSid !== null ? (this.summaryText.textContent ?? "") : copyText("tasksPanel.page.title");
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
  setSession(sid: string | null, got: TasksList): void {
    this.activeSid = sid;
    this.got = got;
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
    const got = await fetchSessionTasks(sid, origin);
    if (mine !== this.refreshSeq || this.activeSid !== sid) return;
    this.got = got;
    this.render();
  }

  private render(): void {
    const total = this.activeSid === null ? 0 : this.got.tasks.length;
    this.summaryElement.style.display = total === 0 ? "none" : "";
    const body = total > 0 ? this.list : this.empty;
    if (this.pageElement.firstChild !== body) this.pageElement.replaceChildren(body);
    if (total === 0) {
      place(this.list, []);
      this.rows.clear();
      this.onLabel?.(this.label);
      return;
    }

    this.summaryText.textContent = this.got.chip;
    const chord = dispatcher.effectiveChord("panel.toggle-tasks");
    this.hint = chord ? `${this.got.hint} · ${KeybindingDispatcher.prettyChord(chord)}` : this.got.hint;
    this.onLabel?.(this.label);
    const items = this.got.tasks.map((t) => this.row(t));
    place(this.list, items);
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
    const sig = JSON.stringify([t.status, t.statusText, t.subject, t.activeForm ?? null, t.description ?? null]);
    if (row.dataset.sig === sig) return row;
    row.dataset.sig = sig;
    const ic = icon(statusIconName(t.status), "compact");
    ic.setAttribute("aria-hidden", "false");
    ic.setAttribute("role", "img");
    ic.setAttribute("aria-label", t.statusText);
    const subject = document.createElement("span");
    subject.className = "tasks-subject one-line";
    subject.textContent = t.subject;
    row.replaceChildren(ic, subject);
    if (t.status === "in_progress" && t.activeForm) {
      const now = document.createElement("span");
      now.className = "tasks-now one-line";
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

/** sid → 它住哪台机器。{@link fetchSessionTasks} 每次调用都记一笔（Tab 创建时那一次必带）。 */
const originBySid = new Map<string, Machine>();

/** 这个会话住哪台机器；从没被问过 ⇒ `undefined`（不猜成本机）。 */
export function originOfSession(sid: string): Machine | undefined {
  return originBySid.get(sid);
}

/**
 * 向那台机器的后端拉一次 task 快照（本机逐字 `LOCAL`）。失败返 {@link NO_TASKS}（panel 自然隐藏）。
 *
 * 第二个参数必填：本机与远端同一条路，差别只在问哪台。它收的是 **Tab 自己那一格**
 * （`Tab["origin"]`）而不是另写一份类型。那一格已收成 `Machine`（本机 = `LOCAL`），
 * 原样过线 —— 上一版这里那个 `?? LOCAL` 随之删了。
 */
export async function fetchSessionTasks(
  sessionId: string,
  tabOrigin: Tab["origin"],
): Promise<TasksList> {
  const origin: Machine = tabOrigin;
  originBySid.set(sessionId, origin);
  try {
    const budget = budgetWithin(TASKS_BUDGET_MS);
    return await callTyped(origin, "tasks-list", { sid: sessionId }, budget);
  } catch (e) {
    console.warn(`[tasks-panel] fetch ${sessionId}@${origin} failed:`, e);
    return NO_TASKS;
  }
}

function cssStatus(status: string): string {
  if (!/^[a-z_]+$/.test(status)) return "unknown";
  return status;
}
