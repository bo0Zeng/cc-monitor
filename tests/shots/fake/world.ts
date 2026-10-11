/**
 * 默认的合成世界：本机 ＋ 三台远端、十来个会话，覆盖卡片的各种样子。场景在它上面改。
 */
import { Convo } from "./records";
import type { SessionSpec, World } from "./types";
import { defaultOps } from "./ops";
import { defaultCommands } from "./commands";
import { putHistory } from "./history";
import { machineCommands, machineOps } from "./machine";
import { ACCOUNTS } from "./ops";
import { blankDisk, putAccounts, type LiveSession, type MachineDisk } from "../disk";
import { putProfiles } from "../disk/profiles";

export const LOCAL = "<local>";
export const REMOTES = ["devbox", "gpu-01", "win-laptop"];

export const sidOf = (n: number): string => `5e55${n.toString(16).padStart(4, "0")}-0000-4000-8000-00000000${n.toString(16).padStart(4, "0")}`;

/** 一个会话：记录（`c`）落成盘上那几份原始 jsonl，活着的配一个替身（[`diskOf`]：开页那一刻按 `world.sessions` 现写，场景改完会话再写也作数）。 */
export function session(n: number, origin: string, cwd: string, c: Convo | null, over: Partial<SessionSpec> = {}): SessionSpec {
  return {
    sid: sidOf(n),
    origin,
    cwd,
    name: null,
    background: false,
    activity: "idle",
    waitingFor: null,
    convo: c,
    tasks: [],
    ended: false,
    idle: false,
    ...over,
  };
}

/** 一段「什么卡都有」的会话：正文、思考、命令、读写文件、改动对比、子 agent、提问、计划、报错重试。 */
export function richConvo(sid: string, cwd: string): { convo: Convo; agentTool: string; agentTool2: string } {
  const c = new Convo(sid, cwd);
  c.title("给订单服务加重试与超时");
  c.user("订单服务调用库存接口时偶尔超时，帮我加上重试（指数退避）和整体超时，顺便补测试。");
  c.think(
    "先看现有的调用点和配置项，再决定重试放在客户端封装里还是调用处。",
    "我先看一下现在的调用点和配置。",
  );
  c.tool("Bash", { command: "rg -n \"inventory_client\" src | head -20", description: "找调用点" }, [
    "src/orders/service.py:14:from .clients import inventory_client",
    "src/orders/service.py:88:    stock = inventory_client.reserve(order.items)",
    "src/orders/service.py:131:    inventory_client.release(order.id)",
    "src/orders/clients.py:7:inventory_client = InventoryClient(base_url=settings.INVENTORY_URL)",
  ].join("\n"));
  c.tool("Read", { file_path: `${cwd}/src/orders/clients.py` }, [
    "     1\timport httpx",
    "     2\tfrom .settings import settings",
    "     3\t",
    "     4\tclass InventoryClient:",
    "     5\t    def __init__(self, base_url: str):",
    "     6\t        self._http = httpx.Client(base_url=base_url)",
    "     7\t",
  ].join("\n"));
  c.say(
    [
      "调用点只有两处，都经过 `InventoryClient`。方案：",
      "",
      "1. 在 `InventoryClient` 里加一个 `_call()`，统一做**重试 ＋ 退避 ＋ 整体超时**；",
      "2. 只重试幂等的读与 `reserve`（带幂等键），`release` 失败只记日志；",
      "3. 配置项进 `settings`：",
      "",
      "| 配置 | 默认 | 说明 |",
      "|---|---|---|",
      "| `INVENTORY_RETRIES` | 3 | 最多重试次数 |",
      "| `INVENTORY_BACKOFF_MS` | 200 | 首次退避，之后翻倍 |",
      "| `INVENTORY_TIMEOUT_S` | 5 | 整体超时 |",
    ].join("\n"),
    61_000,
  );
  c.tool(
    "Edit",
    {
      file_path: `${cwd}/src/orders/clients.py`,
      old_string: "class InventoryClient:\n    def __init__(self, base_url: str):\n        self._http = httpx.Client(base_url=base_url)\n",
      new_string:
        "class InventoryClient:\n    def __init__(self, base_url: str, retries: int = 3, backoff_ms: int = 200, timeout_s: float = 5):\n        self._http = httpx.Client(base_url=base_url, timeout=timeout_s)\n        self._retries = retries\n        self._backoff = backoff_ms / 1000\n\n    def _call(self, method: str, path: str, **kw):\n        for attempt in range(self._retries + 1):\n            try:\n                return self._http.request(method, path, **kw).raise_for_status()\n            except httpx.TransportError:\n                if attempt == self._retries:\n                    raise\n                time.sleep(self._backoff * 2 ** attempt)\n",
    },
    `The file ${cwd}/src/orders/clients.py has been updated.`
  );
  const agentTool = c.tool(
    "Task",
    { description: "补重试的单元测试", prompt: "为 InventoryClient._call 写单元测试：成功、重试后成功、重试耗尽三种。", subagent_type: "general-purpose" },
    "测试已写好：tests/test_inventory_client.py，3 个用例全部通过。",
    { child: childA1(c) },
  );
  const agentTool2 = c.tool(
    "Task",
    { description: "检查其他服务有没有同类调用", prompt: "在 services/ 下找所有直接用 httpx 调内部服务、没有超时的地方。", subagent_type: "Explore" },
    null,
    { child: childA2(c) },
  );
  c.from({ kind: "agentMessage", from: "a1", name: "补重试的单元测试", handback: true, body: "测试已写好：`tests/test_inventory_client.py`，成功、重试后成功、重试耗尽三个用例全部通过。" });
  // 同一个子运行的收场通知（以交回为准：会话事实 handedBack 说它交回了 ⇒ 消息流里不画这一条）。
  c.from({ kind: "taskNotification", taskId: "a1", status: "completed", summary: "补重试的单元测试" });
  c.from({ kind: "taskNotification", taskId: "b1", status: "completed", summary: "lint" });
  c.from({ kind: "taskNotification", taskId: "b2", status: "failed", summary: "类型检查" });
  c.tool("Bash", { command: "pytest -q tests/test_inventory_client.py", description: "跑测试" }, "...\n3 passed in 0.41s");
  c.tool("Bash", { command: "pytest -q", description: "全量测试" }, "FAILED tests/test_checkout.py::test_timeout - AssertionError: expected 5, got 30\n1 failed, 212 passed in 9.82s", { error: true });
  c.say("全量里有一条超时断言还写着旧的 30 秒，我把它改成读配置。", 74_000);
  c.retry(1, 10);
  c.retry(2, 10);
  c.tool(
    "AskUserQuestion",
    {
      questions: [
        {
          question: "release 失败时要不要也重试？",
          header: "release",
          multiSelect: false,
          options: [
            { label: "只记日志", description: "不阻塞下单流程（推荐）" },
            { label: "也重试", description: "与 reserve 同一套退避" },
          ],
        },
      ],
    },
    'User has answered your questions: "release 失败时要不要也重试？"="只记日志". You can now continue with the user\'s answers in mind.'
  );
  c.tool(
    "ExitPlanMode",
    {
      plan: "## 收尾计划\n\n- [x] 客户端统一重试\n- [x] 单元测试\n- [ ] 改 `test_checkout` 的超时断言\n- [ ] 更新 `docs/config.md`",
    },
    "User has approved your plan. You can now start coding."
  );
  c.say(
    "改好了。小结：\n\n```python\nclient = InventoryClient(settings.INVENTORY_URL, retries=3, backoff_ms=200, timeout_s=5)\n```\n\n全量测试 **213 passed**。`docs/config.md` 里加了三个配置项的说明。",
    88_000,
    "end_turn",
  );
  // 照固定起点排出来的；整段挪到四分钟前收尾（这个会话此刻还在跑）。
  c.endAt(Date.now() - 4 * 60_000);
  return { convo: c, agentTool, agentTool2 };
}

/** 两个子 agent 的记录（`subagents/agent-<id>.jsonl`；agent 面板 / 子 agent 卡展开时按运行读）：头一个做完了、第二个还在跑。 */
function childA1(c: Convo): Convo {
  const cwd = c.cwd;
  const a1 = c.child("a1");
  a1.user("为 InventoryClient._call 写单元测试：成功、重试后成功、重试耗尽三种。");
  a1.tool("Write", { file_path: `${cwd}/tests/test_inventory_client.py`, content: "def test_ok(): ...\ndef test_retry_then_ok(): ...\ndef test_retry_exhausted(): ...\n" }, "File created successfully.");
  a1.tool("Bash", { command: "pytest -q tests/test_inventory_client.py" }, "3 passed in 0.38s");
  a1.say("测试已写好：tests/test_inventory_client.py，3 个用例全部通过。", 30_000, "end_turn");
  return a1;
}

function childA2(c: Convo): Convo {
  const a2 = c.child("a2");
  a2.user("在 services/ 下找所有直接用 httpx 调内部服务、没有超时的地方。");
  a2.tool("Grep", { pattern: "httpx\\.(get|post)\\(", path: "services" }, "services/search/client.py:22\nservices/mailer/send.py:41");
  a2.say("找到两处，正在逐个看它们有没有设超时……");
  return a2;
}

export function smallConvo(sid: string, cwd: string, title: string, ask: string, answer: string, tokens = 20_000): Convo {
  const c = new Convo(sid, cwd, "2026-10-01T10:30:00Z");
  c.title(title);
  c.user(ask);
  c.say(answer, tokens);
  return c.endAt(Date.now() - 3 * 60_000);
}

/** 在等你批准的那一个：说完一句、跑一条要批准的 Bash（还没有结果）。 */
function waitingConvo(sid: string): Convo {
  const c = smallConvo(sid, "/home/user/work/web-console", "表格分页改成虚拟滚动", "表格超过一万行就卡，改成虚拟滚动。", "改成按可见区渲染了，一万行首屏从 1.8 秒降到 90 毫秒。");
  c.user("顺手把构建产物清一下重打个包。");
  c.tool("Bash", { command: "rm -rf build/ && npm run build", description: "清掉重打包" }, null);
  // 要批准的那一步落在它开始等的那一刻（pidfile 说等了 2 分钟）。
  return c.endAt(Date.now() - 120_000);
}

/** 在等你回答的那一个（gpu-01 上）：提问工具还没有结果。 */
export function answerConvo(sid: string, cwd: string): Convo {
  const c = smallConvo(sid, cwd, "排序模型蒸馏", "蒸馏用哪个老师模型？", "我先看一下现有的检查点。");
  c.tool("AskUserQuestion", { questions: [{ question: "老师模型用 v7 还是 v8？", options: [{ label: "v7" }, { label: "v8" }] }] }, null);
  // 提问落在它开始等的那一刻（用它的场景说等了 5 分钟）。
  return c.endAt(Date.now() - 300_000);
}

export function defaultWorld(): World {
  const rich = richConvo(sidOf(1), "/home/user/work/orders");
  const s1 = session(1, LOCAL, "/home/user/work/orders", rich.convo, {
    activity: "working",
    tasks: [
      { id: "1", subject: "客户端统一重试", status: "completed", blocks: ["2"], blockedBy: [] },
      { id: "2", subject: "补单元测试", status: "completed", blocks: [], blockedBy: ["1"] },
      { id: "3", subject: "改 test_checkout 的超时断言", status: "in_progress", blocks: [], blockedBy: [], activeForm: "正在改超时断言" },
      { id: "4", subject: "更新 docs/config.md", status: "pending", blocks: [], blockedBy: ["3"] },
    ],
  });
  const sessions: SessionSpec[] = [
    s1,
    session(2, LOCAL, "/home/user/work/web-console", waitingConvo(sidOf(2)), { activity: "needs_you", waitingFor: "permission prompt", waitingSinceMs: Date.now() - 120_000 }),
    session(3, LOCAL, "/home/user/work/notes", smallConvo(sidOf(3), "/home/user/work/notes", "周报草稿", "把这周的提交整理成周报。", "整理好了，按模块分了三段。"), { activity: "idle", ended: true }),
    session(4, "devbox", "/srv/app/billing", smallConvo(sidOf(4), "/srv/app/billing", "账单导出改成流式", "导出大账单时内存会涨到 4G，改成流式写。", "改成边查边写 CSV，峰值内存降到 120M。", 140_000), { activity: "working" }),
    session(5, "devbox", "/srv/app/gateway", smallConvo(sidOf(5), "/srv/app/gateway", "网关限流配置", "给 /api/search 加每用户限流。", "已加：每用户每秒 5 次，突发 10。"), { activity: "idle", background: true }),
    session(6, "gpu-01", "/data/train/ranker", smallConvo(sidOf(6), "/data/train/ranker", "排序模型训练脚本", "训练脚本加断点续训。", "加好了：每 500 步存一次，启动时自动找最新的检查点。", 96_000), { activity: "idle", idle: true }),
    session(7, "win-laptop", "C:\\Users\\user\\work\\desktop-app", smallConvo(sidOf(7), "C:\\Users\\user\\work\\desktop-app", "安装包签名", "安装包要加代码签名。", "签名步骤加进打包脚本了，证书从环境变量读。"), { activity: "idle" }),
  ];
  const machines = [LOCAL, ...REMOTES];
  return {
    machines,
    disk: defaultDisk(machines),
    unseenMachines: [],
    closedMachines: [],
    staleMachines: [],
    config: defaultConfig(),
    sessions,
    ops: { ...defaultOps(), ...machineOps() },
    commands: { ...defaultCommands(), ...machineCommands() },
  };
}

/** claude 在 pidfile 里写的原词（`agents/claudecode/pidfile.rs` 认的那一套）。 */
const PIDFILE_STATUS = { working: "busy", needs_you: "waiting", idle: "idle", background_work: "shell" } as const;

/** 一个会话的替身怎么配（`real/pool.mjs`）：活着的写 pidfile；已结束的开页之后收场；可重连的只留挂着标签的那个 tmux 窗格。 */
export function liveOf(s: SessionSpec): LiveSession {
  // 进程起在它记录的第一条之前（pidfile 的 `startedAt`）：缺省是开页那一刻，记录里更早起的后台命令就被后端当成上一个进程留下的、不算。
  const first = s.convo?.firstAt;
  return {
    sid: s.sid,
    cwd: s.cwd,
    ...(first !== undefined ? { startedAt: first - 5_000 } : {}),
    kind: s.background ? "bg" : "interactive",
    ...(s.name ? { name: s.name } : {}),
    ...(s.outsideTmux ? { tmux: false as const } : {}),
    ...(s.activity ? { status: PIDFILE_STATUS[s.activity] } : {}),
    ...(s.waitingFor ? { waitingFor: s.waitingFor } : {}),
    ...(s.waitingSinceMs ? { statusUpdatedAt: s.waitingSinceMs } : {}),
    ...(s.ended ? { fate: "ended" as const } : s.idle ? { fate: "reconnectable" as const } : {}),
  };
}

/**
 * 每台盘上的那一份（交真后端那一屋子）：场景写的原始文件（`world.disk`）＋ 那台各会话的记录（主记录与子 agent 那几份）＋ 各会话的替身。
 * 开页那一刻按 `world.sessions` 现算：场景在 `defaultWorld()` 之后改会话、换会话照样落盘。
 */
export function diskOf(w: World): Record<string, MachineDisk> {
  return Object.fromEntries(
    w.machines.map((m) => {
      const d = w.disk[m] ?? blankDisk();
      const mine = w.sessions.filter((s) => s.origin === m);
      const h: MachineDisk = { files: {}, mtimes: {}, live: [] };
      putHistory(m, h, w.sessions, ACCOUNTS.slice(0, 2).map((a) => a.name), Date.now());
      const files = { ...h.files, ...d.files };
      const mtimes = { ...h.mtimes, ...d.mtimes };
      for (const s of mine) {
        if (!s.convo) continue;
        Object.assign(files, s.convo.files());
        Object.assign(mtimes, s.convo.mtimes());
      }
      return [m, { ...d, files, mtimes, live: [...d.live, ...mine.map(liveOf)] }];
    }),
  );
}

/** 每台一样的账号库（`ops.ts::ACCOUNTS`）。额度账 · 轮换账本缺省没有（中转还没见过回包）。 */
export function defaultDisk(machines: string[]): Record<string, MachineDisk> {
  return Object.fromEntries(
    machines.map((m) => {
      const d = blankDisk();
      putAccounts(d, ACCOUNTS.map((a) => ({ name: a.name, kind: a.authKind === "api-key" ? "api" : "sub", email: a.email || undefined, isDefault: a.isDefault, baseUrl: a.baseUrl })));
      // 设置窗「别名与配置文件」那一页（devbox）：照设计稿那台的配置文件。
      if (m === "devbox") putProfiles(d, "normal");
      return [m, d];
    }),
  );
}

export function defaultConfig(): Record<string, unknown> {
  return {
    remote: {
      hosts: REMOTES.map((label, i) => ({
        label,
        host: `10.0.0.${11 + i}`,
        port: 22,
        user: "user",
        keyPath: "~/.ssh/id_ed25519",
        hostKeyFingerprint: `SHA256:Aa3x${"Q".repeat(16)}Q${i}`,
        hostKeyPinnedAt: "2026-09-30",
        addresses: [],
        jump: "",
        resumeCommand: "",
        connect: true,
      })),
    },
  };
}
