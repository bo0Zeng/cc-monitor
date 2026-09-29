/**
 * 设计/99 §2.1 ⑬「起停帧不吃 credit、不许丢（登记一条例外）」—— TS 那一侧：例外表 == 金样（Rust `SessionStreamFrame::takes_credit` 写它，异源）；
 * 起停那几格处理掉之后**不还** credit（monitor 交它们时本来没扣），行照旧还。
 */
import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }),
}));
vi.mock("../src/ipc/commands", () => ({
  commands: new Proxy({}, { get: () => vi.fn().mockResolvedValue(undefined) }),
}));
vi.mock("../src/comms/inward/chan", async () => (await import("./test-support/chan-stream-fake.ts")).chanStreamModule);

import { bindEvents, CREDIT_EXEMPT_FRAMES } from "../src/events";
import { streamFake } from "./test-support/chan-stream-fake.ts";
import { REPO_ROOT } from "./test-support/repo-root";

const golden = JSON.parse(
  readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/session-stream-credit.golden.json"), "utf8"),
) as { credit: string[]; exempt: string[] };

describe("会话流的 credit 例外", () => {
  it("TS 那一侧的例外表 == 金样", () => {
    expect([...CREDIT_EXEMPT_FRAMES].sort()).toEqual(golden.exempt);
  });

  it("起停那几格不还 credit、照交处理器；行照还", async () => {
    streamFake.reset();
    const ended: string[] = [];
    await bindEvents(
      { onLine: () => {}, onSessionEnded: (s: string) => ended.push(s) } as never,
      { streams: [{ origin: "<local>", kind: "session-lines" }] },
    );
    streamFake.lifecycle([{ ended: { session_id: "a" } }, { idle: { session_id: "b" } }]);
    await vi.waitFor(() => expect(ended).toEqual(["a"]));
    await new Promise((r) => setTimeout(r, 20));
    expect(streamFake.subscriptions[0]!.wants.reduce((a, b) => a + b, 0), "起停那几格还了 credit").toBe(0);
    streamFake.lines([{ session_id: "s", cwd: "/p", path: "/p/s.jsonl", seq: 1, message: { type: "assistant", uuid: "u" } }]);
    await vi.waitFor(() =>
      expect(streamFake.subscriptions[0]!.wants.reduce((a, b) => a + b, 0), "行没还 credit").toBe(1),
    );
  });
});
