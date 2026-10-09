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
    if (id) c.setAttribute("data-uuid", id);
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
      c.setAttribute("data-uuid", id);
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
    if (uuid) el.dataset.uuid = uuid;
    el.textContent = uuid ?? "sep";
    return el;
  };
  /** 逻辑序列：顶层子节点，wrap 展开读 inner */
  const logical = (c: HTMLElement): string[] => {
    const out: string[] = [];
    for (const ch of Array.from(c.children)) {
      if (ch.classList.contains("branch-fold-wrap")) {
        for (const k of Array.from(ch.querySelector(".branch-fold-body-inner")!.children)) out.push(k.textContent ?? "");
      } else out.push(ch.textContent ?? "");
    }
    return out;
  };

  it("随机 300 步：每一步差量结果 == 从零折", () => {
    const r = rng(20260926);
    const el = document.createElement("div");
    const folder = new BranchFolder(el);
    let next = 0;
    let checked = 0;
    let foldedSeen = 0;
    for (let step = 0; step < 300; step++) {
      const op = r();
      if (op < 0.45 || el.children.length === 0) {
        // 插一张卡：末尾，或插在随机一张现有卡前（它若在折叠段里就插进段里 —— R6 口径）
        const c = card(r() < 0.1 ? null : `u${next++}`);
        const all = Array.from(el.querySelectorAll<HTMLElement>(".card"));
        if (all.length === 0 || r() < 0.5) el.appendChild(c);
        else {
          const anchor = all[Math.floor(r() * all.length)];
          anchor.parentElement!.insertBefore(c, anchor);
        }
      } else {
        // 换主线外清单：每张卡独立地以 0.3 的概率在清单里
        const off = new Set<string>();
        for (const c of Array.from(el.querySelectorAll<HTMLElement>(".card"))) {
          const u = c.dataset.uuid;
          if (u && r() < 0.3) off.add(u);
        }
        folder.setOff(off);
      }
      folder.rebuildNow();
      // 参照：同一逻辑序列的平铺容器，另一个实例从零折
      const flat = document.createElement("div");
      for (const t of logical(el)) flat.appendChild(card(t === "sep" ? null : t));
      const ref = new BranchFolder(flat);
      ref.setOff(offOf(folder));
      expect(el.innerHTML, `第 ${step} 步`).toBe(flat.innerHTML);
      foldedSeen += el.querySelectorAll(".branch-fold-wrap").length;
      checked++;
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
