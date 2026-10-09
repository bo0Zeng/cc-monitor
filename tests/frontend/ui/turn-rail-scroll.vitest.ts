/**
 * 轮次刻度跟滚动：一帧最多量一次，一次只量对数个位置。
 *
 * 以前每来一个 `scroll` 事件就同步把正文里**每一轮的开头**挨个 `getBoundingClientRect` 到视口上沿为止（贴底看长会话 ⇒ 几百次），
 * 还每次重建一张「uuid → 第几轮」的表。滚动一帧来好几个 `scroll`，骨架接上 / 补批时程序化滚动又夹在增删卡之间
 * ⇒ 每一下都逼浏览器当场排版（性能台架 WebKitGTK 冷切那一段：刻度这一项 111 次 898 ms）。
 * 现在：scroll 只排一帧，帧里量一次；轮的开头按文档顺序上下排着 ⇒ 二分找「开头已过视口上沿的最后一轮」。
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { TurnRail } from "../../../src/frontend/ui/turn-rail";
import type { TurnSummary } from "../../../src/frontend/ui/session-reads";

const box = (top: number, bottom: number) => ({ top, bottom, height: bottom - top }) as DOMRect;
const turnOf = (uuid: string): TurnSummary => ({ uuid, startText: "02:01", tools: 1, said: "", reply: "" }) as unknown as TurnSummary;
const frame = () => new Promise((r) => requestAnimationFrame(() => r(null)));

/** n 轮，每轮开头一张卡、中间夹 `filler` 张别的卡；第 i 轮开头的上沿在 `i * 100 - offset`。 */
function rig(n: number, offset: number, filler = 3) {
  const turns = Array.from({ length: n }, (_, i) => turnOf(`u${i}`));
  const content = document.createElement("div");
  let reads = 0;
  const pos = { offset };
  turns.forEach((t, i) => {
    const head = document.createElement("div");
    head.setAttribute("data-uuid", t.uuid);
    head.getBoundingClientRect = () => {
      reads++;
      return box(i * 100 - pos.offset, i * 100 - pos.offset + 20);
    };
    content.appendChild(head);
    for (let k = 0; k < filler; k++) {
      const f = document.createElement("div");
      f.setAttribute("data-uuid", `${t.uuid}-f${k}`);
      f.getBoundingClientRect = () => {
        reads++;
        return box(i * 100 - pos.offset + 25 + k * 20, i * 100 - pos.offset + 40 + k * 20);
      };
      content.appendChild(f);
    }
  });
  const scroller = document.createElement("div");
  scroller.getBoundingClientRect = () => box(0, 400);
  Object.defineProperty(scroller, "clientWidth", { value: 1200 });
  const rail = new TurnRail(scroller, content, { turns: () => turns, waiting: () => false, jump: vi.fn() });
  document.body.replaceChildren(content, rail.el);
  rail.render();
  const viewportTick = () => [...rail.el.querySelectorAll<HTMLElement>(".turn-tick")].findIndex((t) => t.dataset.viewport === "true");
  return { rail, pos, reads: () => reads, resetReads: () => (reads = 0), viewportTick };
}

/** 照定义线性数一遍：开头上沿 ≤ 视口上沿 + 8 的最后一轮；一个都没有 ⇒ 第一轮。 */
const expected = (n: number, offset: number): number => {
  let at = -1;
  for (let i = 0; i < n; i++) if (i * 100 - offset <= 8) at = i;
  return at < 0 ? 0 : at;
};

afterEach(() => document.body.replaceChildren());

describe("轮次刻度跟滚动：一帧量一次、二分", () => {
  it("一连串 scroll：同步段一次几何都不读；下一帧量一次", async () => {
    const r = rig(50, 2000);
    await frame();
    r.resetReads();
    r.pos.offset = 3000;
    for (let k = 0; k < 20; k++) r.rail.onScroll();
    expect(r.reads(), "每个 scroll 当场量 ⇒ 滚一帧量好几遍、还夹在增删卡之间逼排版").toBe(0);
    await frame();
    expect(r.viewportTick(), "下一帧照常更新").toBe(expected(50, 3000));
    expect(r.reads(), "20 个 scroll 合成一次").toBeLessThanOrEqual(2 * Math.ceil(Math.log2(50)) + 4);
  });

  it("量的是对数个位置（400 轮 · 每轮夹 3 张卡），结果与逐个数一致", async () => {
    const n = 400;
    const r = rig(n, 0);
    for (const offset of [0, 7, 9, 150, 12_345, 20_000, 39_950, 50_000]) {
      r.pos.offset = offset;
      r.resetReads();
      r.rail.onScroll();
      await frame();
      expect(r.viewportTick(), `offset ${offset}`).toBe(Math.floor(expected(n, offset) / Math.ceil(n / 60)));
      expect(r.reads(), `offset ${offset}：贴底看长会话时逐个量要几百次`).toBeLessThanOrEqual(2 * Math.ceil(Math.log2(n)) + 4);
    }
  });
});
