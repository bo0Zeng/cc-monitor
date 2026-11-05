/**
 * 会话的顺序（`rot-order.ts`）：照 `rotation-view` 会话形的 `lineup`（一号一行，字都是后端写好的）画；拖一行改先后（状态机 `rot-drag.ts`），
 * 放下交一次 `move`，回包才定。号名 · 字全是合成的夹具，行的几何是量好的假框。期望手写。
 */
import { describe, expect, it, vi } from "vitest";
import { createOrder, type MoveEnd, type OrderActs } from "../../../src/frontend/ui/rot-order";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { RotAccount } from "../../../src/frontend/ui/generated/RotAccount";
import type { RotLine } from "../../../src/frontend/ui/generated/RotLine";
import type { RotationViewReply } from "../../../src/frontend/ui/generated/RotationViewReply";

const NOW = 1_800_000_000;
const fact = (account: string, u5: number): RotAccount => ({
  account,
  plate: { color: 3, label: account, text: account.slice(0, 1) },
  full: u5 >= 100 ? ["5h"] : [],
  kind: "sub",
  w5h: { open: true, used: u5 },
  w7d: { open: true, used: 10 },
  login: "ok",
});
const line = (account: string, group: RotLine["group"], state: string, more: Partial<RotLine> = {}): RotLine => ({
  account,
  group,
  state,
  movable: group === "next",
  ...more,
});
const LINEUP: RotLine[] = [
  line("team", "use", "夹具·在用"),
  line("lab", "next", "夹具·下一个 · 约 16:50", { why: "夹具·lab 的理由" }),
  line("personal", "next", "夹具·备着"),
  line("work", "out", "夹具·16:07 重置", { stay: "夹具·work 此刻无法发送" }),
  line("api", "off", "夹具·先不用", { stay: "夹具·本会话先不用 api" }),
];

function view(over: Partial<RotationViewReply> = {}): RotationViewReply {
  return {
    now: NOW,
    nowText: "14:30",
    title: "夹具会话",
    strategy: { id: "夹具策略", description: "夹具说明", summary: "夹具说明", rev: "h:1", inUse: [], forNew: false, tzUnknown: false, hand: false },
    board: { all: [], rows: [{ account: "lab", row: 0, line: "夹具·lab 那一行", hover: "", cells: [], rules: ["夹具·lab 的规则"], marks: [] }] },
    start: "team",
    current: "team",
    lineup: LINEUP,
    manual: false,
    off: ["api"],
    accounts: [fact("lab", 30), fact("team", 50), fact("personal", 0), fact("work", 100), fact("api", 0)],
    listed: 1,
    decision: null,
    log: [],
    hints: [],
    can: { reorder: true, off: true, edit: true },
    ...over,
  };
}

function acts(end: MoveEnd | Promise<MoveEnd> = "done") {
  return {
    move: vi.fn((_a: string, _to: number) => Promise.resolve(end)),
    off: vi.fn(),
    clear: vi.fn(),
    useNow: vi.fn(),
    editStrategy: vi.fn(),
  } satisfies OrderActs;
}

const rows = (root: HTMLElement): HTMLElement[] => [...root.querySelectorAll<HTMLElement>("[data-rot-row]")];
/** 每行高 40、行距 8。 */
const box = (k: number) => ({ left: 0, right: 300, top: k * 48, bottom: k * 48 + 40, width: 300, height: 40, x: 0, y: k * 48, toJSON: () => ({}) });
const shiftOf = (r: HTMLElement): number => Number(/translateY\((-?\d+(?:\.\d+)?)px\)/.exec(r.style.transform)?.[1] ?? 0);

function mount(a: OrderActs, over: Partial<RotationViewReply> = {}, opt: { offline?: boolean; refused?: { account: string; text: string } } = {}) {
  const o = createOrder("<local>", a, () => {});
  o.draw({ view: view(over), refused: opt.refused ?? null, offline: opt.offline ?? false });
  document.body.replaceChildren(o.el);
  rows(o.el).forEach((r, k) => (r.getBoundingClientRect = () => box(k)));
  return o;
}
const press = (r: HTMLElement, y: number): void => void r.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, button: 0, clientX: 10, clientY: y }));
const moveTo = (y: number): void => void document.dispatchEvent(new MouseEvent("pointermove", { bubbles: true, clientX: 10, clientY: y }));
const up = (y: number): void => void document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, clientX: 10, clientY: y }));

describe("顺序一号一行（字照回包）", () => {
  it("★ 右格就是后端那一串；名下是策略的理由；用不了 · 先不用各成一组、没有把手；勾在行尾，点勾 ⇒ off", () => {
    const a = acts();
    const o = mount(a);
    const rs = rows(o.el);
    expect(rs.map((r) => r.dataset.account)).toEqual(["team", "lab", "personal", "work", "api"]);
    expect(rs[1].querySelector("[data-rot-state]")?.textContent).toBe("夹具·下一个 · 约 16:50");
    expect(rs[1].textContent).toContain("夹具·lab 的理由");
    expect(rs[0].dataset.inUse).toBe("true");
    expect(rs.map((r) => r.querySelector("[data-rot-handle]") !== null)).toEqual([false, true, true, false, false]);
    const heads = [...o.el.querySelectorAll<HTMLElement>("[data-rot-group]")].map((h) => h.textContent);
    expect(heads).toEqual([copyText("rot.order.groupOut"), copyText("rot.order.groupOff")]);
    const tick = (k: number) => rs[k].querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    expect(rs[1].lastElementChild?.contains(tick(1)), "勾在行尾").toBe(true);
    expect([tick(1).checked, tick(4).checked]).toEqual([true, false]);
    tick(1).click();
    tick(4).click();
    expect(a.off.mock.calls).toEqual([["lab", true], ["api", false]]);
  });

  it("★ 勾 · 拖动那句 ·［编辑策略］只照回包的 `can` 出：都不能 ⇒ 三样都不出（`strategy` 在不在不看）；能 ⇒ 都出", () => {
    const shown = (o: ReturnType<typeof mount>) => {
      press(rows(o.el)[1], 60);
      up(60);
      const pop = document.querySelector<HTMLElement>("[data-rot-pop]")!;
      return {
        tick: o.el.querySelector('input[type="checkbox"]') !== null,
        hint: (o.el.textContent ?? "").includes(copyText("rot.h.orderHint")),
        edit: [...pop.querySelectorAll("button")].some((b) => b.textContent === copyText("rot.top.edit")),
      };
    };
    expect(shown(mount(acts(), { can: { reorder: false, off: false, edit: false } }))).toEqual({ tick: false, hint: false, edit: false });
    expect(shown(mount(acts(), { strategy: null }))).toEqual({ tick: true, hint: true, edit: true });
  });

  it("★ 手动排过 ⇒ 标题右边「手动排过 ·［照策略排］」⇒ clear；没排过不出", () => {
    const a = acts();
    expect(mount(a).el.textContent).not.toContain(copyText("rot.order.manual"));
    const o = mount(a, { manual: true });
    expect(o.el.textContent).toContain(copyText("rot.order.manual"));
    [...o.el.querySelectorAll("button")].find((b) => b.textContent === copyText("rot.order.reset"))!.click();
    expect(a.clear).toHaveBeenCalledOnce();
  });

  it("★ 点一行（没拖开）⇒ 小窗：此刻 · 规则（字照回包）·［立即用］；被拦下的那一句写在顺序上方", () => {
    const a = acts();
    const o = mount(a, {}, { refused: { account: "work", text: "夹具·work 用不了那一句" } });
    expect(o.el.querySelector('[role="status"]')?.textContent).toBe("夹具·work 用不了那一句");
    press(rows(o.el)[1], 60);
    up(60);
    const pop = document.querySelector<HTMLElement>("[data-rot-pop]")!;
    expect(pop.textContent).toContain("夹具·lab 的理由");
    expect(pop.textContent).toContain("夹具·lab 的规则");
    [...pop.querySelectorAll("button")].find((b) => b.textContent === copyText("rot.order.useNow"))!.click();
    expect(a.useNow).toHaveBeenCalledWith("lab");
    expect(a.move).not.toHaveBeenCalled();
  });
});

describe("拖一行：跟手 · 让位 · 放下 · 回包才定", () => {
  it("★ 按下挪 30px：被拖的那一行 transform 跟着变、它下面那一行已让位；松手前不交 move", () => {
    const a = acts();
    const o = mount(a);
    const rs = rows(o.el);
    press(rs[1], 60);
    moveTo(90);
    expect(rs[1].dataset.lifted).toBe("true");
    expect(shiftOf(rs[1])).toBe(30);
    expect(shiftOf(rs[2]), "personal 往上让一格").toBe(-48);
    expect(a.move).not.toHaveBeenCalled();
    up(90);
    expect(a.move.mock.calls).toEqual([["lab", 2]]);
  });

  it("★ 悬停第一格 ⇒ 在用那一行滑到第 2 格、空槽写「松手换到 X」；松手 ⇒ move to 0", () => {
    const a = acts();
    const o = mount(a);
    const rs = rows(o.el);
    press(rs[2], 110);
    moveTo(110 - 96);
    expect(shiftOf(rs[0])).toBe(48);
    expect(o.el.querySelector("[data-rot-first]")?.textContent).toBe(`${copyText("rot.order.dropFirst")}personal`);
    up(14);
    expect(a.move.mock.calls).toEqual([["personal", 0]]);
  });

  it("★ 不可挪的行按下挪过 4px 不起；离线不拿起；Esc 退回不交", () => {
    const a = acts();
    let o = mount(a);
    press(rows(o.el)[3], 150);
    moveTo(100);
    expect(rows(o.el)[3].dataset.lifted).toBeUndefined();
    up(100);
    expect(document.querySelector("[data-rot-pop]"), "拖过就不当点").toBeNull();
    o = mount(a, {}, { offline: true });
    press(rows(o.el)[1], 60);
    moveTo(120);
    expect(rows(o.el)[1].dataset.lifted).toBeUndefined();
    up(120);
    o = mount(a);
    press(rows(o.el)[1], 60);
    moveTo(120);
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(shiftOf(rows(o.el)[2])).toBe(0);
    up(120);
    expect(a.move).not.toHaveBeenCalled();
  });

  it("★ 等回包时：列表不收新的拖动、手上这一份不被重画盖掉；回包成 ⇒ 回静止（之后照后端重画）", async () => {
    let done!: (e: MoveEnd) => void;
    const a = acts(new Promise<MoveEnd>((r) => (done = r)));
    const o = mount(a);
    press(rows(o.el)[2], 110);
    moveTo(50);
    up(50);
    expect(o.busy()).toBe(true);
    o.draw({ view: view({ lineup: [...LINEUP].reverse() }), refused: null, offline: false });
    expect(rows(o.el).map((r) => r.dataset.account)[0], "等回包时推送不重画").toBe("team");
    press(rows(o.el)[1], 60);
    moveTo(120);
    expect(rows(o.el)[1].dataset.lifted).toBeUndefined();
    done("done");
    await Promise.resolve();
    await Promise.resolve();
    expect(o.busy()).toBe(false);
  });

  it("★ 拒 ⇒ 滑回拿起前的位置；没回包 ⇒ 留在放下处标「没确认」", async () => {
    const back = mount(acts("back"));
    press(rows(back.el)[2], 110);
    moveTo(50);
    up(50);
    await Promise.resolve();
    await Promise.resolve();
    expect(rows(back.el).map(shiftOf)).toEqual([0, 0, 0, 0, 0]);
    const unsure = mount(acts("unsure"));
    press(rows(unsure.el)[2], 110);
    moveTo(50);
    up(50);
    await Promise.resolve();
    await Promise.resolve();
    const r = rows(unsure.el)[2];
    expect(r.dataset.unconfirmed).toBe("true");
    expect(r.textContent).toContain(copyText("rot.order.unconfirmed"));
  });
});
