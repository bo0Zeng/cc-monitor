/** Batch13-F40b:RecordTimeline 查询扩展(maxSeq/removeByElement)单测——真 MessageStream + jsdom */
import { describe, expect, it } from "vitest";

// jsdom 无 ResizeObserver(MessageStream 构造需要)——空壳即可,贴底行为不在本测范围
globalThis.ResizeObserver = class {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
} as unknown as typeof ResizeObserver;

import { MessageStream } from "./stream";
import { RecordTimeline } from "./record-timeline";

function setup() {
  const root = document.createElement("div");
  document.body.appendChild(root);
  const stream = new MessageStream(root);
  const tl = new RecordTimeline(stream);
  const mk = (seq: number) => {
    const el = document.createElement("div");
    el.textContent = `#${seq}`;
    tl.insert({ seq, element: el, kind: "card", toolGroup: null });
    return el;
  };
  return { stream, tl, mk };
}

describe("RecordTimeline F40b 查询扩展", () => {
  it("maxSeq:空 timeline 为 -Infinity,插入后为最高 seq(乱序插入也取最高)", () => {
    const { tl, mk } = setup();
    expect(tl.maxSeq).toBe(Number.NEGATIVE_INFINITY);
    mk(10);
    mk(30);
    mk(20); // 乱序中部插入
    expect(tl.maxSeq).toBe(30);
    expect(tl.size).toBe(3);
  });

  it("removeByElement:模拟 reconcile(摘 DOM+删账)后,后续插入 anchor 正确不悬空", () => {
    const { tl, mk, stream } = setup();
    mk(10);
    const mid = mk(20); // 孤儿 fallback 卡
    mk(30);
    mid.remove(); // reconcile 摘 DOM
    tl.removeByElement(mid); // S-6:同步删账
    expect(tl.size).toBe(2);
    // seq 21 的右邻居现在是 30(20 已出账)→ 正常 insertBefore,非降级尾追加
    const el21 = document.createElement("div");
    el21.textContent = "#21";
    tl.insert({ seq: 21, element: el21, kind: "card", toolGroup: null });
    const order = [...stream.contentElement.children].map((c) => c.textContent);
    expect(order).toEqual(["#10", "#21", "#30"]);
  });

  it("removeByElement:不存在的元素 no-op", () => {
    const { tl, mk } = setup();
    mk(1);
    tl.removeByElement(document.createElement("div"));
    expect(tl.size).toBe(1);
  });

  it("removeByElement 后 maxSeq 随尾删更新", () => {
    const { tl, mk } = setup();
    mk(10);
    const tail = mk(99);
    tl.removeByElement(tail);
    expect(tl.maxSeq).toBe(10);
  });
});
