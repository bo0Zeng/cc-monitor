/**
 * 主窗口：tab 栏 · 消息流 · 状态栏在各种会话状态下的样子，以及消息流里的各种卡。
 */
import { hm } from "../fake/clock";
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { Convo } from "../fake/records";
import { answerConvo, defaultWorld, LOCAL, session } from "../fake/world";
import { click, hover, mainReady, openTab, rightClick, scrollStream, settled, sleep, streamScroller, until, waitFor } from "./helpers";

const W = 1280;
const H = 800;
const ALL_TABS = 7;

function main(id: string, title: string, desc: string, act: Scene["act"], world: () => World = defaultWorld, size: [number, number] = [W, H]): Scene {
  return { id, page: "index", dir: "主窗口", title, desc, width: size[0], height: size[1], world, act };
}

function card(id: string, title: string, desc: string, act: Scene["act"], world: () => World = defaultWorld): Scene {
  return { id, page: "index", dir: "主窗口-卡片", title, desc, width: W, height: H, world, act };
}

/** 把第一个会话里某种卡展开（连同它所在的那一轮「过程」与工具组）、滚到它；截之前核它真的在眼前。 */
async function openCard(sel: string, nth = 0, block: ScrollLogicalPosition = "start"): Promise<void> {
  await mainReady(ALL_TABS);
  const el = document.querySelectorAll<HTMLElement>(sel)[nth];
  if (!el) throw new Error(`没有 ${sel}（第 ${nth} 个）`);
  await showCard(el, block);
}

/**
 * 让一张卡真的出现在视口里：做完的那一轮过程默认折成一行（`turn-fold.ts` 给过程里的卡挂 `proc-hidden`），
 * 先点开那一轮的过程行，再一层层打开包着它的折叠，最后滚到它。没出现在视口里 ⇒ 抛（宁可这张图截失败，也不截一张主窗口顶部充数）。
 */
async function showCard(el: HTMLElement, block: ScrollLogicalPosition = "start"): Promise<void> {
  const top = el.closest<HTMLElement>("[data-proc-of]");
  if (top?.classList.contains("proc-hidden")) {
    document.querySelector<HTMLElement>(`.proc-line[data-turn="${top.dataset.procOf}"]`)?.click();
    await sleep(400);
  }
  await expand(el);
  el.scrollIntoView({ block });
  await sleep(400);
  const r = el.getBoundingClientRect();
  if (el.closest(".proc-hidden") || r.height === 0 || r.bottom <= 0 || r.top >= window.innerHeight) {
    throw new Error(`卡没出现在视口里（${el.className}）`);
  }
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
  w.sessions = [session(9, LOCAL, "/home/user/work/monorepo", c, { activity: "working", sid })];
  return w;
}

/** 一个会话：两轮，工具调用之间夹着几条系统注入（提醒 · 技能展开）。 */
function injectWorld(): World {
  const w = defaultWorld();
  const sid = "5e55cccc-0000-4000-8000-00000000cccc";
  const cwd = "/home/user/work/orders";
  const c = new Convo(sid, cwd, "2026-10-01T09:00:00Z");
  c.title("超时配置改读环境变量");
  c.user("把超时配置改成读环境变量。");
  c.tool("Read", { file_path: `${cwd}/src/orders/settings.py` }, "TIMEOUT_S = 30\n", { card: "md" });
  c.user("<system-reminder>\nThe TodoWrite tool hasn't been used recently. Consider using it to track progress.\n</system-reminder>", { meta: true });
  c.tool("Edit", { file_path: `${cwd}/src/orders/settings.py`, old_string: "TIMEOUT_S = 30", new_string: 'TIMEOUT_S = int(os.environ.get("INVENTORY_TIMEOUT_S", "5"))' }, "The file has been updated.", { card: "diff" });
  c.say("改好了：`INVENTORY_TIMEOUT_S` 缺省 5 秒。", 30_000, "end_turn");
  c.user("再跑一遍测试。");
  c.user("<command-name>/skills</command-name>\nBase directory for this skill: /home/user/.claude/skills/run-tests\n跑全量测试并汇总失败。", { meta: true });
  c.tool("Bash", { command: "pytest -q", description: "全量测试" }, "213 passed in 9.12s", { card: "command" });
  c.say("全量 213 个通过。", 32_000, "end_turn");
  w.sessions = [session(9, LOCAL, cwd, c, { activity: "idle", sid })];
  return w;
}

/** 会话头「⋯」里点「显示系统注入」，并核对视口里第一张卡没被挪走（挪了 > 2px 场景就失败）。 */
async function toggleInjected(): Promise<void> {
  const scroller = document.querySelector<HTMLElement>(".stream.active")!;
  const pin = [...scroller.querySelectorAll<HTMLElement>(".stream-content > .card:not(.card-injected), .stream-content > .proc-line")].find((e) => e.getBoundingClientRect().bottom > scroller.getBoundingClientRect().top)!;
  const before = pin.getBoundingClientRect().top;
  const more = [...document.querySelectorAll<HTMLElement>("button")].find((b) => b.querySelector("[data-icon='more']") && !b.closest("#tab-bar"))!;
  more.click();
  await sleep(300);
  const item = [...document.querySelectorAll<HTMLElement>("[role^='menuitem']")].find((e) => e.textContent?.includes("显示系统注入"))!;
  item.click();
  await sleep(300);
  const moved = Math.abs(pin.getBoundingClientRect().top - before);
  if (moved > 2) throw new Error(`切显隐后视口里第一张卡挪了 ${moved}px`);
}

/** 一个会话：你那句里粘了一大一小两块。 */
function pasteWorld(): World {
  const w = defaultWorld();
  const sid = "5e55bbbb-0000-4000-8000-00000000bbbb";
  const c = new Convo(sid, "/home/user/work/orders", "2026-10-01T09:00:00Z");
  c.title("超时日志排查");
  const log = Array.from({ length: 20 }, (_, i) => `2026-10-01 08:5${i % 10}:0${i % 6} WARN inventory call timed out after 30s (attempt ${i + 1})`).join("\n");
  c.user(`这是今早的日志：<pasted_content id="p1">\n${log}\n</pasted_content id="p1">\n配置是：<pasted_content id="p2">retries=0\ntimeout_s=30\nbackoff_ms=0</pasted_content id="p2">\n帮我看看为什么一直超时。`);
  c.say("日志里每次都是整 30 秒超时、而且没有重试——配置里 `retries=0`。先把重试与整体超时加上。", 30_000, "end_turn");
  w.sessions = [session(9, LOCAL, "/home/user/work/orders", c, { activity: "idle", sid })];
  return w;
}

/** gpu-01 上那个会话在等你回答（提问工具）。 */
function answerWorld(): World {
  const w = defaultWorld();
  const s = w.sessions[5];
  w.sessions[5] = { ...s, activity: "needs_you", waitingFor: "dialog open", waitingSinceMs: Date.now() - 300_000, idle: false, records: answerConvo(s.sid, s.cwd).records };
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

/** 两个超长标题：中英混排带空格的一个、无空格长串一个（停在哪个由场景定）。 */
const LONG_MIXED = "把订单服务的重试与超时配置统一改成从环境变量读取 refactor retry and timeout config loading across all services";
const LONG_NOSPACE = "refactor_inventory_client_retry_timeout_config_loader_and_backoff_strategy_for_all_regions_v2";

function longTitleWorld(): World {
  const w = defaultWorld();
  w.sessions[0].records.push({ agent: "claude", id: "@title-long-0", t: "title", text: LONG_MIXED, by: "agent" });
  w.sessions[1].records.push({ agent: "claude", id: "@title-long-1", t: "title", text: LONG_NOSPACE, by: "agent" });
  w.sessions[2].records.push({ agent: "claude", id: "@title-long-2", t: "title", text: LONG_MIXED, by: "agent" });
  return w;
}

/** 超长标题那几张：装成 Windows 上的（会话头多一颗 ↗）；`tab` ＝ 停在第几个标签页。 */
function longTitle(id: string, title: string, desc: string, width: number, height: number, tab = 0, after?: () => Promise<void>): Scene {
  return {
    ...main(id, title, desc, async () => {
      await mainReady(ALL_TABS);
      if (tab > 0) await openTab(tab);
      await after?.();
    }, longTitleWorld, [width, height]),
    hostOs: "windows",
  };
}

export const MAIN_SCENES: Scene[] = [
  main("main-default", "主窗口 · 默认", "本机 3 个、远端 4 个会话；停在第一个会话（在跑、2 个子 agent、4 条任务）的底部", async () => {
    await mainReady(ALL_TABS);
  }),
  main("main-top", "主窗口 · 会话开头", "同一个会话滚到最上：用户的话、思考折叠条、命令卡、读文件卡", async () => {
    await mainReady(ALL_TABS);
    await scrollStream("top");
  }),
  main("main-stream-steps", "主窗口 · 过程一步一行", "同一个会话的中段：每一步一行（状态图标 · 工具名 · 主参数 · 说明 · 右侧小字）· agent 交回事件条 · 后台通知并条 · 失败那一步", async () => {
    await mainReady(ALL_TABS);
    // 完成的轮过程默认折成一行：点开那一行看一步一行。
    (await waitFor(".stream.active .proc-line")).click();
    await sleep(200);
    const s = document.querySelector<HTMLElement>(".card-speaker")!;
    s.scrollIntoView({ block: "center" });
    await sleep(300);
  }),
  main("main-turn-folded", "主窗口 · 完成的轮过程折成一行", "这一轮做完了：你说的 · 过程一行（工具 ×N · 思考 ×N · 失败 ×N · 起止 · 用时）· 结论常显", async () => {
    await mainReady(ALL_TABS);
    await waitFor(".stream.active .proc-line");
    await sleep(200);
    document.querySelector<HTMLElement>(".stream.active .proc-line")!.scrollIntoView({ block: "start" });
    await sleep(300);
  }),
  main("main-turn-menu", "主窗口 · 会话头「⋯」里的开关", "会话头「⋯」＝ 这个标签页的右键菜单 ＋「过程默认展开」（右侧是它的快捷键）", async () => {
    await mainReady(ALL_TABS);
    await waitFor(".stream.active .proc-line");
    const more = [...document.querySelectorAll<HTMLElement>("button")].find((b) => b.querySelector("[data-icon='more']") && !b.closest("#tab-bar"));
    more?.click();
    await sleep(400);
  }),
  main("main-turn-rail", "主窗口 · 轮次刻度悬停", "160 轮的会话：右缘相邻并格（每格 3 轮）· 当前那一格加长、强调色；悬停一格左侧出小卡（第几–几轮 · 起始时刻 · 你那句）", async () => {
    await mainReady(1);
    await waitFor(".turn-rail.active .turn-tick");
    await settled(await until(streamScroller));
    const ticks = [...document.querySelectorAll<HTMLElement>(".turn-rail.active .turn-tick")];
    const t = ticks[Math.floor(ticks.length / 2)];
    t.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    await sleep(700);
  }, longWorld),
  main("main-turn-rail-one", "主窗口 · 轮次刻度（一轮一格）悬停", "默认会话：悬停那一格，左侧小卡给第几轮 · 时刻 · 工具数 · 你那句 · 回复头三行", async () => {
    await mainReady(ALL_TABS);
    const t = await waitFor(".turn-rail.active .turn-tick");
    await sleep(300);
    t.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    await sleep(700);
  }),
  main("main-paste-fold", "主窗口 · 粘贴块折起", "你那句里粘进来 20 行日志：折成一行「粘贴的内容 · 20 行」，点开就地展开；3 行的那一块照排、标记不露", async () => {
    await mainReady(1);
    await waitFor(".stream.active .paste-fold");
    document.querySelector<HTMLElement>(".stream.active .card-user")!.scrollIntoView({ block: "start" });
    await sleep(300);
  }, pasteWorld),
  main("main-injected-off", "主窗口 · 系统注入（默认不露）", "两轮会话、工具调用之间夹着系统注入：开关关着时流里没有它们（过程点开看）", async () => {
    await mainReady(1);
    (await waitFor(".stream.active .proc-line")).click();
    await sleep(300);
  }, injectWorld),
  main("main-injected-on", "主窗口 · 显示系统注入", "会话头「⋯」开「显示系统注入」：每条一行「系统注入 · 时刻」、默认收起、淡一档；切的时候视口里第一张卡不挪（场景自己核对）", async () => {
    await mainReady(1);
    (await waitFor(".stream.active .proc-line")).click();
    await sleep(300);
    await toggleInjected();
    document.querySelector<HTMLDetailsElement>(".stream.active .card-injected")!.open = true;
    await sleep(200);
  }, injectWorld),
  main("main-stream-retry", "主窗口 · 重试细条与提问 / 计划结果", "同一个会话靠后：两次重试并成一条（接上了变淡）· 提问答了「已选」· 计划「已批准」", async () => {
    await mainReady(ALL_TABS);
    // 重试细条在那一轮的过程里（完成的轮默认收起）⇒ 先点开那一轮的过程行，再滚到细条。
    const retry = await waitFor(".stream.active .card-api-retry");
    const turn = retry.dataset.procOf;
    if (turn && retry.classList.contains("proc-hidden")) {
      document.querySelector<HTMLElement>(`.stream.active .proc-line[data-turn="${CSS.escape(turn)}"]`)?.click();
      await sleep(300);
    }
    await scrollStream(".stream.active .card-api-retry");
    retry.scrollIntoView({ block: "start" });
    // 顶上留一点上文（细条上面那一句），细条不贴着会话头。
    let box: HTMLElement | null = retry.parentElement;
    while (box && box.scrollHeight <= box.clientHeight + 4) box = box.parentElement;
    if (box) box.scrollTop -= 80;
    await sleep(300);
  }),
  main("main-waiting", "主窗口 · 等授权的会话", "第二个 tab：会话在等用户点授权（黄灯）", async () => {
    await mainReady(ALL_TABS);
    await openTab(1);
  }),
  main("main-needs-answer", "主窗口 · 需手动：等回答", "gpu-01 上的会话在等你回答：栏顶「需手动 2」、行尾「等回答」、钉条是那一问（远端在 tmux 里 ⇒ 在终端里打开）", async () => {
    await mainReady(ALL_TABS);
    await openTab(5);
    await sleep(600);
  }, answerWorld),
  main("main-codex-card", "主窗口 · Codex 会话的卡头", "会话事实说这条是 Codex 的：卡头写「Codex」（画像里的短名），不按文件名猜", async () => {
    await mainReady(ALL_TABS);
    await openTab(0);
    await sleep(900);
  }, () => {
    const w = defaultWorld();
    w.sessions[0].agent = "codex";
    return w;
  }),
  main("main-hover-card", "主窗口 · 标签页悬停卡", "悬停第二个标签页 500ms：全名 · 机器 · 目录 · 状态句 · 它在等的那一句 · 数字键", async () => {
    await mainReady(ALL_TABS);
    const row = document.querySelectorAll<HTMLElement>("#tab-bar .tab")[1];
    row.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    await sleep(800);
  }),
  main("main-hover-running", "主窗口 · 悬停卡 · 在跑", "悬停一个在跑的标签页：状态句已是「调用 {工具} · 多久了」，peek 那一句给那一步在做什么（命令 · 路径，后端成品）", async () => {
    await mainReady(ALL_TABS);
    const row = document.querySelectorAll<HTMLElement>("#tab-bar .tab")[0];
    row.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    await sleep(800);
  }, () => {
    const w = defaultWorld();
    // 第一个会话此刻在跑一步 Bash（两分钟前开始、还没结果）。
    const at = new Date(Date.now() - 125_000).toISOString();
    w.sessions[0].records.push({
      agent: "claude",
      id: "a-run-step",
      at,
      timeText: hm(Date.now() - 125_000),
      t: "reply",
      blocks: [{ type: "tool_use", id: "toolu_run", name: "Bash", input: { command: "pytest -q tests/test_inventory_client.py" } }],
      autoReply: false,
      endsTurn: false,
      steps: { toolu_run: { tool: "Bash", arg: "pytest -q tests/test_inventory_client.py", known: true } },
    });
    return w;
  }),
  main("main-step-awaiting", "主窗口 · 过程里那一步在等你批准", "在等批准的会话：过程里那一步琥珀点 ·「等你批准」· 右侧已等多久（后端 needs.call 指的那一步）", async () => {
    await mainReady(ALL_TABS);
    await openTab(1);
    await sleep(900);
    for (const d of document.querySelectorAll<HTMLDetailsElement>("#message-stream details")) if (d.querySelector(".step-line[data-state=awaiting]")) d.closest("details:not(.block-tool-use)")?.setAttribute("open", "");
    document.querySelector(".step-line[data-state=awaiting]")?.scrollIntoView({ block: "center" });
    await sleep(500);
  }),
  main("main-needs-list", "主窗口 · 「需手动」菜单", "悬停栏顶「需手动 2」500ms：菜单列出每个在等你的会话与它等的那一句（等得最久的在前；标题一行省略、那一句至多两行；点一行切过去）", async () => {
    await mainReady(ALL_TABS);
    await sleep(600);
    document.querySelector<HTMLElement>(".tab-needs")!.dispatchEvent(new Event("mouseenter"));
    await sleep(800);
  }, answerWorld),
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
    const scroller = await until(streamScroller);
    await settled(scroller);
  }, longWorld),
  main("main-long-middle", "主窗口 · 长会话滚到中间", "同一个长会话往上翻到中间（看懒建卡与骨架）", async () => {
    await mainReady(1);
    const scroller = await until(streamScroller);
    await settled(scroller);
    // 先粗跳到一半（懒建卡要在那一带建出来），再把固定的那一轮（第 80 步完成那张）贴到顶上：
    //   一半处的总高里有多少是估的、多少是量过的，看哪几张卡先建完，按总高的一半截，每趟差半轮。
    scroller.scrollTop = scroller.scrollHeight / 2;
    await settled(scroller);
    const card = [...scroller.querySelectorAll<HTMLElement>("[data-id]")].find((e) => e.textContent?.includes("第 80 步完成"));
    if (!card) throw new Error("粗跳到一半之后，第 80 步那张卡不在页里");
    card.scrollIntoView({ block: "start" });
    await settled(scroller);
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
  main("main-head-menu-after-tab", "主窗口 · 点标签页之后开会话头「⋯」", "先点第二个标签页、再点会话头右上角「⋯」：菜单贴在「⋯」下方、右端对齐（排版量具核对）", async () => {
    await mainReady(ALL_TABS);
    await openTab(1);
    await click(await waitFor<HTMLElement>("#session-head button:has([data-icon='more'])"));
    await sleep(400);
  }),
  longTitle("main-long-title", "主窗口 · 超长标题", "中英混排的超长标题在会话头与标签页里：单行省略，右侧按钮（↗ 等）宽度不变、不被压（排版量具核对）", W, H),
  longTitle("main-long-title-narrow", "窄窗口 · 超长标题", "窗口 900×640、无空格的超长标题：会话头先藏目录与状态一句，标题省略，按钮不被压", 900, 640, 1),
  longTitle("main-long-title-tight", "更窄 · 超长标题 · 已结束", "窗口 760×560、已结束的会话（多一颗［恢复 ▾］）：标题省略、机器名与状态一句也能缩，按钮一颗不少、不被压", 760, 560, 2, async () => {
    document.querySelector<HTMLElement>("#tab-bar .tab.active")!.blur();
  }),
  longTitle("main-long-title-tab-acts", "主窗口 · 超长标题的标签页亮出行尾按钮", "当前标签页（超长标题）获得焦点：行尾按钮（目录 · ↗ · ⋯）盖在标题尾巴上，底色不透、标题字不透出来", W, H, 0, async () => {
    document.querySelector<HTMLElement>("#tab-bar .tab.active")!.focus();
    await sleep(300);
  }),
  longTitle("main-long-title-hover", "主窗口 · 超长标题悬停", "悬停会话头那个被省略的标题：提示给全称", W, H, 0, async () => {
    const t = document.querySelector<HTMLElement>("#session-head > span:nth-child(2)")!;
    await hover(t);
    await sleep(700);
  }),
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
  card("card-agent", "子 agent 卡", "两张子 agent 卡：一张跑完（交回的结果收在卡里、展开）、一张还在跑；卡头点了开它自己的窗口", async () => {
    await mainReady(ALL_TABS);
    // 派出卡收在那一轮的「过程」里：先点开那一行。
    const card0 = document.querySelector<HTMLElement>('[data-role="run-card"]');
    const proc = [...document.querySelectorAll<HTMLElement>('.proc-line[aria-expanded="false"]')].find((l) => l.parentElement?.contains(card0) ?? false) ?? document.querySelector<HTMLElement>('.proc-line[aria-expanded="false"]');
    proc?.click();
    await sleep(400);
    await openCard('[data-role="run-card"]', 0);
    const res = document.querySelector<HTMLDetailsElement>('[data-role="run-card"] [data-role="run-result"]');
    if (res) await expand(res);
    await sleep(400);
  }),
  card("card-thinking", "思考折叠条（展开）", "thinking 块展开", async () => {
    await openCard(".block-thinking", 0);
  }),
  card("card-test-fail", "失败的命令", "全量测试那条命令失败（is_error）", async () => {
    await mainReady(ALL_TABS);
    const all = [...document.querySelectorAll<HTMLDetailsElement>(".block-tool-use")];
    const el = all.find((d) => (d.textContent ?? "").includes("全量测试")) ?? all[all.length - 1];
    await showCard(el, "center");
  }),
  card("card-retry", "API 重试细条", "调用失败、CLI 正在重试（第 1/2 次）", async () => {
    await openCard(".card-api-retry", 0, "center");
  }),
  card("card-ask", "提问卡与计划卡", "AskUserQuestion（已选）＋ ExitPlanMode 计划", async () => {
    await openCard(".block-plan", 0, "center");
  }),
  card("card-compact-error", "续接摘要 · 斜杠命令 · API 报错 · 打断", "一个会话里：/compact 续接摘要、斜杠命令卡、529 报错卡、重试细条、用户打断与打断时说的话", async () => {
    await mainReady(ALL_TABS);
    await scrollStream("top");
  }, oddCardsWorld),
  card("card-table-code", "表格与代码块", "assistant 正文里的表格、有序列表、代码块", async () => {
    await mainReady(ALL_TABS);
    await openCard("table", 0, "center");
  }),
];

