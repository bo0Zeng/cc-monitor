/**
 * `KR92D1` 第 ④ 刀，**打在真正读它的那一端**：`HistoryView` 自己。
 *
 * # 为什么 `counted.vitest.ts` 一个人不够
 *
 * 那一份判的是「读法口本身对不对」。谁把 `history.ts` 改回
 * `Number(b.hasLive) - Number(a.hasLive)` / `proj.starredCount += 1`，
 * 那一份**照样全绿** —— 夹具绿了，被测对象没人管（`brief` 第 12 条那一族）。
 * ⇒ 本文件在**真 `HistoryView` 实例**上断：一整趟渲染排出来的顺序、
 * 以及一次真 star 操作之后那一格还是不是「不知道」。
 *
 * mock 骨架照 `history-filter-collapse.vitest.ts`（同一份空壳协作者）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));
vi.mock("../../src/views/session-viewer", () => ({
  SessionViewer: class {
    element = document.createElement("div");
    constructor(_close: () => void) {}
    load(): void {}
    dispose(): void {}
  },
}));
vi.mock("../../src/keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../../src/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../src/remote-launch-run", () => ({ runRemoteResume: vi.fn() }));
vi.mock("../../src/behavior", () => ({ getBehavior: () => ({}) }));
vi.mock("../../src/format", () => ({ formatTimestampSmart: () => "时间" }));

import { invoke } from "@tauri-apps/api/core";
import { HistoryView } from "../../src/views/history";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

/** 三个项目，`lastActivity` 全同 ⇒ 顺序只由「活没活 / 有没有星标」两档决定。 */
function proj(
  name: string,
  hasLive: boolean | null,
  starredCount: number | null,
): Record<string, unknown> {
  return {
    projectPath: `/p/${name}`,
    projectName: name,
    projectDir: name,
    sessionCount: 1,
    starredCount,
    hiddenCount: starredCount,
    lastActivity: 1,
    hasLive,
  };
}

function renderedOrder(): string[] {
  return [...document.querySelectorAll(".history-group-name")].map(
    (e) => e.textContent ?? "",
  );
}

function statsOf(name: string): string {
  const el = [...document.querySelectorAll(".history-group")].find(
    (g) => g.querySelector(".history-group-name")?.textContent === name,
  );
  return el?.querySelector(".history-group-stats")?.textContent ?? "";
}

describe("KR92D1 ④：HistoryView 不把「不知道」当 0 读", () => {
  beforeEach(() => {
    localStorage.clear();
    document.body.replaceChildren();
    invokeMock.mockReset();
  });

  it("排序：不知道自成一档 —— 排在「确定活着」之后、「确定没活」之前", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_history_projects")
        return Promise.resolve([
          proj("确定没活", false, 0),
          proj("不知道", null, null),
          proj("确定活着", true, 0),
        ]);
      if (cmd === "list_remote_history_projects")
        return Promise.resolve({ projects: [], failedHosts: [] });
      return Promise.resolve(undefined);
    });
    const view = new HistoryView();
    await view.open();

    // 🔴 这就是第 ④ 刀：改回 `Number(b.hasLive) - Number(a.hasLive)` ⇒ `Number(null) === 0`
    //   ⇒ 「不知道」与「确定没活」同档，两者退回输入顺序（不知道会排到确定没活**后面**）⇒ 本条红。
    expect(renderedOrder()).toEqual(["确定活着", "不知道", "确定没活"]);
    view.close();
  });

  it("排序：星标那一档同样分得开（不知道 > 查过了一个都没有）", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_history_projects")
        return Promise.resolve([
          proj("查过了没星标", false, 0),
          proj("星标不知道", false, null),
        ]);
      if (cmd === "list_remote_history_projects")
        return Promise.resolve({ projects: [], failedHosts: [] });
      return Promise.resolve(undefined);
    });
    const view = new HistoryView();
    await view.open();
    expect(renderedOrder()).toEqual(["星标不知道", "查过了没星标"]);
    view.close();
  });

  it("组头 chip：算过了才说话 —— 不知道那一档不冒出 `★ 0` / `● live`", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_history_projects")
        return Promise.resolve([proj("不知道", null, null), proj("有星标", true, 2)]);
      if (cmd === "list_remote_history_projects")
        return Promise.resolve({ projects: [], failedHosts: [] });
      return Promise.resolve(undefined);
    });
    const view = new HistoryView();
    await view.open();
    expect(statsOf("有星标")).toContain("● live");
    expect(statsOf("有星标")).toContain("★ 2");
    // 不知道那一格：不许把没人查过的值说成「没有星标」「没有活会话」，也不许假装有。
    expect(statsOf("不知道")).not.toContain("★");
    expect(statsOf("不知道")).not.toContain("live");
    view.close();
  });

  it("求和：一次 star 操作不许把「不知道」变成 1", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_history_projects")
        return Promise.resolve([proj("不知道", null, null)]);
      if (cmd === "update_history_metadata")
        return Promise.resolve({ starred: true, hidden: false, updatedAt: 1 });
      if (cmd === "list_remote_history_projects")
        return Promise.resolve({ projects: [], failedHosts: [] });
      return Promise.resolve(undefined);
    });
    const view = new HistoryView();
    await view.open();
    const internals = view as unknown as {
      projects: { starredCount: number | null; hiddenCount: number | null }[];
      runStar(ctx: unknown): Promise<void>;
      runHide(ctx: unknown): Promise<void>;
    };
    const project = internals.projects[0];
    const entry = { sessionId: "s1", starred: false, hidden: false };

    await internals.runStar({ entry, project, hasEntry: true });
    // 🔴 `proj.starredCount += 1` 在 `null` 上得 `1` —— 「不知道」被一次点击变成了
    //   一个看起来是真值的数，而且再也回不去。本条就钉在这里。
    expect(project.starredCount).toBeNull();

    await internals.runHide({
      entry: { sessionId: "s1", starred: true, hidden: false },
      project,
      hasEntry: true,
    });
    expect(project.hiddenCount).toBeNull();
    view.close();
  });

  it("对照组：算过了的那一档照常加减（本族判据不是靠「什么都不做」绿的）", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_history_projects")
        return Promise.resolve([proj("算过了", false, 0)]);
      if (cmd === "update_history_metadata")
        return Promise.resolve({ starred: true, hidden: false, updatedAt: 1 });
      if (cmd === "list_remote_history_projects")
        return Promise.resolve({ projects: [], failedHosts: [] });
      return Promise.resolve(undefined);
    });
    const view = new HistoryView();
    await view.open();
    const internals = view as unknown as {
      projects: { starredCount: number | null }[];
      runStar(ctx: unknown): Promise<void>;
    };
    const project = internals.projects[0];
    await internals.runStar({
      entry: { sessionId: "s1", starred: false, hidden: false },
      project,
      hasEntry: true,
    });
    expect(project.starredCount).toBe(1);
    view.close();
  });
});
