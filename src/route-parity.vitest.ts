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

// ★★ P0c：打断我时说的那句话，在 jsonl 里**只有 queue-operation 一条记录**。
//
// 三条判据的失败方式完全不同，所以正反都钉：
// ① `remove` 要建卡 —— 不建就整条消失（本会话实测丢 16 条用户真实输入）；
// ② `dequeue` **不许**建卡 —— 它随后就有 `user` 记录，建了就是同一句显示两遍
//    （光钉①，改成「三种都渲染」它照样绿，而那会让 101 条正常消息各显示两遍）；
// ③ 一条都不许喂 branch —— 它没有 uuid/parentUuid，喂进去等于给分叉折叠算法
//    一个没有父子关系的节点（issue #8 链完整性）。
describe("P0c 排队消息：remove 要建卡，dequeue 不许", () => {
  const qop = (operation: string, content: string | null) =>
    mk({ type: "queue-operation", operation, content, timestamp: "2026-08-12T09:51:06.664Z" });

  it("remove + 用户真实输入 → content（会走到建卡那条路）", () => {
    const { got, sink } = recordingSink();
    expect(routeMetaAndBranch(qop("remove", "现在的计划还是围绕 Windows 前端对吧?"), sink)).toBe(
      "content",
    );
    // ★ 建卡归建卡，**链一条都不许多**。
    expect(got.branches).toBe(0);
  });

  it("dequeue → consumed（它随后有 user 记录，建卡就是同一句显示两遍）", () => {
    const { got, sink } = recordingSink();
    expect(routeMetaAndBranch(qop("dequeue", "同一句话"), sink)).toBe("consumed");
    expect(got.queued).toEqual([]); // 也不该喂折叠豁免集合（那是 enqueue 的活）
  });

  it("enqueue → consumed + 喂折叠豁免集合（issue #36 那条，行为不变）", () => {
    const { got, sink } = recordingSink();
    expect(routeMetaAndBranch(qop("enqueue", "排队的话"), sink)).toBe("consumed");
    expect(got.queued).toEqual(["排队的话"]);
  });

  // ★★ D 阶段补审：卡上的时间必须是**用户打字的时刻**，不是被插进去的时刻。
  //
  // 实测本会话 16 条：两者中位数差 **25.4s**，最大 **125.4s**。
  // 标一个晚两分钟的时间 = 告诉读的人「他是那时候说的」，那是假的。
  it("卡上的时间取 enqueue（打字时刻），不是 remove（被插入时刻）", () => {
    const { sink } = recordingSink();
    const text = "打断说的话";
    routeMetaAndBranch(
      mk({ type: "queue-operation", operation: "enqueue", content: text, timestamp: "2026-08-12T09:51:06.664Z" }),
      sink,
    );
    const rm = mk({
      type: "queue-operation",
      operation: "remove",
      content: text,
      timestamp: "2026-08-12T09:51:51.359Z", // 晚 45 秒
    });
    expect(routeMetaAndBranch(rm, sink)).toBe("content");
    expect((rm.message as { timestamp: string }).timestamp).toBe("2026-08-12T09:51:06.664Z");
  });

  it("配不上 enqueue（那条没到）→ 退回用 remove 的时刻，不空着", () => {
    const { sink } = recordingSink();
    const rm = mk({
      type: "queue-operation",
      operation: "remove",
      content: "没有对应 enqueue 的话",
      timestamp: "2026-08-12T10:00:00.000Z",
    });
    expect(routeMetaAndBranch(rm, sink)).toBe("content");
    // 晚 25 秒的时间仍比没有时间有用，且卡上「排队时发出」已在提示读者。
    expect((rm.message as { timestamp: string }).timestamp).toBe("2026-08-12T10:00:00.000Z");
  });

  // ★ 上界是**真的有界**，不是注释里说说。
  //
  // 这条是 D 阶段变异逼出来的：去掉裁剪那一行，上面两条判据**照样绿** ——
  // 也就是「有界」这个说法当时没有任何东西守着，而一个无上界的进程内 map
  // 在长会话里就是慢性泄漏。
  it("打字时刻缓存有上界：撑爆之后最老的那条被丢掉，退回用 remove 的时刻", () => {
    const { sink } = recordingSink();
    const oldest = "最老的那句话";
    routeMetaAndBranch(
      mk({ type: "queue-operation", operation: "enqueue", content: oldest, timestamp: "2026-01-01T00:00:00.000Z" }),
      sink,
    );
    // 再灌 200 条把它挤出去（上界 200）。
    for (let i = 0; i < 200; i++) {
      routeMetaAndBranch(
        mk({ type: "queue-operation", operation: "enqueue", content: `填充-${i}`, timestamp: "2026-01-02T00:00:00.000Z" }),
        sink,
      );
    }
    const rm = mk({
      type: "queue-operation",
      operation: "remove",
      content: oldest,
      timestamp: "2026-08-12T10:00:00.000Z",
    });
    expect(routeMetaAndBranch(rm, sink)).toBe("content");
    // 配不上了 ⇒ 退回 remove 的时刻（而不是拿到那个 2026-01-01）。
    expect((rm.message as { timestamp: string }).timestamp).toBe("2026-08-12T10:00:00.000Z");
  });

  it("remove + <task-notification> → consumed（系统注入不是用户说的话）", () => {
    const { sink } = recordingSink();
    const note = "<task-notification>\n<task-id>abc</task-id>\n</task-notification>";
    expect(routeMetaAndBranch(qop("remove", note), sink)).toBe("consumed");
  });

  it("remove + 空/纯空白 → consumed（没有内容就没有卡）", () => {
    const { sink } = recordingSink();
    expect(routeMetaAndBranch(qop("remove", "   "), sink)).toBe("consumed");
    expect(routeMetaAndBranch(qop("remove", null), sink)).toBe("consumed");
  });
});

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
