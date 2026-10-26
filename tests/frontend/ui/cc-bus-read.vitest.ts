/**
 * 驾驶舱读面（`bus-list` · `bus-inbox`）的界面那一侧：经通道按类型问（回包形状由握手指纹管）、拒绝码逐码一句。
 * 要求：「成品的两侧对拍 … 后端测试产出 == 金样 · TS 解码器读同一份」· 驾驶舱读面经通道。
 * 金样：名单取 `cc-bus-control.golden.json` 的 `bus-list`，收件箱取 `cc-bus-read.golden.json`（后端两条金样判据读同一份）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { readInbox, readRoster } from "../../../src/frontend/ui/cc-bus-control";
import { ChanError } from "../../../src/comms/inward/chan";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, refusedReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const read = (f: string): unknown => JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__", f), "utf8"));
const list = (read("cc-bus-control.golden.json") as { "bus-list": { reply: Record<string, unknown>; codes: string[] } })["bus-list"];
const inbox = (read("cc-bus-read.golden.json") as { inbox: { reply: Record<string, unknown>; codes: string[] } }).inbox;

beforeEach(() => {
  invokeMock.mockReset();
});

describe("SH1 · 驾驶舱读面：经通道问、失败说清", () => {
  it("读名单问的是那台的 `bus-list`、读收件箱问 `bus-inbox` 并带 id", async () => {
    invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => {
      if (cmd !== "chan_call") throw new Error(`不该调 ${cmd}`);
      return chanReply(a.op === "bus-list" ? list.reply : inbox.reply);
    });
    await expect(readRoster("devbox")).resolves.toEqual(list.reply);
    await expect(readInbox("devbox", "alpha_cc")).resolves.toEqual(inbox.reply);
    const sent = invokeMock.mock.calls.map(([, a]) => [(a as ChanCallArgs).origin, (a as ChanCallArgs).op, chanArgsJson(a as ChanCallArgs)]);
    expect(sent).toEqual([
      ["devbox", "bus-list", {}],
      ["devbox", "bus-inbox", { id: "alpha_cc" }],
    ]);
  });

  it("金样里登记的拒绝码逐码一句、两两不同（读不到 ≠ 一个都没有）", async () => {
    const codes = [...new Set([...list.codes, ...inbox.codes])];
    const said = new Set<string>();
    for (const code of codes) {
      invokeMock.mockReset();
      invokeMock.mockImplementation(async () => {
        throw refusedReply(code, `detail-${code}`);
      });
      const e = await readInbox("devbox", "alpha_cc").catch((x: unknown) => x);
      expect(e, code).toBeInstanceOf(ChanError);
      said.add((e as ChanError).message);
    }
    expect(said.size).toBe(codes.length);
  });
});
