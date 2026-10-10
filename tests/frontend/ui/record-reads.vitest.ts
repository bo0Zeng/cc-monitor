/**
 * 记录读面（`src/frontend/ui/record-reads.ts`）交不交「折起那一行」那份声明。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 按偏移取一段 · 按行号取一段带声明；声明就是 `src/shared/views/folded-record.json` 那一份，放在 `chan_call` 的 `view` 那一格、不进载荷 | 「带声明」那一条 |
 * | 按记录 id 取回那一行不带声明（展开那一下要的就是全文） | 「不带声明」那一条 |
 *
 * 买不到：后端照声明裁得对不对（后端 `record_page_tests` 用同一份文件量）；monitor 那一跳把它搬进信封（`webview_tests` · `inbound_client_tests`）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { readLines, readRange, readRecordById } from "../../../src/frontend/ui/record-reads";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const FOLDED = JSON.parse(readFileSync(resolve(REPO_ROOT, "src/shared/views/folded-record.json"), "utf8")) as unknown;

const line = (seq: number, id: string) => ({
  session_id: "s",
  path: "/p/s.jsonl",
  seq,
  cwd: null,
  record: { id },
});

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((_cmd: string, args: ChanCallArgs) =>
    Promise.resolve(
      args.op === "history-page"
        ? chanReply({ lines: [line(3, "r3")], next: 90, nextSeq: 4, eof: true })
        : chanReply({ from: 0, next: 1, eof: true, lines: [line(0, "r0")] }),
    ),
  );
});

const calls = (): ChanCallArgs[] => invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);

describe("记录读面：折起那一行的声明", () => {
  it("那份声明就是省掉工具入参 · 结果正文 · 逐段改动那四格", () => {
    expect(FOLDED).toEqual({
      omit: {
        record: [
          "blocks[type=tool_use].input",
          "blocks[type=tool_result].content",
          "results.*.patch",
          "results.*.patchTruncated",
        ],
      },
    });
  });

  it("★ 按偏移取一段 · 按行号取一段：带声明（`view` 那一格，不进载荷）", async () => {
    await readRange("<local>", "/p/s.jsonl", 0, 90, 3);
    await readLines("<local>", "/p/s.jsonl", 0, undefined, 5_000);
    const [page, lines] = calls();
    expect([page.op, lines.op]).toEqual(["history-page", "history-lines"]);
    expect(page.view).toEqual(FOLDED);
    expect(lines.view).toEqual(FOLDED);
    expect(Object.keys(chanArgsJson(page) as object).sort()).toEqual(["offset", "path", "seq", "until"]);
  });

  it("★ 按记录 id 取回那一行：不带声明（要的就是全文）", async () => {
    const places = { uuidToSeq: new Map([["r3", 3]]), factsOf: () => ({ o: 40, n: 50 }) };
    const rec = await readRecordById("<local>", "/p/s.jsonl", places, "r3");
    expect(rec).toEqual({ id: "r3" });
    expect(calls().map((c) => [c.op, c.view])).toEqual([["history-page", null]]);
  });
});
