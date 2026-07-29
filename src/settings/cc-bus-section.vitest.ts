// B03 批一：cc-bus 驾驶舱的 DOM 行为断言。
// 守的是三条**红线级**性质，不是渲染细节：
//   ① 构造时不发远端请求（不预取、不轮询）；
//   ② 「登记」与「在线」分开呈现——读状态时**不得**顺带全量查在线；
//   ③ 脏数据的 skipped 计数如实显示，不假装干净。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
// **mock 掉 accounts 模块**：`fetchAccounts` 带 TTL 缓存，跨测试会泄漏上一条的结果
// （实测：第一条测试的账号列表会被后面"取不到账号"那条读到）。它自己有测试，
// 这里只需要它的返回值，不该顺带重测它的缓存。
vi.mock("../accounts", () => ({
  fetchAccounts: vi.fn(),
  selectableAccounts: (st: { accounts?: { mode: string; loggedIn: boolean; exists: boolean }[] }) =>
    (st.accounts ?? []).filter((a) => a.mode === "isolated" && a.loggedIn && a.exists),
}));

import { CcBusSection } from "./cc-bus-section";
import { invoke } from "@tauri-apps/api/core";
import { fetchAccounts } from "../accounts";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;
const mockFetchAccounts = fetchAccounts as unknown as ReturnType<typeof vi.fn>;

const STATE = {
  agents: [
    { id: "proj_cc", pane: "proj_cc:0.0", registered_at: "2026-07-28T11:48:32-07:00" },
    { id: "KVM_cc", pane: "KVM_cc:0.0", registered_at: "2026-07-18T07:26:31-07:00" },
  ],
  spawned: [{ id: "proj_cc", dir: "/home/zbl/proj", spawned_at: "2026-07-28T11:48:00-07:00", task: "t" }],
  skipped: 8, // 任意非零值，只为验证"如实显示"；真实盘面是 5（15 行里 5 畸形 + 3 空行）
};

/** 等微任务队列排空（section 内部是 async invoke 链）。 */
const flush = async () => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
};

beforeEach(() => {
  mockInvoke.mockReset();
  mockFetchAccounts.mockReset();
  // 默认：拿不到账号（多数测试不关心账号，只留「基座」一项）
  mockFetchAccounts.mockResolvedValue({ origin: "aya", available: false, accounts: [] });
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
    // 账号列表经 `fetchAccounts`（已 mock），不走 invoke；这里只该看到列远端那一次
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
      account: "", // L2：空串 = 显式基座，**不存在"什么都不传"这一档**
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

describe("B03 审计修复：驾驶舱如实呈现 + 两步确认不可绕过", () => {
  // 真实盘面形态：agents 与 spawned 只有部分交集（实测 37 / 7 / 交集 2）
  const SKEWED = {
    agents: [
      { id: "both_cc", pane: "both_cc:0.0", registered_at: "2026-07-28T11:00:00-07:00" },
      { id: "onlyreg_cc", pane: "onlyreg_cc:0.0", registered_at: "2026-07-28T11:00:00-07:00" },
    ],
    spawned: [
      { id: "both_cc", dir: "/d/both", spawned_at: "2026-07-28T10:00:00-07:00", task: "t" },
      { id: "ghost_cc", dir: "/d/ghost", spawned_at: "2026-07-18T10:00:00-07:00", task: "t2" },
      { id: "ghost2_cc", dir: "/d/g2", spawned_at: "2026-07-18T10:00:00-07:00", task: "t3" },
    ],
    skipped: 0,
  };

  const load = async (state: unknown) => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "read_cc_bus_state") return state;
      throw new Error(cmd);
    });
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    (s.element.querySelector(".cc-bus-read") as HTMLButtonElement).click();
    await flush();
    return s;
  };

  it("【阻塞-1】spawn 过但未登记的 agent 必须**渲染出来**，不能只计数不显示", async () => {
    const s = await load(SKEWED);
    const ids = [...s.element.querySelectorAll<HTMLElement>(".cc-bus-row")].map(
      (r) => r.dataset.agentId,
    );
    expect(ids).toContain("ghost_cc");
    expect(ids).toContain("ghost2_cc");
    expect(ids).toHaveLength(4); // 2 登记 + 2 未登记
  });

  it("【阻塞-1】头条数字必须与可见行数自洽（`其中` 只能数交集）", async () => {
    const s = await load(SKEWED);
    const txt = s.element.querySelector(".cc-bus-status")?.textContent ?? "";
    expect(txt).toContain("登记 2 个");
    expect(txt).toContain("其中 spawn 派生 1 个"); // 交集是 1，不是 spawned 全集 3
    expect(txt).toContain("另有 2 个 spawn 过但未登记");
    expect(txt).not.toContain("其中 spawn 的 3 个");
  });

  it("【阻塞-1】未登记的行要标注出来，别让人以为它在总线上", async () => {
    const s = await load(SKEWED);
    const ghost = [...s.element.querySelectorAll<HTMLElement>(".cc-bus-row")].find(
      (r) => r.dataset.agentId === "ghost_cc",
    )!;
    expect(ghost.querySelector(".cc-bus-meta")?.textContent).toContain("未在 agents.tsv 登记");
    expect(ghost.querySelector(".cc-bus-meta")?.textContent).toContain("/d/ghost");
  });

  it("【重要-1】武装后改目录，再点必须**重新确认**而不是用新值执行", async () => {
    const s = await load(SKEWED);
    mockInvoke.mockClear();
    const dir = s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!;
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    dir.value = "/home/zbl/a";
    btn.click();
    await flush();
    expect(btn.textContent).toBe("确认派生");
    // 偷偷换个目录
    dir.value = "/home/zbl/b";
    dir.dispatchEvent(new Event("input"));
    btn.click();
    await flush();
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_spawn")).toHaveLength(0);
    expect(s.element.querySelector(".cc-bus-spawn-out")?.textContent).toContain("/home/zbl/b");
  });

  it("【重要-1】武装后改 tool 同样要重新确认", async () => {
    const s = await load(SKEWED);
    mockInvoke.mockClear();
    s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!.value = "/d";
    const tool = s.element.querySelector<HTMLSelectElement>(".cc-bus-spawn-tool")!;
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    btn.click();
    await flush();
    tool.value = "codex";
    tool.dispatchEvent(new Event("change"));
    btn.click();
    await flush();
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_spawn")).toHaveLength(0);
  });

  it("【重要-1】武装→清空目录→点击→填新目录→点一次，**不得**直接执行", async () => {
    const s = await load(SKEWED);
    mockInvoke.mockClear();
    const dir = s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!;
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    dir.value = "/d/first";
    btn.click(); // 武装
    await flush();
    dir.value = ""; // 清空
    btn.click(); // 原实现：只提示"请填目录"，仍处武装态
    await flush();
    dir.value = "/d/second";
    btn.click(); // 原实现：这一下就执行了，全程没出现确认文案
    await flush();
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_spawn")).toHaveLength(0);
    expect(btn.textContent).toBe("确认派生"); // 应该是刚武装，不是已执行
  });

  it("【重要-1】UI 不得承诺代码没实现的行为（原文案写了「点别处不算」）", () => {
    const s = new CcBusSection();
    const all = s.element.textContent ?? "";
    expect(all).not.toContain("点别处不算");
  });

  it("参数不变时，第二次点击仍应正常执行（别把功能修没了）", async () => {
    const s = await load(SKEWED);
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "cc_bus_spawn") return "已 spawn";
      if (cmd === "read_cc_bus_state") return SKEWED;
      throw new Error(cmd);
    });
    s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!.value = "/d/x";
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    btn.click();
    await flush();
    btn.click();
    await flush();
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_spawn")).toHaveLength(1);
    expect(btn.textContent).toBe("派生");
  });
});

describe("B03 审计修复：指纹比对是承重机制（隔离测试）", () => {
  const ST = { agents: [], spawned: [], skipped: 0 };
  const load = async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "read_cc_bus_state") return ST;
      throw new Error(cmd);
    });
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    return s;
  };

  // **不派发 input/change 事件**地改值 —— 绕开解除武装的监听器，
  // 单独把指纹比对这条防线暴露出来。上一轮变异检查发现：两个机制互相掩盖，
  // 只测"用户手动输入"路径的话，指纹比对被删掉也测不出来。
  it("程序化改目录（无事件）后再点，仍必须重新确认", async () => {
    const s = await load();
    mockInvoke.mockClear();
    const dir = s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!;
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    dir.value = "/d/one";
    btn.click();
    await flush();
    expect(btn.textContent).toBe("确认派生");
    dir.value = "/d/two"; // 直接赋值，不派发事件
    btn.click();
    await flush();
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_spawn")).toHaveLength(0);
    expect(s.element.querySelector(".cc-bus-spawn-out")?.textContent).toContain("参数已改动");
  });

  it("程序化改 task（无事件）也算参数变化", async () => {
    const s = await load();
    mockInvoke.mockClear();
    s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!.value = "/d";
    const task = s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-task")!;
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    btn.click();
    await flush();
    task.value = "偷偷换的任务";
    btn.click();
    await flush();
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_spawn")).toHaveLength(0);
  });

  it("清空目录后按钮不得仍显示「确认派生」（否则按钮在撒谎）", async () => {
    // 这条守的是 `disarmSpawn()` 在空目录分支里的**可观测**作用。
    // 它在有指纹比对的前提下不产生绕过（变异实测：删掉它仍无法一击直达），
    // 但**按钮文案会与实际状态不符**——按钮说"确认派生"，而当前根本没有可派生的目录。
    // 不给它一条会红的断言，它就是一行无门禁的代码。
    const s = await load();
    const dir = s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!;
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    dir.value = "/d/first";
    btn.click();
    await flush();
    expect(btn.textContent).toBe("确认派生");
    dir.value = "";
    btn.click();
    await flush();
    expect(btn.textContent).toBe("派生");
    expect(s.element.querySelector(".cc-bus-spawn-out")?.textContent).toContain("请先填工作目录");
  });

  it("清空目录（无事件）再填新的，也必须重新确认", async () => {
    const s = await load();
    mockInvoke.mockClear();
    const dir = s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!;
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    dir.value = "/d/first";
    btn.click();
    await flush();
    dir.value = "";
    btn.click();
    await flush();
    dir.value = "/d/second";
    btn.click();
    await flush();
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_spawn")).toHaveLength(0);
  });
});

describe("L2：spawn 必须表态用哪个账号（B03 审计重要-5）", () => {
  const ACCTS = {
    origin: "aya",
    available: true,
    error: null,
    meta: null,
    accounts: [
      { name: "z", email: "z@x", configDir: "/a/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true },
      { name: "b", email: "b@x", configDir: "/a/b", isDefault: false, mode: "isolated", exists: true, loggedIn: true },
      // 不可选的：未登录 / in-place 逃生口 —— 不该出现在下拉里
      { name: "gone", email: "", configDir: "/a/g", isDefault: false, mode: "isolated", exists: true, loggedIn: false },
      { name: "inplace", email: "", configDir: "/a/i", isDefault: false, mode: "in-place", exists: true, loggedIn: true },
    ],
  };
  const load = async () => {
    mockFetchAccounts.mockResolvedValue(ACCTS);
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "read_cc_bus_state") return STATE;
      if (cmd === "cc_bus_spawn") return "已 spawn";
      throw new Error(cmd);
    });
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    return s;
  };

  it("默认必须是「基座」——不替用户默认花掉某个号的额度", async () => {
    const s = await load();
    const sel = s.element.querySelector<HTMLSelectElement>(".cc-bus-spawn-acct")!;
    expect(sel.value).toBe("");
    expect(sel.options[0].textContent).toContain("基座");
  });

  it("只列可选账号（未登录 / in-place 不进下拉）", async () => {
    const s = await load();
    const opts = [...s.element.querySelectorAll<HTMLOptionElement>(".cc-bus-spawn-acct option")].map(
      (o) => o.value,
    );
    expect(opts).toEqual(["", "z", "b"]);
    expect(opts).not.toContain("gone");
    expect(opts).not.toContain("inplace");
  });

  it("选了账号要原样传给后端；不选则传空串（后端翻成 --base）", async () => {
    const s = await load();
    s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!.value = "/d";
    const acct = s.element.querySelector<HTMLSelectElement>(".cc-bus-spawn-acct")!;
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    acct.value = "b";
    acct.dispatchEvent(new Event("change"));
    btn.click();
    await flush();
    btn.click();
    await flush();
    const calls = mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_spawn");
    expect(calls).toHaveLength(1);
    expect((calls[0][1] as Record<string, unknown>).account).toBe("b");
  });

  it("确认文案必须点名账号——「消耗额度」不说是哪个号的额度等于没说", async () => {
    const s = await load();
    s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!.value = "/d";
    const acct = s.element.querySelector<HTMLSelectElement>(".cc-bus-spawn-acct")!;
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    acct.value = "z";
    acct.dispatchEvent(new Event("change"));
    btn.click();
    await flush();
    const out = s.element.querySelector(".cc-bus-spawn-out")?.textContent ?? "";
    expect(out).toContain("账号 z");
    expect(out).toContain("消耗额度");
  });

  it("不选账号时文案要说「基座」，不能含糊", async () => {
    const s = await load();
    s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!.value = "/d";
    (s.element.querySelector(".cc-bus-spawn-go") as HTMLButtonElement).click();
    await flush();
    expect(s.element.querySelector(".cc-bus-spawn-out")?.textContent).toContain("基座");
  });

  it("武装后改账号必须重新确认（换号 = 换花谁的钱）", async () => {
    const s = await load();
    s.element.querySelector<HTMLInputElement>(".cc-bus-spawn-dir")!.value = "/d";
    const acct = s.element.querySelector<HTMLSelectElement>(".cc-bus-spawn-acct")!;
    const btn = s.element.querySelector<HTMLButtonElement>(".cc-bus-spawn-go")!;
    btn.click(); // 武装（基座）
    await flush();
    acct.value = "z"; // 程序化改值，不派发事件 —— 单独考指纹比对这道防线
    btn.click();
    await flush();
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "cc_bus_spawn")).toHaveLength(0);
  });

  it("账号取不到时只留「基座」，不能让人以为选了号而其实没生效", async () => {
    mockFetchAccounts.mockRejectedValue(new Error("daemon 太旧"));
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      throw new Error(cmd);
    });
    const s = new CcBusSection();
    document.body.appendChild(s.element);
    await flush();
    const opts = [...s.element.querySelectorAll<HTMLOptionElement>(".cc-bus-spawn-acct option")];
    expect(opts).toHaveLength(1);
    expect(opts[0].value).toBe("");
  });
});
