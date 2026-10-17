/**
 * 轮换第五版：会话面板（平时 · 点一行看这个号 · 临时条目 · 先不用一个号 · 停发 · 策略出错 · 策略改过了 · 不轮换 · 拖一行改本会话先后）·
 * 设置 → 机器 → 轮换（号牌 · 点号牌出的编辑小窗）。
 *
 * 账号库 · 额度账 · 轮换账本（第三版）· 策略文件落成本机家目录里的原始文件，`rotation-strategies` · `rotation-view` 全由真后端答
 * （真跑那份策略、真推演）。策略是照编辑器那一份建的（随后端带的 `editor.rhai`，换上合成的号列表与设置）。
 * 号名 work · team · lab · personal · api 与会话都是合成的；时刻按此刻的钟现算。
 */
import type { Scene } from "./index";
import { putAccounts, putOldScript, putQuota, putRotation, putStrategy, type QuotaSpec, type RotationSessionSpec } from "../disk";
import type { World } from "../fake/types";
import { defaultWorld, LOCAL, sidOf } from "../fake/world";
import { emit } from "@tauri-apps/api/event";
import { byText, click, mainReady, openTab, sleep, waitFor } from "./helpers";
import EDITOR from "../../../src/shared/rotation/editor.rhai?raw";

const M = 60;
const now = (): number => Math.floor(Date.now() / 1000);
const NAME = "高并发";

/** 照编辑器那一份建的策略：头换成这份设置（时区 UTC）。 */
function edited(settings: unknown): string {
  return EDITOR.replace('"format": 5,', '"format": 5,\n  "tz": "UTC",').replace('"settings": { "accounts": [], "smart": true }', `"settings": ${JSON.stringify(settings)}`);
}

/** 高并发：lab 强制切入；team 20:00–23:00 不用；api 延迟切入 ＋ 强制切出（其余合计 400%）。 */
const SETTINGS = {
  accounts: [
    { id: "work" },
    { id: "team", caps: { "5h": [{ from: 0, to: 1200, v: 90 }, { from: 1200, to: 1380, v: 0 }, { from: 1380, to: 1440, v: 90 }] } },
    { id: "lab", in: "force" },
    { id: "personal" },
    { id: "api", in: "delay", wait: 0, out: "force", outTotal: 400 },
  ],
  smart: true,
};

/** 一个号在额度账上那一条。`5h` / `7d` ＝ 用了几成 ＋ 几分钟后重置；不给 ⇒ 那一窗没开。 */
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

/** 平时：work 5h 94%（97 分后重置）· team 58% · lab 35% · personal 没开窗 · api 按量。 */
const normal = (): QuotaSpec[] => [ledger("work", [94, 97], [40, 3 * 1440]), ledger("team", [58, 127], [30, 4 * 1440]), ledger("lab", [35, 197], [20, 3 * 1440]), ledger("personal", undefined, [20, 2 * 1440])];

interface RotWorld {
  quota: QuotaSpec[];
  /** 按 tab 序号（0 起）。 */
  sessions: Record<number, Omit<RotationSessionSpec, "since"> & { since?: number }>;
  strategies: Record<string, string>;
  forNew: string;
  /** `strategies/v3/` 里没加载的旧格式（文件名 ⇒ 原文）。 */
  old?: Record<string, string>;
}

function rotWorld(build: (rw: RotWorld) => void = () => {}): () => World {
  return () => {
    const w = defaultWorld();
    const t = now();
    const rw: RotWorld = {
      quota: normal(),
      sessions: { 0: { start: "work", current: "team", strategy: NAME, log: [{ at: t - 83 * M, from: "work", to: "team", cause: { k: "order" }, why: "号牌顺序" }] } },
      strategies: { [NAME]: edited(SETTINGS) },
      forNew: NAME,
    };
    build(rw);
    const d = w.disk[LOCAL];
    putAccounts(d, [...["work", "team", "lab", "personal"].map((name, i) => ({ name, kind: "sub" as const, isDefault: i === 0 })), { name: "api", kind: "api" as const }]);
    putQuota(d, rw.quota);
    const sessions: Record<string, RotationSessionSpec> = {};
    for (const [k, s] of Object.entries(rw.sessions)) {
      const sid = w.sessions[Number(k)]?.sid ?? sidOf(Number(k) + 1);
      sessions[sid] = { ...s, since: s.since ?? t - 83 * M };
    }
    putRotation(d, { forNew: rw.forNew, sessions, strategies: rw.strategies });
    for (const [id, src] of Object.entries(rw.strategies)) putStrategy(d, id, src);
    for (const [f, src] of Object.entries(rw.old ?? {})) putOldScript(d, f, src);
    return w;
  };
}

function panelScene(id: string, title: string, desc: string, act: Scene["act"], world: () => World = rotWorld(), size: [number, number] = [1280, 1000]): Scene {
  return { id, page: "index", dir: "轮换", title, desc, width: size[0], height: size[1], world, act };
}

const PANEL = 'aside[role="dialog"]';

async function openPanel(): Promise<void> {
  await mainReady(7);
  await openTab(0);
  await sleep(600);
  await click(".status-account");
  await waitFor(`${PANEL} [data-account], ${PANEL} [data-rot-hint]`, 10_000).catch(() => null);
  await sleep(900);
}

const SECTION = "[data-rot-section]";

/** 点一下（指针按下 ＋ 松开，没拖开）：会话顺序的一行 ⇒ 小窗；设置页的号牌 ⇒ 展开。 */
async function tap(sel: string, then: string): Promise<void> {
  const c = await waitFor(sel);
  c.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0 }));
  document.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0 }));
  await waitFor(then);
  await sleep(400);
}

/** 会话顺序此刻的先后。 */
const order = (): string[] => [...document.querySelectorAll<HTMLElement>(`${PANEL} [data-rot-row]`)].map((e) => e.dataset.account ?? "");

/** 设置窗跳到本机「轮换」栏。 */
async function openSettingsRot(): Promise<void> {
  await waitFor(".settings-nav");
  await sleep(1500);
  await emit("settings-target", JSON.stringify({ machine: "<local>", tab: "rot" }));
  await waitFor(`${SECTION} button[data-account]`, 10_000).catch(() => null);
  await byText(`${SECTION} span`, /设置 ·/).catch(() => null);
  await sleep(1200);
}

let dragFrom: string[] = [];


/** 出错那份策略的代码：头之后第 7 行起，第 12 行除以 0。 */
const NIGHT_BROKEN = `fn decide(ctx) {
  let night = ctx.clock >= 22 * 60 || ctx.clock < 7 * 60;
  let order = if night { ["lab", "team"] } else { ["team", "lab"] };
  let spare = ctx.accounts.len();
  let share = 100;
  let x = share / (spare - spare);
  #{ order: order }
}
`;
export const ROTATION_SCENES: Scene[] = [
  panelScene("rot-panel", "面板 · 平时", "高并发（照编辑器建的那一份）：第一行下拉 ＋ 编辑策略 · 说明 · 顺序（第一格在用的 team，每行一个勾 · 用量 · 号名旁的记号 · 此刻怎样，用不了的沉底）· 模拟照真推演", openPanel),
  panelScene("rot-open", "面板 · 点一行看这个号", "点顺序里 lab 那一行：小窗贴着那一行，只看 —— 此刻怎样 · 用量与重置 · 它在这份策略里的规则 · 立即用 lab · 编辑策略", async () => {
    await openPanel();
    await tap(`${PANEL} [data-rot-row][data-account="lab"]`, "[data-rot-pop]");
  }),
  panelScene(
    "rot-off",
    "面板 · 本会话先不用一个号",
    "personal 那一行的勾去掉：顺序里沉到最下、模拟里那一行整行「不用」；策略不变",
    openPanel,
    rotWorld((rw) => {
      rw.sessions[0] = { ...rw.sessions[0], off: ["personal"] };
    }),
  ),
  {
    id: "rot-edit",
    page: "settings",
    dir: "轮换",
    title: "设置 · 点一个号：在这里改",
    desc: "设置页点号牌 team：编辑小窗贴着号牌出来 —— 用量上限一段一行（20:00–23:00 · 0%）、上面那条只画；7 天［同 5 小时 · 另设］；切入 · 切出；底行「改的是策略「高并发」· 改了就存成新版」·［移出这份策略］",
    width: 1280,
    height: 1000,
    world: rotWorld(),
    act: async () => {
      await openSettingsRot();
      await tap(`${SECTION} button[data-account="team"]`, `${SECTION} [data-rot-editor]`);
    },
  },
  panelScene(
    "rot-temp",
    "面板 · 拖出来的临时条目",
    "会话拖过「接下来 lab → personal」：最上那条拖出来的段白描边加圆点，轮到时用不了的写「跳过」；标题右边 手动 N 段 · 清掉手动顺序",
    openPanel,
    rotWorld((rw) => {
      rw.sessions[0] = { ...rw.sessions[0], temp: ["team", "lab", "personal"] };
    }),
  ),
  panelScene(
    "rot-hold",
    "面板 · 一个号都用不了 · 已停发",
    "四个订阅号都被拒：顶上琥珀一条照后端那一句（几点谁恢复 · 还有多久）；模拟里同一段画成「停下」",
    openPanel,
    rotWorld((rw) => {
      rw.quota = [ledger("work", [100, 97], [60, 3 * 1440], true), ledger("team", [100, 127], [40, 4 * 1440], true), ledger("lab", [100, 47], [30, 3 * 1440], true), ledger("personal", [100, 200], [100, 2 * 1440], true)];
    }),
  ),
  panelScene(
    "rot-error",
    "面板 · 策略出错",
    "夜里省着用（第 12 行除数为 0）：红条照后端那一句（第几行 · 哪一种错 · 改按默认优先级，Rhai 原话进［复制详情］），这一判照号牌先后",
    openPanel,
    rotWorld((rw) => {
      rw.strategies["夜里省着用"] = `// 夜里只用 lab 和 team（示意：decide 里有一处写错了）\n\n/*--- rotation\n{ "format": 5, "tz": "UTC", "settings": { "accounts": [{ "id": "team" }, { "id": "lab" }] }, "form": [] }\n---*/\n\n${NIGHT_BROKEN}`;
      rw.sessions[0] = { start: "work", current: "team", strategy: "夜里省着用" };
    }),
  ),
  panelScene(
    "rot-past",
    "面板 · 时间轴过去那一截",
    "往前 2 小时照会话的换号记录与读数画成实心：最上那条 team → work（停发 10 分钟）→ team；work 那一行从被换下那一刻起发不出去、lab 从开窗那一刻起写 ↻；此刻一条竖线，往后浅色虚边",
    openPanel,
    rotWorld((rw) => {
      const t = now();
      rw.quota = [ledger("work", [100, 97], [40, 3 * 1440]), ledger("team", [58, 127], [30, 4 * 1440]), ledger("lab", [35, 197], [20, 3 * 1440]), ledger("personal", undefined, [20, 2 * 1440])];
      rw.sessions[0] = {
        start: "work",
        current: "team",
        strategy: NAME,
        since: t - 70 * M,
        log: [
          { at: t - 100 * M, from: "team", to: "work", cause: { k: "order" } },
          { at: t - 80 * M, from: "work", to: "work", cause: { k: "hold", until: t - 70 * M } },
          { at: t - 70 * M, from: "work", to: "team", cause: { k: "unusable" } },
        ],
      };
    }),
  ),
  {
    ...panelScene(
      "rot-hints",
      "面板 · 提示带按钮",
      "策略出错（第 12 行，除数为 0；原话进［复制详情］）⇒［看代码］（照「在文件窗口打开」那一路开那份文件）· 时区不明 ⇒［设时区］（悬停写设成哪一个）· 旧格式没加载 ⇒［看是哪几份］，点开列出 strategies/v3/ 里的文件名",
      async () => {
        await openPanel();
        const which = await byText(`${PANEL} button`, /看是哪几份/);
        which.click();
        await sleep(600);
      },
      rotWorld((rw) => {
        rw.strategies["夜里省着用"] = `// 夜里只用 lab 和 team（示意：decide 里有一处写错了，头里也没写时区）\n\n/*--- rotation\n{ "format": 5, "settings": { "accounts": [{ "id": "team" }, { "id": "lab" }] }, "form": [] }\n---*/\n\n${NIGHT_BROKEN}`;
        rw.sessions[0] = { start: "work", current: "team", strategy: "夜里省着用" };
        rw.old = { "旧的按顺序.rhai": "fn score(ctx) { 0 }\n", "旧的夜里.rhai": "fn score(ctx) { 1 }\n" };
      }),
    ),
  },
  panelScene(
    "rot-newer",
    "面板 · 策略改过了",
    "设置页里改过这份策略（存成新版）、这个会话还在用它载入的那一版：蓝条「策略「高并发」改过了 · 这个会话还在用旧版」［载入新版］",
    openPanel,
    rotWorld((rw) => {
      rw.sessions[0] = {
        ...rw.sessions[0],
        stale: {
          rev: "h:000000000000",
          settings: { ...SETTINGS, accounts: SETTINGS.accounts.map((a) => (a.id === "api" ? { ...a, wait: 30 } : a)) },
        },
      };
    }),
  ),
  panelScene(
    "rot-none",
    "面板 · 不轮换",
    "会话没选策略：第一行下拉写「不轮换」，下面一行「只用开会话的号 work」，号牌 · 设置 · 模拟都不出",
    openPanel,
    rotWorld((rw) => {
      rw.sessions[0] = { start: "work", current: "work" };
    }),
  ),
  {
    ...panelScene("rot-drag", "面板 · 拖一行改本会话先后", "真鼠标拖顺序第 3 行松在第 2 行上 ⇒ 拖完的那一串交后端（本会话先后），回来的顺序照它排、模拟照它推演", async () => {
      await openPanel();
      dragFrom = order();
    }),
    drag: () => {
      const want = [...dragFrom];
      const [moved] = want.splice(2, 1);
      want.splice(1, 0, moved);
      return {
        from: document.querySelector<HTMLElement>(`${PANEL} [data-rot-row][data-account="${dragFrom[2]}"]`)!,
        to: document.querySelector<HTMLElement>(`${PANEL} [data-rot-row][data-account="${dragFrom[1]}"]`)!,
        check: async () => {
          for (let i = 0; i < 40; i++) {
            if (order().join() === want.join()) return null;
            await sleep(150);
          }
          return `顺序没排成 ${want.join(" · ")}：此刻 ${order().join(" · ")}`;
        },
      };
    },
  },
  {
    id: "rot-settings",
    page: "settings",
    dir: "轮换",
    title: "设置 · 机器 · 轮换",
    desc: "本机的「轮换」栏：第一行下拉 ＋ ⋯ ＋「新会话用这份」，右边写「设置 · 本机」；号牌（lab ↺ · api 虚线框 ⇥）· 总体只一个「智能切换」· 模拟",
    width: 1280,
    height: 1000,
    world: rotWorld(),
    act: openSettingsRot,
  },
];
