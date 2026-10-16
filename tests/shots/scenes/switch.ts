/**
 * 会话头上换模型 · 切权限档那两颗：能换 · 两个菜单 · 正在换 · 换成了 · 没换成 · 不能换（在跑 · 需手动）· 已退出（也能换）· 窄窗。
 * 字（`modelText` · `permissionModeText`）· 能不能换（会话帧 `switch`）· 菜单里那一家的清单（`session-new-facts.agents`）都是真后端算的；
 * 换的那两条命令（`session-model` · `session-permission`）台架里没有真的那一家可送，场景只演回包的结局那一形（读回的字取那一家自己写的原样）。
 */
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { defaultWorld } from "../fake/world";
import { click, hover, mainReady, openTab, sleep, until, waitFor } from "./helpers";

const W = 1280;
const H = 800;
const ALL_TABS = 7;

function sw(id: string, title: string, desc: string, act: Scene["act"], world: () => World = idleWorld, size: [number, number] = [W, H]): Scene {
  return { id, page: "index", dir: "主窗口-中途切换", title, desc, width: size[0], height: size[1], world, act };
}

/** 第一个会话闲着（能换两样）；其余同默认世界。 */
function idleWorld(): World {
  const w = defaultWorld();
  w.sessions[0] = { ...w.sessions[0], activity: "idle" };
  return w;
}

/** 闲着 ＋ 换的那一下那台的回包（`reply` 不给 ⇒ 一直不回：停在「正在换」）。 */
function replying(reply?: (req: Record<string, unknown>) => unknown): () => World {
  return () => {
    const w = idleWorld();
    const answer = (_o: string, req: Record<string, unknown>) => (reply ? reply(req) : new Promise(() => {}));
    w.ops["session-model"] = answer;
    w.ops["session-permission"] = answer;
    return w;
  };
}

const chip = (k: "model" | "permission") => `#session-head [data-switch="${k}"]`;

/** 两颗都有字了（会话事实到了、清单问到了）。 */
async function chipsReady(): Promise<void> {
  await waitFor(chip("model"));
  await until(() => (document.querySelector(chip("model"))?.textContent !== copyText("session.model.title") ? true : null));
  await sleep(500);
}

async function openMenuOf(k: "model" | "permission"): Promise<void> {
  await click(chip(k));
  await waitFor('[role="menu"]');
  await sleep(300);
}

async function pickFirstOther(): Promise<void> {
  const items = [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')];
  const it = items.find((i) => i.getAttribute("aria-checked") !== "true" && !(i as HTMLButtonElement).disabled);
  if (!it) throw new Error("菜单里没有可选的另一项");
  it.click();
  await sleep(400);
}

export const SWITCH_SCENES: Scene[] = [
  sw("main-switch-idle", "会话头 · 模型 · 权限档（能换）", "闲着的会话：两颗写会话事实的 modelText · permissionModeText（没有 ⇒ 写这颗是什么）；悬停模型那一颗", async () => {
    await mainReady(ALL_TABS);
    await chipsReady();
    await hover(chip("model"));
    await sleep(700);
  }),
  sw("main-switch-model-menu", "会话头 · 换模型的菜单", "点模型那一颗：组名 ·「默认」在前 · 那一家的模型 · 底下那一行（都照那台 session-new-facts）", async () => {
    await mainReady(ALL_TABS);
    await chipsReady();
    await openMenuOf("model");
  }),
  sw("main-switch-permission-menu", "会话头 · 切权限档的菜单", "点权限档那一颗：那一家可切的档，此刻那一档打勾", async () => {
    await mainReady(ALL_TABS);
    await chipsReady();
    await openMenuOf("permission");
  }),
  sw(
    "main-switch-pending",
    "会话头 · 正在换",
    "选了一项、那台还没回：两颗都不可点，换的那颗写选的那一项、原位转圈",
    async () => {
      await mainReady(ALL_TABS);
      await chipsReady();
      await openMenuOf("model");
      await pickFirstOther();
    },
    replying(),
  ),
  sw(
    "main-switch-done",
    "会话头 · 换成了",
    "那台读回 done：模型那一颗写读回的那一句，不出 toast",
    async () => {
      await mainReady(ALL_TABS);
      await chipsReady();
      await openMenuOf("model");
      await pickFirstOther();
      await sleep(600);
    },
    replying(() => ({ result: "done", model: "Sonnet 4.5" })),
  ),
  sw(
    "main-switch-unsure",
    "会话头 · 没换成",
    "那台回 unsure 又没带那一句：回原字 ＋ toast「换权限档结果未知」，灰字写哪个会话",
    async () => {
      await mainReady(ALL_TABS);
      await chipsReady();
      await openMenuOf("permission");
      await pickFirstOther();
      await sleep(600);
    },
    replying(() => ({ result: "unsure", mode: null, modeText: null })),
  ),
  sw(
    "main-switch-running",
    "会话头 · 这一轮在跑",
    "在跑的会话：模型那一颗灰（悬停写那台 switch.model.why），权限档照常能换",
    async () => {
      await mainReady(ALL_TABS);
      await chipsReady();
      await hover(chip("model"));
      await sleep(700);
    },
    defaultWorld,
  ),
  sw(
    "main-switch-needs",
    "会话头 · 需手动",
    "在等批准的会话：两颗都灰，悬停写那台那一句",
    async () => {
      await mainReady(ALL_TABS);
      await openTab(1);
      await waitFor(chip("permission"));
      await sleep(800);
      await hover(chip("permission"));
      await sleep(700);
    },
    defaultWorld,
  ),
  sw(
    "main-switch-ended",
    "会话头 · 已结束的会话",
    "已结束的会话：两颗都亮着（session_state 带的 switch；选了记下、恢复照它起）；［恢复 ▾］照旧在右边",
    async () => {
      await mainReady(ALL_TABS);
      await openTab(2);
      await waitFor(chip("model"));
      await sleep(800);
      await hover(chip("model"));
      await sleep(700);
    },
    defaultWorld,
  ),
  sw(
    "main-switch-narrow",
    "窄窗 · 会话头的两颗",
    "窗口 900×640：两颗不缩、不收进「⋯」，标题先让",
    async () => {
      await mainReady(ALL_TABS);
      await chipsReady();
    },
    idleWorld,
    [900, 640],
  ),
];
