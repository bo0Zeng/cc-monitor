// 新建框「一组」页：行照存好的那一组铺开；预览停手 300ms 现问 set-plan {rows}、照成品画；主按钮的字 · 能不能点照预览；启动 ＝ 先存（saveSet）后起。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const h = vi.hoisted(() => ({
  readProfiles: vi.fn(),
  writeProfiles: vi.fn(),
  planSet: vi.fn(),
  startSet: vi.fn(),
  toast: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/profiles-reads", () => ({ readProfiles: h.readProfiles, writeProfiles: h.writeProfiles }));
vi.mock("../../../src/frontend/ui/set-reads", () => ({ planSet: h.planSet }));
vi.mock("../../../src/frontend/ui/set-menu", () => ({ startSet: h.startSet }));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: h.toast }));

import { copyText } from "../../../src/frontend/ui/copy-table";
import { previewSaid, setPage } from "../../../src/frontend/ui/new-session-set";
import type { SetPlan } from "../../../src/frontend/ui/generated/SetPlan";

const book = {
  rev: "fp-1",
  profiles: [
    { name: "dev", usable: true },
    { name: "look", usable: true },
    { name: "bad", usable: false },
  ],
  sets: [
    {
      name: "工具链",
      problems: [],
      rows: [
        { cwd: "~/work/cc-monitor", alias: "dev", n: 2, tabGroup: null, line: 2 },
        { cwd: "~/work/cc-bus", alias: "look", n: 1, tabGroup: "看图", line: 3 },
      ],
    },
  ],
};

const plan = (more: number): SetPlan => ({
  rows: [
    { cwd: "~/work/cc-monitor", alias: "dev", want: 2, have: 2, more: 0, group: "dev", blocked: null },
    { cwd: "~/work/cc-bus", alias: "look", want: 1, have: 0, more, group: "看图", blocked: null },
  ],
  more,
});

beforeEach(() => {
  vi.useFakeTimers();
  for (const f of Object.values(h)) f.mockReset();
  h.readProfiles.mockResolvedValue(structuredClone(book));
  h.writeProfiles.mockResolvedValue({ wrote: true, rev: "fp-2", reload: null });
});

afterEach(() => {
  vi.useRealTimers();
  document.body.replaceChildren();
});

async function mount(name: string | null) {
  const refresh = vi.fn();
  const p = setPage(refresh);
  document.body.appendChild(p.el);
  await p.load("devbox", name);
  await vi.advanceTimersByTimeAsync(0);
  return { p, refresh };
}

describe("一组页", () => {
  it("打开存好的那一组：行照它铺开（组名留空的占位是那台算的缺省）；预览照 set-plan 成品画，主按钮「启动 N」", async () => {
    h.planSet.mockResolvedValue(plan(1));
    const { p } = await mount("工具链");
    expect(h.planSet).toHaveBeenCalledWith("devbox", {
      rows: [
        { cwd: "~/work/cc-monitor", alias: "dev", n: 2, tabGroup: null },
        { cwd: "~/work/cc-bus", alias: "look", n: 1, tabGroup: "看图" },
      ],
    });
    const rows = [...p.el.querySelectorAll<HTMLElement>('[data-role="set-row"]')];
    expect(rows).toHaveLength(2);
    expect(rows[0].querySelector<HTMLInputElement>('[data-role="set-group"]')!.placeholder).toBe("dev");
    expect(rows[1].querySelector<HTMLInputElement>('[data-role="set-group"]')!.value).toBe("看图");
    const preview = p.el.querySelector<HTMLElement>('[data-role="set-preview"]')!.textContent ?? "";
    expect(preview).toContain(copyText("newSessionSet.preview.have", { n: "2" }));
    expect(preview).toContain(copyText("setMenu.item.more", { n: "1" }));
    expect(preview).toContain(copyText("newSessionSet.preview.total", { n: "1" }));
    expect(p.action()).toBe(copyText("newSessionSet.action.start", { n: "1" }));
    expect(p.blocked()).toBeNull();
  });

  it("都齐了：主按钮字换「已齐」、点不了；起不了的那一行右侧写原因", async () => {
    h.planSet.mockResolvedValue(plan(0));
    const { p } = await mount("工具链");
    expect(p.action()).toBe(copyText("setMenu.item.full"));
    expect(p.blocked()).toBe(copyText("setMenu.item.full"));
    expect(previewSaid({ cwd: "/x", alias: "rev", want: 1, have: 0, more: 0, group: "rev", blocked: "rev 不存在" })).toBe("rev 不存在");
  });

  it("改一格：停手 300ms 才再问一次预览（只问一次）；加一行带上一行的目录与别名", async () => {
    h.planSet.mockResolvedValue(plan(1));
    const { p } = await mount("工具链");
    h.planSet.mockClear();
    p.el.querySelector<HTMLButtonElement>('[data-role="set-add"]')!.click();
    const more = p.el.querySelectorAll<HTMLButtonElement>('[data-role="set-more"]')[0];
    more.click();
    await vi.advanceTimersByTimeAsync(299);
    expect(h.planSet).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(h.planSet).toHaveBeenCalledTimes(1);
    expect(h.planSet.mock.calls[0][1].rows).toEqual([
      { cwd: "~/work/cc-monitor", alias: "dev", n: 3, tabGroup: null },
      { cwd: "~/work/cc-bus", alias: "look", n: 1, tabGroup: "看图" },
      { cwd: "~/work/cc-bus", alias: "look", n: 1, tabGroup: null },
    ]);
  });

  it("启动 ＝ 先存（saveSet，带打开时的名字与读回的版本）后起；存不成 ⇒ 那一句、不起", async () => {
    h.planSet.mockResolvedValue(plan(1));
    const { p } = await mount("工具链");
    expect(await p.submit()).toBeNull();
    expect(h.writeProfiles).toHaveBeenCalledWith(
      "devbox",
      [
        {
          op: "saveSet",
          was: "工具链",
          set: {
            name: "工具链",
            rows: [
              { cwd: "~/work/cc-monitor", alias: "dev", n: 2, tabGroup: null },
              { cwd: "~/work/cc-bus", alias: "look", n: 1, tabGroup: "看图" },
            ],
          },
        },
      ],
      "fp-1",
    );
    expect(h.startSet).toHaveBeenCalledWith("devbox", "工具链");
    expect(h.writeProfiles.mock.invocationCallOrder[0]).toBeLessThan(h.startSet.mock.invocationCallOrder[0]);
    h.startSet.mockClear();
    h.writeProfiles.mockRejectedValue(new Error("合计上限 12"));
    const bad = await p.submit();
    expect(bad?.said).toBe("合计上限 12");
    expect(h.startSet).not.toHaveBeenCalled();
  });

  it("新建一组：一行空的，组名空着点不了；别名只列这台能用的", async () => {
    h.planSet.mockResolvedValue(plan(1));
    const { p } = await mount(null);
    expect(p.el.querySelectorAll('[data-role="set-row"]')).toHaveLength(1);
    expect(p.blocked()).toBe(copyText("newSessionSet.name.empty"));
    expect(h.planSet).not.toHaveBeenCalled();
  });
});
