/**
 * 历史全文搜索（`src/views/history-search.ts`）：本机与各台远端同一条路。
 *
 * 〔C4a · 第四波〕合并与远端 fan-out 从 Rust 搬来时，判据跟着它的家走（原 `tests/bridge/search_tests.rs` 那一组逐条同形）。
 * 〔LOC1b · 第四波 4D〕本机那一半也改问本机后端（`chan.call(LOCAL_ORIGIN, "history-search")`），monitor 内存索引删了 ⇒
 * 合并只剩「一组会话行」这一形；「本机 indexing」那两条随那一态删了。
 * 要求住址：`设计/00 §2.5 ①` 逐字「历史 / 账号 / tmux / MCP 四个面，本机与远端走同一条代码路径」。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));

import { invoke } from "@tauri-apps/api/core";
import {
  mergeSearchResults,
  parseSessionHitsLines,
  searchArgs,
  searchAllMachines,
  type FullTextQuery,
  type SessionHits,
} from "../../src/views/history-search";
import { LOCAL_ORIGIN } from "../../src/ipc/origin";
import { chanArgsJson, isChanCall, linesReply, NO_CHANNEL, type ChanCallArgs } from "../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

function mk(sid: string, updatedAt: number, hitCount: number, origin?: string): SessionHits {
  return {
    sessionId: sid,
    projectPath: "/p",
    projectName: "p",
    jsonlPath: `/${sid}.jsonl`,
    title: sid,
    updatedAt,
    hitCount,
    hits: [],
    hitsTruncated: false,
    ...(origin === undefined ? {} : { origin }),
  };
}

/** 后端 `--search` 的一行（不带 origin）。 */
function row(sid: string, updatedAt: number, hitCount: number): string {
  return JSON.stringify({
    sessionId: sid,
    projectPath: "/p",
    projectName: "p",
    jsonlPath: `/${sid}.jsonl`,
    title: "t",
    updatedAt,
    hitCount,
    hits: [],
  });
}

const Q: FullTextQuery = { query: "kw", includeTools: false, scope: null, afterMs: null, limit: 300 };

beforeEach(() => invokeMock.mockReset());

describe("合并", () => {
  it("拼接 + updatedAt 倒序 + 总数相加；远端 origin 保留、本机不带", () => {
    const merged = mergeSearchResults([mk("local-old", 100, 3), mk("rem-new", 300, 2, "pi"), mk("rem-mid", 200, 1, "wsl")]);
    expect(merged.totalHits).toBe(3 + 2 + 1);
    expect(merged.sessionCount).toBe(3);
    expect(merged.sessions.map((s) => s.sessionId)).toEqual(["rem-new", "rem-mid", "local-old"]);
    expect(merged.sessions[0].origin).toBe("pi");
    expect(merged.sessions[2].origin).toBeUndefined();
  });

  it("★ `K-R100`：任一台任一会话被截断 ⇒ 整体 truncated（反空真：都没截断时不许乱亮；本机那台同样算）", () => {
    expect(mergeSearchResults([mk("loc", 100, 1), { ...mk("rem", 200, 12, "pi"), hitsTruncated: true }]).truncated).toBe(true);
    expect(mergeSearchResults([{ ...mk("loc", 100, 12), hitsTruncated: true }, mk("rem", 200, 1, "pi")]).truncated).toBe(true);
    expect(mergeSearchResults([mk("loc", 100, 1), mk("rem", 200, 1, "pi")]).truncated).toBe(false);
  });

  it("一条都没有 ⇒ 空结果（不是「索引中」）", () => {
    expect(mergeSearchResults([])).toEqual({ totalHits: 0, sessionCount: 0, truncated: false, sessions: [] });
  });
});

describe("后端 `--search` 的逐行", () => {
  it("后端那一行（camelCase，不带 origin）解得出来，并补上那台的 origin", () => {
    const line = `{"sessionId":"s9","projectPath":"/home/pi/p","projectName":"p","jsonlPath":"/home/pi/.claude/projects/p/s9.jsonl","title":"标题","updatedAt":123,"hitCount":2,"hits":[{"uuid":"u1","tsMs":5,"kind":"user","before":"b","matched":"m","after":"a"}]}`;
    const [sh] = parseSessionHitsLines([line], "pi");
    expect(sh.sessionId).toBe("s9");
    expect(sh.hitCount).toBe(2);
    expect(sh.hits).toHaveLength(1);
    expect(sh.hitsTruncated, "老后端缺 hitsTruncated ⇒ false").toBe(false);
    expect(sh.origin).toBe("pi");
    // 〔LOC1b〕本机那一台：不补 origin（界面按「缺 ＝ 本机」画）。
    const [mine] = parseSessionHitsLines([line], undefined);
    expect("origin" in mine).toBe(false);
  });

  it("坏行跳过、不毁整次（不是 JSON / 缺字段 / 命中里有一格坏）", () => {
    const good = `{"sessionId":"a","projectPath":"/p","projectName":"p","jsonlPath":"/a.jsonl","title":"t","updatedAt":1,"hitCount":0,"hits":[]}`;
    const got = parseSessionHitsLines(
      [
        "not json",
        `{"sessionId":"b"}`,
        `{"sessionId":"c","projectPath":"/p","projectName":"p","jsonlPath":"/c.jsonl","title":"t","updatedAt":1,"hitCount":1,"hits":[{"uuid":"u"}]}`,
        good,
      ],
      "pi",
    );
    expect(got.map((s) => s.sessionId)).toEqual(["a"]);
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
      return Promise.resolve(undefined);
    });
    const got = await searchAllMachines({ ...Q, includeTools: true });
    expect(got.sessions.map((s) => [s.sessionId, s.origin])).toEqual([
      ["r", "pi"],
      ["loc", undefined],
    ]);
    expect(got.totalHits).toBe(3);
    const asked = invokeMock.mock.calls.filter((c) => c[0] === "chan_call").map((c) => c[1] as ChanCallArgs);
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
      return Promise.resolve(undefined);
    });
    await expect(searchAllMachines(Q)).rejects.toBeTruthy();
  });

  it("没配远端 ⇒ 只问本机那一台", async () => {
    invokeMock.mockImplementation((cmd: string, args: unknown) =>
      Promise.resolve(
        cmd === "list_remote_mcp_origins" ? [] : isChanCall(cmd, args, "history-search") ? linesReply([row("loc", 1, 4)]) : undefined,
      ),
    );
    const got = await searchAllMachines(Q);
    expect(got.totalHits).toBe(4);
    const asked = invokeMock.mock.calls.filter((c) => c[0] === "chan_call").map((c) => (c[1] as ChanCallArgs).origin);
    expect(asked).toEqual([LOCAL_ORIGIN]);
  });
});
