// 机器页 ②「别名」。
//
// 这里钉的是**界面那一半**：接入在最上、先显示现状 · 清单两组照后端给的归组画（不自己认）· 新增 / 改就地展开同一张表单 ·
// 一步存（带读回时的指纹；被别处改过 ⇒ 重读、表单留着）· 「会执行：…」问后端 · 我自己贴给的是接入那几行 · 构造零 I/O。
// shell 文本长什么样、真 bash 执行下来对不对，归后端 `tests/backend/assets/aliases/aliases_tests.rs`
// —— 本文件一个字节的 shell 文本都不断言（前端也一个字节都不拼）。
// 表单 ⇄ 参数也在后端（`tests/backend/assets/aliases/form_tests.rs`）：这里的替身故意不按 ccm 的写法拼，
// 只钉界面把哪几格原样交过去（含「改」时原来那一条 · 表单没有格子的那一串）、拿回来的那一条又原样用在哪。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { AccountShape, Alias, AliasForm, ExecPolicy, MissingAlias, NameClash } from "../../../../src/frontend/ui/alias-reads";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 12; i += 1) await Promise.resolve();
};

const A = (name: string, args: string[], restTo: "agent" | "ccm" = "agent"): Alias => ({ name, args, restTo });

/** 替身「后端」把表单拼成的那一条：故意不是 ccm 的写法（界面不该在乎拼成什么，只原样用）。 */
const enc = (f: AliasForm): string[] => [
  `form:${f.account}|${f.base}|${f.tmux}|${f.cwdIf.map((c) => `${c.at}>${c.to}`).join(",")}|${f.cwd}|${f.passthru}|${f.ccmOther}`,
];

// 一份组件、平台是入参 ⇒ 同一套断言对两个平台各跑一遍，平台那几格（tmux 能力 · 首开发哪几发 · PowerShell 那几格）各有各的期望。
type Plat = "posix" | "powershell";

describe.each<Plat>(["posix", "powershell"])("buildAliasManager（%s）", (plat) => {
  let seen: Array<{ cmd: string; args?: unknown }>;
  let disk: Alias[];
  let groups: Array<AccountShape | null>;
  let missing: MissingAlias[];
  let fp: number;
  let blockAt: Set<string>;
  let oldAt: Set<string>;
  let clashes: NameClash[];
  let autoLaunchFail: string | null;
  let policyA: ExecPolicy | null;
  let policyAfter: ExecPolicy | null;
  /** 替身「后端」摊开 / 拼回时说的那句话（`null` = 照常答）。 */
  let toFormFail: string | null;
  let fromFormFail: string | null;

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
    disk = [A("cc", []), A("alphacc", ["--", "--account", "z"]), A("mine", ["--", "--account", "z", "--cwd", "/w"]), A("alphacct", ["--", "--account", "z", "--ccm-tmux"])];
    // 归组是后端给的：第三条参数里有 `--account z` 但不是账号那一形 ⇒ `null`（界面不许自己认）。
    groups = [null, { account: "z", tmux: false }, null, { account: "z", tmux: true }];
    missing = [
      { account: "b", tmux: true, alias: A("betacct", ["--", "--account", "b", "--ccm-tmux"]) },
      { account: "b", tmux: false, alias: A("betacc", ["--", "--account", "b"]) },
    ];
    fp = 1;
    blockAt = new Set();
    oldAt = new Set();
    clashes = [];
    autoLaunchFail = null;
    policyA = null;
    policyAfter = null;
    toFormFail = null;
    fromFormFail = null;
    vi.resetModules();
    vi.doMock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));
    vi.doMock("../../../../src/comms/inward/chan", () => ({
      ChanError: class ChanError extends Error {},
      chan: {
        call: (origin: string, op: string, body: Uint8Array) => {
          const args = JSON.parse(new TextDecoder().decode(body)) as { args: string[] };
          seen.push({ cmd: `chan:${op}`, args: { origin, ...args } });
          return Promise.resolve(new TextEncoder().encode(JSON.stringify({ line: `LINE ${args.args.join(" ")}` })));
        },
      },
    }));
    vi.doMock("../../../../src/frontend/ui/alias-reads", async (orig) => {
      class AliasesStale extends Error {}
      const real = await orig<typeof import("../../../../src/frontend/ui/alias-reads")>();
      return {
        AliasesStale,
        emptyForm: real.emptyForm,
        aliasToForm: (origin: string, alias: Alias) => {
          seen.push({ cmd: "aliases_to_form", args: { origin, alias } });
          if (toFormFail !== null) return Promise.reject(new Error(toFormFail));
          // 替身照参数里那几个词填几格（只为看界面把它们原样摆出来）；表单没格子的那一串原样回。
          const at = (w: string): string => alias.args[alias.args.indexOf(w) + 1] ?? "";
          return Promise.resolve({
            ...real.emptyForm(),
            name: alias.name,
            account: alias.args.includes("--account") ? at("--account") : "",
            cwd: alias.args.includes("--cwd") ? at("--cwd") : "",
            passthru: "--x 'a b'",
            ccmOther: "--account-dir /srv/acc",
          });
        },
        aliasFromForm: (origin: string, form: AliasForm, orig: Alias | null) => {
          seen.push({ cmd: "aliases_from_form", args: { origin, form, orig } });
          if (fromFormFail !== null) return Promise.reject(new Error(fromFormFail));
          return Promise.resolve(A(form.name.trim(), enc(form), form.tmux === "attach" ? "ccm" : "agent"));
        },
        readAliases: (origin: string, shell: Plat, rcPath: string | null) => {
          seen.push({ cmd: "aliases_read", args: { origin, shell, rcPath } });
          if (rcPath === "/etc/x") return Promise.reject(new Error("拒绝写这个配置文件：只能落在 home 之内"));
          const other = rcPath ? `/h/${rcPath.replace(/^~\//, "")}` : null;
          return Promise.resolve({
            aliasPath: "/h/.cc-monitor/aliases.x",
            exists: true,
            aliases: disk.map((x) => ({ ...x, args: [...x.args] })),
            groups: [...groups],
            accounts: ["z", "b"],
            missing: [...missing],
            fingerprint: `fp-${fp}`,
            unparsed: ["alias x=ls（认不出）"],
            rcCandidates: [cand("/h/rc-a", { policy: policyA }), cand("/h/rc-b", { exists: false }), ...(other ? [cand(other)] : [])],
            otherRc: other,
          });
        },
        renderAliases: (origin: string, aliases: Alias[], shell: Plat) => {
          seen.push({ cmd: "aliases_render", args: { origin, aliases, shell } });
          return Promise.resolve({
            fileText: "整份别名文件不该出现在界面上",
            lines: [],
            problems: aliases.filter((a) => a.name === "bad").map((a) => ({ name: a.name, message: "名字不对" })),
            collisions: aliases.filter((a) => a.name === "sh").map(() => "sh 撞了这台 PATH 上的程序"),
          });
        },
        installAliases: (origin: string, aliases: Alias[], shell: Plat, fingerprint: string | null) => {
          seen.push({ cmd: "aliases_install", args: { origin, aliases, shell, fingerprint } });
          if (fingerprint !== `fp-${fp}`) return Promise.reject(new AliasesStale("那份文件在这一页读到它之后被别处改过，这次没写。"));
          disk = aliases;
          groups = aliases.map((a) => (a.name === "betacc" ? { account: "b", tmux: false } : null));
          fp += 1;
          return Promise.resolve({ aliasPath: "/h/.cc-monitor/aliases.x", wroteAliasFile: true });
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

  async function mount(confirm?: (msg: string) => boolean): Promise<HTMLDetailsElement> {
    const m = await import("../../../../src/frontend/ui/settings/machine-aliases");
    const el = m.buildAliasManager({ platform: plat, origin: () => "<local>", confirm }) as HTMLDetailsElement;
    document.body.appendChild(el);
    await flush();
    return el;
  }

  async function open(el: HTMLDetailsElement): Promise<void> {
    el.open = true;
    el.dispatchEvent(new Event("toggle"));
    await flush();
  }

  const clickText = (root: Element, text: string): void => {
    const b = [...root.querySelectorAll<HTMLButtonElement>("button")].find((x) => x.textContent === text);
    if (!b) throw new Error(`找不到按钮「${text}」`);
    b.click();
  };
  const rowNames = (box: Element | null): string[] =>
    [...(box?.querySelectorAll(".machine-aliases-row") ?? [])].map(
      (r) => r.querySelector("code")?.textContent ?? `缺:${r.textContent}`,
    );
  const installs = (): Array<{ aliases: Alias[]; fingerprint: string | null }> =>
    seen
      .filter((c) => c.cmd === "aliases_install")
      .map((c) => {
        const { aliases, fingerprint } = c.args as { aliases: Alias[]; fingerprint: string | null };
        return { aliases, fingerprint };
      });
  const setField = (root: Element, role: string, value: string): void => {
    const f = root.querySelector<HTMLInputElement | HTMLSelectElement>(`[data-role="${role}"]`)!;
    f.value = value;
    f.dispatchEvent(new Event("change"));
  };

  const FIRST_OPEN: Record<Plat, string[]> = {
    posix: ["aliases_read", "aliases_render", "local_ccm_entry_status"],
    powershell: ["aliases_read", "aliases_render", "bound_terminal_count", "cc_get_auto_launch", "ccm_user_path_status", "local_ccm_entry_status"],
  };

  it("★ 构造零 I/O；第一次展开**恰好**那几发，再展开一发都不多；别名那几发都带这个平台的 shell", async () => {
    const el = await mount();
    expect(seen, "还没展开就发了 IPC").toEqual([]);
    await open(el);
    expect(seen.map((c) => c.cmd).sort()).toEqual([...FIRST_OPEN[plat]].sort());
    const n = seen.length;
    el.open = false;
    el.dispatchEvent(new Event("toggle"));
    await open(el);
    expect(seen.length, "再展开又读了一遍").toBe(n);
    const shells = seen.filter((c) => c.cmd === "aliases_read" || c.cmd === "aliases_render").map((c) => (c.args as { shell: string }).shell);
    expect(new Set(shells)).toEqual(new Set([plat]));
  });

  it("「终端接入」在最上、先说现状：没接入 ⇒ 醒目的「接入 …」；接入之后说「已接入」并给「卸载 ccm」，两发都带那份文件", async () => {
    const el = await mount();
    await open(el);
    const access = el.querySelector<HTMLElement>('[data-role="access"]')!;
    const list = el.querySelector(".machine-aliases-list")!;
    expect(access.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING, "接入不在清单上面").toBeTruthy();
    expect(access.textContent).toContain("还没接入");
    clickText(access, "接入 /h/rc-a");
    await flush();
    expect(seen.filter((c) => c.cmd === "aliases_block_install").map((c) => c.args)).toEqual([{ origin: "<local>", rcPath: "/h/rc-a" }]);
    expect(access.textContent).toContain("/h/rc-a 已接入");
    clickText(access, "卸载 ccm");
    await flush();
    expect(seen.filter((c) => c.cmd === "aliases_block_remove").map((c) => c.args)).toEqual([{ origin: "<local>", rcPath: "/h/rc-a" }]);
    expect(access.textContent).toContain("还没接入");
  });

  it("接入那一格就地说：块外与清单同名的函数（带行号）· 旧版块给「重新接入」", async () => {
    clashes = [{ name: "cc", line: 5 }];
    blockAt.add("/h/rc-a");
    oldAt.add("/h/rc-a");
    const el = await mount();
    await open(el);
    const access = el.querySelector<HTMLElement>('[data-role="access"]')!;
    expect(access.textContent).toContain("/h/rc-a 第 5 行自己定义了 cc");
    expect(access.textContent).toContain("旧版");
    clickText(access, "重新接入");
    await flush();
    expect(access.textContent).not.toContain("旧版");
  });

  it("「我自己贴」给的是接入那几行（问后端要块的渲染），整份清单从不上屏", async () => {
    const el = await mount();
    await open(el);
    clickText(el, "我自己贴");
    await flush();
    expect(seen.filter((c) => c.cmd === "aliases_block_render").map((c) => c.args)).toEqual([{ origin: "<local>", rcPath: "/h/rc-a" }]);
    const out = el.querySelector<HTMLTextAreaElement>(".ccm-rc-paste textarea")!;
    expect(out.value).toBe("# 接入那几行 → /h/rc-a");
    expect(el.innerHTML).not.toContain("整份别名文件不该出现在界面上");
  });

  it("换一份：其它文件交给读回口过围栏、选中它再接入；过不了围栏 ⇒ 原话上屏", async () => {
    const el = await mount();
    await open(el);
    clickText(el, "换一份…");
    const other = el.querySelector<HTMLInputElement>(".ccm-rc-other")!;
    other.value = "/etc/x";
    clickText(el, "用这份");
    await flush();
    expect(el.textContent).toContain("拒绝写这个配置文件");
    other.value = "~/.zshrc";
    clickText(el, "用这份");
    await flush();
    clickText(el.querySelector('[data-role="access"]')!, "接入 /h/.zshrc");
    await flush();
    expect(seen.filter((c) => c.cmd === "aliases_block_install").map((c) => (c.args as { rcPath: string }).rcPath)).toEqual(["/h/.zshrc"]);
    const last = [...seen].reverse().find((c) => c.cmd === "aliases_read")!.args as { rcPath: string };
    expect(last.rcPath, "指过的那一份之后每次读回都带着").toBe("~/.zshrc");
  });

  it("清单两组照后端的归组画：账号组按账号表的顺序、同号先当前终端后 tmux、缺的给「加上」；参数像账号形但后端没归组的留在「其他」", async () => {
    const el = await mount();
    await open(el);
    const acct = el.querySelector('[data-role="group-accounts"]');
    const other = el.querySelector('[data-role="group-other"]');
    expect(rowNames(acct)).toEqual(["alphacc", "alphacct", "缺:b 还没有 betacc（在当前终端起）加上", "缺:b 还没有 betacct（在 tmux 里起）加上"]);
    expect(rowNames(other)).toEqual(["cc", "mine"]);
    expect(el.textContent).toContain("认不出");
    // 「加上」= 清单末尾多那一条，带读回时的指纹一步存；存完重读。
    const reads = seen.filter((c) => c.cmd === "aliases_read").length;
    const before = [...disk];
    clickText(acct!, "加上");
    await flush();
    expect(installs()).toEqual([{ aliases: [...before, A("betacc", ["--", "--account", "b"])], fingerprint: "fp-1" }]);
    expect(seen.filter((c) => c.cmd === "aliases_read").length).toBe(reads + 1);
  });

  it("＋ 新增别名：就地展开一张空表单（账号下拉读那台的账号表）；表单原样交后端拼，「会执行：…」与保存都用拼回来的那一条", async () => {
    const el = await mount();
    await open(el);
    clickText(el, "＋ 新增别名");
    const form = el.querySelector<HTMLElement>('[data-role="new-slot"] [data-role="alias-form"]')!;
    expect(form, "新增的表单没在清单顶上就地展开").toBeTruthy();
    expect(seen.filter((c) => c.cmd === "aliases_to_form"), "新增也去问后端摊开").toEqual([]);
    const acctSel = form.querySelector<HTMLSelectElement>('[data-role="account"]')!;
    expect([...acctSel.options].map((o) => o.value)).toEqual(["", "(base)", "z", "b"]);
    setField(form, "name", "work");
    setField(form, "account", "b");
    clickText(form, "＋ 按所在目录分一种情况");
    const [at, to] = [...form.querySelectorAll<HTMLInputElement>('[data-role="cwd-case"] input')];
    at.value = "~";
    to.value = "~/w";
    to.dispatchEvent(new Event("change"));
    await flush();
    const asks = seen.filter((c) => c.cmd === "aliases_from_form").map((c) => c.args as { form: AliasForm; orig: Alias | null });
    const last = asks.at(-1)!;
    expect(last.orig, "新增没有原来那一条").toBeNull();
    expect(last.form).toMatchObject({ name: "work", account: "b", base: false, cwdIf: [{ at: "~", to: "~/w" }], ccmOther: "" });
    const want = enc(last.form);
    expect(form.querySelector('[data-role="would-run"]')!.textContent).toContain(`LINE ${want.join(" ")}`);
    const before = [...disk];
    clickText(form, "保存");
    await flush();
    expect(installs().at(-1)).toEqual({ aliases: [...before, A("work", want)], fingerprint: "fp-1" });
    expect(el.querySelector('[data-role="alias-form"]'), "存成了表单还开着").toBeNull();
    // 「不用任何账号」那一项：交过去的是 `base`，不是一个号名。
    clickText(el, "＋ 新增别名");
    const again = el.querySelector<HTMLElement>('[data-role="alias-form"]')!;
    setField(again, "account", "(base)");
    await flush();
    expect(seen.filter((c) => c.cmd === "aliases_from_form").at(-1)!.args).toMatchObject({ form: { account: "", base: true } });
  });

  it("改：先问后端把那一条摊成表单、在那一行下面照原样填好；交回时带着原来那一条与表单没有格子的那一串；存回原位（不追加）；撞名就地说", async () => {
    const el = await mount();
    await open(el);
    const row = [...el.querySelectorAll<HTMLElement>(".machine-aliases-row")].find((r) => r.querySelector("code")?.textContent === "mine")!;
    clickText(row, "改");
    await flush();
    const mine = A("mine", ["--", "--account", "z", "--cwd", "/w"]);
    expect(seen.filter((c) => c.cmd === "aliases_to_form").map((c) => c.args)).toEqual([{ origin: "<local>", alias: mine }]);
    const form = row.nextElementSibling as HTMLElement;
    expect(form.dataset.role).toBe("alias-form");
    expect(form.querySelector<HTMLInputElement>('[data-role="name"]')!.value).toBe("mine");
    expect(form.querySelector<HTMLSelectElement>('[data-role="account"]')!.value).toBe("z");
    expect([...form.querySelectorAll<HTMLInputElement>("input")].map((i) => i.value)).toContain("--x 'a b'");
    setField(form, "name", "sh");
    await flush();
    const last = seen.filter((c) => c.cmd === "aliases_from_form").at(-1)!.args as { form: AliasForm; orig: Alias | null };
    expect(last.orig).toEqual(mine);
    expect(last.form).toMatchObject({ name: "sh", account: "z", cwd: "/w", passthru: "--x 'a b'", ccmOther: "--account-dir /srv/acc" });
    expect(form.querySelector('[data-role="form-notes"]')!.textContent).toContain("sh 撞了这台 PATH 上的程序");
    clickText(form, "保存");
    await flush();
    expect(installs().at(-1)!.aliases.map((a) => a.name)).toEqual(["cc", "alphacc", "sh", "alphacct"]);
    expect(installs().at(-1)!.aliases[2]).toEqual(A("sh", enc(last.form)));
  });

  it("拼不出（那台后端说引号没配对）⇒ 那句话就地说、「会执行」清空；保存也只说那句话、一发都不写", async () => {
    const el = await mount();
    await open(el);
    clickText(el, "＋ 新增别名");
    const form = el.querySelector<HTMLElement>('[data-role="alias-form"]')!;
    await flush();
    expect(form.querySelector('[data-role="would-run"]')!.textContent, "空表单先有一行「会执行」").toContain("LINE ");
    fromFormFail = "交给 agent 的参数里有引号没配对。";
    setField(form, "name", "late");
    await flush();
    expect(form.querySelector('[data-role="form-notes"]')!.textContent).toBe("交给 agent 的参数里有引号没配对。");
    expect(form.querySelector('[data-role="would-run"]')!.textContent).toBe("");
    const n = installs().length;
    clickText(form, "保存");
    await flush();
    expect(installs().length, "拼不出还写了").toBe(n);
    expect(form.querySelector('[data-role="form-notes"]')!.textContent).toBe("交给 agent 的参数里有引号没配对。");
    expect(form.isConnected).toBe(true);
  });

  it("「改」那一下摊不开（那台后端答不了）⇒ 状态行说一句，不展开半张表单", async () => {
    const el = await mount();
    await open(el);
    toFormFail = "那台机器的后端太旧";
    const row = [...el.querySelectorAll<HTMLElement>(".machine-aliases-row")].find((r) => r.querySelector("code")?.textContent === "mine")!;
    clickText(row, "改");
    await flush();
    expect(el.querySelector('[data-role="alias-form"]')).toBeNull();
    expect(el.querySelector(".machine-aliases-status")!.textContent).toBe(
      copyText("machineAliases.form.openFailed", { e: "那台机器的后端太旧" }),
    );
  });

  it("存的时候盘上已被别处改过 ⇒ 那句话就地说、重读一遍、表单留着；再存带新指纹，照常写", async () => {
    const el = await mount();
    await open(el);
    clickText(el, "＋ 新增别名");
    const form = el.querySelector<HTMLElement>('[data-role="alias-form"]')!;
    setField(form, "name", "late");
    fp = 7; // 读回之后，盘上那份被别处换了
    const reads = seen.filter((c) => c.cmd === "aliases_read").length;
    clickText(form, "保存");
    await flush();
    expect(installs().at(-1)!.fingerprint).toBe("fp-1");
    expect(form.querySelector('[data-role="form-notes"]')!.textContent).toContain("被别处改过");
    expect(seen.filter((c) => c.cmd === "aliases_read").length, "拒了没重读").toBe(reads + 1);
    expect(form.isConnected, "拒了表单就没了").toBe(true);
    clickText(form, "保存");
    await flush();
    expect(installs().at(-1)!.fingerprint).toBe("fp-7");
    expect(form.isConnected).toBe(false);
  });

  it("删：那一条不在了的整份清单一步存；点一行展开「会执行什么」（问一次后端）", async () => {
    const el = await mount();
    await open(el);
    const zRow = (): HTMLElement =>
      [...el.querySelectorAll<HTMLElement>(".machine-aliases-row")].find((r) => r.querySelector("code")?.textContent === "alphacc")!;
    zRow().click();
    await flush();
    expect(seen.filter((c) => c.cmd === "chan:ccm-print").map((c) => c.args)).toEqual([{ origin: "<local>", args: ["--", "--account", "z"] }]);
    expect((zRow().nextElementSibling as HTMLElement).textContent).toContain("LINE -- --account z");
    clickText(zRow(), "删");
    await flush();
    expect(installs().at(-1)!.aliases.map((a) => a.name)).toEqual(["cc", "mine", "alphacct"]);
  });

  it("「在哪起」：POSIX 五项都能选（接回会话 ⇒ 账号那几格一起关）；PowerShell 只有「在当前终端起」", async () => {
    const el = await mount();
    await open(el);
    clickText(el, "＋ 新增别名");
    const form = el.querySelector<HTMLElement>('[data-role="alias-form"]')!;
    const where = form.querySelector<HTMLSelectElement>('[data-role="where"]')!;
    const enabled = [...where.options].filter((o) => !o.disabled).map((o) => o.value);
    if (plat === "posix") {
      expect(enabled).toEqual(["none", "auto", "named", "base", "attach"]);
      setField(form, "where", "attach");
      expect(form.querySelector<HTMLSelectElement>('[data-role="account"]')!.disabled).toBe(true);
      setField(form, "name", "cca");
      await flush();
      const last = seen.filter((c) => c.cmd === "aliases_from_form").at(-1)!.args as { form: AliasForm };
      expect(last.form.tmux).toBe("attach");
      expect(form.querySelector('[data-role="would-run"]')!.textContent).toContain(`LINE ${enc(last.form).join(" ")}`);
    } else {
      expect(enabled).toEqual(["none"]);
    }
  });

  if (plat === "powershell") {
    it("PowerShell：握手数 ＋ 自动打开 monitor ＋ 用户级 PATH 收在「终端接入」里，展开才建、只建一份；没有「同时装 cc 函数」那一问", async () => {
      const el = await mount();
      expect(el.querySelector(".ccm-user-path-block"), "还没展开就建了").toBeNull();
      await open(el);
      const access = el.querySelector<HTMLElement>('[data-role="access"]')!;
      expect(access.querySelector(".ccm-user-path-block")).toBeTruthy();
      expect(access.querySelector(".settings-cc-stat-value")!.textContent, "握手数另问 monitor").toBe("2");
      el.open = false;
      el.dispatchEvent(new Event("toggle"));
      await open(el);
      expect(el.querySelectorAll(".ccm-user-path-block").length).toBe(1);
      expect(el.querySelectorAll(".settings-cc-autolaunch").length).toBe(1);
      expect(el.textContent).not.toContain("同时装 cc 函数");
    });

    it("读「自动打开 monitor」失败 ⇒ 复选框禁用、路径那格说读不到（原因原样）", async () => {
      autoLaunchFail = "boom-autolaunch";
      const el = await mount();
      await open(el);
      expect(el.querySelector<HTMLInputElement>(".settings-cc-autolaunch input[type=checkbox]")!.disabled).toBe(true);
      expect(el.querySelector(".settings-cc-autolaunch-path-value")!.textContent).toContain("boom-autolaunch");
    });

    const pol = (effective: string, loads: boolean | null, groupPolicy = false): ExecPolicy => ({
      host: "powershell",
      effective,
      loads,
      groupPolicy,
      error: null,
    });
    const allowBtn = (el: HTMLElement): HTMLButtonElement | undefined =>
      [...el.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "允许运行本机脚本");

    it("执行策略挡住块：接入后也说「不会加载」；按钮先确认，拒了一发都不发，确认了才发、按那一代发", async () => {
      policyA = pol("Restricted", false);
      policyAfter = pol("RemoteSigned", true);
      const asked: string[] = [];
      let answer = false;
      const el = await mount((msg) => {
        asked.push(msg);
        return answer;
      });
      await open(el);
      const access = el.querySelector<HTMLElement>('[data-role="access"]')!;
      expect(access.textContent).toContain("Restricted");
      clickText(access, "接入 /h/rc-a");
      await flush();
      expect(access.textContent).toContain("不会加载它");
      expect(access.textContent).not.toContain("装好了");
      allowBtn(el)!.click();
      await flush();
      expect(asked.length).toBe(1);
      expect(seen.filter((c) => c.cmd === "powershell_policy_set"), "没确认就改了").toEqual([]);
      answer = true;
      allowBtn(el)!.click();
      await flush();
      expect(seen.filter((c) => c.cmd === "powershell_policy_set").map((c) => c.args)).toEqual([{ origin: "<local>", host: "powershell" }]);
      expect(access.textContent).toContain("改好了");
      expect(allowBtn(el)!.hidden).toBe(true);
    });
  } else {
    it("POSIX：没有用户级 PATH 那一格，也没有 PowerShell 那几格", async () => {
      const el = await mount();
      await open(el);
      expect(el.querySelector(".ccm-user-path-block")).toBeNull();
      expect(el.textContent).not.toContain("PowerShell 集成");
    });
  }

  it("远端卡是同一个组件：每一发都带那台的 origin，只本机的那几格不挂", async () => {
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
    await open(el);
    expect(seen.map((c) => c.cmd).sort()).toEqual(["aliases_read", "aliases_render"]);
    clickText(el.querySelector('[data-role="access"]')!, "接入 /h/rc-a");
    await flush();
    clickText(el.querySelector('[data-role="access"]')!, "卸载 ccm");
    await flush();
    clickText(el, "我自己贴");
    await flush();
    clickText(el.querySelector('[data-role="group-accounts"]')!, "加上");
    await flush();
    el.querySelector<HTMLElement>(".machine-aliases-row")!.click();
    await flush();
    const sent = seen.filter((c) => c.cmd.startsWith("aliases_") || c.cmd === "chan:ccm-print");
    expect(new Set(sent.map((c) => c.cmd))).toEqual(
      new Set(["aliases_read", "aliases_render", "aliases_install", "aliases_block_render", "aliases_block_install", "aliases_block_remove", "chan:ccm-print"]),
    );
    expect(sent.filter((c) => (c.args as { origin?: string }).origin !== "devbox"), "有一发没带那台的 origin").toEqual([]);
    expect(seen.filter((c) => !sent.includes(c)), "远端卡问了只有本机才答得了的事").toEqual([]);
    expect(done).toEqual(["install:ok", "remove:ok"]);
    expect(el.querySelector(".ccm-user-path-block")).toBeNull();
    expect([...el.querySelectorAll("button")].map((b) => b.textContent)).not.toContain("打开这份文件");
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
    expect(block.textContent).toContain("认不出这台的系统，没法生成别名块");
    expect([...block.querySelectorAll("button")].map((b) => [b.textContent, b.disabled])).toEqual([["接入", true]]);
  });
});
