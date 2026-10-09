/**
 * 轮换时间轴（面板 · 设置 · 编辑器预览共用）的判据：顶行照后端 head 写 · 本会话轨过去 / 将来两样、换号点在段上面那一层 ·
 * 每段读屏名写出号名（同色也读得出）· 泳道底纹 / 重置 / 开窗 / 在用 N 会话 · 7d 将来画到头之后写 — · 键盘按格走、卡念那一格的一句。
 */
import { describe, it, expect, beforeEach } from "vitest";
import type { PlanRead } from "../../../src/frontend/ui/quota-reads";
import {
  cellSay,
  headLine,
  timelineAxis,
} from "../../../src/frontend/ui/rot-timeline";
import { copyText } from "../../../src/frontend/ui/copy-table";

const T = 100_000;
const H = 3600;
const seg = (a: number, b: number, account: string | null, why: PlanRead["plan"][number]["why"]) => ({
  from: T + a,
  fromText: `t${a / H}`,
  to: T + b,
  toText: `t${b / H}`,
  account,
  why,
});

function plan(p: Partial<PlanRead> = {}): PlanRead {
  return {
    errors: [],
    now: T,
    nowText: "02:00",
    from: T - 6 * H,
    fromText: "20:00",
    until: T + 18 * H,
    past: [seg(-6 * H, -2 * H, "work", null), seg(-2 * H, 0, "personal", { full: { w: "five_hour" } })],
    plan: [seg(0, 2 * H, "team", null), seg(2 * H, 18 * H, "lab", { threshold: { n: 90 } })],
    lanes: [
      {
        account: "team",
        pct: 22,
        usedBy: 2,
        spans: [{ from: T + 2 * H, fromText: "04:00", to: T + 3 * H, toText: "05:00", state: "capped", n: 90 }],
        resets: [
          { w: "5h", at: T + 3 * H, atText: "05:00" },
          { w: "7d", at: T + 9 * H, atText: "11:00" },
        ],
      },
      { account: "lab", pct: null, spans: [], resets: [], usedBy: 0 },
    ],
    effective: {},
    grid: Array.from({ length: 24 }, (_, i) => ({
      at: T - 6 * H + i * H,
      atText: `g${i}`,
      ...(i % 3 === 0 ? { label: `L${i}` } : {}),
    })),
    head: { account: "team", w: "5h", pct: 63, toTrigger: 27 },
    ...p,
  };
}

beforeEach(() => document.body.replaceChildren());

describe("时间轴 · 顶行", () => {
  it("平时 ＝ 在用 号 窗口 用量 · 距触发 N 点；卡住 ＝ 最早回来的号 ↻几点 (+多久) · 哪个窗口重置", () => {
    const parts = [copyText("rot.tl.now", { acct: "team", w: "5h", pct: 63 }), copyText("rot.tl.toTrig", { n: 27 })];
    expect(headLine(plan())).toEqual({ text: parts.join(copyText("kit.text.sep")), parts, blocked: false });
    const b = headLine(plan({ head: { blocked: { account: "team", at: T + 38 * 60, atText: "02:38", w: "5h" } } }))!;
    expect(b.blocked).toBe(true);
    expect(b.text).toBe(
      [
        copyText("rot.tl.blocked", { acct: "team", at: "02:38", rel: "+38m" }),
        copyText("rot.tl.blockedW", { w: "5h" }),
      ].join(copyText("kit.text.sep")),
    );
    expect(headLine(plan({ head: undefined }))).toBeNull();
  });

  it("后端给了估 ⇒ 顶行末尾接「估 几点 到 N%」；没给就不写", () => {
    const est = { at: T + 40 * 60, atText: "02:40", pct: 90, w: "5h" };
    expect(headLine(plan({ head: { account: "team", w: "5h", pct: 63, toTrigger: 27, est } }))!.text).toBe(
      [
        copyText("rot.tl.now", { acct: "team", w: "5h", pct: 63 }),
        copyText("rot.tl.toTrig", { n: 27 }),
        copyText("rot.tl.est", { at: "02:40", pct: 90 }),
      ].join(copyText("kit.text.sep")),
    );
    expect(headLine(plan())!.text).not.toContain(copyText("rot.tl.est", { at: "02:40", pct: 90 }));
  });
});

describe("时间轴 · 本会话轨", () => {
  it("段 ＝ 走过的（实）＋ 将来的（虚）按时间排；换号点在段上面那一层（不在段里），摆在那一段的起点；视窗起点那一段不画换号点", () => {
    const ax = timelineAxis(plan(), { track: copyText("rot.tl.track") });
    const segs = [...ax.querySelectorAll<HTMLElement>("[data-tl-seg]")];
    expect(segs.map((x) => [x.dataset.tlSeg, x.dataset.when])).toEqual([
      ["work", "past"],
      ["personal", "past"],
      ["team", "future"],
      ["lab", "future"],
    ]);
    const whys = [...ax.querySelectorAll<HTMLElement>("[data-tl-why]")];
    expect(whys.map((w) => w.textContent)).toEqual([copyText("rot.why.full"), copyText("rot.why.trig", { n: 90 })]);
    expect(whys.every((w) => w.closest("[data-tl-seg]") === null)).toBe(true);
    expect(whys.map((w) => w.style.left)).toEqual([segs[1].style.left, segs[3].style.left]);
  });

  it("每段段里写号名、读屏名 ＝ `起止 · 号 · 为什么换`（换进来的原因照换号记录的说法）", () => {
    const ax = timelineAxis(plan(), { track: copyText("rot.tl.track") });
    const segs = [...ax.querySelectorAll<HTMLElement>("[data-tl-seg]")];
    expect(segs.map((x) => x.querySelector("[data-tl-seg-name]")?.textContent)).toEqual(["work", "personal", "team", "lab"]);
    expect(segs[3].getAttribute("aria-label")).toBe(
      copyText("rot.pv.segWhy", {
        from: "t2",
        to: "t18",
        acct: "lab",
        why: copyText("acct.hist.pct", { name: "team", n: 90 }),
      }),
    );
  });

  it("将来画到头（7d 只到 +1d）⇒ 其后写 —；不给轨名（设置里）⇒ 没有那一轨", () => {
    const short = plan({ plan: [seg(0, 2 * H, "team", null)] });
    expect(timelineAxis(short, { track: "x" }).querySelector("[data-tl-no-plan]")).not.toBeNull();
    expect(timelineAxis(plan(), { track: "x" }).querySelector("[data-tl-no-plan]")).toBeNull();
    expect(timelineAxis(plan(), { track: null }).querySelector('[data-tl-row="track"]')).toBeNull();
  });
});

describe("时间轴 · 泳道", () => {
  it("行头 头像 · 名 · 此刻用量；底纹照后端 state；5h 竖线 · 7d 菱形；「在用 N 会话」只在设置里、≥2 琥珀", () => {
    const ax = timelineAxis(plan(), { track: null, usedBy: true });
    const team = ax.querySelector<HTMLElement>('[data-tl-row="team"]')!;
    expect(team.textContent).toContain("22%");
    expect(team.querySelector<HTMLElement>("[data-state]")!.dataset.state).toBe("capped");
    expect([...team.querySelectorAll<HTMLElement>("[data-tl-reset]")].map((m) => m.dataset.tlReset)).toEqual(["5h", "7d"]);
    const chip = team.querySelector<HTMLElement>("[data-tl-used-by]")!;
    expect(chip.textContent).toBe(copyText("rot.tl.usedBy", { n: 2 }));
    expect(chip.dataset.warn).toBe("true");
    expect(ax.querySelector('[data-tl-row="lab"] [data-tl-used-by]'), "0 个不写").toBeNull();
    expect(timelineAxis(plan(), { track: null }).querySelector("[data-tl-used-by]"), "面板里不写").toBeNull();
  });

  it("开窗 ○ 只在后端给了 warm 时画，图例也只在那时列「开窗」", () => {
    const none = timelineAxis(plan(), { track: null });
    expect(none.querySelector("[data-tl-warm]")).toBeNull();
    expect(none.querySelector('[data-tl-lg="warm"]')).toBeNull();
    const p = plan();
    p.lanes[1].warm = [{ at: T + H, atText: "03:00" }];
    const w = timelineAxis(p, { track: null });
    expect(w.querySelectorAll("[data-tl-warm]").length).toBe(1);
    expect(w.querySelector('[data-tl-lg="warm"]')).not.toBeNull();
  });
});

describe("时间轴 · 悬停与键盘", () => {
  it("Tab 进轴 ⇒ 卡停在「现在」那一格；→ 走一格；Home / End 到头；Esc 收起；卡念的 ＝ 那一格的一句（几点 · 用谁 · 谁不能用到几点）", () => {
    const p = plan();
    const ax = timelineAxis(p, { track: copyText("rot.tl.track") });
    document.body.appendChild(ax);
    const card = ax.querySelector<HTMLElement>("[data-tl-card]")!;
    expect(card.dataset.shown).toBeUndefined();
    ax.focus();
    expect(card.dataset.shown).toBe("true");
    expect(card.textContent).toBe(cellSay(p, [...p.past!, ...p.plan], T, "g6"));
    const key = (k: string) => ax.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));
    key("ArrowRight");
    key("ArrowRight");
    expect(card.textContent).toBe(cellSay(p, [...p.past!, ...p.plan], T + 2 * H, "g8"));
    expect(card.textContent).toBe(
      [
        "g8",
        copyText("rot.tlHover.use", { acct: "lab" }),
        copyText("rot.tlSt.until", { what: copyText("rot.tlSt.capped", { acct: "team", n: 90 }), at: "05:00" }),
      ].join(copyText("kit.text.sep")),
    );
    key("End");
    expect(card.textContent?.startsWith("g23")).toBe(true);
    key("Home");
    expect(card.textContent?.startsWith("g0")).toBe(true);
    key("Escape");
    expect(card.dataset.shown).toBeUndefined();
  });
});
