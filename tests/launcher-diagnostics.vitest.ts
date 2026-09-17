// F08：越层启动器诊断 + 别名生成器——纯函数单测。只诊断+引导，本文件也锁死"不代改配置"
// 这条边界（`diagnoseRemoteLauncher` 只返回文案，从不修改输入）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  diagnoseRemoteLauncher,
  buildAliasLine,
  suggestAliasName,
} from "../src/launcher-diagnostics";

describe("diagnoseRemoteLauncher", () => {
  it("空/纯空白 → 不诊断（走默认 claude，不算绕过）", () => {
    expect(diagnoseRemoteLauncher("")).toBeNull();
    expect(diagnoseRemoteLauncher("   ")).toBeNull();
  });
  it("裸 claude → 不诊断（显式选基座，不是旧式包装）", () => {
    expect(diagnoseRemoteLauncher("claude")).toBeNull();
    expect(diagnoseRemoteLauncher("  claude  ")).toBeNull();
  });
  it("命令本身含 ccm → 不诊断（已经在用统一 CLI，可能是自定义包装）", () => {
    expect(diagnoseRemoteLauncher("ccm")).toBeNull();
    expect(diagnoseRemoteLauncher("ccm --tmux --account z")).toBeNull();
    expect(diagnoseRemoteLauncher("my-ccm-wrapper")).toBeNull();
  });
  // Phase D 审计（建议项修复）：连写形式（前后无分隔符）也要命中"含 ccm 子串"——早期实现用
  // `\bccm\b`（词边界），对这种连写形式不匹配，与本函数自己"含 ccm 子串即可"的语义不一致。
  it("连写形式（无分隔符）也算含 ccm 子串 → 不诊断", () => {
    expect(diagnoseRemoteLauncher("myccmwrapper")).toBeNull();
  });
  it("旧式绕过命令（cct/oot 这类）→ 命中诊断", () => {
    expect(diagnoseRemoteLauncher("cct")).not.toBeNull();
    expect(diagnoseRemoteLauncher("oot")).not.toBeNull();
    expect(diagnoseRemoteLauncher("zcct")).not.toBeNull();
    expect(diagnoseRemoteLauncher("bcct")).not.toBeNull();
  });
  it("任意不含 ccm 且非 claude 的自定义命令 → 命中诊断（不局限于已知旧命令名单）", () => {
    expect(diagnoseRemoteLauncher("my-custom-launcher")).not.toBeNull();
  });
  it("诊断文案是只读提示，不含任何会被误当成命令/配置的内容，且指向生成器（Phase D 审计：两个 UI 曾互不指涉）", () => {
    const msg = diagnoseRemoteLauncher("cct");
    expect(msg).toContain("ccm");
    expect(msg).toContain("账号/模型偏好");
    expect(msg).toContain("生成器");
  });
});

describe("buildAliasLine", () => {
  it("无名字 → 提示先填名字，不生成半成品", () => {
    expect(buildAliasLine("", {})).toBe("（先填个别名名字）");
    expect(buildAliasLine("   ", {})).toBe("（先填个别名名字）");
  });
  // Phase D 审计（阻塞项修复）：名字含非法字符会拼出语法错误的 shell 代码，已实测复现
  // （`bash -n <<< 'my alias() { ccm "$@"; }'` 真的报语法错误）——补齐校验。
  it("名字含真会拼出语法错误的字符（空格/分号/括号等）→ 拒绝生成，给出可读的错误提示", () => {
    for (const bad of ["my alias", "a;b", "a()", "1abc"]) {
      const out = buildAliasLine(bad, {});
      expect(out.startsWith("（")).toBe(true);
    }
  });
  // bash 函数名实际允许 -/. 这类字符（`bash -n <<< 'a-b() { :; }'` 不报错）——校验刻意比
  // 严格必要更保守（只放行字母/数字/下划线），换取更简单、更容易审查正确性的规则，不是 bug。
  it("名字含 bash 实际允许但本校验刻意更保守拒绝的字符（-/.）→ 同样拒绝（安全余量，非 bug）", () => {
    expect(buildAliasLine("a-b", {}).startsWith("（")).toBe(true);
    expect(buildAliasLine("a.b", {}).startsWith("（")).toBe(true);
  });
  it("合法名字（字母/数字/下划线，不以数字开头）→ 正常生成", () => {
    expect(buildAliasLine("zcct2", {})).toBe('zcct2() { ccm "$@"; }');
    expect(buildAliasLine("_zcct", {})).toBe('_zcct() { ccm "$@"; }');
  });
  it("只有名字，无修饰 → 恒等 alias（透传 $@ 的空壳）", () => {
    expect(buildAliasLine("cch", {})).toBe('cch() { ccm "$@"; }');
  });
  it("--tmux + --account 组合", () => {
    expect(buildAliasLine("zcct", { tmux: true, account: "z" })).toBe(
      `zcct() { ccm --tmux --account 'z' "$@"; }`,
    );
  });
  it("--account 非空时优先于 --base（防御性兜底——UI 层已做主动互斥，这里仍不报错，就地择一）", () => {
    expect(buildAliasLine("x", { account: "z", base: true })).toBe(
      `x() { ccm --account 'z' "$@"; }`,
    );
  });
  it("--base 单独生效（未填 account 时）", () => {
    expect(buildAliasLine("bcc", { base: true })).toBe(
      'bcc() { ccm --base "$@"; }',
    );
  });
  it("--agent codex + --model + --launcher 全组合", () => {
    expect(
      buildAliasLine("oot", {
        tmux: true,
        agent: "codex",
        model: "opus",
        launcher: "mycc",
      }),
    ).toBe(
      `oot() { ccm --tmux --agent codex --model 'opus' --launcher 'mycc' "$@"; }`,
    );
  });
  it("account/model/launcher 里的单引号被正确转义（防生成出语法错误的 shell 代码）", () => {
    expect(buildAliasLine("x", { account: "a'b" })).toBe(
      `x() { ccm --account 'a'\\''b' "$@"; }`,
    );
  });
  it("前后空白被 trim（用户不小心多打的空格不影响生成结果）", () => {
    expect(buildAliasLine("  zcc  ", { account: "  z  " })).toBe(
      `zcc() { ccm --account 'z' "$@"; }`,
    );
  });
});

// ===== T03 迁移等价：别名生成器改走 `buildPasteBlock` 之后，原有的门必须还在 =====
//
// 迁移最容易悄悄丢的就是这类"审计当初专门加上的门"。F08 Phase D 审计的原始发现：
// 名字为空/非法时输出是**中文提示文案**而不是可执行代码，当"生成成功"一样复制出去、
// 粘进 `.bashrc` 会造成语法错误。所以这里逐条钉住迁移后它仍然拦得住。
describe("T03 迁移后：别名生成器的门与三句话", () => {
  let toasts: unknown[][];
  let writeText: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    toasts = [];
    vi.resetModules();
    vi.doMock("../src/error-toast", () => ({
      showActionFailureToast: (...a: unknown[]) => {
        toasts.push(a);
      },
    }));
    writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
  });

  async function mount(): Promise<HTMLElement> {
    const mod = await import("../src/launcher-diagnostics");
    const el = mod.buildAliasGeneratorSection();
    document.body.appendChild(el);
    return el;
  }

  it("名字为空 → 拒绝复制，且**绝不碰剪贴板**", async () => {
    const el = await mount();
    el.querySelector<HTMLButtonElement>(".paste-block-copy")!.click();
    expect(writeText).not.toHaveBeenCalled();
    expect(toasts.at(-1)?.[0]).toBe("还不能贴");
  });

  it("名字非法（含连字符）→ 同样拒绝", async () => {
    const el = await mount();
    const name = el.querySelector<HTMLInputElement>('input[type="text"]')!;
    name.value = "my-alias";
    name.dispatchEvent(new Event("input"));
    expect(
      el.querySelector<HTMLInputElement>(".paste-block-out")!.value,
    ).toContain("只能用字母/数字/下划线");
    el.querySelector<HTMLButtonElement>(".paste-block-copy")!.click();
    expect(writeText).not.toHaveBeenCalled();
  });

  it("名字合法 → 输出真代码，复制放行", async () => {
    const el = await mount();
    const name = el.querySelector<HTMLInputElement>('input[type="text"]')!;
    name.value = "zcct";
    name.dispatchEvent(new Event("input"));
    const out = el.querySelector<HTMLInputElement>(".paste-block-out")!;
    expect(out.value).toBe('zcct() { ccm "$@"; }');
    el.querySelector<HTMLButtonElement>(".paste-block-copy")!.click();
    expect(writeText).toHaveBeenCalledWith('zcct() { ccm "$@"; }');
  });

  it("表单变化会重新求值（迁移前是 regen，迁移后是 paste.refresh）", async () => {
    const el = await mount();
    const name = el.querySelector<HTMLInputElement>('input[type="text"]')!;
    name.value = "zcct";
    name.dispatchEvent(new Event("input"));
    const tmux = el.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    tmux.checked = true;
    tmux.dispatchEvent(new Event("change"));
    expect(el.querySelector<HTMLInputElement>(".paste-block-out")!.value).toBe(
      'zcct() { ccm --tmux "$@"; }',
    );
  });

  it("三句话上屏：贴到哪 / 怎么合并 / 怎样才生效", async () => {
    const el = await mount();
    expect(el.querySelector(".paste-block-target")?.textContent).toContain(
      ".bashrc",
    );
    expect(el.querySelector(".paste-block-merge")?.textContent).toBeTruthy();
    // 迁移前这句只在复制成功的 toast 里出现，现在常驻屏幕
    expect(el.querySelector(".paste-block-activation")?.textContent).toContain(
      "新终端",
    );
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// `K-R49`：**加了账号，那条命令也该跟着有**
// ═════════════════════════════════════════════════════════════════════════════

describe("K-R49 suggestAliasName", () => {
  it("账号名 → `<名>cc`（与 `cc-acct-iso shellinit` 那一族逐字同形）", () => {
    expect(suggestAliasName("z")).toBe("zcc");
    expect(suggestAliasName("b")).toBe("bcc");
    expect(suggestAliasName("work")).toBe("workcc");
  });
  it("非法字符**丢掉**而不是换成下划线 —— 换成下划线会让 `a.b` 与 `a_b` 撞成同一个名字", () => {
    expect(suggestAliasName("a.b")).toBe("abcc");
    expect(suggestAliasName("a_b")).toBe("a_bcc");
    expect(suggestAliasName("a.b")).not.toBe(suggestAliasName("a_b"));
  });
  it("数字打头 → 前缀 `_`（shell 函数名不许数字打头，而账号 0 正是那一形）", () => {
    expect(suggestAliasName("0")).toBe("_0cc");
    // 生成出来的名字必须**自己过得了** `buildAliasLine` 那道校验，否则界面上会出现
    // 一条永远写不进去的命令。
    expect(buildAliasLine(suggestAliasName("0"), { account: "0" })).toBe(
      "_0cc() { ccm --account '0' \"$@\"; }",
    );
  });
  it("整个名字都是非法字符 → 空串（调用方据此把它整条滤掉，而不是生成一个坏名字）", () => {
    expect(suggestAliasName("...")).toBe("");
    expect(suggestAliasName("")).toBe("");
  });
});

describe("K-R49 buildAccountAliasBlock：每个账号一条命令，而且真落盘", () => {
  let calls: { lines: string[]; rcPath: string | null; dryRun: boolean }[];

  const report = (over: Record<string, unknown> = {}) => ({
    aliasPath: "/h/.cc-monitor/account-aliases.sh",
    names: ["zcc", "bcc"],
    collisions: [],
    rcCandidates: [{ path: "/h/.bashrc", sourced: false }],
    wroteAliasFile: false,
    aliasFileUnchanged: false,
    wroteRc: false,
    notes: ["这是预览：盘上一个字节都没动。"],
    ...over,
  });

  beforeEach(async () => {
    calls = [];
    vi.resetModules();
    vi.doMock("../src/ipc/commands", () => ({
      commands: {
        write_account_aliases: (a: {
          lines: string[];
          rcPath: string | null;
          dryRun: boolean;
        }) => {
          calls.push(a);
          return Promise.resolve(
            report(a.dryRun ? {} : { wroteAliasFile: true, notes: ["已落盘"] }),
          );
        },
      },
    }));
  });

  async function mountBlock(
    names: string[] = ["z", "b"],
  ): Promise<HTMLElement> {
    const mod = await import("../src/launcher-diagnostics");
    const el = mod.buildAccountAliasBlock(async () => names);
    document.body.appendChild(el);
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    return el;
  }

  it("每个账号一行，内容来自 `buildAliasLine`（本块一个字节都不自己拼）", async () => {
    const el = await mountBlock();
    const rows = [...el.querySelectorAll(".ccm-acct-alias-row")].map(
      (r) => r.textContent,
    );
    expect(rows).toEqual([
      "zcc() { ccm --account 'z' \"$@\"; }",
      "bcc() { ccm --account 'b' \"$@\"; }",
    ]);
  });

  it("★ 账号名与命令名**不许错位**：过滤掉不合法的名字之后，剩下的仍各配各的账号", async () => {
    // `...` 生成不出名字会被滤掉；若实现先 filter 再按下标回头取账号名，
    // `y` 这条就会拿到 `x` 的账号 —— 两行看起来都对，而其中一条指向了别的号。
    const el = await mountBlock(["x", "...", "y"]);
    const rows = [...el.querySelectorAll(".ccm-acct-alias-row")].map(
      (r) => r.textContent,
    );
    expect(rows).toEqual([
      "xcc() { ccm --account 'x' \"$@\"; }",
      "ycc() { ccm --account 'y' \"$@\"; }",
    ]);
  });

  it("挂上去就先**预览**（dryRun=true），且默认 `rcPath` 是 null —— 界面不替人选 shell 配置", async () => {
    await mountBlock();
    expect(calls).toHaveLength(1);
    expect(calls[0].dryRun).toBe(true);
    expect(calls[0].rcPath).toBeNull();
  });

  it("rc 下拉的**默认项是「不动我的 shell 配置」**，候选来自后端报的那几份", async () => {
    const el = await mountBlock();
    const sel = el.querySelector<HTMLSelectElement>(".ccm-acct-alias-rc")!;
    expect(sel.value).toBe("");
    expect(sel.options[0].textContent).toContain("不动我的 shell 配置");
    expect([...sel.options].map((o) => o.value)).toEqual(["", "/h/.bashrc"]);
  });

  it("按「写入」才真写（dryRun=false），而且带上当时选中的那份 rc", async () => {
    const el = await mountBlock();
    const sel = el.querySelector<HTMLSelectElement>(".ccm-acct-alias-rc")!;
    sel.value = "/h/.bashrc";
    el.querySelector<HTMLButtonElement>(".ccm-acct-alias-write")!.click();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    expect(calls).toHaveLength(2);
    expect(calls[1].dryRun).toBe(false);
    expect(calls[1].rcPath).toBe("/h/.bashrc");
    expect(calls[1].lines).toEqual([
      "zcc() { ccm --account 'z' \"$@\"; }",
      "bcc() { ccm --account 'b' \"$@\"; }",
    ]);
  });

  it("🔴 `§0c 问三`：名字撞了要**在屏幕上出声**（后端报的那几条一条都不许吞）", async () => {
    vi.resetModules();
    vi.doMock("../src/ipc/commands", () => ({
      commands: {
        write_account_aliases: () =>
          Promise.resolve(
            report({
              names: ["cc"],
              collisions: ["`cc`：PATH 上已经有一个同名程序（/usr/bin/cc）"],
            }),
          ),
      },
    }));
    const el = await mountBlock(["c"]);
    expect(el.querySelector(".ccm-acct-alias-out")!.textContent).toContain(
      "/usr/bin/cc",
    );
  });

  it("落盘失败**不许静默** —— 屏幕上要说出是哪一步失败、以及为什么", async () => {
    vi.resetModules();
    vi.doMock("../src/ipc/commands", () => ({
      commands: {
        write_account_aliases: () => Promise.reject(new Error("盘满了")),
      },
    }));
    const el = await mountBlock();
    expect(el.querySelector(".ccm-acct-alias-out")!.textContent).toContain(
      "盘满了",
    );
  });

  it("文案说清落点**不是** `~/.bashrc`，而且说了「整份重写」这件事", async () => {
    const el = await mountBlock();
    const hint = el.querySelector(".ccm-acct-alias-hint")!.textContent ?? "";
    expect(hint).toContain("不是你的 ~/.bashrc");
    expect(hint).toContain("整份重写");
    expect(hint).toContain("删了账号那条就没了");
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// `K-R62`：本机 POSIX 那一格 —— 装口在界面上够得着，「这几行是旧的」也说得出
// ═══════════════════════════════════════════════════════════════════════════
describe("K-R62 本机 POSIX 那一格：装 ccm 别名块 + 逐行指名旧的那几行", () => {
  /** 后端 `profile_installer::scan_profile` 的返回形状（生成物 `ProfileScan`）。 */
  const scan = (over: Record<string, unknown> = {}) => ({
    kind: "Custom",
    path: "/h/.bashrc",
    exists: true,
    has_ccm_block: false,
    ccm_block_version: null,
    conflicting_functions: [],
    manual_cleanup_hint: "",
    size_bytes: 12,
    ...over,
  });

  const aliasReport = () => ({
    aliasPath: "/h/.cc-monitor/account-aliases.sh",
    names: ["zcc"],
    collisions: [],
    rcCandidates: [{ path: "/h/.bashrc", sourced: false }],
    wroteAliasFile: false,
    aliasFileUnchanged: false,
    wroteRc: false,
    notes: [],
  });

  /** 每条用例自己决定后端怎么答；`seen` 记下真发过哪些 IPC。 */
  async function mount(opts: {
    scans?: Record<string, unknown>[];
    installErr?: string;
    seen: string[];
  }): Promise<HTMLElement> {
    const scans = opts.scans ?? [scan()];
    let n = 0;
    vi.resetModules();
    vi.doMock("../src/ipc/commands", () => ({
      commands: {
        write_account_aliases: () => Promise.resolve(aliasReport()),
        cc_integration_scan_path: (a: { path: string }) => {
          opts.seen.push(`scan:${a.path}`);
          const s = scans[Math.min(n, scans.length - 1)];
          n += 1;
          return Promise.resolve(s);
        },
        cc_integration_install: (a: { path: string; includeCcFunction: boolean }) => {
          opts.seen.push(`install:${a.path}:${a.includeCcFunction}`);
          return opts.installErr
            ? Promise.reject(new Error(opts.installErr))
            : Promise.resolve();
        },
        cc_integration_uninstall: (a: { path: string }) => {
          opts.seen.push(`uninstall:${a.path}`);
          return Promise.resolve();
        },
      },
    }));
    const mod = await import("../src/launcher-diagnostics");
    const el = mod.buildAccountAliasBlock(async () => ["z"]);
    document.body.appendChild(el);
    for (let i = 0; i < 5; i += 1) await Promise.resolve();
    return el;
  }

  const pick = async (el: HTMLElement, path: string): Promise<void> => {
    const sel = el.querySelector<HTMLSelectElement>(".ccm-acct-alias-rc")!;
    sel.value = path;
    sel.dispatchEvent(new Event("change"));
    for (let i = 0; i < 5; i += 1) await Promise.resolve();
  };

  it("★ 默认那一档**一条 IPC 都不发**：没选 rc 时整块藏着，连读都不读", async () => {
    const seen: string[] = [];
    const el = await mount({ seen });
    expect(el.querySelector<HTMLElement>(".ccm-rc-block")!.hidden).toBe(true);
    expect(seen).toEqual([]);
  });

  it("★★ 选了那份 rc 才扫，而且**扫的就是人选的那一份**（产品不猜）", async () => {
    const seen: string[] = [];
    const el = await mount({ seen });
    await pick(el, "/h/.bashrc");
    expect(seen).toEqual(["scan:/h/.bashrc"]);
    expect(el.querySelector<HTMLElement>(".ccm-rc-block")!.hidden).toBe(false);
    expect(el.querySelector(".ccm-rc-block-status")!.textContent).toContain(
      "还没有 ccm 别名块",
    );
  });

  it("★★ 「装 ccm 别名块」真的把那条 IPC 发出去了，装完重扫、状态跟着变", async () => {
    const seen: string[] = [];
    const el = await mount({
      seen,
      scans: [scan(), scan({ has_ccm_block: true })],
    });
    await pick(el, "/h/.bashrc");
    el.querySelector<HTMLButtonElement>(".ccm-rc-block-install")!.click();
    for (let i = 0; i < 8; i += 1) await Promise.resolve();
    expect(seen).toEqual([
      "scan:/h/.bashrc",
      // `includeCcFunction: false` —— POSIX 那一块的名字住在 src/shared/ccm-aliases.sh 里，
      // 不由界面这个参数说了算。
      "install:/h/.bashrc:false",
      "scan:/h/.bashrc",
    ]);
    expect(el.querySelector(".ccm-rc-block-status")!.textContent).toContain(
      "已经装了",
    );
    expect(el.querySelector<HTMLElement>(".ccm-rc-block-uninstall")!.hidden).toBe(
      false,
    );
  });

  it("★★ 「你 rc 里这几行是旧的」**原样上屏**（后端逐行指名的那段话，前端不改写）", async () => {
    const hint =
      "/h/.bashrc 里有 2 行提到 ccm，而它们都在 cc-monitor 的围栏之外。\n" +
      "  第 5 行  cc()   { ccm \"$@\"; }     ← 会赢过我们那一块\n" +
      "  第 6 行  cct()  { ccm --tmux \"$@\"; }\n";
    const seen: string[] = [];
    const el = await mount({ seen, scans: [scan({ manual_cleanup_hint: hint })] });
    await pick(el, "/h/.bashrc");
    const box = el.querySelector<HTMLElement>(".ccm-rc-block-legacy")!;
    expect(box.hidden).toBe(false);
    // 逐行等号比，不用子串：截掉半行、少一行，子串比法照样绿。
    expect((box.textContent ?? "").split("\n")).toEqual(hint.split("\n"));
  });

  it("★ 没有旧行时那一块**不出现**（空的提示框比没有更吵）", async () => {
    const seen: string[] = [];
    const el = await mount({ seen });
    await pick(el, "/h/.bashrc");
    expect(el.querySelector<HTMLElement>(".ccm-rc-block-legacy")!.hidden).toBe(true);
  });

  it("★ 装失败**不许静默** —— 屏幕上要说出是哪一步、为什么", async () => {
    const seen: string[] = [];
    const el = await mount({ seen, installErr: "围栏损坏，已中止" });
    await pick(el, "/h/.bashrc");
    el.querySelector<HTMLButtonElement>(".ccm-rc-block-install")!.click();
    for (let i = 0; i < 8; i += 1) await Promise.resolve();
    const s = el.querySelector(".ccm-rc-block-status")!.textContent ?? "";
    expect(s).toContain("装失败");
    expect(s).toContain("围栏损坏，已中止");
  });

  it("★ 「卸载别名块」走的是卸那条 IPC，而且只在装了的时候露出来", async () => {
    const seen: string[] = [];
    const el = await mount({ seen, scans: [scan({ has_ccm_block: true })] });
    await pick(el, "/h/.bashrc");
    expect(el.querySelector<HTMLElement>(".ccm-rc-block-uninstall")!.hidden).toBe(
      false,
    );
    el.querySelector<HTMLButtonElement>(".ccm-rc-block-uninstall")!.click();
    for (let i = 0; i < 8; i += 1) await Promise.resolve();
    expect(seen).toEqual([
      "scan:/h/.bashrc",
      "uninstall:/h/.bashrc",
      "scan:/h/.bashrc",
    ]);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 🔴 `K-R69` / `KR69D3`：**生成出来的那句 `ccm`，指得到我们装的那一份**
// ═══════════════════════════════════════════════════════════════════════════
//
// 立件时现打：`buildAliasLine` 吐**裸 `ccm`**，靠 PATH 解析 —— 而 app **从来没有在
// 本机装过 `ccm`** ⇒ 用户贴上去之后解析到的仍是他自己那份旧的。
// 「今天靠撞运气」这件事，下面这几条就是它被判据看见的地方。

describe("K-R69 ccmInvocation：这条别名该调哪一份 ccm", () => {
  const card = (over: Record<string, unknown> = {}) => ({
    installed: true,
    version: "5",
    capabilities: ["new", "resume"],
    ...over,
  });
  const status = (over: Record<string, unknown> = {}) =>
    ({
      entry: "$HOME/.cc-monitor/bin/ccm",
      ours: card(),
      on_path: card(),
      verdict: "ours",
      message: "",
      ...over,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
    }) as any;

  it("PATH 上那个就是我们这一份 → 裸 `ccm`（没必要把路径塞进用户的 rc）", async () => {
    const { ccmInvocation } = await import("../src/launcher-diagnostics");
    expect(ccmInvocation(status({ verdict: "ours" }))).toBe("ccm");
  });

  it("🔴 PATH 上那个**不是**我们这一份 → 显式指向我们那一份，而且**留一个 `CCM` 的口子**", async () => {
    const { ccmInvocation, buildAliasLine } = await import(
      "../src/launcher-diagnostics"
    );
    const inv = ccmInvocation(status({ verdict: "not_ours" }));
    // ① 不许还是裸名 —— 那就是「靠 PATH 撞运气」原封不动。
    expect(inv).not.toBe("ccm");
    // ② 指得到我们那一份。
    expect(inv).toContain("$HOME/.cc-monitor/bin/ccm");
    // ③ ⚠ **不许写死绝对路径** —— 用户 09-11 明裁「这些命令都是可以自定义的」（`R19`）。
    //    形状上：家目录相对 ＋ 一个环境变量覆盖口。
    expect(inv).toContain("${CCM:-");
    expect(inv.startsWith("/")).toBe(false);
    // ④ 拼出来那一行仍然是合法的 shell 函数（名字校验那道门照旧在）。
    expect(buildAliasLine("zcc", { account: "z" }, inv)).toBe(
      "zcc() { \"${CCM:-$HOME/.cc-monitor/bin/ccm}\" --account 'z' \"$@\"; }",
    );
  });

  it("PATH 上根本没有 `ccm` → 同样显式指向我们那一份（否则这条命令压根跑不起来）", async () => {
    const { ccmInvocation } = await import("../src/launcher-diagnostics");
    expect(
      ccmInvocation(status({ verdict: "absent", on_path: card({ installed: false }) })),
    ).toContain("${CCM:-");
  });

  it("我们那一份没装 / 探不到 → 回落裸名，**不替用户指一条我们自己都没验过的路**", async () => {
    const { ccmInvocation } = await import("../src/launcher-diagnostics");
    expect(ccmInvocation(null)).toBe("ccm");
    expect(ccmInvocation(status({ entry: null, verdict: "undetermined" }))).toBe(
      "ccm",
    );
    expect(
      ccmInvocation(
        status({ ours: card({ installed: false }), verdict: "undetermined" }),
      ),
    ).toBe("ccm");
  });
});

describe("K-R69 buildAccountAliasBlock：那句话上屏 + 生成的命令跟着指对", () => {
  const report = () => ({
    aliasPath: "/h/.cc-monitor/account-aliases.sh",
    names: ["zcc"],
    collisions: [],
    rcCandidates: [],
    wroteAliasFile: false,
    aliasFileUnchanged: false,
    wroteRc: false,
    notes: [],
  });
  const card = (over: Record<string, unknown> = {}) => ({
    installed: true,
    version: "5",
    capabilities: ["new"],
    ...over,
  });

  async function mount(entryStatus: unknown): Promise<{
    el: HTMLElement;
    calls: { lines: string[] }[];
  }> {
    const calls: { lines: string[] }[] = [];
    vi.resetModules();
    vi.doMock("../src/ipc/commands", () => ({
      commands: {
        write_account_aliases: (a: { lines: string[] }) => {
          calls.push(a);
          return Promise.resolve(report());
        },
        local_ccm_entry_status: () => Promise.resolve(entryStatus),
      },
    }));
    const mod = await import("../src/launcher-diagnostics");
    const el = mod.buildAccountAliasBlock(async () => ["z"]);
    document.body.appendChild(el);
    for (let i = 0; i < 8; i += 1) await Promise.resolve();
    return { el, calls };
  }

  it("🔴 PATH 上那个是旧的 → 那句话**原样上屏**，而且落盘的那几行指的是我们那一份", async () => {
    const { el, calls } = await mount({
      entry: "$HOME/.cc-monitor/bin/ccm",
      ours: card({ version: "5" }),
      on_path: card({ version: "4" }),
      verdict: "not_ours",
      message: "🔴 你 PATH 上那个 `ccm` **不是** cc-monitor 装的这一份。",
    });
    // ① 那句话上屏 —— 后端逐字给，前端不改一个字。
    const said = el.querySelector<HTMLElement>(".ccm-path-ccm")!;
    expect(said.hidden).toBe(false);
    expect(said.textContent).toContain("不是");
    // ② 生成的命令跟着指对（**这才是 `KR69D3` 那一格**：说出来还不够，得指得到）。
    const rows = [...el.querySelectorAll(".ccm-acct-alias-row")].map(
      (r) => r.textContent,
    );
    expect(rows).toEqual([
      "zcc() { \"${CCM:-$HOME/.cc-monitor/bin/ccm}\" --account 'z' \"$@\"; }",
    ]);
    // ③ 真落盘的那几行与屏幕上是同一批（不许屏幕一套、写盘一套）。
    expect(calls[0].lines).toEqual(rows);
  });

  it("PATH 上那个就是我们这一份 → **一句话都不说**，命令也保持裸名（别没事吓用户）", async () => {
    const { el } = await mount({
      entry: "$HOME/.cc-monitor/bin/ccm",
      ours: card(),
      on_path: card(),
      verdict: "ours",
      message: "",
    });
    expect(el.querySelector<HTMLElement>(".ccm-path-ccm")!.hidden).toBe(true);
    expect(
      [...el.querySelectorAll(".ccm-acct-alias-row")].map((r) => r.textContent),
    ).toEqual(["zcc() { ccm --account 'z' \"$@\"; }"]);
  });

  it("问不出这一格 → **不许静默**：屏幕上要说「问不到」，命令回落裸名", async () => {
    const calls: unknown[] = [];
    vi.resetModules();
    vi.doMock("../src/ipc/commands", () => ({
      commands: {
        write_account_aliases: (a: unknown) => {
          calls.push(a);
          return Promise.resolve(report());
        },
        local_ccm_entry_status: () => Promise.reject(new Error("后端没起来")),
      },
    }));
    const mod = await import("../src/launcher-diagnostics");
    const el = mod.buildAccountAliasBlock(async () => ["z"]);
    document.body.appendChild(el);
    for (let i = 0; i < 8; i += 1) await Promise.resolve();
    const said = el.querySelector<HTMLElement>(".ccm-path-ccm")!;
    expect(said.hidden).toBe(false);
    expect(said.textContent).toContain("问不到本机 ccm");
    expect(
      [...el.querySelectorAll(".ccm-acct-alias-row")].map((r) => r.textContent),
    ).toEqual(["zcc() { ccm --account 'z' \"$@\"; }"]);
  });
});
