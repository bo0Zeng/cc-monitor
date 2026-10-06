/**
 * `src/frontend/ui/history-reads.ts` 的判据：历史注解与上次账号表经通道问常驻后端，按形状收。
 *
 * 守的要求：「用户的历史注解一条不许丢」—— 注解的读写者是本机常驻后端，界面严格收它回的那一条。
 *
 * 判据：
 * 1. **严格收**：注解那一条 · 上次账号表多一格 / 缺一格 / 类型不对 ⇒ 抛（不猜、不补默认值）。
 * 2. **问的是谁、带了什么**：改注解问 `<local>`、带 `{sid, patch}`；上次账号问会话所在那台；每一发都显式给期限。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { annotate, decodeEntry, decodeLastAccounts, HistoryShapeError, lastAccounts } from "../../../src/frontend/ui/history-reads";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const chanCalls = (): ChanCallArgs[] =>
  invokeMock.mock.calls.filter((c) => c[0] === "chan_call").map((c) => c[1] as ChanCallArgs);

beforeEach(() => {
  invokeMock.mockReset();
});

describe("严格收：形状不对就抛，不猜", () => {
  it("注解那一条与上次账号表", () => {
    const e = { starred: true, customTitle: null, hidden: false, updatedAt: 1 };
    expect(decodeEntry({ entry: e })).toEqual(e);
    // 「上次用哪个号起的」不归注解了：带着那一格 ⇒ 当形状不对。
    expect(() => decodeEntry({ entry: { ...e, lastAccount: "a" } })).toThrow(HistoryShapeError);
    expect(() => decodeEntry({ entry: { ...e, extra: 1 } })).toThrow(HistoryShapeError);
    expect(() => decodeEntry(e)).toThrow(HistoryShapeError);
    expect(decodeLastAccounts({ accounts: { s: "a" } })).toEqual({ s: "a" });
    expect(() => decodeLastAccounts({ accounts: { s: 1 } })).toThrow(HistoryShapeError);
  });
});

describe("问的是谁、带了什么", () => {
  it("改注解问 <local>；上次账号问会话所在那台；请求体逐键、显式给期限", async () => {
    invokeMock.mockImplementation((_cmd: string, a: ChanCallArgs) =>
      Promise.resolve(
        a.op === "history-annotate"
          ? chanReply({ entry: { starred: true, customTitle: null, hidden: false, updatedAt: 9 } })
          : chanReply({ accounts: {} }),
      ),
    );
    await annotate("s1", { customTitle: "" });
    await lastAccounts("dev");
    expect(chanCalls().map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["<local>", "history-annotate", { sid: "s1", patch: { customTitle: "" } }],
      ["dev", "history-last-accounts", {}],
    ]);
    for (const a of chanCalls()) expect(a.leftMs, `${a.op} 没给期限`).toBeGreaterThan(0);
  });
});
