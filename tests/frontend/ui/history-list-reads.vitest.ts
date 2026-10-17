/**
 * `src/frontend/ui/history-list-reads.ts` 的判据：历史页的平铺清单经通道问本机常驻后端，按类型收。
 *
 * 守的要求：一份跨项目的平铺清单，每行带「能做什么」；判定在后端，界面照类型读。
 *
 * 判据：
 * 1. **跨语言金样**：后端 `history_list_tests.rs::the_product_matches_the_cross_language_golden` 产出的成品
 *    （`tests/__fixtures__/history-list.golden.json`）逐行逐格收下（异源）。
 * 2. **回包**：按类型收（形状由握手指纹核）；不带记号问回来的不是整份（`whole`）· `order` 里有 `up` 没给的行 ⇒ 抛；
 *    整份照 `order` 摊成行，窗外还有 ⇒ `truncated`。
 * 3. **问的是谁、带了什么**：一律只问 `<local>`（远端那台的清单由本机后端自己问那台的常驻、记着副本）；远端开页（`fresh`）带 `fresh`、
 *    敲字不带；本机那一问不带 `fresh`；只带给了的那几格，键名是后端的蛇形；拒绝原样抛（那台常驻不在也是），界面不另去问那台。
 * 4. **并列**：各台各自排好的行按 `at` 并成一列，同 `at` 保持台的先后。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { fetchList, mergeByAt, mergeGroups, type HistoryGroup, type HistoryList, type HistoryRow } from "../../../src/frontend/ui/history-list-reads";
import { ReplyUnreadable } from "../../../src/frontend/ui/ipc/chan-caller";
import { chanArgsJson, chanReply, refusedReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = JSON.parse(
  readFileSync(join(__dirname, "../../__fixtures__", "history-list.golden.json"), "utf8"),
) as { order: string[] } & Record<string, unknown>;

/** 一台整份那一答（不带记号问的回包）。 */
const whole = (rows: { sessionId: string }[], total = rows.length) => ({
  held: "h",
  whole: true,
  same: false,
  order: rows.map((r) => r.sessionId),
  up: Object.fromEntries(rows.map((r) => [r.sessionId, r])),
  head: { total, info: { groups: [], notice: null } },
});
const empty = whole([]);
const flatEmpty = { rows: [], groups: [], notice: null, total: 0, truncated: false };

beforeEach(() => invokeMock.mockReset());

describe("跨语言金样（后端出、这里读）", () => {
  it("一台的成品就是生成的那个类型：带 context 的父会话 · 说不清在不在跑的 · 读不了的那一组 · 那台的名字", async () => {
    invokeMock.mockImplementation(() => Promise.resolve(chanReply(golden)));
    const v: HistoryList = await fetchList(undefined, {});
    expect(v.rows.map((r) => r.sessionId)).toEqual(golden.order);
    expect(v.truncated).toBe(false);
    expect(v.rows.some((r) => r.context === true)).toBe(true);
    expect(v.rows.some((r) => r.face.dot === "unknown" && r.can.delete === "unsure")).toBe(true);
    expect(v.groups.some((g) => g.failed !== null)).toBe(true);
    expect(v.rows.every((r) => r.origin === "dev")).toBe(true);
  });
});

describe("回包", () => {
  it("不带记号问却回了「没变」· 先后里有没给的行 ⇒ 抛；窗外还有 ⇒ truncated", async () => {
    invokeMock.mockImplementation(() => Promise.resolve(chanReply({ ...empty, whole: false, same: true, order: null, head: null })));
    await expect(fetchList(undefined, {})).rejects.toThrow(ReplyUnreadable);
    invokeMock.mockImplementation(() => Promise.resolve(chanReply({ ...empty, order: ["s-x"] })));
    await expect(fetchList(undefined, {})).rejects.toThrow(ReplyUnreadable);
    invokeMock.mockImplementation(() => Promise.resolve(chanReply(whole([{ sessionId: "a" }], 9))));
    await expect(fetchList(undefined, {})).resolves.toMatchObject({ total: 9, truncated: true });
  });
});

describe("问的是谁、带了什么", () => {
  const calls = () =>
    invokeMock.mock.calls
      .filter((c) => c[0] === "chan_call")
      .map((c) => c[1] as ChanCallArgs)
      .map((a) => [a.origin, a.op, chanArgsJson(a), typeof a.leftMs]);
  it("一律只问 <local>：远端开页带 fresh、敲字不带，本机那一问不带；只带给了的那几格", async () => {
    invokeMock.mockImplementation(() => Promise.resolve(chanReply(empty)));
    await expect(fetchList(undefined, { fresh: true })).resolves.toEqual(flatEmpty);
    await fetchList("dev", { query: "回调", sort: "created", withinDays: 7, hidden: true, fresh: true });
    await fetchList("dev", { query: "回" });
    await fetchList("dev", { sid: "s-1" });
    expect(calls()).toEqual([
      ["<local>", "history-list", {}, "number"],
      ["<local>", "history-list", { origin: "dev", fresh: true, query: "回调", sort: "created", within_days: 7, hidden: true }, "number"],
      ["<local>", "history-list", { origin: "dev", query: "回" }, "number"],
      // 独立查看窗按会话 ID 要那一行。
      ["<local>", "history-list", { origin: "dev", sid: "s-1" }, "number"],
    ]);
  });
  it("拒绝原样抛（那台常驻不在也是）、界面不另去问那台", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "chan_call" ? Promise.reject(refusedReply("resident_absent", "dev 后端未在运行")) : Promise.resolve(chanReply(empty)),
    );
    await expect(fetchList("dev", { fresh: true })).rejects.toThrow();
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
