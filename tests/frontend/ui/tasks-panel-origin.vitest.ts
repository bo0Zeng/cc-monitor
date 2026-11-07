/**
 * 任务面板按 origin 问那台机器的后端。
 *
 * 三件事：① 调用带着 origin（本机逐字 `LOCAL`，Tab 那一格还是 `null` 时就地换）·
 * ② 远端 tab 被切到 / 面板展开的那一刻现问一次（本机不问 —— 本机有 watcher 推送）·
 * ③ 慢的那次回来不许盖掉后发的那次，也不许盖到已经切走的 tab 上。
 * 夹具只造结构（占位字段），不采任何真会话正文。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// 任务快照改走通道（`chan.call(origin, "tasks-list", {sid})`，后端出成品 `{tasks, chip, hint}`）。
//   替身翻译层：那一发 `chan.call` 按旧形状 `{origin, sessionId}` 交给 `getSessionTasks`、把它答的数组包成成品（{@link listOf}）——
//   下面各条原来的断言因此数得到**真发出去**的那一发（不换的话数的是一个没人调的旧命令，恒为 0，会空真地绿）。
const getSessionTasks = vi.fn();
const chanOps: string[] = [];
vi.mock("../../../src/comms/inward/chan", () => ({
  chan: {
    call: async (origin: string, op: string, body: Uint8Array, budget: { until: number }) => {
      chanOps.push(op);
      if (typeof budget?.until !== "number") throw new Error("没给期限");
      const args = JSON.parse(new TextDecoder().decode(body)) as { sid: string };
      const tasks = await getSessionTasks({ origin, sessionId: args.sid });
      return new TextEncoder().encode(JSON.stringify(listOf(tasks)));
    },
  },
}));

import { NO_TASKS, TasksPanel, fetchSessionTasks, originOfSession } from "../../../src/frontend/ui/tasks-panel";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";
import type { TaskEntry } from "../../../src/frontend/ui/tasks-panel";
import type { TasksList } from "../../../src/frontend/ui/generated/TasksList";

function task(id: string, status = "pending"): TaskEntry {
  return { id, subject: `s${id}`, status, statusText: status, blocks: [] };
}

/** 后端那一份成品的替身：chip 只造一句带总数的占位（下面按「/总数」认是哪一次的快照）。 */
function listOf(tasks: TaskEntry[]): TasksList {
  return { tasks, chip: tasks.length > 0 ? `枚 /${tasks.length}` : "", hint: "" };
}

async function settle(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0));
}

beforeEach(() => {
  getSessionTasks.mockReset();
  chanOps.length = 0;
});

describe("LOC1a：任务快照经通道问 `tasks-list`", () => {
  it("发的是 `tasks-list`，不是别的操作名；回包按类型用（形状由握手指纹管，这里不逐格验）", async () => {
    getSessionTasks.mockResolvedValue([task("1")]);
    expect(await fetchSessionTasks("s1", "devbox")).toEqual(listOf([task("1")]));
    expect(chanOps).toEqual(["tasks-list"]);
  });
});

describe("RM1b：调用带着 origin", () => {
  it("本机 tab（那一格还是 null）⇒ 逐字送 LOCAL；远端 ⇒ 送那台的名字", async () => {
    getSessionTasks.mockResolvedValue([task("1")]);
    await fetchSessionTasks("sid-local", LOCAL);
    await fetchSessionTasks("sid-remote", "devbox");
    expect(getSessionTasks.mock.calls.map((c) => c[0])).toEqual([
      { origin: LOCAL, sessionId: "sid-local" },
      { origin: "devbox", sessionId: "sid-remote" },
    ]);
    expect(originOfSession("sid-local")).toBe(LOCAL);
    expect(originOfSession("sid-remote")).toBe("devbox");
    // 反向：从没问过的 sid 不猜成本机。
    expect(originOfSession("never-seen")).toBeUndefined();
  });

  it("失败 ⇒ 空表（面板自然隐藏），不抛", async () => {
    getSessionTasks.mockRejectedValue("远端 [devbox] 的后端还不认 `tasks-list`");
    expect(await fetchSessionTasks("sid-x", "devbox")).toEqual(NO_TASKS);
  });
});

describe("RM1b：远端现问，本机不问", () => {
  it("切到远端 tab ⇒ 现问一次并换上；切到本机 tab ⇒ 一次都不问", async () => {
    getSessionTasks.mockResolvedValue([]);
    await fetchSessionTasks("r1", "devbox");
    await fetchSessionTasks("l1", LOCAL);
    getSessionTasks.mockReset();

    const p = new TasksPanel();
    getSessionTasks.mockResolvedValue([task("1"), task("2", "completed")]);
    p.setSession("r1", NO_TASKS);
    await settle();
    expect(getSessionTasks).toHaveBeenCalledTimes(1);
    expect(getSessionTasks.mock.calls[0][0]).toEqual({ origin: "devbox", sessionId: "r1" });
    expect(p.summaryElement.style.display).toBe("");
    expect(p.summaryElement.textContent).toContain("/2");

    getSessionTasks.mockReset();
    p.setSession("l1", listOf([task("9")]));
    await settle();
    expect(getSessionTasks, "本机 tab 也去问了 —— 本机有 watcher 推送").toHaveBeenCalledTimes(0);
  });

  it("抽屉开到任务页那一刻，远端现问一次", async () => {
    getSessionTasks.mockResolvedValue([task("1")]);
    await fetchSessionTasks("r2", "devbox");
    const p = new TasksPanel();
    p.setSession("r2", listOf([task("1")]));
    await settle();
    getSessionTasks.mockReset();
    getSessionTasks.mockResolvedValue([task("1"), task("2"), task("3")]);
    getSessionTasks.mockClear();
    p.setVisible(true);
    await settle();
    expect(getSessionTasks).toHaveBeenCalledTimes(1);
    expect(p.summaryElement.textContent).toContain("/3");
  });

  it("★ 回来时已经切走（切到一个不必问的本机 tab）⇒ 不许盖到新 tab 上", async () => {
    getSessionTasks.mockResolvedValue([]);
    await fetchSessionTasks("ra", "devbox");
    await fetchSessionTasks("la", LOCAL);
    const p = new TasksPanel();

    let releaseA!: (v: TaskEntry[]) => void;
    getSessionTasks.mockReset();
    getSessionTasks.mockImplementationOnce(
      () => new Promise<TaskEntry[]>((r) => (releaseA = r)),
    );
    p.setSession("ra", NO_TASKS);
    // 切到本机 tab：本机不现问 ⇒ 代次没动，拦住那次迟到的只能是「sid 已经不是它」这一条。
    p.setSession("la", listOf([task("9")]));
    await settle();
    expect(p.summaryElement.textContent).toContain("/1");
    releaseA([task("1"), task("2"), task("3"), task("4"), task("5")]);
    await settle();
    expect(p.summaryElement.textContent).toContain("/1");
    expect(p.summaryElement.textContent).not.toContain("/5");
  });

  it("★ 同一个远端 tab 连问两次，慢的那次（先发的）不许盖掉快的那次（后发的）", async () => {
    getSessionTasks.mockResolvedValue([]);
    await fetchSessionTasks("rc", "devbox");
    const p = new TasksPanel();

    let releaseFirst!: (v: TaskEntry[]) => void;
    getSessionTasks.mockReset();
    getSessionTasks.mockImplementationOnce(
      () => new Promise<TaskEntry[]>((r) => (releaseFirst = r)),
    );
    getSessionTasks.mockResolvedValueOnce([task("1"), task("2")]);
    p.setSession("rc", NO_TASKS);
    void p.refreshIfRemote("rc");
    await settle();
    expect(p.summaryElement.textContent).toContain("/2");
    releaseFirst([task("1"), task("2"), task("3"), task("4"), task("5")]);
    await settle();
    expect(p.summaryElement.textContent).toContain("/2");
  });
});

describe("重画不丢焦点（kit place）", () => {
  it("★ 焦点在某一行上，推来的新快照换了次序 ⇒ 还是那个节点、焦点还在它上", async () => {
    getSessionTasks.mockResolvedValue([]);
    await fetchSessionTasks("lf", LOCAL);
    const p = new TasksPanel();
    document.body.replaceChildren(p.pageElement);
    p.setSession("lf", listOf([task("1"), task("2"), task("3")]));
    const row = p.pageElement.querySelector<HTMLElement>('[data-task-id="2"]')!;
    row.tabIndex = 0;
    row.focus();
    p.setSession("lf", listOf([task("3"), task("1"), task("2", "completed")]));
    expect(p.pageElement.querySelector('[data-task-id="2"]')).toBe(row);
    expect(document.activeElement).toBe(row);
  });
});
