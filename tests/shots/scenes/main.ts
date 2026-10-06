/**
 * 主窗口：tab 栏 · 消息流 · 状态栏在各种会话状态下的样子，以及消息流里的各种卡。
 */
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { Convo } from "../fake/records";
import { answerConvo, defaultWorld, LOCAL, session } from "../fake/world";
import { mainReady, openTab, rightClick, scrollStream, sleep } from "./helpers";

const W = 1280;
const H = 800;
const ALL_TABS = 7;

function main(id: string, title: string, desc: string, act: Scene["act"], world: () => World = defaultWorld, size: [number, number] = [W, H]): Scene {
  return { id, page: "index", dir: "主窗口", title, desc, width: size[0], height: size[1], world, act };
}

function card(id: string, title: string, desc: string, act: Scene["act"], world: () => World = defaultWorld): Scene {
  return { id, page: "index", dir: "主窗口-卡片", title, desc, width: W, height: H, world, act };
}

/** 把第一个会话里某种卡展开（连同它所在的工具组）、滚到它。 */
async function openCard(sel: string, nth = 0): Promise<void> {
  await mainReady(ALL_TABS);
  const el = document.querySelectorAll<HTMLElement>(sel)[nth];
  if (!el) throw new Error(`没有 ${sel}（第 ${nth} 个）`);
  await expand(el);
  el.scrollIntoView({ block: "start" });
  await sleep(400);
}

/** 展开一张卡：先把外面包着它的折叠（工具组）一层层打开，再打开它自己。 */
async function expand(el: HTMLElement): Promise<void> {
  const chain: HTMLDetailsElement[] = [];
  for (let p: HTMLElement | null = el; p; p = p.parentElement) if (p instanceof HTMLDetailsElement) chain.unshift(p);
  for (const d of chain) {
    if (!d.open) {
      d.open = true;
      d.dispatchEvent(new Event("toggle"));
      await sleep(250);
    }
  }
}

function emptyWorld(): World {
  const w = defaultWorld();
  w.sessions = [];
  return w;
}

function longWorld(): World {
  const w = defaultWorld();
  const sid = "5e55aaaa-0000-4000-8000-00000000aaaa";
  const c = new Convo(sid, "/home/user/work/monorepo", "2026-09-30T08:00:00Z");
  c.title("大仓重构：拆出共享组件库");
  for (let i = 1; i <= 160; i++) {
    c.user(`第 ${i} 步：把 packages/ui-${i % 17} 里重复的按钮样式并到共享组件库。`);
    c.tool("Grep", { pattern: `Button${i % 9}`, path: "packages" }, `packages/ui-${i % 17}/src/Button.tsx\npackages/shared/src/Button.tsx`);
    c.tool(
      "Edit",
      { file_path: `/home/user/work/monorepo/packages/ui-${i % 17}/src/Button.tsx`, old_string: `export const Button${i % 9} = styled.button\``, new_string: `export { Button as Button${i % 9} } from "@acme/shared";` },
      "The file has been updated.",
      { card: "diff" },
    );
    c.say(`第 ${i} 步完成：删掉 ${12 + (i % 30)} 行重复样式，引用改到共享组件。`, 30_000 + i * 900);
  }
  w.sessions = [session(9, LOCAL, "/home/user/work/monorepo", c, { status: "busy", sid })];
  return w;
}

/** gpu-01 上那个会话在等你回答（提问工具）。 */
function answerWorld(): World {
  const w = defaultWorld();
  const s = w.sessions[5];
  w.sessions[5] = { ...s, status: "waiting", waitingFor: "dialog open", waitingSinceMs: Date.now() - 300_000, idle: false, records: answerConvo(s.sid, s.cwd).records };
  return w;
}

function unseenWorld(): World {
  const w = defaultWorld();
  w.unseenMachines = ["gpu-01"];
  return w;
}

/** gpu-01 一开始连着、会话交完之后断了。 */
function droppedWorld(): World {
  const w = defaultWorld();
  w.droppedMachines = ["gpu-01"];
  return w;
}

/** 手动建的两个 tab 集合 ＋ 一个固定的 tab。 */
function groupedWorld(): World {
  const w = defaultWorld();
  const sid = (i: number): string => w.sessions[i].sid;
  w.config = {
    ...w.config,
    tabCollections: [
      { id: "c1", name: "订单与控制台" },
      { id: "c2", name: "训练" },
    ],
    tabBar: { pinned: [sid(0)], groupOf: { [sid(0)]: "c1", [sid(1)]: "c1", [sid(5)]: "c2" } },
  };
  return w;
}

/** 本机后端比界面老：账号、任务、大纲那几问它都不认。 */
function oldBackendWorld(): World {
  const w = defaultWorld();
  w.quiet = ["accounts-list", "accounts-sessions", "history-last-accounts", "tasks-list", "history-user-inputs", "history-facts", "history-index"];
  for (const op of w.quiet) delete w.ops[op];
  return w;
}

function oddCardsWorld(): World {
  const w = defaultWorld();
  const s = w.sessions[0];
  const c = new Convo(s.sid, s.cwd, "2026-10-01T07:00:00Z");
  c.title("续接与报错");
  c.user(
    "This session is being continued from a previous conversation that ran out of context. The conversation is summarized below:\n\n## 摘要\n\n- 订单服务加重试（已完成）\n- 下一步：把超时配置写进文档",
  );
  c.user("<command-name>/compact</command-name>\n<command-message>compact</command-message>\n<command-args>只保留未完成的事</command-args>", { meta: false });
  c.say("好的，继续写文档。");
  c.apiError(529, "API Error: 529 {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}");
  c.retry(3, 10);
  c.user("[Request interrupted by user]", { interrupt: true });
  c.queued("先别写文档了，先把 CHANGELOG 补上");
  c.user("先补 CHANGELOG。");
  c.say("好，CHANGELOG 里加了一条「库存接口调用加重试与整体超时」。");
  s.records = c.records;
  return w;
}

export const MAIN_SCENES: Scene[] = [
  main("main-default", "主窗口 · 默认", "本机 3 个、远端 4 个会话；停在第一个会话（在跑、2 个子 agent、4 条任务）的底部", async () => {
    await mainReady(ALL_TABS);
  }),
  main("main-top", "主窗口 · 会话开头", "同一个会话滚到最上：用户的话、思考折叠条、命令卡、读文件卡", async () => {
    await mainReady(ALL_TABS);
    await scrollStream("top");
  }),
  main("main-waiting", "主窗口 · 等授权的会话", "第二个 tab：会话在等用户点授权（黄灯）", async () => {
    await mainReady(ALL_TABS);
    await openTab(1);
  }),
  main("main-needs-answer", "主窗口 · 需要你：等回答", "gpu-01 上的会话在等你回答：栏顶「需要你 2」、行尾「等回答」、钉条是那一问（远端在 tmux 里 ⇒ 在终端里打开）", async () => {
    await mainReady(ALL_TABS);
    await openTab(5);
    await sleep(600);
  }, answerWorld),
  main("main-hover-card", "主窗口 · 标签页悬停卡", "悬停第二个标签页 500ms：全名 · 机器 · 目录 · 状态句 · 它在等的那一句 · 数字键", async () => {
    await mainReady(ALL_TABS);
    const row = document.querySelectorAll<HTMLElement>("#tab-bar .tab")[1];
    row.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    await sleep(800);
  }),
  main("main-ended", "主窗口 · 已结束的会话", "第三个 tab：会话已结束（tab 灰、斜体）", async () => {
    await mainReady(ALL_TABS);
    await openTab(2);
  }),
  main("main-remote", "主窗口 · 远端会话", "devbox 上的会话：tab 带机器名、账号徽标", async () => {
    await mainReady(ALL_TABS);
    await openTab(3);
  }),
  main("main-bg", "主窗口 · 后台会话", "devbox 上的后台会话（kind=bg）", async () => {
    await mainReady(ALL_TABS);
    await openTab(4);
  }),
  main("main-tmux-idle", "主窗口 · claude 退了、tmux 还在", "gpu-01 上的会话：claude 已退出、终端还开着（灰灯）", async () => {
    await mainReady(ALL_TABS);
    await openTab(5);
  }),
  main("main-windows", "主窗口 · Windows 机器上的会话", "win-laptop 上的会话（Windows 路径，等输入）", async () => {
    await mainReady(ALL_TABS);
    await openTab(6);
  }),
  main("main-empty", "主窗口 · 一个会话都没有", "四台机器都连着、没有在跑的会话（空状态）", async () => {
    await sleep(2500);
  }, emptyWorld),
  main("main-long", "主窗口 · 长会话", "一个 160 轮、640 条记录的会话，停在底部", async () => {
    await mainReady(1);
    await sleep(1500);
  }, longWorld),
  main("main-long-middle", "主窗口 · 长会话滚到中间", "同一个长会话往上翻到中间（看懒建卡与骨架）", async () => {
    await mainReady(1);
    await sleep(1200);
    const box = document.querySelector<HTMLElement>("#message-stream");
    const scroller = [...(box?.querySelectorAll<HTMLElement>("*") ?? [])].find((e) => e.scrollHeight > e.clientHeight + 100);
    if (scroller) scroller.scrollTop = scroller.scrollHeight / 2;
    await sleep(800);
  }, longWorld),
  main("main-unseen-machine", "主窗口 · 一台机器连不上", "gpu-01 的会话流一开始就看不见（那台机器连不上）", async () => {
    await mainReady(ALL_TABS - 1);
    await sleep(800);
  }, unseenWorld),
  main("main-unseen-machine-later", "主窗口 · 一台机器一直连不上", "gpu-01 的会话流一开始就看不见、20 秒后还没连上：右下角说是哪台、能做什么", async () => {
    await mainReady(ALL_TABS - 1);
    await sleep(21_000);
  }, unseenWorld),
  main("main-dropped-machine-later", "主窗口 · 一台机器连着连着断了", "gpu-01 一开始连着、之后断了、20 秒没回来：右下角说连接断了（不说「还没连上」）", async () => {
    await mainReady(ALL_TABS);
    await sleep(22_500);
  }, droppedWorld),
  main("main-groups", "主窗口 · tab 集合与固定", "手动建的两个集合（订单与控制台 · 训练）＋ 一个固定的 tab", async () => {
    await mainReady(ALL_TABS);
  }, groupedWorld),
  main("main-old-backend", "主窗口 · 后端比界面老", "本机后端不认账号 / 任务 / 大纲那几问：状态栏与右上角的样子", async () => {
    await mainReady(ALL_TABS);
    await sleep(600);
  }, oldBackendWorld),
  main("main-narrow", "主窗口 · 窄窗口", "窗口 900×640：tab 栏与状态栏挤一挤的样子", async () => {
    await mainReady(ALL_TABS);
  }, defaultWorld, [900, 640]),
  main("main-narrow-ended-menu", "窄窗口 · 已结束 tab 的右键菜单", "窗口 900×640、tab 栏收成点：右键已结束的「周报草稿」，菜单里能关掉它", async () => {
    await mainReady(ALL_TABS);
    await rightClick(document.querySelectorAll<HTMLElement>("#tab-bar .tab")[2]);
    await sleep(900);
  }, defaultWorld, [900, 640]),
  main("main-wide", "主窗口 · 宽屏", "窗口 1920×1080", async () => {
    await mainReady(ALL_TABS);
  }, defaultWorld, [1920, 1080]),

  card("card-bash", "命令卡（展开）", "Bash 工具卡展开：命令 ＋ 输出", async () => {
    await openCard(".block-tool-use", 0);
  }),
  card("card-read", "读文件卡（展开）", "Read 工具卡展开：带行号的文件内容", async () => {
    await openCard(".block-tool-use", 1);
  }),
  card("card-diff", "改动卡（展开）", "Edit 工具卡展开：行级对比", async () => {
    await openCard(".block-tool-use", 2);
  }),
  card("card-agent", "子 agent 卡", "两张子 agent 卡：一张跑完、一张还在跑", async () => {
    await openCard(".block-agent", 0);
  }),
  card("card-thinking", "思考折叠条（展开）", "thinking 块展开", async () => {
    await openCard(".block-thinking", 0);
  }),
  card("card-test-fail", "失败的命令", "全量测试那条命令失败（is_error）", async () => {
    await mainReady(ALL_TABS);
    const all = [...document.querySelectorAll<HTMLDetailsElement>(".block-tool-use")];
    const el = all.find((d) => (d.textContent ?? "").includes("全量测试")) ?? all[all.length - 1];
    await expand(el);
    el.scrollIntoView({ block: "center" });
    await sleep(400);
  }),
  card("card-retry", "API 重试细条", "调用失败、CLI 正在重试（第 1/2 次）", async () => {
    await mainReady(ALL_TABS);
    await scrollStream(".card-api-retry");
  }),
  card("card-ask", "提问卡与计划卡", "AskUserQuestion（已选）＋ ExitPlanMode 计划", async () => {
    await mainReady(ALL_TABS);
    await scrollStream(".block-plan");
    await sleep(100);
    const s = document.querySelector(".block-plan");
    s?.scrollIntoView({ block: "center" });
    await sleep(300);
  }),
  card("card-compact-error", "续接摘要 · 斜杠命令 · API 报错 · 打断", "一个会话里：/compact 续接摘要、斜杠命令卡、529 报错卡、重试细条、用户打断与打断时说的话", async () => {
    await mainReady(ALL_TABS);
    await scrollStream("top");
  }, oddCardsWorld),
  card("card-table-code", "表格与代码块", "assistant 正文里的表格、有序列表、代码块", async () => {
    await mainReady(ALL_TABS);
    const el = [...document.querySelectorAll("table")][0];
    el?.scrollIntoView({ block: "center" });
    await sleep(400);
  }),
];

