/**
 * 〔RM1b · 第四波〕任务面板按 origin 问那台机器的后端。
 *
 * 三件事：① 调用带着 origin（本机逐字 `LOCAL_ORIGIN`，Tab 那一格还是 `null` 时就地换）·
 * ② 远端 tab 被切到 / 面板展开的那一刻现问一次（本机不问 —— 本机有 watcher 推送）·
 * ③ 慢的那次回来不许盖掉后发的那次，也不许盖到已经切走的 tab 上。
 * 夹具只造结构（占位字段），不采任何真会话正文。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// 〔LOC1a · 第四波 4D〕任务快照改走通道（`chan.call(origin, "tasks-list", {sid})`，后端出成品 `{tasks}`）。
//   替身翻译层：那一发 `chan.call` 按旧形状 `{origin, sessionId}` 交给 `getSessionTasks`、把它答的数组包成成品 ——
//   下面各条原来的断言因此数得到**真发出去**的那一发（不换的话数的是一个没人调的旧命令，恒为 0，会空真地绿）。
const getSessionTasks = vi.fn();
const chanOps: string[] = [];
vi.mock("../src/comms/inward/chan", () => ({
  chan: {
    call: async (origin: string, op: string, body: Uint8Array, budget: { until: number }) => {
      chanOps.push(op);
      if (typeof budget?.until !== "number") throw new Error("没给期限");
      const args = JSON.parse(new TextDecoder().decode(body)) as { sid: string };
      const tasks = await getSessionTasks({ origin, sessionId: args.sid });
      return new TextEncoder().encode(JSON.stringify({ tasks }));
    },
  },
}));

import { TasksPanel, fetchSessionTasks, originOfSession } from "../src/tasks-panel";
import { LOCAL_ORIGIN } from "../src/backend-policy";
import type { TaskEntry } from "../src/tasks-panel";

function task(id: string, status = "pending"): TaskEntry {
  return { id, subject: `s${id}`, status, blocks: [], blockedBy: [] };
}

async function settle(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0));
}

beforeEach(() => {
  getSessionTasks.mockReset();
  chanOps.length = 0;
});

describe("LOC1a：任务快照经通道问 `tasks-list`", () => {
  it("发的是 `tasks-list`，不是别的操作名；成品形状不对 ⇒ 当成失败（空表），不猜", async () => {
    getSessionTasks.mockResolvedValue([task("1")]);
    expect(await fetchSessionTasks("s1", "aya")).toEqual([task("1")]);
    expect(chanOps).toEqual(["tasks-list"]);
    // 后端回了多一格的对象 ⇒ 解码器拒 ⇒ fetch 收成空表（面板自然隐藏）。
    getSessionTasks.mockResolvedValue([{ ...task("2"), extra: 1 }]);
    expect(await fetchSessionTasks("s2", "aya")).toEqual([]);
  });
});

describe("RM1b：调用带着 origin", () => {
  it("本机 tab（那一格还是 null）⇒ 逐字送 LOCAL_ORIGIN；远端 ⇒ 送那台的名字", async () => {
    getSessionTasks.mockResolvedValue([task("1")]);
    await fetchSessionTasks("sid-local", LOCAL_ORIGIN);
    await fetchSessionTasks("sid-remote", "aya");
    expect(getSessionTasks.mock.calls.map((c) => c[0])).toEqual([
      { origin: LOCAL_ORIGIN, sessionId: "sid-local" },
      { origin: "aya", sessionId: "sid-remote" },
    ]);
    expect(originOfSession("sid-local")).toBe(LOCAL_ORIGIN);
    expect(originOfSession("sid-remote")).toBe("aya");
    // 反向：从没问过的 sid 不猜成本机。
    expect(originOfSession("never-seen")).toBeUndefined();
  });

  it("失败 ⇒ 空表（面板自然隐藏），不抛", async () => {
    getSessionTasks.mockRejectedValue("远端 [aya] 的后端还不认 `tasks-list`");
    expect(await fetchSessionTasks("sid-x", "aya")).toEqual([]);
  });
});

describe("RM1b：远端现问，本机不问", () => {
  it("切到远端 tab ⇒ 现问一次并换上；切到本机 tab ⇒ 一次都不问", async () => {
    getSessionTasks.mockResolvedValue([]);
    await fetchSessionTasks("r1", "aya");
    await fetchSessionTasks("l1", LOCAL_ORIGIN);
    getSessionTasks.mockReset();

    const p = new TasksPanel();
    getSessionTasks.mockResolvedValue([task("1"), task("2", "completed")]);
    p.setSession("r1", []);
    await settle();
    expect(getSessionTasks).toHaveBeenCalledTimes(1);
    expect(getSessionTasks.mock.calls[0][0]).toEqual({ origin: "aya", sessionId: "r1" });
    expect(p.summaryElement.style.display).toBe("");
    expect(p.summaryElement.textContent).toContain("2 tasks");

    getSessionTasks.mockReset();
    p.setSession("l1", [task("9")]);
    await settle();
    expect(getSessionTasks, "本机 tab 也去问了 —— 本机有 watcher 推送").toHaveBeenCalledTimes(0);
  });

  it("展开面板那一刻，远端现问一次", async () => {
    getSessionTasks.mockResolvedValue([task("1")]);
    await fetchSessionTasks("r2", "aya");
    const p = new TasksPanel();
    p.setSession("r2", [task("1")]);
    await settle();
    getSessionTasks.mockReset();
    getSessionTasks.mockResolvedValue([task("1"), task("2"), task("3")]);
    // 先保证是折叠态，再展开。
    if (p.summaryElement.getAttribute("aria-expanded") === "true") p.toggle();
    getSessionTasks.mockClear();
    p.toggle();
    await settle();
    expect(getSessionTasks).toHaveBeenCalledTimes(1);
    expect(p.summaryElement.textContent).toContain("3 tasks");
  });

  it("★ 回来时已经切走（切到一个不必问的本机 tab）⇒ 不许盖到新 tab 上", async () => {
    getSessionTasks.mockResolvedValue([]);
    await fetchSessionTasks("ra", "aya");
    await fetchSessionTasks("la", LOCAL_ORIGIN);
    const p = new TasksPanel();

    let releaseA!: (v: TaskEntry[]) => void;
    getSessionTasks.mockReset();
    getSessionTasks.mockImplementationOnce(
      () => new Promise<TaskEntry[]>((r) => (releaseA = r)),
    );
    p.setSession("ra", []);
    // 切到本机 tab：本机不现问 ⇒ 代次没动，拦住那次迟到的只能是「sid 已经不是它」这一条。
    p.setSession("la", [task("9")]);
    await settle();
    expect(p.summaryElement.textContent).toContain("1 tasks");
    releaseA([task("1"), task("2"), task("3"), task("4"), task("5")]);
    await settle();
    expect(p.summaryElement.textContent).toContain("1 tasks");
    expect(p.summaryElement.textContent).not.toContain("5 tasks");
  });

  it("★ 同一个远端 tab 连问两次，慢的那次（先发的）不许盖掉快的那次（后发的）", async () => {
    getSessionTasks.mockResolvedValue([]);
    await fetchSessionTasks("rc", "aya");
    const p = new TasksPanel();

    let releaseFirst!: (v: TaskEntry[]) => void;
    getSessionTasks.mockReset();
    getSessionTasks.mockImplementationOnce(
      () => new Promise<TaskEntry[]>((r) => (releaseFirst = r)),
    );
    getSessionTasks.mockResolvedValueOnce([task("1"), task("2")]);
    p.setSession("rc", []);
    void p.refreshIfRemote("rc");
    await settle();
    expect(p.summaryElement.textContent).toContain("2 tasks");
    releaseFirst([task("1"), task("2"), task("3"), task("4"), task("5")]);
    await settle();
    expect(p.summaryElement.textContent).toContain("2 tasks");
  });
});
