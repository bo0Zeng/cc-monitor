// 机器页 ②「别名」：接入那一格（先显示现状）＋ 清单那一块挂在下面（清单自己的判据在 `profiles-list.vitest.ts`）。
//
// 这里钉的是接入那一半与整块的读法：接入在最上 · 同名函数交给清单那一块画三个选择 · 构造零 I/O · 重读 · 远端同一个组件。
// shell 文本长什么样归后端 `tests/backend/assets/aliases/`，本文件一个字节的 shell 文本都不断言。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { ExecPolicy, NameClash } from "../../../../src/frontend/ui/alias-reads";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { factsOn } from "../../../test-support/host-facts";

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
  let userPathErr: { said: string; detail: string } | null;
  let policyA: ExecPolicy | null;
  let policyAfter: ExecPolicy | null;

  const cand = (path: string, over: { exists?: boolean; policy?: ExecPolicy | null } = {}) => ({
    path,
    sourced: blockAt.has(path),
    policy: over.policy ?? null,
    exists: over.exists ?? true,
    unreadable: null,
    blockLines: 4,
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
    userPathErr = null;
    policyA = null;
    policyAfter = null;
    vi.resetModules();
    toasted.length = 0;
    vi.doMock("../../../../src/frontend/ui/kit/toast", () => ({
      toast: (title: string, body: string) => toasted.push(`${title}|${body}`),
      failToast: (title: string, e: unknown, o?: { fact?: string }) => toasted.push(`${title}|${String(e)}|${o?.fact ?? ""}`),
    }));
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
          if (rcPath === "/etc/x") return Promise.reject(Object.assign(new Error("拒绝写这个配置文件：只能落在 home 之内"), { detail: "d-fence" }));
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
            error: userPathErr,
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
    // 「会在 … 末尾加 N 行」「会动：… 末尾 N 行」的 N 是那台后端数的（候选上的 blockLines），界面不数。
    expect(access.textContent).toContain(copyText("machineAliases.access.willAdd", { path: "~/rc-a", n: 4 }));
    expect(el.textContent).toContain(copyText("machineAliases.writes.line", { path: "~/rc-a", n: 4 }));
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
    expect(clash.dataset.anchor, "「待办」同名那一件［去定…］落不到这里").toBe("clash");
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

  it("「我自己贴」交那台记下、去「待办」那一件（不再就地弹代码）；那一行变「已选自己贴」；［改由 cc-monitor 接上］撤记录再照常接上", async () => {
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

  it("执行策略那一行：不会挡 ⇒ 不占地方；挡住 / 说不清 / 组策略钉着 / 问不到各说各的；只有挡住且不是组策略才给按钮", async () => {
    const pol = (over: Partial<ExecPolicy>): ExecPolicy => ({ host: "powershell", effective: "Restricted", loads: false, groupPolicy: false, error: null, ...over });
    const ps = copyText("machineAliases.policy.hostPowershell");
    const cases: Array<[ExecPolicy, string, boolean]> = [
      [pol({ loads: true }), "", false],
      [pol({}), copyText("machineAliases.policy.blocks", { ps, policy: "Restricted" }), true],
      [pol({ loads: null }), copyText("machineAliases.policy.unclear", { ps, policy: "Restricted" }), false],
      [pol({ groupPolicy: true }), copyText("machineAliases.policy.groupPolicy", { ps, policy: "Restricted" }), false],
      [pol({ effective: null, loads: null, error: copyText("reason.io.unknown") }), copyText("machineAliases.policy.unknown", { ps, why: copyText("reason.io.unknown") }), false],
    ];
    for (const [p, said, btn] of cases) {
      policyA = p;
      blockAt.add("/h/rc-a");
      const el = await mount();
      await open(el);
      const access = el.querySelector<HTMLElement>('[data-role="access"]')!;
      const line = access.querySelector<HTMLElement>(":scope > .cfg-hint:not([data-role])")!;
      expect(line.textContent, JSON.stringify(p)).toBe(said);
      expect(line.hidden).toBe(said === "");
      const allow = [...access.querySelectorAll<HTMLButtonElement>(":scope > button")].find((b) => b.textContent === copyText("machineAliases.policy.allow"))!;
      expect(allow.hidden, JSON.stringify(p)).toBe(!btn);
      const loads = p.loads === false;
      expect(access.textContent!.includes(copyText("machineAliases.access.notLoaded", { path: "~/rc-a", ps })), JSON.stringify(p)).toBe(loads);
      document.body.replaceChildren();
    }
  });

  it("［允许本地脚本］先问一句；不答应不动；答应 ⇒ 请那台设、重读，接着说设好了没有", async () => {
    policyA = { host: "pwsh", effective: "Restricted", loads: false, groupPolicy: false, error: null };
    policyAfter = { host: "pwsh", effective: "RemoteSigned", loads: true, groupPolicy: false, error: null };
    blockAt.add("/h/rc-a");
    let answer = false;
    const asked: string[] = [];
    const el = await mount((spec) => {
      asked.push(spec.body ?? "");
      return answer;
    });
    await open(el);
    const ps = copyText("machineAliases.policy.hostPwsh");
    clickText(el, copyText("machineAliases.policy.allow"));
    await flush();
    expect(asked).toEqual([copyText("machineAliases.policy.confirm", { ps })]);
    expect(seen.some((c) => c.cmd === "powershell_policy_set"), "没答应就改了").toBe(false);
    answer = true;
    seen = [];
    clickText(el, copyText("machineAliases.policy.allow"));
    await flush();
    expect(seen.map((c) => c.cmd).slice(0, 2)).toEqual(["powershell_policy_set", "aliases_read"]);
    expect(el.querySelector('[data-role="access-note"]')!.textContent).toBe(copyText("machineAliases.policy.setDone", { ps, policy: "RemoteSigned" }));
    expect(el.textContent).not.toContain(copyText("machineAliases.access.notLoaded", { path: "~/rc-a", ps }));
  });

  it("收着时那一行：没接上 ⇒ 主动作「接上 …」；旧版块 ⇒ 主动作「更新」；已接上 ⇒ 不给主动作；有同名 ⇒ 点变黄、尾巴点名", async () => {
    const statusOf = (el: HTMLElement): { text: string; dot: string; acts: string[] } => {
      const row = el.querySelector<HTMLElement>('[data-role="access"]')!.closest<HTMLElement>(".cfg-row")!;
      if (!row.querySelector<HTMLElement>(".cfg-body")!.hidden) row.querySelector<HTMLButtonElement>(".cfg-toggle")!.click();
      const head = row.querySelector<HTMLElement>(".cfg-head")!;
      return { text: head.querySelector(".cfg-status")!.textContent ?? "", dot: head.querySelector<HTMLElement>(".cfg-dot")!.dataset.dot ?? "", acts: [...head.querySelectorAll(".cfg-action button")].map((b) => b.textContent ?? "") };
    };
    let el = await mount();
    await open(el);
    expect(statusOf(el)).toMatchObject({ dot: "off", acts: [copyText("machineAliases.status.connect", { path: "~/rc-a" })] });
    document.body.replaceChildren();
    blockAt.add("/h/rc-a");
    oldAt.add("/h/rc-a");
    el = await mount();
    await open(el);
    expect(statusOf(el)).toMatchObject({ dot: "warn", acts: [copyText("machineAliases.status.update")] });
    document.body.replaceChildren();
    oldAt.clear();
    clashes = [{ name: "cc", line: 5, wins: "yours" }];
    el = await mount();
    await open(el);
    const st = statusOf(el);
    expect(st).toMatchObject({ dot: "warn", acts: [] });
    expect(st.text).toContain(copyText("machineAliases.status.clashTail", { names: "cc" }));
    document.body.replaceChildren();
    clashes = [];
    el = await mount();
    await open(el);
    expect(statusOf(el)).toMatchObject({ dot: "ok", acts: [] });
  });

  it("换一份文件的下拉：人换了选项 ⇒ 接上那一跳跟着换文件；候选标出已接上 / 新建", async () => {
    const el = await mount();
    await open(el);
    clickText(el, copyText("machineAliases.access.choose"));
    const sel = el.querySelector<HTMLSelectElement>(".ccm-acct-alias-rc")!;
    expect([...sel.options].map((o) => o.textContent)).toEqual(["/h/rc-a", `/h/rc-b（${copyText("machineAliases.rc.tagNew")}）`]);
    expect(sel.value).toBe("/h/rc-a");
    sel.value = "/h/rc-b";
    sel.dispatchEvent(new Event("change"));
    await flush();
    const access = el.querySelector<HTMLElement>('[data-role="access"]')!;
    clickText(access, copyText("machineAliases.access.connectTo", { path: "~/rc-b" }));
    await flush();
    expect(sel.value, "接上之后下拉停在已接上的那份").toBe("/h/rc-b");
    expect([...sel.options].map((o) => o.textContent)[1]).toContain(copyText("machineAliases.rc.tagBlock"));
    sel.value = "/h/rc-a";
    sel.dispatchEvent(new Event("change"));
    await flush();
    expect(access.textContent).toContain(copyText("machineAliases.access.otherFile", { path: "~/rc-a" }));
    clickText(access, copyText("machineAliases.access.connectTo", { path: "~/rc-a" }));
    await flush();
    expect(seen.filter((c) => c.cmd === "aliases_block_install").map((c) => (c.args as { rcPath: string }).rcPath)).toEqual(["/h/rc-b", "/h/rc-a"]);
  });

  it("「看加了什么」与「卸载」是同一块的两面：再点同一个就收起；没接上时「看一眼」说「会加」", async () => {
    const el = await mount();
    await open(el);
    const access = el.querySelector<HTMLElement>('[data-role="access"]')!;
    const panel = access.querySelector<HTMLElement>(".cfg-panel")!;
    clickText(access, copyText("machineAliases.access.peek"));
    await flush();
    expect(panel.hidden).toBe(false);
    expect(panel.textContent).toContain(copyText("machineAliases.access.previewOff", { path: "~/rc-a" }));
    clickText(access, copyText("machineAliases.access.peek"));
    await flush();
    expect(panel.hidden, "再点同一个没收起").toBe(true);
    clickText(access, copyText("machineAliases.access.connectTo", { path: "~/rc-a" }));
    await flush();
    clickText(access, copyText("machineAliases.access.whatAdded"));
    await flush();
    expect(panel.textContent).toContain(copyText("machineAliases.access.previewOn", { path: "~/rc-a" }));
    clickText(access, copyText("machineAliases.rc.uninstall"));
    await flush();
    const sub = plat === "powershell" ? copyText("machineAliases.access.uninstallSubWindow") : copyText("machineAliases.access.uninstallSub");
    expect(panel.textContent).toContain(sub);
    expect(panel.textContent).toContain(copyText("machineAliases.access.uninstallWhat", { path: "~/rc-a" }));
    clickText(panel, copyText("machineAliases.form.cancel"));
    await flush();
    expect(panel.hidden).toBe(true);
    expect(seen.some((c) => c.cmd === "aliases_block_remove")).toBe(false);
  });

  it("本机才有「用系统编辑器打开」：打开选中的那份；打不开 ⇒ 出声", async () => {
    const opener = await import("@tauri-apps/plugin-opener");
    const openPath = opener.openPath as unknown as ReturnType<typeof vi.fn>;
    const el = await mount();
    await open(el);
    clickText(el, copyText("machineAliases.access.choose"));
    clickText(el, copyText("machineAliases.rc.open"));
    await flush();
    expect(openPath.mock.calls).toEqual([["/h/rc-a"]]);
    openPath.mockRejectedValueOnce(new Error("没有关联程序"));
    clickText(el, copyText("machineAliases.rc.open"));
    await flush();
    // 交给失败 toast：那次失败（带详情 ⇒ 换成那一句）＋ 灰字一格事实（那份的路径）。
    expect(toasted).toEqual([`${copyText("machineAliases.openRc.failed")}|Error: 没有关联程序|/h/rc-a`]);
  });

  it("用户 PATH 探不动（只 PowerShell 有这一格）：那一句上屏、开关不给拨；PowerShell 的原话不上屏，跟在［复制详情］里", async () => {
    userPathErr = { said: copyText("rsProfileInstaller.ps.exitCode"), detail: "原话：Access is denied." };
    const el = await mount();
    await open(el);
    const help = el.querySelector<HTMLElement>(".ccm-user-path-status");
    if (plat === "posix") {
      expect(help, "POSIX 没有用户级 PATH 那一格").toBeNull();
      return;
    }
    expect(help).not.toBeNull();
    if (help === null) return;
    expect(help.textContent).toContain(copyText("rsProfileInstaller.ps.exitCode"));
    expect(help.textContent, "原话不上屏").not.toContain("Access is denied");
    expect(help.querySelector('[data-part="copy-detail"]'), "有详情 ⇒ 句子后面跟［复制详情］").not.toBeNull();
    expect(el.querySelector('[data-role="win-path"] button')!.getAttribute("aria-disabled")).toBe("true");
  });

  it("远端卡是同一个组件：每一发都带那台的 origin，只本机的那几格不挂", async () => {
    const m = await import("../../../../src/frontend/ui/settings/machine-aliases");
    const mgr = m.buildAliasManager({
      platform: "posix",
      origin: () => "devbox",
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
    expect(el.querySelector(".ccm-user-path-block")).toBeNull();
    expect([...el.querySelectorAll("button")].map((b) => b.textContent)).not.toContain(copyText("machineAliases.rc.open"));
  });
});

describe("localShell：本机用哪种方言", () => {
  afterEach(async () => {
    const { __setHostFactsForTests } = await import("../../../../src/frontend/ui/settings/host-os");
    __setHostFactsForTests(null);
  });
  it("Windows ⇒ powershell；Linux / macOS ⇒ posix；认不出 ⇒ 不猜（null），那一格明说、接入入口置灰", async () => {
    vi.resetModules();
    const { __setHostFactsForTests } = await import("../../../../src/frontend/ui/settings/host-os");
    const { localShell, buildUnknownOsAliasBlock } = await import("../../../../src/frontend/ui/settings/machine-aliases");
    const want = { windows: "powershell", linux: "posix", macos: "posix", unknown: null } as const;
    for (const [os, sh] of Object.entries(want)) {
      __setHostFactsForTests(factsOn(os as "windows" | "linux" | "macos" | "unknown"));
      expect(localShell(), os).toBe(sh);
    }
    const block = buildUnknownOsAliasBlock();
    expect(block.textContent).toContain(copyText("machineAliases.unknownOs.said"));
    expect([...block.querySelectorAll("button")].map((b) => [b.textContent, b.disabled])).toEqual([[copyText("machineAliases.access.connect"), true]]);
  });
});
