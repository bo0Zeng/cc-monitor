/**
 * 答批准：会话在等批准 / 回答 / 批计划时，消息流末尾的答题块（答批准设计稿甲1–甲7，桌面那几态）。
 * 请求走真后端：场景照那一家 hook 交来的形（PermissionRequest）经 `needs-ask` 挂上（`fake/backend.ts::hookAsk`），
 * 选项的字 · 规则人话 · 只能在终端里答那一句 · 别处先答了那一句都是真后端写的；台架只造会话记录与 hook 输入（正文都是编的）。
 */
import type { Scene } from "./index";
import type { World } from "../fake/types";
import type { FakeBackend } from "../fake/backend";
import { Convo } from "../fake/records";
import { defaultDisk, defaultWorld, LOCAL, rawJsonlPathOf, session, sidOf } from "../fake/world";
import { click, mainReady, sleep, until } from "./helpers";

const N = 31;
const SID = sidOf(N);
const CWD = "/home/user/work/tickets";
const PROBE = "touch probe1.txt\necho ok > probe1.txt";
const PLAN = "1. 建 planB.txt，写入 hi\n2. 跑一遍测试";
const QUESTIONS = [
  { question: "Which color?", header: "Color", multiSelect: false, options: [{ label: "Red", description: "warm" }, { label: "Blue" }] },
  { question: "Which fruits?", header: "Fruit", multiSelect: true, options: [{ label: "Apple" }, { label: "Pear" }, { label: "Plum" }] },
];

/** 那一个会话：说完一句、那一步还没有结果，在等人（pidfile 说 waiting）。 */
function world(tool: string, input: Record<string, unknown>, waitingFor: string): () => World {
  return () => {
    const w = defaultWorld();
    const c = new Convo(SID, CWD, "2026-10-08T06:20:00Z");
    c.title("整理工单");
    c.user("建个探针文件试一下。");
    const call = c.tool(tool, input, null, { lead: "先把探针文件建出来。", card: tool === "Bash" ? "command" : "interactive" });
    // 盘上那一份（同一个调用 id）：会话事实整份由真后端读它答（needs · 没结果那一步 · 用量 …）。
    const raw = [
      { type: "user", uuid: "u-1", timestamp: "2026-10-08T06:20:00.000Z", cwd: CWD, sessionId: SID, message: { role: "user", content: "建个探针文件试一下。" } },
      { type: "assistant", uuid: "a-1", parentUuid: "u-1", timestamp: "2026-10-08T06:21:05.000Z", cwd: CWD, sessionId: SID, message: { id: "msg-1", role: "assistant", model: "claude-sonnet-4-5", usage: { input_tokens: 12, cache_creation_input_tokens: 2000, cache_read_input_tokens: 40000, output_tokens: 80 }, content: [{ type: "text", text: "先把探针文件建出来。" }, { type: "tool_use", id: call, name: tool, input }] } },
    ];
    w.sessions = [session(N, LOCAL, CWD, c, { activity: "needs_you", waitingFor, waitingSinceMs: Date.now() - 12_000, idle: false, rawJsonl: raw.map((r) => JSON.stringify(r) + "\n").join("") })];
    w.disk = defaultDisk(w.machines, w.sessions);
    w.realStream = true;
    return w;
  };
}

const bashWorld = world("Bash", { command: PROBE, description: "建探针文件" }, "permission prompt");

/** 那一家 hook 交来的那一份（键照 10-10 真机录下的形；路径 · 会话 id 是编的）。 */
function hook(tool: string, input: Record<string, unknown>, extra: Record<string, unknown> = {}): Record<string, unknown> {
  return { session_id: SID, transcript_path: `/home/user/.claude/projects/-home-user-work-tickets/${SID}.jsonl`, cwd: CWD, permission_mode: "default", hook_event_name: "PermissionRequest", tool_name: tool, tool_input: input, ...extra };
}

const SUGGEST = [
  { type: "addDirectories", directories: ["/w"], destination: "session" },
  { type: "setMode", mode: "acceptEdits", destination: "session" },
];

/** 打开那个会话、挂上请求、等答题块画出选项（不抢焦点：截图里焦点不在块里）。 */
async function asked(backend: FakeBackend, h: Record<string, unknown> | null, want = "button[data-choice]"): Promise<HTMLElement> {
  await mainReady(1);
  if (h) {
    backend.hookAsk(LOCAL, h);
    // 真后端挂上了（sessions-needs 里有它）才往下等界面；挂不上就在这里说清，不让后面那一句「没等到」盖住原因。
    let id: string | null = null;
    for (let i = 0; i < 40 && id === null; i++) {
      id = await backend.askIdOf(LOCAL, SID);
      if (id === null) await sleep(250);
    }
    console.info("[shots] 真后端挂上的请求：", id);
  }
  const el = await until(() => document.querySelector<HTMLElement>(`[role=group] ${want}`), 15_000);
  await sleep(600);
  return el;
}

function shot(id: string, title: string, desc: string, act: Scene["act"], w: () => World = bashWorld): Scene {
  return { id, page: "index", dir: "主窗口-答批准", title, desc, width: 1280, height: 800, world: w, act };
}

export const ANSWER_SCENES: Scene[] = [
  shot("answer-approve", "答批准 · 批准一步（甲1）", "会话在等批准 Bash：消息流末尾展开答题块 —— 命令全文、允许一族一行（数字键 1–3）、拒绝一族另起一行，只有「拒绝并停下」红字；字全是真后端写的", async ({ backend }) => {
    await asked(backend, hook("Bash", { command: PROBE, description: "建探针文件" }, { permission_suggestions: SUGGEST }));
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
    await asked(backend, hook("Bash", { command: PROBE, description: "建探针文件" }, { permission_suggestions: SUGGEST }), "button[data-choice=allow]");
    await backend.answerElsewhere(LOCAL, SID, "allow");
    const s = backend.world.sessions[0];
    const call = (JSON.parse(s.rawJsonl!.trim().split("\n").at(-1)!) as { message: { content: { type: string; id?: string }[] } }).message.content.find((b) => b.type === "tool_use")!.id!;
    const result = { type: "user", uuid: "u-2", parentUuid: "a-1", timestamp: "2026-10-08T06:21:20.000Z", cwd: CWD, sessionId: SID, toolUseResult: { stdout: "", stderr: "" }, message: { role: "user", content: [{ type: "tool_result", tool_use_id: call, content: "" }] } };
    await backend.writeFile(LOCAL, rawJsonlPathOf(s), s.rawJsonl + JSON.stringify(result) + "\n");
    await backend.setStatus(LOCAL, SID, "busy");
    await until(() => document.querySelector("[role=group] [data-part=gone]"), 15_000);
    await sleep(1500);
  }),
];
