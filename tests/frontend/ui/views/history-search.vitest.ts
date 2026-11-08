/**
 * 历史全文搜索（`src/frontend/ui/views/history-search.ts`）：只问本机后端一次 `history-search-merge {machines, search}` ——
 * 它自己问到各台、合成一份再答（扇出 · 合并的判法在后端判据里）。界面不逐台问、不把各台回包原样交回去（不当信差）。
 *
 * typed 帧命令（`callTyped`）：入参 · 回包的类型从 Rust 生成，形状由握手指纹核过 ⇒ 这里只核「问了谁 · 交了什么 · 回包原样用」。
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
import { LOCAL } from "../../../../src/frontend/ui/ipc/origin";
import type { Merged } from "../../../../src/frontend/ui/generated/Merged";
import { chanArgsJson, chanReply, isChanCall, NO_CHANNEL, type ChanCallArgs } from "../../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const Q: FullTextQuery = { query: "kw", includeTools: false, scope: null, afterMs: null, limit: 300 };

/** 本机后端合好的那一份（替身：界面只照抄）。 */
const MERGED: Merged = { totalHits: 9, truncated: true, sessions: [], notes: ["核心·头底下那一句"], down: [{ origin: "down", said: "down 离线 · 上线后自动恢复" }], summary: "核心·结果头" };

beforeEach(() => invokeMock.mockReset());

/** 记下每一条通道问答；`history-search-merge` 答 `reply`。 */
function rig(reply: () => Promise<ArrayBuffer>) {
  invokeMock.mockImplementation((cmd: string, args: unknown) => {
    if (isChanCall(cmd, args, "history-search-merge")) return reply();
    return Promise.resolve(undefined);
  });
  return () =>
    invokeMock.mock.calls
      .filter((c) => c[0] === "chan_call")
      .map((c) => c[1] as ChanCallArgs);
}

describe("只问本机后端一次", () => {
  it("★ 只交那一份请求体（问哪几台本机后端照自己的机器表定）；不逐台问 `history-search`；回包原样用（没答上的那几台 · 读不动 · 不覆盖的那几家都在里头）", async () => {
    const calls = rig(() => Promise.resolve(chanReply(MERGED)));
    const got = await searchAllMachines({ ...Q, includeTools: true });
    const asked = calls();
    expect(asked.map((a) => [a.origin, a.op])).toEqual([[LOCAL, "history-search-merge"]]);
    expect(chanArgsJson(asked[0])).toEqual({ search: { query: "kw", limit: 300, include_tools: true } });
    expect(got).toEqual(MERGED);
  });

  it("本机后端问不到 ⇒ 整次失败（不装作「没结果」）", async () => {
    rig(() => Promise.reject(NO_CHANNEL));
    await expect(searchAllMachines(Q)).rejects.toBeTruthy();
  });
});

describe("选项只下发后端认的那几格（各台同一份）", () => {
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
    expect(searchArgs({ ...Q, titles: true })).toEqual({ query: "kw", limit: 300, titles: true });
  });
});
