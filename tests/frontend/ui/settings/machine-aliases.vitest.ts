// 机器页 ②「别名」。
//
// 这里钉的是**界面那一半**：清单从读回口开始 · 表单 ↔ 参数 · 两跳各在什么时候发 · 默认不动用户配置 ·
// 构造零 I/O。shell 文本长什么样、真 bash 执行下来对不对，归后端 `tests/backend/assets/aliases/aliases_tests.rs`
// —— 本文件一个字节的 shell 文本都不断言（前端也一个字节都不拼）。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { Alias, ExecPolicy } from "../../../../src/frontend/ui/alias-reads";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 8; i += 1) await Promise.resolve();
};

describe("表单 ↔ 一条别名（纯函数）", () => {
  it("每一格都往返得回来（两向：表单 → 参数 → 表单 逐格相等）", async () => {
    const m = await import("../../../../src/frontend/ui/settings/machine-aliases");
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
      // `<交给 claude 的…> -- <ccm 自己的…>`
      args: [
        "--model", "opus",
        "--verbose", "--x",
        "--",
        "--cwd", "/home/u/文档/c c",
        "--account", "z",
        "--ccm-tmux=w",
        "--ccm-agent", "codex",
        "--launcher", "/usr/bin/claude",
        "--tmux-size", "200x50",
        "--detach",
        "--bus-register",
        "--bus-note", "备注",
      ],
    });
    expect(m.aliasToForm(a)).toEqual(f);
  });

  it(" 在控件这一侧：不进 tmux ⇒ 容器那几格一个都不出现；不 --detach ⇒ 不登记 cc-bus", async () => {
    const m = await import("../../../../src/frontend/ui/settings/machine-aliases");
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
    expect(noDetach.args).toEqual(["--", "--ccm-tmux"]);
    expect(m.formToAlias({ ...m.emptyForm(), name: "c", account: m.BASE_CHOICE }).args).toEqual([
      "--",
      "--base",
    ]);
    // 只有交给 claude 的、里面又带它自己的 `--` ⇒ 末尾补一个空的 `--`。
    expect(m.formToAlias({ ...m.emptyForm(), name: "d", passthru: "-p -- -x" }).args).toEqual([
      "-p", "--", "-x", "--",
    ]);
  });

  it("认不出的参数不静默丢：原样进透传栏", async () => {
    const m = await import("../../../../src/frontend/ui/settings/machine-aliases");
    expect(m.aliasToForm({ name: "x", args: ["--weird", "--", "--ccm-tmux"] }).passthru).toBe("--weird");
    expect(m.aliasToForm({ name: "x", args: ["--", "--ccm-tmux", "--odd"] }).passthru).toBe("--odd");
  });

  it("〔V153〕-- 右边第一个 new（起新会话 = 缺省）表单读得回、不落进透传；别处的 new 照旧不丢", async () => {
    const m = await import("../../../../src/frontend/ui/settings/machine-aliases");
    const f = m.aliasToForm({ name: "x", args: ["--", "new", "--ccm-tmux"] });
    expect(f.passthru).toBe("");
    expect(f.tmux).toBe("auto");
    expect(m.aliasToForm({ name: "x", args: ["--", "--ccm-tmux", "new"] }).passthru).toBe("new");
  });
});

// 两套合一：从前 POSIX 一套、Windows 那一块（`cc_integration.ts`）
// 零单测、在面板测试里全被替身掉。今天是**一份组件、平台是入参** ⇒ 同一套断言对两个平台各跑一遍，
// 平台那几格（tmux 能力 · 别名块是哪一种 · 首开发哪几发）各有各的期望。
type Plat = "posix" | "powershell";

describe.each<Plat>(["posix", "powershell"])("buildAliasManager（%s）：两跳 ＋ 读回口 ＋ 默认不动用户配置", (plat) => {
  let seen: Array<{ cmd: string; args?: unknown }>;
  /** 预览那一发要不要失败。 */
  let previewFails = false;
  let disk: Alias[];
  let problems: Array<{ name: string; message: string }>;
  /** 替身盘面：哪几份候选里装着别名块（装 / 卸会改它，读回口照它答）。 */
  let blockAt: Set<string>;
  /** 装着旧版别名块的那几份（装一次 = 换成新版 ⇒ 从这里摘掉）。 */
  let oldAt: Set<string>;
  /** 非 null ⇒ 读「自动打开 monitor」那项设置失败，拒绝原因就是它。 */
  let autoLaunchFail: string | null;
  /** `/h/rc-a` 那份候选带的执行策略（后端现问的那一份）；点了「允许」之后换成 `policyAfter`。 */
  let policyA: ExecPolicy | null;
  let policyAfter: ExecPolicy | null;

  /** 一份候选（别名文件那一行与别名块共用）；`block` 是后端那一次扫描带回来的别名块现状。 */
  const cand = (
    path: string,
    over: { exists?: boolean; present?: boolean; hint?: string; outdated?: boolean; policy?: ExecPolicy | null } = {},
  ) => ({
    path,
    sourced: false,
    policy: over.policy ?? null,
    exists: over.exists ?? true,
    block: {
      present: over.present ?? false,
      version: over.outdated ? "v2" : null,
      outdated: over.outdated ?? false,
      conflictingFunctions: [],
      manualCleanupHint: over.hint ?? "",
    },
  });

  beforeEach(() => {
    seen = [];
    disk = [
      { name: "alphacc", args: ["--account", "z"] },
      { name: "betacc", args: ["--account", "b"] },
    ];
    problems = [];
    blockAt = new Set();
    oldAt = new Set();
    autoLaunchFail = null;
    policyA = null;
    policyAfter = null;
    vi.resetModules();
    vi.doMock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));
    // 预览走通道（`chan.call(origin, "ccm-print", …)`）：替身把每一发记进同一本账（`chan:<op>`），
    //   首开那几发的集合相等判据因此也看得见「展开时有没有偷问预览」。
    previewFails = false;
    vi.doMock("../../../../src/comms/inward/chan", () => ({
      // `chan-caller.ts::saidOf` 按 `instanceof ChanError` 分流 —— 替身也得交出这个类（替身抛的是普通 Error）。
      ChanError: class ChanError extends Error {},
      chan: {
        call: (origin: string, op: string, body: Uint8Array) => {
          const args = JSON.parse(new TextDecoder().decode(body)) as { args: string[] };
          seen.push({ cmd: `chan:${op}`, args: { origin, ...args } });
          if (previewFails) return Promise.reject(new Error("后端没接住"));
          return Promise.resolve(
            new TextEncoder().encode(JSON.stringify({ line: `LINE ${args.args.join(" ")}` })),
          );
        },
      },
    }));
    // 别名六问走通道（`src/frontend/ui/alias-reads.ts`）：替身按旧命令名记账（`aliases_*`），判据的集合相等照旧成立。
    vi.doMock("../../../../src/frontend/ui/alias-reads", () => ({
        readAliases: (origin: string, shell: Plat, rcPath: string | null) => {
          const a = { origin, shell, rcPath };
          seen.push({ cmd: "aliases_read", args: a });
          if (a.rcPath === "/etc/x") return Promise.reject("refuse profile path: 只能落在 home 之内");
          const other = a.rcPath ? `/h/${a.rcPath.replace(/^~\//, "")}` : null;
          return Promise.resolve({
            aliasPath: "/h/.cc-monitor/aliases.x",
            exists: true,
            aliases: disk.map((x) => ({ ...x, args: [...x.args] })),
            unparsed: ["alias x=ls（不是 `名字() { … }` 的形状）"],
            rcCandidates: [
              cand("/h/rc-a", {
                present: blockAt.has("/h/rc-a"),
                hint: "第 3 行 ccm",
                outdated: oldAt.has("/h/rc-a"),
                policy: policyA,
              }),
              cand("/h/rc-b", { exists: false, present: blockAt.has("/h/rc-b") }),
              ...(other ? [cand(other, { present: blockAt.has(other) })] : []),
            ],
            otherRc: other,
          });
        },
        renderAliases: (origin: string, aliases: Alias[], shell: Plat) => {
          const a = { origin, aliases, shell };
          seen.push({ cmd: "aliases_render", args: a });
          return Promise.resolve({
            fileText: a.aliases.map((x) => `#${x.name}`).join("\n"),
            lines: [],
            problems,
            collisions: [],
          });
        },
        installAliases: (origin: string, aliases: Alias[], rcPath: string | null, shell: Plat) => {
          const a = { origin, aliases, rcPath, shell };
          seen.push({ cmd: "aliases_install", args: a });
          disk = a.aliases;
          return Promise.resolve({
            aliasPath: "/h/.cc-monitor/aliases.x",
            wroteAliasFile: true,
            notes: ["新开一个终端就能用"],
          });
        },
        installAliasBlock: (origin: string, rcPath: string, withCc: boolean) => {
          const a = { origin, rcPath, withCc };
          seen.push({ cmd: "aliases_block_install", args: a });
          blockAt.add(a.rcPath);
          oldAt.delete(a.rcPath);
          return Promise.resolve();
        },
        removeAliasBlock: (origin: string, rcPath: string) => {
          const a = { origin, rcPath };
          seen.push({ cmd: "aliases_block_remove", args: a });
          blockAt.delete(a.rcPath);
          return Promise.resolve();
        },
        allowLocalScripts: (origin: string, host: string) => {
          seen.push({ cmd: "powershell_policy_set", args: { origin, host } });
          policyA = policyAfter;
          return Promise.resolve({ policy: policyAfter, setError: null });
        },
        renderAliasBlock: (origin: string, rcPath: string, withCc: boolean) => {
          const a = { origin, rcPath, withCc };
          seen.push({ cmd: "aliases_block_render", args: a });
          return Promise.resolve(`# 块 → ${a.rcPath}`);
        },
    }));
    vi.doMock("../../../../src/frontend/ui/ipc/commands", () => ({
      commands: {
        local_ccm_entry_status: () => {
          seen.push({ cmd: "local_ccm_entry_status" });
          return Promise.resolve({ message: "" });
        },
        bound_terminal_count: () => {
          seen.push({ cmd: "bound_terminal_count" });
          return Promise.resolve(2);
        },
        cc_get_auto_launch: () => {
          seen.push({ cmd: "cc_get_auto_launch" });
          if (autoLaunchFail !== null) return Promise.reject(autoLaunchFail);
          return Promise.resolve({ auto_launch_enabled: false, monitor_exe_path: null });
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

  afterEach(() => {
    document.body.replaceChildren();
  });

  async function mount(confirm?: (msg: string) => boolean): Promise<HTMLDetailsElement> {
    const m = await import("../../../../src/frontend/ui/settings/machine-aliases");
    const el = m.buildAliasManager({
      platform: plat,
      origin: () => "<local>",
      confirm,
    }) as HTMLDetailsElement;
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

  /**
   * 首开那几发：两个平台共有的读回 ＋ 渲染，再加各自那一格（POSIX：本机 ccm；PowerShell：自动打开 monitor ＋ 用户级 PATH）。
   * PowerShell 那一侧**不再有**「终端集成」的状态 / 扫一份两发：别名块的现状随读回口的候选一起到。
   */
  const FIRST_OPEN: Record<Plat, string[]> = {
    posix: ["aliases_read", "aliases_render", "local_ccm_entry_status"],
    // 握手终端数从读回口的成品里拆出来、另问 monitor（`bound_terminal_count`）⇒ PowerShell 那一侧首开多这一发。
    // 本机 ccm 那一格 Windows 上也问（新开的 PowerShell 里敲 `ccm` 走到哪）。
    powershell: ["aliases_read", "aliases_render", "bound_terminal_count", "cc_get_auto_launch", "ccm_user_path_status", "local_ccm_entry_status"],
  };

  it("★ 构造零 I/O；第一次展开**恰好**那几发，再展开一发都不多", async () => {
    const el = await mount();
    expect(seen, "还没展开就发了 IPC").toEqual([]);
    await open(el);
    expect(seen.map((c) => c.cmd).sort()).toEqual([...FIRST_OPEN[plat]].sort());
    const n = seen.length;
    el.open = false;
    el.dispatchEvent(new Event("toggle"));
    await open(el);
    expect(seen.length, "再展开又读了一遍").toBe(n);
  });

  it("〔W5-ALIAS · C〕点「预览」恰好问一次本机后端 `ccm-print`，交的就是这一条的参数，答的那一行原样上屏", async () => {
    const el = await mount();
    await open(el);
    const before = seen.length;
    const rows = [...el.querySelectorAll<HTMLElement>(".machine-aliases-row")];
    const zRow = rows.find((r) => r.querySelector("code")?.textContent === "alphacc")!;
    clickText(zRow, "预览");
    await flush();
    const asked = seen.slice(before);
    expect(asked).toEqual([
      { cmd: "chan:ccm-print", args: { origin: "<local>", args: ["--account", "z"] } },
    ]);
    const out = zRow.nextElementSibling as HTMLElement;
    expect(out.dataset.role).toBe("alias-preview");
    expect(out.textContent).toContain("LINE --account z");
    expect(out.textContent).toContain("alphacc");
  });

  it("〔W5-ALIAS · C〕预览没问成 ⇒ 那一条下面照实说「预览不了」＋ 原因，不静默", async () => {
    const el = await mount();
    await open(el);
    previewFails = true;
    const row = el.querySelector<HTMLElement>(".machine-aliases-row")!;
    clickText(row, "预览");
    await flush();
    const out = row.nextElementSibling as HTMLElement;
    expect(out.textContent).toContain("预览不了");
    expect(out.textContent).toContain("后端没接住");
  });

  it("🔴 别名三条命令每一发都带着这个平台的 `shell`（平台是入参，不是组件自己猜的）", async () => {
    const el = await mount();
    await open(el);
    clickText(el, "写入");
    await flush();
    const aliasCalls = seen.filter((c) => c.cmd.startsWith("aliases_"));
    expect(aliasCalls.map((c) => c.cmd).sort()).toEqual(
      ["aliases_install", "aliases_read", "aliases_read", "aliases_render", "aliases_render"].sort(),
    );
    for (const c of aliasCalls) expect((c.args as { shell: Plat }).shell, c.cmd).toBe(plat);
  });

  it("清单从盘上那份开始；认不出的那一行原样说出来（写入时它会被去掉）", async () => {
    const el = await mount();
    await open(el);
    const rows = [...el.querySelectorAll(".machine-aliases-row code")].map((c) => c.textContent);
    expect(rows).toEqual(["alphacc", "betacc"]);
    expect(el.querySelector(".machine-aliases-status")!.textContent).toContain("alias x=ls");
    expect(lastRendered()).toEqual(["alphacc", "betacc"]);
  });

  it("agent 那一栏只列后端注册表里的几家；改一条写了不认识的 agent 的别名 ⇒ 照原样摆出来（不可选），不悄悄变成「不指定」", async () => {
    const { listAgents } = await import("../../../../src/frontend/ui/agent-profile");
    disk = [{ name: "gcc", args: ["--", "--ccm-agent", "gemini"] }];
    const el = await mount();
    await open(el);
    const agentSel = [...el.querySelectorAll<HTMLSelectElement>("select")].find((s) =>
      [...s.options].some((o) => o.value === listAgents()[0]),
    )!;
    expect([...agentSel.options].map((o) => o.value)).toEqual(["", ...listAgents()]);
    const row = el.querySelector<HTMLElement>(".machine-aliases-row")!;
    clickText(row, "改");
    await flush();
    expect(agentSel.value, "不认识的 agent 被换成了「不指定」（= 默认那一家）").toBe("gemini");
    const unknown = [...agentSel.options].filter((o) => o.dataset.role === "agent-unknown");
    expect(unknown.map((o) => [o.value, o.disabled, o.textContent])).toEqual([["gemini", true, "不认识的 agent：gemini"]]);
    // 换一条认得的 ⇒ 那一项收掉，名单回到注册表那几家。
    clickText(el, "清空表单");
    await flush();
    expect([...agentSel.options].map((o) => o.value)).toEqual(["", ...listAgents()]);
  });

  it("表单「加进清单」⇒ 问后端要一次代码（第①跳），清单里多一条", async () => {
    const el = await mount();
    await open(el);
    const name = el.querySelector<HTMLInputElement>('input[title="在终端里敲的那个词"]')!;
    name.value = "mine";
    clickText(el, "加进清单");
    await flush();
    expect(lastRendered()).toEqual(["alphacc", "betacc", "mine"]);
  });

  it("「删」⇒ 那一条从清单里走了，并重新渲染", async () => {
    const el = await mount();
    await open(el);
    const first = el.querySelector<HTMLElement>(".machine-aliases-row")!;
    clickText(first, "删");
    await flush();
    expect(lastRendered()).toEqual(["betacc"]);
  });

  it("没有「为每个账号加一条」那颗按钮：每个号那一条由那台后端随账号表自动写", async () => {
    const el = await mount();
    await open(el);
    expect([...el.querySelectorAll("button")].map((b) => b.textContent)).not.toContain("为每个账号加一条");
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

  it("🔴 「写入」（第②跳）带的是当前清单，而启动文件默认是 null —— 界面不替人选；写完重读盘", async () => {
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
    expect(inst.aliases.map((a) => a.name)).toEqual(["alphacc", "betacc"]);
    expect(seen.filter((c) => c.cmd === "aliases_read").length).toBe(reads + 1);
    expect(el.querySelector(".ccm-acct-alias-out")!.textContent).toContain("已写入");
  });

  it("启动文件下拉：默认「不动」；还不在盘上的那一份说清「写入时新建」", async () => {
    const el = await mount();
    await open(el);
    const sel = el.querySelector<HTMLSelectElement>(".ccm-acct-alias-rc")!;
    expect(sel.value).toBe("");
    const labels = [...sel.options].map((o) => o.textContent);
    expect(labels).toContain("/h/rc-a");
    expect(labels).toContain("/h/rc-b（还不存在，写入时新建）");
  });

  it("tmux 那几格：POSIX 可选；PowerShell（Windows 没有 tmux）整组不给选", async () => {
    const el = await mount();
    const tmux = [...el.querySelectorAll<HTMLSelectElement>("select")].find((s) =>
      [...s.options].some((o) => o.value === "auto"),
    )!;
    expect(tmux.disabled).toBe(plat === "powershell");
    expect(tmux.value).toBe("none");
  });

  // ── 别名块：两种 shell 同一块、同一个选择器、同一族命令（`aliases_block_*`）──────────
  const pick = async (el: HTMLElement, path: string): Promise<void> => {
    const sel = el.querySelector<HTMLSelectElement>(".ccm-acct-alias-rc")!;
    sel.value = path;
    sel.dispatchEvent(new Event("change"));
    await flush();
  };
  const rcBlock = (el: HTMLElement): HTMLElement => el.querySelector<HTMLElement>(".ccm-rc-block")!;

  it("★ 这台机器上启动文件的选择器**恰好一个**（别名文件那一行与别名块共用）；没有版本预设那一族", async () => {
    const el = await mount();
    await open(el);
    const pickers = [...el.querySelectorAll<HTMLSelectElement>("select")].filter((s) =>
      [...s.options].some((o) => o.textContent === "不动我的 shell 配置"),
    );
    expect(pickers.length).toBe(1);
    // 从前 PowerShell 那一块自己推 `$PROFILE`（CurrentHost / AllHosts 预设 ＋ TS 换文件名）—— 今天候选只来自读回口。
    const values = [...el.querySelectorAll("option")].map((o) => (o as HTMLOptionElement).value);
    expect(values.filter((v) => /^Ps(51|7)-/.test(v))).toEqual([]);
    expect(values.filter((v) => v.startsWith("/h/"))).toEqual(["/h/rc-a", "/h/rc-b"]);
  });

  it("别名块：默认那一档整块藏着；选了一份**不发 IPC**（现状随候选到）；装 / 卸带的就是选中那份，装完重读、清单不动", async () => {
    const el = await mount();
    await open(el);
    expect(rcBlock(el).hidden).toBe(true);
    // 表单里先加一条还没写入的 —— 装 / 卸别名块不许把它冲掉。
    el.querySelector<HTMLInputElement>('input[title="在终端里敲的那个词"]')!.value = "mine";
    clickText(el, "加进清单");
    await flush();
    const n = seen.length;
    await pick(el, "/h/rc-a");
    expect(seen.length, "选一份就发了 IPC —— 现状该随读回口的候选一起到").toBe(n);
    expect(rcBlock(el).hidden).toBe(false);
    expect(el.querySelector(".ccm-rc-block-status")!.textContent).toContain("还没有别名块");
    expect(el.querySelector<HTMLElement>(".ccm-rc-block-legacy")!.hidden).toBe(false);
    clickText(el, "装别名块");
    await flush();
    expect(seen.find((c) => c.cmd === "aliases_block_install")!.args).toEqual({
      origin: "<local>",
      rcPath: "/h/rc-a",
      withCc: false,
    });
    expect(seen.slice(n).filter((c) => c.cmd === "aliases_read").length, "装完没重读").toBe(1);
    expect(el.querySelector(".ccm-rc-block-status")!.textContent).toContain("已经装了");
    expect([...el.querySelectorAll(".machine-aliases-row code")].map((c) => c.textContent)).toEqual([
      "alphacc",
      "betacc",
      "mine",
    ]);
    clickText(el, "卸载 ccm"); // 用户 09-26：本机那颗与远端同叫「卸载 ccm」
    await flush();
    expect(seen.find((c) => c.cmd === "aliases_block_remove")!.args).toEqual({
      origin: "<local>",
      rcPath: "/h/rc-a",
    });
    expect(el.querySelector(".ccm-rc-block-status")!.textContent).toContain("还没有别名块");
  });

  it("〔TL1〕旧版别名块：说清是旧版、点「重装别名块」换新；装过之后那句话没了", async () => {
    blockAt.add("/h/rc-a");
    oldAt.add("/h/rc-a");
    const el = await mount();
    await open(el);
    await pick(el, "/h/rc-a");
    const status = () => el.querySelector(".ccm-rc-block-status")!.textContent!;
    expect(status()).toContain("已经装了");
    expect(status()).toContain("旧版");
    clickText(el, "重装别名块");
    await flush();
    expect(status()).toContain("已经装了");
    expect(status(), "重装之后还说是旧版").not.toContain("旧版");
  });

  it("预览别名块：交给后端的是选中那份文件（方言由后端按扩展名定，前端不判）", async () => {
    const el = await mount();
    await open(el);
    await pick(el, "/h/rc-b");
    expect(el.querySelector(".ccm-rc-block-status")!.textContent).toContain("还不存在");
    clickText(el, "预览别名块");
    await flush();
    expect(seen.find((c) => c.cmd === "aliases_block_render")!.args).toEqual({
      origin: "<local>",
      rcPath: "/h/rc-b",
      withCc: false,
    });
    expect(document.querySelector(".settings-cc-modal-code")!.textContent).toBe("# 块 → /h/rc-b");
  });

  it("块还装在别的候选里 ⇒ 照实说出来（从前只查 PowerShell 的 `profile.ps1` 两份，今天每份候选都带块的现状）", async () => {
    blockAt.add("/h/rc-b");
    const el = await mount();
    await open(el);
    await pick(el, "/h/rc-a");
    const elsewhere = el.querySelector<HTMLElement>(".settings-cc-legacy-warn")!;
    expect(elsewhere.hidden).toBe(false);
    expect(elsewhere.textContent).toContain("/h/rc-b");
    const labels = [...el.querySelectorAll<HTMLOptionElement>(".ccm-acct-alias-rc option")].map((o) => o.textContent);
    expect(labels).toContain("/h/rc-b（已装别名块；还不存在，写入时新建）");
  });

  it("其它文件：交给读回口过围栏、并进候选并选中；过不了围栏 ⇒ 原话上屏、候选不动", async () => {
    const el = await mount();
    await open(el);
    const other = el.querySelector<HTMLInputElement>(".ccm-rc-other")!;
    other.value = "/etc/x";
    clickText(el, "用这份");
    await flush();
    expect(el.textContent).toContain("refuse profile path");
    expect(el.querySelector<HTMLSelectElement>(".ccm-acct-alias-rc")!.value).toBe("");
    other.value = "~/.config/x.rc";
    clickText(el, "用这份");
    await flush();
    const sel = el.querySelector<HTMLSelectElement>(".ccm-acct-alias-rc")!;
    expect(sel.value).toBe("/h/.config/x.rc");
    expect(rcBlock(el).hidden).toBe(false);
    // 之后每次读回都带着它（写入后重读、装完重读 …）。
    clickText(el, "装别名块");
    await flush();
    const last = [...seen].reverse().find((c) => c.cmd === "aliases_read")!.args as { rcPath: string };
    expect(last.rcPath).toBe("~/.config/x.rc");
  });

  if (plat === "powershell") {
    // 读不到时别把「不知道」画成「没勾」。
    it("读「自动打开 monitor」失败 ⇒ 复选框禁用、路径那格说读不到（原因原样）；读到 ⇒ 可点（正控）", async () => {
      autoLaunchFail = "boom-autolaunch";
      let el = await mount();
      await open(el);
      await flush();
      const box = (): HTMLInputElement =>
        el.querySelector<HTMLInputElement>(".settings-cc-autolaunch input[type=checkbox]")!;
      const path = (): string => el.querySelector(".settings-cc-autolaunch-path-value")!.textContent ?? "";
      expect(box().disabled, "读失败还让人勾 —— 画成了「关着」").toBe(true);
      expect(path()).toContain("读不到这项设置");
      expect(path()).toContain("boom-autolaunch");
      autoLaunchFail = null;
      el = await mount();
      await open(el);
      await flush();
      expect(box().disabled).toBe(false);
      expect(path()).not.toContain("读不到这项设置");
    });
  }

  if (plat === "posix") {
    it("POSIX：没有「同时装 cc 函数」那一问（`cc` 自带 `declare -f` 让着你）；没有用户级 PATH 那一格", async () => {
      const el = await mount();
      await open(el);
      await pick(el, "/h/rc-a");
      expect(el.querySelector<HTMLElement>(".ccm-rc-withcc")!.hidden).toBe(true);
      expect(el.querySelector(".ccm-user-path-block"), "POSIX 上不该有用户级 PATH 那一格").toBeNull();
      expect(el.textContent).not.toContain("PowerShell 集成");
    });
  } else {
    it("PowerShell：握手数 ＋ 自动打开 monitor ＋ 用户级 PATH 展开才建、只建一份；「同时装 cc 函数」勾了就带着装", async () => {
      const el = await mount();
      expect(el.querySelector(".ccm-user-path-block"), "还没展开就建了").toBeNull();
      await open(el);
      expect(el.textContent).toContain("PowerShell 集成");
      expect(el.querySelector(".settings-cc-stat-value")!.textContent, "握手数该来自读回口").toBe("2");
      expect(el.querySelector(".ccm-user-path-block")).toBeTruthy();
      el.open = false;
      el.dispatchEvent(new Event("toggle"));
      await open(el);
      expect(el.querySelectorAll(".ccm-user-path-block").length).toBe(1);
      expect(el.querySelectorAll(".settings-cc-autolaunch").length).toBe(1);
      await pick(el, "/h/rc-a");
      const withCc = el.querySelector<HTMLElement>(".ccm-rc-withcc")!;
      expect(withCc.hidden).toBe(false);
      expect(withCc.textContent).toContain("同时装 cc 函数");
      const box = withCc.querySelector<HTMLInputElement>("input")!;
      box.checked = true;
      box.dispatchEvent(new Event("change"));
      clickText(el, "装别名块");
      await flush();
      expect(seen.find((c) => c.cmd === "aliases_block_install")!.args).toEqual({
        origin: "<local>",
        rcPath: "/h/rc-a",
        withCc: true,
      });
    });

    // 执行策略：块不会被加载 ⇒ 不说「装好了」、说原因、给标准做法的按钮；点了先确认才发；组策略钉着 ⇒ 只照说。
    const pol = (effective: string, loads: boolean | null, groupPolicy = false): ExecPolicy => ({
      host: "powershell",
      effective,
      loads,
      groupPolicy,
      error: null,
    });
    const policyLine = (el: HTMLElement): string =>
      [...el.querySelectorAll<HTMLElement>(".ccm-rc-block .settings-hint")].map((x) => x.textContent).join("|");
    const allowBtn = (el: HTMLElement): HTMLButtonElement | undefined =>
      [...el.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "允许运行本机脚本");

    it("〔WF1 · L〕执行策略挡住块：装了也说「不会加载」、装后不说「装好了」；按钮先确认，拒了一发都不发，确认了才发、按那一代发", async () => {
      policyA = pol("Restricted", false);
      policyAfter = pol("RemoteSigned", true);
      const asked: string[] = [];
      let answer = false;
      const el = await mount((msg) => {
        asked.push(msg);
        return answer;
      });
      await open(el);
      await pick(el, "/h/rc-a");
      expect(policyLine(el)).toContain("Restricted");
      expect(allowBtn(el)!.hidden).toBe(false);
      clickText(el, "装别名块");
      await flush();
      expect(el.querySelector(".ccm-rc-block-status")!.textContent).toContain("不会加载它");
      expect(policyLine(el)).not.toContain("装好了");
      expect(policyLine(el)).toContain("要看这台 PowerShell 的执行策略");
      allowBtn(el)!.click();
      await flush();
      expect(asked.length).toBe(1);
      expect(asked[0]).toContain("RemoteSigned");
      expect(seen.filter((c) => c.cmd === "powershell_policy_set"), "没确认就改了").toEqual([]);
      answer = true;
      allowBtn(el)!.click();
      await flush();
      expect(seen.filter((c) => c.cmd === "powershell_policy_set").map((c) => c.args)).toEqual([
        { origin: "<local>", host: "powershell" },
      ]);
      expect(policyLine(el)).toContain("改好了");
      expect(el.querySelector(".ccm-rc-block-status")!.textContent).toContain("已经装了");
      expect(allowBtn(el)!.hidden).toBe(true);
    });

    it("〔WF1 · L〕组策略钉着 ⇒ 照说、不给按钮；会加载 ⇒ 执行策略那一行不出声、装后照旧说「装好了」", async () => {
      policyA = pol("AllSigned", false, true);
      const el = await mount(() => true);
      await open(el);
      await pick(el, "/h/rc-a");
      expect(policyLine(el)).toContain("组策略");
      expect(allowBtn(el)!.hidden).toBe(true);
      policyA = pol("RemoteSigned", true);
      clickText(el, "装别名块");
      await flush();
      expect(policyLine(el)).not.toContain("执行策略");
      expect(policyLine(el)).toContain("装好了");
      expect(allowBtn(el)!.hidden).toBe(true);
    });
  }

  /**
   * **B5**：远端卡与本机是同一个组件（「`origin` 是本机还是远端，对这些命令没有区别」）——
   * 每一发 `aliases_*` 与别名预览都带那台的 origin；首开集合相等（本机才答得了的那几发一发都没有）；
   * 平台格与「打开这份文件」不挂、块预览挂；卸那颗叫「卸载 ccm」。
   */
  // 远端恒 POSIX（表 B）：不管这一轮 `plat` 是哪种，远端卡都按 `posix` 建（两轮各跑一遍，结论相同）。
  it("〔AL2〕B5：远端卡是同一个组件，每一发都带那台的 origin，只本机的那几格不挂", async () => {
    const m = await import("../../../../src/frontend/ui/settings/machine-aliases");
    const done: string[] = [];
    const el = m.buildAliasManager({
      platform: "posix",
      origin: () => "devbox",
      onBlockDone: (verb, err) => done.push(`${verb}:${err ?? "ok"}`),
    }) as HTMLDetailsElement;
    document.body.appendChild(el);
    await flush();
    expect(seen, "构造零 I/O").toEqual([]);
    expect(el.dataset.origin).toBe("devbox");
    await open(el);
    expect(seen.map((c) => c.cmd).sort()).toEqual(["aliases_read", "aliases_render"]);
    clickText(el, "写入");
    await flush();
    el.querySelector<HTMLInputElement>(".ccm-rc-other")!.value = "~/.zshrc";
    clickText(el, "用这份");
    await flush();
    await pick(el, "/h/rc-a");
    clickText(el, "预览别名块");
    await flush();
    clickText(el, "装别名块");
    await flush();
    clickText(el, "卸载 ccm");
    await flush();
    clickText(el.querySelector<HTMLElement>(".machine-aliases-row")!, "预览");
    await flush();
    const sent = seen.filter((c) => c.cmd.startsWith("aliases_") || c.cmd === "chan:ccm-print");
    expect(new Set(sent.map((c) => c.cmd))).toEqual(
      new Set([
        "aliases_read",
        "aliases_render",
        "aliases_install",
        "aliases_block_render",
        "aliases_block_install",
        "aliases_block_remove",
        "chan:ccm-print",
      ]),
    );
    expect(
      sent.filter((c) => (c.args as { origin?: string }).origin !== "devbox"),
      "有一发没带那台的 origin",
    ).toEqual([]);
    expect(seen.filter((c) => !sent.includes(c)), "远端卡问了只有本机才答得了的事").toEqual([]);
    expect(done).toEqual(["install:ok", "remove:ok"]);
    expect(el.querySelector(".ccm-user-path-block")).toBeNull();
    expect([...el.querySelectorAll("button")].map((b) => b.textContent)).not.toContain("打开这份文件");
    // 撞名由那台后端查它自己的 `PATH` ⇒ 「远端 PATH 没查」那句说明退役。
    expect(el.textContent).not.toContain("PATH 上的同名程序没查");
  });
});

describe("localShell：本机用哪种方言", () => {
  afterEach(async () => {
    const { __setHostOsForTests } = await import("../../../../src/frontend/ui/settings/host-os");
    __setHostOsForTests(null);
  });
  /** （主会话 09-28 裁 FIX4）「认不出 OS 时：不猜方言 —— 明说、安装入口置灰」。 */
  it("Windows ⇒ powershell；Linux / macOS ⇒ posix；认不出 ⇒ 不猜（null），那一格明说、装的入口置灰", async () => {
    vi.resetModules();
    const { __setHostOsForTests } = await import("../../../../src/frontend/ui/settings/host-os");
    const { localShell, buildUnknownOsAliasBlock } = await import("../../../../src/frontend/ui/settings/machine-aliases");
    const want = { windows: "powershell", linux: "posix", macos: "posix", unknown: null } as const;
    for (const [os, sh] of Object.entries(want)) {
      __setHostOsForTests(os as "windows" | "linux" | "macos" | "unknown");
      expect(localShell(), os).toBe(sh);
    }
    const block = buildUnknownOsAliasBlock();
    expect(block.textContent).toContain("认不出这台的系统，没法生成别名块");
    const btns = [...block.querySelectorAll("button")];
    expect(btns.map((b) => [b.textContent, b.disabled])).toEqual([["装别名块", true]]);
  });
});
