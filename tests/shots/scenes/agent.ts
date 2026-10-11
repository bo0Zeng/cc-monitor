/**
 * agent 窗口：点 agent 面板的一行开那个子运行自己的窗口（查看窗那个入口带 `run=`）。
 */
import type { Scene } from "./index";
import { defaultWorld, sidOf } from "../fake/world";
import { Convo } from "../fake/records";
import type { World } from "../fake/types";
import { sleep, waitFor } from "./helpers";

const CWD = "/home/user/work/orders";
const MIN = 60_000;

/** 一个子 agent 那一段（从 `at` 起排；头一条是派活那段话 —— 子记录里没有上一条的那一句，后端认作派活）。 */
function childAt(sid: string, id: string, at: number, brief: string): Convo {
  const c = new Convo(sid, CWD, new Date(at).toISOString(), undefined, id);
  c.user(brief);
  return c;
}

/**
 * 第一个会话换成几个子运行（盘上的 `subagents/agent-<id>.jsonl`，状态由后端照记录裁）：完成的 · 在等工具的（还派了一个孙 agent）·
 * 失败的 · 状态不明的（记录久未再写）。完成的那个是前台派的（那次调用拿到结果就是收场），其余是后台派的（当场回「已起」）。
 */
export function agentWorld(): World {
  const w = defaultWorld();
  const s = w.sessions[0];
  const now = Date.now();
  const sid = sidOf(1);
  const c = new Convo(sid, CWD, new Date(now - 40 * MIN).toISOString());
  c.title("给订单服务加重试与超时");
  c.user("库存接口调用加重试与整体超时，顺手查一下别处有没有同类问题。");

  const a4 = childAt(sid, "a4", now - 34 * MIN, "对一遍文案表与术语表。\n\n只读，不改仓。");
  a4.clock(now - 33 * MIN - 20_000).say("先找相关的代码在哪。");
  a4.tool("Grep", { pattern: "copyText\\(", path: "src" }, "…");
  a4.clock(now - 32 * MIN - 20_000).apiError(529, 'API Error: 529 {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}');
  c.clock(now - 34 * MIN - 20_000).tool("Task", { description: "对一遍文案表与术语表", prompt: "对一遍文案表与术语表。只读，不改仓。", subagent_type: "Explore" }, "Async agent launched successfully.", { child: a4, launched: true });

  const a5 = childAt(sid, "a5", now - 28 * MIN, "复现「很多 agent 在跑」：在一个安静的会话里派三个后台 agent，等它们结束后不再动会话，看面板多久之后改口。");
  a5.clock(now - 27 * MIN).say("派了三个后台 agent，等它们写完。");
  a5.tool("Bash", { command: "sleep 60" }, "");
  a5.clock(now - 17 * MIN - 20_000).say("三个都还写着在跑。再等一会儿看它会不会改口。");
  a5.writtenAt = now - 17 * MIN;
  c.clock(now - 28 * MIN - 20_000).tool("Task", { description: "复现「很多 agent 在跑」", prompt: "复现「很多 agent 在跑」。", subagent_type: "general-purpose" }, "Async agent launched successfully.", { child: a5, launched: true });

  const a1 = childAt(sid, "a1", now - 26 * MIN, "为 InventoryClient._call 写单元测试：成功、重试后成功、重试耗尽三种。");
  a1.tool("Write", { file_path: `${CWD}/tests/test_inventory_client.py`, content: "def test_ok(): ...\ndef test_retry_then_ok(): ...\ndef test_retry_exhausted(): ...\n" }, "File created successfully.");
  a1.clock(now - 23 * MIN).tool("Bash", { command: "pytest -q tests/test_inventory_client.py" }, "3 passed in 0.38s");
  a1.clock(now - 22 * MIN - 20_000).say("测试已写好：tests/test_inventory_client.py，3 个用例全部通过。", 30_000, "end_turn");
  const a1in = { description: "补重试的单元测试", prompt: "为 InventoryClient._call 写单元测试：成功、重试后成功、重试耗尽三种。", subagent_type: "general-purpose" };
  const a1tool = c.clock(now - 26 * MIN - 20_000).tool("Task", a1in, null, { child: a1 });

  c.clock(now - 21 * MIN - 8_000).answer(a1tool, "Task", a1in, "测试已写好：tests/test_inventory_client.py，3 个用例全部通过。", { child: a1 });

  const a2 = childAt(sid, "a2", now - 15 * MIN, "在 services/ 下找所有直接用 httpx 调内部服务、没有超时的地方。\n\n只读，不改仓。查到的按「在哪 · 是什么 · 为什么」三段交回；拿不准的写拿不准，别猜。\n范围：services/ 与 libs/http，别的目录不用看。\n交回前自己核一遍行号。\n别动测试。");
  a2.say("先找所有 httpx 的调用点。");
  a2.tool("Grep", { pattern: "httpx\\.(get|post)\\(", path: "services" }, "services/search/client.py:22\nservices/mailer/send.py:41");
  a2.say("找到两处。mailer 那处交给另一路看超时配置，我接着看 search。");
  const a3 = childAt(sid, "a3", now - 9 * MIN, "看 services/mailer/send.py 第 41 行那次调用有没有超时。");
  a3.say("先读这个文件。");
  a3.tool("Read", { file_path: `${CWD}/services/mailer/send.py` }, "…");
  a3.clock(now - 1 * MIN - 20_000).say("41 行用的是模块级 client，构造时没给 timeout。");
  a2.clock(now - 9 * MIN - 20_000).tool("Agent", { description: "看 mailer 的超时配置", prompt: "看 services/mailer/send.py 第 41 行那次调用有没有超时", subagent_type: "Explore" }, "Async agent launched successfully.", { child: a3, launched: true });
  a2.clock(now - 6 * MIN - 20_000).tool("Grep", { pattern: "timeout", path: "services/search" }, null);
  c.clock(now - 15 * MIN - 20_000).tool("Task", { description: "检查其他服务有没有同类调用", prompt: "在 services/ 下找所有直接用 httpx 调内部服务、没有超时的地方。", subagent_type: "Explore" }, "Async agent launched successfully.", { child: a2, launched: true });

  s.convo = c;
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
      await waitFor('[data-role="brief"]');
      await sleep(800);
      await after?.();
    },
  };
}

export const AGENT_SCENES: Scene[] = [
  win("agent-window-waiting", "a2", "agent 窗口 · 在等工具", "在跑、最近一条是还没拿到结果的工具调用：说明写「等 Grep · 6m」；它派出的 agent 一排小片；派活那段话是带抬头的框（超过四行收起）"),
  win("agent-window-grandchild", "a3", "agent 窗口 · 孙 agent", "孙 agent 自己的窗口：路径「会话 › 派出它的 agent › 它」，派活那段话的抬头写「「…」派给它的话」"),
  win("agent-window-failed", "a4", "agent 窗口 · 失败", "失败：说明写「失败 · 交回报错」＋ 报错原话；尾巴上一条结束线"),
  win("agent-window-unknown", "a5", "agent 窗口 · 状态不明", "状态不明：说明写「未收到结束 · 记录 17m 无更新」＋［刷新］；尾巴上一条结束线"),
  win("agent-window-done", "a1", "agent 窗口 · 完成", "完成的进来停在最上，从派活的那段话读起；用时 · 工具次数"),
];
