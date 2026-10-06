/**
 * 历史全文搜索（`src/frontend/ui/views/history-search.ts`）：本机与各台远端同一条路。
 *
 * 合并与远端 fan-out 从 Rust 搬来时，判据跟着它的家走（原 `tests/frontend/shell/search_tests.rs` 那一组逐条同形）。
 * 本机那一半也改问本机后端（`chan.call(LOCAL_ORIGIN, "history-search")`），monitor 内存索引删了 ⇒
 * 合并只剩「一组会话行」这一形；「本机 indexing」那两条随那一态删了。
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
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  decodeMerged,
  parseSessionHitsLines,
  searchArgs,
  searchAllMachines,
  type FullTextQuery,
  type SessionHits,
} from "../../../../src/frontend/ui/views/history-search";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import { chanArgsJson, chanReply, isChanCall, linesReply, NO_CHANNEL, type ChanCallArgs } from "../../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

function mk(sid: string, updatedAt: number, hitCount: number, origin?: string): SessionHits {
  return {
    agent: "claude",
    sessionId: sid,
    projectPath: "/p",
    projectName: "p",
    jsonlPath: `/${sid}.jsonl`,
    title: sid,
    updatedAt,
    hitCount,
    hits: [],
    hitsTruncated: false,
    isBg: false,
    status: "ended",
    can: { resume: "yes", accounts: true, fork: true, delete: "yes" },
    ...(origin === undefined ? {} : { origin }),
  };
}

/** 后端 `--search` 的一行（不带 origin）。 */
function row(sid: string, updatedAt: number, hitCount: number): string {
  return JSON.stringify({
    agent: "claude",
    sessionId: sid,
    projectPath: "/p",
    projectName: "p",
    jsonlPath: `/${sid}.jsonl`,
    title: "t",
    updatedAt,
    hitCount,
    hits: [],
    isBg: false,
    status: "ended",
    can: { resume: "yes", accounts: true, fork: true, delete: "yes" },
  });
}

const Q: FullTextQuery = { query: "kw", includeTools: false, scope: null, afterMs: null, limit: 300 };

/** 本机后端合一份那一问的替身（按交进来的原样回，好让下面几条只看扇出那一半）。 */
function passMerge(args: ChanCallArgs): ArrayBuffer {
  const rows = (chanArgsJson(args) as { sessions: SessionHits[] }).sessions;
  return chanReply({ totalHits: rows.reduce((a, s) => a + s.hitCount, 0), sessionCount: rows.length, truncated: false, sessions: rows });
}

beforeEach(() => invokeMock.mockReset());

// 这里原来三条钉前端 `mergeSearchResults`〔散文墓碑〕（倒序 · 总数相加 · 任一被砍 ⇒ truncated · 空）：
//   合并排序搬进本机后端 `history-search-merge`（`search_rules::sort_by_recency`），期望原样搬进
//   `tests/backend/observe/search_query_tests.rs::the_merge_frame_sorts_newest_first_stably_and_sums_what_each_machine_said`。
//   这里钉界面那一半：交给它的是什么、用的是不是它合好的那一份、它回的形状不认怎么办。

/** 替身：本机后端合好的一份 —— 刻意**逆着**交进来的顺序回，好认出「界面用的就是它排的，不是自己排的」。 */
function mergeReply(sent: unknown): ArrayBuffer {
  const rows = ((sent as { sessions: SessionHits[] }).sessions ?? []).slice().reverse();
  return chanReply({
    totalHits: 99,
    sessionCount: rows.length,
    truncated: true,
    sessions: rows,
  });
}

describe("合并问本机后端", () => {
  it("★ 各台的会话行（远端补了 origin）一次交给本机后端合；结果原样用它的（顺序 · 计数 · truncated）", async () => {
    let sent: unknown = null;
    invokeMock.mockImplementation((cmd: string, args: unknown) => {
      if (cmd === "list_remote_mcp_origins") return Promise.resolve(["pi"]);
      if (isChanCall(cmd, args, "history-search")) {
        return Promise.resolve(linesReply([args.origin === LOCAL_ORIGIN ? row("loc", 100, 1) : row("r", 200, 2)]));
      }
      if (isChanCall(cmd, args, "history-search-merge")) {
        expect(args.origin, "合并问的是本机后端").toBe(LOCAL_ORIGIN);
        sent = chanArgsJson(args);
        return Promise.resolve(mergeReply(sent));
      }
      return Promise.resolve(undefined);
    });
    const got = await searchAllMachines(Q);
    expect((sent as { sessions: SessionHits[] }).sessions.map((s) => [s.sessionId, s.origin])).toEqual([
      ["loc", undefined],
      ["r", "pi"],
    ]);
    expect(got.sessions.map((s) => s.sessionId)).toEqual(["r", "loc"]);
    expect([got.totalHits, got.sessionCount, got.truncated]).toEqual([99, 2, true]);
  });

  it("本机后端回的形状不认 ⇒ 抛（不自己补、不自己排）；正控：认得的那一形照收", () => {
    const ok = { totalHits: 1, sessionCount: 1, truncated: false, sessions: [mk("a", 1, 1, "pi")] };
    expect(decodeMerged(ok).sessions[0].origin).toBe("pi");
    for (const bad of [
      { ...ok, extra: 1 },
      { ...ok, sessionCount: 2 },
      { ...ok, sessions: [{ sessionId: "a" }] },
      { ...ok, sessions: [{ ...mk("a", 1, 1), origin: 3 }] },
      null,
    ]) {
      expect(() => decodeMerged(bad)).toThrow(/读不懂/);
    }
  });
});

describe("后端 `--search` 的逐行", () => {
  it("后端那一行（camelCase，不带 origin）解得出来，并补上那台的 origin", () => {
    const line = `{"agent":"claude","sessionId":"s9","projectPath":"/home/pi/p","projectName":"p","jsonlPath":"/home/pi/.claude/projects/p/s9.jsonl","title":"标题","updatedAt":123,"hitCount":2,"hits":[{"uuid":"u1","tsMs":5,"kind":"user","before":"b","matched":"m","after":"a"}],"isBg":false,"status":"ended","can":{"resume":"yes","accounts":true,"fork":true,"delete":"yes"}}`;
    const [sh] = parseSessionHitsLines([line], "pi");
    expect(sh.sessionId).toBe("s9");
    expect(sh.hitCount).toBe(2);
    expect(sh.hits).toHaveLength(1);
    expect(sh.hitsTruncated, "老后端缺 hitsTruncated ⇒ false").toBe(false);
    expect(sh.origin).toBe("pi");
    expect([sh.status, sh.can.resume, sh.isBg]).toEqual(["ended", "yes", false]);
    // 本机那一台：不补 origin（界面按「缺 ＝ 本机」画）。
    const [mine] = parseSessionHitsLines([line], undefined);
    expect("origin" in mine).toBe(false);
  });

  it("坏行跳过、不毁整次（不是 JSON / 缺字段 / 命中里有一格坏）", () => {
    const good = `{"agent":"claude","sessionId":"a","projectPath":"/p","projectName":"p","jsonlPath":"/a.jsonl","title":"t","updatedAt":1,"hitCount":0,"hits":[],"isBg":false,"status":"ended","can":{"resume":"yes","accounts":true,"fork":true,"delete":"yes"}}`;
    const got = parseSessionHitsLines(
      [
        "not json",
        `{"sessionId":"b"}`,
        `{"agent":"claude","sessionId":"c","projectPath":"/p","projectName":"p","jsonlPath":"/c.jsonl","title":"t","updatedAt":1,"hitCount":1,"hits":[{"uuid":"u"}]}`,
        // 能不能恢复由那台判：缺 `can` / `status`、或 `can` 里有一格不认 ⇒ 坏行。
        good.replace(',"status":"ended"', ""),
        good.replace(`"resume":"yes"`, `"resume":"maybe"`),
        good.replace(',"isBg":false', ""),
        good,
      ],
      "pi",
    );
    expect(got.map((s) => s.sessionId)).toEqual(["a"]);
  });
});

describe("跨语言金样：命中行（后端判据产出 `tests/__fixtures__/history-search-row.golden.json`）", () => {
  it("每一行都收下，状态与能做什么照那台判的原样读", () => {
    const golden = JSON.parse(readFileSync(join(__dirname, "../../../__fixtures__", "history-search-row.golden.json"), "utf8")) as { lines: string[] };
    const got = parseSessionHitsLines(golden.lines, undefined);
    expect(got.map((s) => [s.sessionId, s.status, s.isBg, s.can.resume, s.can.fork, s.can.delete])).toEqual([
      ["live1", "live", false, "switch", true, "live"],
      ["ended1", "ended", false, "yes", true, "yes"],
      ["bg1", "ended", true, "bg", false, "yes"],
    ]);
  });
});

describe("选项只下发后端认的那几格（本机远端同一份）", () => {
  it("include_tools 只在真时给 · scope 只给 user/assistant · after_ms 只给正数 · limit 原样", () => {
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
        if (args.origin === LOCAL_ORIGIN) return Promise.resolve(linesReply([row("loc", 100, 1)]));
        return args.origin === "pi" ? Promise.resolve(linesReply([row("r", 200, 2)])) : Promise.reject(NO_CHANNEL);
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
        return args.origin === LOCAL_ORIGIN ? Promise.reject(NO_CHANNEL) : Promise.resolve(linesReply([row("r", 1, 1)]));
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
            ? linesReply([row("loc", 1, 4)])
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
          return Promise.resolve(chanReply({ lines: [row("loc", 1, 1)], unreadable: 2, skipped: ["Codex"] }));
        return args.origin === "pi"
          ? Promise.resolve(chanReply({ lines: [row("r", 2, 1)], unreadable: 1, skipped: ["Codex"] }))
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
