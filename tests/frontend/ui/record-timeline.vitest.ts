/** Batch13-F40b:RecordTimeline 查询扩展(maxSeq/removeByElement)单测——真 MessageStream + jsdom */
import { describe, expect, it } from "vitest";

// jsdom 无 ResizeObserver(MessageStream 构造需要)——空壳即可,贴底行为不在本测范围
globalThis.ResizeObserver = class {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
} as unknown as typeof ResizeObserver;

import { MessageStream } from "../../../src/frontend/ui/stream";
import { RecordTimeline } from "../../../src/frontend/ui/record-timeline";

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

/**
 * 〔W5-RENDER R6〕`设计/10 §3.5` D4 逐字：「`insertBefore` 的锚点可能已过期 ⇒ 降级成末尾追加，DOM 临时错序
 * （账本仍对，等下次折叠 rebuild 自愈）」。现打：rebuild **不**重排卡（它只解开 / 重包），错序不会自愈；
 * 而锚点在折叠段里时，原来爬到顶层、插在整段之前 —— 新卡若该落在段中间，同样错序。
 * 判据（相等）：两种锚点状态下插一条，**文档序**（按 `#seq` 文本取）== seq 序；已离场的后继被出账。
 */
describe("D4 · 锚点离场 / 在折叠段里：插入仍按 seq 序（`设计/10 §3.5`）", () => {
  const docOrder = (stream: MessageStream): string[] =>
    [...stream.contentElement.querySelectorAll("[data-t]")].map((e) => e.textContent ?? "");
  const card = (seq: number): HTMLElement => {
    const el = document.createElement("div");
    el.dataset.t = "1";
    el.textContent = `#${seq}`;
    return el;
  };

  it("后继被外部摘出 DOM（没出账）⇒ 新卡插在下一个还在的后继前，离场的出账", () => {
    const { tl, stream } = setup();
    const els = new Map<number, HTMLElement>();
    for (const s of [10, 20, 30]) {
      const el = card(s);
      els.set(s, el);
      tl.insert({ seq: s, element: el, kind: "card", toolGroup: null });
    }
    els.get(20)!.remove(); // 摘 DOM、不删账 —— 锚点过期
    tl.insert({ seq: 15, element: card(15), kind: "card", toolGroup: null });
    expect(docOrder(stream)).toEqual(["#10", "#15", "#30"]);
    expect(tl.size).toBe(3); // 20 出账了
  });

  it("锚点在折叠段里 ⇒ 新卡落在段内它该在的位置（不是整段之前）", () => {
    const { tl, stream } = setup();
    const els = new Map<number, HTMLElement>();
    for (const s of [10, 20, 30, 40]) {
      const el = card(s);
      els.set(s, el);
      tl.insert({ seq: s, element: el, kind: "card", toolGroup: null });
    }
    // 照 BranchFolder.wrapRun 的形状把 20、30 包进一个折叠段
    const wrap = document.createElement("div");
    wrap.className = "branch-fold-wrap";
    const body = document.createElement("div");
    const inner = document.createElement("div");
    body.appendChild(inner);
    wrap.appendChild(body);
    stream.contentElement.insertBefore(wrap, els.get(20)!);
    inner.appendChild(els.get(20)!);
    inner.appendChild(els.get(30)!);
    tl.insert({ seq: 25, element: card(25), kind: "card", toolGroup: null });
    expect(docOrder(stream)).toEqual(["#10", "#20", "#25", "#30", "#40"]);
  });
});
