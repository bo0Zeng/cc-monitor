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
  // T03：Rust 侧 `Snippet` 现在带 warning（形态与盘上实况冲突时非 null）
  snippet_home: {
    text: '{"hooks":{"SessionStart":[{"hooks":[{"command":"$HOME/.local/bin/cc-register"}]}]}}',
    warning: null as string | null,
  },
  snippet_bare: {
    text: '{"hooks":{"SessionStart":[{"hooks":[{"command":"cc-register"}]}]}}',
    warning: null as string | null,
  },
  source: "/home/zbl/.claude/settings.json",
});

beforeEach(() => {
  mockInvoke.mockReset();
  document.body.replaceChildren();
});

describe("B04 四态不得被误渲染", () => {
  it("installed-at-path 是用户当前状态，**不能**被说成有问题", () => {
    const d = describeState({
      kind: "installed-at-path",
      command: "x",
      path: "$HOME/.local/bin/cc-register",
    });
    expect(d.tone).toBe("ok");
    expect(d.text).toContain("已装");
    expect(d.text).not.toContain("但");
  });

  it("path-missing **绝不能**被说成已装", () => {
    const d = describeState({
      kind: "path-missing",
      command: "x",
      path: "/gone/cc-register",
    });
    expect(d.tone).toBe("bad");
    expect(d.text).toContain("路径不存在");
    expect(d.text).not.toMatch(/^已装/);
  });

  it("四态各自的 ok 判定", () => {
    expect(describeState({ kind: "not-installed" }).tone).toBe("bad");
    expect(
      describeState({ kind: "installed-via-path", command: "cc-register" })
        .tone,
    ).toBe("ok");
    expect(
      describeState({ kind: "installed-at-path", command: "x", path: "/p" })
        .tone,
    ).toBe("ok");
    expect(
      describeState({ kind: "path-missing", command: "x", path: "/p" }).tone,
    ).toBe("bad");
  });

  it("渲染时 ok/bad 的 class 与 dataset 必须与状态一致", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "diagnose_local_cc_bus_hooks")
        return rep(
          {
            kind: "installed-at-path",
            command: "x",
            path: "$HOME/.local/bin/cc-register",
          },
          {
            kind: "path-missing",
            command: "y",
            path: "/gone/cc-bus-stop-hook",
          },
        );
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    const lines = [
      ...s.element.querySelectorAll<HTMLElement>(".cc-bus-hooks-state"),
    ];
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
      if (cmd === "diagnose_local_cc_bus_hooks")
        return rep({ kind: "not-installed" }, { kind: "not-installed" });
      throw new Error(`不该自动调 ${cmd}`);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    const called = mockInvoke.mock.calls.map((c) => c[0]);
    expect(called).toContain("diagnose_local_cc_bus_hooks");
    expect(called).not.toContain("diagnose_remote_cc_bus_hooks");
    expect(
      s.element.querySelector(".cc-bus-hooks-remote")?.textContent,
    ).toContain("尚未检查");
  });

  it("点「检查远端」才发，且带上选中的 origin", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "diagnose_local_cc_bus_hooks")
        return rep({ kind: "not-installed" }, { kind: "not-installed" });
      if (cmd === "diagnose_remote_cc_bus_hooks")
        return rep(
          { kind: "installed-via-path", command: "cc-register" },
          { kind: "not-installed" },
        );
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    (
      s.element.querySelector(".cc-bus-hooks-check-remote") as HTMLButtonElement
    ).click();
    await flush();
    const calls = mockInvoke.mock.calls.filter(
      (c) => c[0] === "diagnose_remote_cc_bus_hooks",
    );
    expect(calls).toHaveLength(1);
    expect(calls[0][1]).toEqual({ origin: "aya" });
  });

  it("待贴片段默认给 $HOME 形态，可切裸命令", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "diagnose_local_cc_bus_hooks")
        return rep({ kind: "not-installed" }, { kind: "not-installed" });
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    // T03：输出面现在由 `buildPasteBlock` 产出，`cc-bus-hooks-out` 落在它的根节点上
    const out =
      s.element.querySelector<HTMLTextAreaElement>(".paste-block-out")!;
    expect(out.value).toContain("$HOME/.local/bin/");
    const sel =
      s.element.querySelector<HTMLSelectElement>(".cc-bus-hooks-form")!;
    sel.value = "bare";
    sel.dispatchEvent(new Event("change"));
    expect(out.value).not.toContain("$HOME");
    expect(out.readOnly).toBe(true); // 只读，别让人以为改了这里就等于改了配置
  });

  it("note 非空必须展示（读取失败的原因不能吞掉）", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return [];
      if (cmd === "diagnose_local_cc_bus_hooks")
        return rep(
          { kind: "not-installed" },
          { kind: "not-installed" },
          "不是合法 JSON —— 未改动它",
        );
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    expect(
      s.element.querySelector(".cc-bus-hooks-note")?.textContent,
    ).toContain("不是合法 JSON");
  });

  it("**绝无写入路径**：源码里不得有任何写 settings 的 invoke", () => {
    const src = readFileSync(
      resolve(process.cwd(), "src/settings/cc-bus-hooks-section.ts"),
      "utf8",
    );
    const code = src
      .split("\n")
      .filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l))
      .join("\n");
    // 反向自检：守卫真的看到了代码（不是过滤成空串在空转）
    expect(code).toContain("class CcBusHooksSection");
    expect(code.length).toBeGreaterThan(1000);
    for (const bad of [
      "write_settings",
      "save_settings",
      "writeTextFile",
      "install_hooks",
      "apply_hooks",
    ]) {
      expect(code).not.toContain(bad);
    }
    // 只允许只读 invoke —— **扫描器用下面那条改进版**（原正则 /invoke<[^>]*>\(\s*"([a-z_]+)"/
    // 被审计实测出对 6 种写法完全看不见：不带类型参数 / 单引号 / 模板串 / 嵌套泛型 /
    // 驼峰命名 / 常量间接。白名单看不见新增的 invoke = 白名单是摆设）。
    // 具体断言见本文件末尾「用改进后的扫描器复查」那条，那里同时守着扫描器自身。
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
      if (cmd === "diagnose_local_cc_bus_hooks")
        return rep({ kind: "not-installed" }, { kind: "not-installed" });
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    // 应降级成"未配置远端"，而不是炸掉整个分节
    expect(
      s.element.querySelector(".cc-bus-hooks-remote")?.textContent,
    ).toContain("未配置远端");
    expect(
      (
        s.element.querySelector(
          ".cc-bus-hooks-check-remote",
        ) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  });

  it("返回非数组（对象）同样不得抛", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return { oops: 1 };
      if (cmd === "diagnose_local_cc_bus_hooks")
        return rep({ kind: "not-installed" }, { kind: "not-installed" });
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    expect(
      s.element.querySelector(".cc-bus-hooks-remote")?.textContent,
    ).toContain("未配置远端");
  });
});

describe("B04 审计修复：第五态、兜底、以及守卫本身", () => {
  it("【B04-4】unknown 要中性，既不说已装也不说未装", () => {
    const d = describeState({
      kind: "unknown",
      command: 'sh -c "cc-register"',
    });
    expect(d.tone).toBe("unknown");
    expect(d.text).toContain("无法判断");
    expect(d.text).not.toContain("未装");
    expect(d.text).not.toMatch(/^已装/);
  });

  it("【建议】后端加第六态时不得整个炸掉（原实现无 default，d.ok 当场抛）", () => {
    // 故意传一个类型上不存在的 kind——模拟后端先行升级
    const d = describeState({ kind: "brand-new-kind" } as unknown as Parameters<
      typeof describeState
    >[0]);
    expect(d.tone).toBe("unknown");
    expect(d.text).toContain("未知状态");
  });

  it("【B04-6】不得再宣称「与本机现状一致」（那是写死的，snippet 根本不看诊断）", () => {
    const s2 = new CcBusHooksSection();
    const all = s2.element.textContent ?? "";
    expect(all).not.toContain("与本机现状一致");
  });

  it("【B04-2】invoke 白名单必须看得见各种写法，而不只是「带类型参数+双引号」", () => {
    // 审计实测：原正则 /invoke<[^>]*>\(\s*"([a-z_]+)"/g 对 6 种写法完全看不见。
    // 这条测试守的是**扫描器本身**——它若只认一种写法，白名单就是摆设。
    const scan = (code: string): string[] =>
      [
        ...code.matchAll(
          /invoke\s*(?:<[^(]*?>)?\s*\(\s*[`'"]([A-Za-z_][A-Za-z0-9_]*)[`'"]/g,
        ),
      ].map((m) => m[1]);
    const forms = [
      ['invoke<void>("new_cmd")', "new_cmd"],
      ['invoke("new_cmd")', "new_cmd"],
      ["invoke<void>('new_cmd')", "new_cmd"],
      ["invoke<void>(`new_cmd`)", "new_cmd"],
      ['invoke<Record<string, string>>("new_cmd")', "new_cmd"],
      ['invoke<void>("newCmd2")', "newCmd2"],
      ['invoke<void>(\n  "new_cmd",\n)', "new_cmd"],
    ] as const;
    for (const [code, want] of forms) {
      expect(scan(code), `这种写法必须被看见: ${code}`).toContain(want);
    }
  });

  it("【B04-2】本文件仍只碰那三条**只读**命令（C04d 批 3 起认包装层形态）", () => {
    // **这条守的性质与 C04a 那条 119 命令守卫不同，所以它不被替代**：
    // C04a 只保证「名字存在」；这一条保证「**这个面板只碰只读命令**」——
    // 钩子诊断面板绝不该有写操作，那是 B04 立的安全不变量。
    //
    // **C04d 批 3 起本文件走包装层**（`commands.xxx()`），裸 `invoke("name")` 已不存在
    // ⇒ 扫描器必须同时认两种形态，否则这条会退化成「扫到空集」的假绿。
    // 上面那组 `forms` 表测的是**裸形态**的扫描器（仍要留着：其它文件还在用裸形态，
    // 且 C04d 后续批次迁完前它一直有效）。
    const src = readFileSync(
      resolve(process.cwd(), "src/settings/cc-bus-hooks-section.ts"),
      "utf8",
    );
    const code = src
      .split("\n")
      .filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l))
      .join("\n");
    const bare = [
      ...code.matchAll(/invoke\s*(?:<[^(]*?>)?\s*\(\s*[`'"]([A-Za-z_][A-Za-z0-9_]*)[`'"]/g),
    ].map((m) => m[1]);
    const wrapped = [...code.matchAll(/\bcommands\.([a-z_][a-z_0-9]*)\s*\(/g)].map((m) => m[1]);
    const found = new Set([...bare, ...wrapped]);
    // 反向自检：**真扫到了东西**。空集必须是失败而不是「通过」——
    // 迁移把调用形态换掉时，这一格正是会静默变空的地方。
    expect(found.size, "一条命令都没扫到——扫描器与当前调用形态脱节了").toBeGreaterThan(0);
    expect(found).toEqual(
      new Set([
        "list_remote_mcp_origins",
        "diagnose_local_cc_bus_hooks",
        "diagnose_remote_cc_bus_hooks",
      ]),
    );
  });
});

// ===== T03 审计阻塞 2：Rust 报的 warning 必须真上屏 =====
//
// 审计实测：删掉本 section 的 warning 接线，56 项**全绿**——而我在 T03 的 commit message 里
// 写着「UI 侧另有测试钉住它真的上屏」，**那句话是假的**。
// 原因：fixture 把 `warning` 恒设 `null`，没有一条测试喂过非空 warning；
// `paste-block.vitest.ts` 那条只证明"组件收到 warning 会显示"，
// 不证明**这个消费者把 Rust 的 warning 接上了**。
// 这正是 T02 教训（纯函数被断言 ≠ 它上了屏）原样重演，还被写成了已完成的门禁。
describe("T03：形态与盘上实况冲突的警示必须上屏", () => {
  const WARN =
    "你选的是 $HOME 显式路径形态，但 $HOME/.local/bin/ 下**找不到** cc-register";

  function repWithWarning(): ReturnType<typeof rep> {
    const r = rep({ kind: "not-installed" }, { kind: "not-installed" });
    r.snippet_home = { ...r.snippet_home, warning: WARN };
    return r;
  }

  it("home 形态带 warning → .cc-bus-hooks-form-warning 非 hidden 且文案上屏", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return [];
      if (cmd === "diagnose_local_cc_bus_hooks") return repWithWarning();
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    const w = s.element.querySelector<HTMLElement>(
      ".cc-bus-hooks-form-warning",
    )!;
    expect(w.hidden).toBe(false);
    expect(w.textContent).toContain("找不到");
  });

  it("切到 bare 形态（那份 warning 为 null）→ 警示隐藏", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return [];
      if (cmd === "diagnose_local_cc_bus_hooks") return repWithWarning();
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    const sel =
      s.element.querySelector<HTMLSelectElement>(".cc-bus-hooks-form")!;
    sel.value = "bare";
    sel.dispatchEvent(new Event("change"));
    const w = s.element.querySelector<HTMLElement>(
      ".cc-bus-hooks-form-warning",
    )!;
    expect(w.hidden).toBe(true);
    expect(w.textContent).toBe("");
  });

  it("远端那份 warning 也必须上屏（此前远端算了 Snippet 却到不了屏幕）", async () => {
    const remote = rep({ kind: "not-installed" }, { kind: "not-installed" });
    remote.source = "[aya] ~/.claude/settings.json";
    remote.snippet_home = { ...remote.snippet_home, warning: "远端那边找不到" };
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["aya"];
      if (cmd === "diagnose_local_cc_bus_hooks")
        return rep({ kind: "not-installed" }, { kind: "not-installed" });
      if (cmd === "diagnose_remote_cc_bus_hooks") return remote;
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    const btn = [...s.element.querySelectorAll("button")].find((b) =>
      (b.textContent ?? "").includes("检查远端"),
    )!;
    btn.click();
    await flush();
    const box = s.element.querySelector(".cc-bus-hooks-remote")!;
    expect(box.querySelector(".cc-bus-hooks-diag-warning")).not.toBeNull();
    expect(box.textContent).toContain("远端那边找不到");
    expect(box.textContent).toContain("$HOME 显式路径形态");
  });

  it("待贴片段的标题要说清它基于哪一端（否则用户以为是远端盘面）", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return [];
      if (cmd === "diagnose_local_cc_bus_hooks")
        return rep({ kind: "not-installed" }, { kind: "not-installed" });
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    expect(s.element.textContent).toContain("待贴片段（基于本机盘面）");
  });

  it("两份都没 warning → 一开始就隐藏（不许留一个空框占位）", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return [];
      if (cmd === "diagnose_local_cc_bus_hooks")
        return rep({ kind: "not-installed" }, { kind: "not-installed" });
      throw new Error(cmd);
    });
    const s = new CcBusHooksSection();
    document.body.appendChild(s.element);
    await flush();
    expect(
      s.element.querySelector<HTMLElement>(".cc-bus-hooks-form-warning")!
        .hidden,
    ).toBe(true);
  });
});
