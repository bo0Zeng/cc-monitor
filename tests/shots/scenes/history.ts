/**
 * 历史页（主窗口里那一层）与查看窗（单个会话另开一扇窗）。
 */
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { Scene } from "./index";
import { defaultWorld, LOCAL } from "../fake/world";
import { putQuota, type QuotaSpec } from "../disk";
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

/** 展开取回那一下：取回全文那一问慢 2.5 秒（「读取中」截得到）。 */
const slowFull = (): ReturnType<typeof defaultWorld> => ({ ...defaultWorld(), fullFetchDelayMs: { "history-page": 2500 } });

/** 查看窗里打开最后一个跑命令（Bash）那一步的结果（连同外面的折叠）；回那个结果的折叠。 */
async function openLastResult(): Promise<HTMLDetailsElement> {
  await waitFor(".session-viewer [data-id]", 15_000);
  await sleep(1200);
  const all = [...document.querySelectorAll<HTMLDetailsElement>(".session-viewer .block-tool-result-inline")].filter((r) =>
    r.parentElement?.closest("details")?.querySelector(":scope > summary")?.textContent?.includes("Bash"),
  );
  const res = all.at(-1);
  if (!res) throw new Error("查看窗里没有跑命令那一步的结果");
  // 做完的那一轮过程默认折成一行（`turn-fold.ts` 挂 `proc-hidden`）：先点开那一轮的过程行。
  const top = res.closest<HTMLElement>("[data-proc-of]");
  if (top?.classList.contains("proc-hidden")) {
    document.querySelector<HTMLElement>(`.proc-line[data-turn="${top.dataset.procOf}"]`)?.click();
    await sleep(400);
  }
  for (let p: HTMLElement | null = res; p; p = p.parentElement) {
    if (p instanceof HTMLDetailsElement && !p.open) {
      p.open = true;
      p.dispatchEvent(new Event("toggle"));
    }
  }
  res.scrollIntoView({ block: "center" });
  const r = res.getBoundingClientRect();
  if (res.closest(".proc-hidden") || r.height === 0 || r.bottom <= 0 || r.top >= window.innerHeight) throw new Error("那个结果没出现在视口里");
  return res;
}

const offline = (): ReturnType<typeof defaultWorld> => ({ ...defaultWorld(), historyDown: ["gpu-01"] });

/** 额度账里有这几台的号的数（恢复菜单每个号后面 `5h N%`）。 */
function withQuota(): ReturnType<typeof defaultWorld> {
  const w = defaultWorld();
  const now = (): number => Math.floor(Date.now() / 1000);
  const t = now();
  const acct = (account: string, pct5: number, pct7: number): QuotaSpec => ({
    account,
    seenAt: t - 120,
    status: "allowed",
    limiting: "five_hour",
    windows: [
      { name: "five_hour", used: pct5 / 100, resetsAt: t + 5400 },
      { name: "seven_day", used: pct7 / 100, resetsAt: t + 86_400 * 3 },
    ],
  });
  putQuota(w.disk[LOCAL], [acct("work", 41, 18), acct("personal", 12, 30)]);
  return w;
}

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
    await click(await byText(".history-view button", copyText("history.row.forks", { n: 2 })));
    await sleep(300);
  }),
  {
    ...hist("history-05-resume-menu", "历史 · 恢复菜单", "右边内容头的［恢复 ▾］点开 ▾：「账号」（每个号后面 5h 用量，上次用的标「上次」）与「运行于」两组单选，底下「在此目录新建会话」", async () => {
      await openHistory();
      await click(await byText('.history-view [role="option"]', "支付回调验签"));
      await sleep(900);
      await click(await waitFor('.history-view [data-role="resume"] button[aria-haspopup="menu"]'));
      await sleep(600);
    }),
    world: withQuota,
  },
  {
    ...hist("history-resume-no-terminal", "历史 · 恢复 · 找不到终端", "本机那条已结束的会话点［恢复］，这台一个终端都没探到：右下「未找到终端」＋［设置］（直达 通用 → 终端）＋［复制命令］，不自动写剪贴板", async () => {
      await openHistory();
      await click(await byText('.history-view [role="option"]', "周报草稿"));
      await sleep(900);
      await click(await waitFor('.history-view [data-role="resume"] button:not([aria-haspopup])'));
      await sleep(900);
    }),
    world: () => {
      const w = defaultWorld();
      w.ops = { ...w.ops, "launch-local": () => ({ cmd: "ccm --resume s3", account: null }) };
      w.commands = { ...w.commands, open_local_terminal: () => "noWindow" };
      w.quiet = [...(w.quiet ?? []), "terminal-name-mint"];
      return w;
    },
  },
  {
    ...hist("history-05b-resume-picked", "历史 · 恢复菜单 · 挪了勾", "▾ 里点 personal、点「在 tmux 里」：只挪勾、菜单不关；主按钮按勾着的那一组起", async () => {
      await openHistory();
      await click(await byText('.history-view [role="option"]', "支付回调验签"));
      await sleep(900);
      await click(await waitFor('.history-view [data-role="resume"] button[aria-haspopup="menu"]'));
      await sleep(500);
      await click(await byText('[role="menuitemradio"]', /personal/));
      await click(await byText('[role="menuitemradio"]', "在 tmux 里"));
      await sleep(300);
    }),
    world: withQuota,
  },
  {
    ...hist("tab-resume-menu", "标签页 · 右键「恢复 ▸」", "已结束的标签页右键「恢复 ▸」：与历史页同一个组件，最上面多一行「恢复」（标签页没有主按钮）", async () => {
      await mainReady(ALL_TABS);
      const tab = await byText("#tab-bar .tab", "周报草稿");
      const r = tab.getBoundingClientRect();
      tab.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + 10, button: 2 }));
      await sleep(900);
      const wrap = [...document.querySelectorAll<HTMLElement>('[role="menu"] > [role="none"]')].find((w) => w.textContent?.includes("Resume"));
      wrap?.querySelector<HTMLElement>(":scope > button")?.click();
      await sleep(500);
    }),
    world: withQuota,
  },
  hist("history-06-codex", "历史 · Codex 会话", "Codex 会话：恢复按它自己那一家起；▾ 里不问账号，灰一行「Codex · 无账号维」", async () => {
    await openHistory();
    await click(await byText('.history-view [role="option"]', "Codex"));
    await sleep(900);
    await click(await waitFor('.history-view [data-role="resume"] button[aria-haspopup="menu"]'));
    await sleep(500);
  }),
  hist("history-18-empty", "历史 · 0 条消息的会话", "点开一个没有消息的会话（排序模型评测脚本，Codex）：消息流那一格是空态「无消息」，不是一片空白", async () => {
    await openHistory();
    await click(await byText('.history-view [role="option"]', "排序模型评测脚本"));
    await sleep(900);
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
    ...hist("history-14-mid", "历史 · 中档 1000 宽", "800–1100：列表 340、不可拖；内容头第二行折行；「你说过的话」只剩图标 ＋ 数字", async () => {
      await openHistory();
      await click(await row(0));
      await sleep(900);
    }),
    width: 1000,
    height: 760,
  },
  {
    ...hist("history-15-narrow-list", "历史 · 窄档列表", "< 800：单块，只有列表；「筛选」只剩图标", async () => {
      await openHistory();
    }),
    width: 720,
    height: 560,
  },
  {
    ...hist("history-16-narrow-open", "历史 · 窄档点开一个", "< 800：点一行 ⇒ 内容盖满，头左端「← 列表」（Esc 也回列表）", async () => {
      await openHistory();
      await click(await row(0));
      await sleep(900);
    }),
    width: 720,
    height: 560,
  },
  {
    id: "viewer-window",
    page: "viewer",
    query: "viewer=5e550001-0000-4000-8000-000000000001",
    dir: "历史",
    title: "独立查看窗 · 在跑",
    desc: "一个会话另开在独立窗口里：细顶栏 · 左边「你说过的话」一栏 · 消息流跟着长 · 状态栏「只读 · N 条 · 实时」",
    width: 900,
    height: 720,
    world: defaultWorld,
    act: async () => {
      await waitFor(".session-viewer [data-id]", 15_000);
      await sleep(1500);
    },
  },
  {
    id: "viewer-expand-fetch-loading",
    page: "viewer",
    query: "viewer=5e550001-0000-4000-8000-000000000001",
    dir: "历史",
    title: "独立查看窗 · 展开取回 · 读取中",
    desc: "桌面读正文交「折起那一行」声明：工具结果正文不随行来，点开那一下按记录 id 去取 —— 取回之前写「读取中」",
    width: 900,
    height: 720,
    world: slowFull,
    act: async () => {
      const res = await openLastResult();
      await sleep(600);
      if (!res.querySelector(".block-omitted")) throw new Error("点开之后没出「读取中」—— 正文没被省掉，或取得太快");
    },
  },
  {
    id: "viewer-expand-fetch-landed",
    page: "viewer",
    query: "viewer=5e550001-0000-4000-8000-000000000001",
    dir: "历史",
    title: "独立查看窗 · 展开取回 · 落全文",
    desc: "同上，取回之后「读取中」换成那一块全文",
    width: 900,
    height: 720,
    world: slowFull,
    act: async () => {
      const res = await openLastResult();
      await sleep(3600);
      if (res.querySelector(".block-omitted")) throw new Error("等了 3.6 秒还在「读取中」/ 取失败");
    },
  },
  {
    id: "viewer-window-ended",
    page: "viewer",
    query: "viewer=5e550003-0000-4000-8000-000000000003",
    dir: "历史",
    title: "独立查看窗 · 已结束",
    desc: "已结束的会话也开得了：［恢复 ▾］· 状态栏「只读 · N 条 · 已结束」",
    width: 900,
    height: 720,
    world: defaultWorld,
    act: async () => {
      await waitFor(".session-viewer [data-id]", 15_000);
      await sleep(1200);
    },
  },
  {
    id: "viewer-window-narrow",
    page: "viewer",
    query: "viewer=5e550001-0000-4000-8000-000000000001",
    dir: "历史",
    title: "独立查看窗 · 窄于 760",
    desc: "窄于 760：左边那一栏收起，顶栏左端那颗按钮开它",
    width: 600,
    height: 560,
    world: defaultWorld,
    act: async () => {
      await waitFor(".session-viewer [data-id]", 15_000);
      await sleep(1200);
    },
  },
];
