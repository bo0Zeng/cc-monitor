/**
 * 历史页（主窗口里那一层）与查看窗（单个会话另开一扇窗）。
 */
import type { Scene } from "./index";
import { defaultWorld } from "../fake/world";
import { SEARCH_WORD } from "../fake/history";
import { byText, click, key, mainReady, rightClick, sleep, type, waitFor } from "./helpers";

const ALL_TABS = 7;

function hist(id: string, title: string, desc: string, act: Scene["act"]): Scene {
  return { id, page: "index", dir: "历史", title, desc, width: 1280, height: 800, world: defaultWorld, act };
}

async function openHistory(): Promise<void> {
  await mainReady(ALL_TABS);
  await click(".history-trigger");
  await waitFor(".history-group-header");
  await sleep(600);
}

export const HISTORY_SCENES: Scene[] = [
  hist("history-tree", "历史 · 按项目", "历史页默认：本机与各台远端，按项目分组（项目这一层收着）", async () => {
    await openHistory();
  }),
  hist("history-project-open", "历史 · 点开一个项目", "点开本机 orders 项目：它的各个会话（标星、后台、在跑的标记）", async () => {
    await openHistory();
    await click(".history-group-header");
    await sleep(900);
  }),
  hist("history-filter", "历史 · 按名字过滤（没命中）", "「项目」模式下输入「订单」：这个词只出现在会话标题里、不在项目名里，结果是 0 个项目", async () => {
    await openHistory();
    await type(".history-search", "订单");
    await sleep(700);
  }),
  hist("history-filter-hit", "历史 · 按名字过滤（命中）", "「项目」模式下输入「web」：只剩 web-console 那一个项目", async () => {
    await openHistory();
    await type(".history-search", "web");
    await sleep(700);
  }),
  hist("history-fulltext", "历史 · 全文搜索结果", `切到「全文」、搜「${SEARCH_WORD}」：本机与远端命中的会话与片段`, async () => {
    await openHistory();
    await click(await byText(".history-mode-btn", "全文"));
    await type(".history-search", SEARCH_WORD);
    await key("Enter");
    await waitFor(".history-search-results *", 8000);
    await sleep(900);
  }),
  hist("history-row-menu", "历史 · 会话右键菜单", "点开项目、在一个会话上点右键", async () => {
    await openHistory();
    await click(".history-group-header");
    await sleep(700);
    await rightClick(".history-entry");
    await sleep(500);
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
