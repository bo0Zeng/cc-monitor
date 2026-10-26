// cc-bus 驾驶舱的 DOM 行为断言。守的是几条**红线级**性质，不是渲染细节：
//   ① 构造时不发远端请求（不预取、不轮询）；
//   ② 「登记」与「在线」分开呈现——读名单时**不得**顺带逐个查在线；
//   ③ 读不到要说清，不留空面板；
//   ④ 驾驶舱不起、不收会话，也不广播（起会话走「开会话」）。
// 界面经通道直接说后端的 `bus-*`（`src/frontend/ui/cc-bus-control.ts`）；这里的替身直接答 `chan_call`，
//   通道那一跳的判据在 `tests/frontend/ui/cc-bus-control.vitest.ts`。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { componentSources } from "../../../test-support/component-sources";
const mockInvoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

import { CcBusSection } from "../../../../src/frontend/ui/views/cc-bus-section";
import { __resetMachineContextForTests } from "../../../../src/frontend/ui/machine-context";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { nonEmpty } from "../../../test-support/non-empty.ts";
import { chanArgsJson, chanReply, refusedReply, type ChanCallArgs } from "../../../test-support/chan-fake";

const ROSTER = {
  agents: [
    { id: "proj_cc", target: "proj_cc:0.0", unread: 2, live: false, ccm_sid: null },
    { id: "KVM_cc", target: "KVM_cc:0.0", unread: 0, live: true, ccm_sid: "sid-1" },
  ],
};

/** 等微任务队列排空（section 内部是 async 通道链）。 */
const flush = async () => {
  for (let i = 0; i < 24; i++) await Promise.resolve();
};

type Answer = (op: string, body: Record<string, unknown>) => unknown;

/** 替身：远端清单固定；`chan_call` 交给 `answer`（抛 ⇒ 对端说「不行」，原话原样）。 */
function serve(answer: Answer, remotes: string[] = ["<local>", "devbox"]): void {
  mockInvoke.mockImplementation(async (cmd: string, a: unknown) => {
    if (cmd === "backend_machines") return remotes;
    if (cmd !== "chan_call") throw new Error(`没想到会调 ${cmd}`);
    const args = a as ChanCallArgs;
    let v: unknown;
    try {
      v = answer(args.op, chanArgsJson(args) as Record<string, unknown>);
    } catch (e) {
      throw refusedReply("failed", e instanceof Error ? e.message : String(e));
    }
    return chanReply(v);
  });
}

/** 这一趟发出去的 `(origin, op, 请求体)`。 */
function chanCalls(): [string, string, Record<string, unknown>][] {
  return mockInvoke.mock.calls
    .filter(([cmd]) => cmd === "chan_call")
    .map(([, a]) => [(a as ChanCallArgs).origin, (a as ChanCallArgs).op, chanArgsJson(a as ChanCallArgs) as Record<string, unknown>]);
}

const ops = (op: string) => chanCalls().filter((c) => c[1] === op);

beforeEach(() => {
  mockInvoke.mockReset();
  __resetMachineContextForTests();
  document.body.replaceChildren();
});

/** 起面板、点一次「读取」。 */
async function readOnce(answer: Answer = () => ROSTER): Promise<CcBusSection> {
  serve(answer);
  const s = new CcBusSection();
  document.body.appendChild(s.element);
  await flush();
  (s.element.querySelector(".cc-bus-read") as HTMLButtonElement).click();
  await flush();
  return s;
}

describe("cc-bus 驾驶舱：不预取、不轮询", () => {
  it("构造时只列一次远端清单，不发任何通道往返；驾驶舱里没有装 cc-bus 的按钮（装在扩展页）", async () => {
    serve(() => {
      throw new Error("不该在构造时问后端");
    });
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    expect(mockInvoke.mock.calls.map((c) => c[0])).toEqual(["backend_machines"]);
    expect(s.element.querySelector(".cc-bus-status")?.textContent).toContain(copyText("ccBus.build.notRead"));
    expect(s.element.querySelector(".cc-bus-deploy")).toBeNull();
  });

  it("点「读取」才问一次那台的 `bus-list`", async () => {
    await readOnce();
    expect(ops("bus-list")).toEqual([["devbox", "bus-list", {}]]);
  });

  it("源码里没有定时器（红线：不新增轮询）", () => {
    const files = componentSources("views", "cc-bus-section");
    nonEmpty(files);
    const code = files.map((f) => f.code).join("\n");
    expect(code).not.toMatch(/setInterval|requestAnimationFrame/);
    expect(code).not.toMatch(/setTimeout\s*\(/);
  });

  it("★ 驾驶舱不起、不收会话，不广播，也不发信：按钮只有读取 · 检查 · 收件箱，没有输入框", async () => {
    const s = await readOnce();
    // 按钮的样子归 kit/button；这里只认各自那一个 `cc-bus-*` 钩子。
    const buttons = new Set([...s.element.querySelectorAll("button")].flatMap((b) => [...b.classList].filter((c) => c.startsWith("cc-bus-"))));
    expect([...buttons].sort()).toEqual(["cc-bus-check", "cc-bus-inbox", "cc-bus-read"]);
    expect(s.element.querySelectorAll("input")).toHaveLength(0);
  });
});

describe("登记 ≠ 在线", () => {
  it("读名单**不得**顺带逐行查在线：名单里带着 `live` 也照样显示「未知」，等人点「检查」", async () => {
    const s = await readOnce();
    expect(ops("bus-list")).toHaveLength(1);
    const states = [...s.element.querySelectorAll(".cc-bus-online")].map((e) => e.textContent);
    expect(states).toEqual([copyText("ccBus.row.onlineUnknown"), copyText("ccBus.row.onlineUnknown")]);
    const txt = s.element.querySelector(".cc-bus-status")?.textContent ?? "";
    expect(txt).toContain(copyText("ccBus.summary.registered", { n: "2" }));
    expect(txt).toContain(copyText("ccBus.summary.notOnline"));
  });

  it("点某一行的「检查」只改那一行", async () => {
    const s = await readOnce();
    const rows = [...s.element.querySelectorAll<HTMLElement>(".cc-bus-row")];
    (rows[1].querySelector(".cc-bus-check") as HTMLButtonElement).click();
    await flush();
    expect(ops("bus-list")).toHaveLength(2);
    expect(rows[1].querySelector(".cc-bus-online")?.textContent).toBe(copyText("ccBus.check.online"));
    expect(rows[0].querySelector(".cc-bus-online")?.textContent).toBe(copyText("ccBus.row.onlineUnknown"));
  });

  it("查失败 ≠ 不在线（不能把网络抖动报成 agent 死了）", async () => {
    const s = await readOnce();
    serve(() => {
      throw new Error("connection reset");
    });
    const row = s.element.querySelector<HTMLElement>(".cc-bus-row")!;
    (row.querySelector(".cc-bus-check") as HTMLButtonElement).click();
    await flush();
    const el = row.querySelector<HTMLElement>(".cc-bus-online")!;
    expect(el.textContent).toContain("connection reset");
    expect(el.textContent).not.toBe(copyText("ccBus.check.offline"));
    expect(el.dataset.state).toBe("error");
  });
});

describe("如实呈现", () => {
  it("读取失败要说清，不能留空面板让人以为「没有 agent」", async () => {
    const s = await readOnce(() => {
      throw new Error("ssh timeout");
    });
    expect(s.element.querySelector(".cc-bus-status")?.textContent ?? "").toContain("ssh timeout");
    expect(s.element.querySelectorAll(".cc-bus-row")).toHaveLength(0);
  });

  it("名单是空的 ⇒ 说「没有」，不留空白", async () => {
    const s = await readOnce(() => ({ agents: [] }));
    expect(s.element.querySelector(".cc-bus-list")?.textContent).toBe(copyText("ccBus.render.empty"));
  });

  it("★ 无远端时**仍能读本机**，只是说清没有远端", async () => {
    serve(() => ROSTER, []);
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    expect((s.element.querySelector(".cc-bus-read") as HTMLButtonElement).disabled).toBe(false);
    expect(s.element.querySelector(".cc-bus-status")?.textContent).toContain(copyText("ccBus.machine.noRemote"));
    const sel = s.element.querySelector(".cc-bus-origin") as HTMLSelectElement;
    expect([...sel.options].map((o) => o.value)).toEqual(["<local>"]);
  });

  it("每一行带上它登记的地址", async () => {
    const s = await readOnce();
    const metas = [...s.element.querySelectorAll(".cc-bus-meta")].map((e) => e.textContent);
    expect(metas).toEqual(["proj_cc:0.0", "KVM_cc:0.0"]);
  });
});

/** 收件箱的一封信（规范信封的形状；占位值）。 */
function env(over: Record<string, unknown>): Record<string, unknown> {
  return {
    id: "m19a1b2c3d4e0001",
    ts: "2026-07-26T12:06:19Z",
    from: "peer-a",
    to: "proj-b",
    tier: 2,
    act: "inform",
    act_said: "SAID-inform",
    title: "占位标题",
    re: null,
    refs: [],
    text: "",
    ...over,
  };
}

describe("收件箱", () => {
  it("收件箱按需读，一次一个 agent", async () => {
    const s = await readOnce((op) =>
      op === "bus-inbox"
        ? { messages: [env({ from: "KVM-cc", title: "A 好了" })], skipped: 0, truncated: false }
        : ROSTER,
    );
    const row = s.element.querySelector<HTMLElement>(".cc-bus-row")!;
    (row.querySelector(".cc-bus-inbox") as HTMLButtonElement).click();
    await flush();
    expect(ops("bus-inbox")).toEqual([["devbox", "bus-inbox", { id: "proj_cc" }]]);
    expect(row.querySelector(".cc-bus-detail")?.textContent).toContain("KVM-cc");
    expect(row.querySelector(".cc-bus-detail")?.textContent).toContain("A 好了");
  });

  it("收件箱一行的拼法走文案表：时刻 · 发信方 · 动作 · 标题 · 正文，缺值写「—」", async () => {
    const s = await readOnce((op) =>
      op === "bus-inbox"
        ? {
            messages: [env({ act: "ask", act_said: "SAID-ask", title: "要不要合", text: "hello" }), env({ from: "", ts: "", title: "只有标题" })],
            skipped: 0,
            truncated: false,
          }
        : ROSTER,
    );
    const row = s.element.querySelector<HTMLElement>(".cc-bus-row")!;
    (row.querySelector(".cc-bus-inbox") as HTMLButtonElement).click();
    await flush();
    const lines = [...row.querySelectorAll(".cc-bus-msg-line")].map((l) => l.textContent);
    const none = copyText("ccBus.inbox.missing");
    expect(lines).toEqual([
      copyText("ccBus.inbox.line", { ts: "2026-07-26T12:06:19Z", from: "peer-a", act: "SAID-ask", title: "要不要合", text: "hello" }),
      copyText("ccBus.inbox.line", { ts: none, from: none, act: "SAID-inform", title: "只有标题", text: "" }),
    ]);
  });

  it("空收件箱要说「空」，不能留个空白让人以为坏了", async () => {
    const s = await readOnce((op) => (op === "bus-inbox" ? { messages: [], skipped: 0, truncated: false } : ROSTER));
    const row = s.element.querySelector<HTMLElement>(".cc-bus-row")!;
    (row.querySelector(".cc-bus-inbox") as HTMLButtonElement).click();
    await flush();
    expect(row.querySelector(".cc-bus-detail")?.textContent).toBe(copyText("ccBus.inbox.empty"));
  });
});
