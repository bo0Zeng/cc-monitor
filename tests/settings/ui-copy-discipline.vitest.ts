/**
 * 🔴 `设计/70 §2.4` 那条通用纪律的判据（第二刀 · 步 8）＋ `§8` 判据 **#5**（界面上零 markdown 标记）。
 *
 * 纪律逐字：
 * > **后端返回的字符串，凡是会直接进界面的，都不许包含：
 * > markdown 标记 · 文件路径以外的源码住址 · 设计论证 · 「下一步」处方 · 日志行格式。**
 *
 * # 这一条判的是**前端这一侧**，而且**不冒充**判了后端那一侧
 *
 * 纪律的主语是「后端返回的字符串」，而**同一条形状前端自己也犯**：
 * `70 §10.4` 第二刀那一行点名的六条现打实例里，有四条在 `src/settings/` 里
 *（`diagnostics-section.ts` 的源码住址与三处内部标识符 · `data-section.ts` 的文案嵌 HTML ·
 * `config-surface-section.ts` 的设计承诺当文案与欠账当文案）。本条盯的就是它们。
 *
 * 🔴 **〔射程 · 说清它盖不到什么〕**
 * - **盖不到后端产的那一半**：`**下一步：…**`（`src/frontend/shell/src/backend_policy.rs::death_copy`）
 *   与整条 `ledger_line` 是**运行期**才拼出来的，jsdom 里没有真后端 ⇒ 这把尺子看不见它们。
 *   那是 `70 §7` **第二刀 步 7** 的活，住 `src/frontend/shell/`（本轮写区之外）。
 *   它们登记在下面的 `BACKEND_SIDE_DEBT` 里 —— **登记不等于判了**，写出来是为了
 *   「没提」不被读成「治好了」。
 * - **盖不到运行期才灌进来的后端字符串**：`data_paths.rs` 那条带 `sid` / `HWND` 的说明
 *   （`70 §10.2` 差项 4）是后端给的数据，本条扫的是**前端源码里写死的那些句子**
 *   ＋ **真渲染出来的 DOM**（而 DOM 里那部分今天是 mock 出来的）。
 * - **盖不到「文案写得好不好」**：它只认那五种**形状**。
 *
 * # 反空真
 *
 * 两道：① **正控** —— 拿一段合成文本喂给同一个 `violationsOf()`，五种形状必须一条不落地被逮到；
 * ② **量具自检** —— 真扫出来的文本量要够大（零字节时上面每一条「零命中」都是空转）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { ipc } = vi.hoisted(() => ({
  ipc: { calls: [] as string[], replies: new Map<string, unknown>() },
}));

vi.mock("../../src/ipc/commands", () => ({
  commands: new Proxy(
    {},
    {
      get: (_t, name: string) => (args?: unknown) => {
        ipc.calls.push(name);
        // 〔ST2 · `70 §11.4` 末〕默认一律 reject（各块自有 catch）；给了答复的命令照答复回 ——
        //   原来这里**只有** reject ⇒ 足迹那张表一行都不渲染 ⇒ `describeUndo` / `summarizeOwedInstallers`
        //   的输出**从来没上过被扫的 DOM**（「判据不在执行链上就等于不存在」的一个活例）。
        // 〔MIG-3b 续〕答复可以是函数（按入参答：足迹经通道问，`chan_call` 要看 op 与载荷）。
        if (ipc.replies.has(name)) {
          const r = ipc.replies.get(name);
          return typeof r === "function" ? (r as (a: unknown) => Promise<unknown>)(args) : Promise.resolve(r);
        }
        return Promise.reject(new Error(`[录音机] ${name} 没有真后端`));
      },
    },
  ),
}));
vi.mock("../../src/settings/remote-section", () => ({
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:（本机）",
  RemoteSection: class {
    element = document.createElement("div");
    refresh = vi.fn().mockResolvedValue(undefined);
    constructor(opts?: {
      pages?: { addMachinePage: (id: string, t: string, el: HTMLElement) => void };
    }) {
      setTimeout(() => {
        opts?.pages?.addMachinePage(
          "machine:（本机）",
          "本机",
          document.createElement("div"),
        );
      }, 0);
    }
  },
}));
vi.mock("../../src/settings/accounts-section", () => ({
  AccountsSection: class { element = document.createElement("div"); },
}));
vi.mock("../../src/settings/mcp-section", () => ({
  McpSection: class { element = document.createElement("div"); },
}));
vi.mock("../../src/settings/plugins-section", () => ({
  PluginsSection: class { element = document.createElement("div"); },
}));
vi.mock("../../src/settings/cc-bus-hooks-section", () => ({
  CcBusHooksSection: class { element = document.createElement("div"); },
}));
// 〔AL1c · 4B〕`cc_integration.ts` 并进了 `machine-aliases.ts`（终端集成成了「别名」那一块 PowerShell 那一侧），它的替身随之删掉。
vi.mock("../../src/keybindings/editor", () => ({
  KeybindingsEditor: class { element = document.createElement("div"); },
}));
vi.mock("../../src/keybindings/registry", () => ({
  dispatcher: {
    pushOverlay: vi.fn(),
    popOverlay: vi.fn(),
    startRecording: vi.fn(),
    cancelRecording: vi.fn(),
    exportOverrides: vi.fn().mockReturnValue({}),
    applyOverrides: vi.fn(),
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ close: vi.fn() }) }));

import { SettingsPanel } from "../../src/settings/panel";
import { __setHostOsForTests } from "../../src/settings/host-os";
import { __resetMachineContextForTests } from "../../src/settings/machine-context";

/** `70 §2.4` 那五种形状，一条规则一个名字（红的时候要说得出是哪一种）。 */
const SHAPES: ReadonlyArray<{ name: string; re: RegExp; why: string }> = [
  {
    name: "markdown 标记",
    // `**粗体**`。⚠ 只认成对的，单个星号（`*.log` 之类）不算。
    re: /\*\*[^*\n]+\*\*/g,
    why: "界面不渲染 markdown ⇒ 星号会连着一起显示给用户看（`70 §2.1` #1）",
  },
  {
    name: "源码住址",
    // `foo.rs` / `bar.ts:123` / `a::b`。⚠ **刻意不认** `.json` / `.log` / `.sh`
    // ——那些是**用户自己机器上的文件路径**，纪律里写明「文件路径以外的源码住址」。
    re: /[\w/-]+\.(?:rs|ts|tsx|mts|mjs)\b|[A-Za-z_]\w*::[A-Za-z_]\w*|\b\w+_subsystem\s*=/g,
    why: "用户不需要知道这件事发生在我们哪个文件的第几行（`70 §2.4`）",
  },
  {
    name: "内部标识符",
    re: /\btracing\b|\bts-rs\b|\bserde\b|\bOnceCell\b|\bspawn_blocking\b/g,
    why: "我们这一侧的词（`91 §2.1` 那一族）——用户不知道我们用的是哪个库",
  },
  {
    name: "日志行格式",
    re: /\[死亡账\]|\borigin=|\b判定=|\b退出状态=/g,
    why: "`ledger_line` 自己的注释就写着「落点是 monitor 自己的滚动日志」（`70 §2.1` #3）",
  },
  {
    name: "设计论证 / 下一步处方",
    re: /下一步：|放大器|本条不推翻|如实登记|判不了/g,
    why: "写给开发文档看的论证，不该出现在设置面板上（`70 §2.1` #2）",
  },
  {
    // 〔ST2 · `70 §11.4` 末那条射程缺口〕五种形状里原来**没有这一种** ⇒ 足迹那两句
    //   「该由 cc-monitor 自带、而安装入口还没写」这把尺子一条都逮不到。
    name: "欠账当产品文案",
    re: /该由 cc-monitor 自带|入口还没写|还没写|我们欠/g,
    why: "把我们还没做完的实现写成给用户看的话（`70 §11.4` #1 / #2）——要说的是状态（「还没有安装入口」），不是谁欠谁",
  },
];

/** 一段文本犯了哪几条。**判据与正控共用同一个函数** —— 两份实现会各自漂。 */
export function violationsOf(text: string): { shape: string; hit: string }[] {
  const out: { shape: string; hit: string }[] = [];
  for (const s of SHAPES) {
    for (const m of text.matchAll(s.re)) out.push({ shape: s.name, hit: m[0] });
  }
  return out;
}

/**
 * 🔴 **后端那一侧今天还欠着的**（本条**判不了**，登记在此）。
 * 逐条：住址 → 它今天产的是哪一种形状。
 */
const BACKEND_SIDE_DEBT: Readonly<Record<string, string>> = {
  // 〔第四波 ST2 · 步 7〕`backend_policy.rs::death_copy` / `::ledger_line` 两条**还清了**：
  //   death_copy 不再产 markdown / 论证；界面上「最后一次」接的是 `last_brief`（判定 ＋ 退出状态），
  //   账行只落日志。判据在 Rust 那侧：`backend_policy_tests.rs::what_reaches_the_settings_panel_carries_no_markdown_no_argument_no_log_format`。
  // 〔第四波 ST2 · 子步 6〕`data_paths.rs` 那条（条目说明里的 `sid` / `HWND`）**还清了**，
  //   判据在 Rust 那侧：`data_paths_tests.rs::no_entry_description_speaks_our_internal_words`。
};

const tick = () => new Promise((r) => setTimeout(r, 0));

/** 面板上**用户真看得见**的全部文字：文本节点 ＋ `title` ＋ ⓘ 的 `aria-label`。 */
function visibleCopy(root: HTMLElement): string {
  const parts: string[] = [];
  const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let n = walk.nextNode(); n; n = walk.nextNode()) parts.push(n.nodeValue ?? "");
  for (const el of root.querySelectorAll<HTMLElement>("[title]")) {
    parts.push(el.getAttribute("title") ?? "");
  }
  // ⓘ 的正文住在 `aria-label` 上（tooltip 只在 hover 期间存在，见 `info-icon.ts` 头注）。
  for (const el of root.querySelectorAll<HTMLElement>("[aria-label]")) {
    parts.push(el.getAttribute("aria-label") ?? "");
  }
  for (const el of root.querySelectorAll<HTMLElement>("[placeholder]")) {
    parts.push(el.getAttribute("placeholder") ?? "");
  }
  return parts.join("\n");
}

describe("`70 §2.4` 文案纪律 ＋ `§8` #5：界面上零 markdown / 零源码住址", () => {
  beforeEach(() => {
    ipc.calls = [];
    document.body.replaceChildren();
    __resetMachineContextForTests();
    __setHostOsForTests("windows");
  });

  it("🔴 正控：五种形状，同一个 `violationsOf()` 一条不落地逮得到", () => {
    const sample =
      "崩了：**下一步：这一格才是自愈要治的那一格**，而重起归第二档 —— 判据不可信的时候重起是放大器。\n" +
      "src/frontend/shell/src/backend_policy.rs:371 里那句；monitor 是 GUI 应用（windows_subsystem=windows）。\n" +
      "所有后端 tracing 输出写到文件。\n" +
      "「[死亡账] origin=<local> 判定=崩了 退出状态=exit -1073741510」\n" +
      "这一项该由 cc-monitor 自带，而安装入口还没写。";
    const shapes = new Set(violationsOf(sample).map((v) => v.shape));
    expect([...shapes].sort()).toEqual(SHAPES.map((s) => s.name).sort());
  });

  it("🔴 反向正控：一段干净文案一条都不许命中（免得这把尺子是「见字就红」）", () => {
    const clean =
      "cc-monitor 会碰你哪些文件、对它做什么、现在什么状态、还能不能撤。\n" +
      "按天滚动写入 monitor.YYYY-MM-DD.log，保留最近 3 天。\n" +
      "项目里的 .mcp.json 得先知道是哪个项目；Windows 的 $PROFILE 由 PowerShell 决定。";
    expect(violationsOf(clean)).toEqual([]);
  });

  it("设置面板**真渲染出来**的文字：一条都不许命中", async () => {
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    await tick();
    // 两个顶层页 + 本机子页都走一遍（〔ST2〕顶层「改动足迹」已删）—— 只看落地页等于只判了一部分。
    for (const id of ["app", "app-appearance", "app-logs", "app-data", "machine:（本机）", "machines"]) {
      const btn = document.querySelector<HTMLButtonElement>(
        `[id="settings-tab-${id}"]`,
      );
      btn?.click();
      await tick();
    }
    // 〔ST2〕漂移记账在本机子页的「足迹」栏里（per-machine 那一批）—— 走过本机子页它就在被扫的 DOM 里。
    expect(document.querySelector(".drift-ledger-section"), "本机子页上没有「未识别的数据」那一块").not.toBeNull();
    const root = document.querySelector<HTMLElement>(".settings-panel")!;
    const copy = visibleCopy(root);
    // 量具自检：扫到的文字量要够大。零字节时下面那条「一条都不许命中」是空转。
    expect(copy.length, "面板上一个字都没扫到 ⇒ 本条在空转").toBeGreaterThan(2000);
    const bad = violationsOf(copy);
    expect(
      bad.map((v) => `${v.shape}: ${v.hit}`).sort(),
      `设置面板上出现了 ${bad.length} 处 \`70 §2.4\` 禁的形状。\n` +
        SHAPES.map((s) => `  · ${s.name} —— ${s.why}`).join("\n"),
    ).toEqual([]);
  });

  it("〔ST2〕足迹那张表**喂一份真 report** 再扫：四档各一行上屏，一条都不许命中", async () => {
    const row = (tier: string, name: string, state: unknown) => ({
      tool_id: name,
      tool_name: name,
      source_label: "来源",
      path_declared: `~/${name}`,
      path_resolved: `/h/${name}`,
      host_label: "本机",
      note: null,
      effect_label: "它做什么",
      state,
      installable: tier === "AppInstalls",
      uninstallable: false,
      tier,
    });
    // 〔MIG-3b 续〕足迹经通道问本机后端（`footprint-report`，一问）；monitor 自己进程的那几条事实问 `footprint_client_facts`。
    const report = {
      rows: [
        row("AppInstalls", "甲", { kind: "present", detail: "文件，1 字节" }),
        row("AppShipsNoInstallerYet", "乙", { kind: "absent" }),
        row("UserInstallsWePrompt", "丙", { kind: "undetermined", why: "查不动" }),
        row("AppOnlyChecks", "丁", { kind: "absent" }),
      ],
      settings_scopes: [],
      claude_config_dir: "/h/.claude",
      home: "/h",
    };
    ipc.replies.set("footprint_client_facts", { home: "/h", agentHome: "/h/.claude", path: null });
    ipc.replies.set("chan_call", (a: unknown) => {
      const { op, payload } = a as { op: string; payload: number[] };
      if (op !== "footprint-report") return Promise.reject(new Error(`[录音机] ${op} 没有真后端`));
      void payload;
      return Promise.resolve(new TextEncoder().encode(JSON.stringify(report)).buffer);
    });
    try {
      const p = new SettingsPanel({ windowMode: true });
      await p.open();
      await tick();
      document.querySelector<HTMLButtonElement>('[id="settings-tab-machine:（本机）"]')?.click();
      await tick();
      await tick();
      const rows = document.querySelectorAll(".config-surface-row");
      expect(rows.length, "表一行都没上屏 ⇒ 下面的零命中是空转（原来就是这样空转的）").toBe(4);
      const root = document.querySelector<HTMLElement>(".config-surface-section")!;
      // 反空真：欠账那一档的状态话**真的在**被扫的文字里。
      expect(visibleCopy(root)).toContain("还没有安装入口");
      expect(violationsOf(visibleCopy(root)).map((v) => `${v.shape}: ${v.hit}`)).toEqual([]);
    } finally {
      ipc.replies.clear();
    }
  });

  it("后端那一侧的欠账**登记在案**（本条判不了，别把「没提」读成「治好了」）", () => {
    // 这一格不是断言代码，是断言**我们没有假装那几条已经没了**。
    // 它会在有人把登记清空时红 —— 那时要么债真还了（去 `src/frontend/shell/` 核过再删），
    // 要么是有人把不方便的话删掉了。
    // 〔ST2〕三条全还清 ⇒ 0。再有人往这里登记，就是又欠了一笔（要写清住址与理由）。
    expect(Object.keys(BACKEND_SIDE_DEBT).length).toBe(0);
    for (const [addr, why] of Object.entries(BACKEND_SIDE_DEBT)) {
      expect(addr.startsWith("src/frontend/shell/"), `${addr} 不在后端那一侧，登记错地方了`).toBe(true);
      expect(why.length, `${addr} 的理由是空的`).toBeGreaterThan(10);
    }
  });
});
