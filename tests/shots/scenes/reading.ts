/**
 * 读记录中（会话认出来就宣告、记录还在后台预读）：侧栏行 · 会话头 · 悬停卡 · 窄栏。
 * 照设计稿 `主窗口/读记录中.md`：界面照宣告那一帧画（字 ＝ 后端写的「读记录中」，语气 plain），消息流在读完之前是空的。
 *
 * 只有「开着即活」那一家（没有进程状态文件，此刻在干什么只有记录说）宣告时判不出、要等记录读完：世界里给本机那条 Codex 会话
 * 配一个替身开着它的 rollout 写（后端认写者、宣告），再把它的预读演成一直没读完（`MachineDisk.primeHeld`：宣告之后的格不往壳那一跳交）。
 * 夹具只造结构与中性的项目名。
 */
import type { Scene } from "./index";
import type { SessionSpec, World } from "../fake/types";
import { Convo } from "../fake/records";
import { defaultWorld, LOCAL, session } from "../fake/world";
import { CODEX_SID, codexRolloutRel, codexSaidRollout } from "../fake/history";
import { mainReady, sleep } from "./helpers";
import { copyText } from "../../../src/frontend/ui/copy-table";

/**
 * 截之前核一句：那条会话此刻真是「读记录中」（后端宣告那一帧写的字）。预读没扣住（读完了、改口说判出来的那一态）⇒ 这一张不算数，
 * 当场报出来，不截一张样子对、状态不对的图。
 */
function mustSayReading(where: string, text: string | null | undefined): void {
  const word = copyText("beSession.activity.reading");
  if (!text?.includes(word)) throw new Error(`${where}没写「${word}」：预读没扣住（${(text ?? "").slice(0, 120)}）`);
}

interface Row {
  cwd: string;
  title: string;
  over?: Partial<SessionSpec>;
}

const ROWS: Row[] = [
  { cwd: "/home/user/work/orders", title: "订单服务加重试", over: { activity: "working" } },
  { cwd: "/home/user/work/orders", title: "订单接口补测试", over: { activity: "idle" } },
  { cwd: "/home/user/work/docs", title: "部署文档", over: { activity: "idle" } },
];
/** 三条 Claude 会话 ＋ 那条在读记录的 Codex 会话。 */
const N = ROWS.length + 1;

function readingWorld(): World {
  const w = defaultWorld();
  w.sessions = ROWS.map((r, i) => {
    const sid = `5eed${(i + 1).toString(16).padStart(4, "0")}-0000-4000-8000-00000000${(i + 1).toString(16).padStart(4, "0")}`;
    const c = new Convo(sid, r.cwd, new Date(Date.now() - 2 * 3600_000).toISOString());
    c.title(r.title);
    c.user(`${r.title}：开始。`);
    c.say("好的，先看一下现状。", 20_000);
    return session(300 + i, LOCAL, r.cwd, c, { sid, ...r.over });
  });
  const now = Date.now();
  const rel = codexRolloutRel(now);
  const d = w.disk[LOCAL];
  w.disk[LOCAL] = { ...d, files: { ...d.files, [rel]: codexSaidRollout(now) }, writers: [rel], primeHeld: [CODEX_SID] };
  return w;
}

function readingTab(): HTMLElement {
  const sid8 = CODEX_SID.slice(0, 8);
  for (const el of document.querySelectorAll<HTMLElement>("#tab-bar .tab")) {
    if (el.dataset.sid === CODEX_SID || el.textContent?.includes(sid8) || el.textContent?.includes("ranker")) return el;
  }
  throw new Error("没有那条在读记录的 Codex 会话的标签页");
}

function rd(id: string, title: string, desc: string, act: Scene["act"], size: [number, number] = [1280, 800]): Scene {
  return { id, page: "index", dir: "读记录中", title, desc, width: size[0], height: size[1], world: readingWorld, act };
}

export const READING_SCENES: Scene[] = [
  rd("rd-00-row-head", "侧栏行 · 会话头 · 消息流", "当前是那条在读记录的 Codex 会话：行上灰实心点、行尾不写字；会话头那一句「读记录中」；消息流在读完之前是空的", async () => {
    await mainReady(N);
    readingTab().click();
    await sleep(600);
    mustSayReading("会话头", document.getElementById("session-head")?.textContent);
  }),
  rd("rd-01-hover", "悬停卡", "悬停那条在读记录的会话：状态一句「读记录中」，没有 peek（记录还没读）", async () => {
    await mainReady(N);
    readingTab().dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    await sleep(800);
    mustSayReading("悬停卡", document.querySelector<HTMLElement>("[role=tooltip]")?.textContent ?? document.body.textContent);
  }),
  rd("rd-02-narrow", "窄栏（44px）", "900 宽：每行只剩点，那条在读记录的是灰实心点（同闲着）", async () => {
    await mainReady(N);
    await sleep(300);
  }, [900, 700]),
];
