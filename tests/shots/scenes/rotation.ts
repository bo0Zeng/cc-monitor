/**
 * 轮换第三版（稿 `调研/设计稿/额度与账号/轮换-v3.md`，截图 `轮换-v3-截图/`）：会话面板（平时 · 卡住 · 策略出错 · 备用先等 · 手动调整 ·
 * 打开副本 · 号详情 · 参数…）与设置 → 机器 → 轮换（配置列表 · 配置编辑器 · 策略列表 · 试跑）。
 *
 * 账号库 · 额度账 · 轮换账本（第二版）· 本机策略目录落成本机家目录里的原始文件，轮换那一组命令全由真后端答
 * （`rotation-configs-read` · `-strategies-read` · `-session-*` · `-plan` · `-strategy-try`）；只有热切换那一下要中转，场景替那台答。
 * 号名 work · personal · team · lab · api 与会话都是合成的；时刻按此刻的钟现算。
 */
import type { Scene } from "./index";
import { putAccounts, putQuota, putRotation, putStrategy, type AcctRowSpec, type ConfigSpec, type QuotaSpec, type RotationSessionSpec } from "../disk";
import type { World } from "../fake/types";
import { defaultWorld, LOCAL, sidOf } from "../fake/world";
import { emit } from "@tauri-apps/api/event";
import { byText, click, mainReady, openTab, rightClick, sleep, waitFor } from "./helpers";

const M = 60;
const now = (): number => Math.floor(Date.now() / 1000);

/** 账号库：四个订阅号 ＋ 一个按量号。 */
const ACCTS = ["work", "personal", "team", "lab"];

/** 一个号在额度账上那一条（中转从回包头记下的原始几格）。`5h` / `7d` ＝ 用了几成 ＋ 几分钟后重置；不给 ⇒ 那一窗没开。 */
function ledger(account: string, w5?: [number, number], w7?: [number, number], refused = false): QuotaSpec {
  const t = now();
  const win = (name: string, x?: [number, number]) => (x ? [{ name, used: x[0] / 100, resetsAt: t + x[1] * M }] : []);
  return {
    account,
    seenAt: t - 60,
    status: refused ? "rejected" : "allowed",
    refused,
    ...(w5 ? { limiting: "five_hour" } : w7 ? { limiting: "seven_day" } : {}),
    ...(refused && w5 ? { resetsAt: t + w5[1] * M } : {}),
    windows: [...win("five_hour", w5), ...win("seven_day", w7)],
  };
}

/** 稿里平时那一刻：work 5h 94%（线 90，97 分后重置）· personal 没开窗 · team 58% · lab 35% · api 按量。 */
function normalQuota(): QuotaSpec[] {
  return [ledger("work", [94, 97], [60, 3 * 24 * 60]), ledger("personal", undefined, [20, 2 * 24 * 60]), ledger("team", [58, 127], [30, 4 * 24 * 60]), ledger("lab", [35, 197], [30, 3 * 24 * 60])];
}

const P2 = { grab_min: 30, grab_left: 30, skip: 2, tired_7d: 10, delay: 0, all_out: "stop" };
type Who = { start: true } | { rest: true } | { id: string };
const row = (who: Who, p: Record<string, unknown> = {}): AcctRowSpec => ({ ...who, params: { line: 90, slots: [], mode: "normal", ...p } });

/** 默认配置 2（内置策略 2）：起会话的号 ＋ 其余订阅号 · team 到 99% · lab 到 99%、17:00–02:00 到 80% · api 备用。 */
function cfg2(over: Partial<ConfigSpec> = {}): ConfigSpec {
  return {
    name: "默认配置 2",
    strategy: "2",
    accounts: [row({ start: true }), row({ rest: true }), row({ id: "team" }, { line: 99 }), row({ id: "lab" }, { line: 99, slots: [{ at: "17:00-02:00", line: 80 }] }), row({ id: "api" }, { mode: "standby" })],
    params: P2,
    acctInit: { line: 90, slots: [], mode: "normal" },
    rev: 3,
    updatedAt: now() - 86_400,
    ...over,
  };
}

const P1 = { back: false, delay: 0, standby_wait: 40, all_out: "stop" };
const row1 = (who: Who, p: Record<string, unknown> = {}): AcctRowSpec => ({ ...who, params: { line: 90, slots: [], mode: "normal", stint: null, ...p } });

/** 日常（内置策略 1）：按写下的先后 · 4 个号 · 到 90% · lab 20:00–23:00 不用 · 都到上限就停。 */
function daily(over: Partial<ConfigSpec> = {}): ConfigSpec {
  return {
    name: "日常",
    strategy: "1",
    accounts: [row1({ id: "team" }), row1({ start: true }), row1({ id: "lab" }, { slots: [{ at: "20:00-23:00", line: 0 }] }), row1({ id: "personal" })],
    params: P1,
    acctInit: { line: 90, slots: [], mode: "normal", stint: null },
    rev: 2,
    updatedAt: now() - 2 * 86_400,
    ...over,
  };
}

/** 一份外加的策略：`meta` · `texts` · `describe` 都对，`priority` 第 12 行引用了没定义的变量（判一次就出错 ⇒ 这一判照内置策略 1 排）。 */
const NIGHT = `// 夜里只用 lab 和 team（示意：priority 里有一处写错了）
fn meta() { #{ format: 1, name: "夜里省着用", ordered: true, params: [], acct_params: [ #{ k: "line", type: "line", init: 90, label: "p_line" } ] } }
fn texts() {
  #{ zh: #{ p_line: "用到多少换", d: "夜里只用 lab 和 team" }, en: #{ p_line: "Switch at", d: "Night: only lab and team" } }
}
fn describe(params, accounts) { ["d"] }

fn priority(ctx) {
  let out = #{};
  for a in ctx.accounts {
    let u = lib::usable(a, ctx);
    out[a.id] = #{ p: cap, why: "d" };
  }
  out
}
`;

interface RotWorld {
  quota: QuotaSpec[];
  configs: Record<string, ConfigSpec>;
  defaultConfig: string;
  /** 按 tab 序号（0 起）。 */
  sessions: Record<number, Omit<RotationSessionSpec, "since"> & { since?: number }>;
  strategies?: Record<string, string>;
}

function rotWorld(build: (rw: RotWorld) => void = () => {}): () => World {
  return () => {
    const w = defaultWorld();
    const t = now();
    const rw: RotWorld = {
      quota: normalQuota(),
      configs: { c_00000001: daily({ name: "默认配置 1", accounts: [row1({ start: true }, { line: null })], params: { ...P1, all_out: "go" }, acctInit: { line: null, slots: [], mode: "normal", stint: null } }), c_00000002: cfg2(), c_00000003: { ...cfg2({ name: "默认配置 3", strategy: "3", params: { soon_min: 60, soon_left: 40, near_left: 10, near_min: 20, delay: 0, all_out: "stop" } }), accounts: [row({ start: true }), row({ rest: true })].map((r) => ({ ...r, params: { line: 90, slots: [] } })), acctInit: { line: 90, slots: [] } }, c_daily: daily() },
      defaultConfig: "c_00000002",
      sessions: {
        0: {
          start: "work",
          current: "team",
          config: { id: "c_00000002", rev: 3 },
          history: [{ at: t - 83 * M, from: "work", to: "team", leave: { k: "unusable", why: { k: "@line", w: "5h", n: 90, at: t + 97 * M } } }],
        },
        3: { start: "team", current: "team", config: { id: "c_00000002", rev: 3 } },
      },
    };
    build(rw);
    const d = w.disk[LOCAL];
    putAccounts(d, [...ACCTS.map((name, i) => ({ name, kind: "sub" as const, isDefault: i === 0 })), { name: "api", kind: "api" as const }]);
    putQuota(d, rw.quota);
    const sessions: Record<string, RotationSessionSpec> = {};
    for (const [k, s] of Object.entries(rw.sessions)) {
      const sid = w.sessions[Number(k)]?.sid ?? sidOf(Number(k) + 1);
      sessions[sid] = { ...s, since: s.since ?? t - 83 * M };
    }
    putRotation(d, { defaultConfig: rw.defaultConfig, configs: rw.configs, sessions });
    for (const [id, src] of Object.entries(rw.strategies ?? {})) putStrategy(d, id, src);
    // 热切换要这台的中转正经手那个会话的请求（台架里没有）⇒ 场景替那台答成了。
    w.ops["rotation-switch"] = (_o, req) => ({ sessions: Object.fromEntries((req.sessions as string[]).map((sid) => [sid, { state: "done" }])) });
    return w;
  };
}

function panelScene(id: string, title: string, desc: string, act: Scene["act"], world: () => World = rotWorld(), size: [number, number] = [1280, 900]): Scene {
  return { id, page: "index", dir: "轮换", title, desc, width: size[0], height: size[1], world, act };
}

function settingsScene(id: string, title: string, desc: string, act: Scene["act"], world: () => World = rotWorld(), size: [number, number] = [1280, 980]): Scene {
  return { id, page: "settings", dir: "轮换", title, desc, width: size[0], height: size[1], world, act };
}

const PANEL = 'aside[role="dialog"]';

async function openPanel(): Promise<void> {
  await mainReady(7);
  await openTab(0);
  await sleep(600);
  await click(".status-account");
  await waitFor(`${PANEL} [data-rot-list]`);
  // 合图那一问（rotation-plan）回来、策略表回来再截。
  await waitFor(`${PANEL} [data-rc-bar], ${PANEL} [data-rc-seg]`, 8000).catch(() => null);
  await sleep(900);
}

async function rowEl(account: string): Promise<HTMLElement> {
  return waitFor(`${PANEL} [data-rc-row="${account}"]`);
}

async function goRotation(anchor?: string): Promise<void> {
  await waitFor(".settings-nav");
  await sleep(1500);
  await emit("settings-target", JSON.stringify({ machine: "<local>", tab: "rot", ...(anchor ? { anchor } : {}) }));
  await waitFor("[data-rot-section] [data-rot-config], [data-rot-section] [data-rot-strategy]", 10_000);
  await sleep(1800);
}

const dirtyCopy = (): ConfigSpec => {
  const c = cfg2();
  return { ...c, accounts: c.accounts.map((r) => ("id" in r && r.id === "lab" ? { ...r, params: { ...r.params, line: 95 } } : r)) };
};

export const ROTATION_SCENES: Scene[] = [
  // ── 会话面板（图 03 · 04 · 05）
  panelScene("rot-panel", "面板 · 平时：一张合图", "默认配置 2（内置策略 2）：列表就是有效优先级，第一行在用；行下一句是策略给的为什么；此刻不挑的沉到下面、整行淡", openPanel),
  panelScene("rot-panel-hover", "面板 · 悬停将来那个换号点 · 选中一行", "悬停本会话那行的 ◇：时刻（估）· 换到谁 · 为什么；单击选中 lab", async () => {
    await openPanel();
    (await rowEl("lab")).click();
    await sleep(300);
    const d = document.querySelectorAll<HTMLElement>(`${PANEL} [data-rc-switch][data-est]`)[0];
    d?.dispatchEvent(new MouseEvent("mouseenter"));
    d?.focus();
    await sleep(800);
  }),
  panelScene(
    "rot-stuck",
    "面板 · 内置策略 1 · 都到上限 · 已停发",
    "日常：team · lab 到线 · work 被拒 · personal 7d 用满；没号可挑就停发，卡住条写最早几点谁恢复",
    openPanel,
    rotWorld((rw) => {
      rw.quota = [ledger("team", [92, 127], [40, 4 * 1440]), ledger("work", [100, 97], [60, 3 * 1440], true), ledger("lab", [93, 227], [30, 3 * 1440]), ledger("personal", [10, 200], [100, 2 * 1440], true)];
      rw.sessions[0] = { start: "work", current: "team", config: { id: "c_daily", rev: 2 }, history: [] };
    }),
  ),
  panelScene(
    "rot-strategy-error",
    "面板 · 外加的策略出错",
    "夜里省着用（本机策略，第 12 行引用了没定义的变量）：顶上红条照说，这一判照内置策略 1 排",
    openPanel,
    rotWorld((rw) => {
      rw.strategies = { night: NIGHT };
      rw.configs.c_night = { name: "夜里省着用", strategy: "night", accounts: [{ id: "team", params: { line: 90 } }, { id: "lab", params: { line: 90 } }], params: {}, acctInit: { line: 90 }, rev: 1, updatedAt: now() - 86_400 };
      rw.sessions[0] = { start: "work", current: "team", config: { id: "c_night", rev: 1 }, history: [] };
    }),
  ),
  panelScene(
    "rot-standby-wait",
    "面板 · 用备用号前先等",
    "日常 ＋ api 用法备用：team 到线、work 被拒 27 分后恢复 ⇒ 先等 work，备用号此刻不挑",
    openPanel,
    rotWorld((rw) => {
      rw.quota = [ledger("team", [91, 127], [40, 4 * 1440]), ledger("work", [100, 27], [60, 3 * 1440], true)];
      rw.configs.c_daily = daily({ accounts: [row1({ id: "team" }), row1({ start: true }), row1({ id: "api" }, { mode: "standby" })] });
      rw.sessions[0] = { start: "work", current: "team", config: { id: "c_daily", rev: 2 }, history: [] };
    }),
  ),
  panelScene(
    "rot-manual",
    "面板 · 手动调整之后（24h 视窗）",
    "personal 被拖到第一行（已热换过去，不出任何提示）· lab 拖到第二行；列表头 刷新策略 清掉手动调整",
    async () => {
      await openPanel();
      (await byText(`${PANEL} [data-rc-view] button`, "24h")).click();
      await sleep(1500);
    },
    rotWorld((rw) => {
      const t = now();
      rw.quota = [ledger("work", [94, 97], [60, 3 * 1440]), ledger("personal", [6, 290], [20, 2 * 1440]), ledger("team", [58, 127], [30, 4 * 1440]), ledger("lab", [35, 197], [30, 3 * 1440])];
      rw.sessions[0] = {
        start: "work",
        current: "personal",
        config: { id: "c_00000002", rev: 3 },
        manual: { hot: { account: "personal", since: t - 13 * M }, place: [{ account: "lab", at: 1 }] },
        history: [
          { at: t - 83 * M, from: "work", to: "team", leave: { k: "top" } },
          { at: t - 13 * M, from: "team", to: "personal", leave: { k: "manual" } },
        ],
      };
    }),
  ),
  panelScene("rot-row-menu", "面板 · 右键一行", "换到这个号 · 用法（这份策略声明了才出）· 不参与 · 号详情…", async () => {
    await openPanel();
    await rightClick(await rowEl("lab"));
    await sleep(500);
  }),
  panelScene("rot-bulk", "面板 · 多选几个号一起改", "Ctrl 点选 team · lab ⇒ 已选 2 个号：每个每号参数一格（空着 ＝ 不改）＋ 全部改成", async () => {
    await openPanel();
    (await rowEl("team")).click();
    await sleep(200);
    (await rowEl("lab")).dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true }));
    await sleep(600);
    document.querySelector(`${PANEL} [data-rot-bulk]`)?.scrollIntoView({ block: "center" });
    await sleep(300);
  }),
  panelScene("rot-switch", "面板 · 切换…", "列表头 切换… 开一张浮层：选号 · 热切换 / 重启切换", async () => {
    await openPanel();
    await click(`${PANEL} [data-rot-switch]`);
    await sleep(700);
  }),

  // ── 加载 ＝ 打开一份副本（图 06）
  panelScene("rot-dropdown", "面板 · 配置下拉", "各份配置（名 · N 会话 · 本机默认）· 分隔 · 另存为… · 管理配置…", async () => {
    await openPanel();
    await click(`${PANEL} [data-rot-cfg]`);
    await sleep(700);
  }),
  panelScene(
    "rot-dirty",
    "面板 · 改了 lab 的线以后",
    "副本改过：配置名后 已改，旁边 保存 · 另存为… · 还原；描述照副本写",
    openPanel,
    rotWorld((rw) => {
      rw.sessions[0] = { ...rw.sessions[0], config: { id: "c_00000002", rev: 3, copy: dirtyCopy() } };
    }),
  ),
  panelScene(
    "rot-ask",
    "面板 · 有没存的改动时换到别的配置",
    "先问 保存 · 丢掉 · 取消，列出本会话改了哪几格",
    async () => {
      await openPanel();
      await click(`${PANEL} [data-rot-cfg]`);
      await sleep(500);
      await click(await byText('[role="menu"] [role^="menuitem"]', /^日常/));
      await waitFor('[role="dialog"][aria-modal="true"]', 5000);
      await sleep(600);
    },
    rotWorld((rw) => {
      rw.sessions[0] = { ...rw.sessions[0], config: { id: "c_00000002", rev: 3, copy: dirtyCopy() } };
    }),
  ),
  panelScene(
    "rot-newer",
    "面板 · 别处存了新版、本会话改过",
    "头下一条 有新版本 · 几点在哪存的 · 本会话改过，没跟 ＋ 载入新版本",
    openPanel,
    rotWorld((rw) => {
      rw.configs.c_00000002 = cfg2({ rev: 4, updatedAt: now() - 20 * M, params: { ...P2, grab_left: 45 } });
      rw.sessions[0] = { ...rw.sessions[0], config: { id: "c_00000002", rev: 3, copy: dirtyCopy() } };
    }),
  ),

  // ── 号详情 · 参数…（图 07）
  panelScene(
    "rot-detail",
    "号详情 · lab（内置策略 2）· 改过线",
    "双击一行：此刻（排第几与为什么 · 换到它 · 计量条）＋ 每号参数（照策略声明生成；改过的格一道琥珀线、写原值）",
    async () => {
      await openPanel();
      (await rowEl("lab")).dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
      await waitFor(`${PANEL} [data-rot-detail]`);
      await sleep(600);
    },
    rotWorld((rw) => {
      rw.sessions[0] = { ...rw.sessions[0], config: { id: "c_00000002", rev: 3, copy: dirtyCopy() } };
    }),
  ),
  panelScene(
    "rot-detail-api",
    "号详情 · api（内置策略 1 · 按量号）",
    "按量号没有窗口：用到多少换 · 按时段另算两格不出；用法 备用",
    async () => {
      await openPanel();
      (await rowEl("api")).dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
      await waitFor(`${PANEL} [data-rot-detail]`);
      await sleep(600);
    },
    rotWorld((rw) => {
      rw.configs.c_daily = daily({ accounts: [row1({ id: "team" }), row1({ start: true }), row1({ id: "lab" }), row1({ id: "personal" }), row1({ id: "api" }, { mode: "standby" })] });
      rw.sessions[0] = { start: "work", current: "team", config: { id: "c_daily", rev: 2 }, history: [] };
    }),
  ),
  panelScene("rot-params", "参数…（本会话副本的整份参数）", "策略下拉 · 看说明与源码 › · 整份参数（照声明）· 新加的号缺省", async () => {
    await openPanel();
    await click(`${PANEL} [data-rot-params]`);
    await waitFor(`${PANEL} [data-rot-params-view]`);
    await sleep(600);
  }),

  // ── 设置 → 机器 → 轮换（图 08 · 09）
  settingsScene("rot-settings-configs", "设置 · 轮换 · 配置列表 ＋ 配置编辑器", "同一个文档做法：号表一行一个号、列是这份策略的每号参数；整份参数照声明；底下预览同一张合图", async () => {
    await goRotation("config:c_00000002");
  }),
  settingsScene("rot-settings-bulk", "设置 · 配置编辑器 · 多选 · 改一行", "点行首勾选 team · lab ⇒ 已选 2 个号 ＋ 全部改成；点一行在下面展开那一行的表单", async () => {
    await goRotation("config:c_00000002");
    for (const id of ["team", "lab"]) document.querySelector<HTMLInputElement>(`[data-rot-row="${id}"] input[type=checkbox]`)?.click();
    await sleep(300);
    document.querySelector<HTMLElement>('[data-rot-row="lab"]')?.click();
    await sleep(800);
  }),
  settingsScene(
    "rot-settings-strategies",
    "设置 · 轮换 · 策略列表 · 试跑",
    "顶上一行 lib · 共用小函数（引擎）；内置三份 ＋ 本机一份（读不懂的照实写错在第几行）；详情 · 试跑（不写盘不换号）· 源码",
    async () => {
      await goRotation("strategy:3");
      await click("[data-rot-try]");
      await waitFor("[data-rot-tried]", 8000).catch(() => null);
      await sleep(700);
    },
    rotWorld((rw) => {
      rw.strategies = { night: NIGHT };
    }),
  ),
  settingsScene("rot-settings-lib", "设置 · 轮换 · 共用小函数 lib", "引擎给策略的小函数库：只读，看源码", async () => {
    await goRotation("strategy:2");
    await click('[data-rot-strategy="@lib"]');
    await sleep(700);
  }),
];
