/**
 * 会话的顺序用键盘改先后（`rot-keys.ts`，同 `rot-drag.ts` 那张状态表）：↑↓ 挪焦点；空格拿起 · ↑↓ 挪空槽 · 空格 / Enter 放下 · Esc 退回；
 * 没拿起时 Enter ＝ 点。每一步读屏念一句骨架字；不可挪的念回包那一句为什么。夹具同 `rot-order.vitest.ts` 那一形，期望手写。
 */
import { describe, expect, it, vi } from "vitest";
import { createOrder, type OrderActs } from "../../../src/frontend/ui/rot-order";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { RotLine } from "../../../src/frontend/ui/generated/RotLine";
import type { RotationViewReply } from "../../../src/frontend/ui/generated/RotationViewReply";

const line = (account: string, group: RotLine["group"], more: Partial<RotLine> = {}): RotLine => ({
  account,
  group,
  state: `夹具·${account}`,
  movable: group === "next",
  ...more,
});

const VIEW: RotationViewReply = {
  now: 1_800_000_000,
  nowText: "14:30",
  title: null,
  strategy: { id: "夹具策略", description: "", summary: "", rev: "h:1", remove: { enabled: true }, forNew: false, tzUnknown: false, hand: false },
  board: { all: [], rows: [] },
  start: "team",
  current: "team",
  lineup: [line("team", "use"), line("lab", "next"), line("personal", "next"), line("work", "out", { stay: "夹具·work 此刻无法发送" })],
  manual: false,
  off: [],
  accounts: [],
  listed: 0,
  decision: null,
  log: [],
  hints: [],
};

function mount() {
  const a = {
    move: vi.fn((_a: string, _to: number) => Promise.resolve("done" as const)),
    off: vi.fn(),
    clear: vi.fn(),
    useNow: vi.fn(),
  } satisfies OrderActs;
  const o = createOrder("<local>", a, () => {});
  o.draw({ view: VIEW, refused: null, offline: false });
  document.body.replaceChildren(o.el);
  const rs = [...o.el.querySelectorAll<HTMLElement>("[data-rot-row]")];
  rs.forEach((r, k) => (r.getBoundingClientRect = () => ({ left: 0, right: 300, top: k * 48, bottom: k * 48 + 40, width: 300, height: 40, x: 0, y: k * 48, toJSON: () => ({}) })));
  const key = (k: string): void => void (document.activeElement ?? o.el).dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));
  const said = (): string => o.el.querySelector<HTMLElement>("[aria-live]")?.textContent ?? "";
  return { o, a, rs, key, said };
}

describe("键盘改先后", () => {
  it("★ 焦点在第 3 行：空格 · ↑ · 空格 ⇒ 只交一次 move {personal, to 1}；每一步念一句（空槽念第几格）", () => {
    const { a, rs, key, said } = mount();
    rs[2].focus();
    key(" ");
    expect(said()).toBe(copyText("rot.order.lifted"));
    key("ArrowUp");
    expect(said()).toBe("2");
    key(" ");
    expect(said()).toBe(copyText("rot.order.dropped"));
    expect(a.move.mock.calls).toEqual([["personal", 1]]);
  });

  it("★ 挪到第一格念「放下即换过去」；Esc 中途 ⇒ 一次都不交", () => {
    const { a, rs, key, said } = mount();
    rs[2].focus();
    key(" ");
    key("ArrowUp");
    key("ArrowUp");
    expect(said()).toBe(copyText("rot.order.atFirst"));
    key("Escape");
    expect(said()).toBe(copyText("rot.order.cancel"));
    key(" ");
    key("Escape");
    expect(a.move).not.toHaveBeenCalled();
  });

  it("★ 没拿起时 ↑↓ 挪焦点、Enter ＝ 点（出小窗）；不可挪的行空格念回包那一句为什么", () => {
    const { a, rs, key, said } = mount();
    rs[1].focus();
    key("ArrowDown");
    expect(document.activeElement).toBe(rs[2]);
    key("ArrowDown");
    key(" ");
    expect(said()).toBe("夹具·work 此刻无法发送");
    key("Enter");
    expect(document.querySelector("[data-rot-pop]")).not.toBeNull();
    expect(a.move).not.toHaveBeenCalled();
  });
});
