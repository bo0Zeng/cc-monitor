/**
 * 发附件 · 一组起会话 · 标签页改名这三块各一组图：附件那一排（chip · ［送出］悬停）· 消息流里带附件的那一句 ·
 * 组菜单 · 一组页（上限 · 重行 · 删组）· 单个页选了别名 · F2 就地框 · 右键菜单 · 对话框。
 * 附件那几条帧命令不在真后端那一份里 ⇒ 这里合成答法（结构占位，句子照文案表里后端那几条的样子写）；`set-plan` 交真后端答。
 */
import type { Scene } from "./index";
import { Refuse, type World } from "../fake/types";
import { defaultWorld, LOCAL } from "../fake/world";
import { Convo } from "../fake/records";
import { REAL_OPS } from "../fake/backend";
import { PROFILES_FILE, putProfileAccounts, staleBook } from "../disk/profiles";
import { byText, click, hover, key, mainReady, rightClick, sleep, type, waitFor } from "./helpers";

// 补足一组的样子由真后端照台架盘上的配置文件答（起会话那一条不放：沙箱里不起 claude）。
REAL_OPS.add("set-plan");

const W = 1280;
const H = 800;
const STORAGE = { "cc-monitor.tab-bar-w": "260", "cc-monitor.cmdk-hint.seen": "1" };

/** 一张 96×60 的小图（两色格子）。 */
const PNG_B64 =
  "iVBORw0KGgoAAAANSUhEUgAAAGAAAAA8CAIAAAAWtijjAAAAgElEQVR42u3ZsQkAIBADwJ/O2traSawd2iWCiBxkgPyV+Rp9RdLmjuS1PgUIECBAgAABAgQIECBAgAABAgQIECBAgGJAvx6W6gMIECBAgAABAgQIECBAgAABAgQIECBAgHJARntfDUCAAAECBAgQIECAAAECBAgQIECAAAG61OcA77TQB+DA8O4AAAAASUVORK5CYII=";

function pngBytes(): Uint8Array {
  const s = atob(PNG_B64);
  const b = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) b[i] = s.charCodeAt(i);
  return b;
}

function file(name: string, bytes: number): File {
  if (name.endsWith(".png")) return new File([pngBytes()], name, { type: "image/png" });
  return new File([new Uint8Array(bytes)], name, { type: "application/octet-stream" });
}

// ---------- 发附件 ----------

const CLASS: Record<string, [string, string]> = { png: ["image", "图片"], css: ["code", "代码"], pdf: ["pdf", "PDF"], log: ["text", "文本"] };
const classOf = (name: string): [string, string] => CLASS[name.split(".").pop() ?? ""] ?? ["other", "文件"];

/** 附件那几口：按加进来的先后认是哪一份；`hangAt` ⇒ 那一份第几块起不再答（传输中）；`fail` ⇒ 那一份传到第 2 块那台说的那一句。 */
function attachOps(w: World, order: string[], opt: { hangAt?: Record<string, number>; fail?: Record<string, string> } = {}): void {
  const byKey = new Map<string, string>();
  let n = 0;
  w.ops["session-attachment-check"] = (_o, req) => ({
    items: (req.items as { name: string }[]).map((it) => {
      const [cls, text] = classOf(it.name);
      return { ok: true, class: cls, class_text: text };
    }),
  });
  w.ops["files-stage-chunk"] = (_o, req) => {
    const k = String(req.key);
    if (!byKey.has(k)) byKey.set(k, order[byKey.size] ?? "");
    const cur = byKey.get(k) ?? "";
    const seq = Number(req.seq);
    if (opt.fail?.[cur] !== undefined && seq >= 1) throw new Refuse("io", opt.fail[cur]);
    const at = opt.hangAt?.[cur];
    if (at !== undefined && seq >= at) return new Promise(() => {});
    return {};
  };
  w.ops["session-attachment-commit"] = (_o, req) => {
    const name = String(req.name);
    const id = `1028-1200-${(++n).toString(16).padStart(4, "0")}`;
    return { id, name, path: `/home/user/work/orders/.ccm-attach/${id}/${name}`, rel: `.ccm-attach/${id}/${name}`, bytes: Number(req.bytes), sha256: "0".repeat(64), class: classOf(name)[0] };
  };
  w.ops["session-attachment-drop"] = () => ({});
}

/** 会话都当活着闲着（收场那一段这几张不演）。 */
function liveWorld(): World {
  const w = defaultWorld();
  for (const s of w.sessions) {
    s.ended = false;
    s.idle = false;
  }
  return w;
}

/** 消息流底下那个输入框（附件那一排只住它；终端页的输入行只敲键）。 */
const COMPOSER = "[data-role=composer]";

async function openComposer(tabTitle?: string): Promise<void> {
  await mainReady(7);
  if (tabTitle) {
    await click(await byText("#tab-bar .tab", tabTitle));
    await sleep(800);
  }
  await waitFor(`${COMPOSER} textarea`);
  await sleep(800);
}

async function pick(files: File[]): Promise<void> {
  const inp = await waitFor<HTMLInputElement>(`${COMPOSER} [data-role=attach-plus] input[type=file]`);
  const dt = new DataTransfer();
  for (const f of files) dt.items.add(f);
  inp.files = dt.files;
  inp.dispatchEvent(new Event("change", { bubbles: true }));
  await sleep(1200);
}

function attachScene(id: string, title: string, desc: string, world: () => World, act: () => Promise<void>, storage: Record<string, string> = STORAGE): Scene {
  return { id, page: "index", dir: "发附件", title, desc, width: W, height: H, storage, world, act: async () => act() };
}

/** 消息流里带附件的那几句：图片块 ＋ 两份 @ 引用；另一句引的那一份那台已清掉；一句只有图片没有字。 */
function streamWorld(): World {
  const w = liveWorld();
  const s = w.sessions[0];
  const c = new Convo(s.sid, s.cwd, "2026-10-28T07:20:00Z");
  c.title("登录页窄屏错位");
  c.user("登录页在窄屏下按钮错位了，看下截图");
  c.say("好，发一下截图和那份样式表。");
  (c as unknown as { push: (t: string, s: number, b: unknown) => string }).push("user", 30, {
    message: {
      role: "user",
      content: [
        { type: "image", source: { type: "base64", media_type: "image/png", data: PNG_B64 } },
        { type: "text", text: "[Image #1] 截图在这，样式表也附上了\n@.ccm-attach/1028-1528-0001/login.css\n@.ccm-attach/1028-1528-0002/design-spec.pdf" },
      ],
    },
  });
  c.say("看到了：`.login-actions` 在 360px 以下没换行……");
  c.user("这份是上周那次的\n@.ccm-attach/1020-0910-0003/trace.log");
  c.say("那份已经不在了，我按今天的复现重新看。");
  s.convo = c.endAt(Date.now() - 60_000);
  s.activity = "idle";
  const d = w.disk[LOCAL];
  d.files["work/orders/.ccm-attach/1028-1528-0001/login.css"] = ".login-actions{display:flex}\n".repeat(400);
  d.files["work/orders/.ccm-attach/1028-1528-0002/design-spec.pdf"] = "%PDF-1.4\n".repeat(3000);
  return w;
}

const ATTACH_SCENES: Scene[] = [
  attachScene("compose-attach-row", "附件 · 一排三态", "［＋］选了三份：图片已完成 · css 传输中（只写词 ＋ 进度条）· pdf 排队；［送出］灰、悬停一句", () => {
    const w = liveWorld();
    attachOps(w, ["login-narrow.png", "login.css", "design-spec.pdf"], { hangAt: { "login.css": 3 } });
    return w;
  }, async () => {
    await openComposer();
    await type(`${COMPOSER} textarea`, "截图在这，样式表也附上了");
    await pick([file("login-narrow.png", 4700), file("login.css", 1_300_000), file("design-spec.pdf", 4_000_000)]);
    await hover(`${COMPOSER} [data-role=composer-send]`);
    await sleep(900);
  }),
  attachScene("compose-attach-offline", "附件 · 那台离线", "传字节那一跳断开 ⇒ 虚线 ＋ 连接层那一句；［送出］悬停也是那一句", () => {
    const w = liveWorld();
    attachOps(w, ["trace.png", "gateway.log"]);
    w.hopDown = { devbox: ["files-stage-chunk"] };
    return w;
  }, async () => {
    await openComposer("账单导出");
    await pick([file("trace.png", 4700), file("gateway.log", 3_100_000)]);
    await type(`${COMPOSER} textarea`, "上面这个 trace 是 p99 那次的");
    await hover(`${COMPOSER} [data-role=composer-send]`);
    await sleep(900);
  }),
  attachScene("compose-said-attach", "消息流 · 发出去的那一句", "附件画在气泡外、上方、靠右；那台已清掉的那份虚线「已清理」", streamWorld, async () => {
    await mainReady(7);
    await sleep(1200);
    (await waitFor("[data-role=said-attachments]")).scrollIntoView({ block: "center" });
    await sleep(600);
  }, STORAGE),
];

// ---------- 输入队 ----------

/** 每条活会话那台盘上一份队（五条排着；会话都在跑 ⇒ 投递那一边等着，不送）：名单由真后端照它答。 */
function queueWorld(): World {
  const w = liveWorld();
  const heads = ["跑完测试先别提交，等我看一眼 diff", "顺手把 CHANGELOG 补上，按模块分三段写", "README 里的端口也改一下", "重试那段抽成一个函数", "超时改成可配"];
  const ends = ["mobile", "monitor", "cc-bus", "mobile", "monitor"];
  const items = heads.map((text, i) => ({ seq: i + 1, id: `q-${i + 1}`, end: ends[i], atMs: 1_761_650_000_000 + i, arrivedMs: 1_761_650_000_000 + i, text, state: "queued" }));
  const book = JSON.stringify({ version: 1, nextSeq: items.length + 1, ends: {}, seen: items.map((it) => it.id), items, sent: [] });
  for (const s of w.sessions) {
    const d = w.disk[s.origin];
    if (s.ended || d === undefined) continue;
    w.disk[s.origin] = { ...d, files: { ...d.files, [`.cc-monitor/session-queue/${s.sid}.json`]: book } };
  }
  return w;
}

const QUEUE_SCENES: Scene[] = [
  {
    id: "compose-queue-many",
    page: "index",
    dir: "输入队",
    title: "输入队 · 五条",
    desc: "那台名单五条：那一排最多露三行、多的框里滚，不往上顶消息流；每条一行不写几点",
    width: W,
    height: H,
    storage: STORAGE,
    world: queueWorld,
    act: async () => {
      await mainReady(7);
      await waitFor('[data-role="queue-line"]');
      await sleep(1200);
    },
  },
];

// ---------- 一组起会话 ----------

const EXTRA = [
  "",
  "[dev]",
  'from = "cct"',
  'account = "work"',
  'tab-group = "开发"',
  "",
  "[look]",
  'from = "cct"',
  'account = "lab"',
  "ccm-tmux = true",
  'tab-group = "看图"',
  'say = "读 pb 图（pb dump），回答关于计划的问题，不改文件。"',
  "",
  '[set."工具链开发"]',
  "row = [",
  '  { cwd = "~/work/orders", alias = "dev", n = 2 },',
  '  { cwd = "~/work/pb", alias = "dev", n = 1, tab-group = "pb" },',
  '  { cwd = "~/work/cc-bus", alias = "look", n = 1 },',
  "]",
  "",
  '[set."周报"]',
  "row = [",
  '  { cwd = "~/work/notes", alias = "dev", n = 8 },',
  "]",
  "",
  '[set."ledger 评审"]',
  "row = [",
  '  { cwd = "~/work/ledger", alias = "dev", n = 2 },',
  '  { cwd = "~/work/ledger", alias = "rev", n = 1 },',
  "]",
  "",
].join("\n");

/** 只留本机：配置文件（典型那份 ＋ dev · look 两条 ＋ 三组）与几处目录。 */
function setsWorld(): World {
  const w = defaultWorld();
  const d = { ...w.disk[LOCAL], files: { ...w.disk[LOCAL].files } };
  putProfileAccounts(d);
  d.files[PROFILES_FILE] = staleBook() + EXTRA;
  for (const dir of ["work/orders", "work/pb", "work/cc-bus", "work/notes", "work/ledger"]) d.files[`${dir}/.keep`] = "";
  return { ...w, machines: [LOCAL], unseenMachines: [], sessions: w.sessions.filter((s) => s.origin === LOCAL && !s.ended), disk: { [LOCAL]: d } };
}

const declared = (): number => window.__shots?.sessions?.().declared ?? 1;

function setScene(id: string, title: string, desc: string, act: () => Promise<void>, height = H): Scene {
  return { id, page: "index", dir: "一组起会话", title, desc, width: W, height, world: setsWorld, act: async () => act() };
}

async function openSetPage(pick?: string): Promise<void> {
  await mainReady(declared());
  await click('[data-role="new-session"]');
  await waitFor("[role=dialog]");
  await click('[role=dialog] [data-page="set"]');
  await waitFor('[data-role="set-page"]');
  await sleep(800);
  if (pick) {
    await click('[data-role="set-switch"]');
    await sleep(300);
    await click(await byText("[role=menu] [role^=menuitem]", pick));
    await sleep(2000);
  }
}

const SET_SCENES: Scene[] = [
  setScene("compose-set-menu", "组菜单 · 第二行一短句", "栏顶「⌄」：每组第二行是那台的短句（几个目录 · 几个会话 · 几行起不了），悬停看逐行侧卡", async () => {
    await mainReady(declared());
    await click('[data-role="set-menu"]');
    await sleep(2500);
    await hover(await byText("[role=menu] [role^=menuitem]", "工具链开发"));
    await sleep(900);
  }),
  setScene("compose-set-count-max", "一组页 · 个数到上限", "周报那一行加到 9：「＋」当场灰，悬停是那台那一句", async () => {
    await openSetPage("周报");
    await click('[data-role="set-more"]');
    await sleep(1500);
    await hover('[data-role="set-more"]');
    await sleep(900);
  }),
  setScene("compose-set-dup-row", "一组页 · 重行", "加一行（带上一行目录与别名）＝ 同目录同别名两行：第二行当场淡掉、下面写那台那一句", async () => {
    await openSetPage("工具链开发");
    await click('[data-role="set-add"]');
    await sleep(1800);
  }, 900),
  setScene("compose-set-delete", "一组页 · 删组 ＋ 撤销", "组名格旁［删除］：直接删、页换新建、右下 toast［撤销］", async () => {
    await openSetPage("ledger 评审");
    await click('[data-role="set-delete"]');
    await sleep(1500);
  }),
  setScene("compose-single-alias", "单个页 · 选了别名", "别名选 look：账号 · 运行于 · 开口那句写合下来的值 ＋「来自 look」", async () => {
    await mainReady(declared());
    await click('[data-role="new-session"]');
    await waitFor("[role=dialog]");
    await sleep(800);
    await click('[role=dialog] button[aria-label="别名"]');
    await sleep(300);
    await click(await byText("[role=menu] [role^=menuitem]", /^look/));
    await sleep(1500);
  }, 900),
];

// ---------- 标签页改名 ----------

/** 只留本机那一台。 */
function localWorld(): World {
  const w = defaultWorld();
  w.machines = [LOCAL];
  w.sessions = w.sessions.filter((s) => s.origin === LOCAL && !s.ended);
  w.disk = { [LOCAL]: w.disk[LOCAL] };
  w.quiet = [...(w.quiet ?? []), "groups-read"];
  return w;
}

function renameScene(id: string, title: string, desc: string, act: () => Promise<void>): Scene {
  return { id, page: "index", dir: "标签页改名", title, desc, width: W, height: H, world: localWorld, act: async () => act() };
}

const RENAME_SCENES: Scene[] = [
  renameScene("compose-rename-f2", "F2 · 就地框", "焦点在标签页上按 F2：框只盖名字那一格、同标题字号、占位「留空 = 原标题」", async () => {
    await mainReady(2);
    const tab = await waitFor<HTMLElement>("#tab-bar .tab");
    tab.focus();
    await key("F2", { code: "F2" });
    await waitFor("body > input");
    const box = document.querySelector<HTMLInputElement>("body > input")!;
    box.value = "";
    box.dispatchEvent(new Event("input", { bubbles: true }));
    await sleep(500);
  }),
  renameScene("compose-rename-menu", "右键菜单 ·「改标题」带 F2", "标签页右键：「改标题」右侧写键，同菜单别的项", async () => {
    await mainReady(2);
    await rightClick("#tab-bar .tab");
    await sleep(600);
  }),
  renameScene("compose-rename-dialog", "对话框 · 框下一行", "历史页一行 ⋯ →「改标题」：框下一行写「留空 = 原标题」", async () => {
    await mainReady(2);
    await click(".history-trigger");
    await waitFor('.history-view [role="option"]');
    await sleep(600);
    const row = document.querySelectorAll<HTMLElement>('.history-view [role="option"]')[1];
    await click(row);
    await click(row.querySelector<HTMLElement>("button[aria-label]")!);
    await click(await byText('[role^="menuitem"]', "改标题"));
    await sleep(400);
  }),
];

export const COMPOSE_SCENES: Scene[] = [...ATTACH_SCENES, ...QUEUE_SCENES, ...SET_SCENES, ...RENAME_SCENES];
