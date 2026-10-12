/**
 * 合图：一条轴，最上一行本会话、下面每号一行（行序照给的那一份 ＝ 有效优先级）。
 * 只照 `rotation-plan` 的回答画：段 · 换号点（◆ 已换 · ◇ 估）· 每号的段（`k` 定样子）· 重置刻度；界面不推演、不判到线。
 */
import { describe, it, expect } from "vitest";
import { combinedChart, type ChartRow } from "../../../src/frontend/ui/rot-chart";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { PlanReply } from "../../../src/frontend/ui/generated/PlanReply";

const T = 1_760_187_780;
const M = 60;
const plan = (p: Partial<PlanReply> = {}): PlanReply => ({
  now: T,
  nowText: "21:03",
  from: T - 120 * M,
  until: T + 600 * M,
  view: "12h",
  plan: [
    { from: T - 120 * M, fromText: "19:03", to: T - 83 * M, toText: "19:40", account: "work" },
    { from: T - 83 * M, fromText: "19:40", to: T, toText: "21:03", account: "team", why: "W-past" },
    { from: T, fromText: "21:03", to: T + 91 * M, toText: "22:34", account: "team", est: true },
    { from: T + 91 * M, fromText: "22:34", to: T + 600 * M, toText: "07:03", account: "lab", why: "W-lab", est: true },
  ],
  rows: [
    { account: "team", bars: [{ from: T, to: T + 91 * M, k: "use", est: true }], resets: [{ w: "5h", at: T + 127 * M, atText: "23:10" }] },
    { account: "work", bars: [{ from: T, to: T + 97 * M, k: "line", est: true }], resets: [{ w: "5h", at: T + 97 * M, atText: "22:40" }, { w: "7d", at: T + 500 * M, atText: "10-14" }] },
  ],
  grid: [
    { at: T - 60 * M, atText: "20:00", label: "20:00" },
    { at: T + 57 * M, atText: "22:00" },
    { at: T + 177 * M, atText: "00:00", label: "00:00" },
  ],
  ...p,
});

const rows: ChartRow[] = [
  { account: "team", head: document.createElement("span"), why: "W-team", inuse: true, alsoUsed: 1 },
  { account: "work", head: document.createElement("span"), why: "W-work", skip: true, alsoUsed: 0 },
];

function draw(p: PlanReply | null = plan(), extra: Partial<Parameters<typeof combinedChart>[0]> = {}) {
  const views: string[] = [];
  const c = combinedChart({ plan: p, view: "12h", onView: (v) => views.push(v), session: { title: copyText("rot.chart.session"), sub: "orders" }, rows, origin: "<local>", ...extra });
  document.body.replaceChildren(c.root);
  return { ...c, views };
}

describe("rot-chart · 合图", () => {
  it("轴：此刻那一格红字写 nowText；刻度只写带 label 的", () => {
    const { root } = draw();
    const axis = root.querySelector<HTMLElement>("[data-rc-axis]")!;
    expect(axis.querySelector("[data-rc-now]")?.textContent).toBe("21:03");
    expect([...axis.querySelectorAll("[data-rc-tick]")].map((e) => e.textContent)).toEqual(["20:00", "", "00:00"]);
  });

  it("12h / 24h：点 24h 回调", () => {
    const { root, views } = draw();
    const b = [...root.querySelectorAll<HTMLButtonElement>("[data-rc-view] button")].find((x) => x.textContent === "24h")!;
    b.click();
    expect(views).toEqual(["24h"]);
  });

  it("本会话那行：段照 plan（过去实、将来估）；换号点 ◆ 已换 · ◇ 估，悬停写时刻 · 换到谁 · 为什么", () => {
    const { root } = draw();
    const sess = root.querySelector<HTMLElement>("[data-rc-session]")!;
    const segs = [...sess.querySelectorAll<HTMLElement>("[data-rc-seg]")];
    expect(segs.map((s) => [s.dataset.rcSeg, s.dataset.when])).toEqual([
      ["work", "past"],
      ["team", "past"],
      ["team", "future"],
      ["lab", "future"],
    ]);
    const dms = [...sess.querySelectorAll<HTMLElement>("[data-rc-switch]")];
    expect(dms.map((d) => d.dataset.est ?? "no")).toEqual(["no", "true"]);
    expect(dms[1].getAttribute("aria-label")).toBe(`${copyText("rot.chart.switchEst", { at: "22:34", acct: "lab" })}${copyText("kit.text.sep")}W-lab`);
    expect(dms[0].getAttribute("aria-label")).toBe(`${copyText("rot.chart.switchAt", { at: "19:40", acct: "team" })}${copyText("kit.text.sep")}W-past`);
  });

  it("停发那一段：空着画虚框（account 为 null）", () => {
    const p = plan({ plan: [{ from: T, fromText: "21:03", to: T + 97 * M, toText: "22:40", account: null, est: true }, { from: T + 97 * M, fromText: "22:40", to: T + 600 * M, toText: "07:03", account: "work", est: true }] });
    const { root } = draw(p);
    const held = root.querySelector<HTMLElement>("[data-rc-session] [data-rc-held]")!;
    expect(held).not.toBeNull();
    expect(held.getAttribute("aria-label")).toBe(copyText("rot.chart.held", { from: "21:03", to: "22:40" }));
  });

  it("每号一行：行序照给的那一份；段的样子照 k；重置刻度写 ↻ 时刻；7d 的带窗名", () => {
    const { root, rowEls } = draw();
    const rs = [...root.querySelectorAll<HTMLElement>("[data-rc-row]")];
    expect(rs.map((r) => r.dataset.rcRow)).toEqual(["team", "work"]);
    expect(rowEls.get("team")).toBe(rs[0]);
    expect(rs[0].querySelector<HTMLElement>("[data-rc-bar]")?.dataset.rcBar).toBe("use");
    expect(rs[1].querySelector<HTMLElement>("[data-rc-bar]")?.dataset.rcBar).toBe("line");
    expect([...rs[1].querySelectorAll("[data-rc-reset]")].map((e) => e.textContent)).toEqual([
      copyText("rot.chart.reset", { at: "22:40" }),
      copyText("rot.chart.reset7d", { at: "10-14" }),
    ]);
  });

  it("后端写好的四样照抄：余 N（在用的段上）· 时段带（≤80% · 不用）· 在用号估的到线段 · 视窗外的重置", () => {
    const p = plan({
      rows: [
        { account: "team", bars: [{ from: T, to: T + 91 * M, k: "use", est: true }], resets: [{ w: "5h", at: T + 127 * M, atText: "23:10" }], left: 41, leftText: "L-41", est: { at: T + 91 * M, atText: "22:34", w: "5h" } },
        { account: "work", bars: [], resets: [], slots: [{ from: T - 60 * M, to: T + 200 * M, line: 80, text: "S-80" }, { from: T + 300 * M, to: T + 400 * M, line: 0, text: "S-off" }], farResets: [{ w: "7d", at: T + 9000 * M, atText: "10-14", text: "F-7d" }] },
      ],
    });
    const { root } = draw(p);
    const [team, work] = [...root.querySelectorAll<HTMLElement>("[data-rc-row]")];
    expect(team.querySelector("[data-rc-left]")?.textContent).toBe("L-41");
    expect(team.querySelector("[data-rc-left]")?.closest("[data-rc-bar]"), "余 N 写在在用那一段里").not.toBeNull();
    const est = team.querySelector<HTMLElement>("[data-rc-est]")!;
    expect(est.textContent).toBe(copyText("rot.chart.est", { at: "22:34" }));
    expect(est.style.left, "估的到线段从估的那一刻起").not.toBe("");
    const slots = [...work.querySelectorAll<HTMLElement>("[data-rc-slot]")];
    expect(slots.map((x) => [x.textContent, x.dataset.unused ?? ""])).toEqual([
      ["S-80", ""],
      ["S-off", "true"],
    ]);
    expect(work.querySelector("[data-rc-far]")?.textContent).toBe("F-7d");
    expect(team.querySelector("[data-rc-far]")).toBeNull();
  });

  it("★ 余 N 要么整句看得见、要么挪到段的右端外面（不被段截掉）；段上悬停也写这一句", async () => {
    const p = plan({ rows: [{ account: "team", bars: [{ from: T, to: T + 3 * M, k: "use" }], resets: [], left: 41, leftText: "L-41" }] });
    const fits = (sw: number, cw: number) => {
      Object.defineProperty(HTMLElement.prototype, "scrollWidth", { configurable: true, get: () => sw });
      Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => cw });
    };
    const frame = () => new Promise((r) => requestAnimationFrame(() => r(null)));
    try {
      fits(80, 20);
      let label = draw(p).root.querySelector<HTMLElement>("[data-rc-left]")!;
      await frame();
      expect(label.dataset.place, "段太窄 ⇒ 挪到右端外面").toBe("after");
      expect(label.closest<HTMLElement>("[data-rc-bar]")?.dataset.labelOut).toBe("true");
      expect(label.closest<HTMLElement>("[data-rc-bar]")?.getAttribute("aria-label")).toBe("L-41");
      fits(20, 80);
      label = draw(p).root.querySelector<HTMLElement>("[data-rc-left]")!;
      await frame();
      expect(label.dataset.place, "放得下 ⇒ 留在段里").toBeUndefined();
    } finally {
      delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollWidth;
      delete (HTMLElement.prototype as unknown as Record<string, unknown>).clientWidth;
    }
  });

  it("行下一句：策略给的为什么；此刻不挑 ⇒ 跳过 · 为什么、整行淡；别的会话也钉着 ⇒ 琥珀一句；在用那行带标记", () => {
    const { root } = draw();
    const [team, work] = [...root.querySelectorAll<HTMLElement>("[data-rc-row]")];
    expect(team.dataset.inuse).toBe("true");
    expect(team.querySelector("[data-rc-why]")?.textContent).toBe(`W-team${copyText("kit.text.sep")}${copyText("rot.row.alsoUsed", { n: 1 })}`);
    expect(work.dataset.skip).toBe("true");
    expect(work.querySelector("[data-rc-why]")?.textContent).toBe(copyText("rot.row.skip", { why: "W-work" }));
  });

  it("推演还没到 ⇒ 行照画（没有段）；图例只出一份", () => {
    const { root } = draw(null);
    expect(root.querySelectorAll("[data-rc-row]").length).toBe(2);
    expect(root.querySelectorAll("[data-rc-bar]").length).toBe(0);
    expect(root.querySelectorAll("[data-rc-legend]").length).toBe(1);
  });

  it("加号那一行：给了才出，点了回调", () => {
    let n = 0;
    const { root } = draw(plan(), { onAdd: () => n++ });
    root.querySelector<HTMLButtonElement>("[data-rc-add]")!.click();
    expect(n).toBe(1);
    expect(draw(plan()).root.querySelector("[data-rc-add]")).toBeNull();
  });
});
