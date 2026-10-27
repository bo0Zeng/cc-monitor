/**
 * 计划页（pb 0.5 · dump 形状 6）：大纲 ＋ 概览 · 一个节点的详情 · 各种状态 · 需手动 · 退回框 · 会话头那一枚标 · 交接说明 · 新登记的仓库还没有图。
 */
import type { Scene } from "./index";
import type { SceneCtx, World } from "../fake/types";
import { defaultWorld, LOCAL } from "../fake/world";
import { LEDGER_REL, LEDGER_WS, ledgerDump, planWorld, type Landed, type PlanWorldOpts } from "../fake/plan";
import { dumpFile } from "../disk/plan";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { byText, click, mainReady, sleep, type, until, waitFor } from "./helpers";

const TABS = 12;

function plan(id: string, title: string, desc: string, act: Scene["act"], o: PlanWorldOpts = {}, size: [number, number] = [1280, 800]): Scene {
  const world = (): World => planWorld(defaultWorld(), o);
  return { id, page: "index", dir: "计划", title, desc, width: size[0], height: size[1], world, act };
}

async function openPlan(wait = true): Promise<void> {
  await mainReady(TABS);
  await click(".plan-trigger");
  if (wait) await waitFor(".plan-view .plan-row[data-cell]");
  await sleep(500);
}

/** 点大纲里那一个节点（按编号认行，标题会互相包含）。 */
async function row(id: string): Promise<void> {
  await click(`.plan-view .plan-row[data-cell="${id}"]`);
  await sleep(400);
}

/** 开页之前退回过一次（经真后端，同退回框送出的那一条）；`landed` ⇒ 之后 pb 那一侧在底下加了一个节点。 */
async function returnedBefore({ backend }: SceneCtx, id: string, text: string, landed?: Landed, o: PlanWorldOpts = {}): Promise<void> {
  await mainReady(TABS);
  const r = await backend.planReturn(LOCAL, { workspace: LEDGER_WS, slice: "ledger", id, text });
  if (r.result !== "delivered") throw new Error(`退回没送到：${JSON.stringify(r)}`);
  if (landed) await backend.writeFile(LOCAL, dumpFile(LEDGER_REL), JSON.stringify(ledgerDump(o, landed)));
}

async function scrollPane(px: number): Promise<void> {
  const p = await waitFor(".plan-view .plan-pane");
  p.scrollTop = px;
  await sleep(300);
}

export const PLAN_SCENES: Scene[] = [
  plan("plan-01-outline", "计划 · 大纲 ＋ 概览", "开计划页：左大纲只写标题（状态图标 · 类一道颜色 · 原因短词 · 划出的根节点挂负责人与阶段），右概览（权力范围 · 顶层进度）；顶上一条读不成的那一处、子头「无法判定的节点」", async () => {
    await openPlan();
  }),
  plan("plan-02-entry", "计划 · 顶栏入口", "顶栏多一颗「计划」（单键 P），悬停出名字与快捷键", async () => {
    await mainReady(TABS);
    await waitFor(".plan-trigger");
  }, {}),
  plan("plan-03-filter-open", "计划 · 过滤 没签收的", "子头过滤点「没签收的」：只留算活没签收的、待定还没人定的与它们的上级", async () => {
    await openPlan();
    await click(await byText(".plan-view .plan-filter", copyText("plan.filter.open")));
    await sleep(300);
  }),
  plan("plan-04-slices-offline", "计划 · 多个仓库切换 · 那台离线", "切到 devbox 那个仓库，之后 devbox 离线（那台后端被杀）：保留最后读到的、整体变淡、顶上一条［重新连接］；［整张图］灰着（只在本机）", async ({ backend }) => {
    await openPlan();
    await click(await byText(".plan-view .plan-slice", "site"));
    await sleep(600);
    await backend.dropMachine("devbox");
    await click(await byText(".plan-view button", "返回"));
    await click(".plan-trigger");
    await sleep(900);
  }),
  plan("plan-05-empty", "计划 · 空态", "哪台都没有 pb 工作区：一句 ＋［添加工作区…］", async () => {
    await openPlan(false);
    await sleep(800);
  }, { empty: true }),
  plan("plan-06-slice-unreadable", "计划 · 这个仓库读不成", "pb 对这个仓库给了一句原因：红线原样出那一句，下面留上一次读好的（变淡）", async () => {
    await openPlan();
  }, { sliceStale: true }),
  plan("plan-07a-pb-missing", "计划 · pb 没装", "这台没装 planned-build：整页一句", async () => {
    await openPlan(false);
    await sleep(800);
  }, { pb: "missing" }),
  plan("plan-07b-pb-old", "计划 · pb 太旧", "pb 没有 dump（后端判）：整页一句 ＋ 要更新", async () => {
    await openPlan(false);
    await sleep(800);
  }, { pb: "old" }),
  plan("plan-08-loading", "计划 · 首次加载", "plan-list 还没回来：骨架", async () => {
    await mainReady(TABS);
    await click(".plan-trigger");
    await sleep(500);
  }, { slowMs: 20_000 }),
  plan("plan-09-narrow", "计划 · 窄窗 1000 × 640", "窄窗：大纲 360、类的词收起只留颜色、搜索框收起；折起签收了的分支", async () => {
    await openPlan();
    await click(".plan-view .plan-row[data-cell='A1'] [aria-expanded]");
    await sleep(300);
  }, {}, [1000, 640]),
  plan("plan-10-whole", "计划 · 整张图", "点［整张图］：调 pb render、系统浏览器开；toast 一句", async () => {
    await openPlan();
    await click(await byText(".plan-view button", "整张图"));
    await sleep(500);
  }),
  plan("plan-11-auto-failed", "计划 · 自动继续没切成", "点开关、pb 拒：开关弹回原位，旁边一条红边小条", async () => {
    await openPlan();
    await click(".plan-view [role='switch']");
    await sleep(700);
  }, { autoRefuse: true }),
  plan("plan-12-fresh", "计划 · 新登记的仓库还没有图", "切到 invoice（pb repo 建了 project、还没人认领）：整页一句「invoice · 软件 · 无图」＋ 怎么让它有 ＋［起新会话…］＋ project 的交接说明（不带认领码）［复制］", async () => {
    await openPlan();
    await click(await byText(".plan-view .plan-slice", "invoice"));
    await waitFor('.plan-view [data-view="fresh"] .plan-handoff');
    await sleep(400);
  }, { fresh: true }),
  plan("plan-13-fresh-new", "计划 · 无图 · 起新会话", "无图那一屏点［起新会话…］：全产品那一个起新会话框，工作目录是工作区根", async () => {
    await openPlan();
    await click(await byText(".plan-view .plan-slice", "invoice"));
    await click(await byText('.plan-view [data-view="fresh"] button', copyText("plan.fresh.newSession")));
    await sleep(900);
  }, { fresh: true }),
  plan("plan-14-docs-only", "计划 · 只做计划的仓库", "ledger 还没 pb adopt：子头一枚「只做计划」，页头多一颗［接过仓库…］", async () => {
    await openPlan();
    await waitFor('.plan-view [data-act="adopt"]');
  }, { docsOnly: true }),
  plan("plan-15-adopt-confirm", "计划 · 接过仓库 · 问一句", "点［接过仓库…］：退不回 ⇒ 先问一句（之后 pb 照规则守那个仓库）", async () => {
    await openPlan();
    await click('.plan-view [data-act="adopt"]');
    await sleep(500);
  }, { docsOnly: true }),
  plan("plan-16-adopt-done", "计划 · 接过仓库之后", "点［接过仓库］：toast 说接过来的是哪一个 · 仓库在哪；重读之后「只做计划」与那颗按钮都没了", async () => {
    await openPlan();
    await click('.plan-view [data-act="adopt"]');
    await click(await byText("[role=dialog] button", copyText("plan.adopt.action")));
    // pb 改了工作区配置 ⇒ 那台后端盯盘推一帧、页上重读：等那颗按钮没了。
    await until(() => (document.querySelector('.plan-view [data-act="adopt"]') ? null : true), 20_000);
    await sleep(300);
  }, { docsOnly: true }),
  plan("plan-17-groups", "计划 · 权力范围表的「组」那一列", "接手会话在哪几组（真后端照注解文件里的组标好）：导入解析的子 agent 接手的那一块照父会话写「导入」，工程底座写「底座」，导出实现不在组里写「—」", async () => {
    await openPlan();
    await waitFor(".plan-view .plan-groups");
  }, { groups: true }),
  plan("plan-18-new", "计划 · 新计划框", "页头［新计划…］：机器 · 工作区（已有的在前，会话的工作目录在后）· 路径 · 领域（pb dump --domains 给的几档）· 只读「执行」那一行（真后端写的）", async () => {
    await openPlan();
    await click('.plan-view [data-act="new-plan"]');
    await type("[role=dialog] input", "invoice");
    await sleep(600);
    await click(await byText("[role=dialog] [role=radiogroup] button", "软件"));
    await until(() => (document.querySelector("[role=dialog] code")?.textContent ? true : null), 20_000);
    await sleep(400);
  }),
  plan("plan-19-new-exists", "计划 · 新计划框 · 已有这一片", "路径写成已有的 ledger：领域锁成它的、主按钮［切换到此片］，执行那一行只带名字", async () => {
    await openPlan();
    await click('.plan-view [data-act="new-plan"]');
    await type("[role=dialog] input", "ledger");
    await until(() => (document.querySelector("[role=dialog] [data-locked]") ? true : null), 20_000);
    await sleep(400);
  }),
  plan("plan-21-leaf-done", "一个节点 · 叶子 已签收", "点「解析模块」：在哪 · 头 · 位置 · 要做成什么样（话里的编号换成标题链接）· 连着的 · 连着它的", async () => {
    await openPlan();
    await row("A2-1-1");
  }),
  plan("plan-22-leaf-scrolled", "一个节点 · 同一个往下滚", "管的文件 · 状态（已签收 2 · 依据它的）· agent 看到的（折着）· 跟 agent 说时用的编号", async () => {
    await openPlan();
    await row("A2-1-1");
    await scrollPane(10_000);
  }),
  plan("plan-23-block-root", "一个节点 · 划出的根节点", "点「导入解析」：根节点的位置行 · 底下签收了几个 · 待执行的那一条 sets（虚线）", async () => {
    await openPlan();
    await row("A2-1");
  }),
  plan("plan-24-upper", "一个节点 · 等上一级", "点「工程底座」：交回的那一条 sets 等上一级删掉划出那一项（虚线，pb 说的为什么）", async () => {
    await openPlan();
    await row("A4");
  }),
  plan("plan-25-files", "一个节点 · 没签 · 文件缺 / 空 / 坏", "点「导出实现」：文件核对那份坏了（带 pb 的原因）", async () => {
    await openPlan();
    await row("A3-1");
  }),
  plan("plan-26-undecided", "一个节点 · 待定还没人定", "点「要不要支持网页导入」：状态图标是待定那一种、行尾「待人定」", async () => {
    await openPlan();
    await row("A5");
  }),
  plan("plan-27-agent-view", "一个节点 · agent 站在这里看到的（展开）", "展开那一段：等宽、pb 原样（dump --node）", async () => {
    await openPlan();
    await row("A2-1-1");
    await click(".plan-view .plan-agent-view");
    await sleep(500);
    await scrollPane(10_000);
  }),
  plan("plan-28-handoff", "一个节点 · 划出了没人接着", "点「导出格式选项」：位置行下面一块交接说明（pb 原样那一句，认领码找上一级要）［复制］", async () => {
    await openPlan();
    await row("A3-3");
    await waitFor(".plan-view .plan-cell .plan-handoff");
  }),
  plan("plan-29-takeover", "一个节点 · 新会话接手", "「导出格式选项」交接说明底下点［新会话接手…］：全产品那一个新建会话框，机器锁住、工作目录锁在工作区根（没有［最近的］）、「第一句话」预填 pb 那一句交接说明", async () => {
    await openPlan();
    await row("A3-3");
    await click(await byText(".plan-view .plan-cell .plan-handoff button", copyText("plan.takeover.open")));
    await waitFor("[role=dialog] textarea");
    await sleep(900);
  }),
  plan("plan-31-needs-strip", "需手动 · 标签栏的数 ＋ 会话头那一枚标", "标签栏「需手动 N」含计划项（会话在前、计划项在后）；切到总负责：会话头多一枚「ledger · project · 开发」", async () => {
    await mainReady(TABS);
    await click(await byText("#tab-bar .tab", /^(?!.*站点).*总负责/));
    await waitFor(".session-plan-mark");
    await sleep(400);
  }),
  plan("plan-32-top", "需手动 · 顶层全签收", "概览顶上一条「顶层全签收 · 需过目」：［认可］［退回…］· 需手动 1 / N · 下一条", async () => {
    await openPlan();
    await waitFor(".plan-view .plan-need");
  }, { topDone: true }),
  plan("plan-33-red", "需手动 · 校验不通过", "点「工程底座」：红条 · 规则 · 权力范围 · pb 的话 ＋ 改法；［认可］［说给负责的…］", async () => {
    await openPlan();
    await row("A4");
  }),
  plan("plan-34-ask", "需手动 · agent 提问", "点「编码探测」：agent 在此节点提问 · 等回答；只有［去会话答］（计数算在会话上）", async () => {
    await openPlan();
    await row("A2-1-3");
  }),
  plan("plan-35-ended", "需手动 · 负责人的会话停了", "点「导出实现」：负责人的会话已结束 · 根节点没签收；［恢复会话］［认可］", async () => {
    await openPlan();
    await row("A3-1");
  }),
  plan("plan-36-return", "退回框", "「检查脚本」头右［退回…］：要改什么 · 送给（负责的 / 写得动它的）· 送出的一行跟着输入变", async () => {
    await openPlan();
    await row("A4-1");
    await click(await byText(".plan-view .plan-cell button", copyText("plan.review.return")));
    await type(".plan-return-text", "收集 0 条时要退 5，现在退 0；补一条测试。");
    await sleep(300);
  }),
  plan("plan-37-return-resume", "退回框 · 负责人已结束", "「导出实现」点［说给负责的…］：负责人的会话已结束 ⇒ 框里一句「送出时先恢复它」，主按钮［恢复并送出］", async () => {
    await openPlan();
    await row("A3-1");
    await click(await byText(".plan-view .plan-cell button", copyText("plan.review.tell")));
    await type(".plan-return-text", "导出先只做 CSV，别的格式下一轮。");
    await sleep(300);
  }),
  plan("plan-38-returned", "退回之后 · 等它改", "「检查脚本」顶上一条：已退回 · 送给工程底座 · 几点 · 等它在底下加一个节点或改正文", async (ctx) => {
    await returnedBefore(ctx, "A4-1", "收集 0 条时要退 5，现在退 0；补一条测试。");
    await openPlan();
    await row("A4-1");
  }),
  plan("plan-39-landed", "退回已落地", "「检查脚本」顶上一条：退回已落地 · 在底下加了「收集 0 条时退 5」", async (ctx) => {
    await returnedBefore(ctx, "A4-1", "收集 0 条时要退 5，现在退 0；补一条测试。", "cell");
    await openPlan();
    await row("A4-1");
  }),
  plan("plan-40-acked", "认可之后", "不通过那一条点［认可］：toast「已认可 · 工程底座」［撤销］，选到下一条", async () => {
    await openPlan();
    await row("A4");
    await click(await byText(".plan-view .plan-need button", copyText("plan.review.ack")));
    await sleep(900);
  }),
  plan("plan-41-top-return", "project 的退回框", "顶层全签收那一条点［退回…］：标题是仓库名、送给只有 project 的负责人一项（project 没有写得动它的上一级）；送出的一行「人 · project ledger：…」", async () => {
    await openPlan();
    await waitFor(".plan-view .plan-need");
    await click(await byText(".plan-view .plan-need button", copyText("plan.review.return")));
    await type(".plan-return-text", "导出那一半先停，导入先和真账单对一遍。");
    await sleep(300);
  }, { topDone: true }),
  plan("plan-42-top-returned", "project 退回之后 · 等它改", "概览顶上：顶层那一条下面一条「已退回 · 送给总负责 · 几点」", async (ctx) => {
    await returnedBefore(ctx, "project", "导出那一半先停，导入先和真账单对一遍。");
    await openPlan();
    await waitFor(".plan-view .plan-returned");
  }, { topDone: true }),
  plan("plan-43-top-landed", "project 退回已落地", "概览顶上：「退回已落地 · 在底下加了「导出与导入对拍」」", async (ctx) => {
    await returnedBefore(ctx, "project", "导出那一半先停，导入先和真账单对一遍。", "top", { topDone: true });
    await openPlan();
    await waitFor(".plan-view .plan-returned");
  }, { topDone: true }),
];
