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
import {
  invalidateAccountsCache,
  resolvePendingLocalLaunches,
  __resetPendingLocalLaunchesForTests,
  __pendingLocalLaunchCountForTests,
  __resetLocalLaunchSnapshotForTests,
  __setLocalLaunchSnapshotForTests,
  type AccountsState,
} from "../accounts";

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

  // ═════════════════════════════════════════════════════════════════════════
  // `K-P5h` `KP5HD2`：**起会话方拿回 token → 会话跑起来后把 sid 反查出来 → 补写 pin**
  //
  // ⚠ 本条是这条链上**唯一驱动 `views/history.ts` 那一跳**的判据：
  //   `accounts.vitest.ts` 那两组量的是 `sidOfLaunch` 与待回填表**本身**，
  //   把 `rememberLocalLaunch(launchId, …)` 这一行从 `runNewSession` 里删掉，
  //   那两组**一条都不会红** —— 与 `D8 阻-1` 那次「判据的射程上界卡在下游」同形。
  // ═════════════════════════════════════════════════════════════════════════
  it("★★ `K-P5h`：本地起新会话 → 记住 token → 会话出现后把账号 pin 补写到反查出来的 sid 上", async () => {
    __resetPendingLocalLaunchesForTests();
    __resetLocalLaunchSnapshotForTests();
    invalidateAccountsCache();
    const TOKEN = "0198f0d2-1111-4222-8333-444455556666";
    const ACCT = {
      name: "acct-a",
      email: "a@x.edu",
      configDir: "/h/.claude-accts/acct-a",
      isDefault: true,
      mode: "isolated",
      exists: true,
      loggedIn: true,
    };
    invokeMock.mockImplementation((cmd: string) => {
      switch (cmd) {
        // 账号快照（`localLaunchAccountNameSync` 要它才说得出账号名）。
        case "list_local_accounts":
          return Promise.resolve({ available: true, error: null, meta: null, accounts: [ACCT], notice: null });
        case "load_config":
          return Promise.resolve({ accounts: { defaultName: "acct-a" } });
        case "list_last_accounts":
          return Promise.resolve({});
        // ★ 起会话这一跳**交回身份 token**（`KP5HD1` 那一格的前端这一侧）。
        case "new_local_session":
          return Promise.resolve(TOKEN);
        // ★ 会话真的跑起来了 —— 两条行里只有一条带着我们那个 token。
        case "list_local_session_accounts":
          return Promise.resolve({
            available: true,
            error: null,
            sessions: [
              { pid: 1, sessionId: "sid-other", cwd: "/w", configDir: null, account: null, bare: false, alive: true, launchId: "别人的" },
              { pid: 2, sessionId: "sid-new", cwd: "/p", configDir: null, account: null, bare: false, alive: true, launchId: TOKEN },
            ],
          });
        default:
          return Promise.resolve({});
      }
    });

    // ⚠ **快照要先喂热**：`primeLocalLaunchAccounts` 是**不等待**地踢出去的
    //   （多等一拍会撞那两条只放行一个微任务的 DOM 判据，见 `localLaunchAccountSync` 头注），
    //   所以起会话那一跳读到的很可能还是冷快照 —— 那是一条**已登记的诚实边界**，不是本条要量的东西。
    //   本条量的是「**说得出账号名时，那次拉起被记住了**」。
    const snap: AccountsState = {
      origin: "__local__",
      available: true,
      error: null,
      notice: null,
      meta: null,
      accounts: [ACCT],
      defaultName: "acct-a",
    };
    __setLocalLaunchSnapshotForTests(snap, {});

    const view = new HistoryView();
    const row = buildRow(view, entry(), proj());
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    menuItem("在该目录起新会话")!.click();
    // 冲一轮宏任务：起会话那一跳的 `await` 要落地。
    await new Promise((r) => setTimeout(r, 0));

    // ① 起会话方**记住了**这次拉起（这一刻它手上只有 token，没有 sid）。
    expect(
      __pendingLocalLaunchCountForTests(),
      "起完新会话没有挂上待回填 —— `rememberLocalLaunch` 那一行被摘了，\n" +
        "或者交回来的 token 是空的（`new_local_session` 还在回 `void`）",
    ).toBe(1);

    // ② 会话出现之后（生产上由 `main.ts` 的 `session-started` 事件触发这一跳），
    //    sid 被反查出来、pin 落到**那一条**上。
    await resolvePendingLocalLaunches();
    const pin = invokeMock.mock.calls
      .filter((c) => c[0] === "update_history_metadata")
      .map((c) => c[1]);
    expect(pin, "反查出 sid 之后没有补写账号 pin").toHaveLength(1);
    // 🔴 判别格：落在 `sid-new` 上而不是 `sid-other` —— 这一格只有 token 说得出来。
    expect(pin[0]).toMatchObject({ sessionId: "sid-new", patch: { lastAccount: "acct-a" } });
    expect(__pendingLocalLaunchCountForTests()).toBe(0);
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

  // F05 Phase D 审计：runNewSession 走 `withAccount(origin, null, ..., {follow:{}})`——恒跟随
  // 解析（新会话无显式账号入口）。此前本文件从未验证过跟随解析真命中账号时，
  // `(cd, an) => runNewSessionRemote(..., cd, an)` 是否真把 an 转传。
  it("菜单「在该目录起新会话」远端（跟随解析命中当前账号）→ runNewSessionRemote 收到真实 configDir + accountName", async () => {
    // fetchAccounts 有模块级缓存(30s TTL)——上一条用例已经用默认 mock 值给 "hostA" 缓存过一次
    // (不含 available 字段的错误响应)，不清掉这里会命中陈旧缓存、永远走不到下面的自定义 mock。
    invalidateAccountsCache();
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_remote_accounts") {
        return Promise.resolve({
          available: true,
          error: null,
          meta: { enabled: true, acctsDir: "/h/.claude-accts", manifestPath: "/h/.claude-accts/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null },
          accounts: [{ name: "z", email: "z@x.edu", configDir: "/h/.claude-accts/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true }],
        });
      }
      return Promise.resolve(undefined);
    });
    const view = new HistoryView();
    const row = buildRow(view, entry({ origin: "hostA" }), proj({ origin: "hostA" }));
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    menuItem("在该目录起新会话")!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(runNewRemote).toHaveBeenCalledWith("hostA", "/p", "", { configDir: "/h/.claude-accts/z", accountName: "z", modelOverride: undefined });
    invalidateAccountsCache(); // fetchAccounts 有模块级缓存,别泄漏进同文件其它测试
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
