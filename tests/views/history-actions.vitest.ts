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
vi.mock("../../src/views/session-viewer", () => ({
  SessionViewer: class {
    element = document.createElement("div");
    constructor(_c: () => void) {}
    load(): void {}
    dispose(): void {}
  },
}));
vi.mock("../../src/keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../../src/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../src/remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runNewSessionRemote: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../src/behavior", () => ({
  getBehavior: () => ({ resumeCommandLocal: "", resumeCommandRemote: "" }),
}));
vi.mock("../../src/format", () => ({ formatTimestampSmart: () => "时间" }));

import { invoke } from "@tauri-apps/api/core";
import { HistoryView } from "../../src/views/history";
import { runNewSessionRemote } from "../../src/remote-launch-run";
import type { AccountsState } from "../../src/accounts";
import { invalidateAccountsCache } from "../../src/account-reads";
import { __resetLocalLaunchSnapshotForTests, __setLocalLaunchSnapshotForTests } from "../../src/launch-account";
import { resolvePendingLocalLaunches, __resetPendingLocalLaunchesForTests, __pendingLocalLaunchCountForTests } from "../../src/local-launch-backfill";
import { historyCalls, isChanCall, launchRenderShim, linesReply, localLaunchCalls, withAccountReads, withHistoryReads } from "../test-support/chan-fake";
import { LOCAL_ORIGIN } from "../../src/ipc/origin";
import { answerAskDialog, answerAskText, askDialogText, noAskDialog } from "../test-support/ask-dialog-driver.ts";
import { showActionFailureToast } from "../../src/error-toast";
import { copyText } from "../../src/copy-table";

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
    // 〔C4d〕改注解 / 上次账号那几问改走通道（问本机常驻后端）⇒ 经 chan-fake 译回旧名字再答。
    invokeMock.mockImplementation(withHistoryReads(launchRenderShim(() => Promise.resolve({ starred: true, hidden: false, customTitle: null }))));
    runNewRemote.mockClear();
    document.body.replaceChildren();
  });

  it("inline 星标按钮仍触发 update_history_metadata（回归护栏）", async () => {
    const view = new HistoryView();
    const row = buildRow(view, entry({ starred: false }), proj());
    row.querySelector<HTMLButtonElement>(".history-star")!.click();
    await Promise.resolve();
    const call = historyCalls(invokeMock.mock.calls, "update_history_metadata")[0];
    expect(call).toBeTruthy();
    expect(call!).toMatchObject({ sessionId: "s1", patch: { starred: true } });
  });

  // 〔CFG1 · 4D〕星标 / 改名 / 隐藏写失败要出声（E §3.3：从前只 `console.warn`，点了什么都没变、也不说）。
  //   守的要求：`INVARIANTS §12`「关键失败必须 …… 状态栏 toast」。期望标题从文案表取（表是对外文案的唯一来源）。
  it("〔CFG1〕星标 / 改名 / 隐藏写失败 ⇒ 各恰好一条 toast、标题是表里那句", async () => {
    const toast = vi.mocked(showActionFailureToast);
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "chan_call") throw new Error("盘写不进去");
      return undefined;
    });
    vi.spyOn(console, "warn").mockImplementation(() => {});
    // 〔CFG1 × W5-UI 合并〕改名那一格原先靠 `window.prompt` 给新标题；W5-UI 之后改名走应用内 `askText`，要在对话框里答。
    const view = new HistoryView();
    for (const [label, key] of [
      ["标星", "history.star.failed"],
      ["重命名", "history.rename.failed"],
      ["隐藏", "history.hide.failed"],
    ] as const) {
      toast.mockClear();
      document.body.replaceChildren();
      const row = buildRow(view, entry(), proj());
      row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
      menuItem(label)!.click();
      if (label === "重命名") await answerAskText("新标题");
      await new Promise((r) => setTimeout(r, 0));
      expect(toast.mock.calls.map((a) => a[0]), label).toEqual([copyText(key)]);
    }
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
    await new Promise((r) => setTimeout(r, 0));
    const call = localLaunchCalls(invokeMock.mock.calls, "new_local_session")[0];
    expect(call).toBeTruthy();
    expect(call).toMatchObject({ cwd: "/p", launcher: null });
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
      configDir: "/h/.claude-alt/acct-a",
      isDefault: true,
      mode: "isolated",
      exists: true,
      loggedIn: true,
      authKind: "subscription" as const,
      authReady: true,
    };
    invokeMock.mockImplementation(withHistoryReads(withAccountReads(launchRenderShim((cmd: string, args: unknown) => {
      // ★ 会话真的跑起来了 —— 两条行里只有一条带着我们那个 token。
      //   〔C4a〕经通道问本机后端 `accounts-sessions`（原先是 E79 那条已退役的本机 Tauri 命令）。
      if (isChanCall(cmd, args, "accounts-sessions")) {
        return Promise.resolve(
          linesReply([
            { pid: 1, sessionId: "sid-other", cwd: "/w", configDir: null, account: null, bare: false, alive: true, launchId: "别人的" },
            { pid: 2, sessionId: "sid-new", cwd: "/p", configDir: null, account: null, bare: false, alive: true, launchId: TOKEN },
          ]),
        );
      }
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
        default:
          return Promise.resolve({});
      }
    }))));

    // ⚠ **快照要先喂热**：`primeLocalLaunchAccounts` 是**不等待**地踢出去的
    //   （多等一拍会撞那两条只放行一个微任务的 DOM 判据，见 `localLaunchAccountSync` 头注），
    //   所以起会话那一跳读到的很可能还是冷快照 —— 那是一条**已登记的诚实边界**，不是本条要量的东西。
    //   本条量的是「**说得出账号名时，那次拉起被记住了**」。
    const snap: AccountsState = {
      origin: LOCAL_ORIGIN, // 〔C4b〕账号面的本机就是 `LOCAL_ORIGIN`（`"__local__"` 已退役）
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
    const pin = historyCalls(invokeMock.mock.calls, "update_history_metadata");
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
    expect(localLaunchCalls(invokeMock.mock.calls, "new_local_session")).toEqual([]);
  });

  // F05 Phase D 审计：runNewSession 走 `withAccount(origin, null, ..., {follow:{}})`——恒跟随
  // 解析（新会话无显式账号入口）。此前本文件从未验证过跟随解析真命中账号时，
  // `(cd, an) => runNewSessionRemote(..., cd, an)` 是否真把 an 转传。
  it("菜单「在该目录起新会话」远端（跟随解析命中当前账号）→ runNewSessionRemote 收到真实 configDir + accountName", async () => {
    // fetchAccounts 有模块级缓存(30s TTL)——上一条用例已经用默认 mock 值给 "hostA" 缓存过一次
    // (不含 available 字段的错误响应)，不清掉这里会命中陈旧缓存、永远走不到下面的自定义 mock。
    invalidateAccountsCache();
    invokeMock.mockImplementation(withHistoryReads(withAccountReads((cmd: string) => {
      if (cmd === "list_remote_accounts") {
        return Promise.resolve({
          available: true,
          error: null,
          meta: { enabled: true, acctsDir: "/h/.claude-alt", manifestPath: "/h/.claude-alt/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null },
          accounts: [{ name: "z", email: "z@x.edu", configDir: "/h/.claude-alt/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true, authKind: "subscription", authReady: true }],
        });
      }
      return Promise.resolve(undefined);
    })));
    const view = new HistoryView();
    const row = buildRow(view, entry({ origin: "hostA" }), proj({ origin: "hostA" }));
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    menuItem("在该目录起新会话")!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(runNewRemote).toHaveBeenCalledWith("hostA", "/p", "", { configDir: "/h/.claude-alt/z", accountName: "z", modelOverride: undefined });
    invalidateAccountsCache(); // fetchAccounts 有模块级缓存,别泄漏进同文件其它测试
  });

  // 〔C4d · 第四波 4B〕BACKLOG E35：「留空恢复默认」要真的清掉标题 —— 清空传**空串**（缺格 / `null` 在后端 patch 里都是「不改」）。
  //   守的要求：主会话 09-25 裁（`调研/第四波记录/C4d.md`「主会话裁」第 2 条）注解读写者换成本机常驻后端，patch 语义逐格照搬
  //   monitor 那一份（`null` = 不改）⇒ 界面这一侧要传对的那一个值。
  it("重命名留空 ⇒ 交的是空串（清掉），不是 null（后端当「不改」）", async () => {
    const view = new HistoryView();
    const row = buildRow(view, entry({ customTitle: "旧名" }), proj());
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    menuItem("重命名")!.click();
    await Promise.resolve();
    // 〔W5-UI〕问名字走应用内对话框（初值 = 现名）。
    expect(document.querySelector<HTMLInputElement>('[role="dialog"] input')!.value).toBe("旧名");
    await answerAskText("   ");
    const call = historyCalls(invokeMock.mock.calls, "update_history_metadata")[0];
    expect(call, "重命名一发都没出去").toBeTruthy();
    expect(call!.patch).toEqual({ customTitle: "" });
  });

  it("菜单 star 与 inline star 走同一 run（都触发 update_history_metadata）", async () => {
    const view = new HistoryView();
    const row = buildRow(view, entry({ starred: false }), proj());
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    menuItem("标星")!.click();
    await Promise.resolve();
    const call = historyCalls(invokeMock.mock.calls, "update_history_metadata")[0];
    expect(call).toBeTruthy();
    expect(call!).toMatchObject({ patch: { starred: true } });
  });

  it("inline 删除（本地）二次确认 + invoke delete_history_session（回归护栏）", async () => {
    const view = new HistoryView();
    const row = buildRow(view, entry(), proj());
    // delete 是最后一个 .history-action-danger
    row.querySelector<HTMLButtonElement>(".history-action-danger")!.click();
    await Promise.resolve();
    expect(historyCalls(invokeMock.mock.calls, "delete_history_session").length > 0, "还没答就删了").toBe(false);
    await answerAskDialog(true);
    const call = historyCalls(invokeMock.mock.calls, "delete_history_session")[0];
    expect(call).toBeTruthy();
    // 🔴 〔步 12·C 09-20〕`origin` 是**新加的必填项**，而且本机要逐字送 `"<local>"`。
    //    ⚠ `toMatchObject` 是**子集**匹配 ⇒ 光靠它，调用点漏送 origin 这一条照样绿。
    //      所以下面那格单独把 origin 断死（这一条正是本仓治过的「子集匹配假绿」那一形）。
    // 〔MIG-3b〕经通道直说那台后端 `files-delete-session`：只交 sid（落点由后端按 sid 找），路径不过线。
    expect(call!).toMatchObject({ sessionId: "s1" });
    expect(
      (call! as { origin?: unknown }).origin,
      "本机删除没送 `<local>` —— Rust 侧 `Origin::route` 会拒（`null`/缺省都不是本机）",
    ).toBe("<local>");
  });

  it("〔W5-UI〕inline 删除（本地）确认框点取消 ⇒ 一发都不出去（答案是异步到的，与真 app 同形）", async () => {
    const view = new HistoryView();
    const row = buildRow(view, entry(), proj());
    row.querySelector<HTMLButtonElement>(".history-action-danger")!.click();
    await Promise.resolve();
    await answerAskDialog(false);
    expect(historyCalls(invokeMock.mock.calls, "delete_history_session").length > 0).toBe(false);
  });

  // 〔FW1 · 第四波 4D · 主会话裁 D-e〕删会话前看活不活：活着（条目说活 / tab 栏里活）⇒ 多问一句；说不清（`isLive: null`）⇒ 也多问；
  //   确定不活 ⇒ 照原来那一问。多问那句答「不」⇒ 一趟 delete 都不发。异源：问了什么由文案表现取、发没发由 invoke 记录判。
  it("〔FW1〕删会话前看活不活：活 / 说不清多问一句，不活照旧；多问那句答不 ⇒ 不删", async () => {
    // 〔W5-UI 之后〕问的是应用内对话框：当用户读正文、点真按钮（`ask-dialog-driver`），答案异步到，与真 app 同形。
    const runOnce = async (over: Record<string, unknown>, liveInTabs: boolean, answers: boolean[]) => {
      invokeMock.mockClear();
      const asked: string[] = [];
      const view = new HistoryView();
      view.liveInTabs = () => liveInTabs;
      const row = buildRow(view, entry(over), proj());
      row.querySelector<HTMLButtonElement>(".history-action-danger")!.click();
      await Promise.resolve();
      for (const ok of answers) {
        if (noAskDialog()) break;
        asked.push(askDialogText());
        await answerAskDialog(ok);
      }
      expect(noAskDialog(), "答完了还挂着一个对话框（问的比预期多）").toBe(true);
      const deleted = historyCalls(invokeMock.mock.calls, "delete_history_session").length > 0;
      return { asked, deleted };
    };
    const live = copyText("sessionState.deleteLive.confirm", { label: "T" });
    const unknown = copyText("sessionState.deleteUnknown.confirm", { label: "T" });
    const plain = copyText("history.delete.confirmLocal", { label: "T" });
    expect(await runOnce({ isLive: true }, false, [true, true])).toEqual({ asked: [live, plain], deleted: true });
    expect(await runOnce({ isLive: false }, true, [true, true]), "tab 栏里活着却没多问").toEqual({ asked: [live, plain], deleted: true });
    expect(await runOnce({ isLive: null }, false, [true, true])).toEqual({ asked: [unknown, plain], deleted: true });
    expect(await runOnce({ isLive: false }, false, [true])).toEqual({ asked: [plain], deleted: true });
    expect(await runOnce({ isLive: true }, false, [false]), "多问那句答了不，还是删了").toEqual({ asked: [live], deleted: false });
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

  // 🔴 〔步 12·C 09-20〕标题里的命令名跟上：`delete_remote_history_session` 已退役，〔散文墓碑〕
  //    远端删除走的是**同一条** `delete_history_session`，只是 `origin` 是那台机器。
  it("删除远端项目最后一个会话 → delete_history_session(origin=hostA) + remoteCache 同步移除（F76 护栏）", async () => {
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
    // 远端删除走 SFTP 命令 + 二次确认（两次都得答「确定」才删）
    await answerAskDialog(true);
    expect(historyCalls(invokeMock.mock.calls, "delete_history_session").length > 0, "只答了一次就删了").toBe(false);
    await answerAskDialog(true);
    // 🔴 判的是「**带着那台机器的 origin** 调了那条命令」——只判命令名不够：
    //    合并之后本机与远端**同名**，光判名字的话「远端删除误走了本机那条路」不会红。
    const remoteCall = historyCalls(invokeMock.mock.calls, "delete_history_session").find(
      (c) => c.origin === "hostA",
    );
    expect(
      remoteCall,
      "没有一趟 `delete_history_session` 带着 `origin: \"hostA\"` —— " +
        "要么命令没发，要么 origin 丢了（丢了就会去删**本机**的同名路径）",
    ).toBeTruthy();
    // 反向：这一趟**不许**同时冒出一条本机的删除。
    expect(
      historyCalls(invokeMock.mock.calls, "delete_history_session").filter((c) => c.origin === "<local>"),
      "远端删除顺手也发了一条本机删除",
    ).toEqual([]);
    // F76 承重不变式：删空的远端项目从 remoteCache 同步移除，否则 TTL 内重开会拼回幽灵
    const cache = (view as unknown as { remoteCache: { projects: unknown[] } }).remoteCache;
    // 〔合并 MIG-3b × MIG-2〕删会话经通道（`session-writes.ts::deleteSession`）、替身又多包了一层起会话翻译 ⇒ 应答晚几拍到；等它落定再判。
    await vi.waitFor(() => expect(cache.projects.length).toBe(0));
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// `K-R46`：**历史页 resume 要把 tmux 会话名铸出来传下去**（行为，不是文本）
// ═════════════════════════════════════════════════════════════════════════════
//
// 病（09-10 现打）：后端**故意**拒绝自己铸名 —— `history.rs` 的 `NO_TMUX_NAME` 注释逐字
// 「在这里补一个铸造口 = 第三次犯同一个错」⇒ 名字只能由前端传下去。
// `tabs.ts` 那条 tab 栏 resume 早就传了，**历史页这条（与搜索卡片共用同一个 `runResume`）
// 一个字都没传** ⇒ `render_local_ccm` 早退 ⇒ 后端如实降级回旧路 ⇒ 起出来的会话
// **不在具名 tmux 容器里**，右键那两条（「杀死会话（kill tmux …）」「就地 resume（复用空 tmux …）」）
// 对它一条都给不出来。
//
// ⚠ **为什么非要行为判据**：`ipc/commands.vitest.ts` 那条同族判据量的是
//   「`tmuxName` 那行字在不在」—— 把值换成恒 `null` 它照绿（`D4` 两刀已经证过这一形）。
//   本组量的是**那一发 `invoke` 载荷里真正的那个值**。
//
// ⚠ **本组买不到什么**：它止于「monitor 发出去的载荷里有这个名字」。
//   「后端真的用它建了一个 tmux 容器」要真 tmux（POSIX，且账号那一格还得说得出话来），归 e2e；
//   〔`K-R53` 09-11 订正这一句的现在时：原文写「还得是显式『账号 0』—— 具名账号与『没表态』
//    都 §35 降级」。**具名那一半今天不成立了** —— 具名账号带上名字之后渲染得出来
//    （`LaunchAccount::Named::name`）。今天仍然降级的是：只说得出目录 · 没表态（继承）·
//    这个号走中转 · 没装 ccm · Windows。逐格表住 `history.rs::tests::
//    every_local_account_shape_gets_a_named_verdict_from_the_backend_path`。〕
//   「`↗ 调出终端` 按钮真的能用」更远：本机那一支走的是 Win32 HWND 缓存
//   （`bind::activate` 在非 Windows 上逐字回 `only supported on Windows`），**与这个名字无关**。
//   别把本组的绿读成「按钮好了」。
describe("K-R46：历史页 resume 的 tmux 名（行为）", () => {
  /** 那一发 `resume_history_session` 的载荷。 */
  function resumePayload(): Record<string, unknown> {
    const call = localLaunchCalls(invokeMock.mock.calls, "resume_history_session")[0];
    expect(call, "一次 `resume_history_session` 都没发出去 —— 主路没走到，下面在空转").toBeTruthy();
    return call!;
  }

  /** 让 `list_local_tmux` 回一份本机 tmux 快照（`null` = 不知道）。 */
  function serveLocalTmux(names: string[] | null): void {
    invokeMock.mockImplementation(withHistoryReads(launchRenderShim((cmd: string) => {
      if (cmd === "list_local_tmux") {
        return Promise.resolve(
          names === null
            ? null
            : names.map((name) => ({
                name,
                path: "/p",
                command: "claude",
                attached: false,
                windows: 1,
                sid: null,
              })),
        );
      }
      return Promise.resolve(undefined);
    })));
  }

  async function clickResume(): Promise<void> {
    const view = new HistoryView();
    const row = buildRow(view, entry(), proj());
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    const btn = menuItem("在新终端 resume");
    expect(btn, "右键菜单里没有「在新终端 resume」—— 文案改了，下面整条在空转").toBeTruthy();
    btn!.click();
    await new Promise((r) => setTimeout(r, 0));
  }

  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    __resetLocalLaunchSnapshotForTests();
    invalidateAccountsCache(); // 模块级缓存，别让上面几组的 hostA 快照漏进来
    document.body.replaceChildren();
  });

  it("★★ 基名被占 ⇒ 载荷里的 `tmuxName` **让到了 `-2`**（证明它真过了铸造口，不是拼出来的）", async () => {
    // 判别格：基名 `p-cc`（`K-R96`：`basename(cwd)` + `-cc`，`cwd` 夹具是 `/p`）已经被占着。
    // 恒回一个常量、或自己拼一份基名规则（那正是 F13 修掉的坑），这一格都过不了。
    serveLocalTmux(["p-cc", "别人的-cc"]);
    await clickResume();
    expect(
      resumePayload().tmuxName,
      "历史页 resume 的载荷里没有让过位的 tmux 名 ——\n" +
        "要么名字压根没传（后端 `NO_TMUX_NAME` 早退 ⇒ 会话不进具名容器），\n" +
        "要么没过 `remote-launch.ts::mintTmuxName`（全仓唯一带撞名避让的铸造口）：\n" +
        "另一处精心让出 `-2`，你直接撞上去 —— 那就是 issue #76 的形状。",
    ).toBe("p-cc-2");
  });

  it("★ 没被占 ⇒ 就是基名本身（反过来钉住：它不是**恒**加后缀）", async () => {
    serveLocalTmux(["别人的-cc"]);
    await clickResume();
    expect(resumePayload().tmuxName).toBe("p-cc");
  });

  it("★★ KR96D3：名字读得出是哪个项目，且**一个 sid 片段都没有**", async () => {
    // 夹具的 sid 是 `s1`、cwd 是 `/p` ⇒ 名字该是 `p-cc`（从 cwd 来），不是 `s1-cc`。
    // 用户 `R55` 裁定一逐字：「**要是可读的名字 / 不要id**」。
    serveLocalTmux([]);
    await clickResume();
    const name = String(resumePayload().tmuxName);
    expect(name).toBe("p-cc");
    expect(
      name.includes("s1"),
      "会话名里还带着 sid —— sid 的载体是 tmux 的 `@ccm_sid`，不是名字",
    ).toBe(false);
    // 而 sid **必须还在载荷里**（后端拿它去 `set-option @ccm_sid`）：
    // 把它一起去掉 ⇒ 这一行当场红。
    expect(resumePayload().sessionId).toBe("s1");
  });

  it("★★ 本机 tmux 快照是 `null`（**不知道**）⇒ `tmuxName` 传 `null`，**绝不硬铸**", async () => {
    // `list_local_tmux` 回 `null` = 本机后端通道没起 / 还没推过帧 = 不知道，
    // **不是**「一个名字都没占」。此时硬铸就是「不避让」⇒ issue #76
    //「静默接进第一个会话，而用户以为开了新的」。诚实的做法是不传，让后端降级回旧路。
    serveLocalTmux(null);
    await clickResume();
    expect(
      resumePayload().tmuxName,
      "不知道本机占了哪些名字时铸了一个 —— 那是把「不知道」当成了「空集」",
    ).toBeNull();
    // 反空真：这一趟主路**真的走到了**（否则上面那条是「什么都没发生」的空真）。
    expect(localLaunchCalls(invokeMock.mock.calls, "resume_history_session").length > 0).toBe(true);
  });

  it("★ 远端那条路**不受影响**：不查本机 tmux、也不发本机 resume", async () => {
    serveLocalTmux(["s1-cc"]);
    const view = new HistoryView();
    const row = buildRow(view, entry({ origin: "hostA" }), proj({ origin: "hostA" }));
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    menuItem("在新终端 resume")!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(localLaunchCalls(invokeMock.mock.calls, "resume_history_session").length > 0).toBe(false);
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "list_local_tmux"),
      "远端 resume 去查了本机的 tmux 名 —— 那是拿本机的事实去铸远端的名字",
    ).toBe(false);
  });
});
