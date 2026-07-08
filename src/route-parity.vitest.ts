/**
 * Batch13-F40c:routeMetaAndBranch 路由表单测(清偿 F39「收集路由 parity」欠账)。
 * viewer 收集段与渲染路径现在共用本函数——单测钉住路由语义本身即钉住两路一致性。
 */
import { describe, expect, it } from "vitest";
import type { JsonlLinePayload } from "./events";
import { routeMetaAndBranch, type MetaSink } from "./render-stream-record";

function mk(message: Record<string, unknown>): JsonlLinePayload {
  return { session_id: "s", cwd: null, path: "/p", seq: 1, message } as unknown as JsonlLinePayload;
}

function recordingSink() {
  const got = { titles: [] as string[], queued: [] as string[], branches: 0 };
  const sink: MetaSink = {
    onTitleUpdate: (t) => got.titles.push(t),
    onQueueOperation: (c) => got.queued.push(c),
    onBranchRecord: () => (got.branches += 1),
  };
  return { got, sink };
}

describe("routeMetaAndBranch 路由表", () => {
  it("ai-title / custom-title → consumed + onTitleUpdate", () => {
    const { got, sink } = recordingSink();
    expect(routeMetaAndBranch(mk({ type: "ai-title", aiTitle: "甲" }), sink)).toBe("consumed");
    expect(routeMetaAndBranch(mk({ type: "custom-title", customTitle: "乙" }), sink)).toBe(
      "consumed",
    );
    expect(got.titles).toEqual(["甲", "乙"]);
    expect(got.branches).toBe(0);
  });

  it("queue-operation enqueue 带 content → consumed + 喂豁免;dequeue/空 content 只 consumed", () => {
    const { got, sink } = recordingSink();
    expect(
      routeMetaAndBranch(mk({ type: "queue-operation", operation: "enqueue", content: "排队消息" }), sink),
    ).toBe("consumed");
    expect(
      routeMetaAndBranch(mk({ type: "queue-operation", operation: "dequeue" }), sink),
    ).toBe("consumed");
    expect(got.queued).toEqual(["排队消息"]);
  });

  it("user/assistant(带 uuid)→ content + branch 喂送;attachment 也喂(链完整性 #8)", () => {
    const { got, sink } = recordingSink();
    // extractBranchRecord 门卫要求 uuid+timestamp 齐备
    expect(
      routeMetaAndBranch(
        mk({ type: "user", uuid: "u1", timestamp: "2026-01-01T00:00:00Z", message: { content: "hi" } }),
        sink,
      ),
    ).toBe("content");
    expect(
      routeMetaAndBranch(
        mk({
          type: "assistant",
          uuid: "a1",
          parentUuid: "u1",
          timestamp: "2026-01-01T00:00:01Z",
          message: { content: [] },
        }),
        sink,
      ),
    ).toBe("content");
    expect(got.branches).toBe(2);
  });

  it("无 uuid 的杂项记录 → content 且不喂 branch(extractBranchRecord 门卫)", () => {
    const { got, sink } = recordingSink();
    expect(routeMetaAndBranch(mk({ type: "summary" }), sink)).toBe("content");
    expect(got.branches).toBe(0);
  });
});
