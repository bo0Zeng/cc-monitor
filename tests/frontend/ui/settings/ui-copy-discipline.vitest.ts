/**
 * 🔴 那条通用纪律的判据（第二刀 · 步 8）＋ `§8` 判据 **#5**（界面上零 markdown 标记）。
 *
 * 纪律逐字：
 * > **后端返回的字符串，凡是会直接进界面的，都不许包含：
 * > markdown 标记 · 文件路径以外的源码住址 · 设计论证 · 「下一步」处方 · 日志行格式。**
 *
 * # 这一条判的是**前端这一侧**，而且**不冒充**判了后端那一侧
 *
 * 纪律的主语是「后端返回的字符串」，而**同一条形状前端自己也犯**：
 * 第二刀那一行点名的六条现打实例里，有四条在 `src/frontend/ui/settings/` 里
 *（`diagnostics-section.ts` 的源码住址与三处内部标识符 · `data-section.ts` 的文案嵌 HTML ·
 * `config-surface-section.ts` 的设计承诺当文案与欠账当文案）。本条盯的就是它们。
 *
 * 🔴 **〔射程 · 说清它盖不到什么〕**
 * - **盖不到后端产的那一半**：`**下一步：…**`（`src/frontend/shell/src/backend_policy.rs::death_copy`）
 *   与整条 `ledger_line` 是**运行期**才拼出来的，jsdom 里没有真后端 ⇒ 这把尺子看不见它们。
 *   那是 **第二刀 步 7** 的活，住 `src/frontend/shell/`（本轮写区之外）。
 *   它们登记在下面的 `BACKEND_SIDE_DEBT` 里 —— **登记不等于判了**，写出来是为了
 *   「没提」不被读成「治好了」。
 * - **盖不到运行期才灌进来的后端字符串**：`data_paths.rs` 那条带 `sid` / `HWND` 的说明
 *   （差项 4）是后端给的数据，本条扫的是**前端源码里写死的那些句子**
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

vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy(
    {},
    {
      get: (_t, name: string) => (args?: unknown) => {
        ipc.calls.push(name);
        // 〔末〕默认一律 reject（各块自有 catch）；给了答复的命令照答复回 ——
        //   原来这里**只有** reject ⇒ 足迹那张表一行都不渲染 ⇒ `describeUndo` / `summarizeOwedInstallers`
        //   的输出**从来没上过被扫的 DOM**（「判据不在执行链上就等于不存在」的一个活例）。
        // 答复可以是函数（按入参答：足迹经通道问，`chan_call` 要看 op 与载荷）。
        if (ipc.replies.has(name)) {
          const r = ipc.replies.get(name);
          return typeof r === "function" ? (r as (a: unknown) => Promise<unknown>)(args) : Promise.resolve(r);
        }
        return Promise.reject(new Error(`[录音机] ${name} 没有真后端`));
      },
    },
  ),
}));
vi.mock("../../../../src/frontend/ui/settings/remote-section", () => ({
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:（本机）",
  RemoteSection: class {
    headActions = (): HTMLElement[] => [];
    pageIdOfMachine = (): string | null => null;
    menuFor = (): unknown[] => [];
    metaOfPage = (): string | null => null;
    setConnected = (): void => {};
    originOfPage = (): string | null => null;
    isUnconfiguredPage = (): boolean => false;
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
vi.mock("../../../../src/frontend/ui/settings/accounts-section", () => ({
  AccountsSection: class { element = document.createElement("div"); },
}));
vi.mock("../../../../src/frontend/ui/settings/mcp-section", () => ({
  McpSection: class { element = document.createElement("div"); },
}));
vi.mock("../../../../src/frontend/ui/settings/plugins-section", () => ({
  PluginsSection: class { element = document.createElement("div"); },
}));
// `cc_integration.ts` 并进了 `machine-aliases.ts`（终端集成成了「别名」那一块 PowerShell 那一侧），它的替身随之删掉。
vi.mock("../../../../src/frontend/ui/keybindings/editor", () => ({
  KeybindingsEditor: class { element = document.createElement("div"); },
}));
vi.mock("../../../../src/frontend/ui/keybindings/registry", () => ({
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

import { SettingsPanel } from "../../../../src/frontend/ui/settings/panel";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { __setHostOsForTests } from "../../../../src/frontend/ui/settings/host-os";
import { __resetMachineContextForTests } from "../../../../src/frontend/ui/settings/machine-context";

/** 那五种形状，一条规则一个名字（红的时候要说得出是哪一种）。 */
const SHAPES: ReadonlyArray<{ name: string; re: RegExp; why: string }> = [
  {
    name: "markdown 标记",
    // `**粗体**`。⚠ 只认成对的，单个星号（`*.log` 之类）不算。
    re: /\*\*[^*\n]+\*\*/g,
    why: "界面不渲染 markdown ⇒ 星号会连着一起显示给用户看",
  },
  {
    name: "源码住址",
    // `foo.rs` / `bar.ts:123` / `a::b`。⚠ **刻意不认** `.json` / `.log` / `.sh`
    // ——那些是**用户自己机器上的文件路径**，纪律里写明「文件路径以外的源码住址」。
    re: /[\w/-]+\.(?:rs|ts|tsx|mts|mjs)\b|[A-Za-z_]\w*::[A-Za-z_]\w*|\b\w+_subsystem\s*=/g,
    why: "用户不需要知道这件事发生在我们哪个文件的第几行",
  },
  {
    name: "内部标识符",
    re: /\btracing\b|\bts-rs\b|\bserde\b|\bOnceCell\b|\bspawn_blocking\b/g,
    why: "我们这一侧的词——用户不知道我们用的是哪个库",
  },
  {
    name: "日志行格式",
    re: /\[死亡账\]|\borigin=|\b判定=|\b退出状态=/g,
    why: "`ledger_line` 自己的注释就写着「落点是 monitor 自己的滚动日志」",
  },
  {
    name: "设计论证 / 下一步处方",
    re: /下一步：|放大器|本条不推翻|如实登记|判不了/g,
    why: "写给开发文档看的论证，不该出现在设置面板上",
  },
  {
    // 用户 09-29「所有文案…不能把开发过程混进去」⇒ 「还没有安装入口 / 暂未提供 / 今天还没有」也归这一形。
    // 〔末那条射程缺口〕五种形状里原来**没有这一种** ⇒ 足迹那两句
    //   「该由 cc-monitor 自带、而安装入口还没写」这把尺子一条都逮不到。
    name: "欠账当产品文案",
    re: /该由 cc-monitor 自带|入口还没写|还没写|我们欠|还没有安装|暂未提供|今天还没有/g,
    why: "把我们还没做完的实现写成给用户看的话（/ #2）——说现状与你能做什么（「这项不能在这里装」），不说「还没有 / 今天 / 以后」",
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
  // `backend_policy.rs::death_copy` / `::ledger_line` 两条**还清了**：
  //   death_copy 不再产 markdown / 论证；界面上「最后一次」接的是 `last_brief`（判定 ＋ 退出状态），
  //   账行只落日志。判据在 Rust 那侧：`backend_policy_tests.rs::what_reaches_the_settings_panel_carries_no_markdown_no_argument_no_log_format`。
  // `data_paths.rs` 那条（条目说明里的 `sid` / `HWND`）**还清了**，
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

describe(" 文案纪律 ＋ `§8` #5：界面上零 markdown / 零源码住址", () => {
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
    // 各页 + 本机子页都走一遍 —— 只看落地页等于只判了一部分。
    for (const id of ["general", "appearance", "logs", "data", "machine:（本机）", "machines"]) {
      const btn = document.querySelector<HTMLButtonElement>(
        `[id="settings-tab-${id}"]`,
      );
      btn?.click();
      await tick();
    }
    // 文件与数据那一页（两栏）—— 走过它就在被扫的 DOM 里。
    expect(document.querySelector(".data-page"), "文件与数据那一页没上屏").not.toBeNull();
    const root = document.querySelector<HTMLElement>(".settings-panel")!;
    const copy = visibleCopy(root);
    // 量具自检：扫到的文字量要够大。零字节时下面那条「一条都不许命中」是空转。
    // 下限 1500：「直接敲的也走中转」那一大块搬进「要你动手」（要读回成品才上屏）、上下文上限那段长说明退场之后，静态面板约 1800 字。
    expect(copy.length, "面板上一个字都没扫到 ⇒ 本条在空转").toBeGreaterThan(1500);
    const bad = violationsOf(copy);
    expect(
      bad.map((v) => `${v.shape}: ${v.hit}`).sort(),
      `设置面板上出现了 ${bad.length} 处 \`\` 禁的形状。\n` +
        SHAPES.map((s) => `  · ${s.name} —— ${s.why}`).join("\n"),
    ).toEqual([]);
  });

  const CHORE = { id: "", kind: "optional", state: "todo", name: "", loc: "", said: "", why: "", steps: [], diff: [], copy: null, whole: null, wholeCovers: [], file: null, go: null, howUrl: null, mask: null, action: "copySnippet" };

  it("〔ST2〕文件与数据**喂一份真成品**再扫：要你动手各类 ＋ 改过你的文件都上屏，一条都不许命中", async () => {
    // 文件与数据经通道问本机后端（`data-report`，一问）；monitor 自己进程的那几条事实问 `footprint_client_facts`。
    const report = {
      home: "/h",
      changedFiles: [{ path: "~/.bashrc", what: "ccm 命令入口", undo: { page: "machine", tab: "config", anchor: "connect-terminal" } }],
      todo: [
        { ...CHORE, id: "install:claude-cli", kind: "install", name: "Claude Code", loc: "claude", said: copyText("dataPage.chores.countInstall", { n: 1 }), action: "how", howUrl: "https://example.invalid/claude" },
        { ...CHORE, id: "relay", name: "实时显示", loc: "~/.claude/settings.json · env", said: "当前：写入记录后才显示", why: "慢一拍", steps: ["打开 ~/.claude/settings.json"], diff: [{ n: 3, op: "same", text: '  "env": {' }, { n: null, op: "add", text: '    "K": "v",' }], copy: '"K": "v"', whole: "{}", wholeCovers: ["relay"], file: "/h/.claude/settings.json" },
      ],
      tmux: false,
      chores: 1,
      own: [],
    };
    ipc.replies.set("footprint_client_facts", { home: "/h", path: null });
    ipc.replies.set("chan_call", (a: unknown) => {
      const { op } = a as { op: string };
      if (op !== "data-report") return Promise.reject(new Error(`[录音机] ${op} 没有真后端`));
      return Promise.resolve(new TextEncoder().encode(JSON.stringify(report)).buffer);
    });
    try {
      const p = new SettingsPanel({ windowMode: true });
      await p.open();
      await tick();
      document.querySelector<HTMLButtonElement>('[id="settings-tab-data"]')?.click();
      await tick();
      await tick();
      const root = document.querySelector<HTMLElement>(".data-page")!;
      expect(root.querySelectorAll("[data-chore]").length, "要你动手那几件一件都没上屏 ⇒ 下面的零命中是空转").toBe(2);
      root.querySelector<HTMLButtonElement>('[data-chore="relay"] [aria-expanded]')!.click();
      // 反空真：类的标签与展开后的几段**真的在**被扫的文字里（按文案键取，不钉原文）。
      expect(visibleCopy(root)).toContain(copyText("dataPage.install.kind"));
      expect(visibleCopy(root)).toContain(copyText("dataPage.chore.autoDetect"));
      expect(violationsOf(visibleCopy(root)).map((v) => `${v.shape}: ${v.hit}`)).toEqual([]);
    } finally {
      ipc.replies.clear();
    }
  });

  it("后端那一侧的欠账**登记在案**（本条判不了，别把「没提」读成「治好了」）", () => {
    // 这一格不是断言代码，是断言**我们没有假装那几条已经没了**。
    // 它会在有人把登记清空时红 —— 那时要么债真还了（去 `src/frontend/shell/` 核过再删），
    // 要么是有人把不方便的话删掉了。
    // 三条全还清 ⇒ 0。再有人往这里登记，就是又欠了一笔（要写清住址与理由）。
    expect(Object.keys(BACKEND_SIDE_DEBT).length).toBe(0);
    for (const [addr, why] of Object.entries(BACKEND_SIDE_DEBT)) {
      expect(addr.startsWith("src/frontend/shell/"), `${addr} 不在后端那一侧，登记错地方了`).toBe(true);
      expect(why.length, `${addr} 的理由是空的`).toBeGreaterThan(10);
    }
  });
});
