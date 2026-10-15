/**
 * 「起停帧不吃 credit、不许丢（登记一条例外）」—— TS 那一侧：例外表 == 金样（Rust `SessionStreamFrame::takes_credit` 写它，异源）；
 * 起停那几格处理掉之后**不还** credit（monitor 交它们时本来没扣），行照旧还。
 */
import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }),
}));
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy({}, { get: () => vi.fn().mockResolvedValue(undefined) }),
}));
vi.mock("../../../src/comms/inward/chan", async () => (await import("../../test-support/chan-stream-fake.ts")).chanStreamModule);

import { bindEvents, CREDIT_EXEMPT_FRAMES } from "../../../src/frontend/ui/events";
import { streamFake } from "../../test-support/chan-stream-fake.ts";
import { REPO_ROOT } from "../../test-support/repo-root";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";

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
    let lines = 0;
    await bindEvents(
      { onLine: () => void lines++, onSessionEnded: (s: string) => ended.push(s) } as never,
      { streams: [{ origin: "<local>", kind: "session-lines" }] },
    );
    streamFake.lifecycle([{ ended: { session_id: "a" } }, { idle: { session_id: "b" } }]);
    await vi.waitFor(() => expect(ended).toEqual(["a"]), UNTIL_CEILING);
    // 格按到的先后处理、每一片处理完当场还 credit ⇒ 后到的那一行交到处理器了，前面的起停格必然已经处理完、该还的都还了：总数只该是行那一格。
    streamFake.lines([{ session_id: "s", cwd: "/p", path: "/p/s.jsonl", seq: 1, message: { type: "assistant", uuid: "u" } }]);
    await vi.waitFor(() => expect(lines, "行没交到处理器").toBe(1), UNTIL_CEILING);
    expect(streamFake.subscriptions[0]!.wants.reduce((a, b) => a + b, 0), "起停那几格还了 credit · 或行没还").toBe(1);
  });
});
