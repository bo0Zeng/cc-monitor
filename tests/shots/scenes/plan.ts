/**
 * 计划页（设计稿 planned-build 01 · 02 · 03）：大纲 ＋ 概览 · 一格详情 · 各种状态 · 需手动 · 退回框 · 会话头那一枚标。
 */
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { defaultWorld } from "../fake/world";
import { planWorld, type PlanWorldOpts } from "../fake/plan";
import { byText, click, mainReady, sleep, type, waitFor } from "./helpers";

const TABS = 12;

function plan(id: string, title: string, desc: string, act: Scene["act"], o: PlanWorldOpts = {}, size: [number, number] = [1280, 800]): Scene {
  const world = (): World => planWorld(defaultWorld(), o);
  return { id, page: "index", dir: "计划", title, desc, width: size[0], height: size[1], world, act };
}

async function openPlan(wait = true): Promise<void> {
  await mainReady(TABS);
  await click(".plan-trigger");
  if (wait) await waitFor(".plan-view .plan-row[data-cell]", 10_000);
  await sleep(500);
}

/** 点大纲里那一格（按编号认行，标题会互相包含）。 */
async function row(id: string): Promise<void> {
  await click(`.plan-view .plan-row[data-cell="${id}"]`);
  await sleep(400);
}

async function scrollPane(px: number): Promise<void> {
  const p = await waitFor(".plan-view .plan-pane");
  p.scrollTop = px;
  await sleep(300);
}

export const PLAN_SCENES: Scene[] = [
  plan("plan-01-outline", "计划 · 大纲 ＋ 概览", "开计划页：左大纲只写标题（状态图标 · 类一道颜色 · 原因短词 · 块根格挂接手与阶段），右概览（块 · 顶层进度 · 最近签收）", async () => {
    await openPlan();
  }),
  plan("plan-02-entry", "计划 · 顶栏入口", "顶栏多一颗「计划」（单键 P），悬停出名字与快捷键", async () => {
    await mainReady(TABS);
    await waitFor(".plan-trigger");
  }, {}),
  plan("plan-03-filter-open", "计划 · 过滤 没做完", "子头过滤点「没做完」：只留没做完的格与它们的上级", async () => {
    await openPlan();
    await click(await byText(".plan-view .plan-filter", "没做完"));
    await sleep(300);
  }),
  plan("plan-04-slices-offline", "计划 · 多片切换 · 那台离线", "切到 devbox 那一片，之后 devbox 离线：保留最后读到的、整体变淡、顶上一条［重新连接］；［整张图］灰着（只在本机）", async () => {
    await openPlan();
    await click(await byText(".plan-view .plan-slice", "site"));
    await sleep(600);
    await click(await byText(".plan-view button", "返回"));
    await click(".plan-trigger");
    await sleep(900);
  }, { devboxDown: true }),
  plan("plan-05-empty", "计划 · 空态", "哪台都没有 pb 工作区：一句 ＋［添加工作区…］", async () => {
    await openPlan(false);
    await sleep(800);
  }, { empty: true }),
  plan("plan-06-slice-unreadable", "计划 · 这一片读不成", "pb 对这一片给了一句原因：红线原样出那一句，下面留上一次读好的（变淡）", async () => {
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
  plan("plan-09-narrow", "计划 · 窄窗 1000 × 640", "窄窗：大纲 360、类的词收起只留颜色、搜索框收起；折起做完的分支", async () => {
    await openPlan();
    await click(".plan-view .plan-row[data-cell='A1'] [aria-expanded]");
    await sleep(300);
  }, {}, [1000, 640]),
  plan("plan-10-whole", "计划 · 整张图", "点［整张图］：调 pb view、系统浏览器开；toast 一句", async () => {
    await openPlan();
    await click(await byText(".plan-view button", "整张图"));
    await sleep(500);
  }),
  plan("plan-11-auto-failed", "计划 · 自动接着做没切成", "点开关、pb 拒：开关弹回原位，旁边一条红边小条", async () => {
    await openPlan();
    await click(".plan-view [role='switch']");
    await sleep(700);
  }, { autoRefuse: true }),
  plan("plan-21-leaf-done", "一格 · 叶子格 做完了", "点「解析模块」：在哪 · 头 · 位置 · 要做成什么样（话里的格换成标题链接）· 连着的 · 连着它的", async () => {
    await openPlan();
    await row("A2-1-1");
  }),
  plan("plan-22-leaf-scrolled", "一格 · 同一格往下滚", "管的文件 · 收下（两条，后签的作数）· agent 看到的（折着）· 跟 agent 说时用的编号", async () => {
    await openPlan();
    await row("A2-1-1");
    await scrollPane(10_000);
  }),
  plan("plan-23-block-root", "一格 · 派出块的根格", "点「导入解析」：块根格的位置行 · 里面 2/3", async () => {
    await openPlan();
    await row("A2-1");
  }),
  plan("plan-24-upper", "一格 · 等上一级收下", "点「工程底座」：没做完 · 等上一级收下", async () => {
    await openPlan();
    await row("A4");
  }),
  plan("plan-25-files", "一格 · 没签 · 文件缺 / 空 / 坏", "点「导出实现」：对账里那份坏了（带 pb 的原因）", async () => {
    await openPlan();
    await row("A3-1");
  }),
  plan("plan-26-dropped", "一格 · 不做了", "点「网页导入」：划掉变淡、一条「不做了 · 被…顶掉」", async () => {
    await openPlan();
    await row("A5");
  }),
  plan("plan-27-agent-view", "一格 · agent 站在这一格看到的（展开）", "展开那一段：等宽、pb 原样", async () => {
    await openPlan();
    await row("A2-1-1");
    await click(".plan-view .plan-agent-view");
    await sleep(500);
    await scrollPane(10_000);
  }),
  plan("plan-28-unknown-signer", "一格 · 签收人认不出", "点「打包配置」：签收人认不出 ⇒ 虚线框「认不出 · 前后几位」", async () => {
    await openPlan();
    await row("A4-2");
  }),
  plan("plan-31-needs-strip", "需手动 · 标签栏的数 ＋ 会话头那一枚标", "标签栏「需手动 N」含计划项（会话在前、计划项在后）；切到总负责：会话头多一枚「ledger · 顶块 · 执行」", async () => {
    await mainReady(TABS);
    await click(await byText("#tab-bar .tab", "总负责"));
    await waitFor(".session-plan-mark", 10_000);
    await sleep(400);
  }),
  plan("plan-32-top", "需手动 · 顶块走到看全局", "概览顶上一条「整片做完 · 需过目」：［认可］［退回…］· 需手动 1 / 3 · 下一条", async () => {
    await openPlan();
    await waitFor(".plan-view .plan-need");
  }, { topDone: true }),
  plan("plan-33-red", "需手动 · 判据红", "点「工程底座」：红条 · 规则 · 块 · pb 的话 ＋ 改法；［认可］［说给负责的…］", async () => {
    await openPlan();
    await row("A4");
  }),
  plan("plan-34-ask", "需手动 · agent 提问", "点「编码探测」：agent 在这一格提问 · 等回答；只有［去会话答］（计数算在会话上）", async () => {
    await openPlan();
    await row("A2-1-3");
  }),
  plan("plan-35-ended", "需手动 · 接手的会话停了", "点「导出实现」：接手的会话已结束 · 这一块没做完；［恢复会话］［认可］", async () => {
    await openPlan();
    await row("A3-1");
  }),
  plan("plan-36-return", "退回框", "「检查脚本」头右［退回…］：要改什么 · 送给（在长它的 / 签它的）· 送出的一行跟着输入变", async () => {
    await openPlan();
    await row("A4-1");
    await click(await byText(".plan-view .plan-cell button", "退回…"));
    await type(".plan-return-text", "收集 0 条时要退 5，现在退 0；补一条测试。");
    await sleep(300);
  }),
  plan("plan-38-returned", "退回之后 · 等它改", "「检查脚本」顶上一条：已退回 · 送给工程底座 · 几点 · 等它在底下加一格或改正文", async () => {
    await openPlan();
    await row("A4-1");
  }, { returned: "returned" }),
  plan("plan-39-landed", "退回已落地", "「检查脚本」顶上一条：退回已落地 · 在底下加了「收集 0 条时退 5」", async () => {
    await openPlan();
    await row("A4-1");
  }, { returned: "landed" }),
  plan("plan-40-acked", "认可之后", "判据红那一条点［认可］：toast「已认可 · 工程底座」［撤销］，选到下一条", async () => {
    await openPlan();
    await row("A4");
    await click(await byText(".plan-view .plan-need button", "认可"));
    await sleep(900);
  }),
  plan("plan-41-top-return", "顶块的退回框", "顶块那一条点［退回…］：标题是片名、送给只有顶块接手一项（project 没有签它的）；送出的一行「人 · project ledger：…」", async () => {
    await openPlan();
    await waitFor(".plan-view .plan-need");
    await click(await byText(".plan-view .plan-need button", "退回…"));
    await type(".plan-return-text", "导出那一半先停，导入先和真账单对一遍。");
    await sleep(300);
  }, { topDone: true }),
  plan("plan-42-top-returned", "顶块退回之后 · 等它改", "概览顶上：顶块那一条下面一条「已退回 · 送给总负责 · 几点」", async () => {
    await openPlan();
    await waitFor(".plan-view .plan-returned");
  }, { topDone: true, topReturned: "returned" }),
  plan("plan-43-top-landed", "顶块退回已落地", "概览顶上：「退回已落地 · 在底下加了「导出与导入对拍」」", async () => {
    await openPlan();
    await waitFor(".plan-view .plan-returned");
  }, { topDone: true, topReturned: "landed" }),
  plan("plan-44-signs", "签收排成一条", "概览「最近签收 · 全部 N」点进去：整片的签收按时间排成一条、新的在上（不进需手动）；右上按标题找", async () => {
    await openPlan();
    await click(".plan-view .plan-signs-all");
    await waitFor('.plan-view [data-view="signs"]');
    await sleep(300);
  }),
  plan("plan-45-signs-find", "签收流 · 按标题找", "签收流里输「解析」：只剩那几格的签收（被后签的那条也在）", async () => {
    await openPlan();
    await click(".plan-view .plan-signs-all");
    await type('.plan-view [data-find="signs"]', "解析");
    await sleep(300);
  }),
];
