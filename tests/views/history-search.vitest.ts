/**
 * 〔C4a · 第四波〕历史全文搜索的「本机 ＋ 各台远端」（`src/views/history-search.ts`）。
 *
 * 合并那四条是从 Rust（`tests/bridge/search_tests.rs` 里原来那一组）**逐条同形**搬来的 ——
 * 合并本身随远端 fan-out 一起搬到了前端，判据跟着它的家走：
 * 拼接排序求和 · 无远端原样 · 远端截断不许在合并处丢掉（`K-R100`）· 本机 indexing 而有远端 ⇒ ready。
 * 外加：后端行的解释（原 `search_tests.rs` 那条反序列化金样同一行）· 远端选项只下发后端认的 ·
 * 逐台并发、逐台失败只跳过、全程经通道（`history-search`）。
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
  remoteSearchArgs,
  searchAllMachines,
  type FullTextQuery,
} from "../../src/views/history-search";
import type { SearchResponse } from "../../src/generated/SearchResponse";
import type { SessionHits } from "../../src/generated/SessionHits";
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

function resp(status: string, total: number, sessions: SessionHits[]): SearchResponse {
  return {
    status,
    totalHits: total,
    sessionCount: sessions.length,
    truncated: false,
    indexedSessions: 1,
    indexedMessages: 1,
    sessions,
  };
}

const Q: FullTextQuery = { query: "kw", includeTools: false, scope: null, afterMs: null, limit: 300 };

beforeEach(() => invokeMock.mockReset());

describe("合并（逐条同形，原住 Rust `search.rs`）", () => {
  it("拼接 + updatedAt 倒序 + 总数相加；远端 origin 保留", () => {
    const merged = mergeSearchResults(resp("ready", 3, [mk("local-old", 100, 3)]), [
      mk("rem-new", 300, 2, "pi"),
      mk("rem-mid", 200, 1, "wsl"),
    ]);
    expect(merged.status).toBe("ready");
    expect(merged.totalHits).toBe(3 + 2 + 1);
    expect(merged.sessionCount).toBe(3);
    expect(merged.sessions.map((s) => s.sessionId)).toEqual(["rem-new", "rem-mid", "local-old"]);
    expect(merged.sessions[0].origin).toBe("pi");
    expect(merged.sessions[2].origin).toBeUndefined();
  });

  it("无远端 → 原样返回本机（含 indexing 态不被改写）", () => {
    const local = resp("indexing", 0, []);
    expect(mergeSearchResults(local, [])).toBe(local);
  });

  it("★ `K-R100`：远端截断不许在合并那一步被丢掉（反空真：远端没截断时不许乱亮）", () => {
    const rem = { ...mk("rem", 200, 12, "pi"), hitsTruncated: true };
    expect(mergeSearchResults(resp("ready", 1, [mk("loc", 100, 1)]), [rem]).truncated).toBe(true);
    expect(mergeSearchResults(resp("ready", 1, [mk("loc", 100, 1)]), [mk("rem", 200, 1, "pi")]).truncated).toBe(false);
  });

  it("本机 indexing 但有远端结果 → status=ready（不丢远端）", () => {
    const merged = mergeSearchResults(resp("indexing", 0, []), [mk("rem", 50, 4, "pi")]);
    expect(merged.status).toBe("ready");
    expect(merged.totalHits).toBe(4);
    expect(merged.sessions).toHaveLength(1);
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

describe("远端选项只下发后端认的那几格", () => {
  it("include_tools 只在真时给 · scope 只给 user/assistant · after_ms 只给正数 · limit 原样", () => {
    expect(remoteSearchArgs(Q)).toEqual({ query: "kw", limit: 300 });
    expect(remoteSearchArgs({ ...Q, includeTools: true, scope: "user", afterMs: 7 })).toEqual({
      query: "kw",
      limit: 300,
      include_tools: true,
      scope: "user",
      after_ms: 7,
    });
    expect(remoteSearchArgs({ ...Q, scope: "all", afterMs: 0, limit: null })).toEqual({ query: "kw" });
  });
});

describe("本机 ＋ 各台远端（经通道）", () => {
  it("★ 逐台经通道问 `history-search`，一台失败只跳过；本机那半只问本机索引一次", async () => {
    invokeMock.mockImplementation((cmd: string, args: unknown) => {
      if (cmd === "search_history") return Promise.resolve(resp("ready", 1, [mk("loc", 100, 1)]));
      if (cmd === "list_remote_mcp_origins") return Promise.resolve(["pi", "down"]);
      if (isChanCall(cmd, args, "history-search")) {
        return args.origin === "pi"
          ? Promise.resolve(
              linesReply([
                `{"sessionId":"r","projectPath":"/p","projectName":"p","jsonlPath":"/r.jsonl","title":"t","updatedAt":200,"hitCount":2,"hits":[]}`,
              ]),
            )
          : Promise.reject(NO_CHANNEL);
      }
      return Promise.resolve(undefined);
    });
    const got = await searchAllMachines({ ...Q, includeTools: true });
    expect(got.sessions.map((s) => [s.sessionId, s.origin])).toEqual([
      ["r", "pi"],
      ["loc", undefined],
    ]);
    expect(got.totalHits).toBe(3);
    expect(invokeMock.mock.calls.filter((c) => c[0] === "search_history")).toHaveLength(1);
    const asked = invokeMock.mock.calls
      .filter((c) => c[0] === "chan_call")
      .map((c) => c[1] as ChanCallArgs);
    expect(asked.map((a) => [a.origin, a.op])).toEqual([
      ["pi", "history-search"],
      ["down", "history-search"],
    ]);
    expect(chanArgsJson(asked[0])).toEqual({ query: "kw", limit: 300, include_tools: true });
  });

  it("没配远端 ⇒ 一次都不扇出，原样回本机那一份", async () => {
    const local = resp("indexing", 0, []);
    invokeMock.mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "search_history" ? local : cmd === "list_remote_mcp_origins" ? [] : undefined),
    );
    expect(await searchAllMachines(Q)).toEqual(local);
    expect(invokeMock.mock.calls.some((c) => c[0] === "chan_call")).toBe(false);
  });
});
