/**
 * tab 栏批量停 / 起交给那几台后端：停 ⇒ 同一台的那几个一次调用、各台各发一次（一台失败不挡别台）；起 ⇒ 逐项问那台（同一台挨个、各台并发），
 * 每项只交 sid 与放在哪，结局取那台那一句；落在窗口里的逐个开窗。后端答什么由替身给（`chan_call` 那一跳），tmux / 会话一个都不起。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/behavior", () => ({ getBehavior: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-open", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/terminal-open")>()),
  openTerminal: vi.fn(),
}));

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

describe("批量起：逐项问那台（`session-open`），同一台挨个、各台并发；每项的结局取那台那一句", () => {
  const opened = (sid: string, landed: string, line: string | null = null) => ({
    landed, session: landed === "window" ? null : `${sid}-cc`, line, sid, agent: "claude", cwd: `/w/${sid}`, account: null, note: null, prompt: null,
  });

  it("★ 两台各三项 ⇒ 六问、每问只交那一条 ＋ 放在哪；同一台任一时刻至多一问在飞，两台同时在飞", async () => {
    const flying = new Map<string, number>();
    let most = 0;
    const mostPer = new Map<string, number>();
    invokeMock.mockImplementation(async (cmd: string, a: unknown) => {
      if (cmd !== "chan_call") return undefined;
      const c = a as ChanCallArgs;
      calls.push([c.origin, c.op, chanArgsJson(c) as Record<string, unknown>]);
      flying.set(c.origin, (flying.get(c.origin) ?? 0) + 1);
      mostPer.set(c.origin, Math.max(mostPer.get(c.origin) ?? 0, flying.get(c.origin)!));
      most = Math.max(most, [...flying.values()].reduce((x, y) => x + y, 0));
      await new Promise((r) => setTimeout(r, 0));
      flying.set(c.origin, flying.get(c.origin)! - 1);
      const sid = (chanArgsJson(c) as { what: { sid: string } }).what.sid;
      return chanReply(opened(sid, "started"));
    });
    const out = await startMany([tab("a1", "r1"), tab("a2", "r1"), tab("a3", "r1"), tab("b1", "r2"), tab("b2", "r2"), tab("b3", "r2")], "tmux");
    expect(calls.map(([o, op, body]) => [o, op, body])).toEqual(
      expect.arrayContaining(["a1", "a2", "a3"].map((sid) => ["r1", "session-open", { what: { kind: "resume", sid }, run: "tmux", local: false }])),
    );
    expect(calls).toHaveLength(6);
    expect(calls.filter(([o]) => o === "r1").map(([, , b]) => (b.what as { sid: string }).sid), "同一台按条上的顺序挨个").toEqual(["a1", "a2", "a3"]);
    expect([...mostPer.values()], "同一台至多一问在飞").toEqual([1, 1]);
    expect(most, "两台同时在飞").toBe(2);
    expect(out.map((o) => [o.sid, o.outcome])).toEqual(["a1", "a2", "a3", "b1", "b2", "b3"].map((sid) => [sid, "done"]));
  });

  it("★ 在 tmux 里后台起 ⇒ 一个窗口都不开；本来就在跑的记跳过；那台不起的那一项照它那一句说、带复制详情", async () => {
    backend((_o, _op, args) => {
      const sid = (args.what as { sid: string }).sid;
      if (sid === "e") return { err: "Refused", body: Array.from(new TextEncoder().encode(JSON.stringify({ code: "record_gone", message: "那台写的那一句", data: { field: null, unavailable: null } }))), said: "那台写的那一句", detail: "detail-x" };
      return opened(sid, sid === "r" ? "running" : "started", `ccm --attach ${sid}-cc`);
    });
    const out = await startMany([tab("c", "r1"), tab("e", "r1"), tab("r", "r1")], "tmux");
    expect(openTerminal).not.toHaveBeenCalled();
    expect(out).toEqual([
      { sid: "c", outcome: "done", why: "" },
      { sid: "e", outcome: "failed", why: "那台写的那一句", detail: "detail-x" },
      { sid: "r", outcome: "skipped", why: copyText("tabBatch.why.running", { name: "r-cc" }) },
    ]);
  });

  it("★ 各开一个终端 ⇒ 落在窗口里的每项各开一次那一行；窗口开不出来的记失败", async () => {
    backend((_o, _op, args) => {
      const sid = (args.what as { sid: string }).sid;
      return opened(sid, "window", `ccm --resume ${sid}`);
    });
    vi.mocked(openTerminal).mockImplementation(async (_o, cmd) => {
      if (cmd.endsWith("y")) throw new Error("开不了");
    });
    const out = await startMany([tab("x", "r1"), tab("y", "r1")], "direct");
    expect(calls.map(([, , b]) => b.run)).toEqual(["direct", "direct"]);
    expect(vi.mocked(openTerminal).mock.calls).toEqual([
      ["r1", "ccm --resume x", "x"],
      ["r1", "ccm --resume y", "y"],
    ]);
    expect(out).toEqual([
      { sid: "x", outcome: "done", why: "" },
      { sid: "y", outcome: "failed", why: copyText("tabBatch.why.windowFailed") },
    ]);
  });
});
