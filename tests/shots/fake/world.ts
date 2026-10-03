/**
 * 默认的合成世界：本机 ＋ 三台远端、十来个会话，覆盖卡片的各种样子。场景在它上面改。
 */
import { Convo } from "./records";
import type { SessionSpec, World } from "./types";
import { defaultOps } from "./ops";
import { defaultCommands } from "./commands";
import { historyOps } from "./history";
import { machineCommands, machineOps } from "./machine";

export const LOCAL = "<local>";
export const REMOTES = ["devbox", "gpu-01", "win-laptop"];

const sidOf = (n: number): string => `5e55${n.toString(16).padStart(4, "0")}-0000-4000-8000-00000000${n.toString(16).padStart(4, "0")}`;

export function session(n: number, origin: string, cwd: string, c: Convo | null, over: Partial<SessionSpec> = {}): SessionSpec {
  return {
    sid: sidOf(n),
    origin,
    cwd,
    name: null,
    kind: null,
    status: "idle",
    waitingFor: null,
    container: "tmux",
    records: c ? c.records : [],
    runs: [],
    runRecords: {},
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
    `The file ${cwd}/src/orders/clients.py has been updated.`,
    { card: "diff" },
  );
  const agentTool = c.tool(
    "Task",
    { description: "补重试的单元测试", prompt: "为 InventoryClient._call 写单元测试：成功、重试后成功、重试耗尽三种。", subagent_type: "general-purpose" },
    "测试已写好：tests/test_inventory_client.py，3 个用例全部通过。",
    { card: "agent", child: { label: "补重试的单元测试", kind: "general-purpose" } },
  );
  const agentTool2 = c.tool(
    "Task",
    { description: "检查其他服务有没有同类调用", prompt: "在 services/ 下找所有直接用 httpx 调内部服务、没有超时的地方。", subagent_type: "Explore" },
    null,
    { card: "agent", child: { label: "检查其他服务有没有同类调用", kind: "Explore" } },
  );
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
    'User has answered your questions: "release 失败时要不要也重试？"="只记日志". You can now continue with the user\'s answers in mind.',
    { card: "interactive" },
  );
  c.tool(
    "ExitPlanMode",
    {
      plan: "## 收尾计划\n\n- [x] 客户端统一重试\n- [x] 单元测试\n- [ ] 改 `test_checkout` 的超时断言\n- [ ] 更新 `docs/config.md`",
    },
    "User has approved your plan. You can now start coding.",
    { card: "interactive" },
  );
  c.say(
    "改好了。小结：\n\n```python\nclient = InventoryClient(settings.INVENTORY_URL, retries=3, backoff_ms=200, timeout_s=5)\n```\n\n全量测试 **213 passed**。`docs/config.md` 里加了三个配置项的说明。",
    88_000,
  );
  c.turnDuration(184_000);
  return { convo: c, agentTool, agentTool2 };
}

/** 两个子 agent 的记录（agent 面板 / 子 agent 卡展开时按运行读）。 */
function subagentRecords(sid: string, cwd: string): SessionSpec["runRecords"] {
  const a1 = new Convo(sid, cwd, "2026-10-01T09:05:00Z");
  a1.user("为 InventoryClient._call 写单元测试：成功、重试后成功、重试耗尽三种。");
  a1.tool("Write", { file_path: `${cwd}/tests/test_inventory_client.py`, content: "def test_ok(): ...\ndef test_retry_then_ok(): ...\ndef test_retry_exhausted(): ...\n" }, "File created successfully.", { card: "diff" });
  a1.tool("Bash", { command: "pytest -q tests/test_inventory_client.py" }, "3 passed in 0.38s");
  a1.say("测试已写好：tests/test_inventory_client.py，3 个用例全部通过。");
  const a2 = new Convo(sid, cwd, "2026-10-01T09:06:00Z");
  a2.user("在 services/ 下找所有直接用 httpx 调内部服务、没有超时的地方。");
  a2.tool("Grep", { pattern: "httpx\\.(get|post)\\(", path: "services" }, "services/search/client.py:22\nservices/mailer/send.py:41");
  a2.say("找到两处，正在逐个看它们有没有设超时……");
  return { "agent-a1": a1.records, "agent-a2": a2.records };
}

function smallConvo(sid: string, cwd: string, title: string, ask: string, answer: string, tokens = 20_000): Convo {
  const c = new Convo(sid, cwd, "2026-10-01T10:30:00Z");
  c.title(title);
  c.user(ask);
  c.say(answer, tokens);
  return c;
}

export function defaultWorld(): World {
  const rich = richConvo(sidOf(1), "/home/user/work/orders");
  const s1 = session(1, LOCAL, "/home/user/work/orders", rich.convo, {
    status: "busy",
    runs: [
      { run: "agent-a1", label: "补重试的单元测试", kind: "general-purpose", tool: rich.agentTool, state: "done", last: { t: "say" } },
      { run: "agent-a2", label: "检查其他服务有没有同类调用", kind: "Explore", tool: rich.agentTool2, state: "running", last: { t: "tool", name: "Grep" } },
    ],
    runRecords: subagentRecords(sidOf(1), "/home/user/work/orders"),
    tasks: [
      { id: "1", subject: "客户端统一重试", status: "completed", blocks: ["2"], blockedBy: [] },
      { id: "2", subject: "补单元测试", status: "completed", blocks: [], blockedBy: ["1"] },
      { id: "3", subject: "改 test_checkout 的超时断言", status: "in_progress", blocks: [], blockedBy: [], activeForm: "正在改超时断言" },
      { id: "4", subject: "更新 docs/config.md", status: "pending", blocks: [], blockedBy: ["3"] },
    ],
  });
  const sessions: SessionSpec[] = [
    s1,
    session(2, LOCAL, "/home/user/work/web-console", smallConvo(sidOf(2), "/home/user/work/web-console", "表格分页改成虚拟滚动", "表格超过一万行就卡，改成虚拟滚动。", "好的，先量一下现在的渲染耗时，再换成按可见区渲染。"), { status: "waiting", waitingFor: "permission prompt" }),
    session(3, LOCAL, "/home/user/work/notes", smallConvo(sidOf(3), "/home/user/work/notes", "周报草稿", "把这周的提交整理成周报。", "整理好了，按模块分了三段。"), { status: "idle", ended: true }),
    session(4, "devbox", "/srv/app/billing", smallConvo(sidOf(4), "/srv/app/billing", "账单导出改成流式", "导出大账单时内存会涨到 4G，改成流式写。", "改成边查边写 CSV，峰值内存降到 120M。", 140_000), { status: "busy" }),
    session(5, "devbox", "/srv/app/gateway", smallConvo(sidOf(5), "/srv/app/gateway", "网关限流配置", "给 /api/search 加每用户限流。", "已加：每用户每秒 5 次，突发 10。"), { status: "idle", kind: "bg" }),
    session(6, "gpu-01", "/data/train/ranker", smallConvo(sidOf(6), "/data/train/ranker", "排序模型训练脚本", "训练脚本加断点续训。", "加好了：每 500 步存一次，启动时自动找最新的检查点。", 96_000), { status: "idle", idle: true }),
    session(7, "win-laptop", "C:\\Users\\user\\work\\desktop-app", smallConvo(sidOf(7), "C:\\Users\\user\\work\\desktop-app", "安装包签名", "安装包要加代码签名。", "签名步骤加进打包脚本了，证书从环境变量读。"), { status: "shell" }),
  ];
  return {
    machines: [LOCAL, ...REMOTES],
    unseenMachines: [],
    closedMachines: [],
    config: defaultConfig(),
    sessions,
    ops: { ...defaultOps(), ...historyOps(), ...machineOps() },
    commands: { ...defaultCommands(), ...machineCommands() },
  };
}

export function defaultConfig(): Record<string, unknown> {
  return {
    remote: {
      enabled: true,
      hosts: REMOTES.map((label, i) => ({
        label,
        host: `10.0.0.${11 + i}`,
        port: 22,
        user: "user",
        keyPath: "~/.ssh/id_ed25519",
        hostKeyFingerprint: `SHA256:${"A".repeat(20)}${i}`,
        addresses: [],
        jump: "",
        resumeCommand: "",
      })),
    },
  };
}
