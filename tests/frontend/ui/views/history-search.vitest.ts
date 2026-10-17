/**
 * 历史全文搜索（`src/frontend/ui/views/history-search.ts`）：本机与各台远端同一条路。
 *
 * 两条都是 typed 帧命令（`callTyped`）：入参 · 回包的类型从 Rust 生成，形状由握手指纹核过 ⇒ 这里不再有逐格的解码器，
 * 只核扇出那一半（问了哪几台 · 交给合并的是什么 · 结果原样用合并那一份）。
 * 要求：「历史 / 账号 / tmux / MCP 四个面，本机与远端走同一条代码路径」。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));

import { invoke } from "@tauri-apps/api/core";
import { searchArgs, searchAllMachines, type FullTextQuery } from "../../../../src/frontend/ui/views/history-search";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { SessionHits } from "../../../../src/frontend/ui/generated/SessionHits";
import type { SessionFace } from "../../../../src/frontend/ui/generated/SessionFace";
import faces from "../../../__fixtures__/session-face.golden.json";
import type { MachineHits } from "../../../../src/frontend/ui/generated/MachineHits";
import { chanArgsJson, chanReply, isChanCall, NO_CHANNEL, type ChanCallArgs } from "../../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

/** 那台 `history-search` 答回来的一行（不带 origin）。 */
function row(sid: string, updatedAt: number, hitCount: number): SessionHits {
  return {
    agent: "claude",
    sessionId: sid,
    projectPath: "/p",
    projectName: "p",
    jsonlPath: `/${sid}.jsonl`,
    title: "t",
    updatedAt,
    hitCount,
    hits: [],
    hitsTruncated: false,
    isBg: false,
    face: faces.ended as SessionFace,
    can: { resume: "yes", accounts: true, fork: true, delete: "yes" },
  };
}

/** 那台 `history-search` 的回包。 */
function searched(rows: SessionHits[], unreadable = 0, skipped: string[] = []): ArrayBuffer {
  return chanReply({ sessions: rows, unreadable, skipped });
}

const Q: FullTextQuery = { query: "kw", includeTools: false, scope: null, afterMs: null, limit: 300 };

/** 合并那一问交进来的各台行，摊平、远端添上名字（同后端 `merge_at`），`reverse` ⇒ 逆着交进来的顺序回。 */
function mergedRows(args: ChanCallArgs, reverse: boolean): unknown[] {
  const machines = (chanArgsJson(args) as { machines: MachineHits[] }).machines;
  const rows = machines.flatMap((m) =>
    m.sessions.map((s) => ({ ...s, ...(m.origin === LOCAL_ORIGIN ? {} : { origin: m.origin }), atText: "12:00", spanText: "12:00–12:00" })),
  );
  return reverse ? rows.reverse() : rows;
}

/** 本机后端合一份那一问的替身（按交进来的原样回，好让下面几条只看扇出那一半）。 */
function passMerge(args: ChanCallArgs): ArrayBuffer {
  const rows = mergedRows(args, false) as SessionHits[];
  return chanReply({ totalHits: rows.reduce((a, s) => a + s.hitCount, 0), sessionCount: rows.length, truncated: false, sessions: rows });
}

beforeEach(() => invokeMock.mockReset());

describe("合并问本机后端", () => {
  it("★ 各台的会话行按台（带那台的名字）一次交给本机后端合；结果原样用它的（顺序 · 计数 · truncated）", async () => {
    let sent: MachineHits[] = [];
    invokeMock.mockImplementation((cmd: string, args: unknown) => {
      if (cmd === "list_remote_mcp_origins") return Promise.resolve(["pi"]);
      if (isChanCall(cmd, args, "history-search")) {
        return Promise.resolve(searched([args.origin === LOCAL_ORIGIN ? row("loc", 100, 1) : row("r", 200, 2)]));
      }
      if (isChanCall(cmd, args, "history-search-merge")) {
        expect(args.origin, "合并问的是本机后端").toBe(LOCAL_ORIGIN);
        sent = (chanArgsJson(args) as { machines: MachineHits[] }).machines;
        const rows = mergedRows(args, true);
        return Promise.resolve(chanReply({ totalHits: 99, sessionCount: rows.length, truncated: true, sessions: rows }));
      }
      return Promise.resolve(undefined);
    });
    const got = await searchAllMachines(Q);
    expect(sent.map((m) => [m.origin, m.sessions.map((s) => s.sessionId)])).toEqual([
      [LOCAL_ORIGIN, ["loc"]],
      ["pi", ["r"]],
    ]);
    expect(got.sessions.map((s) => [s.sessionId, s.origin])).toEqual([
      ["r", "pi"],
      ["loc", undefined],
    ]);
    expect([got.totalHits, got.sessionCount, got.truncated]).toEqual([99, 2, true]);
  });
});

describe("选项只下发后端认的那几格（本机远端同一份）", () => {
  it("include_tools 只在真时给 · scope 只给 user/assistant/report · after_ms 只给正数 · limit 原样", () => {
    expect(searchArgs(Q)).toEqual({ query: "kw", limit: 300 });
    expect(searchArgs({ ...Q, includeTools: true, scope: "user", afterMs: 7 })).toEqual({
      query: "kw",
      limit: 300,
      include_tools: true,
      scope: "user",
      after_ms: 7,
    });
    expect(searchArgs({ ...Q, scope: "all", afterMs: 0, limit: null })).toEqual({ query: "kw" });
  });
});

describe("本机 ＋ 各台远端（都经通道）", () => {
  it("★ 本机也经通道问本机后端 `history-search`（与远端同一个 op、同一份请求体）；远端一台失败只跳过", async () => {
    invokeMock.mockImplementation((cmd: string, args: unknown) => {
      if (cmd === "list_remote_mcp_origins") return Promise.resolve(["pi", "down"]);
      if (isChanCall(cmd, args, "history-search")) {
        if (args.origin === LOCAL_ORIGIN) return Promise.resolve(searched([row("loc", 100, 1)]));
        return args.origin === "pi" ? Promise.resolve(searched([row("r", 200, 2)])) : Promise.reject(NO_CHANNEL);
      }
      if (isChanCall(cmd, args, "history-search-merge")) return Promise.resolve(passMerge(args));
      return Promise.resolve(undefined);
    });
    const got = await searchAllMachines({ ...Q, includeTools: true });
    // 顺序归本机后端（替身原样回）；这里只核「活着的两台都进了合并、坏的那台跳过、远端补 origin」。
    expect(got.sessions.map((s) => [s.sessionId, s.origin])).toEqual([
      ["loc", undefined],
      ["r", "pi"],
    ]);
    expect(got.totalHits).toBe(3);
    const asked = invokeMock.mock.calls
      .filter((c) => isChanCall(String(c[0]), c[1], "history-search"))
      .map((c) => c[1] as ChanCallArgs);
    expect(asked.map((a) => [a.origin, a.op]).sort()).toEqual(
      [
        [LOCAL_ORIGIN, "history-search"],
        ["down", "history-search"],
        ["pi", "history-search"],
      ].sort(),
    );
    const bodies = asked.map((a) => JSON.stringify(chanArgsJson(a)));
    expect(new Set(bodies).size, "本机远端同一份请求体").toBe(1);
    expect(chanArgsJson(asked[0])).toEqual({ query: "kw", limit: 300, include_tools: true });
    // 反空真：monitor 进程内那份索引的命令一次都没被问（它已经不存在了）。
    expect(invokeMock.mock.calls.some((c) => c[0] === "search_history")).toBe(false);
  });

  it("本机那一台失败 ⇒ 整次失败（本机是必答的那一台，与迁前同形），不装作「只是本机没结果」", async () => {
    invokeMock.mockImplementation((cmd: string, args: unknown) => {
      if (cmd === "list_remote_mcp_origins") return Promise.resolve(["pi"]);
      if (isChanCall(cmd, args, "history-search")) {
        return args.origin === LOCAL_ORIGIN ? Promise.reject(NO_CHANNEL) : Promise.resolve(searched([row("r", 1, 1)]));
      }
      if (isChanCall(cmd, args, "history-search-merge")) return Promise.resolve(passMerge(args));
      return Promise.resolve(undefined);
    });
    await expect(searchAllMachines(Q)).rejects.toBeTruthy();
  });

  it("没配远端 ⇒ 只问本机那一台", async () => {
    invokeMock.mockImplementation((cmd: string, args: unknown) =>
      Promise.resolve(
        cmd === "list_remote_mcp_origins"
          ? []
          : isChanCall(cmd, args, "history-search")
            ? searched([row("loc", 1, 4)])
            : isChanCall(cmd, args, "history-search-merge")
              ? passMerge(args)
              : undefined,
      ),
    );
    const got = await searchAllMachines(Q);
    expect(got.totalHits).toBe(4);
    const asked = invokeMock.mock.calls
      .filter((c) => isChanCall(String(c[0]), c[1], "history-search"))
      .map((c) => (c[1] as ChanCallArgs).origin);
    expect(asked).toEqual([LOCAL_ORIGIN]);
  });
});

describe("搜得不全要说出来（没答上的台 · 读不动的会话记录 · 内容搜索不覆盖的那几家）", () => {
  it("★ 这三样都交回给界面（此前远端没答只进日志、读不动只进后端日志、Codex 搜不到也不说）", async () => {
    invokeMock.mockImplementation((cmd: string, args: unknown) => {
      if (cmd === "list_remote_mcp_origins") return Promise.resolve(["pi", "down"]);
      if (isChanCall(cmd, args, "history-search")) {
        if (args.origin === LOCAL_ORIGIN)
          return Promise.resolve(searched([row("loc", 1, 1)], 2, ["Codex"]));
        return args.origin === "pi"
          ? Promise.resolve(searched([row("r", 2, 1)], 1, ["Codex"]))
          : Promise.reject(NO_CHANNEL);
      }
      if (isChanCall(cmd, args, "history-search-merge")) return Promise.resolve(passMerge(args));
      return Promise.resolve(undefined);
    });
    const got = await searchAllMachines(Q);
    expect([got.failedHosts, got.unreadable, got.skipped]).toEqual([["down"], 3, ["Codex"]]);
    expect(searchArgs({ ...Q, titles: true })).toEqual({ query: "kw", limit: 300, titles: true });
  });
});
