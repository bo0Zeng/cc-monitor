/**
 * 大折叠：一条长的中性会话（几轮 · 连续工具 · 思考 · 中间的话 · 派 agent 与交回 · 后台任务 · 失败 · 系统注入 ·
 * 压缩 · 重试 · 中途插话 · 最后一轮在等批准），实时窗口与独立查看窗各截一份，看两条人说的话之间哪些没收进折叠。
 */
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { Convo } from "../fake/records";
import { defaultWorld, LOCAL, session, sidOf } from "../fake/world";
import { mainReady, scrollStream, sleep, waitFor } from "./helpers";

const SID = sidOf(1);
const CWD = "/home/user/work/sample";

function foldConvo(): { c: Convo; t1: string; t2: string } {
  const c = new Convo(SID, CWD, "2026-10-01T09:00:00Z");
  c.title("示例项目：配置读取整理");
  // 第一轮：思考 · 连续工具 · 中间的话 · 一次失败 · 结论
  c.user("把配置读取整理一下，统一从一个地方读。");
  c.think("先找出所有读配置的地方。", "我先找一下现在读配置的地方。");
  c.tool("Grep", { pattern: "load_config", path: "src" }, "src/a.py:3\nsrc/b.py:9\nsrc/c.py:12");
  c.tool("Read", { file_path: `${CWD}/src/a.py` }, "def load_config():\n    return {}\n", { card: "md" });
  c.tool("Read", { file_path: `${CWD}/src/b.py` }, "def load_config():\n    return {}\n", { card: "md" });
  c.say("三处各写了一份，内容一样。我把它们并到 `src/config.py`。");
  c.tool("Write", { file_path: `${CWD}/src/config.py`, content: "def load_config():\n    return {}\n" }, "File created successfully.", { card: "diff" });
  c.tool("Edit", { file_path: `${CWD}/src/a.py`, old_string: "def load_config():\n    return {}\n", new_string: "from .config import load_config\n" }, "The file has been updated.", { card: "diff" });
  c.tool("Bash", { command: "pytest -q", description: "跑测试" }, "FAILED tests/test_b.py::test_x - ImportError\n1 failed, 40 passed", { error: true, card: "command" });
  c.say("有一处导入路径没改到，补上。");
  c.tool("Edit", { file_path: `${CWD}/src/b.py`, old_string: "def load_config():\n    return {}\n", new_string: "from .config import load_config\n" }, "The file has been updated.", { card: "diff" });
  c.tool("Bash", { command: "pytest -q", description: "跑测试" }, "41 passed in 1.20s", { card: "command" });
  c.say("整理好了：三处读配置并到 `src/config.py`，测试 41 个全过。", 40_000, "end_turn");

  // 第二轮：派两个 agent · 交回 · 后台任务通知 · 系统注入 · 结论
  c.user("再让两个 agent 分别查一下文档和示例里有没有旧写法。");
  c.say("好，分两路查。");
  const t1 = c.tool("Task", { description: "查文档里的旧写法", prompt: "在 docs/ 里找 load_config 的旧用法。", subagent_type: "Explore" }, "docs/ 里有 2 处旧用法：docs/a.md:10、docs/b.md:4。", { card: "agent", child: { label: "查文档里的旧写法", kind: "Explore" } });
  const t2 = c.tool("Task", { description: "查示例里的旧写法", prompt: "在 examples/ 里找 load_config 的旧用法。", subagent_type: "Explore" }, "examples/ 里没有旧用法。", { card: "agent", child: { label: "查示例里的旧写法", kind: "Explore" } });
  c.from({ kind: "agentMessage", from: "r1", name: "查文档里的旧写法", handback: true, body: "docs/ 里有 2 处旧用法：`docs/a.md:10`、`docs/b.md:4`。" });
  c.from({ kind: "agentMessage", from: "r2", name: "查示例里的旧写法", handback: true, body: "examples/ 里没有旧用法。" });
  c.from({ kind: "taskNotification", taskId: "bg1", status: "completed", summary: "lint" });
  c.from({ kind: "taskNotification", taskId: "bg2", status: "failed", summary: "类型检查" });
  c.user("<system-reminder>\nPlaceholder reminder text.\n</system-reminder>", { meta: true });
  c.tool("Edit", { file_path: `${CWD}/docs/a.md`, old_string: "old", new_string: "new" }, "The file has been updated.", { card: "diff" });
  c.say("文档里两处旧写法改掉了，示例里没有。类型检查那个后台任务失败了，是早就有的问题，与这次无关。", 52_000, "end_turn");

  // 压缩
  c.from({ kind: "slashCommand", name: "/compact", args: "" });
  c.from({ kind: "compactSummary" });
  (c.records[c.records.length - 1] as { who: { text: string } }).who.text = "## 摘要\n\n- 配置读取已统一\n- 文档旧写法已改";

  // 第三轮：重试 · 中途你按了 Esc 并插话 · 再做 · 结论
  c.user("给 load_config 加缓存。");
  c.retry(1, 10);
  c.retry(2, 10);
  c.tool("Read", { file_path: `${CWD}/src/config.py` }, "def load_config():\n    return {}\n", { card: "md" });
  c.user("[Request interrupted by user]", { interrupt: true });
  c.queued("缓存用 functools 就行，别自己写");
  c.user("缓存用 functools 就行，别自己写。");
  c.tool("Edit", { file_path: `${CWD}/src/config.py`, old_string: "def load_config():", new_string: "@functools.cache\ndef load_config():" }, "The file has been updated.", { card: "diff" });
  c.tool("Bash", { command: "pytest -q", description: "跑测试" }, "41 passed in 1.10s", { card: "command" });
  c.say("加了 `functools.cache`，测试全过。", 30_000, "end_turn");

  // 第四轮（正在跑）：思考 · 几步 · 中间的话 · 一步在等批准
  c.user("把旧的三个文件删掉。");
  c.think("先确认没有别处再引用它们。", "先确认一下没人再用它们。");
  c.tool("Grep", { pattern: "from .a import|from .b import", path: "src" }, "");
  c.say("没有引用了，删掉。");
  c.tool("Bash", { command: "git rm src/c.py", description: "删旧文件" }, null, { card: "command" });
  return { c, t1, t2 };
}

function foldWorld(activity: "needs_you" | "working" = "needs_you"): World {
  const w = defaultWorld();
  const { c, t1, t2 } = foldConvo();
  w.sessions[0] = session(1, LOCAL, CWD, c, {
    activity,
    ...(activity === "needs_you" ? { waitingFor: "permission prompt", waitingSinceMs: Date.now() - 60_000 } : {}),
    runs: [
      { run: "agent-r1", label: "查文档里的旧写法", kind: "Explore", tool: t1, state: "done", last: { t: "say" } },
      { run: "agent-r2", label: "查示例里的旧写法", kind: "Explore", tool: t2, state: "done", last: { t: "say" } },
    ],
  });
  return w;
}

/** 长会话：很多轮（每轮思考 · 几步工具 · 中间的话 · 结论），尾部窗口装不下 ⇒ 上面由骨架占位顶着、滚到哪物化到哪。 */
function longWorld(n = 90): World {
  const w = defaultWorld();
  const c = new Convo(SID, CWD, "2026-10-01T09:00:00Z");
  c.title("示例项目：长会话");
  for (let i = 1; i <= n; i++) {
    c.user(`第 ${i} 轮：把模块 ${i} 的读取整理一下。`);
    c.think("先找读取的地方。", `先看模块 ${i}。`);
    c.tool("Grep", { pattern: `mod${i}`, path: "src" }, `src/m${i}.py:3`);
    c.tool("Read", { file_path: `${CWD}/src/m${i}.py` }, "def load():\n    return {}\n", { card: "md" });
    c.say(`模块 ${i} 只有一处，改掉。`);
    c.tool("Edit", { file_path: `${CWD}/src/m${i}.py`, old_string: "a", new_string: "b" }, "The file has been updated.", { card: "diff" });
    c.say(`模块 ${i} 改好了。`, 20_000, "end_turn");
  }
  w.sessions[0] = session(1, LOCAL, CWD, c, { activity: "idle" });
  return w;
}


/**
 * 读数（左上角那一行）：查看窗只渲尾巴、其余由骨架占位顶着 ⇒ 刚接上时的滚动高度（估）与一路往上滚到顶、全部物化之后的滚动高度（实）
 * 对一下；折着的过程不物化，剩下几块 0 高的占位。
 */
async function measure(): Promise<void> {
  await waitFor(".session-viewer [data-id]", 15_000);
  await sleep(1500);
  const scroller = document.querySelector<HTMLElement>(".session-viewer-stream")!;
  const est = scroller.scrollHeight;
  const t0 = performance.now();
  let still = 0;
  let lastH = -1;
  while (performance.now() - t0 < 22_000 && still < 5) {
    scroller.scrollTop = Math.max(0, scroller.scrollTop - scroller.clientHeight);
    scroller.dispatchEvent(new Event("scroll"));
    await sleep(8);
    const h = scroller.scrollHeight;
    still = scroller.scrollTop === 0 && h === lastH ? still + 1 : 0;
    lastH = h;
  }
  await sleep(800);
  const real = scroller.scrollHeight;
  const cards = document.querySelectorAll(".session-viewer .stream-content > [data-id]").length;
  const left = document.querySelectorAll(".session-viewer .stream-skeleton-gap").length;
  const d = document.createElement("div");
  d.style.cssText = "position:fixed;top:0;left:0;z-index:99999;background:#000;color:#0f0;font:16px monospace;padding:6px";
  d.textContent = `est=${est} real=${real} est/real=${(est / real).toFixed(3)} cards=${cards} gapsLeft=${left} ms=${Math.round(performance.now() - t0)}`;
  document.body.appendChild(d);
}

function live(id: string, title: string, desc: string, act: Scene["act"], height = 800): Scene {
  return { id, page: "index", dir: "大折叠", title, desc, width: 1280, height, world: foldWorld, act };
}

export const FOLDALL_SCENES: Scene[] = [
  live("foldall-live-whole", "大折叠 · 实时窗口 · 整条", "四轮的会话，窗口拉到很高一屏看完：今天哪些收进过程行、哪些平铺在外", async () => {
    await mainReady(7);
    await scrollStream("top");
    await sleep(800);
  }, 3600),
  live("foldall-live-expanded", "大折叠 · 实时窗口 · 过程全点开", "把每一轮的过程行都点开：里面原来那些小折叠怎么排", async () => {
    await mainReady(7);
    for (const l of document.querySelectorAll<HTMLElement>(".stream.active .proc-line")) l.click();
    await sleep(400);
    await scrollStream("top");
    await sleep(800);
  }, 3600),
  live("foldall-live-marked", "大折叠 · 实时窗口 · 标出没折的", "红框：完成的轮里没收进过程行的；橙框：正在跑那一轮平铺的", async () => {
    await mainReady(7);
    await scrollStream("top");
    await sleep(800);
    const kids = [...document.querySelectorAll<HTMLElement>(".stream.active .stream-content > *")];
    const lastUser = kids.map((k) => k.classList.contains("card-user")).lastIndexOf(true);
    kids.forEach((k, i) => {
      if (k.classList.contains("proc-hidden") || k.classList.contains("card-user") || k.classList.contains("proc-line")) return;
      const out = /card-speaker|card-notice|card-event-line|card-compact|card-api-error/.test(k.className);
      const run = i > lastUser && !k.classList.contains("card-assistant-conclusion");
      if (out) k.style.outline = "2px solid #ff3b3b";
      else if (run) k.style.outline = "2px dashed #ff9f1a";
      if (out || run) k.style.outlineOffset = "2px";
    });
    await sleep(300);
  }, 2000),
  live("foldall-live-bottom", "大折叠 · 实时窗口 · 底部", "停在底部：正在跑、在等批准的那一轮", async () => {
    await mainReady(7);
    await sleep(800);
  }),
  { ...live("foldall-live-running", "大折叠 · 实时窗口 · 正在跑", "最后一轮在跑（不在等批准）：也折着，行上转圈 ＋「现在：那一步」", async () => {
    await mainReady(7);
    await sleep(800);
  }), world: () => foldWorld("working") },
  live("foldall-live-tail", "大折叠 · 实时窗口 · 长过程末尾", "点开第一轮（过程比一屏长）：左边一道竖线，末尾一行「收起这段过程」", async () => {
    await mainReady(7);
    await scrollStream("top");
    document.querySelector<HTMLElement>(".stream.active .proc-line")?.click();
    await sleep(400);
    const tail = document.querySelector<HTMLElement>(".stream.active .proc-tail");
    tail?.scrollIntoView({ block: "center" });
    await sleep(600);
  }, 700),
  { ...live("foldall-long-top", "大折叠 · 长会话 · 滚到最上面", "九十轮的长会话，上面由骨架占位顶着：滚到最上面，物化出来的轮照样每轮一行过程", async () => {
    await mainReady(7);
    await sleep(600);
    for (let k = 0; k < 6; k++) {
      await scrollStream("top");
      await sleep(300);
    }
    await sleep(600);
  }), world: longWorld },
  { id: "foldall-measure-viewer-folded", page: "viewer", query: `viewer=${SID}`, dir: "大折叠", title: "大折叠 · 读数 · 查看窗 · 折着", desc: "460 轮的会话在独立查看窗里：刚接上骨架时的滚动高度 vs 滚到顶全部物化后的（左上角）；机器忙时读数不稳，要在空的时候截", width: 1100, height: 800, world: () => longWorld(460), act: measure },
  { id: "foldall-viewer-long-expand", page: "viewer", query: `viewer=${SID}`, dir: "大折叠", title: "大折叠 · 查看窗 · 长会话里点开一轮", desc: "460 轮、滚到顶（折着的过程都没建），点开第二轮：它那段过程这时才建出来", width: 1100, height: 900, world: () => longWorld(460), act: async () => {
    await measure();
    document.querySelectorAll<HTMLElement>(".session-viewer .proc-line")[1]?.click();
    await sleep(1200);
  } },
  { id: "foldall-measure-viewer-open", page: "viewer", query: `viewer=${SID}`, dir: "大折叠", title: "大折叠 · 读数 · 查看窗 · 全展开", desc: "同上，「过程默认展开」开着", width: 1100, height: 800, world: () => longWorld(460), storage: { "cc-monitor.stream.process-expanded": "1" }, act: measure },
  {
    id: "foldall-viewer-whole",
    page: "viewer",
    query: `viewer=${SID}`,
    dir: "大折叠",
    title: "大折叠 · 独立查看窗 · 整条",
    desc: "同一个会话在独立查看窗里",
    width: 1100,
    height: 3600,
    world: foldWorld,
    act: async () => {
      await waitFor(".session-viewer [data-id]", 15_000);
      await sleep(1500);
      const s = document.querySelector<HTMLElement>(".session-viewer .stream, .session-viewer [class*='stream']");
      s?.scrollTo(0, 0);
      await sleep(600);
    },
  },
];
