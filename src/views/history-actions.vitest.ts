// F96（#62）历史条目共享动作表 + 右键菜单 + 起新会话的 jsdom 测试。
//
// 为什么需要它：history.ts 1710+ 行零 TS 单测；F96 把 inline 五按钮 handler byte-for-byte
// 抽进 run 方法 + 加右键菜单 + new-session。这里锁：inline 按钮仍触发正确 IPC（回归护栏）、
// 右键菜单出正确项、new-session 本地/远端走正确入口、inline 与菜单走同一 run。
//
// 用 (view as any).buildEntryRow(entry, proj) 直接产一行来测（不必铺全 render）。

import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));
vi.mock("./session-viewer", () => ({
  SessionViewer: class {
    element = document.createElement("div");
    constructor(_c: () => void) {}
    load(): void {}
    dispose(): void {}
  },
}));
vi.mock("../keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runNewSessionRemote: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../behavior", () => ({
  getBehavior: () => ({ resumeCommandLocal: "", resumeCommandRemote: "" }),
}));
vi.mock("../format", () => ({ formatTimestampSmart: () => "时间" }));

import { invoke } from "@tauri-apps/api/core";
import { HistoryView } from "./history";
import { runNewSessionRemote } from "../remote-launch-run";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const runNewRemote = runNewSessionRemote as unknown as ReturnType<typeof vi.fn>;

function proj(over: Record<string, unknown> = {}): Record<string, unknown> {
  return { projectPath: "/p", projectName: "P", projectDir: "pd", sessionCount: 2, starredCount: 0, hiddenCount: 0, lastActivity: 1, hasLive: false, ...over };
}
function entry(over: Record<string, unknown> = {}): Record<string, unknown> {
  return { sessionId: "s1", projectPath: "/p", projectName: "P", aiTitle: "T", firstUserExcerpt: "x", startedAt: 1, updatedAt: 1, jsonlPath: "/p/s1.jsonl", isLive: false, messageCountApprox: 1, starred: false, hidden: false, ...over };
}
function buildRow(view: HistoryView, e: Record<string, unknown>, p: Record<string, unknown>): HTMLElement {
  const row = (view as unknown as { buildEntryRow(e: unknown, p: unknown): HTMLElement }).buildEntryRow(e, p);
  document.body.appendChild(row);
  return row;
}
function menuItems(): HTMLButtonElement[] {
  return [...document.querySelectorAll<HTMLButtonElement>(".history-context-item")];
}
function menuItem(text: string): HTMLButtonElement | undefined {
  return menuItems().find((b) => b.textContent === text);
}

describe("HistoryView 共享动作表 + 右键菜单 (F96 #62)", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue({ starred: true, hidden: false, customTitle: null });
    runNewRemote.mockClear();
    document.body.replaceChildren();
  });

  it("inline 星标按钮仍触发 update_history_metadata（回归护栏）", async () => {
    const view = new HistoryView();
    const row = buildRow(view, entry({ starred: false }), proj());
    row.querySelector<HTMLButtonElement>(".history-star")!.click();
    await Promise.resolve();
    const call = invokeMock.mock.calls.find((c) => c[0] === "update_history_metadata");
    expect(call).toBeTruthy();
    expect(call![1]).toMatchObject({ sessionId: "s1", patch: { starred: true } });
  });

  it("右键条目 → 菜单出全套动作（本地）", () => {
    const view = new HistoryView();
    const row = buildRow(view, entry(), proj());
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 10, clientY: 10 }));
    const labels = menuItems().map((b) => b.textContent);
    expect(labels).toEqual(["在新终端 resume", "在该目录起新会话", "标星", "重命名", "隐藏", "删除…"]);
  });

  it("菜单「在该目录起新会话」本地 → invoke new_local_session", async () => {
    const view = new HistoryView();
    const row = buildRow(view, entry(), proj());
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    menuItem("在该目录起新会话")!.click();
    await Promise.resolve();
    const call = invokeMock.mock.calls.find((c) => c[0] === "new_local_session");
    expect(call).toBeTruthy();
    expect(call![1]).toMatchObject({ cwd: "/p", launcher: null });
  });

  it("菜单「在该目录起新会话」远端 → runNewSessionRemote（不 invoke new_local_session）", async () => {
    const view = new HistoryView();
    const row = buildRow(view, entry({ origin: "hostA" }), proj({ origin: "hostA" }));
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    menuItem("在该目录起新会话")!.click();
    // account-ux U3：远端新会话现经 withAccount(await fetchAccounts) → 多冲一轮宏任务排空微任务队列。
    await new Promise((r) => setTimeout(r, 0));
    expect(runNewRemote).toHaveBeenCalledTimes(1);
    expect(runNewRemote.mock.calls[0][0]).toBe("hostA");
    expect(runNewRemote.mock.calls[0][1]).toBe("/p");
    expect(invokeMock.mock.calls.some((c) => c[0] === "new_local_session")).toBe(false);
  });

  it("菜单 star 与 inline star 走同一 run（都触发 update_history_metadata）", async () => {
    const view = new HistoryView();
    const row = buildRow(view, entry({ starred: false }), proj());
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    menuItem("标星")!.click();
    await Promise.resolve();
    const call = invokeMock.mock.calls.find((c) => c[0] === "update_history_metadata");
    expect(call).toBeTruthy();
    expect(call![1]).toMatchObject({ patch: { starred: true } });
  });

  it("inline 删除（本地）二次确认 + invoke delete_history_session（回归护栏）", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const view = new HistoryView();
    const row = buildRow(view, entry(), proj());
    // delete 是最后一个 .history-action-danger
    row.querySelector<HTMLButtonElement>(".history-action-danger")!.click();
    await Promise.resolve();
    await Promise.resolve();
    const call = invokeMock.mock.calls.find((c) => c[0] === "delete_history_session");
    expect(call).toBeTruthy();
    expect(call![1]).toMatchObject({ sessionId: "s1", jsonlPath: "/p/s1.jsonl" });
    confirmSpy.mockRestore();
  });

  it("菜单开着按 Esc（经 handleEscape）→ 只关菜单，不误关整个历史视图", () => {
    const view = new HistoryView();
    (view as unknown as { isOpen: boolean }).isOpen = true; // 免全量 open() 的 invoke mock
    const row = buildRow(view, entry(), proj());
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    const inner = view as unknown as { openEntryMenu: HTMLElement | null; isOpen: boolean; handleEscape(): void };
    expect(inner.openEntryMenu).toBeTruthy();
    inner.handleEscape(); // 模拟 overlay dispatcher 的 Esc
    expect(inner.openEntryMenu).toBeNull(); // 菜单关了
    expect(inner.isOpen).toBe(true); // 视图没被误关
  });

  it("删除远端项目最后一个会话 → delete_remote_history_session + remoteCache 同步移除（F76 护栏）", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const view = new HistoryView();
    // 远端项目、仅 1 个会话 → 删掉即空 → 触发 this.projects + remoteCache 同步移除
    const p = proj({ origin: "hostA", sessionCount: 1 });
    (view as unknown as { remoteCache: { projects: unknown[]; loadedAt: number } }).remoteCache = {
      projects: [p],
      loadedAt: 1_000_000,
    };
    const row = buildRow(view, entry({ origin: "hostA" }), p);
    row.querySelector<HTMLButtonElement>(".history-action-danger")!.click();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    // 远端删除走 SFTP 命令 + 二次确认
    expect(confirmSpy).toHaveBeenCalledTimes(2);
    expect(invokeMock.mock.calls.some((c) => c[0] === "delete_remote_history_session")).toBe(true);
    // F76 承重不变式：删空的远端项目从 remoteCache 同步移除，否则 TTL 内重开会拼回幽灵
    const cache = (view as unknown as { remoteCache: { projects: unknown[] } }).remoteCache;
    expect(cache.projects.length).toBe(0);
    confirmSpy.mockRestore();
  });
});
