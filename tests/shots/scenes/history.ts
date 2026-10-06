/**
 * 历史页（主窗口里那一层）与查看窗（单个会话另开一扇窗）。
 */
import type { Scene } from "./index";
import { defaultWorld } from "../fake/world";
import { SEARCH_WORD } from "../fake/history";
import { byText, click, key, mainReady, sleep, type, waitFor } from "./helpers";

const ALL_TABS = 7;

function hist(id: string, title: string, desc: string, act: Scene["act"]): Scene {
  return { id, page: "index", dir: "历史", title, desc, width: 1280, height: 800, world: defaultWorld, act };
}

async function openHistory(): Promise<void> {
  await mainReady(ALL_TABS);
  await click(".history-trigger");
  await waitFor('.history-view [role="option"]');
  await sleep(600);
}

/** 第 n 行（按列表里的先后）。 */
async function row(n: number): Promise<HTMLElement> {
  await waitFor('.history-view [role="option"]');
  return document.querySelectorAll<HTMLElement>('.history-view [role="option"]')[n];
}

const offline = (): ReturnType<typeof defaultWorld> => ({ ...defaultWorld(), historyDown: ["gpu-01"] });

export const HISTORY_SCENES: Scene[] = [
  hist("history-01-by-time", "历史 · 按时间（默认）", "开历史页：按时间平铺，今天 · 昨天 · 本周 · 按月分段；点第一行，右边就地出内容", async () => {
    await openHistory();
    await click(await row(0));
    await sleep(800);
  }),
  hist("history-02-by-project", "历史 · 按项目", "切到「按项目」，点开第一组", async () => {
    await openHistory();
    await click(await byText('.history-view [role="tab"]', "按项目"));
    await click(await waitFor('.history-view [role="treeitem"]'));
    await sleep(400);
  }),
  hist("history-03-type-to-filter", "历史 · 敲字即搜", "敲「导出」：后端按标题 / 第一句 / 项目名搜全部会话", async () => {
    await openHistory();
    await type(".history-search", "导出");
    await sleep(800);
  }),
  hist("history-04-content-search", "历史 · 搜内容", `敲「${SEARCH_WORD}」回车：命中按会话分块，每块前几处`, async () => {
    await openHistory();
    await type(".history-search", SEARCH_WORD);
    await key("Enter");
    await waitFor(".history-view mark", 8000);
    await sleep(400);
  }),
  hist("history-07-row-menu", "历史 · 行菜单", "选中一行、点行尾 ⋯", async () => {
    await openHistory();
    await click(await row(1));
    await click((await row(1)).querySelector<HTMLElement>("button[aria-label]")!);
    await sleep(300);
  }),
  hist("history-08-delete-once", "历史 · 删除只问一次", "⋯ → 删除…：一个框、焦点在「取消」", async () => {
    await openHistory();
    await click(await row(1));
    await click((await row(1)).querySelector<HTMLElement>("button[aria-label]")!);
    await click(await byText('[role^="menuitem"]', "删除"));
    await sleep(300);
  }),
  {
    ...hist("history-10-partial", "历史 · 部分连不上", "gpu-01 那台的清单没答上：列表顶一条［重新连接］，别的台照画", async () => {
      await openHistory();
    }),
    world: offline,
  },
  hist("history-11-no-match", "历史 · 筛选没结果", "敲一个哪儿都没有的词", async () => {
    await openHistory();
    await type(".history-search", "没有这个词");
    await sleep(800);
  }),
  hist("history-12-filter", "历史 · 筛选浮层", "点「筛选」", async () => {
    await openHistory();
    await click(await byText(".history-view button", "筛选"));
    await sleep(300);
  }),
  hist("history-forks", "历史 · 分叉", "devbox 上那个在跑的会话下挂着两个分叉：点「2 个分叉 ▸」展开", async () => {
    await openHistory();
    await click(await byText(".history-view button", "2 个分叉"));
    await sleep(300);
  }),
  hist("history-05-resume-menu", "历史 · 恢复菜单", "右边内容头的［恢复 ▾］点开 ▾：tmux / 不用 tmux × 账号，再加「在此目录新建会话」", async () => {
    await openHistory();
    await click(await byText('.history-view [role="option"]', "支付回调验签"));
    await sleep(900);
    await click(await waitFor('.history-view [data-role="resume"] button[aria-haspopup="menu"]'));
    await sleep(500);
  }),
  hist("history-06-codex", "历史 · Codex 会话", "Codex 会话：恢复按它自己那一家起；▾ 里不问账号，灰一行「Codex · 无账号维」", async () => {
    await openHistory();
    await click(await byText('.history-view [role="option"]', "Codex"));
    await sleep(900);
    await click(await waitFor('.history-view [data-role="resume"] button[aria-haspopup="menu"]'));
    await sleep(500);
  }),
  hist("history-09-said", "历史 · 你说过的话", "查看器工具行「你说过的话 · N ▾」：浮层列出你说过的每一句，当前读到的那句高亮", async () => {
    await openHistory();
    await click(await row(0));
    await sleep(900);
    await click(await waitFor('.history-view [data-role="said"]'));
    await sleep(400);
  }),
  hist("history-17-find", "历史 · 会话内查找", "Ctrl+F 进查找框、回车：命中清单在工具行下就地展开（稿里没画，主会话 10-06 定）", async () => {
    await openHistory();
    await click(await row(0));
    await sleep(900);
    await type(".history-view [data-role=\"find-input\"]", "重试");
    await key("Enter");
    await waitFor('.history-view [data-role="find-hit"]');
    await sleep(400);
  }),
  {
    id: "viewer-window",
    page: "viewer",
    query: "viewer=5e550001-0000-4000-8000-000000000001",
    dir: "历史",
    title: "查看窗",
    desc: "一个会话另开在独立窗口里（tab 右键「在新窗口打开」）",
    width: 1100,
    height: 760,
    world: defaultWorld,
    act: async () => {
      await waitFor("#message-stream .card", 15_000);
      await sleep(1500);
    },
  },
];
