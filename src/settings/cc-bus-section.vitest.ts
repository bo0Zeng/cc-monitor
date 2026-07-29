// B03 批一：cc-bus 驾驶舱的 DOM 行为断言。
// 守的是三条**红线级**性质，不是渲染细节：
//   ① 构造时不发远端请求（不预取、不轮询）；
//   ② 「登记」与「在线」分开呈现——读状态时**不得**顺带全量查在线；
//   ③ 脏数据的 skipped 计数如实显示，不假装干净。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { CcBusSection } from "./cc-bus-section";
import { invoke } from "@tauri-apps/api/core";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

const STATE = {
  agents: [
    { id: "proj_cc", pane: "proj_cc:0.0", registered_at: "2026-07-28T11:48:32-07:00" },
    { id: "KVM_cc", pane: "KVM_cc:0.0", registered_at: "2026-07-18T07:26:31-07:00" },
  ],
  spawned: [{ id: "proj_cc", dir: "/home/zbl/proj", spawned_at: "2026-07-28T11:48:00-07:00", task: "t" }],
  skipped: 8, // 盘面实况：spawned.tsv 15 行里 8 行坏
};

/** 等微任务队列排空（section 内部是 async invoke 链）。 */
const flush = async () => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
};

beforeEach(() => {
  mockInvoke.mockReset();
  document.body.replaceChildren();
});

describe("B03 cc-bus 驾驶舱：不预取、不轮询", () => {
  it("构造时只列远端，**绝不**读 cc-bus 状态", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      throw new Error(`不该在构造时调用 ${cmd}`);
    });
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    const called = mockInvoke.mock.calls.map((c) => c[0]);
    expect(called).toEqual(["list_remote_mcp_origins"]);
    expect(called).not.toContain("read_cc_bus_state");
    expect(s.element.querySelector(".cc-bus-status")?.textContent).toContain("尚未读取");
  });

  it("点「读取」才发一次 read_cc_bus_state", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "read_cc_bus_state") return STATE;
      throw new Error(cmd);
    });
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    (s.element.querySelector(".cc-bus-read") as HTMLButtonElement).click();
    await flush();
    const reads = mockInvoke.mock.calls.filter((c) => c[0] === "read_cc_bus_state");
    expect(reads).toHaveLength(1);
    expect(reads[0][1]).toEqual({ origin: "aya" });
  });

  it("源码里没有定时器（红线：不新增轮询）", () => {
    const src = readFileSync(resolve(process.cwd(), "src/settings/cc-bus-section.ts"), "utf8");
    // 只看**代码行**——文件头注释里写了"不得出现 setInterval"，按整文件 grep 会假红
    const code = src
      .split("\n")
      .filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l))
      .join("\n");
    expect(code).not.toMatch(/setInterval|requestAnimationFrame/);
    expect(code).not.toMatch(/setTimeout\s*\(/);
  });
});

describe("B03 登记 ≠ 在线", () => {
  const setup = async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "read_cc_bus_state") return STATE;
      throw new Error(cmd);
    });
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    (s.element.querySelector(".cc-bus-read") as HTMLButtonElement).click();
    await flush();
    return s;
  };

  it("读状态**不得**顺带查在线——否则一屏 37 个 agent 就是 37 次往返", async () => {
    const s = await setup();
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "check_cc_bus_agent_online")).toHaveLength(0);
    const states = [...s.element.querySelectorAll(".cc-bus-online")].map((e) => e.textContent);
    expect(states).toEqual(["在线未知", "在线未知"]);
  });

  it("点某一行的「检查」只查那一行", async () => {
    const s = await setup();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "check_cc_bus_agent_online") return true;
      throw new Error(cmd);
    });
    const rows = [...s.element.querySelectorAll<HTMLElement>(".cc-bus-row")];
    (rows[1].querySelector(".cc-bus-check") as HTMLButtonElement).click();
    await flush();
    const checks = mockInvoke.mock.calls.filter((c) => c[0] === "check_cc_bus_agent_online");
    expect(checks).toHaveLength(1);
    expect(checks[0][1]).toEqual({ origin: "aya", id: "KVM_cc" });
    expect(rows[1].querySelector(".cc-bus-online")?.textContent).toBe("在线");
    // 另一行不受影响，仍是未知
    expect(rows[0].querySelector(".cc-bus-online")?.textContent).toBe("在线未知");
  });

  it("查失败 ≠ 不在线（不能把网络抖动报成 agent 死了）", async () => {
    const s = await setup();
    mockInvoke.mockImplementation(async () => {
      throw new Error("connection reset");
    });
    const row = s.element.querySelector<HTMLElement>(".cc-bus-row")!;
    (row.querySelector(".cc-bus-check") as HTMLButtonElement).click();
    await flush();
    const el = row.querySelector(".cc-bus-online")!;
    expect(el.textContent).toContain("查不到");
    expect(el.textContent).not.toBe("不在线");
    expect(el.className).toContain("cc-bus-online-error");
  });
});

describe("B03 脏数据如实呈现", () => {
  it("skipped 计数显示出来，不假装干净", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "read_cc_bus_state") return STATE;
      throw new Error(cmd);
    });
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    (s.element.querySelector(".cc-bus-read") as HTMLButtonElement).click();
    await flush();
    const txt = s.element.querySelector(".cc-bus-status")?.textContent ?? "";
    expect(txt).toContain("8 条无法解析");
    expect(txt).toContain("登记 2 个");
    expect(txt).toContain("登记」不等于「在线");
  });

  it("skipped=0 时不显示那句（不制造无谓噪音）", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "read_cc_bus_state") return { ...STATE, skipped: 0 };
      throw new Error(cmd);
    });
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    (s.element.querySelector(".cc-bus-read") as HTMLButtonElement).click();
    await flush();
    expect(s.element.querySelector(".cc-bus-status")?.textContent).not.toContain("无法解析");
  });

  it("读取失败要说清，不能留空面板让人以为「没有 agent」", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      throw new Error("ssh timeout");
    });
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    (s.element.querySelector(".cc-bus-read") as HTMLButtonElement).click();
    await flush();
    const txt = s.element.querySelector(".cc-bus-status")?.textContent ?? "";
    expect(txt).toContain("读取失败");
    expect(txt).toContain("ssh timeout");
  });

  it("无远端时禁用读取并说明原因", async () => {
    mockInvoke.mockImplementation(async () => []);
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    expect((s.element.querySelector(".cc-bus-read") as HTMLButtonElement).disabled).toBe(true);
    expect(s.element.querySelector(".cc-bus-status")?.textContent).toContain("未配置远端");
  });

  it("spawn 派生与自行登记要区分开", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "read_cc_bus_state") return STATE;
      throw new Error(cmd);
    });
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    (s.element.querySelector(".cc-bus-read") as HTMLButtonElement).click();
    await flush();
    const metas = [...s.element.querySelectorAll(".cc-bus-meta")].map((e) => e.textContent ?? "");
    expect(metas[0]).toContain("cc-spawn 派生");
    expect(metas[0]).toContain("/home/zbl/proj");
    expect(metas[1]).toContain("自行登记");
  });
});

describe("B03 批二：派活 / 收信 / 图形化 spawn", () => {
  const setup = async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "read_cc_bus_state") return STATE;
      throw new Error(cmd);
    });
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    (s.element.querySelector(".cc-bus-read") as HTMLButtonElement).click();
    await flush();
    return s;
  };

  it("收件箱按需读，一次一个 agent", async () => {
    const s = await setup();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "read_cc_bus_inbox")
        return [
          { from: "KVM_cc", ts: "2026-07-26T05:06:19-07:00", text: "A 就绪", class: "direct" },
        ];
      throw new Error(cmd);
    });
    const row = s.element.querySelector<HTMLElement>(".cc-bus-row")!;
    (row.querySelector(".cc-bus-inbox") as HTMLButtonElement).click();
    await flush();
    const calls = mockInvoke.mock.calls.filter((c) => c[0] === "read_cc_bus_inbox");
    expect(calls).toHaveLength(1);
    expect(calls[0][1]).toEqual({ origin: "aya", id: "proj_cc" });
    expect(row.querySelector(".cc-bus-detail")?.textContent).toContain("KVM_cc");
    expect(row.querySelector(".cc-bus-detail")?.textContent).toContain("A 就绪");
  });

  it("空收件箱要说「空」，不能留个空白让人以为坏了", async () => {
    const s = await setup();
    mockInvoke.mockImplementation(async () => []);
    const row = s.element.querySelector<HTMLElement>(".cc-bus-row")!;
    (row.querySelector(".cc-bus-inbox") as HTMLButtonElement).click();
    await flush();
    expect(row.querySelector(".cc-bus-detail")?.textContent).toContain("空的");
  });

  it("发消息把原文原样交给后端（引用是后端的事，前端不得自己加工）", async () => {
    const s = await setup();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "cc_bus_send") return "queued";
      throw new Error(cmd);
    });
    const row = s.element.querySelector<HTMLElement>(".cc-bus-row")!;
    const input = row.querySelector<HTMLInputElement>(".cc-bus-msg")!;
    const evil = "hi'; rm -rf ~; echo '";
    input.value = evil;
    (row.querySelector(".cc-bus-send") as HTMLButtonElement).click();
    await flush();
    const calls = mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_send");
    expect(calls).toHaveLength(1);
    expect(calls[0][1]).toEqual({ origin: "aya", id: "proj_cc", text: evil });
    expect(input.value).toBe(""); // 发完清空，免得手滑重发
  });

  it("空消息不发（不浪费一次往返）", async () => {
    const s = await setup();
    mockInvoke.mockClear();
    const row = s.element.querySelector<HTMLElement>(".cc-bus-row")!;
    row.querySelector<HTMLInputElement>(".cc-bus-msg")!.value = "   ";
    (row.querySelector(".cc-bus-send") as HTMLButtonElement).click();
    await flush();
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_send")).toHaveLength(0);
  });

  it("spawn 必须两步确认——第一次点只是武装，不得真起 agent", async () => {
    const s = await setup();
    mockInvoke.mockClear();
    s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!.value = "/home/zbl/proj";
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    btn.click();
    await flush();
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_spawn")).toHaveLength(0);
    expect(btn.textContent).toContain("确认");
    expect(s.element.querySelector(".cc-bus-spawn-out")?.textContent).toContain("消耗额度");
  });

  it("第二次点才真派生，且把 tool/dir/task 原样传下去", async () => {
    const s = await setup();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "cc_bus_spawn") return "已 spawn: proj_cc";
      if (cmd === "read_cc_bus_state") return STATE;
      throw new Error(cmd);
    });
    s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!.value = "/home/zbl/proj";
    s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-task")!.value = "分析架构";
    s.element.querySelector<HTMLSelectElement>(".cc-bus-spawn-tool")!.value = "codex";
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    btn.click();
    await flush();
    btn.click();
    await flush();
    const calls = mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_spawn");
    expect(calls).toHaveLength(1);
    expect(calls[0][1]).toEqual({
      origin: "aya",
      dir: "/home/zbl/proj",
      task: "分析架构",
      tool: "codex",
    });
    expect(btn.textContent).toBe("派生"); // 武装状态要复位，不能一直停在"确认"
  });

  it("目录为空时连武装都不该发生", async () => {
    const s = await setup();
    mockInvoke.mockClear();
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    btn.click();
    await flush();
    expect(btn.textContent).toBe("派生");
    expect(s.element.querySelector(".cc-bus-spawn-out")?.textContent).toContain("请先填工作目录");
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_spawn")).toHaveLength(0);
  });

  it("**零引用 launch IR**：spawn 走远端 exec，不经会话启动那套", () => {
    const src = readFileSync(resolve(process.cwd(), "src/settings/cc-bus-section.ts"), "utf8");
    const code = src
      .split("\n")
      .filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l))
      .join("\n");
    expect(code).not.toMatch(/from ["']\.\.\/launch/);
    expect(code).not.toMatch(/tryRenderCli|buildLaunchPlan|LAUNCH_DIMENSIONS/);
  });
});
