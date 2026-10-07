// 机器页 ②「别名」：接入那一格（先显示现状）＋ 清单那一块挂在下面（清单自己的判据在 `profiles-list.vitest.ts`）。
//
// 这里钉的是接入那一半与整块的读法：接入在最上 · 同名函数交给清单那一块画三个选择 · 构造零 I/O · 重读 · 远端同一个组件。
// shell 文本长什么样归后端 `tests/backend/assets/aliases/`，本文件一个字节的 shell 文本都不断言。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { ExecPolicy, NameClash } from "../../../../src/frontend/ui/alias-reads";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 12; i += 1) await Promise.resolve();
};

// 一份组件、平台是入参 ⇒ 同一套断言对两个平台各跑一遍，平台那几格（tmux 能力 · 首开发哪几发 · PowerShell 那几格）各有各的期望。
type Plat = "posix" | "powershell";

describe.each<Plat>(["posix", "powershell"])("buildAliasManager（%s）", (plat) => {
  let seen: Array<{ cmd: string; args?: unknown }>;
  const toasted: string[] = [];
  let blockAt: Set<string>;
  let oldAt: Set<string>;
  let clashes: NameClash[];
  let autoLaunchFail: string | null;
  let policyA: ExecPolicy | null;
  let policyAfter: ExecPolicy | null;

  const cand = (path: string, over: { exists?: boolean; policy?: ExecPolicy | null } = {}) => ({
    path,
    sourced: blockAt.has(path),
    policy: over.policy ?? null,
    exists: over.exists ?? true,
    unreadable: null,
    block: {
      present: blockAt.has(path),
      version: oldAt.has(path) ? "v2" : null,
      outdated: oldAt.has(path),
      conflictingFunctions: path === "/h/rc-a" ? clashes : [],
      manualCleanupHint: "",
    },
  });

  beforeEach(() => {
    seen = [];
    blockAt = new Set();
    oldAt = new Set();
    clashes = [];
    autoLaunchFail = null;
    policyA = null;
    policyAfter = null;
    vi.resetModules();
    toasted.length = 0;
    vi.doMock("../../../../src/frontend/ui/kit/toast", () => ({ toast: (title: string, body: string) => toasted.push(`${title}|${body}`) }));
    vi.doMock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));
    vi.doMock("../../../../src/comms/inward/chan", () => ({
      ChanError: class ChanError extends Error {},
      chan: {
        subscribe: () => Promise.resolve({ want: () => undefined, stop: () => undefined }),
        call: (origin: string, op: string, body: Uint8Array) => {
          const args = JSON.parse(new TextDecoder().decode(body)) as { args?: string[] };
          seen.push({ cmd: `chan:${op}`, args: { origin, ...args } });
          return Promise.resolve(new TextEncoder().encode(JSON.stringify({ line: `LINE ${(args.args ?? []).join(" ")}` })));
        },
      },
    }));
    vi.doMock("../../../../src/frontend/ui/profiles-reads", async (orig) => {
      const real = await orig<typeof import("../../../../src/frontend/ui/profiles-reads")>();
      return {
        ...real,
        readProfiles: (origin: string) => {
          seen.push({ cmd: "profiles_read", args: { origin } });
          return Promise.resolve({
            home: "/h",
            path: "/h/.cc-monitor/profiles.toml",
            exists: true,
            fingerprint: "fp",
            modified: 1,
            fileProblem: null,
            profiles: [],
            seed: [],
            migrated: null,
            binDir: "/h/.cc-monitor/bin",
            accounts: ["z", "b"],
          });
        },
      };
    });
    vi.doMock("../../../../src/frontend/ui/alias-reads", () => {
      return {
        readAliases: (origin: string, shell: Plat, rcPath: string | null) => {
          seen.push({ cmd: "aliases_read", args: { origin, shell, rcPath } });
          if (rcPath === "/etc/x") return Promise.reject(new Error("拒绝写这个配置文件：只能落在 home 之内"));
          const other = rcPath ? `/h/${rcPath.replace(/^~\//, "")}` : null;
          return Promise.resolve({
            home: "/h",
            rcCandidates: [cand("/h/rc-a", { policy: policyA }), cand("/h/rc-b", { exists: false }), ...(other ? [cand(other)] : [])],
            otherRc: other,
          });
        },
        installAliasBlock: (origin: string, rcPath: string) => {
          seen.push({ cmd: "aliases_block_install", args: { origin, rcPath } });
          blockAt.add(rcPath);
          oldAt.delete(rcPath);
          return Promise.resolve();
        },
        removeAliasBlock: (origin: string, rcPath: string) => {
          seen.push({ cmd: "aliases_block_remove", args: { origin, rcPath } });
          blockAt.delete(rcPath);
          return Promise.resolve();
        },
        allowLocalScripts: (origin: string, host: string) => {
          seen.push({ cmd: "powershell_policy_set", args: { origin, host } });
          policyA = policyAfter;
          return Promise.resolve({ policy: policyAfter, setError: null });
        },
        renderAliasBlock: (origin: string, rcPath: string) => {
          seen.push({ cmd: "aliases_block_render", args: { origin, rcPath } });
          return Promise.resolve(`# 接入那几行 → ${rcPath}`);
        },
      };
    });
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

  /** 建好的那几组（`open` 按元素找回它的 `load`：这一栏露出来时宿主就是这样叫的）。 */
  const built = new Map<HTMLElement, { load(): void }>();

  async function mount(confirm?: (spec: { body?: string }) => boolean): Promise<HTMLElement> {
    const m = await import("../../../../src/frontend/ui/settings/machine-aliases");
    const mgr = m.buildAliasManager({ platform: plat, origin: () => "<local>", confirm });
    built.set(mgr.element, mgr);
    document.body.appendChild(mgr.element);
    await flush();
    return mgr.element;
  }

  /** 这一栏露出来（宿主叫 `load`）。 */
  async function open(el: HTMLElement): Promise<void> {
    built.get(el)!.load();
    await flush();
  }

  const clickText = (root: Element, text: string): void => {
    const b = [...root.querySelectorAll<HTMLButtonElement>("button")].find((x) => x.textContent === text);
    if (!b) throw new Error(`找不到按钮「${text}」`);
    b.click();
  };
  /** 重读：别名那一份 ＋ 本机 ccm 那一格；PowerShell 那几格跟着重问（用户级 PATH 那一格有自己的「刷新」）。 */
  const REREAD: Record<Plat, string[]> = {
    posix: ["aliases_read", "local_ccm_entry_status", "profiles_read"],
    powershell: ["aliases_read", "bound_terminal_count", "cc_get_auto_launch", "ccm_user_path_status", "local_ccm_entry_status", "profiles_read"],
  };

  const FIRST_OPEN: Record<Plat, string[]> = {
    posix: ["aliases_read", "local_ccm_entry_status", "profiles_read"],
    powershell: ["aliases_read", "bound_terminal_count", "cc_get_auto_launch", "ccm_user_path_status", "local_ccm_entry_status", "profiles_read"],
  };

  it("★ 构造零 I/O；第一次展开**恰好**那几发，再展开一发都不多；别名那几发都带这个平台的 shell", async () => {
    const el = await mount();
    expect(seen, "还没展开就发了 IPC").toEqual([]);
    await open(el);
    expect(seen.map((c) => c.cmd).sort()).toEqual([...FIRST_OPEN[plat]].sort());
    seen = [];
    await open(el);
    expect(seen.map((c) => c.cmd).sort(), "再露出来 ⇒ 重读一遍（不多问别的）").toEqual(REREAD[plat]);
    const shells = seen.filter((c) => c.cmd === "aliases_read").map((c) => (c.args as { shell: string }).shell);
    expect(new Set(shells)).toEqual(new Set([plat]));
  });

  it("★ 回到这一页 / 别处改了这台的别名清单：展开过的按机器重读一遍；没展开过的、别台的都不读", async () => {
    const m = await import("../../../../src/frontend/ui/settings/machine-aliases");
    const el = await mount();
    m.rereadAliases("<local>");
    await flush();
    expect(seen, "没展开过就读了").toEqual([]);
    await open(el);
    seen = [];
    m.rereadAliases("devbox");
    await flush();
    expect(seen, "别台的清单变了，这台跟着读了").toEqual([]);
    m.rereadAliases("<local>");
    await flush();
    expect(seen.map((c) => c.cmd).sort()).toEqual(REREAD[plat]);
  });

  it("「终端接入」在最上、先说现状：没接入 ⇒ 醒目的「接入 …」；接入之后说「已接入」并给「卸载 ccm」，两发都带那份文件", async () => {
    const el = await mount();
    await open(el);
    const access = el.querySelector<HTMLElement>('[data-role="access"]')!;
    const list = el.querySelector('[data-role="profiles"]')!;
    expect(access.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING, "接入不在清单上面").toBeTruthy();
    const off = plat === "powershell" ? copyText("machineAliases.access.offWindow") : copyText("machineAliases.access.off");
    expect(access.textContent).toContain(off);
    expect(access.dataset.anchor, "主窗口 ↗［接上终端］落到这里").toBe("connect-terminal");
    clickText(access, copyText("machineAliases.access.connectTo", { path: "~/rc-a" }));
    await flush();
    expect(seen.filter((c) => c.cmd === "aliases_block_install").map((c) => c.args)).toEqual([{ origin: "<local>", rcPath: "/h/rc-a" }]);
    const on = plat === "powershell" ? copyText("machineAliases.access.onWindow", { path: "~/rc-a" }) : copyText("machineAliases.access.on", { path: "~/rc-a" });
    expect(access.textContent).toContain(on);
    // 卸载先给要拿掉的那几行（问后端要块的渲染），再点一次才卸。
    clickText(access, copyText("machineAliases.rc.uninstall"));
    await flush();
    expect(seen.filter((c) => c.cmd === "aliases_block_remove"), "没确认就卸了").toEqual([]);
    expect(access.querySelector('[data-role="block-text"]')!.textContent).toBe("# 接入那几行 → /h/rc-a");
    clickText(access.querySelector(".cfg-panel")!, copyText("machineAliases.rc.uninstall"));
    await flush();
    expect(seen.filter((c) => c.cmd === "aliases_block_remove").map((c) => c.args)).toEqual([{ origin: "<local>", rcPath: "/h/rc-a" }]);
    expect(access.textContent).toContain(off);
  });

  it("同名那一行照后端给的码说谁生效（界面不比行号）· 旧版块给「换成新版」", async () => {
    clashes = [{ name: "cc", line: 5, wins: "yours" }];
    blockAt.add("/h/rc-a");
    oldAt.add("/h/rc-a");
    const el = await mount();
    await open(el);
    // 同名函数交给清单那一块画（三个选择）：标题里说生效的是哪一个（按后端给的码取句）。
    const clash = el.querySelector<HTMLElement>('[data-role="clash"]')!;
    expect(clash.dataset.anchor, "「要你动手」同名那一件［去定…］落不到这里").toBe("clash");
    expect(clash.textContent).toContain(copyText("machineAliases.clash.nowYours"));
    expect(clash.textContent).toContain("~/rc-a");
    const access = el.querySelector<HTMLElement>('[data-role="access"]')!;
    expect(access.textContent).toContain(copyText("machineAliases.access.outdated", { path: "~/rc-a" }));
    clickText(access, copyText("machineAliases.access.reconnect"));
    await flush();
    expect(access.textContent).not.toContain(copyText("machineAliases.access.outdated", { path: "~/rc-a" }));
    // 码换成「清单那条」⇒ 句子跟着换（同一份界面，只认码）。
    clashes = [{ name: "cc", line: 5, wins: "list" }];
    built.get(el)!.load();
    await flush();
    expect(el.querySelector('[data-role="clash"]')!.textContent).toContain(copyText("machineAliases.clash.nowList"));
  });

  it("「我自己贴」交那台记下、去「要你动手」那一件（不再就地弹代码）；那一行变「已选自己贴」；［改由 cc-monitor 接上］撤记录再照常接上", async () => {
    const el = await mount();
    const went: unknown[] = [];
    el.addEventListener("settings-go", (e) => went.push((e as CustomEvent).detail));
    await open(el);
    clickText(el, copyText("machineAliases.access.selfPaste"));
    await flush();
    expect(seen.filter((c) => c.cmd === "chan:chores-mark").map((c) => c.args)).toEqual([{ origin: "<local>", op: "selfPaste", rc: "/h/rc-a" }]);
    expect(seen.some((c) => c.cmd === "aliases_block_render"), "不再就地渲染那几行").toBe(false);
    expect(went).toEqual([{ page: "data", anchor: "chores:<local>" }]);
    const access = el.querySelector<HTMLElement>('[data-role="access"]')!;
    expect(access.textContent).toContain(copyText("machineAliases.selfPaste.waiting"));
    clickText(access, copyText("machineAliases.selfPaste.undo"));
    await flush();
    expect(seen.filter((c) => c.cmd === "chan:chores-mark").at(-1)!.args).toEqual({ origin: "<local>", op: "unselfPaste" });
    expect(seen.filter((c) => c.cmd === "aliases_block_install").map((c) => (c.args as { rcPath: string }).rcPath)).toEqual(["/h/rc-a"]);
  });

  it("换一份：其它文件交给读回口过围栏、选中它再接入；过不了围栏 ⇒ 原话上屏", async () => {
    const el = await mount();
    await open(el);
    clickText(el, copyText("machineAliases.access.choose"));
    const other = el.querySelector<HTMLInputElement>(".ccm-rc-other")!;
    other.value = "/etc/x";
    clickText(el, copyText("machineAliases.rc.useOther"));
    await flush();
    expect(el.textContent).toContain("拒绝写这个配置文件");
    other.value = "~/.zshrc";
    clickText(el, copyText("machineAliases.rc.useOther"));
    await flush();
    clickText(el.querySelector('[data-role="access"]')!, copyText("machineAliases.access.connectTo", { path: "~/.zshrc" }));
    await flush();
    expect(seen.filter((c) => c.cmd === "aliases_block_install").map((c) => (c.args as { rcPath: string }).rcPath)).toEqual(["/h/.zshrc"]);
    const last = [...seen].reverse().find((c) => c.cmd === "aliases_read")!.args as { rcPath: string };
    expect(last.rcPath, "指过的那一份之后每次读回都带着").toBe("~/.zshrc");
  });

  it("远端卡是同一个组件：每一发都带那台的 origin，只本机的那几格不挂", async () => {
    const m = await import("../../../../src/frontend/ui/settings/machine-aliases");
    const done: string[] = [];
    const mgr = m.buildAliasManager({
      platform: "posix",
      origin: () => "devbox",
      onBlockDone: (verb, err) => done.push(`${verb}:${err ?? "ok"}`),
    });
    const el = mgr.element;
    built.set(el, mgr);
    document.body.appendChild(el);
    await flush();
    expect(seen, "构造零 I/O").toEqual([]);
    await open(el);
    expect(seen.map((c) => c.cmd).sort()).toEqual(["aliases_read", "profiles_read"]);
    clickText(el.querySelector('[data-role="access"]')!, copyText("machineAliases.access.connectTo", { path: "~/rc-a" }));
    await flush();
    clickText(el.querySelector('[data-role="access"]')!, copyText("machineCard.aliases.uninstall"));
    await flush();
    clickText(el.querySelector(".cfg-panel")!, copyText("machineCard.aliases.uninstall"));
    await flush();
    clickText(el, copyText("machineAliases.access.selfPaste"));
    await flush();
    const sent = seen.filter((c) => c.cmd.startsWith("aliases_") || c.cmd.startsWith("profiles_") || c.cmd === "chan:chores-mark");
    expect(new Set(sent.map((c) => c.cmd))).toEqual(new Set(["aliases_read", "profiles_read", "aliases_block_render", "aliases_block_install", "aliases_block_remove", "chan:chores-mark"]));
    expect(sent.filter((c) => (c.args as { origin?: string }).origin !== "devbox"), "有一发没带那台的 origin").toEqual([]);
    expect(seen.filter((c) => !sent.includes(c)), "远端卡问了只有本机才答得了的事").toEqual([]);
    expect(done).toEqual(["install:ok", "remove:ok"]);
    expect(el.querySelector(".ccm-user-path-block")).toBeNull();
    expect([...el.querySelectorAll("button")].map((b) => b.textContent)).not.toContain(copyText("machineAliases.rc.open"));
  });
});

describe("localShell：本机用哪种方言", () => {
  afterEach(async () => {
    const { __setHostOsForTests } = await import("../../../../src/frontend/ui/settings/host-os");
    __setHostOsForTests(null);
  });
  it("Windows ⇒ powershell；Linux / macOS ⇒ posix；认不出 ⇒ 不猜（null），那一格明说、接入入口置灰", async () => {
    vi.resetModules();
    const { __setHostOsForTests } = await import("../../../../src/frontend/ui/settings/host-os");
    const { localShell, buildUnknownOsAliasBlock } = await import("../../../../src/frontend/ui/settings/machine-aliases");
    const want = { windows: "powershell", linux: "posix", macos: "posix", unknown: null } as const;
    for (const [os, sh] of Object.entries(want)) {
      __setHostOsForTests(os as "windows" | "linux" | "macos" | "unknown");
      expect(localShell(), os).toBe(sh);
    }
    const block = buildUnknownOsAliasBlock();
    expect(block.textContent).toContain(copyText("machineAliases.unknownOs.said"));
    expect([...block.querySelectorAll("button")].map((b) => [b.textContent, b.disabled])).toEqual([[copyText("machineAliases.access.connect"), true]]);
  });
});
