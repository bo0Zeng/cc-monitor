// 新建框「一组」页：行照存好的那一组铺开；预览停手 300ms 现问 set-plan {rows}、照成品画；主按钮的字 · 能不能点照预览；启动 ＝ 先存（saveSet）后起。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const h = vi.hoisted(() => ({
  readProfiles: vi.fn(),
  writeProfiles: vi.fn(),
  planSet: vi.fn(),
  startSet: vi.fn(),
  toast: vi.fn(),
  undoToast: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/profiles-reads", () => ({ readProfiles: h.readProfiles, writeProfiles: h.writeProfiles }));
vi.mock("../../../src/frontend/ui/set-reads", () => ({ planSet: h.planSet }));
vi.mock("../../../src/frontend/ui/set-start", () => ({ startSet: h.startSet }));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: h.toast, undoToast: h.undoToast }));

import { copyText } from "../../../src/frontend/ui/copy-table";
import { setPage } from "../../../src/frontend/ui/new-session-set";
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

/** 那台写好的几句用占位字（界面照摆、不拼）；都够了 ⇒ 主按钮那一句是「已齐」那一形。 */
const plan = (more: number): SetPlan => ({
  rows: [
    { cwd: "~/work/cc-monitor", alias: "dev", want: 2, have: 2, more: 0, group: "dev", blocked: null, said: "R1", haveSaid: "HAVE-2", moreSaid: "DASH", canMore: true, moreWhy: null },
    { cwd: "~/work/cc-bus", alias: "look", want: 1, have: 0, more, group: "看图", blocked: more ? null : "look 不存在", said: "R2", haveSaid: "HAVE-0", moreSaid: more ? "MORE-1" : "look 不存在", canMore: true, moreWhy: null },
  ],
  more,
  short: "SHORT",
  moreSaid: more ? "MORE" : "FULL",
  totalSaid: `TOTAL-${more}`,
  startSaid: more ? "START-1" : "FULL",
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
    for (const w of ["HAVE-2", "DASH", "HAVE-0", "MORE-1", "TOTAL-1"]) expect(preview).toContain(w);
    expect(p.action()).toBe("START-1");
    expect(p.blocked()).toBeNull();
  });

  it("都齐了：主按钮照那台那一句、点不了；起不了的那一行右侧写原因", async () => {
    h.planSet.mockResolvedValue(plan(0));
    const { p } = await mount("工具链");
    expect(p.action()).toBe("FULL");
    expect(p.blocked()).toBe("FULL");
    const lines = [...p.el.querySelectorAll<HTMLElement>('[data-role="set-preview"] > *')];
    expect(lines[1].dataset.intent).toBe("error");
    expect(lines[1].textContent).toContain("look 不存在");
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

  it("★「＋」照那台的 canMore 灰、悬停是 moreWhy；重行（blocked）淡掉、下面写那台那一句", async () => {
    const p0 = plan(1);
    p0.rows[0] = { ...p0.rows[0], canMore: false, moreWhy: "MAX-9" };
    p0.rows[1] = { ...p0.rows[1], blocked: "DUP-1", more: 0 };
    h.planSet.mockResolvedValue(p0);
    const { p } = await mount("工具链");
    const rows = [...p.el.querySelectorAll<HTMLElement>('[data-role="set-row"]')];
    const plus = rows[0].querySelector<HTMLButtonElement>('[data-role="set-more"]')!;
    expect(plus.getAttribute("aria-disabled") ?? String(plus.disabled), "到了上限「＋」灰").toMatch(/true/);
    expect(plus.title).toBe("MAX-9");
    expect(rows[1].querySelector<HTMLButtonElement>('[data-role="set-more"]')!.title, "能加的不挂那一句").not.toBe("MAX-9");
    expect(rows[1].dataset.blocked).toBe("true");
    expect(p.el.querySelector<HTMLElement>('[data-role="set-row-why"]')?.textContent).toBe("DUP-1");
  });

  it("★ 删组：组名格旁［删除］直接删（removeSet）、页换新建、toast［撤销］；撤销 ＝ saveSet 照读到的那份写回；那台拒 ⇒ toast 换成那一句", async () => {
    h.planSet.mockResolvedValue(plan(1));
    const { p } = await mount("工具链");
    const del = p.el.querySelector<HTMLButtonElement>('[data-role="set-delete"]')!;
    expect(del, "打开的是存好的一组才有［删除］").not.toBeNull();
    del.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(h.writeProfiles).toHaveBeenLastCalledWith("devbox", [{ op: "removeSet", name: "工具链" }], "fp-1");
    expect(p.el.querySelector<HTMLInputElement>('[data-role="set-name"]')!.value, "页换成新建").toBe("");
    expect(del.style.display, "新建那一页没有［删除］").toBe("none");
    expect(h.undoToast).toHaveBeenCalledWith(copyText("newSessionSet.delete.done"), expect.any(Function), expect.any(Function));
    h.writeProfiles.mockResolvedValueOnce({ wrote: true, rev: "fp-3", reload: null });
    h.undoToast.mock.calls[0][1]();
    await vi.advanceTimersByTimeAsync(0);
    expect(h.writeProfiles).toHaveBeenLastCalledWith(
      "devbox",
      [
        {
          op: "saveSet",
          was: null,
          set: {
            name: "工具链",
            rows: [
              { cwd: "~/work/cc-monitor", alias: "dev", n: 2, tabGroup: null },
              { cwd: "~/work/cc-bus", alias: "look", n: 1, tabGroup: "看图" },
            ],
          },
        },
      ],
      "fp-2",
    );
    h.writeProfiles.mockRejectedValueOnce(new Error("别处改过（夹具）"));
    h.undoToast.mock.calls[0][1]();
    await vi.advanceTimersByTimeAsync(0);
    expect(h.toast).toHaveBeenLastCalledWith(copyText("newSessionSet.delete.undoFailed"), "别处改过（夹具）");
  });

  it("新建一组：一行空的，组名空着点不了；别名只列这台能用的", async () => {
    h.planSet.mockResolvedValue(plan(1));
    const { p } = await mount(null);
    expect(p.el.querySelectorAll('[data-role="set-row"]')).toHaveLength(1);
    expect(p.blocked()).toBe(copyText("newSessionSet.name.empty"));
    expect(h.planSet).not.toHaveBeenCalled();
  });
});
