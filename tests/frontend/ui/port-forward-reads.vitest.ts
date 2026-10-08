/**
 * 「端口转发 3 条进本机后端（帧命令，界面 `call(<local>)`；后端不在按 D11 明说）」。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | TS 解码器读得懂**后端真出的**转发清单 —— 同一份金样，后端 `dial_forwards_tests::a_failed_ack_never_enters_the_ledger_and_stop_drops_the_link` 写它（异源：Rust 造、TS 解） | 「金样」 |
 * | 形状不对 ⇒ 抛，不替后端补值 | 「严格收」 |
 * | 三问都问 `<local>`、对的帧命令、对的请求体；本机后端不在 ⇒ 抛（不当成「没有转发」） | 「请求」 |
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { decodeForwards, listForwards, startForward, stopForward } from "../../../src/frontend/ui/port-forward-reads";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/backend-policy";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, NO_CHANNEL, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = JSON.parse(
  readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/forward-list.golden.json"), "utf8"),
) as { forwards: Record<string, unknown>[] };

beforeEach(() => {
  invokeMock.mockReset();
});

describe("金样：后端出的转发清单，TS 这一侧读得懂", () => {
  it("照收", () => {
    expect(decodeForwards(golden)).toEqual([
      { id: "fwd-1", origin: "dev", localPort: 15432, remoteHost: "localhost", remotePort: 5432, state: "running", connCount: 2 },
    ]);
  });
});

describe("严格收", () => {
  const row = golden.forwards[0]!;
  it.each([
    ["多一格", { ...row, error: null }],
    ["缺一格", { ...row, connCount: undefined }],
    ["状态不认得", { ...row, state: "paused" }],
    ["端口不是整数", { ...row, localPort: "15432" }],
  ])("%s ⇒ 抛", (_n, v) => {
    expect(() => decodeForwards({ forwards: [JSON.parse(JSON.stringify(v))] })).toThrow();
  });
});

describe("请求", () => {
  it("三问都问本机那台、对的帧命令与请求体", async () => {
    const seen: { op: string; origin: string; body: unknown }[] = [];
    invokeMock.mockImplementation((cmd: string, args: ChanCallArgs) => {
      expect(cmd).toBe("chan_call");
      seen.push({ op: args.op, origin: args.origin, body: chanArgsJson(args) });
      return Promise.resolve(chanReply(args.op === "forward-list" ? golden : { id: "fwd-1" }));
    });
    expect(await startForward({ origin: "dev", localPort: 15432, remoteHost: "localhost", remotePort: 5432 })).toBe("fwd-1");
    await stopForward("fwd-1");
    expect(await listForwards()).toHaveLength(1);
    expect(seen).toEqual([
      { op: "forward-start", origin: LOCAL_ORIGIN, body: { origin: "dev", localPort: 15432, remoteHost: "localhost", remotePort: 5432 } },
      { op: "forward-stop", origin: LOCAL_ORIGIN, body: { id: "fwd-1" } },
      { op: "forward-list", origin: LOCAL_ORIGIN, body: {} },
    ]);
  });
  it("〔MIG-1 续〕带上那台的配置 ⇒ 请求体里多 `machine` / `jump`（流没起的那台由本机后端按它自己拨）", async () => {
    let body: unknown = null;
    invokeMock.mockImplementation((cmd: string, args: ChanCallArgs) => {
      if (cmd !== "chan_call") return Promise.resolve(undefined);
      body = chanArgsJson(args);
      return Promise.resolve(chanReply({ id: "fwd-2" }));
    });
    const machine = { label: "dev", host: "198.51.100.9", port: 22, user: "u", keyPath: "", hostKeyFingerprint: "", addresses: [], jump: "", resumeCommand: "", connect: true };
    await startForward({ origin: "dev", localPort: 1, remoteHost: "h", remotePort: 2 }, { machine, jump: null });
    expect(body).toEqual({ origin: "dev", localPort: 1, remoteHost: "h", remotePort: 2, machine, jump: null });
  });
  it("本机后端不在 ⇒ 抛（不是空清单）", async () => {
    invokeMock.mockRejectedValue(NO_CHANNEL);
    await expect(listForwards()).rejects.toThrow();
  });
});
