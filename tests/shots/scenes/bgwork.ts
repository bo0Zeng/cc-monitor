/**
 * 后台任务运行中（一轮停了、后台命令还在跑）：侧栏行 · 会话头 · 悬停卡 · 收着的组头 · 窄栏 · 监控板。
 * 照设计稿 `主窗口/后台在跑.md` 那几节，名字与稿一致（夹具只造结构与中性的项目名）。
 */
import type { Scene } from "./index";
import type { SessionSpec, World } from "../fake/types";
import { Convo } from "../fake/records";
import { defaultWorld, LOCAL, session } from "../fake/world";
import { click, mainReady, sleep, waitFor } from "./helpers";

const MIN = 60_000;

interface Row {
  origin: string;
  cwd: string;
  title: string;
  over?: Partial<SessionSpec>;
}

const ROWS: Row[] = [
  { origin: LOCAL, cwd: "/home/user/work/orders", title: "订单服务加重试", over: { activity: "working" } },
  { origin: LOCAL, cwd: "/home/user/work/orders", title: "订单接口补测试", over: { activity: "needs_you", waitingFor: "permission prompt", waitingSinceMs: Date.now() - 90_000 } },
  { origin: LOCAL, cwd: "/home/user/work/orders", title: "全量回归", over: { activity: "background_work", bgCommands: [{ cmd: "make test-all", sinceMs: Date.now() - 12 * MIN - 20_000 }] } },
  {
    origin: "gpu-01",
    cwd: "/data/train/ranker",
    title: "排序模型训练",
    over: {
      activity: "background_work",
      bgCommands: [
        { cmd: "python train.py --epochs 40", sinceMs: Date.now() - 64 * MIN - 10_000 },
        { cmd: "tensorboard --logdir runs", sinceMs: Date.now() - 60 * MIN },
      ],
    },
  },
  { origin: "gpu-01", cwd: "/data/train/ranker", title: "评测脚本", over: { activity: "idle" } },
  { origin: "devbox", cwd: "/srv/app/gateway", title: "网关限流", over: { activity: "idle" } },
  { origin: LOCAL, cwd: "/home/user/work/docs", title: "部署文档", over: { activity: "idle" } },
];
const N = ROWS.length;

function bgWorld(folded: readonly string[] = []): World {
  const w = defaultWorld();
  w.sessions = ROWS.map((r, i) => {
    const sid = `7ab1${(i + 1).toString(16).padStart(4, "0")}-0000-4000-8000-00000000${(i + 1).toString(16).padStart(4, "0")}`;
    const c = new Convo(sid, r.cwd, "2026-10-08T09:00:00Z");
    c.title(r.title);
    c.user(`${r.title}：开始。`);
    c.say(r.over?.activity === "background_work" ? "已在后台起了命令，跑完再看结果。" : "好的，先看一下现状。", 20_000);
    return session(200 + i, r.origin, r.cwd, c, { sid, ...r.over });
  });
  const sid = (i: number): string => w.sessions[i].sid;
  w.config = {
    ...w.config,
    tabCollections: [
      { id: "g1", name: "订单" },
      { id: "g2", name: "训练" },
    ].map((g) => (folded.includes(g.id) ? { ...g, collapsed: true } : g)),
    tabBar: { groupOf: { [sid(0)]: "g1", [sid(1)]: "g1", [sid(2)]: "g1", [sid(3)]: "g2", [sid(4)]: "g2" } },
  };
  return w;
}

function tabOf(title: string): HTMLElement {
  for (const el of document.querySelectorAll<HTMLElement>("#tab-bar .tab")) if (el.textContent?.includes(title)) return el;
  throw new Error(`没有标签页「${title}」`);
}

function bw(id: string, title: string, desc: string, act: Scene["act"], world: () => World = () => bgWorld(), size: [number, number] = [1280, 800]): Scene {
  return { id, page: "index", dir: "后台任务运行中", title, desc, width: size[0], height: size[1], world, act };
}

export const BGWORK_SCENES: Scene[] = [
  bw("bw-00-row-head", "侧栏行 · 会话头", "当前是「全量回归」（后台任务运行中）：行上只换点（淡紫、不呼吸、行尾不写字）；会话头那一句「后台任务运行中 · make test-all · 12m」", async () => {
    await mainReady(N);
    await click(tabOf("全量回归"));
    await sleep(600);
  }),
  bw("bw-01-hover", "悬停卡", "悬停「排序模型训练」：状态句「后台任务运行中 · python train.py --epochs 40 等 2 条 · 1h4m」，peek 照空闲（它最后一句）", async () => {
    await mainReady(N);
    await click(tabOf("全量回归"));
    await sleep(300);
    tabOf("排序模型训练").dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    await sleep(800);
  }),
  bw("bw-02-group-folded", "收着的组头", "「订单」「训练」两组收着：组头汇总「需要你 1 · 在跑 1 · 后台任务 1」与「后台任务 1」；展开的组头不汇总", async () => {
    await mainReady(N);
    await click(tabOf("部署文档"));
    await sleep(300);
    const caret = document.querySelector<HTMLElement>(".tab-group-caret[aria-expanded=true]");
    if (caret) await click(caret);
    await sleep(400);
  }, () => bgWorld(["g2"])),
  bw("bw-03-narrow", "窄栏（44px）", "900 宽：每行只剩点，「全量回归」「排序模型训练」是淡紫点；收着的「训练」组分隔上的小点（需手动 ＞ 运行中 ＞ 后台任务运行中）", async () => {
    await mainReady(N);
    await sleep(300);
  }, () => bgWorld(["g2"]), [900, 700]),
  bw("bw-04-grid", "监控板", "监控板：kit 状态点（空闲是灰、不再是红）· 后台任务运行中那一格多一枚「后台任务 · make test-all」· 组内排序 需手动 ＞ 运行中 ＞ 后台任务运行中 ＞ 空闲", async () => {
    await mainReady(N);
    await click(".grid-monitor-trigger");
    await waitFor(".grid-monitor");
    await sleep(600);
  }),
];
