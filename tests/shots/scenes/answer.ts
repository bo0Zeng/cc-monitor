/**
 * 答批准：会话在等批准 / 回答 / 批计划时，消息流末尾的答题块（桌面那几态）。
 * 请求走真后端：场景照那一家 hook 交来的形（PermissionRequest）经 `needs-ask` 挂上（`fake/backend.ts::hookAsk`），
 * 选项的字 · 规则人话 · 只能在终端里答那一句 · 别处先答了那一句都是真后端写的；台架只造会话记录（`Convo` 落成盘上的原始 jsonl）与 hook 输入（正文都是编的）。
 */
import type { Scene } from "./index";
import type { World } from "../fake/types";
import type { FakeBackend } from "../fake/backend";
import { HOME } from "../disk";
import { Convo, jsonlOf } from "../fake/records";
import { defaultWorld, LOCAL, session, sidOf } from "../fake/world";
import { WALL_CEILING_MS } from "../../test-support/wall-ceiling";
import { byText, click, hover, mainReady, rightClick, sleep, until } from "./helpers";
import { copyText } from "../../../src/frontend/ui/copy-table";

const N = 31;
const SID = sidOf(N);
const CWD = "/home/user/work/tickets";
const PROBE = "touch probe1.txt\necho ok > probe1.txt";
const PLAN = "1. 建 planB.txt，写入 hi\n2. 跑一遍测试";
const QUESTIONS = [
  { question: "Which color?", header: "Color", multiSelect: false, options: [{ label: "Red", description: "warm" }, { label: "Blue" }] },
  { question: "Which fruits?", header: "Fruit", multiSelect: true, options: [{ label: "Apple" }, { label: "Pear" }, { label: "Plum" }] },
];

/** 那一个会话：说完一句、那一步还没有结果，在等人（pidfile 说 waiting）。记录末一条落在它开始等的那一刻。 */
function world(tool: string, input: Record<string, unknown>, waitingFor: string): () => World {
  return () => {
    const w = defaultWorld();
    const since = Date.now() - 12_000;
    const c = new Convo(SID, CWD);
    c.title("整理工单");
    c.user("建个探针文件试一下。");
    c.tool(tool, input, null, { lead: "先把探针文件建出来。" });
    c.endAt(since);
    w.sessions = [session(N, LOCAL, CWD, c, { activity: "needs_you", waitingFor, waitingSinceMs: since })];
    return w;
  };
}

const BASH_INPUT = { command: PROBE, description: "建探针文件" };
const bashWorld = world("Bash", BASH_INPUT, "permission prompt");

/** 那一家 hook 交来的那一份（键照 10-10 真机录下的形；路径 · 会话 id 是编的）。 */
function hook(tool: string, input: Record<string, unknown>, extra: Record<string, unknown> = {}): Record<string, unknown> {
  return { session_id: SID, transcript_path: `${HOME}/${jsonlOf(CWD, SID)}`, cwd: CWD, permission_mode: "default", hook_event_name: "PermissionRequest", tool_name: tool, tool_input: input, ...extra };
}

const SUGGEST = [
  { type: "addDirectories", directories: ["/w"], destination: "session" },
  { type: "setMode", mode: "acceptEdits", destination: "session" },
];

/** 记录里最后一次工具调用的 id（还没有结果的那一步）。 */
function lastCall(c: Convo): string {
  for (const l of [...c.lines].reverse()) {
    if (typeof l === "string") continue;
    const blocks = ((l.message as { content?: unknown } | undefined)?.content ?? []) as { type?: string; id?: string }[];
    const use = Array.isArray(blocks) ? blocks.find((b) => b.type === "tool_use") : undefined;
    if (use?.id) return use.id;
  }
  throw new Error(`${c.sid} 的记录里没有工具调用`);
}

/** 真后端给这个会话挂上了请求（`sessions-needs` 里有它）；到兜底上限还没有 ⇒ 抛（不让后面那一句「没等到」盖住原因）。 */
async function hung(backend: FakeBackend): Promise<void> {
  const t0 = performance.now();
  while ((await backend.askIdOf(LOCAL, SID)) === null) {
    if (performance.now() - t0 > WALL_CEILING_MS) throw new Error(`真后端 ${WALL_CEILING_MS}ms 没挂上 ${SID} 的请求`);
    await new Promise((r) => requestAnimationFrame(() => r(null)));
  }
}

/** 打开那个会话、挂上请求、等答题块画出选项（不抢焦点：截图里焦点不在块里）。 */
async function asked(backend: FakeBackend, h: Record<string, unknown> | null, want = "button[data-choice]"): Promise<HTMLElement> {
  await mainReady(1);
  if (h) {
    backend.hookAsk(LOCAL, h);
    await hung(backend);
  }
  const el = await until(() => document.querySelector<HTMLElement>(`[role=group] ${want}`));
  await sleep(600);
  return el;
}

function shot(id: string, title: string, desc: string, act: Scene["act"], w: () => World = bashWorld): Scene {
  return { id, page: "index", dir: "主窗口-答批准", title, desc, width: 1280, height: 800, world: w, act };
}

export const ANSWER_SCENES: Scene[] = [
  shot("answer-approve", "答批准 · 批准一步（甲1）", "会话在等批准 Bash：消息流末尾展开答题块 —— 命令全文、允许一族一行（数字键 1–3）、拒绝一族另起一行，只有「拒绝并停下」红字；字全是真后端写的", async ({ backend }) => {
    await asked(backend, hook("Bash", BASH_INPUT, { permission_suggestions: SUGGEST }));
  }),
  shot("answer-deny-note", "答批准 · 拒绝并说明（甲2）", "点「拒绝并说明…」：原地换成一行输入框（Enter 交 · Esc 回）", async ({ backend }) => {
    const deny = await asked(backend, hook("Bash", { command: "rm -rf build/", description: "清掉构建产物" }, { permission_suggestions: SUGGEST }), "button[data-choice=deny-note]");
    await click(deny);
    const input = await until(() => document.querySelector<HTMLInputElement>("[role=group] input[data-note]"));
    input.value = "build/ 里有手改的文件，先只删 build/tmp";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await sleep(300);
  }, world("Bash", { command: "rm -rf build/", description: "清掉构建产物" }, "permission prompt")),
  shot("answer-questions", "答批准 · 回答提问（甲3）", "提问两题：单选圆点、多选方块、每题末尾一格「其他…」；每题都答了「提交答案」才可点", async ({ backend }) => {
    await asked(backend, hook("AskUserQuestion", { questions: QUESTIONS }), "[data-option]");
    const pick = async (q: number, o: number) => click(document.querySelectorAll<HTMLElement>("[role=group] [data-question]")[q].querySelectorAll<HTMLElement>("[data-option]")[o]);
    await pick(0, 0);
    await pick(1, 0);
    await pick(1, 1);
    await sleep(300);
  }, world("AskUserQuestion", { questions: QUESTIONS }, "permission prompt")),
  shot("answer-plan", "答批准 · 批计划（甲4）", "计划原文进等宽块；四项由核心给：自动模式 · 自动接受编辑 · 逐个确认 · 继续规划…", async ({ backend }) => {
    await asked(backend, hook("ExitPlanMode", { plan: PLAN }, { permission_mode: "plan" }));
  }, world("ExitPlanMode", { plan: PLAN }, "permission prompt")),
  shot("answer-subagent", "答批准 · 子 agent 的请求（甲5）", "后台子 agent 发来的批准：头上标是哪一种 agent；多一项「改到终端里答」排末尾、幽灵样式", async ({ backend }) => {
    await asked(backend, hook("Bash", { command: "touch fgA.txt", description: "建 fgA.txt" }, { permission_suggestions: SUGGEST.slice(0, 1), agent_id: "a1f0", agent_type: "general-purpose" }));
  }, world("Bash", { command: "touch fgA.txt", description: "建 fgA.txt" }, "permission prompt")),
  shot("answer-manual", "答批准 · 只能在终端里答（甲6）", "没挂上请求（这条会话不是经答题口起的）：核心写好的那一句 ＋［看终端］", async ({ backend }) => {
    await asked(backend, null, "[data-part=manual]");
  }),
  shot("answer-elsewhere", "答批准 · 别处先答了（甲7）", "终端那边先答了：经真后端答掉那一条，记录里落下那一步的结果、pidfile 改回在跑（照真终端答了之后的样子）⇒ 真后端推 changed {needs}、重读里没了那条请求 ⇒ 块收起，留一行「已在终端答过或已关闭」（稳定态，下一条请求来了 / 切走才没）；头上与过程行照真后端此刻说的", async ({ backend }) => {
    await asked(backend, hook("Bash", BASH_INPUT, { permission_suggestions: SUGGEST }), "button[data-choice=allow]");
    await backend.answerElsewhere(LOCAL, SID, "allow");
    // 那一步的结果接在记录末尾（同一个调用 id），pidfile 改回在跑。
    const c = backend.world.sessions[0].convo!;
    const from = c.lines.length;
    c.answer(lastCall(c), "Bash", BASH_INPUT, "");
    await backend.appendLines(LOCAL, c, from);
    await backend.setStatus(LOCAL, SID, "busy");
    await until(() => document.querySelector("[role=group] [data-part=gone]"));
    await sleep(1500);
  }),
  shot("answer-auto-menu", "答批准 · 没人答时自动答 ▸（甲8）", "会话头菜单（右键标签页同一份）「没人答时自动答 ▸」：关 · 5m · 15m · 30m 一组单选，勾在会话注解里那一格上（这里经真后端设成 15 分钟）", async ({ backend }) => {
    await mainReady(1);
    await backend.annotate(LOCAL, SID, { autoAnswer: { afterMin: 15 } });
    await sleep(800);
    await rightClick("#tab-bar .tab");
    await hover(await byText("[role=menu] > [role=none]", copyText("tabMenu.autoAnswer.label")));
    await sleep(900);
  }, world("AskUserQuestion", { questions: QUESTIONS }, "permission prompt")),
  shot("answer-auto-answered", "答批准 · 没人答、已自动答（甲9）", "会话设了 1 分钟没人答就自动答：提问挂上、一分钟没人点 ⇒ 真后端替人答了（每题答「其他」＋ 缺省那一句），块收起、留一行核心写的「1 分钟没人答，已自动答：…」", async ({ backend }) => {
    await mainReady(1);
    await backend.annotate(LOCAL, SID, { autoAnswer: { afterMin: 1 } });
    await asked(backend, hook("AskUserQuestion", { questions: QUESTIONS }), "[data-option]");
    // 那台到点替人答（真钟一分钟）；记录里那一步的结果照真的接上、pidfile 改回在跑。
    const t0 = performance.now();
    while ((await backend.askIdOf(LOCAL, SID)) !== null) {
      if (performance.now() - t0 > WALL_CEILING_MS) throw new Error(`真后端 ${WALL_CEILING_MS}ms 没替人答 ${SID} 的提问`);
      await sleep(500);
    }
    const c = backend.world.sessions[0].convo!;
    const from = c.lines.length;
    c.answer(lastCall(c), "AskUserQuestion", { questions: QUESTIONS }, "");
    await backend.appendLines(LOCAL, c, from);
    await backend.setStatus(LOCAL, SID, "busy");
    await until(() => document.querySelector("[role=group] [data-part=gone]"));
    await sleep(1500);
  }, world("AskUserQuestion", { questions: QUESTIONS }, "permission prompt")),
];
