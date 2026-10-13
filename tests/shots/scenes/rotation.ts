/**
 * 轮换第五版：会话面板的四块（平时 · 点开一个号 · 24 小时条上刷一段 · 临时条目 · 停发 · 策略出错 · 不轮换）· 拖号牌改默认优先级 ·
 * 设置 → 机器 → 轮换。
 *
 * 账号库 · 额度账 · 轮换账本（第三版）· 策略文件落成本机家目录里的原始文件，`rotation-strategies` · `rotation-view` 全由真后端答
 * （真跑那份策略、真推演）。策略取 `rotation-weight.rhai`（照设计稿「我们的轮换」那份的写法，号名换成合成的）。
 * 号名 work · team · lab · personal · api 与会话都是合成的；时刻按此刻的钟现算。
 */
import type { Scene } from "./index";
import { putAccounts, putQuota, putRotation, putStrategy, type QuotaSpec, type RotationSessionSpec } from "../disk";
import type { World } from "../fake/types";
import { defaultWorld, LOCAL, sidOf } from "../fake/world";
import { emit } from "@tauri-apps/api/event";
import { byText, click, mainReady, openTab, sleep, waitFor } from "./helpers";
import WEIGHT from "./rotation-weight.rhai?raw";

const M = 60;
const now = (): number => Math.floor(Date.now() / 1000);
const NAME = "按权重轮换";

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
}

function rotWorld(build: (rw: RotWorld) => void = () => {}): () => World {
  return () => {
    const w = defaultWorld();
    const t = now();
    const rw: RotWorld = {
      quota: normal(),
      sessions: { 0: { start: "work", current: "team", strategy: NAME, log: [{ at: t - 83 * M, from: "work", to: "team", cause: { k: "order" }, why: "权重最高" }] } },
      strategies: { [NAME]: WEIGHT },
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
    putRotation(d, { forNew: rw.forNew, sessions });
    for (const [id, src] of Object.entries(rw.strategies)) putStrategy(d, id, src);
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

async function card(account: string): Promise<HTMLElement> {
  return waitFor(`${PANEL} button[data-account="${account}"]`);
}

/** 点一张号牌（指针按下 ＋ 松开，没拖开 ⇒ 展开）。 */
async function openCard(account: string): Promise<void> {
  const c = await card(account);
  c.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0 }));
  document.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0 }));
  await waitFor(`${PANEL} div[data-account="${account}"]`);
  await sleep(400);
}

/** 号牌此刻的先后。 */
const order = (): string[] => [...document.querySelectorAll<HTMLElement>(`${PANEL} button[data-account]`)].map((e) => e.dataset.account ?? "");

let dragFrom: string[] = [];

export const ROTATION_SCENES: Scene[] = [
  panelScene("rot-panel", "面板 · 平时", "按权重轮换（照「我们的轮换」写法的策略）：第一行下拉 ＋ 说明 · 号牌（在用 team 带绿点，work 到上限变淡）· 总体设置照策略声明的 scope: all 那几格 · 模拟照真推演", openPanel),
  panelScene("rot-open", "面板 · 点开一个号", "点号牌 lab：头一行此刻用量 · 重置 · 立即用 · 移出；下面是声明里 scope: each 的格（用量上限另设了 5h 分段，7d 跟总体画虚线 · 耗尽后启用 · 启用延迟 · 单次用量上限）", async () => {
    await openPanel();
    await openCard("lab");
  }),
  panelScene("rot-day", "面板 · 24 小时条上刷一段", "team 的 5h 条上点一下：点中的那一段整段弹出 `00:00–24:00 · 上限 [  ] %` · 0% · 50% · 100% · 删这段", async () => {
    await openPanel();
    await openCard("team");
    const el = await waitFor(`${PANEL} div[data-account="team"] [data-day-bar]`);
    const r = el.getBoundingClientRect();
    el.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: r.left + 2 }));
    el.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: r.left + 2 }));
    await sleep(700);
  }),
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
    "四个订阅号都被拒、api 还在启用延迟里：顶上琥珀一条照后端那一句（几点谁恢复 · 还有多久）；模拟里同一段画成「停下」",
    openPanel,
    rotWorld((rw) => {
      rw.quota = [ledger("work", [100, 97], [60, 3 * 1440], true), ledger("team", [100, 127], [40, 4 * 1440], true), ledger("lab", [100, 47], [30, 3 * 1440], true), ledger("personal", [100, 200], [100, 2 * 1440], true)];
    }),
  ),
  panelScene(
    "rot-error",
    "面板 · 策略出错",
    "夜里省着用（第 3 行除数为 0）：红条照后端那一句（第几行 · 原话 · 改按默认优先级），这一判照号牌先后",
    openPanel,
    rotWorld((rw) => {
      rw.strategies["夜里省着用"] = `// 夜里只用 lab 和 team（示意：decide 里有一处写错了）\n\n/*--- rotation\n{ "format": 5, "tz": "UTC", "settings": { "accounts": [{ "id": "team" }, { "id": "lab" }] }, "form": [] }\n---*/\n\nfn decide(ctx) {\n  let x = 1 / 0;\n  #{ order: ["lab"] }\n}\n`;
      rw.sessions[0] = { start: "work", current: "team", strategy: "夜里省着用" };
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
    ...panelScene("rot-drag", "面板 · 拖号牌改默认优先级", "真鼠标拖第 1 张号牌（work）松在第 3 张（lab）上 ⇒ 整份设置交后端，回来的号牌排成 team · work · lab · …", async () => {
      await openPanel();
      dragFrom = order();
    }),
    drag: () => {
      const want = [...dragFrom];
      const [moved] = want.splice(0, 1);
      want.splice(1, 0, moved);
      return {
        from: document.querySelector<HTMLElement>(`${PANEL} button[data-account="${dragFrom[0]}"]`)!,
        to: document.querySelector<HTMLElement>(`${PANEL} button[data-account="${dragFrom[2]}"]`)!,
        check: async () => {
          for (let i = 0; i < 40; i++) {
            if (order().join() === want.join()) return null;
            await sleep(150);
          }
          return `号牌没排成 ${want.join(" · ")}：此刻 ${order().join(" · ")}`;
        },
      };
    },
  },
  {
    id: "rot-settings",
    page: "settings",
    dir: "轮换",
    title: "设置 · 机器 · 轮换",
    desc: "本机的「轮换」栏：第一行下拉选中的就是新会话用的，右边写「设置 · 本机」；下面同一个四块（没有会话 ⇒ 没有「立即用」与拖时间轴）",
    width: 1280,
    height: 1000,
    world: rotWorld(),
    act: async () => {
      await waitFor(".settings-nav");
      await sleep(1500);
      await emit("settings-target", JSON.stringify({ machine: "<local>", tab: "rot" }));
      await waitFor("[data-rot-section] button[data-account]", 10_000).catch(() => null);
      await byText("[data-rot-section] span", /设置 ·/).catch(() => null);
      await sleep(1200);
    },
  },
];
