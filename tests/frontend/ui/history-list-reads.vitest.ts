/**
 * `src/frontend/ui/history-list-reads.ts` 的判据：历史页的平铺清单经通道问本机常驻后端，按类型收。
 *
 * 守的要求（设计稿「文件与历史」乙7 ①④）：一份跨项目的平铺清单，每行带「能做什么」；判定在后端，界面照类型读。
 *
 * 判据：
 * 1. **跨语言金样**：后端 `history_list_tests.rs::the_product_matches_the_cross_language_golden` 产出的成品
 *    （`tests/__fixtures__/history-list.golden.json`）逐行逐格收下（异源）。
 * 2. **两形**：回包按类型收（形状由握手指纹核）；平时那一问回了 raw 清单、raw 那一问回了成品 ⇒ 抛。
 * 3. **问的是谁、带了什么**：本机问 `<local>`；远端开页（`fresh`）先经长连接问那台常驻 `raw`、再带着那份（`listing`）交 `<local>` 并 · 筛 · 排；
 *    敲字只问 `<local>`（它记着那份）；`<local>` 说 `no_listing` ⇒ 问那台、带着再问；只带给了的那几格，键名是后端的蛇形。
 * 4. **并列**：各台各自排好的行按 `at` 并成一列，同 `at` 保持台的先后。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { fetchList, mergeByAt, mergeGroups, type HistoryGroup, type HistoryList, type HistoryRow } from "../../../src/frontend/ui/history-list-reads";
import { HistoryShapeError } from "../../../src/frontend/ui/history-reads";
import { chanArgsJson, chanReply, refusedReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = JSON.parse(
  readFileSync(join(__dirname, "../../__fixtures__", "history-list.golden.json"), "utf8"),
) as { rows: Record<string, unknown>[]; groups: Record<string, unknown>[] } & Record<string, unknown>;

beforeEach(() => invokeMock.mockReset());

describe("跨语言金样（后端出、这里读）", () => {
  it("一台的成品就是生成的那个类型：带 context 的父会话 · 说不清在不在跑的 · 读不了的那一组 · 那台的名字", () => {
    const v = golden as unknown as HistoryList;
    expect(v.rows.length).toBe(golden.rows.length);
    expect(v.rows.some((r) => r.context === true)).toBe(true);
    expect(v.rows.some((r) => r.status === "unknown" && r.can.delete === "unsure")).toBe(true);
    expect(v.groups.some((g) => g.failed !== null)).toBe(true);
    expect(v.rows.every((r) => r.origin === "dev")).toBe(true);
  });
});

describe("回包两形", () => {
  const empty = { rows: [], groups: [], total: 0, truncated: false, notice: null };
  const raw = { rows: [], failed: [] };
  it("平时那一问回了 raw 清单 ⇒ 抛；raw 那一问回了成品 ⇒ 抛", async () => {
    invokeMock.mockImplementation(() => Promise.resolve(chanReply(raw)));
    await expect(fetchList(undefined, {})).rejects.toThrow(HistoryShapeError);
    invokeMock.mockImplementation(() => Promise.resolve(chanReply(empty)));
    await expect(fetchList("dev", { fresh: true })).rejects.toThrow(HistoryShapeError);
  });
});

describe("问的是谁、带了什么", () => {
  const empty = { rows: [], groups: [], total: 0, truncated: false, notice: null };
  const raw = { rows: [{ sessionId: "r-1" }], failed: [] };
  const calls = () =>
    invokeMock.mock.calls
      .filter((c) => c[0] === "chan_call")
      .map((c) => c[1] as ChanCallArgs)
      .map((a) => [a.origin, a.op, chanArgsJson(a), typeof a.leftMs]);
  it("本机问 <local>；远端开页（fresh）先经长连接问那台常驻 raw、再带着那份交 <local> 合并；只带给了的那几格", async () => {
    invokeMock.mockImplementation((_cmd: string, a?: ChanCallArgs) => Promise.resolve(chanReply(a?.origin === "dev" ? raw : empty)));
    await fetchList(undefined, {});
    await fetchList("dev", { query: "回调", sort: "created", withinDays: 7, hidden: true, fresh: true });
    expect(calls()).toEqual([
      ["<local>", "history-list", {}, "number"],
      ["dev", "history-list", { raw: true }, "number"],
      ["<local>", "history-list", { origin: "dev", listing: raw, query: "回调", sort: "created", within_days: 7, hidden: true }, "number"],
    ]);
  });
  it("远端敲字（不 fresh）只问 <local>（用它记着的那份），不去那台", async () => {
    invokeMock.mockImplementation(() => Promise.resolve(chanReply(empty)));
    await fetchList("dev", { query: "回" });
    await fetchList("dev", { sid: "s-1" });
    expect(calls()).toEqual([
      ["<local>", "history-list", { origin: "dev", query: "回" }, "number"],
      // 独立查看窗按会话 ID 要那一行。
      ["<local>", "history-list", { origin: "dev", sid: "s-1" }, "number"],
    ]);
  });
  it("<local> 说还没有那台的清单（no_listing）⇒ 问那台 raw、带着再问一次", async () => {
    let first = true;
    invokeMock.mockImplementation((cmd: string, a?: ChanCallArgs) => {
      if (cmd !== "chan_call") return Promise.resolve(chanReply(empty));
      if (a?.origin === "dev") return Promise.resolve(chanReply(raw));
      if (first) {
        first = false;
        return Promise.reject(refusedReply("no_listing", "dev 的会话清单还没取回"));
      }
      return Promise.resolve(chanReply(empty));
    });
    await expect(fetchList("dev", { query: "回" })).resolves.toEqual(empty);
    expect(calls()).toEqual([
      ["<local>", "history-list", { origin: "dev", query: "回" }, "number"],
      ["dev", "history-list", { raw: true }, "number"],
      ["<local>", "history-list", { origin: "dev", listing: raw, query: "回" }, "number"],
    ]);
  });
  it("别的拒绝原样抛、不去那台", async () => {
    invokeMock.mockImplementation((cmd: string) => (cmd === "chan_call" ? Promise.reject(refusedReply("bad_args", "x")) : Promise.resolve(chanReply(empty))));
    await expect(fetchList("dev", { query: "回" })).rejects.toThrow();
    expect(calls().map((c) => c[0])).toEqual(["<local>"]);
  });
});

describe("并列", () => {
  const r = (sid: string, at: number) => ({ sessionId: sid, at }) as unknown as HistoryRow;
  it("按 at 并、同 at 保持台的先后", () => {
    const got = mergeByAt([[r("a", 9), r("b", 5), r("c", 1)], [r("x", 9), r("y", 6)], []]);
    expect(got.map((x) => x.sessionId)).toEqual(["a", "x", "y", "b", "c"]);
  });
  it("组按 order 并", () => {
    const g = (key: string, order: number) => ({ key, order }) as unknown as HistoryGroup;
    expect(mergeGroups([[g("a", 30), g("b", 10)], [g("x", 20)]]).map((x) => x.key)).toEqual(["a", "x", "b"]);
  });
});
