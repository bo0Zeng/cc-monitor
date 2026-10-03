/**
 * tab 栏批量停 / 起交给那几台后端：同一台的那几个一次调用、各台各发一次（一台失败不挡别台）· 账号跟随每台只取一次清单 ·
 * 每一个的结局说成单个那条会说的那句 · 开终端那一形逐个开窗。后端答什么由替身给（`chan_call` 那一跳），tmux / 会话一个都不起。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/account-reads", () => ({ fetchAccounts: vi.fn() }));
vi.mock("../../../src/frontend/ui/history-reads", () => ({ lastAccounts: vi.fn() }));
vi.mock("../../../src/frontend/ui/behavior", () => ({ getBehavior: vi.fn() }));
vi.mock("../../../src/frontend/ui/remote-config", () => ({ resolveResumeCommand: vi.fn() }));
vi.mock("../../../src/frontend/ui/account-prefs", () => ({ getModelForAccount: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-open", () => ({ openTerminal: vi.fn() }));
vi.mock("../../../src/frontend/ui/launch-account", async (orig) => {
  const real = await orig<typeof import("../../../src/frontend/ui/launch-account")>();
  return {
    ...real,
    localFollowPlan: vi.fn(() => ({ kind: "silent" })),
    primeLocalLaunchAccounts: vi.fn(),
    recordLastAccount: vi.fn(),
    recordLocalLaunchAccount: vi.fn(),
  };
});

import { invoke } from "@tauri-apps/api/core";
import { stopMany, startMany, decodeBatch } from "../../../src/frontend/ui/tab-batch-run";
import { killRefusals } from "../../../src/frontend/ui/tmux-control";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { fetchAccounts } from "../../../src/frontend/ui/account-reads";
import { lastAccounts } from "../../../src/frontend/ui/history-reads";
import { getBehavior } from "../../../src/frontend/ui/behavior";
import { resolveResumeCommand } from "../../../src/frontend/ui/remote-config";
import { getModelForAccount } from "../../../src/frontend/ui/account-prefs";
import { openTerminal } from "../../../src/frontend/ui/terminal-open";
import { recordLastAccount } from "../../../src/frontend/ui/launch-account";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { AccountsState } from "../../../src/frontend/ui/accounts";
import { chanArgsJson, chanReply, NO_CHANNEL, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const tab = (sid: string, origin: string, cwd = `/w/${sid}`): Tab =>
  ({ sessionId: sid, origin, projectDir: cwd, title: `T-${sid}` }) as unknown as Tab;

/** 后端那一跳：`(origin, op)` ⇒ 怎么答（成品 / 抛）。记下每一次调用。 */
let calls: [string, string, Record<string, unknown>][] = [];
function backend(answer: (origin: string, op: string, args: Record<string, unknown>) => unknown): void {
  invokeMock.mockImplementation(async (cmd: string, a: unknown) => {
    if (cmd !== "chan_call") return undefined; // 问握手那一跳（置灰用）等别的：不答
    const c = a as ChanCallArgs;
    const args = chanArgsJson(c) as Record<string, unknown>;
    calls.push([c.origin, c.op, args]);
    const got = answer(c.origin, c.op, args);
    if (got instanceof Error || (typeof got === "object" && got !== null && "err" in got)) throw got;
    return chanReply(got);
  });
}

const res = (sid: string, outcome: string, why: string | null = null, extra: Record<string, unknown> = {}) => ({
  sid,
  outcome,
  why,
  detail: "",
  session: null,
  bus: null,
  cmd: null,
  ...extra,
});

beforeEach(() => {
  vi.clearAllMocks();
  invokeMock.mockReset();
  calls = [];
  vi.mocked(getBehavior).mockResolvedValue({ resumeCommandLocal: "", resumeCommandRemote: "" } as never);
  vi.mocked(resolveResumeCommand).mockResolvedValue("");
  vi.mocked(getModelForAccount).mockResolvedValue("opus");
  vi.mocked(lastAccounts).mockResolvedValue({ c: "work", e: "gone" });
  vi.mocked(fetchAccounts).mockResolvedValue({
    origin: "r1",
    available: true,
    error: null,
    oldBackend: false,
    meta: null,
    defaultName: "work",
    notice: null,
    accounts: [{ name: "work", email: "", configDir: "/h/.cc/work", isDefault: true, mode: "isolated", exists: true, loggedIn: true, authKind: "subscription", authReady: true }],
  } as unknown as AccountsState);
});

describe("批量停：每台一次、逐个答", () => {
  it("同一台的几个一次调用、各台各一次；一台不通只挡它自己；每一个说的就是单个那条的那一句", async () => {
    backend((origin, _op, args) => {
      if (origin === "r2") throw NO_CHANNEL;
      const sids = args.sids as string[];
      if (origin === LOCAL_ORIGIN) return { results: [res(sids[0], "done", null, { session: "a-cc" }), res(sids[1], "skipped", "not_in_tmux")] };
      return { results: [res(sids[0], "failed", "too_many_windows", { session: "c-cc", detail: "2" })] };
    });
    const out = await stopMany([tab("a", LOCAL_ORIGIN), tab("c", "r1"), tab("b", LOCAL_ORIGIN), tab("d", "r2")]);
    expect(calls.map(([o, op, a]) => [o, op, a])).toEqual([
      [LOCAL_ORIGIN, "sessions-stop", { sids: ["a", "b"] }],
      ["r1", "sessions-stop", { sids: ["c"] }],
      ["r2", "sessions-stop", { sids: ["d"] }],
    ]);
    const by = Object.fromEntries(out.map((o) => [o.sid, o]));
    expect(by.a).toEqual({ sid: "a", outcome: "done", why: "" });
    expect(by.b).toEqual({ sid: "b", outcome: "skipped", why: copyText("tabBatch.why.notInTmux", { machine: "本机" }) });
    expect(by.c).toEqual({ sid: "c", outcome: "failed", why: killRefusals("c-cc").byCode("too_many_windows", "2") });
    expect(by.d.outcome).toBe("failed");
    expect(by.d.why).not.toBe("");
  });

  it("应答形状对不上（缺一格 · 个数 / 次序不符）⇒ 那一台整批记失败，不猜", () => {
    expect(() => decodeBatch("r1", "sessions-stop", ["a"], { results: [{ sid: "a", outcome: "done" }] })).toThrow();
    expect(() => decodeBatch("r1", "sessions-stop", ["a", "b"], { results: [res("b", "done"), res("a", "done")] })).toThrow();
    expect(() => decodeBatch("r1", "sessions-stop", ["a"], { results: [{ ...res("a", "done"), extra: 1 }] }), "多一格").toThrow();
    expect(decodeBatch("r1", "sessions-stop", ["a"], { results: [res("a", "done")] })[0].outcome, "正控").toBe("done");
  });
});

describe("批量起：账号跟随同单个那条、每台只取一次清单", () => {
  it("在 tmux 里起：上次用的号照跟、选不了的跳过不发；做成了才记上次用的号", async () => {
    backend((_o, _op, args) => ({
      results: (args.items as { sid: string }[]).map((i) => res(i.sid, "done", null, { session: `${i.sid}-cc` })),
    }));
    const out = await startMany([tab("c", "r1"), tab("e", "r1"), tab("f", "r1")], "tmux");
    expect(vi.mocked(fetchAccounts)).toHaveBeenCalledTimes(1);
    expect(calls).toEqual([
      [
        "r1",
        "sessions-start",
        {
          mode: "tmux",
          local: false,
          items: [
            { agent: "claude", sid: "c", cwd: "/w/c", account: { kind: "named", name: "work", configDir: "/h/.cc/work" }, model: "opus", launcher: "claude", defaultLauncher: "claude" },
            // 没有 pin 的那一个跟当前号（同单个那条的跟随），不是跳过。
            { agent: "claude", sid: "f", cwd: "/w/f", account: { kind: "named", name: "work", configDir: "/h/.cc/work" }, model: "opus", launcher: "claude", defaultLauncher: "claude" },
          ],
        },
      ],
    ]);
    expect(out.find((o) => o.sid === "e")).toEqual({ sid: "e", outcome: "skipped", why: copyText("tabBatch.why.accountGone", { name: "gone" }) });
    expect(vi.mocked(recordLastAccount).mock.calls).toEqual([["c", "work"], ["f", "work"]]);
  });

  it("开终端：后端渲好的那一行逐个开窗；窗口开不出来的记失败、不记上次用的号", async () => {
    vi.mocked(lastAccounts).mockResolvedValue({});
    backend((_o, _op, args) => ({
      results: (args.items as { sid: string }[]).map((i) => res(i.sid, "done", null, { cmd: `ccm --resume ${i.sid}` })),
    }));
    vi.mocked(openTerminal).mockImplementation(async (_o, cmd) => {
      if (cmd.endsWith("y")) throw new Error("开不了");
    });
    const out = await startMany([tab("x", "r1"), tab("y", "r1")], "window");
    expect(vi.mocked(openTerminal).mock.calls).toEqual([
      ["r1", "ccm --resume x"],
      ["r1", "ccm --resume y"],
    ]);
    expect(out).toEqual([
      { sid: "x", outcome: "done", why: "" },
      { sid: "y", outcome: "failed", why: copyText("tabBatch.why.windowFailed", { detail: "Error: 开不了" }) },
    ]);
    expect(vi.mocked(recordLastAccount).mock.calls).toEqual([["x", "work"]]);
  });
});
