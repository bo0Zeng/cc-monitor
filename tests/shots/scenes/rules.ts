/**
 * 设置 → 机器 →「轮换」栏：规则列表 · ⋯ · 在用展开 · 批量条 · 新建 · 删除框 · 空态 · 窄窗（稿 `轮换规则.md` §5.5、截图 05）。
 * 假后端答 `rotation-rules-read`（三条规则，名字与摘要是编的）；会话标题取图集那份会话清单（`history-list`）。
 */
import { emit } from "@tauri-apps/api/event";
import { fakePlan } from "../fake/timeline";
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { defaultWorld } from "../fake/world";
import { click, sleep, waitFor } from "./helpers";

const S1 = "5e550001-0000-4000-8000-000000000001";
const S2 = "5e550002-0000-4000-8000-000000000002";
const S3 = "5e550003-0000-4000-8000-000000000003";

/** 在用名单里各会话此刻的状态（后端判；这里编的：一个在跑、一个等批准，已结束的照写已结束）。 */
const DOING: Record<string, { state: string; needs: string | null }> = {
  [S1]: { state: "working", needs: null },
  [S2]: { state: "needsYou", needs: "approve" },
};

interface R {
  id: string;
  name: string;
  summary: string;
  live?: string[];
  ended?: string[];
  isDefault?: boolean;
  follow?: number;
}

const ROT = {
  order: [{ start: true }, "personal", "work"],
  enabled: ["personal", "work"],
  when: "full",
  atLimit: "continue",
  wait: 40,
};

const THREE: R[] = [
  {
    id: "r_daily",
    name: "日常",
    summary: "personal → work → team · ≥90% · 停 · 封顶 2",
    live: [S1, S2],
    ended: [S3],
    isDefault: true,
    follow: 2,
  },
  {
    id: "r_night",
    name: "夜间",
    summary: "team → lab → work · 满 · 抢回 · work 17:00–02:00 停用",
    live: [S2],
  },
  {
    id: "r_saver",
    name: "省额度",
    summary: "lab → team → api · ≥80% · 单段 10",
  },
];

function rulesWorld(list: R[] = THREE): () => World {
  return () => {
    const w = defaultWorld();
    w.ops["rotation-plan"] = (_o, req) =>
      req.machine
        ? fakePlan({ view: (req.view as "6h" | "24h" | "7d") ?? "24h", session: false, warm: true })
        : { errors: [] };
    w.ops["rotation-rules-read"] = () => ({
      state: "present",
      reason: null,
      path: "/home/user/.cc-monitor/rotation.json",
      defaultRule: list.find((r) => r.isDefault)?.id ?? list[0].id,
      rules: list.map((r) => ({
        id: r.id,
        name: r.name,
        rotation: ROT,
        rev: 2,
        updatedAt: 0,
        isDefault: r.isDefault ?? false,
        users: {
          live: r.live?.length ?? 0,
          ended: r.ended?.length ?? 0,
          follow: r.follow ?? 0,
          doing: Object.fromEntries([
            ...(r.live ?? []).map((sid) => [
              sid,
              DOING[sid] ?? { state: "working", needs: null },
            ]),
            ...(r.ended ?? []).map((sid) => [
              sid,
              { state: "ended", needs: null },
            ]),
          ]),
          sids: r.live ?? [],
          endedSids: r.ended ?? [],
        },
        summary: r.summary,
        explain: "",
        missing: [],
        atLimitApplies: false,
      })),
    });
    return w;
  };
}

/** 编辑器那一组：夜间的轮换（抢回 · work 17:00–02:00 停用）、这台三个号的此刻用量、后端算好的预览（编的）。 */
const NIGHT_ROT = {
  order: [{ start: true }, "team", "lab", "work", "personal"],
  enabled: ["team", "lab", "work"],
  when: { threshold: { n: 90 } },
  atLimit: "continue",
  wait: 40,
  preempt: true,
  fallback: ["work"],
  cap: {
    work: {
      "*": [
        { at: "17:00-02:00", n: 0 },
        { at: "02:00-17:00", n: 99 },
      ],
    },
    lab: { "5h": 80 },
  },
};

function editorWorld(): () => World {
  const base = rulesWorld();
  return () => {
    const w = base();
    const t = Math.floor(Date.now() / 1000);
    const H = 3600;
    const at = (x: number): { at: number; atText: string } => ({
      at: t + x,
      atText: hm(t + x),
    });
    const hm = (x: number): string => {
      const d = new Date(x * 1000);
      return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
    };
    const span = (a: number, b: number) => ({
      from: t + a,
      fromText: hm(t + a),
      to: t + b,
      toText: hm(t + b),
    });
    const rulesRead = w.ops["rotation-rules-read"];
    w.ops["rotation-rules-read"] = (o, req, world) => {
      const r = rulesRead(o, req, world) as {
        rules: {
          id: string;
          rotation: unknown;
          explain: string;
          atLimitApplies: boolean;
        }[];
      };
      for (const x of r.rules)
        if (x.id === "r_night") {
          x.rotation = NIGHT_ROT;
          x.explain =
            "起始账号先用 · 到 90% 从头取首个可用 · 前面的号有额度就换回它 · work 只兜底 · 别的号有额度就不用 work · 40 分钟内有号恢复就先等 · work 17:00-02:00 停用 · 都到上限仍发";
          x.atLimitApplies = true;
        }
      return r;
    };
    const slot = (pct: number, resetsIn: number) => [
      {
        slot: "5h",
        pct,
        resetsAt: t + resetsIn,
        resetsAtText: hm(t + resetsIn),
      },
      { slot: "7d", pct: Math.round(pct / 3), resetsAt: t + 4 * 86_400 },
    ];
    w.ops["quota-read"] = () => ({
      state: "present",
      reason: null,
      path: "/home/user/.cc-monitor/quota.json",
      now: t,
      accounts: [
        {
          agent: "claude-code",
          account: "team",
          seenAt: t - 60,
          reading: {},
          kind: "sub",
          state: "near",
          stale: false,
          limiting: "5h",
          slots: slot(88, 2 * H),
          login: "ok",
        },
        {
          agent: "claude-code",
          account: "lab",
          seenAt: t - 60,
          reading: {},
          kind: "sub",
          state: "ok",
          stale: false,
          limiting: "5h",
          slots: slot(22, 4 * H),
          login: "ok",
        },
        {
          agent: "claude-code",
          account: "work",
          seenAt: t - 60,
          reading: {},
          kind: "sub",
          state: "ok",
          stale: false,
          limiting: "5h",
          slots: slot(40, 3 * H),
          login: "ok",
        },
        {
          agent: "claude-code",
          account: "personal",
          seenAt: t - 60,
          reading: {},
          kind: "sub",
          state: "ok",
          stale: false,
          limiting: "5h",
          slots: slot(63, H),
          login: "ok",
        },
      ],
      unseen: [],
      usableNow: ["team", "lab", "work", "personal"],
      earliestReturn: null,
    });
    w.ops["rotation-plan"] = (_o, req) => {
      if (req.machine)
        return fakePlan({ view: (req.view as "6h" | "24h" | "7d") ?? "24h", session: false, warm: true });
      if (req.rotation)
        return {
          errors: [],
          now: t,
          nowText: hm(t),
          until: t + 12 * H,
          plan: [],
          lanes: [],
          effective: {
            work: {
              "*": { v: 0, layer: "all", below: { v: 90, layer: "trigger" } },
            },
          },
        };
      const end =
        (req.span === "6h"
          ? 6
          : req.span === "24h"
            ? 24
            : req.span === "7d"
              ? 168
              : 12) * H;
      return {
        errors: [],
        now: t,
        nowText: hm(t),
        until: t + end,
        plan: [
          { ...span(0, 0.6 * H), account: "team", why: null },
          {
            ...span(0.6 * H, 2 * H),
            account: "lab",
            why: { threshold: { n: 90 } },
          },
          { ...span(2 * H, 5 * H), account: "team", why: "preempt" },
          {
            ...span(5 * H, end),
            account: "lab",
            why: { threshold: { n: 90 } },
          },
        ],
        lanes: [
          {
            account: "team",
            spans: [
              { ...span(0.6 * H, 2 * H), state: "capped", n: 90 },
              { ...span(5 * H, Math.min(end, 7 * H)), state: "capped", n: 90 },
            ],
            resets: [{ w: "5h", ...at(2 * H) }],
          },
          { account: "lab", spans: [], resets: [{ w: "5h", ...at(4 * H) }] },
          {
            account: "work",
            spans: [
              { ...span(0, Math.min(end, 6 * H)), state: "off", n: null },
            ],
            resets: [{ w: "5h", ...at(3 * H) }],
          },
        ],
        effective: {
          team: {
            "5h": {
              v: 90,
              layer: "trigger",
              below: { v: 90, layer: "trigger" },
            },
            "7d": {
              v: 90,
              layer: "trigger",
              below: { v: 90, layer: "trigger" },
            },
            "*": {
              v: 90,
              layer: "trigger",
              below: { v: 90, layer: "trigger" },
            },
          },
          lab: {
            "5h": {
              v: 80,
              layer: "window",
              below: { v: 90, layer: "trigger" },
            },
            "7d": {
              v: 90,
              layer: "trigger",
              below: { v: 90, layer: "trigger" },
            },
            "*": {
              v: 90,
              layer: "trigger",
              below: { v: 90, layer: "trigger" },
            },
          },
          work: {
            "5h": { v: 0, layer: "all", below: { v: 0, layer: "all" } },
            "7d": { v: 0, layer: "all", below: { v: 0, layer: "all" } },
            "*": { v: 0, layer: "all", below: { v: 90, layer: "trigger" } },
          },
        },
      };
    };
    return w;
  };
}

async function goEditor(): Promise<void> {
  await goRules();
  await click('[data-rule="r_night"] [role="cell"]');
  await sleep(1500);
}

function scene(
  id: string,
  title: string,
  desc: string,
  act: Scene["act"],
  world: () => World = rulesWorld(),
  width = 960,
  height = 620,
): Scene {
  return {
    id,
    page: "settings",
    dir: "额度与账号",
    title,
    desc,
    width,
    height,
    world,
    act,
  };
}

/** 带目的地开到本机的「轮换」栏（窄窗时导航收起，点不着导航项 ⇒ 一律走目的地）。 */
async function goRules(): Promise<void> {
  await waitFor(".settings-nav");
  await sleep(1500);
  await emit(
    "settings-target",
    JSON.stringify({ machine: "<local>", tab: "rot" }),
  );
  await sleep(900);
  await waitFor("[data-rule]");
  await sleep(1700); // 落地那一下的高亮 1.5 秒后撤
}

const more = (id: string) => `[data-rules-more="${id}"]`;

export const RULES_SCENES: Scene[] = [
  scene(
    "rules-list",
    "设置 · 轮换 · 规则列表",
    "本机的规则：日常（默认）· 夜间 · 省额度；摘要后端写、一行截断；在用 2 会话 / 1 会话 / —",
    goRules,
  ),
  scene(
    "rules-users",
    "设置 · 轮换 · 在用展开",
    "点日常的「2 会话」：行下名单（标题取会话清单）· 已结束 1 折着 ·［全部改用 ▾］［全部转为本会话］",
    async () => {
      await goRules();
      await click('[data-rules-use="r_daily"]');
      await sleep(900);
    },
  ),
  scene(
    "rules-users-picked",
    "设置 · 轮换 · 在用展开 · 勾一个",
    "勾上一个会话：脚上「已勾 1 ·［改用 ▾］［转为本会话］」",
    async () => {
      await goRules();
      await click('[data-rules-use="r_daily"]');
      await sleep(900);
      await click(`[data-rules-users="r_daily"] [data-sid="${S1}"] input`);
      await sleep(400);
    },
  ),
  scene(
    "rules-more",
    "设置 · 轮换 · ⋯ 菜单",
    "夜间那一行的 ⋯：复制 · 改名 · 设为默认 · 复制到 · 删除",
    async () => {
      await goRules();
      await click(more("r_night"));
      await sleep(500);
    },
  ),
  scene(
    "rules-more-default",
    "设置 · 轮换 · 默认那条的 ⋯",
    "日常（默认）的 ⋯：设为默认打勾不可点 · 删除灰、第二行「默认规则 · 先设别的为默认」",
    async () => {
      await goRules();
      await click(more("r_daily"));
      await sleep(500);
    },
  ),
  scene(
    "rules-batch",
    "设置 · 轮换 · 批量条",
    "勾了夜间与省额度：表头换成「已选 2 ·［复制到 ▸］［删除］」",
    async () => {
      await goRules();
      await click('[data-rule="r_night"] input[type="checkbox"]');
      await sleep(200);
      await click('[data-rule="r_saver"] input[type="checkbox"]');
      await sleep(400);
    },
  ),
  scene(
    "rules-new",
    "设置 · 轮换 · 新建",
    "［+ 新建规则］：小浮层 名称 ＋ 从［日常 默认 ▾］·［取消］［建］",
    async () => {
      await goRules();
      await click("[data-rules-new]");
      await sleep(500);
    },
  ),
  scene(
    "rules-delete",
    "设置 · 轮换 · 删一条在用的",
    "删夜间（在用 1 会话）：确认框 改动 ＋ 单选（转为本会话 · 改为跟随默认）；焦点在取消",
    async () => {
      await goRules();
      await click(more("r_night"));
      await sleep(400);
      const del = [
        ...document.querySelectorAll<HTMLButtonElement>(
          '[role="menu"] [role^="menuitem"]',
        ),
      ].find((b) => b.textContent?.startsWith("删除"))!;
      await click(del);
      await sleep(900);
    },
  ),
  scene(
    "rules-rename",
    "设置 · 轮换 · 行内改名",
    "夜间 ⋯ 改名：名称格变成输入框（Enter 存 · Esc 取消 · 失焦存）",
    async () => {
      await goRules();
      await click(more("r_night"));
      await sleep(400);
      const it = [
        ...document.querySelectorAll<HTMLButtonElement>(
          '[role="menu"] [role^="menuitem"]',
        ),
      ].find((b) => b.textContent === "改名")!;
      await click(it);
      await sleep(500);
    },
  ),
  scene(
    "rules-empty",
    "设置 · 轮换 · 只有默认那一条",
    "空态：表下一行灰字「会话面板里『存为规则…』也能建」＋［新建规则］",
    goRules,
    rulesWorld([
      {
        id: "r_daily",
        name: "默认",
        summary: "起始 · 满",
        isDefault: true,
        live: [S1],
        follow: 1,
      },
    ]),
  ),
  scene(
    "rules-editor",
    "设置 · 轮换 · 规则编辑器",
    "点夜间：面包屑 轮换 / 夜间 · 后端那句说明 · 顺序（起始账号占位 · 封顶 · 兜底 · 用量）· 触发 · 换法三卡 · 无号可换 · 按号封顶表 · 预览",
    goEditor,
    editorWorld(),
    960,
    1500,
  ),
  scene(
    "rules-editor-cap",
    "设置 · 轮换 · 编辑器 · 封顶浮层",
    "work「全部窗口」那一格：按时段两段 ＋ 24h 色带，下面「其余时段 ＝ 触发 ≥90%」（后端 effective 的下一层）",
    async () => {
      await goEditor();
      await click('[data-ed-cap="work.*"]');
      await sleep(900);
    },
    editorWorld(),
    960,
    1100,
  ),
  scene(
    "rules-editor-narrow",
    "设置 · 轮换 · 编辑器 · 窄窗",
    "设置窗 <640：标签列折到上面一行，换法三卡竖排",
    goEditor,
    editorWorld(),
    600,
    1700,
  ),
  scene(
    "rules-narrow",
    "设置 · 轮换 · 窄窗",
    "设置窗 <640：摘要折到名称下一行，在用 · ⋯ 仍在行尾",
    goRules,
    rulesWorld(),
    600,
    620,
  ),
  scene(
    "rules-timeline",
    "设置 · 轮换 · 时间轴",
    "规则列表下常开：这台全部号 · 行头 在用 N 会话（≥2 琥珀）· 无本会话轨 · 顶行 可用 … · 最早恢复 …；开窗 ○",
    async () => {
      await goRules();
      document.querySelector("[data-rules-timeline]")?.scrollIntoView({ block: "start" });
      await sleep(400);
    },
    () => {
      const w = rulesWorld()();
      const t = Math.floor(Date.now() / 1000);
      const back = t + 2 * 3600;
      const d = new Date(back * 1000);
      w.ops["quota-read"] = () => ({
        state: "present",
        reason: null,
        now: t,
        accounts: [],
        unseen: [],
        usableNow: ["personal", "team", "lab", "api"],
        earliestReturn: {
          account: "work",
          at: back,
          atText: `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`,
        },
      });
      return w;
    },
    960,
    620,
  ),
];
