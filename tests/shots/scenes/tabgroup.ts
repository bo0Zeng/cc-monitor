/**
 * 标签页分组：十几个标签页、三个组，真拖一遍（排序 · 压住建组 · 进组 / 出组 · 组头 · 多选 · 窄栏 · 右键）。
 * 拖拽用真 DOM 事件：在 tab 上 mousedown，document 上 mousemove（带 buttons=1），要「落下之后」的图再 mouseup。
 */
import type { Scene } from "./index";
import type { SessionSpec, World } from "../fake/types";
import { Convo } from "../fake/records";
import { defaultWorld, LOCAL, session } from "../fake/world";
import { byText, click, key, mainReady, rightClick, sleep, waitFor } from "./helpers";

const W = 1280;
const H = 800;
const N = 14;

interface Row {
  origin: string;
  cwd: string;
  title: string;
  over?: Partial<SessionSpec>;
}

const ROWS: Row[] = [
  { origin: LOCAL, cwd: "/home/user/work/orders", title: "订单服务加重试", over: { activity: "working" } },
  { origin: LOCAL, cwd: "/home/user/work/orders", title: "订单接口补测试", over: { activity: "needs_you", waitingFor: "permission prompt", waitingSinceMs: Date.now() - 90_000 } },
  { origin: LOCAL, cwd: "/home/user/work/orders", title: "下单页文案", over: { activity: "idle", ended: true } },
  { origin: "gpu-01", cwd: "/data/train/ranker", title: "排序模型训练", over: { activity: "working" } },
  { origin: "gpu-01", cwd: "/data/train/ranker", title: "评测脚本", over: { activity: "idle" } },
  { origin: "devbox", cwd: "/srv/app/gateway", title: "网关限流", over: { activity: "idle" } },
  { origin: "devbox", cwd: "/srv/app/billing", title: "账单导出改流式", over: { activity: "working" } },
  { origin: LOCAL, cwd: "/home/user/work/notes", title: "周报草稿", over: { activity: "idle", ended: true } },
  { origin: LOCAL, cwd: "/home/user/work/web-console", title: "表格虚拟滚动", over: { activity: "idle" } },
  { origin: LOCAL, cwd: "/home/user/work/web-console", title: "暗色主题", over: { activity: "working" } },
  { origin: "win-laptop", cwd: "C:\\Users\\user\\work\\desktop-app", title: "安装包签名", over: { activity: "idle" } },
  { origin: LOCAL, cwd: "/home/user/work/docs", title: "部署文档", over: { activity: "idle" } },
  { origin: "devbox", cwd: "/srv/app/search", title: "搜索索引重建", over: { activity: "needs_you", waitingFor: "permission prompt", waitingSinceMs: Date.now() - 30_000 } },
  { origin: LOCAL, cwd: "/home/user/work/cli", title: "命令行补全", over: { activity: "idle" } },
];

function tabWorld(groups = true, folded: readonly string[] = []): World {
  const w = defaultWorld();
  w.sessions = ROWS.map((r, i) => {
    const sid = `7ab0${(i + 1).toString(16).padStart(4, "0")}-0000-4000-8000-00000000${(i + 1).toString(16).padStart(4, "0")}`;
    const c = new Convo(sid, r.cwd, "2026-10-08T09:00:00Z");
    c.title(r.title);
    c.user(`${r.title}：开始。`);
    c.say("好的，先看一下现状。", 20_000);
    return session(100 + i, r.origin, r.cwd, c, { sid, ...r.over });
  });
  if (groups) {
    const sid = (i: number): string => w.sessions[i].sid;
    w.config = {
      ...w.config,
      tabCollections: [
        { id: "g1", name: "订单" },
        { id: "g2", name: "训练" },
        { id: "g3", name: "网关" },
      ].map((g) => (folded.includes(g.id) ? { ...g, collapsed: true } : g)),
      tabBar: {
        groupOf: { [sid(0)]: "g1", [sid(1)]: "g1", [sid(2)]: "g1", [sid(3)]: "g2", [sid(4)]: "g2", [sid(5)]: "g3", [sid(6)]: "g3" },
      },
    };
  }
  return w;
}

/** 按标题找标签页那一行。 */
function tabOf(title: string): HTMLElement {
  for (const el of document.querySelectorAll<HTMLElement>("#tab-bar .tab")) if (el.textContent?.includes(title)) return el;
  throw new Error(`没有标签页「${title}」`);
}

const mid = (el: Element): { x: number; y: number } => {
  const r = el.getBoundingClientRect();
  return { x: r.left + Math.min(60, r.width / 2), y: r.top + r.height / 2 };
};

/** 按下一个标签页、拖到 (x, y)；分几步走，让拖拽状态机越过阈值。 */
async function dragTo(from: HTMLElement, x: number, y: number): Promise<void> {
  const s = mid(from);
  from.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true, clientX: s.x, clientY: s.y, button: 0, buttons: 1 }));
  const steps = 8;
  for (let i = 1; i <= steps; i++) {
    const cx = s.x + ((x - s.x) * i) / steps;
    const cy = s.y + ((y - s.y) * i) / steps;
    document.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, cancelable: true, clientX: cx, clientY: cy, button: 0, buttons: 1 }));
    await sleep(16);
  }
}

/** 指针停在 (x, y) 附近来回抖 5px（不让停留计时攒满）：截「只插不建组」的那一刻用。截完页就关，计时器不用收。 */
function keepMoving(x: number, y: number): void {
  let k = 0;
  window.setInterval(() => {
    k++;
    document.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, cancelable: true, clientX: x + (k % 2 ? 5 : 0), clientY: y, button: 0, buttons: 1 }));
  }, 60);
}

async function drop(x: number, y: number): Promise<void> {
  document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0, buttons: 0 }));
  await sleep(400);
}

function tg(id: string, title: string, desc: string, act: Scene["act"], world: () => World = () => tabWorld(), size: [number, number] = [W, H]): Scene {
  return { id, page: "index", dir: "标签页分组", title, desc, width: size[0], height: size[1], world, act };
}

export const TABGROUP_SCENES: Scene[] = [
  tg("tg-00-today", "三个组 ＋ 散的", "14 个标签页：订单（3）· 训练（2）· 网关（2）＋ 7 个散的", async () => {
    await mainReady(N);
  }),
  tg("tg-01-insert", "拖着排序（插入线）", "把「部署文档」拖到「表格虚拟滚动」与「暗色主题」之间，没松手", async () => {
    await mainReady(N);
    const to = tabOf("暗色主题").getBoundingClientRect();
    await dragTo(tabOf("部署文档"), to.left + 60, to.top + 4);
    keepMoving(to.left + 60, to.top + 4);
    await sleep(300);
  }),
  tg("tg-02-dwell-loose", "压在散的中间停住（要建组）", "「部署文档」压在「命令行补全」中间停住", async () => {
    await mainReady(N);
    const p = mid(tabOf("命令行补全"));
    await dragTo(tabOf("部署文档"), p.x, p.y);
    await sleep(700);
  }),
  tg("tg-03-found", "松手建组之后", "上一张松手：组就在目标那一格，名字框已打开；可撤的 toast", async () => {
    await mainReady(N);
    const p = mid(tabOf("命令行补全"));
    await dragTo(tabOf("部署文档"), p.x, p.y);
    await sleep(700);
    await drop(p.x, p.y);
  }),
  tg("tg-04-dwell-member", "压在组员上停住（要进组）", "「搜索索引重建」压在网关组的「网关限流」中间停住", async () => {
    await mainReady(N);
    const p = mid(tabOf("网关限流"));
    await dragTo(tabOf("搜索索引重建"), p.x, p.y);
    await sleep(700);
  }),
  tg("tg-05-group-tail", "放到「训练」组末尾", "把「表格虚拟滚动」拖到「评测脚本」下 1/4", async () => {
    await mainReady(N);
    const r = tabOf("评测脚本").getBoundingClientRect();
    await dragTo(tabOf("表格虚拟滚动"), r.left + 60, r.bottom - 3);
    keepMoving(r.left + 60, r.bottom - 3);
    await sleep(300);
  }),
  tg("tg-06-group-tail-drop", "上一张松手", "「表格虚拟滚动」进的是哪个组", async () => {
    await mainReady(N);
    const r = tabOf("评测脚本").getBoundingClientRect();
    await dragTo(tabOf("表格虚拟滚动"), r.left + 60, r.bottom - 3);
    await drop(r.left + 60, r.bottom - 3);
  }),
  tg("tg-07-head-drag", "拖组头", "在「训练」组头上按下往下拖", async () => {
    await mainReady(N);
    const head = [...document.querySelectorAll<HTMLElement>(".tab-group-head")].find((h) => h.textContent?.includes("训练"))!;
    const r = head.getBoundingClientRect();
    await dragTo(head, r.left + 40, r.top + 200);
  }),
  tg("tg-08-out-blank", "拖出组到栏底空白", "「订单接口补测试」拖到栏底空白处，没松手", async () => {
    await mainReady(N);
    const bar = document.querySelector("#tab-bar")!.getBoundingClientRect();
    await dragTo(tabOf("订单接口补测试"), bar.left + 80, bar.bottom - 60);
    keepMoving(bar.left + 80, bar.bottom - 60);
    await sleep(300);
  }),
  tg("tg-09-out-window", "往右拖出栏（主区里）", "「周报草稿」拖到消息流中间：松开在新窗口打开", async () => {
    await mainReady(N);
    await dragTo(tabOf("周报草稿"), 640, 300);
    keepMoving(640, 300);
    await sleep(300);
  }),
  tg("tg-10-multi", "多选三个后拖", "Ctrl 选了三个散的，拖其中一个", async () => {
    await mainReady(N);
    for (const t of ["部署文档", "命令行补全", "安装包签名"]) {
      const el = tabOf(t);
      const p = mid(el);
      el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, clientX: p.x, clientY: p.y, ctrlKey: true }));
      await sleep(80);
    }
    const p = mid(tabOf("订单接口补测试"));
    await dragTo(tabOf("部署文档"), p.x, p.y);
    await sleep(700);
  }),
  tg("tg-11-menu", "右键里的分组项", "右键「部署文档」：「加入分组 ▸」子菜单", async () => {
    await mainReady(N);
    await rightClick(tabOf("部署文档"));
    const item = await byText("[aria-haspopup='menu']", "分组");
    item.click();
    await sleep(400);
  }),
  tg("tg-12-menu-member", "组员的右键", "右键「订单接口补测试」（在「订单」组里）", async () => {
    await mainReady(N);
    await rightClick(tabOf("订单接口补测试"));
    await sleep(300);
  }),
  tg("tg-13-rename", "组头改名", "点「训练」组名：就地输入框", async () => {
    await mainReady(N);
    const name = [...document.querySelectorAll<HTMLElement>(".tab-group-name")].find((h) => h.textContent?.includes("训练"))!;
    name.click();
    await sleep(300);
  }),
  tg("tg-14-head-hover", "悬停组头", "悬停「订单」组头：右侧出「⋯」", async () => {
    await mainReady(N);
    const head = document.querySelector<HTMLElement>(".tab-group-head")!;
    head.style.background = "var(--state-hover)";
    head.querySelector<HTMLElement>(".tab-group-more")!.style.visibility = "visible";
    await sleep(200);
  }),
  tg("tg-15-dissolve", "解散组之后", "「订单」组头「⋯」→ 解散分组：组员留在原位变散的，toast 可撤", async () => {
    await mainReady(N);
    await click(document.querySelector<HTMLElement>(".tab-group-more")!);
    await click(await byText("[role=menu] [role^=menuitem]", "解散分组"));
    await sleep(400);
  }),
  tg("tg-16-narrow", "窄窗（栏收成 44px）", "900 宽", async () => {
    await mainReady(N);
    await sleep(300);
  }, () => tabWorld(), [900, 700]),
  tg("tg-17-short", "栏放不下（拖时滚动）", "窗高 520：14 行放不下，拖到栏底边上停住", async () => {
    await mainReady(N);
    const bar = document.querySelector("#tab-bar")!.getBoundingClientRect();
    await dragTo(tabOf("订单服务加重试"), bar.left + 80, bar.bottom - 4);
    await sleep(600);
  }, () => tabWorld(), [1280, 520]),
  tg("tg-18-key", "键盘", "焦点在「部署文档」上按 Shift+F10", async () => {
    await mainReady(N);
    tabOf("部署文档").focus();
    await key("F10", { shift: true, code: "F10" });
    await sleep(300);
  }),
  tg("tg-19-cancel", "往下拖出栏（取消）", "「部署文档」拖到窗口底边外：影子写「松开取消」、不画插入线", async () => {
    await mainReady(N);
    await dragTo(tabOf("部署文档"), 120, H - 12);
    keepMoving(120, H - 12);
    await sleep(300);
  }),
  tg("tg-20-head-drop", "落在组头（进组排第一）", "「命令行补全」拖到「训练」组头下半：组头整块高亮、小标签「进「训练」」", async () => {
    await mainReady(N);
    const head = [...document.querySelectorAll<HTMLElement>(".tab-group-head")].find((h) => h.textContent?.includes("训练"))!;
    const r = head.getBoundingClientRect();
    await dragTo(tabOf("命令行补全"), r.left + 60, r.bottom - 6);
    keepMoving(r.left + 60, r.bottom - 6);
    await sleep(300);
  }),
  tg("tg-21-after-group", "放到组后面（组外）", "「部署文档」拖到「网关」组下沿：插入线通栏、不点亮组", async () => {
    await mainReady(N);
    const r = tabOf("账单导出改流式").getBoundingClientRect();
    await dragTo(tabOf("部署文档"), r.left + 60, r.bottom + 3);
    keepMoving(r.left + 60, r.bottom + 3);
    await sleep(300);
  }),
  tg("tg-22-collapsed", "收着的组", "「训练」「网关」收着：组头写组员数，后面汇总在等你 / 在跑", async () => {
    await mainReady(N);
    await sleep(200);
  }, () => tabWorld(true, ["g2", "g3"])),
  tg("tg-23-head-menu", "组头「⋯」菜单", "点「订单」组头的「⋯」", async () => {
    await mainReady(N);
    await click(document.querySelector<HTMLElement>(".tab-group-more")!);
    await sleep(300);
  }),
  tg("tg-24-narrow-collapsed", "窄栏 · 收着的组", "900 宽、「训练」「网关」收着：组头收成「›」＋ 最要紧的点 ＋ 组员数", async () => {
    await mainReady(N);
    await sleep(300);
  }, () => tabWorld(true, ["g2", "g3"]), [900, 700]),
  tg("tg-25-current-collapsed", "当前标签页在收着的组里", "切到「排序模型训练」再收起「训练」：组头左边一道 accent", async () => {
    await mainReady(N);
    await click(tabOf("排序模型训练"));
    const head = [...document.querySelectorAll<HTMLElement>(".tab-group-head")].find((h) => h.textContent?.includes("训练"))!;
    await click(head.querySelector<HTMLElement>(".tab-group-caret")!);
    await sleep(200);
  }),
  tg("tg-26-drop-collapsed", "落在收着的组头上", "「命令行补全」拖到收着的「网关」组头：组头高亮，进组排最后、组保持收着", async () => {
    await mainReady(N);
    const head = [...document.querySelectorAll<HTMLElement>(".tab-group-head")].find((h) => h.textContent?.includes("网关"))!;
    const r = head.getBoundingClientRect();
    await dragTo(tabOf("命令行补全"), r.left + 60, r.top + r.height / 2);
    keepMoving(r.left + 60, r.top + r.height / 2);
    await sleep(300);
  }, () => tabWorld(true, ["g2", "g3"])),
];

void waitFor;
