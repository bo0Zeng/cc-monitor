/**
 * 主窗口状态栏（这个会话的几枚 · 竖线 · 命令）与底部抽屉（任务 · agent · 终端）。
 */
import type { Scene } from "./index";
import { Refuse, type World } from "../fake/types";
import { defaultWorld } from "../fake/world";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { emit } from "@tauri-apps/api/event";
import { Convo } from "../fake/records";
import { click, mainReady, openTab, waitFor, sleep } from "./helpers";

const ALL_TABS = 7;

function scene(id: string, title: string, desc: string, act: Scene["act"], world: () => World = defaultWorld): Scene {
  return { id, page: "index", dir: "主窗口-状态栏与抽屉", title, desc, width: 1280, height: 800, world, act };
}

/** gpu-01 上那个会话 Claude 已退出、tmux 还在，最后一轮报错中断了。 */
function exitedErrorWorld(): World {
  const w = defaultWorld();
  const s = w.sessions[5];
  const c = new Convo(s.sid, s.cwd, "2026-10-01T07:00:00Z");
  c.title("排序模型训练脚本");
  c.user("训练脚本加断点续训。");
  c.say("我先看一下现在的训练循环。");
  c.apiError(529, "API Error: 529 {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}");
  s.records = c.records;
  return w;
}

/** 第一个会话（本机）活着、但不在 tmux 里（直接敲 cc 起的）。 */
function outsideWorld(): World {
  const w = defaultWorld();
  w.sessions[0].container = { form: "none" };
  return w;
}

/** 终端实时画面那一路停了：推一屏之后那台断开。 */
function liveOfflineWorld(): World {
  const w = defaultWorld();
  w.terminalLive = "offline";
  return w;
}

/** 那台只能快照：订实时被拒（tmux 低于 3.2）。 */
function snapOnlyWorld(): World {
  const w = defaultWorld();
  w.ops["terminal-follow"] = () => {
    throw new Refuse("tmux_too_old", "tmux 3.1c 不支持实时画面 · 需要 3.2 以上");
  };
  return w;
}

/** 抽屉开在终端页、拉高到 460（看得全那一屏的颜色）。 */
const TALL_DRAWER = { "cc-monitor.tab-bar-w": "260", "cc-monitor.cmdk-hint.seen": "1", "cc-monitor.bottom-drawer": JSON.stringify({ page: "terminal", height: 460 }) };

function emptyWorld(): World {
  const w = defaultWorld();
  w.sessions = [];
  return w;
}

export const DRAWER_SCENES: Scene[] = [
  scene("status-session", "状态栏 · 有当前会话", "右边：任务 · agent · 上下文 · 账号（本会话）· 竖线 · 命令", async () => {
    await mainReady(ALL_TABS);
    await sleep(800);
  }),
  scene("status-no-session", "状态栏 · 没有会话", "这个会话的几枚都不渲染；账号退成「默认 …」排到竖线后", async () => {
    await waitFor("#status-bar .status-cmdk");
    await sleep(1500);
  }, emptyWorld),
  scene("status-ctx-open", "上下文 chip 点开", "百分比 · 进度条 · 最新一轮 / 上限 · 模型 · 上限来源与［改上限…］", async () => {
    await mainReady(ALL_TABS);
    await sleep(600);
    const chips = [...document.querySelectorAll<HTMLElement>("#status-bar button")];
    const ctx = chips.find((b) => b.textContent?.startsWith("上下文"));
    if (!ctx) throw new Error("没有上下文 chip");
    await click(ctx);
    await waitFor("[role=dialog][aria-label='上下文用量']");
    await sleep(300);
  }),
  scene("drawer-tasks", "底部抽屉 · 任务", "点状态栏「任务 2/4」：抽屉在主区底部、状态栏之上，把消息流往上推；页签就是 chip 的字", async () => {
    await mainReady(ALL_TABS);
    await click(".status-tasks");
    await waitFor("#bottom-drawer:not([hidden]) .tasks-item");
    await sleep(500);
  }),
  scene("drawer-agents", "底部抽屉 · agent", "开着时点「agent 2 · 1 在跑」：换页不关；页里照今天的 agent 列表", async () => {
    await mainReady(ALL_TABS);
    await click(".status-tasks");
    await waitFor("#bottom-drawer:not([hidden])");
    await click(".status-agents");
    await waitFor("#bottom-drawer .agent-row");
    await sleep(500);
  }),
  scene("drawer-agents-opened", "底部抽屉 · agent · 点过的行标「窗口已开」", "点一行开了它自己的窗口：那一行状态字左边多一个「窗口已开」，派出它的那张卡同一个位置也标上", async () => {
    await mainReady(ALL_TABS);
    await click(".status-agents");
    await waitFor("#bottom-drawer .agent-row");
    await emit("agent-window", { sessionId: defaultWorld().sessions[0].sid, run: "agent-a2", open: true });
    await waitFor('#bottom-drawer [data-role="window-open"]');
    await sleep(500);
  }),
  scene("drawer-empty", "底部抽屉 · 切到没有任务的会话", "抽屉开着切标签页：照开，页里写空态（不藏），页签退成页名", async () => {
    await mainReady(ALL_TABS);
    await click(".status-tasks");
    await waitFor("#bottom-drawer:not([hidden])");
    await openTab(1);
    await sleep(800);
  }),
  scene("drawer-terminal", "底部抽屉 · 终端", "会话头「看它的终端」：抽屉开到终端页 —— 页头（会话 · 目录 · 机器 · 连着几个终端窗口 · 画面几点）· 画面 · 送往哪 · 输入框 · 常用键", async () => {
    await mainReady(ALL_TABS);
    await click("#session-head button[aria-label='看它的终端']");
    await waitFor("#bottom-drawer pre");
    await sleep(800);
  }),
  scene(
    "drawer-terminal-failed",
    "底部抽屉 · 终端 · 画面读不出",
    "那台说「无运行中的 tmux」：终端页顶一条错误条 ＝ 那台写好的一句（原话不上句子）＋［刷新］＋［复制详情］（详情是那台后端写好、monitor 转交的那几行）",
    async () => {
      await mainReady(ALL_TABS);
      await click("#session-head button[aria-label='看它的终端']");
      await waitFor('#bottom-drawer [data-part="copy-detail"]');
      // 剪贴板写不进（截图里没有剪贴板）⇒ 点［复制详情］就地展开那一段：看得见复制出去的是什么。
      Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: () => Promise.reject(new Error("denied")) } });
      document.querySelector<HTMLButtonElement>('#bottom-drawer [data-part="copy-detail"] button')?.click();
      await sleep(500);
    },
    () => {
      const w = defaultWorld();
      w.ops["terminal-preview"] = () => {
        throw new Refuse("no_server", copyText("terminalReads.preview.noServer"), undefined, "no server running on /tmp/tmux-1000/default");
      };
      return w;
    },
  ),
  scene("drawer-terminal-bg", "底部抽屉 · 终端 · 后台", "没有终端窗口连着的那个会话：写「后台 · 输入直达」", async () => {
    await mainReady(ALL_TABS);
    await openTab(1);
    await click("#session-head button[aria-label='看它的终端']");
    await waitFor("#bottom-drawer pre");
    const box = document.querySelector<HTMLTextAreaElement>("#bottom-drawer textarea");
    if (box) {
      box.value = "1";
      box.dispatchEvent(new Event("input"));
    }
    await sleep(800);
  }),
  scene("drawer-terminal-ended", "底部抽屉 · 终端 · 已结束的会话", "抽屉开在终端页时切到已结束的会话：页里写「已结束 · 无终端」", async () => {
    await mainReady(ALL_TABS);
    await click("#session-head button[aria-label='看它的终端']");
    await waitFor("#bottom-drawer pre");
    await openTab(2);
    await sleep(900);
  }),
  scene("drawer-terminal-outside", "底部抽屉 · 终端 · 不在 tmux 里", "活着、不在 tmux 里的会话（直接敲 cc 起的）：照实写「不在 tmux 里」，不说「非 cc-monitor 启动」（这台不是 Windows ⇒ 没有［切到终端］）", async () => {
    await mainReady(ALL_TABS);
    await click("#session-head button[aria-label='看它的终端']");
    await waitFor("#bottom-drawer [class*=empty]");
    await sleep(800);
  }, outsideWorld),
  {
    ...scene("drawer-terminal-live-tall", "底部抽屉 · 终端 · 实时（拉高）", "抽屉拉高到 460：头上「● 实时」（没有「画面几点 ＋ 重新看」）；画面带颜色 —— Bash 输出、路径蓝、「Building…」那一行橙、「accept edits」品红", async () => {
      await mainReady(ALL_TABS);
      await waitFor("#bottom-drawer pre span");
      await sleep(900);
    }),
    height: 1000,
    storage: TALL_DRAWER,
  },
  scene("drawer-terminal-live-stopped", "底部抽屉 · 终端 · 实时断了", "远端会话的实时画面推了一屏之后那台断开：画面留着变淡、头上退回「画面几点 ＋ 重新看」、头下一条「实时已停 · devbox 断开」＋［重新接上］、输入灰（不自己重连）", async () => {
    await mainReady(ALL_TABS);
    await openTab(3);
    await click("#session-head button[aria-label='看它的终端']");
    await waitFor("#bottom-drawer pre span");
    await sleep(1200);
  }, liveOfflineWorld),
  scene("drawer-terminal-snaponly", "底部抽屉 · 终端 · 只能快照", "那台的 tmux 低于 3.2：照 L1 的样子（画面几点 ＋ 重新看），多一枚「仅快照」，悬停说为什么", async () => {
    await mainReady(ALL_TABS);
    await click("#session-head button[aria-label='看它的终端']");
    await waitFor("#bottom-drawer pre span");
    await sleep(900);
  }, snapOnlyWorld),
  scene("drawer-terminal-scrolled", "底部抽屉 · 终端 · 往上翻不拽人", "实时画面里往上翻：停在看的位置，右下「回到最新」", async () => {
    await mainReady(ALL_TABS);
    await click("#session-head button[aria-label='看它的终端']");
    await waitFor("#bottom-drawer pre span");
    await sleep(900);
    const box = document.querySelector<HTMLElement>("#bottom-drawer pre")?.parentElement;
    if (box) {
      box.scrollTop = 0;
      box.dispatchEvent(new Event("scroll"));
    }
    await sleep(300);
  }),
  scene("error-card-terminal", "报错卡 · 去它的终端", "远端会话 Claude 已退出、tmux 还在：报错卡上出［在终端里打开］（与会话头同一道；这台不是 Windows ⇒ 没有［切到终端］）", async () => {
    await mainReady(ALL_TABS);
    await openTab(5);
    await waitFor(".card-api-error");
    document.querySelector<HTMLElement>(".card-api-error")?.scrollIntoView({ block: "center" });
    await sleep(600);
  }, exitedErrorWorld),
];
