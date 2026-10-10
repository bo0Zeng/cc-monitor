/**
 * `BranchFolder`：按那台后端给的主线外清单（回退掉的那几条记录的 `id`）把连续的那几张卡折成一段。
 * 界面不判谁在主线上 —— 清单整份来、整份换；这里钉的是排版：
 * - 清单换了就当场重折，同一份再来不动；新卡挂进来只排一次帧末重折（清单空 ⇒ 一次都不排）；
 * - 折叠段的展开态按段首条 id 记，重折之后继承；
 * - 差量重折 == 平铺容器上从零折（C1）。
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { BranchFolder } from "../../../src/frontend/ui/branch-fold";

/** 一份折叠层此刻的清单（判据读私有格：等价那一格要拿同一份清单去折参照）。 */
const offOf = (f: BranchFolder): Set<string> => new Set((f as unknown as { off: ReadonlySet<string> }).off);

function mount(ids: Array<string | null>): { el: HTMLElement; folder: BranchFolder } {
  const el = document.createElement("div");
  document.body.replaceChildren(el);
  for (const id of ids) {
    const c = document.createElement("div");
    c.className = "card";
    if (id) c.setAttribute("data-id", id);
    c.textContent = id ?? "sep";
    el.appendChild(c);
  }
  return { el, folder: new BranchFolder(el) };
}
const folded = (el: HTMLElement): string[][] =>
  [...el.querySelectorAll(".branch-fold-wrap")].map((w) => [...w.querySelectorAll(".branch-fold-body-inner > *")].map((k) => k.textContent ?? ""));

describe("按清单折", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("清单里连着的几张折成一段；没有 id 的元素与主线卡断段；清单换了当场重折", () => {
    const { el, folder } = mount(["a", "b", "c", null, "d", "e"]);
    folder.setOff(new Set(["b", "c", "d", "e"]));
    expect(folded(el)).toEqual([["b", "c"], ["d", "e"]]);
    folder.setOff(new Set(["a"]));
    expect(folded(el)).toEqual([["a"]]);
    folder.setOff(new Set());
    expect(folded(el)).toEqual([]);
  });

  it("展开过的那一段：重折之后还是展开的（按段首条 id 记）", () => {
    const { el, folder } = mount(["a", "b", "c"]);
    folder.setOff(new Set(["b", "c"]));
    el.querySelector<HTMLElement>(".branch-fold-header")!.click();
    folder.setOff(new Set(["b"]));
    folder.setOff(new Set(["b", "c"]));
    expect(el.querySelector(".branch-fold-wrap")?.classList.contains("expanded")).toBe(true);
  });

  it("新卡挂进来：清单非空 ⇒ 帧末重折一次（一帧来几张都只一次）；清单空 ⇒ 一次都不排", () => {
    const frames: FrameRequestCallback[] = [];
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => frames.push(cb));
    const { el, folder } = mount(["a"]);
    folder.cardsAdded();
    expect(frames.length, "清单空也排了活").toBe(0);
    folder.setOff(new Set(["x", "y"]));
    for (const id of ["x", "y"]) {
      const c = document.createElement("div");
      c.setAttribute("data-id", id);
      c.textContent = id;
      el.appendChild(c);
      folder.cardsAdded();
    }
    expect(frames.length, "一帧来两张该只排一次").toBe(1);
    expect(folded(el), "帧末之前不折").toEqual([]);
    frames.shift()!(0);
    expect(folded(el)).toEqual([["x", "y"]]);
  });

  it("dispose 之后：帧末那次醒来不动 DOM，清单也不再收", () => {
    const frames: FrameRequestCallback[] = [];
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => frames.push(cb));
    const { el, folder } = mount(["a", "b"]);
    folder.setOff(new Set(["b"]));
    folder.cardsAdded();
    folder.unwrapAll();
    folder.dispose();
    frames.shift()!(0);
    folder.setOff(new Set(["a"]));
    expect(folded(el)).toEqual([]);
  });
});

/**
 * 「`BranchFolder.rebuild()` 全量 unwrap ＋ 重新包裹」—— 三处「主动放弃增量」的 DOM 操作之一。
 * 修法：按段差量（恰好等于目标段的现存 wrap 原地不动，只拆 / 建归属变了的段）。
 * 判据（异源）：随机操作序列（插卡 —— 包括插进折叠段里、按 R6 的 `insertNode` 口径插在锚点前 —— ＋ 换主线外清单），
 * 每一步之后差量重折的 DOM == 同一逻辑序列在**平铺容器**上从零折一遍的 DOM（另一个实例）；
 * 以及「清单没变 / 只在尾巴长一条主线卡」时一个节点都不搬。
 */
describe("C1 · 差量重折 == 从零折", () => {
  function rng(seed: number): () => number {
    let a = seed >>> 0;
    return () => {
      a = (a + 0x6d2b79f5) >>> 0;
      let t = a;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }
  const card = (uuid: string | null): HTMLElement => {
    const el = document.createElement("div");
    el.className = "card";
    if (uuid) el.dataset.id = uuid;
    el.textContent = uuid ?? "sep";
    return el;
  };
  /**
   * 一段随机历史（`steps` 步，从空容器起）。每一步之后：差量重折的 DOM == 平铺容器上另一个实例从零折的 DOM。
   * 参照那一侧：同一串卡按同一逻辑顺序平铺在另一个容器里（从不折），每一步克隆一份去从零折。平铺那份跟着插卡同步长
   * （插在锚点对应的那张前面），不从被测容器里读回来 —— 被测那一侧要是把顺序弄乱了，两边就对不上。
   * → [这一段查了几步, 一共见过几个折叠段]
   */
  function history(seed: number, steps: number): [number, number] {
    const r = rng(seed);
    const el = document.createElement("div");
    const folder = new BranchFolder(el);
    const flat = document.createElement("div");
    const order: HTMLElement[] = []; // 被测那一侧的卡，按逻辑顺序（＝文档顺序）
    const twin = new Map<HTMLElement, HTMLElement>(); // 被测的卡 → 平铺那份里的同一张
    let next = 0;
    let checked = 0;
    let foldedSeen = 0;
    for (let step = 0; step < steps; step++) {
      const op = r();
      if (op < 0.45 || order.length === 0) {
        // 插一张卡：末尾，或插在随机一张现有卡前（它若在折叠段里就插进段里 —— R6 口径）
        const id = r() < 0.1 ? null : `u${next++}`;
        const c = card(id);
        const t = card(id);
        if (order.length === 0 || r() < 0.5) {
          el.appendChild(c);
          flat.appendChild(t);
          order.push(c);
        } else {
          const at = Math.floor(r() * order.length);
          const anchor = order[at];
          anchor.parentElement!.insertBefore(c, anchor);
          flat.insertBefore(t, twin.get(anchor)!);
          order.splice(at, 0, c);
        }
        twin.set(c, t);
      } else {
        // 换主线外清单：每张卡独立地以 0.3 的概率在清单里
        const off = new Set<string>();
        for (const c of order) {
          const u = c.dataset.id;
          if (u && r() < 0.3) off.add(u);
        }
        folder.setOff(off);
      }
      folder.rebuildNow();
      const ref = flat.cloneNode(true) as HTMLElement;
      new BranchFolder(ref).setOff(offOf(folder));
      if (!el.isEqualNode(ref)) expect(el.innerHTML, `种子 ${seed} 第 ${step} 步`).toBe(ref.innerHTML);
      foldedSeen += el.querySelectorAll(":scope > .branch-fold-wrap").length;
      checked++;
    }
    expect(el.querySelectorAll(".card").length, "被测那一侧的卡数与参照对不上").toBe(order.length);
    return [checked, foldedSeen];
  }

  // 三段各 100 步（三个种子，第一段就是原来那一段的前 100 步），共 300 步、每一步都查。
  //   原先是一段 300 步：卡数一路长到一百二十张，每一步从零折一整份 ⇒ 平方级，带覆盖率负载 89–150 时 8–26 s，挂在 30 s 期限上过半；
  //   jsdom 里折一段要建八九个节点，大头在产品那份折叠本身，判据这边省不出来。分三段之后每段长到四十来张（照样有插进段里 ·
  //   段并段拆 · 一步换十几段），平方那一项是原来的三分之一。
  it("随机 3 × 100 步：每一步差量结果 == 从零折", () => {
    let checked = 0;
    let foldedSeen = 0;
    for (const seed of [20260926, 20260927, 20260928]) {
      const [c, f] = history(seed, 100);
      checked += c;
      foldedSeen += f;
    }
    expect(checked).toBe(300);
    expect(foldedSeen, "反空真：序列里真出现过折叠段").toBeGreaterThan(50);
  }, 30_000); // 满载下全量套件里跑过 5 s

  it("清单没变 / 只在尾巴长一条主线卡：零搬动（DOM 一次写都没有）", () => {
    const el = document.createElement("div");
    const folder = new BranchFolder(el);
    for (let i = 0; i < 10; i++) el.appendChild(card(`k${i}`));
    folder.setOff(new Set(["k2", "k3", "k4", "k7", "k8"])); // 折出两段：k2–k4 · k7–k8
    expect(el.querySelectorAll(".branch-fold-wrap").length).toBe(2);
    const mo = new MutationObserver(() => {});
    mo.observe(el, { childList: true, subtree: true, attributes: true });
    folder.setOff(new Set(["k2", "k3", "k4", "k7", "k8"])); // 同一份清单再来
    folder.rebuildNow(); // 清单没变
    el.appendChild(card("k10"));
    const before = mo.takeRecords().length; // 只有那一次 append
    folder.rebuildNow(); // 尾巴长了一条主线卡
    const after = mo.takeRecords().length;
    mo.disconnect();
    expect(before).toBe(1);
    expect(after).toBe(0);
  });
});
