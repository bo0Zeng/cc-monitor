/**
 * 主窗口上的面板与浮层：子 agent / 任务 / 账号 / 命令面板 / 会话内查找 / 大纲 / 监控板 / 历史 / 右键菜单 / 提示。
 */
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { defaultWorld } from "../fake/world";
import { byText, click, key, mainReady, openTab, rightClick, sleep, type, waitFor } from "./helpers";

const ALL_TABS = 7;

function panel(id: string, title: string, desc: string, act: Scene["act"], world: () => World = defaultWorld): Scene {
  return { id, page: "index", dir: "主窗口-面板", title, desc, width: 1280, height: 800, world, act };
}

async function scrollStreamTop(): Promise<void> {
  const box = document.querySelector<HTMLElement>("#message-stream");
  for (const e of box ? [box, ...box.querySelectorAll<HTMLElement>("*")] : []) if (e.scrollHeight > e.clientHeight + 50) e.scrollTop = 0;
  await sleep(500);
}

async function openCommandBar(): Promise<void> {
  await mainReady(ALL_TABS);
  await click(".status-cmdk");
  await waitFor(".command-bar-input");
}

/** 分叉要先问的那种：源会话不在跑、那台也说不出它上次用的哪个号。 */
function forkAskWorld(): World {
  const w = defaultWorld();
  w.ops["history-last-accounts"] = () => ({ accounts: {} });
  w.ops["accounts-sessions"] = () => ({ lines: [] });
  return w;
}

/** gpu-01 连不上，而它那个会话是固定着的 ⇒ 复活出来「说不清」。 */
function unseenPinnedWorld(): World {
  const w = defaultWorld();
  w.unseenMachines = ["gpu-01"];
  const s = w.sessions[5];
  w.config = {
    ...w.config,
    tabBar: { pinned: [{ sid: s.sid, origin: "gpu-01", cwd: s.cwd, jsonlPath: `/home/user/.claude/projects/x/${s.sid}.jsonl`, title: "排序模型训练脚本" }] },
  };
  return w;
}

function closedWorld(): World {
  const w = defaultWorld();
  w.closedMachines = ["devbox"];
  return w;
}

/** 活卡：第一个会话正在出一轮新的应答（思考 → 正文 → 调工具），还没收尾。 */
async function streamLiveReply(ctx: Parameters<Scene["act"]>[0]): Promise<void> {
  const sid = ctx.backend.world.sessions[0].sid;
  let n = 0;
  const ev = (e: unknown): unknown => ({ origin: "<local>", stream: sid, resp: 1, n: n++, ev: e });
  ctx.backend.pushTap("<local>", [
    ev({ t: "start", rid: "req_live0001" }),
    ev({ t: "block", i: 0, kind: "thinking" }),
    ev({ t: "text", i: 0, s: "文档里的配置表要和 settings 里的默认值对得上，先读一下 settings.py 再写。" }),
    ev({ t: "block", i: 1, kind: "text" }),
    ev({ t: "text", i: 1, s: "好，我来更新 `docs/config.md`：把三个新配置项加进表里，并写一段" }),
    ev({ t: "text", i: 1, s: "「什么时候该调大重试次数」的说明。" }),
    ev({ t: "block", i: 2, kind: "tool", tool: "Read" }),
  ]);
  await sleep(900);
}

export const PANEL_SCENES: Scene[] = [
  panel("panel-live-card", "活卡：正在出的一轮应答", "当前会话正在流式出一轮新的应答（思考、正文、调工具），还没写进记录", async (ctx) => {
    await mainReady(ALL_TABS);
    await streamLiveReply(ctx);
  }),
  panel("panel-agent-run-open", "子 agent 面板 · 点开一个", "子 agent 面板里点开第一行（在跑的那一个排在前面）：它的时间线", async () => {
    await mainReady(ALL_TABS);
    await click(".status-agents");
    await waitFor(".agents-popover .agent-row");
    await click(".agents-popover .agent-row");
    await sleep(1200);
  }),
  panel("panel-fork-ask", "从这一轮分叉", "已结束的会话里，用户那句话上的分叉按钮：源会话不在跑、说不清用哪个账号 ⇒ 先问", async () => {
    await mainReady(ALL_TABS);
    await openTab(2);
    await scrollStreamTop();
    const btn = await waitFor(".viewer-branch-btn");
    await click(btn);
    await waitFor(".fork-ask");
    await sleep(700);
  }, forkAskWorld),
  panel("panel-kill-confirm", "杀死会话前的确认", "tab 右键「杀死会话」：确认框", async () => {
    await mainReady(ALL_TABS);
    await rightClick("#tab-bar .tab");
    await sleep(800);
    await click(await byText(".tab-context-menu-item", "杀死会话"));
    await waitFor("[role='dialog']");
    await sleep(400);
  }),
  panel("panel-close-undo", "W 关掉已结束的 tab 之后", "切到已结束的「周报草稿」按 W：tab 先摘下来，右下角给 8 秒「撤销」", async () => {
    await mainReady(ALL_TABS);
    await openTab(2);
    document.querySelector<HTMLElement>("#message-stream")?.focus();
    (document.activeElement as HTMLElement | null)?.blur();
    await key("w");
    await sleep(600);
  }),
  panel("panel-unseen-menu", "说不清的 tab 的右键菜单", "gpu-01 连不上、它那个固定着的会话说不清：右键菜单里恢复置灰、说为什么", async () => {
    await mainReady(ALL_TABS);
    const t = [...document.querySelectorAll<HTMLElement>("#tab-bar .tab")].find((x) => (x.textContent ?? "").includes("排序模型"));
    await rightClick(t ?? "#tab-bar .tab");
    await sleep(900);
  }, unseenPinnedWorld),
  {
    ...panel("panel-batch-kill-short", "批量杀会话的确认框 · 矮窗口", "全选之后右键「杀死会话」，窗口只有 220 高：清单在框里滚、按钮够得着，焦点在「取消」", async () => {
      await mainReady(ALL_TABS);
      const tabs = [...document.querySelectorAll<HTMLElement>("#tab-bar .tab")];
      for (const t of tabs) {
        const r = t.getBoundingClientRect();
        t.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true, clientX: r.left + 10, clientY: r.top + 5 }));
        await sleep(60);
      }
      await rightClick(tabs[0]);
      await sleep(600);
      await click(await byText(".tab-context-menu-item", "杀死会话"));
      await waitFor("[role='dialog']");
      await sleep(400);
    }),
    height: 220,
  },
  panel("panel-pane-preview", "预览终端画面", "远端会话的 tab 右键「预览画面」：那个 tmux 窗口此刻的样子", async () => {
    await mainReady(ALL_TABS);
    await rightClick(document.querySelectorAll("#tab-bar .tab")[3]);
    await sleep(800);
    await click(await byText(".tab-context-menu-item", "预览画面"));
    await waitFor(".pane-preview-box");
    await sleep(700);
  }),
  panel("panel-agents", "子 agent 面板", "状态栏点「2 agents」：本会话的子 agent 列表（一个跑完、一个在跑）", async () => {
    await mainReady(ALL_TABS);
    await click(".status-agents");
    await waitFor(".agents-popover");
    await sleep(500);
  }),
  panel("panel-tasks", "任务面板", "状态栏点「4 tasks」：本会话的任务清单", async () => {
    await mainReady(ALL_TABS);
    await click(".status-tasks:not(.status-agents)");
    await waitFor(".tasks-popover:not(.agents-popover)");
    await sleep(500);
  }),
  panel("panel-account", "账号选单", "状态栏点账号徽标：切默认账号的选单", async () => {
    await mainReady(ALL_TABS);
    await click(".status-account");
    await waitFor(".account-picker");
    await sleep(500);
  }),
  panel("panel-cmdk", "命令面板", "Ctrl+K：全部命令（含切到各会话）", async () => {
    await openCommandBar();
    await sleep(300);
  }),
  panel("panel-cmdk-filter", "命令面板 · 过滤", "命令面板里输入「设置」", async () => {
    await openCommandBar();
    await type(".command-bar-input", "设置");
  }),
  panel("panel-cmdk-empty", "命令面板 · 没有匹配", "命令面板里输入一串匹配不到的字", async () => {
    await openCommandBar();
    await type(".command-bar-input", "zzzzqqq");
  }),
  panel("panel-find", "会话内查找", "Ctrl+F：在当前会话里找「重试」", async () => {
    await mainReady(ALL_TABS);
    await key("f", { ctrl: true });
    await waitFor(".session-find-input");
    await type(".session-find-input", "重试");
    await key("Enter");
    await sleep(800);
  }),
  panel("panel-outline", "大纲", "右上「大纲」：本会话里用户说过的每一句", async () => {
    await mainReady(ALL_TABS);
    await click(".user-inputs-toggle");
    await sleep(800);
  }),
  panel("panel-grid", "监控板", "右上田字格：所有会话一屏看", async () => {
    await mainReady(ALL_TABS);
    await click(".grid-monitor-trigger");
    await waitFor(".grid-monitor");
    await sleep(600);
  }),
  panel("panel-grid-peek", "监控板 · 选中一格", "监控板里点开第一格：右侧看那个会话的模型、碰过的文件、子 agent", async () => {
    await mainReady(ALL_TABS);
    await click(".grid-monitor-trigger");
    await waitFor(".grid-monitor");
    await sleep(400);
    const cell = document.querySelector<HTMLElement>(".grid-monitor-body [class*='cell']");
    if (cell) await click(cell);
    await sleep(600);
  }),
  panel("panel-history", "历史", "右上时钟：按项目列出本机与远端的历史会话", async () => {
    await mainReady(ALL_TABS);
    await click(".history-trigger");
    await waitFor(".history-view");
    await sleep(1200);
  }),
  panel("panel-cc-bus", "cc-bus 驾驶舱", "命令面板里打开「cc-bus 驾驶舱」：各台机器上 claude 实例之间的消息", async () => {
    await openCommandBar();
    await type(".command-bar-input", "cc-bus");
    const item = await byText(".command-bar-item", /cc-bus/);
    await click(item);
    await sleep(1200);
  }),
  panel("panel-tab-menu", "tab 右键菜单 · 本机会话", "在第一个 tab 上点右键", async () => {
    await mainReady(ALL_TABS);
    await rightClick("#tab-bar .tab");
    await sleep(900);
  }),
  panel("panel-tab-menu-remote", "tab 右键菜单 · 远端会话", "在 devbox 那个 tab 上点右键", async () => {
    await mainReady(ALL_TABS);
    await rightClick(document.querySelectorAll("#tab-bar .tab")[3]);
    await sleep(900);
  }),
  panel("panel-tab-menu-ended", "tab 右键菜单 · 已结束的会话", "在已结束那个 tab 上点右键", async () => {
    await mainReady(ALL_TABS);
    await rightClick(document.querySelectorAll("#tab-bar .tab")[2]);
    await sleep(900);
  }),
  panel("panel-batch-menu", "多选 tab 的右键菜单", "按住 Ctrl 选中三个 tab，再在其中一个上点右键：对选中的那几个一起做", async () => {
    await mainReady(ALL_TABS);
    const tabs = document.querySelectorAll<HTMLElement>("#tab-bar .tab");
    await click(tabs[0]);
    for (const i of [1, 3]) {
      const r = tabs[i].getBoundingClientRect();
      tabs[i].dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true, clientX: r.left + 30, clientY: r.top + 5 }));
      await sleep(150);
    }
    await rightClick(tabs[3]);
    await sleep(900);
  }),
  panel("panel-sftp-picker", "文件按钮 · 选机器", "右上文件夹：有多台机器时先选去哪台", async () => {
    await mainReady(ALL_TABS);
    await click(".sftp-trigger");
    await sleep(600);
  }),
  panel("panel-stream-closed", "会话流断了的提示", "devbox 的会话流被那台后端关掉：右下角报错提示", async () => {
    await mainReady(ALL_TABS - 2);
    await sleep(800);
  }, closedWorld),
  panel("panel-first-run", "首次打开", "第一次开：命令面板入口高亮、tab 栏默认宽度", async () => {
    await mainReady(ALL_TABS);
  }),
];

// 首次打开那一张不带默认存储（tab 栏默认宽、命令面板提示没看过）
PANEL_SCENES[PANEL_SCENES.length - 1].storage = {};
