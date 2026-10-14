/**
 * 会话隐藏（设计稿 `会话隐藏.md` §4.3 桌面那几样）：栏底「已隐藏 N」收着 / 展开 · 窄栏 · 右键「隐藏」与撤销 toast ·
 * 「已隐藏」里的行右键「取消隐藏」· 会话头「已隐藏」「由 X 启动」「子会话 N」· 设置机器页两行 · 行为页没有「显示分身会话」。
 *
 * 隐藏那一格由后端判：会话宣告里那一格要等后端那一路进来，场景在开页之后照推帧主题「会话隐藏」推（`changed/session_hidden`，
 * 格体 `{key: 会话, body: {by, said}}`）。`said` 是后端写的那一句，这里是台架编的占位。血缘两格经 `world.factsLineage` 并进会话事实。
 */
import type { Scene } from "./index";
import type { SceneCtx, World } from "../fake/types";
import { Convo } from "../fake/records";
import { defaultWorld, LOCAL, session } from "../fake/world";
import { byText, click, mainReady, rightClick, sleep, waitFor } from "./helpers";
import { copyText } from "../../../src/frontend/ui/copy-table";

const W = 1280;
const H = 760;

interface Row {
  title: string;
  cwd: string;
  activity: "working" | "idle" | "needs_you";
  background?: boolean;
}

const ROWS: Row[] = [
  { title: "订单服务加重试", cwd: "/home/user/work/orders", activity: "working" },
  { title: "部署文档", cwd: "/home/user/work/docs", activity: "idle" },
  { title: "表格虚拟滚动", cwd: "/home/user/work/web-console", activity: "idle" },
  { title: "命令行补全", cwd: "/home/user/work/cli", activity: "working" },
  // 下面三个隐藏：子会话（在等批准）· 子会话 · 分身会话。
  { title: "帮手：跑一遍测试", cwd: "/home/user/work/orders", activity: "needs_you" },
  { title: "帮手：查日志", cwd: "/home/user/work/orders", activity: "idle" },
  { title: "文档索引", cwd: "/home/user/work/docs", activity: "working", background: true },
];
const N = ROWS.length;
const sidAt = (i: number): string => `7ab1${(i + 1).toString(16).padStart(4, "0")}-0000-4000-8000-00000000${(i + 1).toString(16).padStart(4, "0")}`;
/** 台架编的「后端那一句」（真句子由后端拼，界面照抄）。 */
const SAID = { child: "台架占位 · 子会话默认隐藏", bg: "台架占位 · 分身会话默认隐藏", self: "台架占位 · 已隐藏" };
const HIDDEN: [number, keyof typeof SAID][] = [
  [4, "child"],
  [5, "child"],
  [6, "bg"],
];

function hideWorld(): World {
  const w = defaultWorld();
  w.sessions = ROWS.map((r, i) => {
    const sid = sidAt(i);
    const c = new Convo(sid, r.cwd, "2026-10-14T09:00:00Z");
    c.title(r.title);
    c.user(`${r.title}：开始。`);
    c.say("好的，先看一下现状。", 20_000);
    c.endAt(Date.now() - 60_000);
    return session(200 + i, LOCAL, r.cwd, c, {
      sid,
      activity: r.activity,
      background: r.background === true,
      ...(r.activity === "needs_you" ? { waitingFor: "permission prompt", waitingSinceMs: Date.now() - 90_000 } : {}),
    });
  });
  // 两个帮手都是「订单服务加重试」起的。
  w.factsLineage = {
    [sidAt(0)]: { startedBy: null, children: [4, 5].map((i) => ({ sid: sidAt(i), title: ROWS[i].title, live: true, hidden: { by: "child", said: SAID.child } })) },
    [sidAt(4)]: { startedBy: { sid: sidAt(0), title: ROWS[0].title }, children: [] },
    [sidAt(5)]: { startedBy: { sid: sidAt(0), title: ROWS[0].title }, children: [] },
  };
  // 右键「隐藏」交的那一条（注解住本机后端；台架答一份写完的注解）。
  w.ops["history-annotate"] = () => ({ entry: { starred: false, customTitle: null, hidden: false, updatedAt: 0 } });
  return w;
}

/** 推帧主题「会话隐藏」：这几个会话那一格（`null` ＝ 不隐藏了）。 */
function pushHidden(ctx: SceneCtx, cells: [number, keyof typeof SAID | null][]): void {
  for (const [i, by] of cells) ctx.backend.pushChanged(LOCAL, "session_hidden", { key: sidAt(i), body: by === null ? null : { by, said: SAID[by] } });
}

/** 开页、等标签页都到了、推那三格、等栏挪好。 */
async function ready(ctx: SceneCtx): Promise<void> {
  await mainReady(N);
  pushHidden(ctx, HIDDEN);
  await waitFor(".tab-hidden");
  await sleep(500);
}

function tabOf(title: string): HTMLElement {
  for (const el of document.querySelectorAll<HTMLElement>("#tab-bar .tab")) if (el.textContent?.includes(title)) return el;
  throw new Error(`没有标签页「${title}」`);
}

async function openRow(): Promise<void> {
  await click(".tab-hidden .tab-group-head");
  await sleep(400);
}

function hs(id: string, title: string, desc: string, act: Scene["act"], opts: { world?: () => World; size?: [number, number]; storage?: Record<string, string> } = {}): Scene {
  const [width, height] = opts.size ?? [W, H];
  return { id, page: "index", dir: "会话隐藏", title, desc, width, height, world: opts.world ?? hideWorld, act, ...(opts.storage ? { storage: opts.storage } : {}) };
}

function settingsScene(id: string, title: string, desc: string, act: () => Promise<void>): Scene {
  return { id, page: "settings", dir: "会话隐藏", title, desc, width: 960, height: 740, world: hideWorld, act };
}

async function go(...ids: string[]): Promise<void> {
  await waitFor(".settings-nav");
  await sleep(600);
  for (const id of ids) {
    await click(`#${CSS.escape(`settings-tab-${id}`)}`);
    await sleep(700);
  }
  await sleep(600);
}

export const HIDDEN_SCENES: Scene[] = [
  hs("hide-00-row", "栏底「已隐藏 3」（收着）", "7 个会话里 3 个隐藏（两个子会话、一个分身）：散的标签页之后一行，收着时汇总需手动那颗点；「需手动 1」照样数隐藏的那个", async (ctx) => {
    await ready(ctx);
  }),
  hs("hide-01-row-open", "「已隐藏」展开", "点那一行：列活着的隐藏会话，行与标签页同形、缩进一格", async (ctx) => {
    await ready(ctx);
    await openRow();
  }),
  hs("hide-02-menu-hide", "右键「隐藏」", "右键「部署文档」：固定 · 分组之后一项「隐藏」", async (ctx) => {
    await ready(ctx);
    await rightClick(tabOf("部署文档"));
    await sleep(400);
  }),
  hs("hide-03-toast", "隐藏之后的撤销 toast", "点了「隐藏」：toast「已隐藏「部署文档」」［撤销］；推帧来了它挪进「已隐藏」（4）", async (ctx) => {
    await ready(ctx);
    await rightClick(tabOf("部署文档"));
    await sleep(300);
    (await byText("[role^=menuitem]", /^隐藏/)).click();
    await sleep(300);
    pushHidden(ctx, [[1, "self"]]);
    await sleep(600);
  }),
  hs("hide-04-menu-unhide", "「已隐藏」里右键「取消隐藏」", "展开「已隐藏」、右键「文档索引」（分身会话）：只有「取消隐藏」，不固定、不分组", async (ctx) => {
    await ready(ctx);
    await openRow();
    await rightClick(tabOf("文档索引"));
    await sleep(400);
  }),
  hs("hide-05-head-child", "子会话的会话头", "切到「帮手：跑一遍测试」：会话头「已隐藏」「由 订单服务加重试 启动」；「已隐藏」那一行自己展开、选中它", async (ctx) => {
    await ready(ctx);
    await openRow();
    tabOf("帮手：跑一遍测试").click();
    await sleep(900);
  }),
  hs("hide-06-head-children", "主会话的「子会话 2」", "「订单服务加重试」的会话头点「子会话 2」：列两个子会话（状态点 · 名字 · 已隐藏）", async (ctx) => {
    await ready(ctx);
    tabOf("订单服务加重试").click();
    await sleep(600);
    await click("#session-head [data-head=children]");
    await sleep(400);
  }),
  hs(
    "hide-07-folded",
    "窄栏：「隐藏」图标 ＋ 计数",
    "栏收成 44px：「已隐藏」那一行只剩图标与 3，点开是同一列的菜单",
    async (ctx) => {
      await ready(ctx);
      await click(".tab-hidden .tab-group-head");
      await sleep(400);
    },
    { size: [1100, H], storage: { "cc-monitor.tab-bar-w": "260", "cc-monitor.cmdk-hint.seen": "1", "cc-monitor.tab-bar.folded": "1" } },
  ),
  settingsScene("hide-08-settings-machine", "设置 · 本机 · 子会话 · 分身会话", "「该机的 cc-monitor」里怎么起会话那几行：子会话 ［显示｜隐藏］· 分身会话 ［显示｜隐藏］（名字短，说明在悬停）", async () => {
    await go("machine:（本机）");
    await click(await byText(".settings-page:not([hidden]) button", copyText("machinePage.cc.title")));
    await sleep(800);
    document.querySelector<HTMLElement>(".settings-page:not([hidden]) [data-role=child-tab]")?.scrollIntoView({ block: "center" });
    await sleep(400);
  }),
  settingsScene("hide-09-settings-behavior", "设置 · 通用 · 行为（没有「显示分身会话」）", "行为那一组：自动跟随 · 同时提到前台 · 两个通知；「显示分身会话」整条删了（分身进不进标签栏改在机器页）", async () => {
    await go("general");
  }),
];
