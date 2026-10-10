/**
 * 额度与账号：状态栏账号按钮各态 ＋ 悬停卡 · 「账号」面板（当前 · 轮换 · 无号可换两态 · 切换 · 记录 · 时间轴）·
 * 消息流换号条 · 会话头下提示条 · 标签页 `✕ 5h` · 右键「账号…」。
 *
 * 假后端答 `quota-read` · `rotation-rules-read` · `rotation-session-read/-set` · `rotation-switch`（形状照 IPC-PROTOCOL.md 与生成的类型），
 * 号名是编的。时刻按页里此刻的钟现算（「还有多久」只在画的那一刻算）。
 */
import type { Scene } from "./index";
import { fakePlan } from "../fake/timeline";
import type { OpHandler, World } from "../fake/types";
import { defaultWorld, LOCAL } from "../fake/world";
import {
  byText,
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
  follow: boolean;
  /** 用某条规则（给了就不看 `follow`）。 */
  rule?: string;
  custom?: Record<string, unknown>;
  history: Record<string, unknown>[];
  inPlace: string;
  next?: string;
  blocked?: Record<string, unknown>;
  fallbackApi?: string;
  absent?: boolean;
  /** 会话血缘里的父（tab 序号）；`followParent` ⇒ 来源 ＝ 跟随父会话。 */
  parent?: number;
  followParent?: boolean;
}
interface AcctWorld {
  accounts: Acct[];
  default: Record<string, unknown> | null;
  /** 默认那条（日常）之外的规则。 */
  rules?: {
    id: string;
    name: string;
    rotation: Record<string, unknown>;
    users?: number;
  }[];
  /** 按 tab 序号（0 起）。 */
  sessions: Record<number, Sess>;
  /** 重启那一形 `rotation-switch` 每个会话答什么（形状照 `rotation-switch-restart` 金样）；不给 ⇒ 成了、开 `proj-cc`。 */
  restartReply?: Record<string, unknown>;
  /** 时间轴：卡住（池里都被拒）· quota-warm 在跑。 */
  tl?: { blocked?: boolean; warm?: boolean; fallback?: boolean };
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
        { slot: "7d", pct: 12, resetsAt: 110 * H1 },
      ],
    },
    { account: "api", kind: "api", state: "ok", slots: [] },
  ];
}

const ROT_CUSTOM = {
  order: [{ start: true }, "personal", "team", "api"],
  enabled: ["personal", "team"],
  when: { threshold: { n: 90 } },
  atLimit: "continue",
  wait: 40,
};

function swappedSess(over: Partial<Sess> = {}): Sess {
  return {
    start: "work",
    current: "personal",
    follow: false,
    custom: structuredClone(ROT_CUSTOM),
    history: [
      {
        at: -0.4 * H1,
        from: "work",
        to: "personal",
        why: { full: { w: "5h" } },
        fromResetsAt: 2.3 * H1,
      },
    ],
    inPlace: "ok",
    next: "team",
    ...over,
  };
}

function abs(t: number | undefined): number | undefined {
  return t === undefined ? undefined : now() + Math.round(t);
}

function showOf(a: Acct): Record<string, unknown> {
  const slots = a.slots.map((x) => ({
    slot: x.slot,
    ...(x.pct === undefined ? {} : { pct: x.pct }),
    ...(x.resetsAt === undefined ? {} : { resetsAt: abs(x.resetsAt) }),
  }));
  return {
    kind: a.kind,
    state: a.unseen ? "unseen" : a.state,
    stale: a.stale ?? false,
    ...(a.limiting ? { limiting: a.limiting } : {}),
    slots,
    login: a.login ?? "ok",
  };
}

/** 规则表里的一行（摘要 · 说明是编的，形状照 `rotation-rules-read`）。 */
function ruleRow(
  id: string,
  name: string,
  rotation: Record<string, unknown>,
  isDefault: boolean,
  live: number,
): Record<string, unknown> {
  return {
    id,
    name,
    rotation,
    rev: 3,
    updatedAt: now() - 3600,
    isDefault,
    users: {
      live,
      ended: 2,
      follow: isDefault ? live : 0,
      doing: {},
      sids: [],
      endedSids: [],
    },
    summary: isDefault
      ? "起始 → personal · 满"
      : "team → personal · ≥90% · 抢回",
    explain: "起始账号先用 · 被拒才换 · 不主动换回",
    missing: [],
    atLimitApplies: rotation.when !== "full",
  };
}

function acctOps(aw: AcctWorld, w: () => World): Record<string, OpHandler> {
  const sidAt = (i: number): string => w().sessions[i].sid;
  const byAcct = (name: string): Acct | undefined =>
    aw.accounts.find((a) => a.account === name);
  const sessOf = (sid: string): Sess | undefined => {
    const i = w().sessions.findIndex((s) => s.sid === sid);
    return aw.sessions[i];
  };
  return {
    "accounts-list": () => ({
      accounts: aw.accounts.map((a, i) => ({
        name: a.account,
        email: a.kind === "api" ? "" : `${a.account}@example.com`,
        configDir: `/home/user/.cc-monitor/accounts/${a.account}`,
        isDefault: i === 0,
        mode: "isolated",
        exists: true,
        loggedIn: a.kind === "sub",
        authKind: a.kind === "api" ? "api-key" : "subscription",
        authReady: true,
        keyMasked: a.kind === "api" ? "••••••••a1b2" : null,
        baseUrl: a.kind === "api" ? "https://api.example.com" : null,
      })),
      meta: {
        enabled: true,
        acctsDir: "/home/user/.cc-monitor/accounts",
        manifestPath: "/home/user/.cc-monitor/accounts/accounts.json",
        updatedAt: "2026-10-01T08:00:00Z",
        sharedStore: null,
        count: aw.accounts.length,
        error: null,
        unsupported: null,
        nextDefault: null,
        home: "/home/user",
      },
      notice: null,
    }),
    "quota-read": () => ({
      state: "present",
      reason: null,
      path: "/home/user/.cc-monitor/quota.json",
      now: now(),
      accounts: aw.accounts
        .filter((a) => !a.unseen)
        .map((a) => ({
          agent: "claude-code",
          account: a.account,
          seenAt: now() - (a.seenAgo ?? 120),
          reading: {
            refused: a.state === "refused",
            ...(a.readingReset ? { resetsAt: abs(a.readingReset) } : {}),
          },
          ...showOf(a),
        })),
      unseen: aw.accounts
        .filter((a) => a.unseen)
        .map((a) => ({
          agent: "claude-code",
          account: a.account,
          kind: a.kind,
          login: a.login ?? "ok",
        })),
      usableNow: aw.accounts
        .filter(
          (a) =>
            !["refused", "overageInUse"].includes(a.state) &&
            (a.login ?? "ok") === "ok",
        )
        .map((a) => a.account),
      earliestReturn: aw.accounts.some((a) => a.state === "refused")
        ? { account: "work", at: abs(2.3 * H1) }
        : null,
    }),
    "rotation-rules-read": () => ({
      state: aw.default ? "present" : "absent",
      reason: null,
      path: "/home/user/.cc-monitor/rotation.json",
      defaultRule: "r_daily",
      rules: [
        ruleRow(
          "r_daily",
          "日常",
          aw.default ?? {
            order: [{ start: true }],
            enabled: [],
            when: "full",
            atLimit: "continue",
            wait: 40,
          },
          true,
          4,
        ),
        ...(aw.rules ?? []).map((r) =>
          ruleRow(r.id, r.name, r.rotation, false, r.users ?? 0),
        ),
      ],
    }),
    "rotation-session-read": (_o, req) => {
      const sessions: Record<string, unknown> = {};
      for (const sid of req.sids as string[]) {
        const s = sessOf(sid);
        if (!s || s.absent) {
          sessions[sid] = { state: "absent", inPlace: "noRelay" };
          continue;
        }
        const cur = byAcct(s.current);
        const parentSid = s.parent === undefined ? undefined : sidAt(s.parent);
        const decider = s.followParent && s.parent !== undefined ? aw.sessions[s.parent] : s;
        sessions[sid] = {
          state: "present",
          agent: "claude-code",
          source: s.followParent && parentSid
            ? { parent: parentSid }
            : s.rule
              ? { rule: s.rule }
              : s.follow
                ? "follow"
                : "custom",
          ...(parentSid ? { parent: parentSid } : {}),
          ...(decider && (decider.rule || decider.follow)
            ? {
                ruleName:
                  (aw.rules ?? []).find((r) => r.id === decider.rule)?.name ?? "日常",
              }
            : {}),
          explain:
            "起始账号先用 · 到 90% 从头取首个可用 · 不主动换回 · 都到上限仍发",
          ...(s.custom ? { custom: s.custom } : {}),
          account: {
            start: s.start,
            current: s.current,
            since: now() - 1800,
            history: s.history.map((h) => ({
              ...h,
              at: abs(h.at as number),
              ...(h.fromResetsAt === undefined
                ? {}
                : { fromResetsAt: abs(h.fromResetsAt as number) }),
            })),
            inPlace: s.inPlace,
          },
          ...(s.next ? { next: s.next } : {}),
          ...(s.blocked
            ? {
                blocked: {
                  earliest: {
                    account: (s.blocked as { account: string }).account,
                    at: abs((s.blocked as { at: number }).at),
                  },
                },
              }
            : {}),
          ...(s.fallbackApi ? { fallbackApi: s.fallbackApi } : {}),
          atLimit:
            (s.custom as { atLimit?: string } | undefined)?.atLimit ??
            "continue",
          quota: cur
            ? showOf(cur)
            : {
                kind: "sub",
                state: "unseen",
                stale: false,
                slots: [],
                login: "ok",
              },
        };
      }
      return { state: "present", reason: null, now: now(), sessions };
    },
    "rotation-session-set": (_o, req) => {
      const out: Record<string, unknown> = {};
      for (const sid of req.sids as string[]) {
        const s = sessOf(sid);
        if (!s) continue;
        const r = req.rotation as unknown;
        s.rule = undefined;
        s.followParent = false;
        if (r === "follow") s.follow = true;
        else if (r === "parent") s.followParent = true;
        else if (r === "custom" || r === "detach") {
          s.follow = false;
          s.custom ??= structuredClone(aw.default ?? ROT_CUSTOM);
        } else if (typeof r === "object" && r !== null && "rule" in r) {
          s.follow = false;
          s.rule = (r as { rule: string }).rule;
        } else {
          s.follow = false;
          s.custom = (r as { custom: Record<string, unknown> }).custom;
        }
        out[sid] = { state: "done" };
      }
      return { sessions: out };
    },
    "rotation-plan": (_o, req) =>
      req.view
        ? fakePlan({
            view: req.view as "6h" | "24h" | "7d",
            session: true,
            blocked: aw.tl?.blocked,
            warm: aw.tl?.warm,
            fallback: aw.tl?.fallback,
          })
        : { errors: [] },
    "rotation-rule-save": (_o, req) => ({
      state: "refused",
      errors: [{ cell: "name", code: "dup" }],
      ...(req.name === "x" ? {} : {}),
    }),
    "rotation-switch": (_o, req) => {
      const out: Record<string, unknown> = {};
      for (const it of req.sessions as unknown[]) {
        const sid = typeof it === "string" ? it : (it as { sid: string }).sid;
        const s = sessOf(sid);
        if (!s) continue;
        if (req.mode === "restart") {
          const reply = aw.restartReply ?? {
            state: "done",
            terminal: "proj-cc",
          };
          out[sid] = reply;
          if (reply.state === "failed") continue;
        }
        s.history.push({
          at: 0,
          from: s.current,
          to: req.target,
          why: req.mode === "hot" ? "manualHot" : "manualRestart",
        });
        s.current = String(req.target);
        out[sid] ??= { state: "done" };
      }
      void sidAt;
      return { sessions: out };
    },
  };
}

function world(
  build: (aw: AcctWorld) => void,
  base: () => World = defaultWorld,
): () => World {
  return () => {
    const w = base();
    const aw: AcctWorld = {
      accounts: baseAccounts(),
      default: {
        order: [{ start: true }, "personal"],
        enabled: ["personal"],
        when: "full",
        atLimit: "continue",
        wait: 40,
      },
      sessions: { 0: swappedSess() },
    };
    build(aw);
    Object.assign(
      w.ops,
      acctOps(aw, () => w),
    );
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

/** 面板里选 重启切换 再点；问会打断什么的框出来就点确认；等那一条报错 toast 出来。 */
async function restartThroughConfirm(): Promise<void> {
  await openPanel();
  await scrollPanelTo("切换");
  const r = [
    ...document.querySelectorAll<HTMLInputElement>(
      'aside[role="dialog"] input[type="radio"][name^="acct-mode"]',
    ),
  ][1];
  r.checked = true;
  r.dispatchEvent(new Event("change"));
  await sleep(300);
  await click(await byText('aside[role="dialog"] button', "重启切换"));
  const confirm = await waitFor('[role="alertdialog"]', 3000).catch(() => null);
  if (confirm) {
    await sleep(300);
    await click(await byText('[role="alertdialog"] button', "重启切换"));
  }
  await waitFor('#kit-toast-stack [role="alert"]');
  await sleep(500);
}

/** 开面板、摊开「时间轴」那一折、滚到它。 */
async function openTimeline(): Promise<void> {
  await openPanel();
  await scrollPanelTo("轮换");
  const head = await byText('aside[role="dialog"] [data-fold-head]', "时间轴").catch(() => null);
  if (head) await click(head);
  await sleep(900);
  document.querySelector("[data-tl-head]")?.scrollIntoView({ block: "start" });
  await sleep(300);
}

const scrollPanelTo = async (text: string): Promise<void> => {
  const h = await byText('aside[role="dialog"] h3', text);
  h.scrollIntoView({ block: "start" });
  await sleep(300);
};

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
        follow: false,
        custom: { ...ROT_CUSTOM, enabled: ["personal"] },
        history: [],
        inPlace: "ok",
        blocked: { account: "personal", at: 1.83 * H1 },
        fallbackApi: "api",
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
        follow: true,
        history: [],
        inPlace: "ok",
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
        follow: true,
        history: [],
        inPlace: "ok",
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
        follow: true,
        history: [],
        inPlace: "ok",
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
        follow: true,
        history: [],
        inPlace: "ok",
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
  // ── 面板
  scene(
    "acct-panel",
    "面板 · 本会话 · 第一屏",
    "点状态栏按钮：当前（两根条）· 轮换（本会话 · 触发 ≥90% · 无号可换 继续跑）· 切换 · 记录 · 底栏新会话默认",
    openPanel,
    world(() => {}),
  ),
  scene(
    "acct-src-open",
    "面板 · 来源下拉开着",
    "轮换第一行 来源 [本会话 ▾]：点开列 跟随默认 · 本会话（当前项打勾）；合着时方向键不写",
    async () => {
      await openPanel();
      await click('aside[role="dialog"] [data-acct-src]');
      await waitFor("[role=menu]");
      await sleep(400);
    },
    world(() => {}),
  ),
  scene(
    "acct-src-done",
    "面板 · 换了来源 ⇒ 带撤销的 toast",
    "下拉里选 跟随默认：只写一次；右下 toast orders · 跟随默认 ［撤销］",
    async () => {
      await openPanel();
      await click('aside[role="dialog"] [data-acct-src]');
      await waitFor("[role=menu]");
      await click(await byText('[role=menu] [role^="menuitem"]', "跟随默认"));
      await waitFor('#kit-toast-stack [role="status"]');
      await sleep(600);
    },
    world(() => {}),
  ),
  scene(
    "acct-src-peek",
    "面板 · 来源下拉 · 规则悬停小卡",
    "下拉里悬停 夜间 300ms：右侧只读小卡 顺序（起始 · team · personal，封顶标签 · 兜底）＋ 后端那句说明",
    async () => {
      await openPanel();
      await click('aside[role="dialog"] [data-acct-src]');
      await waitFor("[role=menu]");
      const night = [
        ...document.querySelectorAll<HTMLElement>(
          '[role="menu"] [role^="menuitem"]',
        ),
      ].find((b) => b.textContent?.startsWith("夜间"))!;
      await hover(night);
      await sleep(700);
    },
    world((aw) => {
      aw.rules = [
        {
          id: "r_night",
          name: "夜间",
          rotation: {
            order: [{ start: true }, "team", "personal"],
            enabled: ["team", "personal"],
            when: { threshold: { n: 90 } },
            atLimit: "continue",
            wait: 40,
            preempt: true,
            fallback: ["personal"],
            cap: { team: { "*": [{ at: "17:00-02:00", n: 0 }] } },
          },
          users: 2,
        },
      ];
    }),
  ),
  scene(
    "acct-src-rules",
    "面板 · 来源下拉列规则",
    "这台有 日常（默认）· 夜间 两条：下拉列 跟随默认（灰字 日常）· 规则 日常 · 规则 夜间 · 本会话；本会话在用 夜间",
    async () => {
      await openPanel();
      await click('aside[role="dialog"] [data-acct-src]');
      await waitFor("[role=menu]");
      await sleep(400);
    },
    world((aw) => {
      aw.rules = [
        {
          id: "r_night",
          name: "夜间",
          rotation: {
            order: [{ start: true }, "team", "personal"],
            enabled: ["team", "personal"],
            when: { threshold: { n: 90 } },
            atLimit: "continue",
            wait: 40,
            preempt: true,
          },
          users: 2,
        },
      ];
      aw.sessions[0] = {
        ...swappedSess(),
        follow: false,
        rule: "r_night",
        custom: undefined,
      };
    }),
  ),
  scene(
    "acct-rule-readonly",
    "面板 · 来源 ＝ 规则 夜间（只读）",
    "上方一行 规则 夜间 · 在用 2 会话 ［编辑规则…］［转为本会话］；触发灰、列表只读、personal 行尾封顶写成标签 17:00-02:00 停用；team 一枚实心 兜底（关着的不画）",
    openPanel,
    world((aw) => {
      aw.rules = [
        {
          id: "r_night",
          name: "夜间",
          rotation: {
            order: [{ start: true }, "team", "personal"],
            enabled: ["team", "personal"],
            when: { threshold: { n: 90 } },
            atLimit: "continue",
            wait: 40,
            preempt: true,
            fallback: ["team"],
            cap: { personal: { "*": [{ at: "17:00-02:00", n: 0 }] } },
          },
          users: 2,
        },
      ];
      aw.sessions[0] = {
        ...swappedSess(),
        follow: false,
        rule: "r_night",
        custom: undefined,
      };
    }),
  ),
  scene(
    "acct-src-parent",
    "面板 · 来源下拉有「跟随父会话」",
    "这个会话是在 tab 2 那个会话里起的：下拉紧跟 跟随默认 出 跟随父会话（灰字 父标题 · 夜间）；当前项打勾",
    async () => {
      await openPanel();
      await click('aside[role="dialog"] [data-acct-src]');
      await waitFor("[role=menu]");
      await sleep(400);
    },
    world((aw) => {
      aw.rules = [
        {
          id: "r_night",
          name: "夜间",
          rotation: {
            order: [{ start: true }, "team", "personal"],
            enabled: ["team", "personal"],
            when: "full",
            atLimit: "continue",
            wait: 40,
            preempt: true,
          },
          users: 2,
        },
      ];
      aw.sessions[1] = { ...swappedSess(), follow: false, rule: "r_night", custom: undefined };
      aw.sessions[0] = { ...swappedSess(), custom: undefined, parent: 1, followParent: true };
    }),
  ),
  scene(
    "acct-parent-readonly",
    "面板 · 来源 ＝ 跟随父会话（只读）",
    "上方一行 跟随父会话 {父标题} · 规则 夜间 ［打开父会话］［转为本会话］；触发灰、列表只读",
    openPanel,
    world((aw) => {
      aw.rules = [
        {
          id: "r_night",
          name: "夜间",
          rotation: {
            order: [{ start: true }, "team", "personal"],
            enabled: ["team", "personal"],
            when: "full",
            atLimit: "continue",
            wait: 40,
            preempt: true,
          },
          users: 2,
        },
      ];
      aw.sessions[1] = { ...swappedSess(), follow: false, rule: "r_night", custom: undefined };
      aw.sessions[0] = { ...swappedSess(), custom: undefined, parent: 1, followParent: true };
    }),
  ),
  scene(
    "acct-custom-edit",
    "面板 · 本会话（可改）",
    "换法 按顺序 | 抢回 | 单段预算 · 最多等 10 分；每行行尾 封顶 按钮（team 设了 ≤80）；右上 存为规则…",
    openPanel,
    world((aw) => {
      aw.sessions[0] = {
        ...swappedSess(),
        custom: { ...ROT_CUSTOM, cap: { team: { "*": 80 } } },
      };
    }),
  ),
  scene(
    "acct-fallback-on",
    "面板 · 兜底开着",
    "team 标了兜底：行里一枚实心 兜底；别的号的开关平时不显示",
    openPanel,
    world((aw) => {
      aw.sessions[0] = {
        ...swappedSess(),
        custom: { ...ROT_CUSTOM, fallback: ["team"] },
      };
    }),
  ),
  scene(
    "acct-fallback-off",
    "面板 · 兜底都关着",
    "没有号标兜底：每行都不显示 兜底（悬停或键盘到那一行才出）",
    openPanel,
    world((aw) => {
      aw.sessions[0] = { ...swappedSess(), custom: { ...ROT_CUSTOM } };
    }),
  ),
  scene(
    "acct-fallback-hover",
    "面板 · 悬停一行 ⇒ 兜底开关出来",
    "鼠标停在 team 那一行：行尾出一枚描边 兜底（点了才变实心）；api 标了兜底、实心",
    openPanel,
    world((aw) => {
      aw.sessions[0] = {
        ...swappedSess(),
        custom: { ...ROT_CUSTOM, fallback: ["api"] },
      };
    }),
    [W, H],
    '[data-acct-row="team"]',
  ),
  scene(
    "acct-rows-narrow",
    "面板 · 窄窗 · 行挤了先截号名",
    "窗宽 700：长号名截成省略号，在用 · 起始 · 兜底 等标签全字、重置时刻一行",
    openPanel,
    world((aw) => {
      aw.accounts.push({
        account: "research-shared-pool-01",
        kind: "sub",
        state: "ok",
        limiting: "5h",
        slots: [
          { slot: "5h", pct: 22, resetsAt: 3.2 * H1 },
          { slot: "7d", pct: 30, resetsAt: 90 * H1 },
        ],
      });
      aw.sessions[0] = {
        ...swappedSess(),
        current: "research-shared-pool-01",
        custom: {
          ...ROT_CUSTOM,
          order: [
            { start: true },
            "research-shared-pool-01",
            "personal",
            "team",
            "api",
          ],
          enabled: ["research-shared-pool-01", "personal", "team"],
          fallback: ["team"],
          cap: { personal: { "*": 99 } },
        },
      };
    }),
    [700, 600],
  ),
  scene(
    "acct-cap-pop",
    "面板 · 封顶浮层 · 按时段",
    "personal 行尾 封顶 ⇒ 浮层 personal · 全部窗口：不设 | 固定 | 按时段；两段 17:00–02:00 上限 0 · 02:00–17:00 上限 99；24h 色带（0 斜纹）",
    async () => {
      await openPanel();
      await click('aside[role="dialog"] [data-rot-cap-btn="personal"]');
      await waitFor('[data-rot-cap="personal"]');
      await sleep(400);
    },
    world((aw) => {
      aw.sessions[0] = {
        ...swappedSess(),
        custom: {
          ...ROT_CUSTOM,
          cap: {
            personal: {
              "*": [
                { at: "17:00-02:00", n: 0 },
                { at: "02:00-17:00", n: 99 },
              ],
            },
          },
        },
      };
    }),
  ),
  scene(
    "acct-save-as",
    "面板 · 存为规则浮层 · 重名",
    "右上 存为规则… ⇒ 名称 夜间 · ☑ 本会话改用这条规则；按存 ⇒ 后端回重名，框下红字",
    async () => {
      await openPanel();
      await click('aside[role="dialog"] [data-acct-save-as]');
      const input = await waitFor<HTMLInputElement>(
        "[data-rot-save] input[type=text]",
      );
      input.value = "夜间";
      await click("[data-rot-save-ok]");
      await sleep(500);
    },
    world(() => {}),
  ),
  scene(
    "acct-wait-record",
    "面板 · 记录 · 停着等前面的号",
    "personal 被拒、work 2 分钟后恢复（最多等 10 分钟）⇒ 不用兜底 team：记录一行 先等 work · 不用兜底 team ↻…",
    async () => {
      await openPanel();
      await scrollPanelTo("记录");
    },
    world((aw) => {
      aw.sessions[0] = {
        ...swappedSess(),
        history: [
          ...swappedSess().history,
          {
            at: -0.02 * H1,
            from: "personal",
            to: "personal",
            why: { wait: { account: "work", instead: "team" } },
            fromResetsAt: 0.03 * H1,
          },
        ],
      };
    }),
  ),
  scene(
    "acct-panel-toast",
    "面板开着时来一条提示",
    "账号面板开着、按快捷键翻一下自动跟随：右下角那条让到面板左边，不压面板底栏「新会话默认」那一行",
    async () => {
      await openPanel();
      const t = document.activeElement as HTMLElement | null;
      (t ?? document.body).dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "J",
          code: "KeyJ",
          ctrlKey: true,
          shiftKey: true,
          bubbles: true,
          cancelable: true,
        }),
      );
      await sleep(700);
    },
    world(
      () => {},
      () => {
        const w = defaultWorld();
        w.config = {
          ...w.config,
          keybindings: { "behavior.toggle-auto-follow": "Ctrl+Shift+KeyJ" },
        };
        return w;
      },
    ),
  ),
  scene(
    "acct-panel-lower",
    "面板 · 切换 · 记录 · 底栏",
    "同上，滚到下半",
    async () => {
      await openPanel();
      await scrollPanelTo("切换");
    },
    world(() => {}),
  ),
  scene(
    "acct-panel-follow",
    "面板 · 跟随默认",
    "轮换 = 默认：列表只读（无把手、勾灰），上方 跟随本机默认 设置…；触发与无号可换照默认那一份、灰着",
    openPanel,
    world((aw) => {
      aw.sessions[0] = { ...swappedSess(), follow: true, custom: undefined };
    }),
  ),
  scene(
    "acct-panel-norelay",
    "面板 · 未实时显示（热切换不成立）",
    "中转没见过：当前块一条 未实时显示 · 无用量 · 不自动轮换 [接入…]；热切换灰、写原因、主按钮 重启切换",
    openPanel,
    world((aw) => {
      aw.sessions[0] = { ...swappedSess(), absent: true };
    }),
  ),
  scene(
    "acct-panel-timeline",
    "面板 · 被拒 ⇒ 时间轴自动展开",
    "本会话的号被拒：时间轴自动展开（红段 = 被拒区间，| 5h 重置 ◆ 7d 重置）",
    async () => {
      await openPanel();
      await scrollPanelTo("轮换");
    },
    world((aw) => {
      aw.sessions[0] = {
        start: "work",
        current: "work",
        follow: false,
        custom: { ...ROT_CUSTOM },
        history: [],
        inPlace: "ok",
        next: "personal",
      };
    }),
  ),
  scene(
    "acct-panel-pick",
    "面板 · 选号下拉",
    "切换那一块的下拉：在用项灰 在用、被拒项琥珀、按量 按量",
    async () => {
      await openPanel();
      await scrollPanelTo("切换");
      const pick = document.querySelector<HTMLElement>(
        'aside[role="dialog"] button[aria-label="选账号"]',
      );
      if (pick) await click(pick);
      await sleep(500);
    },
    world(() => {}),
  ),
  scene(
    "acct-panel-restart",
    "面板 · 重启切换 ⇒ 先问会打断什么",
    "选 重启切换 再点：标题 重启切换 → team，中断 当前轮次 · 保留 会话记录 · 续接",
    async () => {
      await openPanel();
      await scrollPanelTo("切换");
      const r = [
        ...document.querySelectorAll<HTMLInputElement>(
          'aside[role="dialog"] input[type="radio"][name^="acct-mode"]',
        ),
      ][1];
      r.checked = true;
      r.dispatchEvent(new Event("change"));
      await sleep(300);
      await click(await byText('aside[role="dialog"] button', "重启切换"));
      await waitFor(
        '[role="alertdialog"], dialog[open], [role="dialog"][aria-modal]',
      );
      await sleep(500);
    },
    world(() => {}),
  ),
  scene(
    "acct-restart-fail-kept",
    "面板 · 重启切换失败 · 原会话保留",
    "重启切换确认后那台答 failed · not_in_terminal · old=kept：toast 重启切换失败 · 原会话保留 · 原因，不自动消失、无按钮",
    async () => {
      await restartThroughConfirm();
    },
    world((aw) => {
      aw.restartReply = {
        state: "failed",
        code: "not_in_terminal",
        old: "kept",
      };
    }),
  ),
  scene(
    "acct-restart-fail-ended",
    "面板 · 重启切换失败 · 原会话已结束",
    "重启切换确认后那台答 failed · start_failed · old=ended：toast 重启切换失败 · 原会话已结束 · 原因 ＋［恢复…］［日志］，不自动消失",
    async () => {
      await restartThroughConfirm();
    },
    world((aw) => {
      aw.restartReply = { state: "failed", code: "start_failed", old: "ended" };
    }),
  ),
  scene(
    "acct-switch-strip",
    "消息流 · 换号条",
    "面板里热切换到 team：toast · 当前与记录原地换 · 消息流末尾一行 ⇄ 时刻 personal → team · 手动 · 热切换 · 账号…",
    async (ctx) => {
      await openPanel();
      await scrollPanelTo("切换");
      await click(await byText('aside[role="dialog"] button', "切换"));
      await sleep(400);
      ctx.backend.pushQuota(LOCAL, { sid: ctx.backend.world.sessions[0].sid });
      await sleep(800);
      document.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
      );
      document.body.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "Escape",
          code: "Escape",
          bubbles: true,
          cancelable: true,
        }),
      );
      await sleep(500);
      document
        .querySelector<HTMLElement>("[data-acct-strip]")
        ?.scrollIntoView({ block: "center" });
      await sleep(400);
    },
    world(() => {}),
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
  // ── 稿里没画的：无号可换两态（触发那一行旁）
  scene(
    "acct-atlimit-continue",
    "无号可换 · 继续跑（缺省）",
    "触发 ≥90% 时：轮换内都 ≥90% ⇒ 继续跑（留在当前号照发、被拒才换）；悬停一行说明",
    async () => {
      await openPanel();
      await scrollPanelTo("轮换");
      await hover(
        await byText(
          'aside[role="dialog"] [aria-label="无号可换时"] button',
          "继续跑",
        ),
      );
    },
    world(() => {}),
  ),
  scene(
    "acct-atlimit-stop",
    "无号可换 · 停",
    "选 停：轮换内都 ≥90% ⇒ 停发、最早有号回到 90% 以下时续发（claude 照它自己的用满样子停下）；悬停一行说明",
    async () => {
      await openPanel();
      await scrollPanelTo("轮换");
      await click(
        await byText(
          'aside[role="dialog"] [aria-label="无号可换时"] button',
          "停",
        ),
      );
      await sleep(600);
      await hover(
        await byText(
          'aside[role="dialog"] [aria-label="无号可换时"] button',
          "停",
        ),
      );
    },
    world(() => {}),
  ),
  scene(
    "acct-atlimit-full",
    "无号可换 · 触发 = 满 时灰着",
    "触发 = 满（被拒才换）时没有 N%：两态灰着，悬停 仅按 ≥N% 触发时有效",
    async () => {
      await openPanel();
      await scrollPanelTo("轮换");
      await hover(await byText('aside[role="dialog"] span', "无号可换"));
    },
    world((aw) => {
      aw.sessions[0] = {
        ...swappedSess(),
        custom: { ...ROT_CUSTOM, when: "full" },
      };
    }),
  ),
  // ── 时间轴
  scene(
    "acct-timeline",
    "面板 · 时间轴 · 平时（24h）",
    "顶行 在用 personal 5h 63% · 距触发 27 点；本会话轨：过去实线（work → personal，换号点 ✕）、将来虚线（≥90% → team · 抢回 → work）；泳道 被拒 · 过封顶 · 时段停用 · 重置；现在竖线",
    async () => {
      await openTimeline();
    },
    world((aw) => {
      aw.tl = { warm: true };
    }),
    [W, 1000],
  ),
  scene(
    "acct-timeline-blocked",
    "面板 · 时间轴 · 卡住（自动展开）",
    "顶行琥珀条 轮换内均不可用 · 最早 team ↻… · 5h 重置；本会话轨此刻起一段停发（斜纹），之后接 team",
    async () => {
      await openPanel();
      await scrollPanelTo("轮换");
      await sleep(900);
      document.querySelector("[data-tl-head]")?.scrollIntoView({ block: "start" });
      await sleep(300);
    },
    world((aw) => {
      aw.tl = { blocked: true };
    }),
    [W, 1000],
  ),
  scene(
    "acct-timeline-fallback",
    "面板 · 时间轴 · 兜底（6h）",
    "team 是兜底：过去 work 被拒、personal 快恢复 ⇒ 一段 先等 personal（不发上游）再接 personal；将来 ≥90% → team，personal 重置 ⇒ 换下兜底 → personal",
    async () => {
      await openTimeline();
      await click(await byText("[data-tl-view] button", "6h"));
      await sleep(900);
    },
    world((aw) => {
      aw.tl = { fallback: true };
    }),
    [W, 1000],
  ),
  scene(
    "acct-timeline-hover",
    "面板 · 时间轴 · 悬停一列",
    "键盘进轴、→ 走三格：竖向细线 ＋ 卡「几点 · 用谁 · 谁不能用到几点」",
    async () => {
      await openTimeline();
      const tl = await waitFor("[data-tl]");
      (tl as HTMLElement).focus();
      for (let i = 0; i < 3; i++)
        tl.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
      await sleep(300);
    },
    world(() => {}),
    [W, 1000],
  ),
  scene(
    "acct-timeline-7d",
    "面板 · 时间轴 · 7d",
    "7d 视窗（前 1d · 后 6d）：本会话轨将来只画到 +1d，其后写 —；刻度写日期",
    async () => {
      await openTimeline();
      await click(await byText("[data-tl-view] button", "7d"));
      await sleep(900);
    },
    world(() => {}),
    [W, 1000],
  ),
];
