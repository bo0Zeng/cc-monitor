/**
 * 假后端的 `rotation-plan`（时间轴那一问：带 `view`）：照稿截图 08 · 09 的样子造一份回答 —— 刻度 · 本会话走过的 · 将来的 ·
 * 泳道（被拒 · 过封顶 · 时段停用 · 重置 · 开窗）· 顶行。时刻按浏览器本地钟写字（假后端就在这台）。
 */

const H = 3600;

const hm = (t: number): string => {
  const d = new Date(t * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
};
const md = (t: number): string => {
  const d = new Date(t * 1000);
  return `${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

type View = "6h" | "24h" | "7d";

export interface FakePlanOpts {
  view: View;
  /** 本会话那一问（有 past · head）；否则设置那一问（全部号 · usedBy）。 */
  session: boolean;
  /** 卡住：池里都被拒。 */
  blocked?: boolean;
  /** quota-warm 在跑（泳道画 ○）。 */
  warm?: boolean;
  /** team 是兜底：过去 work 被拒时先等 personal（不用兜底 team）；将来 personal 到 90% 用 team，personal 重置就换下兜底。 */
  fallback?: boolean;
}

export function fakePlan(o: FakePlanOpts): Record<string, unknown> {
  const t = Math.floor(Date.now() / 1000);
  const [before, after, step, major] =
    o.view === "6h"
      ? [2 * H, 4 * H, 900, H]
      : o.view === "24h"
        ? [6 * H, 18 * H, H, 3 * H]
        : [24 * H, 144 * H, 6 * H, 24 * H];
  const from = t - before;
  const until = t + after;
  const off = -new Date().getTimezoneOffset() * 60;
  const grid: Record<string, unknown>[] = [];
  for (let g = Math.ceil((from + off) / step) * step - off; g <= until; g += step) {
    const cell: Record<string, unknown> = { at: g, atText: hm(g) };
    if ((g + off) % major === 0) cell.label = major === 24 * H ? md(g) : hm(g);
    grid.push(cell);
  }
  const span = (a: number, b: number) => ({
    from: t + a,
    fromText: hm(t + a),
    to: t + b,
    toText: hm(t + b),
  });
  const planEnd = o.view === "7d" ? 24 * H : after;
  const cut = (x: number): number => Math.min(x, planEnd);
  // 走过的：work 满了 ⇒ 换到 personal，之后一直在 personal。
  const sw = Math.max(-before + 0.5 * H, -4.7 * H);
  const pastSegs = o.fallback
    ? [
        { ...span(-before, sw - 0.3 * H), account: "work", why: null },
        { ...span(sw - 0.3 * H, sw), account: null, why: { wait: { account: "personal", instead: "team" } } },
        { ...span(sw, 0), account: "personal", why: { full: { w: "five_hour" } } },
      ]
    : [
        { ...span(-before, sw), account: "work", why: null },
        { ...span(sw, 0), account: "personal", why: { full: { w: "five_hour" } } },
      ];
  const plan = o.fallback
    ? [
        { ...span(0, 1.6 * H), account: "personal", why: null },
        { ...span(1.6 * H, 3 * H), account: "team", why: { threshold: { n: 90, w: "5h" } } },
        { ...span(3 * H, cut(after)), account: "personal", why: "leaveFallback" },
      ].filter((x) => x.to > x.from)
    : o.blocked
    ? [
        { ...span(0, 0.63 * H), account: null, why: { held: { n: 90, w: "5h" } } },
        { ...span(0.63 * H, cut(after)), account: "team", why: "preempt" },
      ]
    : [
        { ...span(0, 1.6 * H), account: "personal", why: null },
        { ...span(1.6 * H, cut(6 * H)), account: "team", why: { threshold: { n: 90, w: "5h" } } },
        { ...span(cut(6 * H), cut(after)), account: "work", why: "preempt" },
      ].filter((x) => x.to > x.from);
  const lanes = [
    {
      account: "personal",
      pct: o.blocked ? 100 : 63,
      spans: o.blocked
        ? [{ ...span(0, 1.8 * H), state: "refused", n: null }]
        : [{ ...span(1.6 * H, 2.2 * H), state: "capped", n: 90, w: "5h" }],
      resets: [{ w: "5h", at: t + 2.2 * H, atText: hm(t + 2.2 * H) }],
      ...(o.session ? {} : { usedBy: 2 }),
    },
    {
      account: "work",
      pct: 100,
      spans: [
        { ...span(0, 2 * H), state: "refused", n: null },
        { ...span(2 * H, 5.5 * H), state: "off", n: null },
      ],
      resets: [
        { w: "5h", at: t + 2 * H, atText: hm(t + 2 * H) },
        { w: "7d", at: t + 15 * H, atText: hm(t + 15 * H) },
      ],
      ...(o.session ? {} : { usedBy: 1 }),
    },
    {
      account: "team",
      pct: 22,
      // 此刻 7d 到线（列表行尾「7d 到线」那一枚签照它）；1.2h 后 7d 那一窗的线放宽（时段）、又能接。
      spans: o.blocked
        ? [{ ...span(0, 0.63 * H), state: "refused", n: null }]
        : [{ ...span(0, 1.2 * H), state: "capped", n: 90, w: "7d" }],
      resets: [{ w: "5h", at: t + 4 * H, atText: hm(t + 4 * H) }],
      ...(o.session ? {} : { usedBy: 1 }),
      ...(o.warm ? { warm: [{ at: t + 2.5 * H, atText: hm(t + 2.5 * H) }] } : {}),
    },
  ];
  if (!o.session)
    lanes.push(
      { account: "lab", pct: 4, spans: [], resets: [], usedBy: 0 } as never,
      { account: "api", pct: null, spans: [], resets: [], usedBy: 0 } as never,
    );
  const out: Record<string, unknown> = {
    errors: [],
    now: t,
    nowText: hm(t),
    from,
    fromText: hm(from),
    until,
    plan,
    lanes,
    effective: {},
    grid,
  };
  if (o.session) {
    out.past = pastSegs;
    out.head = o.blocked
      ? { blocked: { account: "team", at: t + 0.63 * H, atText: hm(t + 0.63 * H), w: "5h" } }
      : { account: "personal", w: "5h", pct: 63, toLine: { w: "5h", n: 27 }, est: { at: t + 1.6 * H, atText: hm(t + 1.6 * H), pct: 90, w: "5h" } };
  }
  return out;
}
