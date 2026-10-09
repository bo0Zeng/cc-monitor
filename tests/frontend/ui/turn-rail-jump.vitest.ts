// 轮次刻度点了 / Alt+↑↓ 跳到一轮、那一段还取不回来（骨架没接上、也不在尾部窗口里）：跳不过去就算了 ——
// 不许变成一条没人接的 rejection（全局兜底会把整条状态栏换成一行「REJ: …」，任务 · agent · 账号几枚一起没了）。
import { describe, it, expect, vi, afterEach } from "vitest";
import { TabStreamView } from "../../../src/frontend/ui/tab-stream-view";
import { TabStore } from "../../../src/frontend/ui/tab-store";

describe("刻度跳不过去", () => {
  const seen: unknown[] = [];
  const onRej = (r: unknown): void => void seen.push(r);
  afterEach(() => {
    process.off("unhandledRejection", onRej);
    seen.length = 0;
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("点一格、那一轮还取不回来 ⇒ 不留没人接的 rejection", async () => {
    process.on("unhandledRejection", onRej);
    vi.stubGlobal("ResizeObserver", class { observe(): void {} unobserve(): void {} disconnect(): void {} });
    vi.stubGlobal("CSS", { escape: (s: string) => s });
    const store = new TabStore();
    const tsv = new TabStreamView(store, document.createElement("div"), {} as never);
    const dom = tsv.mountTabDom("t");
    store.tabs.set("t", {
      sessionId: "t",
      skeleton: null,
      skeletonFetch: "done",
      window: { pendingCount: 0, peek: () => [] },
      ...dom,
    } as never);
    store.activeId = "t";
    (dom.turnFold as unknown as { turns: unknown[] }).turns = [{ uuid: "u-far", at: 0, parts: [{ text: "x", tone: "plain" }], ending: [] }];
    dom.turnRail.render();
    const tick = dom.turnRail.el.querySelector<HTMLElement>(".turn-tick");
    expect(tick).not.toBeNull();
    tick!.click();
    dom.turnRail.step(-1);
    await new Promise((r) => setTimeout(r, 20));
    expect(seen).toEqual([]);
  });
});
