/**
 * 额度与账号：状态栏账号按钮各态 ＋ 悬停卡 · 「账号」面板（读不出 · 未实时显示 · 底栏）· 右键「账号…」。
 * 轮换那几块（会话面板里的轮换 · 设置窗「轮换」· 换号条 · 停发提示）第 4 刀照 `轮换-v5` 的稿施工，场景随它一起加。
 *
 * 账号库 · 额度账 · 轮换账本 · 血缘落成本机家目录里的原始文件（`acctDisk`），额度 · 在用的号的成品全是真后端读它们算的。
 * 号名是编的。时刻按此刻的钟现算。
 */
import type { Scene } from "./index";
import { putAccounts, putBrokenQuota, putLineage, putQuota, putRotation, putWarm, type QuotaSpec, type RotationSessionSpec } from "../disk";
import type { World } from "../fake/types";
import { defaultWorld, LOCAL } from "../fake/world";
import {
  click,
  mainReady,
  openTab,
  rightClick,
  sleep,
  waitFor,
} from "./helpers";

const W = 1280;
const H = 800;
const ALL_TABS = 7;

type Slot = { slot: string; pct?: number; resetsAt?: number };
interface Acct {
  account: string;
  kind: "sub" | "api";
  state: string;
  stale?: boolean;
  limiting?: string;
  slots: Slot[];
  login?: string;
  seenAgo?: number;
  readingReset?: number;
  unseen?: boolean;
}
interface Sess {
  start: string;
  current: string;
  absent?: boolean;
  /** 会话血缘里的父（tab 序号）。 */
  parent?: number;
}
interface AcctWorld {
  accounts: Acct[];
  /** 不写轮换账本（中转还没见过回包）。 */
  noBook?: boolean;
  /** 按 tab 序号（0 起）。 */
  sessions: Record<number, Sess>;
  /** 时间轴：卡住（池里都被拒）· quota-warm 在跑。 */
  /** quota-warm 在跑：号 → 下一次开窗（距此刻多少秒）。 */
  warm?: { account: string; at: number }[];
}

const now = (): number => Math.floor(Date.now() / 1000);
const H1 = 3600;

/** 稿里那一套号：work 起始 · personal 在用 · team · api（按量）。`at` 里的数是「距此刻多少秒」。 */
function baseAccounts(): Acct[] {
  return [
    {
      account: "work",
      kind: "sub",
      state: "refused",
      limiting: "5h",
      slots: [
        { slot: "5h", pct: 100, resetsAt: 2.3 * H1 },
        { slot: "7d", pct: 78, resetsAt: 40 * H1 },
      ],
      readingReset: 2.3 * H1,
    },
    {
      account: "personal",
      kind: "sub",
      state: "ok",
      limiting: "5h",
      slots: [
        { slot: "5h", pct: 63, resetsAt: 1.83 * H1 },
        { slot: "7d", pct: 41, resetsAt: 88 * H1 },
      ],
    },
    {
      account: "team",
      kind: "sub",
      state: "ok",
      limiting: "5h",
      slots: [
        { slot: "5h", pct: 4, resetsAt: 1.5 * H1 },
        { slot: "7d", pct: 91, resetsAt: 110 * H1 },
      ],
    },
    { account: "api", kind: "api", state: "ok", slots: [] },
  ];
}

function swappedSess(over: Partial<Sess> = {}): Sess {
  return { start: "work", current: "personal", ...over };
}

function abs(t: number | undefined): number | undefined {
  return t === undefined ? undefined : now() + Math.round(t);
}

/** 稿里的语义位 → claude 回包头里那个窗口的原名（额度账按原名记）。 */
const RAW_WINDOW: Record<string, string> = { "5h": "five_hour", "7d": "seven_day" };

/** 一个号在额度账上那一条（中转从回包头记下的原始几格）：被拒 · 超额 · 预警只写回包说了什么，显示成什么样由真后端判。 */
function ledgerOf(a: Acct): QuotaSpec {
  const t = now();
  const lim = a.limiting ? RAW_WINDOW[a.limiting] : undefined;
  const limSlot = a.slots.find((x) => x.slot === a.limiting);
  return {
    account: a.account,
    seenAt: t - (a.seenAgo ?? 120),
    status: a.state === "refused" || a.state === "overageInUse" ? "rejected" : a.state === "near" ? "warning" : "allowed",
    refused: a.state === "refused",
    ...(lim ? { limiting: lim } : {}),
    ...(a.readingReset !== undefined ? { resetsAt: abs(a.readingReset) } : a.state === "overageInUse" && limSlot?.resetsAt !== undefined ? { resetsAt: abs(limSlot.resetsAt) } : {}),
    windows: a.slots.map((x) => ({ name: RAW_WINDOW[x.slot] ?? x.slot, ...(x.pct === undefined ? {} : { used: x.pct / 100 }), ...(x.resetsAt === undefined ? {} : { resetsAt: abs(x.resetsAt) }) })),
    ...(a.state === "overageInUse" ? { overage: { status: "allowed" as const, inUse: true } } : {}),
  };
}

/** 账号世界落成本机家目录里的原始文件：账号库 · 额度账 · 轮换账本（第三版：每个会话那一条，不轮换）· 会话血缘。 */
function acctDisk(aw: AcctWorld, w: World): void {
  const d = w.disk[LOCAL];
  const t = now();
  putAccounts(d, aw.accounts.map((a, i) => ({ name: a.account, kind: a.kind, isDefault: i === 0, signedIn: (a.login ?? "ok") === "ok" })));
  putQuota(d, aw.accounts.filter((a) => !a.unseen).map(ledgerOf));
  const sidAt = (i: number): string | undefined => w.sessions[i]?.sid;
  const sessions: Record<string, RotationSessionSpec> = {};
  const parents: Record<string, string> = {};
  for (const [k, s] of Object.entries(aw.sessions)) {
    const sid = sidAt(Number(k));
    if (sid === undefined) continue;
    const parent = s.parent === undefined ? undefined : sidAt(s.parent);
    if (parent !== undefined) parents[sid] = parent;
    if (s.absent) continue;
    sessions[sid] = { start: s.start, current: s.current, since: t - 1800 };
  }
  if (!aw.noBook) putRotation(d, { sessions });
  if (Object.keys(parents).length > 0) putLineage(d, parents, t - 1800);
  if (aw.warm) putWarm(d, aw.warm.map((x) => ({ account: x.account, at: t + x.at })));
}

function world(
  build: (aw: AcctWorld) => void,
  base: () => World = defaultWorld,
): () => World {
  return () => {
    const w = base();
    const aw: AcctWorld = {
      accounts: baseAccounts(),
      sessions: { 0: swappedSess() },
    };
    build(aw);
    acctDisk(aw, w);
    return w;
  };
}

function scene(
  id: string,
  title: string,
  desc: string,
  act: Scene["act"],
  w: () => World,
  size: [number, number] = [W, H],
  pointer?: string,
): Scene {
  return {
    id,
    page: "index",
    dir: "额度与账号",
    title,
    desc,
    width: size[0],
    height: size[1],
    world: w,
    act,
    ...(pointer ? { pointer } : {}),
  };
}

async function hover(sel: string | Element): Promise<void> {
  const el = typeof sel === "string" ? await waitFor(sel) : sel;
  el.dispatchEvent(new MouseEvent("mouseenter"));
  await sleep(700);
}

async function ready(): Promise<void> {
  await mainReady(ALL_TABS);
  await openTab(0);
  await sleep(600);
}

async function openPanel(): Promise<void> {
  await ready();
  await click(".status-account");
  await waitFor('aside[role="dialog"]');
  await sleep(700);
}

export const ACCT_SCENES: Scene[] = [
  // ── 状态栏按钮各态（每张都带悬停卡）
  scene(
    "acct-chip-swapped",
    "按钮 · 换过号 ＋ 悬停卡",
    "本会话 work 满了自动换到 personal：⇄ personal 5h 63%；悬停卡多 ⇄ 14:20 ← work 与 下一个 team",
    async () => {
      await ready();
      await hover(".status-account");
    },
    world(() => {}),
  ),
  scene(
    "acct-chip-near",
    "按钮 · 到阈值",
    "personal 5h 86%：数字琥珀（阈值 = 本会话 ≥90%，后端判的 near）",
    async () => {
      await ready();
      await hover(".status-account");
    },
    world((aw) => {
      aw.accounts[1] = {
        ...aw.accounts[1],
        state: "near",
        slots: [
          { slot: "5h", pct: 92, resetsAt: 1.83 * H1 },
          { slot: "7d", pct: 41, resetsAt: 88 * H1 },
        ],
      };
    }),
  ),
  scene(
    "acct-chip-refused",
    "按钮 · 被拒 ＋ 提示条 ＋ 标签页 ✕ 5h",
    "work 5h ✕ ↻19:00（红）；会话头下提示条 轮换内账号均已满 · 最早 …；tab 标题后 ✕ 5h；可用的按量号给一颗 切到 api · 按量",
    async () => {
      await ready();
      await hover(".status-account");
    },
    world((aw) => {
      aw.sessions[0] = {
        start: "work",
        current: "work",
      };
      aw.accounts[1] = {
        ...aw.accounts[1],
        state: "refused",
        readingReset: 1.83 * H1,
        slots: [
          { slot: "5h", pct: 100, resetsAt: 1.83 * H1 },
          { slot: "7d", pct: 41, resetsAt: 88 * H1 },
        ],
      };
    }),
  ),
  scene(
    "acct-chip-overage",
    "按钮 · 超额在兜",
    "work 5h 超额（琥珀）；悬停 超额 使用中 另计费",
    async () => {
      await ready();
      await hover(".status-account");
    },
    world((aw) => {
      aw.accounts[0] = {
        ...aw.accounts[0],
        state: "overageInUse",
        slots: [
          { slot: "5h", pct: 104, resetsAt: 2.3 * H1 },
          { slot: "7d", pct: 78, resetsAt: 40 * H1 },
        ],
      };
      aw.sessions[0] = {
        start: "work",
        current: "work",
      };
    }),
  ),
  scene(
    "acct-chip-api",
    "按钮 · 按量 / 按量被拒",
    "api 按量；悬停 5h/7d — 无限额",
    async () => {
      await ready();
      await hover(".status-account");
    },
    world((aw) => {
      aw.sessions[0] = {
        start: "api",
        current: "api",
      };
    }),
  ),
  scene(
    "acct-chip-api-refused",
    "按钮 · 按量被拒",
    "api ✕ ↻16:52（红）；悬停 状态 ✕ ↻.. +12m",
    async () => {
      await ready();
      await hover(".status-account");
    },
    world((aw) => {
      aw.accounts[3] = {
        ...aw.accounts[3],
        state: "refused",
        readingReset: 0.2 * H1,
      };
      aw.sessions[0] = {
        start: "api",
        current: "api",
      };
    }),
  ),
  scene(
    "acct-chip-stale",
    "按钮 · 数旧 · 上一窗已过",
    "team 5h —（上一窗已过）、采样 1h 前 ⇒ 整枚降透明度；悬停 ↻.. 已过 · 采样 几点 · 旧",
    async () => {
      await ready();
      await hover(".status-account");
    },
    world((aw) => {
      aw.accounts[2] = {
        ...aw.accounts[2],
        state: "resetSinceSeen",
        stale: true,
        seenAgo: 3900,
        slots: [
          { slot: "5h", pct: 97, resetsAt: -0.6 * H1 },
          { slot: "7d", pct: 12, resetsAt: 110 * H1 },
        ],
      };
      aw.sessions[0] = {
        start: "team",
        current: "team",
      };
    }),
  ),
  scene(
    "acct-chip-norelay",
    "按钮 · 未实时显示",
    "中转没见过这个会话：只画名字；悬停 用量 — 未实时显示 · 切换 仅重启切换",
    async () => {
      await ready();
      await hover(".status-account");
    },
    world((aw) => {
      aw.sessions[0] = { ...swappedSess(), absent: true };
    }),
  ),
  scene(
    "acct-chip-narrow",
    "按钮 · 窄窗 < 720",
    "只留头像 ＋ 值：PE 63%",
    async () => {
      await ready();
    },
    world(() => {}),
    [700, 600],
  ),
  scene(
    "acct-chip-default",
    "按钮 · 无会话 ⇒ 默认 work ＋ 默认下拉",
    "没有 tab：按钮退成 默认 work，点开即新会话默认下拉（每项带用量）",
    async () => {
      await waitFor(".status-account");
      await sleep(1200);
      await click(".status-account");
      await waitFor("[role=menu]");
      await sleep(500);
    },
    world(
      () => {},
      () => {
        const w = defaultWorld();
        w.sessions = [];
        return w;
      },
    ),
  ),
  scene(
    "acct-panel-unreadable",
    "面板 · 额度账读不出",
    "那份额度账读不出：头下一条警告条，后端写好的那一句（读的哪份 · 原因词）＋［复制详情］",
    openPanel,
    () => {
      const w = world(() => {})();
      putBrokenQuota(w.disk[LOCAL]);
      return w;
    },
  ),
  // ── 面板
  scene(
    "acct-panel",
    "面板 · 本会话 · 第一屏",
    "点状态栏按钮：底栏新会话默认 · 管理账号…（轮换那一块第 4 刀照稿施工，此刻空着）",
    openPanel,
    world(() => {}),
  ),
  scene(
    "acct-panel-norelay",
    "面板 · 未实时显示（热切换不成立）",
    "中转没见过：头下一条 未实时显示 [接入…]",
    openPanel,
    world((aw) => {
      aw.sessions[0] = { ...swappedSess(), absent: true };
    }),
  ),
  scene(
    "acct-tab-menu",
    "右键 · 账号…",
    "tab 右键：「换号重启 ▸」三层子菜单换成一项 账号…（开同一个面板）",
    async () => {
      await ready();
      await rightClick("#tab-bar .tab");
      await waitFor("[role=menu]");
      await sleep(600);
    },
    world(() => {}),
  ),
];