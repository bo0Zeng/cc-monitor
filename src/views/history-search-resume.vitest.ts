// F85（#44）搜索结果卡片直接 resume 的 jsdom 测试。
//
// 复用 F96 的 runResume（hasEntry:false 的 ctx）。锁：本地/远端搜索卡片 resume 各触发正确 IPC
// + 参数；点 resume 不误开只读 viewer。照 history-actions.vitest.ts mock 骨架。

import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
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
import { runRemoteResume } from "../remote-launch-run";
import { invalidateAccountsCache } from "../accounts";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const runRemote = runRemoteResume as unknown as ReturnType<typeof vi.fn>;

function searchSession(over: Record<string, unknown> = {}): Record<string, unknown> {
  return { sessionId: "s1", projectPath: "/p", projectName: "P", jsonlPath: "/p/s1.jsonl", title: "T", updatedAt: 1, hitCount: 1, hits: [], ...over };
}
function buildCard(view: HistoryView, s: Record<string, unknown>): HTMLElement {
  const g = (view as unknown as { buildSearchSession(s: unknown): HTMLElement }).buildSearchSession(s);
  document.body.appendChild(g);
  return g;
}

describe("HistoryView 搜索卡片 resume (F85 #44)", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    runRemote.mockClear();
    document.body.replaceChildren();
  });

  it("本地搜索卡片 resume → invoke resume_history_session（不开 viewer）", async () => {
    const view = new HistoryView();
    const card = buildCard(view, searchSession());
    const btn = card.querySelector<HTMLButtonElement>(".search-session-resume")!;
    expect(btn).toBeTruthy();
    btn.click();
    await Promise.resolve();
    const call = invokeMock.mock.calls.find((c) => c[0] === "resume_history_session");
    expect(call).toBeTruthy();
    expect(call![1]).toMatchObject({ sessionId: "s1", cwd: "/p", launcher: null });
    // 点 resume 不误开只读 viewer
    expect((view as unknown as { viewer: unknown }).viewer).toBeNull();
  });

  it("远端搜索卡片 resume → runRemoteResume（不 invoke 本地 resume）", async () => {
    const view = new HistoryView();
    const card = buildCard(view, searchSession({ origin: "hostA" }));
    card.querySelector<HTMLButtonElement>(".search-session-resume")!.click();
    // account-ux U3：远端 resume 现经 withAccount(await fetchAccounts) → 多冲一轮宏任务排空微任务队列。
    await new Promise((r) => setTimeout(r, 0));
    expect(runRemote).toHaveBeenCalledTimes(1);
    expect(runRemote.mock.calls[0].slice(0, 3)).toEqual(["hostA", "s1", "/p"]);
    expect(invokeMock.mock.calls.some((c) => c[0] === "resume_history_session")).toBe(false);
  });

  // F05 Phase D 审计：本文件此前只用 `.slice(0,3)` 断言前三个参数，从未验证过
  // history.ts::runResume 的 `(cd, an) => runRemoteResume(..., cd, an)` 接线本身。搜索卡片的
  // resume 按钮不带显式账号（`cardCtx.account` 恒 undefined，见 `buildSearchSession`），故这里
  // 走的是"跟随解析"分支——补一条"跟随解析真命中当前账号时 accountName 真的转传"的集成测试。
  it("远端搜索卡片 resume（跟随解析命中当前账号）→ runRemoteResume 收到真实 configDir + accountName", async () => {
    invalidateAccountsCache(); // fetchAccounts 有模块级缓存，防陈旧缓存挡住下面的自定义 mock
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_remote_accounts") {
        return Promise.resolve({
          available: true,
          error: null,
          meta: { enabled: true, acctsDir: "/h/.claude-accts", manifestPath: "/h/.claude-accts/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null },
          accounts: [{ name: "z", email: "z@x.edu", configDir: "/h/.claude-accts/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true }],
        });
      }
      if (cmd === "list_last_accounts") return Promise.resolve({}); // 无既有 pin → 落 current
      return Promise.resolve(undefined);
    });
    const view = new HistoryView();
    const card = buildCard(view, searchSession({ origin: "hostA" }));
    card.querySelector<HTMLButtonElement>(".search-session-resume")!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(runRemote).toHaveBeenCalledWith("hostA", "s1", "/p", "", { configDir: "/h/.claude-accts/z", accountName: "z", modelOverride: undefined });
    invalidateAccountsCache(); // fetchAccounts 有模块级缓存,别泄漏进同文件/同 worker 的其它测试
  });
});
