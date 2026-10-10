/**
 * 设置 → 机器 →「轮换」栏：规则列表 · ⋯ · 在用展开 · 批量条 · 新建 · 删除框 · 空态 · 窄窗（稿 `轮换规则.md` §5.5、截图 05）。
 * 规则表落在本机的轮换账本里（三条规则，名字是编的），摘要 · 说明 · 在用名单 · 预览都是真后端读它算的；会话标题取图集那份会话清单（`history-list`）。
 */
import { emit } from "@tauri-apps/api/event";
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { defaultWorld, LOCAL } from "../fake/world";
import { putAccounts, putBrokenRotation, putQuota, putRotation, putWarm, type QuotaSpec } from "../disk";
import { click, hover, sleep, type, waitFor } from "./helpers";

const S1 = "5e550001-0000-4000-8000-000000000001";
const S2 = "5e550002-0000-4000-8000-000000000002";
const S3 = "5e550003-0000-4000-8000-000000000003";

interface R {
  id: string;
  name: string;
  /** 用这条规则的会话（默认那条 ⇒ 跟随默认；其余 ⇒ 指定这条）。活没活着看那台的 pidfile（默认世界里 S3 已结束）。 */
  users?: string[];
  isDefault?: boolean;
  rotation?: Record<string, unknown>;
}

const ROT = {
  order: [{ start: true }, "personal", "work", "team"],
  enabled: ["personal", "work", "team"],
  atLimit: "continue",
  wait: 40,
  cap: { "*": { "5h": 90 } },
  stint: { "*": 2 },
};

/** 夜间的轮换（抢回 · work 只兜底、17:00–02:00 停用 · lab 单独的线）。 */
const NIGHT_ROT = {
  order: [{ start: true }, "team", "lab", "work", "personal"],
  enabled: ["team", "lab", "work"],
  atLimit: "continue",
  wait: 40,
  preempt: true,
  fallback: ["work"],
  cap: {
    "*": { "5h": 90 },
    work: {
      "*": [
        { at: "17:00-02:00", n: 0 },
        { at: "02:00-17:00", n: 99 },
      ],
    },
    lab: { "5h": 80, "7d": 95 },
  },
};

const SAVER_ROT = {
  order: [{ start: true }, "lab", "team", "api"],
  enabled: ["lab", "team", "api"],
  atLimit: "continue",
  wait: 40,
  cap: { "*": { "5h": 80, "7d": 95 } },
  stint: { "*": 10 },
};

const THREE: R[] = [
  { id: "r_daily", name: "日常", users: [S1, S3], isDefault: true, rotation: ROT },
  { id: "r_night", name: "夜间", users: [S2], rotation: NIGHT_ROT },
  { id: "r_saver", name: "省额度", rotation: SAVER_ROT },
];

/** 这台的号（规则里提到的都在库里）。 */
const ACCTS = ["work", "personal", "team", "lab"];

/** 规则表落成本机的轮换账本：规则 ＋ 用它的那几个会话记下的那一份；账号库按规则里提到的号建；quota-warm 在跑。 */
function rulesWorld(list: R[] = THREE): () => World {
  return () => {
    const w = defaultWorld();
    const d = w.disk[LOCAL];
    const t = Math.floor(Date.now() / 1000);
    putAccounts(d, [...ACCTS.map((name, i) => ({ name, kind: "sub" as const, isDefault: i === 0 })), { name: "api", kind: "api" }]);
    const dflt = list.find((r) => r.isDefault) ?? list[0];
    putRotation(d, {
      defaultRule: dflt.id,
      rules: Object.fromEntries(list.map((r) => [r.id, { name: r.name, rotation: r.rotation ?? ROT, rev: 2, updatedAt: t - 86_400 }])),
      sessions: Object.fromEntries(
        list.flatMap((r) => (r.users ?? []).map((sid) => [sid, { start: "work", current: "work", since: t - 3600, source: r === dflt ? "follow" : { rule: r.id } }])),
      ),
    });
    putWarm(d, [{ account: "lab", at: t + 2.5 * 3600 }]);
    return w;
  };
}

/** 编辑器那一组：这台四个号此刻的用量（team 快到线 · 其余有余量）。 */
function editorWorld(): () => World {
  const base = rulesWorld();
  return () => {
    const w = base();
    const t = Math.floor(Date.now() / 1000);
    const H = 3600;
    const acct = (account: string, pct: number, resetsIn: number): QuotaSpec => ({
      account,
      seenAt: t - 60,
      status: "allowed",
      limiting: "five_hour",
      windows: [
        { name: "five_hour", used: pct / 100, resetsAt: t + resetsIn },
        { name: "seven_day", used: Math.round(pct / 3) / 100, resetsAt: t + 4 * 86_400 },
      ],
    });
    putQuota(w.disk[LOCAL], [acct("team", 88, 2 * H), acct("lab", 22, 4 * H), acct("work", 40, 3 * H), acct("personal", 63, H)]);
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
    "rules-unreadable",
    "设置 · 轮换 · 规则表读不出",
    "那份规则表读不出：表顶一条警告条，后端写好的那一句（读的哪份 · 原因词）＋［复制详情］；表照后端给的（只剩默认那条）",
    goRules,
    () => {
      const w = rulesWorld([THREE[0]])();
      putBrokenRotation(w.disk[LOCAL]);
      return w;
    },
  ),
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
    rulesWorld([{ id: "r_daily", name: "默认", isDefault: true, users: [S1], rotation: { order: [{ start: true }], enabled: [], atLimit: "continue", wait: 40 } }]),
  ),
  scene(
    "rules-editor",
    "设置 · 轮换 · 规则编辑器",
    "点夜间：面包屑 轮换 / 夜间 · 后端那句说明 · 顺序（起始账号占位 · 封顶 · 兜底 · 用量）· 触发 满 / 到线 5h 90 · 7d 空 · 换法三卡 · 无号可换 · 按号封顶表（没设的格写落下来的灰值）· 预览",
    goEditor,
    editorWorld(),
    960,
    1500,
  ),
  scene(
    "rules-editor-cap",
    "设置 · 轮换 · 编辑器 · 封顶浮层",
    "work「全部窗口」那一格：按时段两段 ＋ 24h 色带，下面「其余时段 · 5h ≤90 · 7d 不封顶」（后端那一句两窗各取多少）",
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
    "rules-editor-line-bad",
    "设置 · 轮换 · 编辑器 · 触发填错",
    "5h 格填 120：后端回 cap.*.5h · range ⇒ 那一格框红、照原样留着 120 ＋ 行尾红字 1–99，不写盘；7d 那一格照旧",
    async () => {
      await goEditor();
      await type('[data-rot-line-num="5h"]', "120");
      document
        .querySelector('[data-rot-line-num="5h"]')
        ?.dispatchEvent(new Event("change", { bubbles: true }));
      await sleep(900);
      document.querySelector("[data-rot-trigger]")?.scrollIntoView({ block: "center" });
      await sleep(300);
    },
    () => {
      return editorWorld()();
    },
    960,
    620,
  ),
  scene(
    "rules-editor-line-empty",
    "设置 · 轮换 · 编辑器 · 7d 空着",
    "到线 · 只设 5h：悬停 7d 那一格 ⇒「空即 7d 满才换」",
    async () => {
      await goEditor();
      document.querySelector("[data-rot-trigger]")?.scrollIntoView({ block: "center" });
      await sleep(300);
      await hover('[data-rot-line-num="7d"]');
      await sleep(900);
    },
    editorWorld(),
    960,
    620,
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
      // work 被拒、两小时后回来；其余几个号有余量。
      const w = rulesWorld()();
      const t = Math.floor(Date.now() / 1000);
      const win = (pct: number, resetsIn: number) => [
        { name: "five_hour", used: pct / 100, resetsAt: t + resetsIn },
        { name: "seven_day", used: 0.3, resetsAt: t + 4 * 86_400 },
      ];
      putQuota(w.disk[LOCAL], [
        { account: "work", seenAt: t - 300, status: "rejected", refused: true, limiting: "five_hour", resetsAt: t + 2 * 3600, windows: win(100, 2 * 3600) },
        { account: "personal", seenAt: t - 300, status: "allowed", limiting: "five_hour", windows: win(35, 3 * 3600) },
        { account: "team", seenAt: t - 300, status: "allowed", limiting: "five_hour", windows: win(12, 4 * 3600) },
        { account: "lab", seenAt: t - 300, status: "allowed", limiting: "five_hour", windows: win(50, 1.5 * 3600) },
      ]);
      return w;
    },
    960,
    620,
  ),
];
