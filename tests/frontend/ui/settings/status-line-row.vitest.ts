/**
 * 每台「状态行」那一格：模板进输入框（没设 ⇒ 那台回的缺省）· 敲字问预览、左右两段与认不出的那一句照回包摆 · 离开那一格存 ·
 * 恢复缺省发 `null`。那台的回包是假的（照后端那一形）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const DEFAULT = "{name}|{time}";
const { disk, sets, asks } = vi.hoisted(() => ({
  disk: { statusLine: undefined as string | undefined, state: "ok" },
  sets: [] as unknown[],
  asks: [] as (string | null)[],
}));
const reply = () => ({
  state: disk.state,
  reason: disk.state === "ok" ? null : "那台读不懂",
  detail: null,
  path: null,
  prefs: disk.statusLine === undefined ? {} : { statusLine: disk.statusLine },
});
vi.mock("../../../../src/frontend/ui/machine-prefs", () => ({
  readPrefs: async () => reply(),
  setPrefs: async (_origin: string, patch: { statusLine?: string | null }) => {
    sets.push(patch);
    disk.statusLine = patch.statusLine ?? undefined;
    return reply();
  },
}));
vi.mock("../../../../src/frontend/ui/ipc/chan-caller", () => ({
  budgetWithin: () => ({}),
  callTyped: async (_origin: string, name: string, req: { template: string | null }) => {
    expect(name).toBe("status-line-preview");
    asks.push(req.template);
    const t = req.template ?? DEFAULT;
    const unknown = t.includes("{nope}") ? ["nope"] : [];
    const [l, r = ""] = t.split("|");
    return {
      line: t.trim() === "" ? null : `L:${l} R:${r}`,
      unknownSaid: unknown.length ? "认不出：{nope}" : null,
      fields: [{ key: "{name}", says: "说明之一" }],
      defaultTemplate: DEFAULT,
      from: "照 w1 此刻算",
    };
  },
}));

import { statusLineRow } from "../../../../src/frontend/ui/settings/status-line-row";

const settle = async () => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
};
const mount = () => {
  const m = statusLineRow("devbox");
  document.body.replaceChildren(m.row);
  m.load();
  return m.row;
};
const input = (row: HTMLElement) => row.querySelector<HTMLInputElement>("input")!;
const text = (row: HTMLElement) => row.textContent ?? "";

beforeEach(() => {
  disk.statusLine = undefined;
  disk.state = "ok";
  sets.length = 0;
  asks.length = 0;
});

describe("每台那一格状态行：问那台", () => {
  it("★ 没设 ⇒ 输入框是那台回的缺省模板，预览照回包摆，字段表照回包列", async () => {
    const row = mount();
    expect(input(row).disabled, "读回之前不可改").toBe(true);
    await settle();
    expect(asks).toEqual([null]);
    expect(input(row).value).toBe(DEFAULT);
    expect(input(row).disabled).toBe(false);
    expect(text(row)).toContain("L:{name} R:{time}");
    expect(text(row)).toContain("照 w1 此刻算");
    expect(text(row)).toContain("说明之一");
  });

  it("★ 设过的照它；敲字问预览、认不出的那一句照回包；离开那一格存", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    disk.statusLine = "{name}";
    const row = mount();
    await vi.runAllTimersAsync();
    expect(input(row).value).toBe("{name}");
    input(row).value = "{nope}|x";
    input(row).dispatchEvent(new Event("input"));
    await vi.runAllTimersAsync();
    expect(asks.at(-1)).toBe("{nope}|x");
    expect(text(row)).toContain("认不出：{nope}");
    expect(text(row)).toContain("L:{nope}");
    input(row).dispatchEvent(new Event("change"));
    await vi.runAllTimersAsync();
    expect(sets).toEqual([{ statusLine: "{nope}|x" }]);
    vi.useRealTimers();
  });

  it("★ 恢复缺省发 null，输入框回到缺省模板", async () => {
    disk.statusLine = "{name}";
    const row = mount();
    await settle();
    row.querySelector<HTMLButtonElement>("button")!.click();
    await settle();
    expect(sets).toEqual([{ statusLine: null }]);
    expect(input(row).value).toBe(DEFAULT);
  });

  it("没改就离开那一格 ⇒ 不存", async () => {
    const row = mount();
    await settle();
    input(row).dispatchEvent(new Event("change"));
    await settle();
    expect(sets).toEqual([]);
  });

  it("那台读不懂 ⇒ 那一格禁用、说那台那一句", async () => {
    disk.state = "unreadable";
    const row = mount();
    await settle();
    expect(input(row).disabled).toBe(true);
    expect(text(row)).toContain("那台读不懂");
  });
});
