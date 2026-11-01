/**
 * tab 栏批量停 / 起交给本机后端：停 · 起都只发一条 `sessions-batch`，按台扇出 · 跳不跳过 · 合那一句都在后端；
 * 起那一批回包里落在窗口里的那几行由这里逐个开窗。后端答什么由替身给（`chan_call` 那一跳），tmux / 会话一个都不起。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/behavior", () => ({ getBehavior: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-open", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/terminal-open")>()),
  openTerminal: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/kit/toast", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/kit/toast")>()),
  failToast: vi.fn(),
}));
import { failToast } from "../../../src/frontend/ui/kit/toast";

import { invoke } from "@tauri-apps/api/core";
import {
  stopMany,
  startMany,
} from "../../../src/frontend/ui/tab-batch-run";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { getBehavior } from "../../../src/frontend/ui/behavior";
import { openTerminal } from "../../../src/frontend/ui/terminal-open";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import {
  chanArgsJson,
  chanReply,
  NO_CHANNEL,
  type ChanCallArgs,
} from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const tab = (
  sid: string,
  origin: string,
  cwd = `/w/${sid}`,
  agent: string | null = "claude",
): Tab =>
  ({
    sessionId: sid,
    origin,
    projectDir: cwd,
    title: `T-${sid}`,
    agent,
  }) as unknown as Tab;

/** 后端那一跳：`(origin, op)` ⇒ 怎么答（成品 / 抛）。记下每一次调用。 */
let calls: [string, string, Record<string, unknown>][] = [];
function backend(
  answer: (
    origin: string,
    op: string,
    args: Record<string, unknown>,
  ) => unknown,
): void {
  invokeMock.mockImplementation(async (cmd: string, a: unknown) => {
    if (cmd !== "chan_call") return undefined; // 问握手那一跳（置灰用）等别的：不答
    const c = a as ChanCallArgs;
    const args = chanArgsJson(c) as Record<string, unknown>;
    calls.push([c.origin, c.op, args]);
    const got = answer(c.origin, c.op, args);
    if (
      got instanceof Error ||
      (typeof got === "object" && got !== null && "err" in got)
    )
      throw got;
    return chanReply(got);
  });
}


beforeEach(() => {
  vi.clearAllMocks();
  invokeMock.mockReset();
  calls = [];
  vi.mocked(getBehavior).mockResolvedValue({ resumeCommand: "" } as never);
});

describe("批量停：交本机后端一条（`sessions-batch`），按台扇出 · 合句都在后端", () => {
  const merged = { results: [], said: "S", failed: false, lines: [], copyDetail: "" };
  it("★ 两台四个 tab ⇒ 只发一发、发给本机、items 是选中的全部（照选中的先后，不先筛）；回包原样交回", async () => {
    backend(() => merged);
    const out = await stopMany([tab("a", LOCAL_ORIGIN), tab("c", "r1"), tab("b", LOCAL_ORIGIN), tab("d", "r1")]);
    expect(calls).toEqual([
      [
        LOCAL_ORIGIN,
        "sessions-batch",
        {
          op: "stop",
          items: [
            { machine: LOCAL_ORIGIN, sid: "a" },
            { machine: "r1", sid: "c" },
            { machine: LOCAL_ORIGIN, sid: "b" },
            { machine: "r1", sid: "d" },
          ],
        },
      ],
    ]);
    expect(out).toEqual(merged);
  });
  it("本机后端问不到 ⇒ 抛（由调用方照那一句说）", async () => {
    backend(() => {
      throw NO_CHANNEL;
    });
    await expect(stopMany([tab("a", LOCAL_ORIGIN)])).rejects.toBeDefined();
  });
});

describe("批量起：交本机后端一条（`sessions-batch`，`start`），跳不跳过 · 合句在后端；开窗照回包的那几行", () => {
  const reply = (windows: unknown[] = []) => ({ results: [], said: "S", failed: false, lines: [], copyDetail: "", windows });

  it("★ 两台四个 tab ⇒ 只发一发、发给本机、items 是选中的全部（照选中的先后）；回包原样交回、tmux 那一种一个窗口都不开", async () => {
    backend(() => reply());
    const out = await startMany([tab("a", LOCAL_ORIGIN), tab("c", "r1"), tab("b", LOCAL_ORIGIN), tab("d", "r1")], "tmux");
    expect(calls).toEqual([
      [
        LOCAL_ORIGIN,
        "sessions-batch",
        {
          op: { start: { run: "tmux" } },
          items: [
            { machine: LOCAL_ORIGIN, sid: "a" },
            { machine: "r1", sid: "c" },
            { machine: LOCAL_ORIGIN, sid: "b" },
            { machine: "r1", sid: "d" },
          ],
        },
      ],
    ]);
    expect(out).toEqual(reply());
    expect(openTerminal).not.toHaveBeenCalled();
  });

  it("★ 回包里落在窗口里的那几项各开一次那一行（远端带那条会话）；开不出来那一项另出一条、回包不改", async () => {
    const windows = [
      { machine: "r1", sid: "x", line: "ccm --resume x", cwd: "/w/x" },
      { machine: "r1", sid: "y", line: "ccm --resume y", cwd: null },
    ];
    backend(() => reply(windows));
    vi.mocked(openTerminal).mockImplementation(async (_o, cmd) => {
      if (cmd.endsWith("y")) throw new Error("开不了");
    });
    const out = await startMany([tab("x", "r1"), tab("y", "r1")], "direct");
    expect(calls.map(([, op, b]) => [op, b.op])).toEqual([["sessions-batch", { start: { run: "direct" } }]]);
    expect(vi.mocked(openTerminal).mock.calls).toEqual([
      ["r1", "ccm --resume x", "x"],
      ["r1", "ccm --resume y", "y"],
    ]);
    expect(out).toEqual(reply(windows));
    expect(vi.mocked(failToast).mock.calls.map(([t]) => t)).toEqual([copyText("tabBatch.why.windowFailed")]);
  });

  it("本机后端问不到 ⇒ 抛（由调用方照那一句说）", async () => {
    backend(() => {
      throw NO_CHANNEL;
    });
    await expect(startMany([tab("a", "r1")], "tmux")).rejects.toBeDefined();
  });
});
