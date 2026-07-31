/**
 * S6：cc-bus 驾驶舱作为顶层视图的外壳行为。
 *
 * 壳很薄，但有三条性质值得钉 —— 它们都是「搬家时最容易搬丢」的那类。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));
vi.mock("../accounts", () => ({
  fetchAccounts: vi.fn().mockResolvedValue({ accounts: [] }),
  selectableAccounts: () => [],
}));
const { pushed, popped } = vi.hoisted(() => ({
  pushed: [] as unknown[],
  popped: [] as unknown[],
}));
vi.mock("../keybindings/registry", () => ({
  dispatcher: {
    pushOverlay: (o: unknown) => void pushed.push(o),
    popOverlay: (o: unknown) => void popped.push(o),
  },
}));

import { invoke } from "@tauri-apps/api/core";
import { CcBusView } from "./cc-bus-view";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

beforeEach(() => {
  pushed.length = 0;
  popped.length = 0;
  mockInvoke.mockReset();
  mockInvoke.mockResolvedValue([]);
  document.body.replaceChildren();
});

describe("CcBusView", () => {
  it("★ 构造时**不**建驾驶舱、不发请求（一个从不用 cc-bus 的人不该为它付往返）", () => {
    new CcBusView();
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(document.querySelector(".cc-bus-view")).toBeNull();
  });

  it("首次 open 才建驾驶舱本体；再 open 不重复建", () => {
    const v = new CcBusView();
    v.open();
    const sections = document.querySelectorAll(".cc-bus-view-body > *").length;
    expect(sections).toBe(1);
    v.close();
    v.open();
    expect(document.querySelectorAll(".cc-bus-view-body > *").length).toBe(1);
  });

  it("open / close 进出 overlay 栈（Esc 要能逐层关，别漏 pop）", () => {
    const v = new CcBusView();
    v.open();
    expect(v.isVisible()).toBe(true);
    expect(pushed).toHaveLength(1);
    v.close();
    expect(v.isVisible()).toBe(false);
    expect(popped).toHaveLength(1);
    expect(document.querySelector(".cc-bus-view")).toBeNull();
  });

  it("handleEsc = 关闭；重复 open/close 幂等", () => {
    const v = new CcBusView();
    v.open();
    v.open(); // 幂等
    expect(pushed).toHaveLength(1);
    v.handleEsc();
    expect(v.isVisible()).toBe(false);
    v.close(); // 幂等
    expect(popped).toHaveLength(1);
  });
});
