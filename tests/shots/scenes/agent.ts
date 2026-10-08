/**
 * agent 窗口：点 agent 面板的一行开那个子运行自己的窗口（查看窗那个入口带 `run=`）。
 */
import { hm } from "../fake/clock";
import type { Scene } from "./index";
import { defaultWorld, sidOf } from "../fake/world";
import { Convo } from "../fake/records";
import type { World } from "../fake/types";
import type { RunInfo } from "../../../src/frontend/ui/generated/RunInfo";
import { sleep, waitFor } from "./helpers";

const CWD = "/home/user/work/orders";
const MIN = 60_000;

/** 派活那段话：子运行记录里第一条（`agentTask`）。 */
function briefed(c: Convo, text: string): Convo {
  c.user(text);
  const first = c.records[c.records.length - 1] as { userText?: unknown };
  first.userText = { speaker: { kind: "agentTask" }, text };
  return c;
}

/** 第一个会话里多几个子运行：在等工具的（还派了一个孙 agent）· 失败的 · 状态不明的 · 完成的。 */
export function agentWorld(): World {
  const w = defaultWorld();
  const s = w.sessions[0];
  const now = Date.now();
  const sid = sidOf(1);
  const a2 = briefed(new Convo(sid, CWD, "2026-10-01T09:06:00Z"), "在 services/ 下找所有直接用 httpx 调内部服务、没有超时的地方。\n\n只读，不改仓。查到的按「在哪 · 是什么 · 为什么」三段交回；拿不准的写拿不准，别猜。\n范围：services/ 与 libs/http，别的目录不用看。\n交回前自己核一遍行号。\n别动测试。");
  a2.say("先找所有 httpx 的调用点。");
  a2.tool("Grep", { pattern: "httpx\\.(get|post)\\(", path: "services" }, "services/search/client.py:22\nservices/mailer/send.py:41");
  a2.say("找到两处。mailer 那处交给另一路看超时配置，我接着看 search。");
  a2.tool("Agent", { description: "看 mailer 的超时配置", prompt: "看 services/mailer/send.py 第 41 行那次调用有没有超时", subagent_type: "Explore" }, null, { card: "agent", child: { label: "看 mailer 的超时配置", kind: "Explore" } });
  a2.tool("Grep", { pattern: "timeout", path: "services/search" }, null);
  const a3 = briefed(new Convo(sid, CWD, "2026-10-01T09:08:00Z"), "看 services/mailer/send.py 第 41 行那次调用有没有超时。");
  a3.say("先读这个文件。");
  a3.tool("Read", { file_path: `${CWD}/services/mailer/send.py` }, "…");
  a3.say("41 行用的是模块级 client，构造时没给 timeout。");
  const a4 = briefed(new Convo(sid, CWD, "2026-10-01T08:58:00Z"), "对一遍文案表与术语表。\n\n只读，不改仓。");
  a4.say("先找相关的代码在哪。");
  a4.tool("Grep", { pattern: "copyText\\(", path: "src" }, "…");
  const a5 = briefed(new Convo(sid, CWD, "2026-10-01T09:04:00Z"), "复现「很多 agent 在跑」：在一个安静的会话里派三个后台 agent，等它们结束后不再动会话，看面板多久之后改口。");
  a5.say("派了三个后台 agent，等它们写完。");
  a5.tool("Bash", { command: "sleep 60" }, "", { card: "command" });
  a5.say("三个都还写着在跑。再等一会儿看它会不会改口。");
  const a2tool = s.runs.find((r) => r.run === "agent-a2")?.tool;
  const runs: RunInfo[] = [
    { run: "agent-a1", label: "补重试的单元测试", kind: "general-purpose", tool: s.runs[0].tool, state: "done", last: { t: "say" }, started_ms: now - 26 * MIN, active_ms: now - 22 * MIN, ended_ms: now - 21 * MIN, why: "reported", calls: 2 },
    { run: "agent-a2", label: "检查其他服务有没有同类调用", kind: "Explore", tool: a2tool, state: "running", last: { t: "tool", name: "Grep" }, waiting: "Grep", started_ms: now - 15 * MIN, active_ms: now - 6 * MIN, calls: 3, background: true },
    { run: "agent-a3", label: "看 mailer 的超时配置", kind: "Explore", tool: "toolu_a3", parent: "agent-a2", state: "running", last: { t: "say" }, started_ms: now - 9 * MIN, active_ms: now - 1 * MIN, calls: 1 },
    { run: "agent-a4", label: "对一遍文案表与术语表", kind: "Explore", tool: "toolu_a4", state: "failed", last: { t: "tool", name: "Grep" }, started_ms: now - 34 * MIN, active_ms: now - 33 * MIN, ended_ms: now - 32 * MIN, why: "reported", calls: 2, error: 'API Error: 529 {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}' },
    { run: "agent-a5", label: "复现「很多 agent 在跑」", kind: "general-purpose", tool: "toolu_a5", state: "unknown", why: "quiet", last: { t: "say" }, started_ms: now - 28 * MIN, active_ms: now - 12 * MIN, calls: 5, background: true },
  ];
  s.runs = runs.map((r) => (r.started_ms === undefined ? r : { ...r, started_text: hm(r.started_ms) }));
  s.runRecords = { ...s.runRecords, "agent-a2": a2.records, "agent-a3": a3.records, "agent-a4": a4.records, "agent-a5": a5.records };
  if (s.runRecords["agent-a1"]?.[0]) (s.runRecords["agent-a1"][0] as { userText?: unknown }).userText = { speaker: { kind: "agentTask" }, text: "为 InventoryClient._call 写单元测试：成功、重试后成功、重试耗尽三种。" };
  return w;
}

function win(id: string, run: string, title: string, desc: string, after?: () => Promise<void>): Scene {
  return {
    id,
    page: "viewer",
    query: `viewer=${sidOf(1)}&run=${run}`,
    dir: "agent 窗口",
    title,
    desc,
    width: 900,
    height: 720,
    world: agentWorld,
    act: async () => {
      await waitFor('[data-role="brief"]', 15_000);
      await sleep(800);
      await after?.();
    },
  };
}

export const AGENT_SCENES: Scene[] = [
  win("agent-window-waiting", "agent-a2", "agent 窗口 · 在等工具", "在跑、最近一条是还没拿到结果的工具调用：说明写「等 Grep · 6m」；它派出的 agent 一排小片；派活那段话是带抬头的框（超过四行收起）"),
  win("agent-window-grandchild", "agent-a3", "agent 窗口 · 孙 agent", "孙 agent 自己的窗口：路径「会话 › 派出它的 agent › 它」，派活那段话的抬头写「「…」派给它的话」"),
  win("agent-window-failed", "agent-a4", "agent 窗口 · 失败", "失败：说明写「失败 · 交回报错」＋ 报错原话；尾巴上一条结束线"),
  win("agent-window-unknown", "agent-a5", "agent 窗口 · 状态不明", "状态不明：说明写「未收到结束 · 记录 12m 无更新」＋［刷新］；尾巴上一条结束线"),
  win("agent-window-done", "agent-a1", "agent 窗口 · 完成", "完成的进来停在最上，从派活的那段话读起；用时 · 工具次数"),
];
