// 〔AL1 · 2026-09-24〕机器页 ②「别名」（`设计/71` · `设计/70 §3.3`）。
//
// 这里钉的是**界面那一半**：清单从读回口开始 · 表单 ↔ 参数 · 两跳各在什么时候发 · 默认不动用户配置 ·
// 构造零 I/O。shell 文本长什么样、真 bash 执行下来对不对，归后端 `tests/bridge/account_aliases_tests.rs`
// —— 本文件一个字节的 shell 文本都不断言（前端也一个字节都不拼）。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { Alias } from "../../src/generated/Alias";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 8; i += 1) await Promise.resolve();
};

describe("suggestAliasName（从 launcher-diagnostics 搬来，行为一字不变）", () => {
  it("账号名 → `<名>cc`（与 `cc-acct-iso shellinit` 那一族逐字同形）", async () => {
    const { suggestAliasName } = await import("../../src/settings/machine-aliases");
    expect(suggestAliasName("z")).toBe("alphacc");
    expect(suggestAliasName("work")).toBe("workcc");
  });
  it("非法字符丢掉而不是换成下划线（`a.b` 与 `a_b` 不许撞名）；数字打头前缀 `_`；全非法 ⇒ 空串", async () => {
    const { suggestAliasName } = await import("../../src/settings/machine-aliases");
    expect(suggestAliasName("a.b")).toBe("abcc");
    expect(suggestAliasName("a_b")).toBe("a_bcc");
    expect(suggestAliasName("0")).toBe("_0cc");
    expect(suggestAliasName("...")).toBe("");
  });
});

describe("表单 ↔ 一条别名（纯函数）", () => {
  it("每一格都往返得回来（两向：表单 → 参数 → 表单 逐格相等）", async () => {
    const m = await import("../../src/settings/machine-aliases");
    const f = {
      ...m.emptyForm(),
      name: "convz",
      cwd: "/home/u/文档/c c",
      account: "z",
      tmux: "named" as const,
      tmuxName: "w",
      agent: "codex",
      model: "opus",
      launcher: "/usr/bin/claude",
      tmuxSize: "200x50",
      detach: true,
      busRegister: true,
      busNote: "备注",
      passthru: "--verbose --x",
    };
    const a = m.formToAlias(f);
    expect(a).toEqual({
      name: "convz",
      args: [
        "--cwd", "/home/u/文档/c c",
        "--account", "z",
        "--tmux=w",
        "--agent", "codex",
        "--model", "opus",
        "--launcher", "/usr/bin/claude",
        "--tmux-size", "200x50",
        "--detach",
        "--bus-register",
        "--bus-note", "备注",
        "--", "--verbose", "--x",
      ],
    });
    expect(m.aliasToForm(a)).toEqual(f);
  });

  it("`71 §5` 在控件这一侧：不进 tmux ⇒ 容器那几格一个都不出现；不 --detach ⇒ 不登记 cc-bus", async () => {
    const m = await import("../../src/settings/machine-aliases");
    const noTmux = m.formToAlias({
      ...m.emptyForm(),
      name: "a",
      tmuxSize: "80x24",
      detach: true,
      busRegister: true,
    });
    expect(noTmux.args).toEqual([]);
    const noDetach = m.formToAlias({
      ...m.emptyForm(),
      name: "b",
      tmux: "auto",
      busRegister: true,
      busNote: "x",
    });
    expect(noDetach.args).toEqual(["--tmux"]);
    expect(m.formToAlias({ ...m.emptyForm(), name: "c", account: m.BASE_CHOICE }).args).toEqual([
      "--base",
    ]);
  });

  it("认不出的参数不静默丢：原样进透传栏", async () => {
    const m = await import("../../src/settings/machine-aliases");
    expect(m.aliasToForm({ name: "x", args: ["--weird", "--tmux"] }).passthru).toBe("--weird");
  });
});

describe("buildAliasManager：两跳 ＋ 读回口 ＋ 默认不动用户配置", () => {
  let seen: Array<{ cmd: string; args?: unknown }>;
  let disk: Alias[];
  let problems: Array<{ name: string; message: string }>;

  beforeEach(() => {
    seen = [];
    disk = [
      { name: "alphacc", args: ["--account", "z"] },
      { name: "alphacct", args: ["--tmux", "--account", "z"] },
    ];
    problems = [];
    vi.resetModules();
    vi.doMock("../../src/ipc/commands", () => ({
      commands: {
        local_ccm_entry_status: () => {
          seen.push({ cmd: "local_ccm_entry_status" });
          return Promise.resolve({ message: "" });
        },
        aliases_read: () => {
          seen.push({ cmd: "aliases_read" });
          return Promise.resolve({
            aliasPath: "/h/.cc-monitor/account-aliases.sh",
            exists: true,
            aliases: disk.map((a) => ({ ...a, args: [...a.args] })),
            unparsed: ["alias x=ls（不是 `名字() { … }` 的形状）"],
            rcCandidates: [{ path: "/h/.bashrc", sourced: false }],
          });
        },
        aliases_render: (a: { aliases: Alias[] }) => {
          seen.push({ cmd: "aliases_render", args: a });
          return Promise.resolve({
            code: a.aliases.map((x) => `#${x.name}`).join("\n"),
            lines: [],
            problems,
            collisions: [],
          });
        },
        aliases_install: (a: { aliases: Alias[]; rcPath: string | null }) => {
          seen.push({ cmd: "aliases_install", args: a });
          disk = a.aliases;
          return Promise.resolve({
            aliasPath: "/h/.cc-monitor/account-aliases.sh",
            wroteAliasFile: true,
            wroteRc: false,
            notes: ["新开一个终端就能用"],
          });
        },
        cc_integration_scan_path: (a: { path: string }) => {
          seen.push({ cmd: "cc_integration_scan_path", args: a });
          return Promise.resolve({
            kind: "Custom",
            path: a.path,
            exists: true,
            has_ccm_block: false,
            ccm_block_version: null,
            conflicting_functions: [],
            manual_cleanup_hint: "",
            size_bytes: 1,
          });
        },
        cc_integration_install: (a: unknown) => {
          seen.push({ cmd: "cc_integration_install", args: a });
          return Promise.resolve();
        },
        ccm_user_path_status: () => {
          seen.push({ cmd: "ccm_user_path_status" });
          return Promise.resolve({
            supported: true,
            onUserPath: false,
            dir: "C:\\Users\\u\\.cc-monitor\\bin",
            error: null,
            addCommand: "（加的那段）",
            removeCommand: null,
          });
        },
      },
    }));
  });

  afterEach(async () => {
    const { __setHostOsForTests } = await import("../../src/settings/host-os");
    __setHostOsForTests(null);
    document.body.replaceChildren();
  });

  async function mount(os: "linux" | "windows" = "linux"): Promise<HTMLDetailsElement> {
    const { __setHostOsForTests } = await import("../../src/settings/host-os");
    __setHostOsForTests(os);
    const m = await import("../../src/settings/machine-aliases");
    const el = m.buildAliasManager({ loadAccounts: async () => ["z", "b", "0"] }) as HTMLDetailsElement;
    document.body.appendChild(el);
    await flush();
    return el;
  }

  async function open(el: HTMLDetailsElement): Promise<void> {
    el.open = true;
    el.dispatchEvent(new Event("toggle"));
    await flush();
  }

  const lastRendered = (): string[] => {
    const r = [...seen].reverse().find((c) => c.cmd === "aliases_render");
    return ((r?.args as { aliases: Alias[] }).aliases ?? []).map((a) => a.name);
  };
  const clickText = (root: HTMLElement, text: string): void => {
    const b = [...root.querySelectorAll<HTMLButtonElement>("button")].find(
      (x) => x.textContent === text,
    );
    if (!b) throw new Error(`找不到按钮「${text}」`);
    b.click();
  };

  it("★ 构造零 I/O；第一次展开**恰好**三发（本机 ccm 那一格 · 读回 · 渲染），再展开一发都不多", async () => {
    const el = await mount();
    expect(seen, "还没展开就发了 IPC").toEqual([]);
    await open(el);
    expect(seen.map((c) => c.cmd).sort()).toEqual(
      ["aliases_read", "aliases_render", "local_ccm_entry_status"].sort(),
    );
    const n = seen.length;
    el.open = false;
    el.dispatchEvent(new Event("toggle"));
    await open(el);
    expect(seen.length, "再展开又读了一遍").toBe(n);
  });

  it("清单从盘上那份开始；认不出的那一行原样说出来（写入时它会被去掉）", async () => {
    const el = await mount();
    await open(el);
    const rows = [...el.querySelectorAll(".machine-aliases-row code")].map((c) => c.textContent);
    expect(rows).toEqual(["alphacc", "alphacct"]);
    expect(el.querySelector(".machine-aliases-status")!.textContent).toContain("alias x=ls");
    expect(lastRendered()).toEqual(["alphacc", "alphacct"]);
  });

  it("表单「加进清单」⇒ 问后端要一次代码（第①跳），清单里多一条", async () => {
    const el = await mount();
    await open(el);
    const name = el.querySelector<HTMLInputElement>('input[placeholder="名字，如 alphacct"]')!;
    name.value = "mine";
    clickText(el, "加进清单");
    await flush();
    expect(lastRendered()).toEqual(["alphacc", "alphacct", "mine"]);
  });

  it("「删」⇒ 那一条从清单里走了，并重新渲染", async () => {
    const el = await mount();
    await open(el);
    const first = el.querySelector<HTMLElement>(".machine-aliases-row")!;
    clickText(first, "删");
    await flush();
    expect(lastRendered()).toEqual(["alphacct"]);
  });

  it("「为每个账号加一条」只补没有的，不重复（`alphacc` 已在），全非法的名字跳过", async () => {
    const el = await mount();
    await open(el);
    clickText(el, "为每个账号加一条");
    await flush();
    expect(lastRendered()).toEqual(["alphacc", "alphacct", "betacc", "_0cc"]);
  });

  it("有不合格的 ⇒ 写入按钮不给点，问题一条条上屏", async () => {
    problems = [{ name: "a", message: "`--account` 与 `--base` 只能选一个" }];
    const el = await mount();
    await open(el);
    const write = [...el.querySelectorAll<HTMLButtonElement>("button")].find(
      (b) => b.textContent === "写入",
    )!;
    expect(write.disabled).toBe(true);
    expect(el.querySelector(".machine-aliases-problems")!.textContent).toContain("只能选一个");
  });

  it("🔴 「写入」（第②跳）带的是当前清单，而 rc 默认是 null —— 界面不替人选 shell 配置；写完重读盘", async () => {
    const el = await mount();
    await open(el);
    const reads = seen.filter((c) => c.cmd === "aliases_read").length;
    clickText(el, "写入");
    await flush();
    const inst = seen.find((c) => c.cmd === "aliases_install")!.args as {
      aliases: Alias[];
      rcPath: string | null;
    };
    expect(inst.rcPath).toBeNull();
    expect(inst.aliases.map((a) => a.name)).toEqual(["alphacc", "alphacct"]);
    expect(seen.filter((c) => c.cmd === "aliases_read").length).toBe(reads + 1);
    expect(el.querySelector(".ccm-acct-alias-out")!.textContent).toContain("已写入");
  });

  it("别名块：默认那一档不扫；选了那份 rc 才扫、装的就是那一份、而且不抢 cc 函数名", async () => {
    const el = await mount();
    await open(el);
    expect(seen.some((c) => c.cmd === "cc_integration_scan_path")).toBe(false);
    expect(el.querySelector<HTMLElement>(".ccm-rc-block")!.hidden).toBe(true);
    const sel = el.querySelector<HTMLSelectElement>(".ccm-acct-alias-rc")!;
    sel.value = "/h/.bashrc";
    sel.dispatchEvent(new Event("change"));
    await flush();
    expect(el.querySelector<HTMLElement>(".ccm-rc-block")!.hidden).toBe(false);
    clickText(el, "装别名块");
    await flush();
    expect(seen.find((c) => c.cmd === "cc_integration_install")!.args).toEqual({
      path: "/h/.bashrc",
      commandName: "cc",
      includeCcFunction: false,
    });
  });

  it("Windows 本机 ⇒ 一句说明、没有别名表单；构造零 I/O，展开才建「用户级 PATH」那一格（`K-R135`）", async () => {
    const el = await mount("windows");
    expect(seen, "还没展开就发了 IPC").toEqual([]);
    await open(el);
    expect(el.querySelector(".machine-aliases-list"), "Windows 上不该有 POSIX 别名清单").toBeNull();
    expect(el.textContent).toContain("PowerShell 写法的别名还没做");
    // 那一格就是 Windows 上「让终端找到 ccm」的那条路；它的读数只问这一发。
    expect(seen.map((c) => c.cmd)).toEqual(["ccm_user_path_status"]);
    expect(el.querySelector(".ccm-user-path-block")).toBeTruthy();
    // 再展开不再建第二份。
    el.open = false;
    el.dispatchEvent(new Event("toggle"));
    await open(el);
    expect(el.querySelectorAll(".ccm-user-path-block").length).toBe(1);
  });
});
