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
