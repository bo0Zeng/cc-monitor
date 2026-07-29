// B04 UI 断言。守的是**四态不得被误渲染**这条要害，以及"绝不写入"这条红线。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));

import { CcBusHooksSection, describeState } from "./cc-bus-hooks-section";
import { invoke } from "@tauri-apps/api/core";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;
const flush = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
};

const rep = (ss: unknown, stop: unknown, note = "") => ({
  diagnosis: { session_start: ss, stop, note },
  snippet_home: '{"hooks":{"SessionStart":[{"hooks":[{"command":"$HOME/.local/bin/cc-register"}]}]}}',
  snippet_bare: '{"hooks":{"SessionStart":[{"hooks":[{"command":"cc-register"}]}]}}',
  source: "/home/zbl/.claude/settings.json",
});

beforeEach(() => {
  mockInvoke.mockReset();
  document.body.replaceChildren();
});

describe("B04 四态不得被误渲染", () => {
  it("installed-at-path 是用户当前状态，**不能**被说成有问题", () => {
    const d = describeState({ kind: "installed-at-path", command: "x", path: "$HOME/.local/bin/cc-register" });
    expect(d.ok).toBe(true);
    expect(d.text).toContain("已装");
    expect(d.text).not.toContain("但");
  });

  it("path-missing **绝不能**被说成已装", () => {
    const d = describeState({ kind: "path-missing", command: "x", path: "/gone/cc-register" });
    expect(d.ok).toBe(false);
    expect(d.text).toContain("路径不存在");
    expect(d.text).not.toMatch(/^已装/);
  });

  it("四态各自的 ok 判定", () => {
    expect(describeState({ kind: "not-installed" }).ok).toBe(false);
    expect(describeState({ kind: "installed-via-path", command: "cc-register" }).ok).toBe(true);
    expect(describeState({ kind: "installed-at-path", command: "x", path: "/p" }).ok).toBe(true);
    expect(describeState({ kind: "path-missing", command: "x", path: "/p" }).ok).toBe(false);
  });

  it("渲染时 ok/bad 的 class 与 dataset 必须与状态一致", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "diagnose_local_cc_bus_hooks")
        return rep(
          { kind: "installed-at-path", command: "x", path: "$HOME/.local/bin/cc-register" },
          { kind: "path-missing", command: "y", path: "/gone/cc-bus-stop-hook" },
        );
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    const lines = [...s.element.querySelectorAll<HTMLElement>(".cc-bus-hooks-state")];
    expect(lines).toHaveLength(2);
    expect(lines[0].dataset.kind).toBe("installed-at-path");
    expect(lines[0].className).toContain("cc-bus-hooks-ok");
    expect(lines[1].dataset.kind).toBe("path-missing");
    expect(lines[1].className).toContain("cc-bus-hooks-bad");
  });
});

describe("B04 只读与文案", () => {
  it("本机诊断直接读（纯本地），远端**必须**点了才发", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "diagnose_local_cc_bus_hooks") return rep({ kind: "not-installed" }, { kind: "not-installed" });
      throw new Error(`不该自动调 ${cmd}`);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    const called = mockInvoke.mock.calls.map((c) => c[0]);
    expect(called).toContain("diagnose_local_cc_bus_hooks");
    expect(called).not.toContain("diagnose_remote_cc_bus_hooks");
    expect(s.element.querySelector(".cc-bus-hooks-remote")?.textContent).toContain("尚未检查");
  });

  it("点「检查远端」才发，且带上选中的 origin", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "diagnose_local_cc_bus_hooks") return rep({ kind: "not-installed" }, { kind: "not-installed" });
      if (cmd === "diagnose_remote_cc_bus_hooks")
        return rep({ kind: "installed-via-path", command: "cc-register" }, { kind: "not-installed" });
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    (s.element.querySelector(".cc-bus-hooks-check-remote") as HTMLButtonElement).click();
    await flush();
    const calls = mockInvoke.mock.calls.filter((c) => c[0] === "diagnose_remote_cc_bus_hooks");
    expect(calls).toHaveLength(1);
    expect(calls[0][1]).toEqual({ origin: "aya" });
  });

  it("待贴片段默认给 $HOME 形态，可切裸命令", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "diagnose_local_cc_bus_hooks") return rep({ kind: "not-installed" }, { kind: "not-installed" });
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    const out = s.element.querySelector<HTMLTextAreaElement>(".cc-bus-hooks-out")!;
    expect(out.value).toContain("$HOME/.local/bin/");
    const sel = s.element.querySelector<HTMLSelectElement>(".cc-bus-hooks-form")!;
    sel.value = "bare";
    sel.dispatchEvent(new Event("change"));
    expect(out.value).not.toContain("$HOME");
    expect(out.readOnly).toBe(true); // 只读，别让人以为改了这里就等于改了配置
  });

  it("note 非空必须展示（读取失败的原因不能吞掉）", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return [];
      if (cmd === "diagnose_local_cc_bus_hooks")
        return rep({ kind: "not-installed" }, { kind: "not-installed" }, "不是合法 JSON —— 未改动它");
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    expect(s.element.querySelector(".cc-bus-hooks-note")?.textContent).toContain("不是合法 JSON");
  });

  it("**绝无写入路径**：源码里不得有任何写 settings 的 invoke", () => {
    const src = readFileSync(resolve(process.cwd(), "src/settings/cc-bus-hooks-section.ts"), "utf8");
    const code = src
      .split("\n")
      .filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l))
      .join("\n");
    // 反向自检：守卫真的看到了代码（不是过滤成空串在空转）
    expect(code).toContain("class CcBusHooksSection");
    expect(code.length).toBeGreaterThan(1000);
    for (const bad of ["write_settings", "save_settings", "writeTextFile", "install_hooks", "apply_hooks"]) {
      expect(code).not.toContain(bad);
    }
    // 只允许这三条只读 invoke
    const invokes = [...code.matchAll(/invoke<[^>]*>\(\s*"([a-z_]+)"/g)].map((m) => m[1]);
    expect(new Set(invokes)).toEqual(
      new Set(["list_remote_mcp_origins", "diagnose_local_cc_bus_hooks", "diagnose_remote_cc_bus_hooks"]),
    );
  });

  it("文案要讲清为什么不代劳，而不是只说「请手动粘贴」", () => {
    const s = new CcBusHooksSection();
    const why = s.element.querySelector(".cc-bus-hooks-why")?.textContent ?? "";
    expect(why).toContain("共享");
    expect(why).toContain("覆盖");
    expect(why).toContain("安装脚本");
  });
});

describe("B04 对自己的 IPC 返回值也要防御", () => {
  it("list_remote_mcp_origins resolve 成 undefined 时不得抛（曾真的抛过）", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return undefined; // 不是 reject，是返回了怪东西
      if (cmd === "diagnose_local_cc_bus_hooks") return rep({ kind: "not-installed" }, { kind: "not-installed" });
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    // 应降级成"未配置远端"，而不是炸掉整个分节
    expect(s.element.querySelector(".cc-bus-hooks-remote")?.textContent).toContain("未配置远端");
    expect((s.element.querySelector(".cc-bus-hooks-check-remote") as HTMLButtonElement).disabled).toBe(true);
  });

  it("返回非数组（对象）同样不得抛", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return { oops: 1 };
      if (cmd === "diagnose_local_cc_bus_hooks") return rep({ kind: "not-installed" }, { kind: "not-installed" });
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    expect(s.element.querySelector(".cc-bus-hooks-remote")?.textContent).toContain("未配置远端");
  });
});
