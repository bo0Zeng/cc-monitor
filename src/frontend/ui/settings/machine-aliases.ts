/**
 * 机器页上的 ②「别名」—— （别名 ＝ ccm 参数附加器）＋（并入机器页）。
 *
 * # 它取代了什么
 *
 * 从前别名有两块、住在「应用 → 行为」里：一块「按账号生成命令」（加了账号就多一条、删了就没一条 ——
 * 把**用户的清单**当成了**账号表的投影**），一块「生成自定义别名」（只吐文本、不落盘）。
 * 账号表单下面还有一句散文把人指过去 ——「用一句散文告诉用户去另一个顶层页找一个功能，
 * 本身就是 IA 失败的自证」。
 *
 * 今天只有**一类**：一条别名 ＝ 名字 ＋ 一组 ccm 参数；账号只是参数里的一个维度。
 * 清单归用户，「为每个账号加一条」只是个一次性的便利按钮。
 *
 * # 两跳—— 本文件一个字节的 shell 文本都不自己拼
 *
 * ① `aliases_render`（纯）：清单 → 代码 ＋ 每条的问题 ＋ 撞名提示。预览与「复制去手贴」只调它；
 * ② `aliases_install`（唯一的副作用）：同一份渲染落进 `~/.cc-monitor/aliases.sh`（经本机后端写），
 *    选了 rc 就**只查**它接没接上（接上那一行只住别名块里，不代装）。
 * 读回口 `aliases_read`：打开这一块时先读盘上那份，清单从它开始编辑。
 *
 * # 纪律（`launcher-diagnostics.ts` 头注那条，原样适用）
 *
 * **绝不在用户没要求时改他的配置**：打开这一块只读；「写入」按钮写明写到哪、写什么；
 * 那份 rc 由人在下拉里选，默认项是「不动我的 shell 配置」。
 * **构造零 I/O**：这一块是个 `<details>`，第一次展开才发第一条 IPC。
 *
 * # 两个平台一份组件—— 平台是它的一个输入
 *
 * 从前 Windows 上这里只有一句「PowerShell 写法的别名还没做」，`cc` 那一块住另一份组件（`cc_integration.ts`，
 * 本机页上单独一块「终端集成」）。今天两份并成这一份，`platform`（`posix` / `powershell`）是入参：
 * - **同一套**：清单 · 表单 · 第①跳渲染 · 第②跳写入 · 启动文件下拉 · 读回口 —— 三条命令都带着 `shell`，
 *   写法由后端的方言那一层出（`shell_dialect.rs`），本文件照旧一个字节的 shell 文本都不拼。
 * - **差在平台那几格**：tmux 那几格在 PowerShell 上整组禁用（Windows 没有 tmux —— 能力，不是方言）；
 *   「别名块」在 POSIX 是 `cc` / `cct` 片段，在 PowerShell 是终端集成块（`__ccm_bind` ＋ 可选 `function cc`，
 *   原 `cc_integration.ts` 整块搬进来）＋ 用户级 PATH 那一格。
 */
import { commands } from "../ipc/commands";
import { chan } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "../ipc/chan-caller";
import { isLocalOrigin } from "../ipc/origin";
import { showActionFailureToast } from "../error-toast"; // `K-R135`：用户级 PATH 那一格的失败要出声
import { buildPasteBlock } from "../paste-block";
import { DEFAULT_AGENT, listAgents } from "../agent-profile";
// 别名六问走通道、那台后端出成品（`../alias-reads`）；类型随成品住那边（从前是 monitor 生成的类型）。
import type { Alias, AliasRender, ExecPolicy, PsHost, StartupFile, Shell } from "../alias-reads";
import { askConfirm, type ConfirmFn } from "../ask-dialog";
import {
  allowLocalScripts,
  installAliasBlock,
  installAliases,
  readAliases,
  removeAliasBlock,
  renderAliasBlock,
  renderAliases,
} from "../alias-reads";
import type { Origin } from "../generated/Origin";
import { openPath } from "@tauri-apps/plugin-opener";
import { makeInfoIcon } from "./info-icon";
import { hostOs } from "./host-os";
import { copyText } from "../copy-table";
import { recordFacet, LOCAL_MACHINE_KEY } from "./machine-status";
import type { LocalCcmEntry } from "../generated/LocalCcmEntry";

/** 界面上怎么叫那一代 PowerShell（两代的执行策略分开存）。 */
const psName = (h: PsHost): string =>
  h === "pwsh" ? copyText("machineAliases.policy.hostPwsh") : copyText("machineAliases.policy.hostPowershell");

/**
 * 这台机器（monitor 跑在的那台本机）用哪种 shell 的方言。
 * **认不出就不猜**（`null`）：别名块那一格明说「认不出这台的系统」、安装入口置灰
 * （[`buildUnknownOsAliasBlock`]）—— 先前按 POSIX 猜，Windows 上猜错了就把装 bash 块的入口摆给一台 PowerShell 机器。
 */
export function localShell(): Shell | null {
  const os = hostOs();
  if (os === "unknown") return null;
  return os === "windows" ? "powershell" : "posix";
}

/** 认不出本机系统时别名那一格：说清为什么没有，安装入口在、但置灰（出声不静默）。 */
export function buildUnknownOsAliasBlock(): HTMLElement {
  const wrap = el("details", "ccm-alias-gen machine-aliases");
  wrap.dataset.shell = "unknown";
  wrap.appendChild(el("summary", "", copyText("machineAliases.manager.title")));
  wrap.appendChild(el("p", "settings-hint", copyText("machineAliases.unknownOs.said")));
  const install = el("button", "settings-btn", copyText("machineAliases.rc.install"));
  install.type = "button";
  install.disabled = true;
  install.title = copyText("machineAliases.unknownOs.said");
  wrap.appendChild(install);
  return wrap;
}

/** 平台那几格的措辞（写死在一处，组件里按 `platform` 取）。 */
// 做成函数、用到时才取文：模块顶层一句取文口调用都不留 —— 顶层有调用，Rollup 就把这份（连同 paste-block / info-icon）
//   从设置窗口的入口 chunk 挪进主窗也加载的共享 chunk，主窗的样式清单就对不上了（entry-graphs 那条判据现打逮到）。
const platformCopy = (): Record<
  Shell,
  {
    pasteTarget: string;
    pasteActivation: string;
    rcLabel: string;
    nameHint: string;
    /** 别名块是什么（状态行里「还没有别名块（…）」那一格）。 */
    blockWhat: string;
    blockInstallTitle: string;
    blockAfterInstall: string;
  }
> => ({
  posix: {
    pasteTarget: copyText("machineAliases.posix.pasteTarget"),
    pasteActivation: copyText("machineAliases.posix.pasteActivation"),
    rcLabel: copyText("machineAliases.posix.rcLabel"),
    nameHint: copyText("machineAliases.posix.nameHint"),
    blockWhat: copyText("machineAliases.posix.blockWhat"),
    blockInstallTitle:
      copyText("machineAliases.posix.blockInstallTitle"),
    blockAfterInstall: copyText("machineAliases.posix.blockAfterInstall"),
  },
  powershell: {
    pasteTarget: copyText("machineAliases.powershell.pasteTarget"),
    pasteActivation: copyText("machineAliases.powershell.pasteActivation"),
    rcLabel: copyText("machineAliases.powershell.rcLabel"),
    nameHint: copyText("machineAliases.powershell.nameHint"),
    blockWhat: copyText("machineAliases.powershell.blockWhat"),
    blockInstallTitle:
      copyText("machineAliases.powershell.blockInstallTitle"),
    blockAfterInstall:
      copyText("machineAliases.powershell.blockAfterInstall"),
  },
});

/** 问一次预览（`ccm-print`）最多等多久：读一份账号库 ＋ 问一次会话快照，秒级内。 */
const PREVIEW_BUDGET_MS = 10_000;

/**
 * **一条别名实际会执行什么**（「`ccm --print` 不跑、吐出等价的一行 shell
 * ⇒ 生成器旁边显示这条别名实际会执行什么，是真验证，不是前端拼串」）。
 *
 * 问这台机器的后端（帧命令 `ccm-print`，经通道 `chan.call`）—— 与终端里 `ccm --print` 同一个计划函数；
 * 语境是「家目录里的一个新终端」（后端那一侧写死，`control/ccm/plan.rs::Env::for_preview`）。
 * 本文件一个字节的 shell 都不拼：`line` 原样上屏。点了才问（不在首开的那几发里）。
 */
// 问的是**那台**机器的后端（`origin`）：本机远端同一条帧命令。
export async function previewAlias(origin: Origin, a: Alias): Promise<string> {
  try {
    const budget = budgetWithin(PREVIEW_BUDGET_MS);
    const body = jsonBody({ args: a.args });
    const reply = await chan.call(origin, "ccm-print", body, budget);
    const got = readJson(reply) as { line?: unknown };
    return copyText("machineAliases.aliasPreview.line", {
      name: a.name,
      line: typeof got.line === "string" ? got.line : "",
    });
  } catch (e) {
    return copyText("machineAliases.aliasPreview.failed", {
      reason: saidOf(e, copyText("machineAliases.aliasPreview.tooOld")),
    });
  }
}

/** tmux 那一维的四个取值（第一档）。 */
export type TmuxMode = "none" | "auto" | "named" | "base";

/**
 * tmux 四选各自**撞名时会怎样**（表单上那四个选项只有名字，没有一句说撞了会怎样）。
 *
 * 规则不在这里：取名与退让住后端 `control/ccm/plan.rs::build`（`next_free_name`，`--print` 与真跑同一个名字）。
 * 这里只把那三条取名路的态度说成人话 —— `stepsAside` 那一格与后端逐条对拍
 * （`tests/frontend/ui/settings/machine-aliases-naming.vitest.ts` 读后端原文，两向相等），说明里「依次试」出现 ⇔ 它为真。
 */
// ⚠ `text` 是取文函数、不是模块加载时就取好的串：模块顶层调 `copyText` 算一次副作用，会让打包器把本模块挪进
//   主窗口也要的共享块（`tests/frontend/ui/entry-graphs.vitest.ts` 当场逮到：主窗口因此「挂得上」设置页的一堆类）。
export const TMUX_NAMING: Record<TmuxMode, { stepsAside: boolean | null; text: () => string }> = {
  none: { stepsAside: null, text: () => copyText("machineAliases.tmuxNaming.none") },
  auto: {
    stepsAside: true,
    text: () => copyText("machineAliases.tmuxNaming.auto"),
  },
  named: {
    stepsAside: false,
    text: () => copyText("machineAliases.tmuxNaming.named"),
  },
  base: {
    stepsAside: true,
    text: () => copyText("machineAliases.tmuxNaming.base"),
  },
};

/** 表单那一侧的样子。**只是编辑界面** —— 合不合格由后端 `account_aliases::check_alias` 判。 */
export interface AliasForm {
  name: string;
  cwd: string;
  /** `""` = 不指定；`"--base"` = 显式不带账号；其余 = 账号名。 */
  account: string;
  tmux: TmuxMode;
  tmuxName: string;
  agent: string;
  model: string;
  launcher: string;
  tmuxSize: string;
  detach: boolean;
  busRegister: boolean;
  busNote: string;
  /** 原样交给 agent 的那几个词（按空白切；渲在 `--` 左边）。 */
  passthru: string;
}

export const BASE_CHOICE = "--base";

export function emptyForm(): AliasForm {
  return {
    name: "",
    cwd: "",
    account: "",
    tmux: "none",
    tmuxName: "",
    agent: "",
    model: "",
    launcher: "",
    tmuxSize: "",
    detach: false,
    busRegister: false,
    busNote: "",
    passthru: "",
  };
}

/**
 * 表单 → 一条别名（原样的 ccm argv）。纯函数。
 * `<交给 claude 的…> -- <ccm 自己的…>`：`--model` 与透传栏是 claude 的（左边），其余是 ccm 的（右边）。
 */
export function formToAlias(f: AliasForm): Alias {
  const claude: string[] = [];
  const ours: string[] = [];
  const cwd = f.cwd.trim();
  if (cwd) ours.push("--cwd", cwd);
  if (f.account === BASE_CHOICE) ours.push("--base");
  else if (f.account.trim()) ours.push("--account", f.account.trim());
  const tn = f.tmuxName.trim();
  if (f.tmux === "auto") ours.push("--ccm-tmux");
  else if (f.tmux === "named" && tn) ours.push(`--ccm-tmux=${tn}`);
  else if (f.tmux === "base" && tn) ours.push("--tmux-base", tn);
  if (f.agent) ours.push("--ccm-agent", f.agent);
  if (f.model.trim()) claude.push("--model", f.model.trim());
  if (f.launcher.trim()) ours.push("--launcher", f.launcher.trim());
  if (f.tmux !== "none") {
    if (f.tmuxSize.trim()) ours.push("--tmux-size", f.tmuxSize.trim());
    if (f.detach) ours.push("--detach");
    if (f.detach && f.busRegister) {
      ours.push("--bus-register");
      if (f.busNote.trim()) ours.push("--bus-note", f.busNote.trim());
    }
  }
  claude.push(...f.passthru.trim().split(/\s+/).filter(Boolean));
  const needsEnd = ours.length > 0 || claude.includes("--");
  return { name: f.name.trim(), args: needsEnd ? [...claude, "--", ...ours] : claude };
}

/** 一条别名 → 表单（「改」那一下）。按最后一个 `--` 切；认不出的参数原样塞回透传栏，**不静默丢**。 */
export function aliasToForm(a: Alias): AliasForm {
  const f = emptyForm();
  f.name = a.name;
  const extra: string[] = [];
  const cut = a.args.lastIndexOf("--");
  const left = cut < 0 ? a.args : a.args.slice(0, cut);
  const right = cut < 0 ? [] : a.args.slice(cut + 1);
  // 右边第一个词 `new`（起新会话，缺省就是它）是 ccm 的位置词，表单里没有对应格、也不进透传。
  if (right[0] === "new") right.shift();
  for (let i = 0; i < left.length; i++) {
    if (left[i] === "--model" && i + 1 < left.length && !f.model) f.model = left[++i];
    else extra.push(left[i]);
  }
  const it = right[Symbol.iterator]();
  const next = (): string => {
    const r = it.next();
    return r.done ? "" : r.value;
  };
  for (let r = it.next(); !r.done; r = it.next()) {
    const w = r.value;
    if (w.startsWith("--ccm-tmux=")) {
      f.tmux = "named";
      f.tmuxName = w.slice("--ccm-tmux=".length);
      continue;
    }
    switch (w) {
      case "--cwd": f.cwd = next(); break;
      case "--account": f.account = next(); break;
      case "--base": f.account = BASE_CHOICE; break;
      case "--ccm-tmux": f.tmux = "auto"; break;
      case "--tmux-base": f.tmux = "base"; f.tmuxName = next(); break;
      case "--ccm-agent": f.agent = next(); break;
      case "--launcher": f.launcher = next(); break;
      case "--tmux-size": f.tmuxSize = next(); break;
      case "--detach": f.detach = true; break;
      case "--bus-register": f.busRegister = true; break;
      case "--bus-note": f.busNote = next(); break;
      default: extra.push(w);
    }
  }
  f.passthru = extra.join(" ");
  return f;
}

/** 给人看的那一串参数（带空格的值加引号）。**只是显示**，写进 shell 的那一份由后端渲染。 */
export function describeArgs(args: readonly string[]): string {
  if (!args.length) return copyText("machineAliases.describeArgs.none");
  return args.map((a) => (/[\s'"]/.test(a) ? JSON.stringify(a) : a)).join(" ");
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

// 〔AR1 拍板 3〕`variant` 空串 = 默认那一种按钮（原先挂一个从没有过规则的 `settings-btn-secondary`，已摘）。
function button(label: string, variant: string, onClick: () => void): HTMLButtonElement {
  const b = el("button", variant ? `settings-btn ${variant}` : "settings-btn", label);
  b.type = "button";
  b.addEventListener("click", onClick);
  return b;
}

/**
 * 机器卡上的 ②。两个平台同一份，`platform` 是入参；本机远端同一份，`origin` 是入参。
 *
 * @param platform 这台机器用哪种 shell 的方言（本机 = [`localShell`]；远端恒 `posix`）。
 * @param origin 那台机器（取值函数：远端卡改名后跟着它走）。六条 `aliases_*` 都带它。
 * @param onBlockDone 装 / 卸别名块之后（远端卡拿它记机器列表那一格；`error` 为空 = 成了）。
 */
export function buildAliasManager(opts: {
  platform: Shell;
  origin: () => Origin;
  onBlockDone?: (verb: "install" | "remove", error: string | null) => void;
  /** 改执行策略之前问一句的注入缝（缺省走应用内对话框）。 */
  confirm?: ConfirmFn;
}): HTMLElement {
  const shell = opts.platform;
  const copy = platformCopy()[shell];
  // 只本机挂的几格（平台格 · 本机 ccm 入口 · 用系统编辑器打开）问的是 monitor 这台，远端不挂。
  const local = isLocalOrigin(opts.origin());
  const wrap = el("details", "ccm-alias-gen machine-aliases");
  wrap.dataset.shell = shell;
  wrap.dataset.origin = opts.origin();
  wrap.appendChild(el("summary", "", copyText("machineAliases.manager.title")));
  wrap.appendChild(
    el(
      "p",
      "ccm-alias-gen-hint",
      copyText("machineAliases.manager.intro"),
    ),
  );

  // ── 盘上那份 ────────────────────────────────────────────────────────────
  const status = el("div", "settings-hint machine-aliases-status");
  // ⚠ 会被 `hidden` 切的几个元素**不走 `el()`**：类名要在这一行上看得见，
  //   `css-conventions` S30 ⑦ 那把尺子才判得了「CSS 有没有在它身上裸写 display」。
  const pathCcm = document.createElement("pre");
  pathCcm.className = "ccm-path-ccm";
  pathCcm.hidden = true;
  const listBox = el("div", "machine-aliases-list");
  wrap.append(status, pathCcm, listBox);

  // ── 一条的表单 ───────────────────────────────────────────────────────────
  const grid = el("div", "ccm-alias-gen-grid");
  const text = (placeholder: string, title = ""): HTMLInputElement => {
    const i = el("input", "");
    i.type = "text";
    i.placeholder = placeholder;
    if (title) i.title = title;
    return i;
  };
  const select = (pairs: Array<[string, string]>): HTMLSelectElement => {
    const s = el("select", "");
    for (const [v, t] of pairs) {
      const o = el("option", "", t);
      o.value = v;
      s.appendChild(o);
    }
    return s;
  };
  const nameIn = text(copy.nameHint, copyText("machineAliases.form.nameHint"));
  const cwdIn = text(copyText("machineAliases.form.cwd"));
  const acctSel = select([
    ["", copyText("machineAliases.form.accountNone")],
    [BASE_CHOICE, copyText("machineAliases.form.accountBase")],
  ]);
  const tmuxSel = select([
    ["none", copyText("machineAliases.form.tmuxNone")],
    ["auto", copyText("machineAliases.form.tmuxAuto")],
    ["named", copyText("machineAliases.form.tmuxNamed")],
    ["base", copyText("machineAliases.form.tmuxBase")],
  ]);
  const tmuxNameIn = text(copyText("machineAliases.form.tmuxName"));
  const agentSel = select([
    ["", copyText("machineAliases.form.agentNone", { agent: DEFAULT_AGENT })],
    ...listAgents().map((a): [string, string] => [a, `agent：${a}`]),
  ]);
  grid.append(nameIn, cwdIn, acctSel, tmuxSel, tmuxNameIn, agentSel);
  // 选了哪种 tmux，下面一句说清撞名时会怎样（`TMUX_NAMING`）。
  for (const o of [...tmuxSel.options]) o.title = TMUX_NAMING[o.value as TmuxMode].text();
  // 挂钩用 `data-role` 不用类名：这一行的外观就是 `.settings-hint`，多一个没有规则的类名只会让悬空类名那条棘轮多一格。
  const tmuxHint = el("div", "settings-hint");
  tmuxHint.dataset.role = "tmux-naming";

  const adv = el("details", "ccm-alias-gen");
  adv.appendChild(el("summary", "", copyText("machineAliases.form.more")));
  const advGrid = el("div", "ccm-alias-gen-grid");
  const modelIn = text(copyText("machineAliases.form.model"));
  const launcherIn = text(copyText("machineAliases.form.launcher"));
  const sizeIn = text(copyText("machineAliases.form.tmuxSize"));
  const detachCk = el("input", "");
  detachCk.type = "checkbox";
  const detachLabel = el("label", "");
  detachLabel.append(detachCk, copyText("machineAliases.form.detach"));
  const busCk = el("input", "");
  busCk.type = "checkbox";
  const busLabel = el("label", "");
  busLabel.append(busCk, copyText("machineAliases.form.busRegister"));
  const busNoteIn = text(copyText("machineAliases.form.busNote"));
  const passIn = text(copyText("machineAliases.form.passthru"));
  advGrid.append(modelIn, launcherIn, sizeIn, detachLabel, busLabel, busNoteIn, passIn);
  adv.appendChild(advGrid);

  const formRow = el("div", "settings-row settings-row-actions");
  const saveBtn = button(copyText("machineAliases.form.save"), "settings-btn-primary", () => onSave());
  const clearBtn = button(copyText("machineAliases.form.clear"), "", () => fillForm(emptyForm(), -1));
  // 每个账号那一条（`<名>cc`）由那台后端在账号表变了的时候自己并进别名文件（新建 / 删号 / 修复），这里不再有一颗按钮。
  formRow.append(saveBtn, clearBtn);
  wrap.append(grid, tmuxHint, adv, formRow);

  // ── 渲染结果（第①跳）────────────────────────────────────────────────────
  const problemsBox = el("div", "settings-hint machine-aliases-problems");
  wrap.appendChild(problemsBox);
  // 撞名那一格由那台后端查它自己的 `PATH`（规则住在那台上）⇒ 「远端只核自带别名块」那句说明退役。
  let rendered: AliasRender | null = null;
  const paste = buildPasteBlock({
    text: () => rendered?.fileText ?? "",
    target: copy.pasteTarget,
    mergeNote: copyText("machineAliases.paste.mergeNote"),
    activation: copy.pasteActivation,
    invalidReason: () =>
      rendered === null
        ? copyText("machineAliases.invalid.notYet")
        : rendered.problems.length
          ? copyText("machineAliases.invalid.fixFirst")
          : null,
    multiline: true,
    rows: 6,
    className: "ccm-alias-gen-out",
  });
  wrap.appendChild(paste.element);

  // ── 写入（第②跳）────────────────────────────────────────────────────────
  // 这个下拉是这台机器上**唯一**的启动文件选择器：别名块装进哪份、
  // 「写入」时查哪份接没接上别名文件（只查不写 —— 接上那一行只住别名块里），都是它（从前 PowerShell 上另有一个版本预设下拉 ＋ 路径框，候选另有来历，`AL1d.md §1.2`）。
  // 候选只来自读回口 `aliases_read`（`$PROFILE` 在哪由后端 `shell_dialect.rs` 一处答），本文件一个路径都不推。
  const rcSel = el("select", "ccm-acct-alias-rc");
  const rcRow = el("label", "settings-row");
  rcRow.append(copy.rcLabel, rcSel);
  // 人另指的一份（原「自定义路径」，两种 shell 通用）：过后端围栏、并进候选。
  const otherRow = el("div", "settings-row");
  const otherIn = el("input", "settings-input settings-input-wide ccm-rc-other");
  otherIn.type = "text";
  otherIn.placeholder = copyText("machineAliases.rc.other");
  const otherBtn = button(copyText("machineAliases.rc.useOther"), "", () => void onOther());
  const otherErr = el("div", "settings-hint");
  otherRow.append(otherIn, otherBtn, otherErr);
  const writeBtn = button(copyText("machineAliases.rc.write"), "settings-btn-primary", () => void onWrite());
  writeBtn.title = copyText("machineAliases.rc.writeHint");
  const result = el("pre", "ccm-acct-alias-out");
  wrap.append(rcRow, otherRow, writeBtn, result);

  // ── 别名块：装 / 卸 / 预览 / 现状 —— 两种 shell 同一块，装进上面那个下拉选中的那份 ──────
  // `K-R62`：本机 POSIX 那一格的装口从前就住这里；PowerShell 那一块（`__ccm_bind` ＋ 可选 `cc`，
  // 原「终端集成」）并进来了：同一个选择器、同一次读回、同一族命令（`aliases_block_*`）。
  // 默认什么都不做：下拉停在「不动我的 shell 配置」时整块藏着。选了之后**不发 IPC** —— 现状随读回口的候选一起到。
  const rcBlock = document.createElement("div");
  rcBlock.className = "ccm-rc-block";
  rcBlock.hidden = true;
  const rcStatusRow = el("div", "settings-cc-profile-status");
  const rcStatus = el("span", "ccm-rc-block-status settings-cc-profile-badge");
  rcStatusRow.appendChild(rcStatus);
  // `with_cc`：只有 PowerShell 那一块有「要不要连 `cc` 函数一起装」这一问（POSIX 那一块的 `cc` 自带 `declare -f` 让着你）。
  const withCcLabel = el("label", "settings-row settings-row-checkbox");
  const withCc = el("input", "settings-checkbox");
  withCc.type = "checkbox";
  withCc.checked = false; // 默认只装握手那一小段，不抢你已有的 `cc`
  withCcLabel.append(withCc, el("span", "settings-checkbox-label", copyText("machineAliases.rc.withCc")));
  withCcLabel.appendChild(
    makeInfoIcon(
      copyText("machineAliases.rc.withCcHint"),
    ),
  );
  // ⚠ `hidden` 切的是外面这层 div 而不是那个 label：`.settings-row` / `.settings-row-checkbox` 在 CSS 里写了 `display`，
  //   会压过 UA 的 `[hidden]`（`css-conventions` S30 ⑦）。外层不走 `el()`：类名要在这一行上看得见。
  const withCcRow = document.createElement("div");
  withCcRow.className = "ccm-rc-withcc";
  withCcRow.appendChild(withCcLabel);
  withCcRow.hidden = shell !== "powershell";
  const rcWarn = el("div", "");
  const rcButtons = el("div", "settings-cc-profile-buttons");
  const installBtn = button(copyText("machineAliases.rc.install"), "", () =>
    void runRc("install", (path) =>
      installAliasBlock(opts.origin(), path, withCc.checked),
    ),
  );
  installBtn.title = copy.blockInstallTitle;
  const uninstallBtn = document.createElement("button");
  uninstallBtn.type = "button";
  uninstallBtn.className = "settings-btn";
  // 远端卡那一颗按 V134 叫「卸载 ccm」；本机那颗要不要随之改名还待用户（主会话现场），不在这里裁。
  uninstallBtn.textContent = local ? copyText("machineAliases.rc.uninstall") : copyText("machineCard.aliases.uninstall");
  uninstallBtn.addEventListener("click", () =>
    void runRc("remove", (path) => removeAliasBlock(opts.origin(), path)),
  );
  uninstallBtn.title = copyText("machineAliases.rc.uninstallHint");
  const previewBtn = button(copyText("machineAliases.rc.preview"), "", () => void onPreview());
  previewBtn.title = copyText("machineAliases.rc.previewHint");
  const openBtn = button(copyText("machineAliases.rc.open"), "", () => void onOpenRc());
  openBtn.title = copyText("machineAliases.rc.openHint");
  const rescanBtn = button(copyText("machineAliases.rc.rescan"), "", () => void readBack(true));
  rescanBtn.title = copyText("machineAliases.rc.rescanHint");
  rcButtons.append(installBtn, uninstallBtn, previewBtn, ...(local ? [openBtn] : []), rescanBtn);
  const rcLegacy = document.createElement("pre");
  rcLegacy.className = "ccm-rc-block-legacy";
  rcLegacy.hidden = true;
  // 块还装在别的候选里（从前只查 PowerShell 的 `profile.ps1` 那两份「v1.7 装错位置」，今天每份候选都带块的现状，照实说）。
  const rcElsewhere = document.createElement("div");
  rcElsewhere.className = "settings-cc-legacy-warn";
  rcElsewhere.hidden = true;
  const rcNote = el("div", "settings-hint");
  // 加载这份 `$PROFILE` 的那一代 PowerShell 会不会跑它（执行策略由那台后端现问、判）；
  //   不会且不是组策略钉着 ⇒ 给标准做法的按钮，点了先确认再发。
  // 两个都会被 `hidden` 切：照 `uninstallBtn` 那一形直接挂类（`css-conventions` S30 ⑦ 那把尺子才认得出它们身上没有裸 display）。
  const rcPolicy = document.createElement("div");
  rcPolicy.className = "settings-hint";
  rcPolicy.hidden = true;
  const allowBtn = document.createElement("button");
  allowBtn.type = "button";
  allowBtn.className = "settings-btn";
  allowBtn.textContent = copyText("machineAliases.policy.allow");
  allowBtn.hidden = true;
  allowBtn.addEventListener("click", () => void onAllow());
  rcBlock.append(rcStatusRow, withCcRow, rcWarn, rcPolicy, allowBtn, rcButtons, rcNote, rcLegacy, rcElsewhere);
  wrap.appendChild(rcBlock);
  // PowerShell 那一侧还有两格不随启动文件走：握手的终端数 ＋ 自动打开 monitor（原「终端集成」）· 用户级 PATH。
  // 它们一构造就问后端 ⇒ **第一次展开才建**（见下面 `toggle`），守住这一块「构造零 I/O」。
  const psSlot = el("div", "ccm-ps-slot");
  wrap.appendChild(psSlot);
  let psExtras: PsExtras | null = null;

  // ── 状态与动作 ───────────────────────────────────────────────────────────
  let list: Alias[] = [];
  let editing = -1;

  const readForm = (): AliasForm => ({
    name: nameIn.value,
    cwd: cwdIn.value,
    account: acctSel.value,
    tmux: tmuxSel.value as TmuxMode,
    tmuxName: tmuxNameIn.value,
    agent: agentSel.value,
    model: modelIn.value,
    launcher: launcherIn.value,
    tmuxSize: sizeIn.value,
    detach: detachCk.checked,
    busRegister: busCk.checked,
    busNote: busNoteIn.value,
    passthru: passIn.value,
  });

  /** / V4 在控件上：不进 tmux ⇒ 那几格禁用；不 --detach ⇒ 登记那格禁用。 */
  // 能力（不是方言）：PowerShell 目标 ⇔ Windows ⇔ 没有 tmux ⇒ tmux 那几格整组不给选
  // （后端 `account_aliases::check_alias` 的能力闸是真判定；这里只是不让人选一个必被拒的组合）。
  const hasTmux = shell === "posix";
  if (!hasTmux) {
    tmuxSel.value = "none";
    tmuxSel.disabled = true;
    tmuxSel.title = copyText("machineAliases.buildAliasManager.noTmux");
  }
  const syncEnabled = (): void => {
    const inTmux = tmuxSel.value !== "none";
    tmuxHint.textContent = TMUX_NAMING[tmuxSel.value as TmuxMode].text();
    tmuxNameIn.disabled = !(tmuxSel.value === "named" || tmuxSel.value === "base");
    sizeIn.disabled = !inTmux;
    detachCk.disabled = !inTmux;
    busCk.disabled = !inTmux || !detachCk.checked;
    busNoteIn.disabled = busCk.disabled || !busCk.checked;
  };
  for (const c of [tmuxSel, detachCk, busCk]) c.addEventListener("change", syncEnabled);

  const ensureAccountOption = (name: string): void => {
    if (!name || name === BASE_CHOICE) return;
    if ([...acctSel.options].some((o) => o.value === name)) return;
    const o = el("option", "", copyText("machineAliases.form.accountNamed", { name }));
    o.value = name;
    acctSel.appendChild(o);
  };

  const fillForm = (f: AliasForm, index: number): void => {
    editing = index;
    ensureAccountOption(f.account);
    nameIn.value = f.name;
    cwdIn.value = f.cwd;
    acctSel.value = f.account;
    tmuxSel.value = f.tmux;
    tmuxNameIn.value = f.tmuxName;
    agentSel.value = f.agent;
    modelIn.value = f.model;
    launcherIn.value = f.launcher;
    sizeIn.value = f.tmuxSize;
    detachCk.checked = f.detach;
    busCk.checked = f.busRegister;
    busNoteIn.value = f.busNote;
    passIn.value = f.passthru;
    saveBtn.textContent = index >= 0 ? copyText("machineAliases.form.update") : copyText("machineAliases.form.save");
    syncEnabled();
  };

  const renderList = (): void => {
    listBox.textContent = "";
    if (!list.length) {
      listBox.appendChild(el("div", "settings-hint", copyText("machineAliases.list.empty")));
      return;
    }
    list.forEach((a, i) => {
      const row = el("div", "settings-row machine-aliases-row");
      row.append(el("code", "", a.name), el("span", "settings-hint", describeArgs(a.args)));
      // 「实际会执行什么」那一行：点「预览」才问后端（`previewAlias`），答案挂在这一条下面。
      // 点了才建、才挂（不用 `hidden` 切：会被切的元素要一个静态认得出的类，而这一格的外观没有自己的规则）。
      const previewOut = document.createElement("pre");
      previewOut.dataset.role = "alias-preview";
      row.append(
        button(copyText("machineAliases.aliasPreview.button"), "", () => {
          if (!previewOut.isConnected) row.after(previewOut);
          previewOut.textContent = copyText("machineAliases.aliasPreview.asking");
          void previewAlias(opts.origin(), a).then((t) => {
            previewOut.textContent = t;
          });
        }),
        button(copyText("machineAliases.list.edit"), "", () => fillForm(aliasToForm(a), i)),
        button(copyText("machineAliases.list.delete"), "", () => {
          list = list.filter((_, j) => j !== i);
          if (editing === i) fillForm(emptyForm(), -1);
          void changed();
        }),
      );
      listBox.appendChild(row);
    });
  };

  /** 第①跳：清单一变就问后端要一次代码。**没有定时器** —— 每一次是一个人的动作触发的。 */
  const changed = async (): Promise<void> => {
    renderList();
    try {
      rendered = await renderAliases(opts.origin(), list, shell);
    } catch (e) {
      rendered = null;
      problemsBox.textContent = copyText("machineAliases.changed.failed", { e: String(e) });
      paste.refresh();
      return;
    }
    const lines: string[] = [];
    for (const p of rendered.problems) lines.push(`✗ ${p.name}：${p.message}`);
    // 🔴 撞名**只出声、不拦**（`cc` 在多数机器上是 C 编译器，盖不盖由人定）。
    for (const c of rendered.collisions) lines.push(`⚠ ${c}`);
    problemsBox.textContent = lines.join("\n");
    writeBtn.disabled = rendered.problems.length > 0;
    paste.refresh();
  };

  const onSave = (): void => {
    const a = formToAlias(readForm());
    if (editing >= 0 && editing < list.length) list[editing] = a;
    else list = [...list, a];
    fillForm(emptyForm(), -1);
    void changed();
  };

  /** 这台机器的启动文件候选（读回口那一次扫描的结果；别名文件那一行与别名块共用）。 */
  let cands: StartupFile[] = [];
  /** 人另指的那一份（原样的输入串；后端过围栏）。一旦指过，之后每次读回都带着它。 */
  let otherRc: string | null = null;

  const fillRcOptions = (list: readonly StartupFile[]): void => {
    const keep = rcSel.value;
    rcSel.textContent = "";
    const none = el("option", "", copyText("machineAliases.rc.none"));
    none.value = "";
    rcSel.appendChild(none);
    for (const c of list) {
      const tags: string[] = [];
      if (c.sourced) tags.push(copyText("machineAliases.rc.tagSourced"));
      if (c.block.present) tags.push(copyText("machineAliases.rc.tagBlock"));
      if (!c.exists) tags.push(copyText("machineAliases.rc.tagNew"));
      const o = el("option", "", tags.length ? `${c.path}（${tags.join("；")}）` : c.path);
      o.value = c.path;
      rcSel.appendChild(o);
    }
    if ([...rcSel.options].some((o) => o.value === keep)) rcSel.value = keep;
  };

  /**
   * 握手终端数住 monitor 进程里（不是那台盘上的事实）⇒ 另问 monitor；只有本机 PowerShell 那一格显示它。
   */
  const refreshBound = (): void => {
    if (!psExtras || !local) return;
    void commands.bound_terminal_count().then(
      (n) => psExtras?.setBound(n),
      () => undefined,
    );
  };

  /**
   * 读回口：盘上那份就是清单的起点；认不出的行原样说出来 —— 写回去之前人得知道它们会没。
   * 同一次读回带回启动文件候选（各带别名块的现状）与握手终端数 ⇒ 别名块那一格不再另问。
   * `keepList`：只刷新盘上的现状，不动人正在编辑的清单（装 / 卸别名块之后、「重新读一遍」）。
   */
  const readBack = async (keepList: boolean): Promise<void> => {
    try {
      const got = await readAliases(opts.origin(), shell, otherRc);
      if (!keepList) list = got.aliases;
      const head = got.exists
        ? copyText("machineAliases.readBack.count", { path: got.aliasPath, n: got.aliases.length })
        : copyText("machineAliases.readBack.missing", { path: got.aliasPath });
      const bad = got.unparsed.map((u) => copyText("machineAliases.readBack.unknownLine", { u }));
      status.textContent = [head, ...bad].join("\n");
      cands = got.rcCandidates;
      fillRcOptions(cands);
      refreshBound();
    } catch (e) {
      status.textContent = copyText("machineAliases.readBack.failed", { e: String(e) });
    }
    refreshRc();
  };

  const load = async (): Promise<void> => {
    // 本机 ccm 那一格：我们那一份装下来了没有 ＋ 终端里敲 `ccm` 走到的是不是它（Windows 上问新开的 PowerShell）。
    wrap.dataset.origin = opts.origin();
    if (local) {
      try {
        const st = await noteLocalCcm();
        pathCcm.hidden = !st.message;
        pathCcm.textContent = st.message;
      } catch (e) {
        pathCcm.hidden = false;
        pathCcm.textContent = copyText("machineAliases.load.ccmFailed", { e: String(e) });
      }
    }
    await readBack(false);
    await changed();
  };

  const onWrite = async (): Promise<void> => {
    writeBtn.disabled = true;
    try {
      const r = await installAliases(opts.origin(), list, rcSel.value || null, shell);
      result.textContent = [r.wroteAliasFile ? copyText("machineAliases.write.done", { path: r.aliasPath }) : "", ...r.notes]
        .filter(Boolean)
        .join("\n");
    } catch (e) {
      result.textContent = copyText("machineAliases.write.failed", { e: String(e) });
    }
    // 成功失败都重读：**盘上现在是什么样，就显示什么样**。
    await load();
  };

  /** 「其它文件」：交给读回口过围栏、并进候选，然后选中它。过不了围栏 ⇒ 原话上屏，候选不动。 */
  const onOther = async (): Promise<void> => {
    const raw = otherIn.value.trim();
    if (!raw) return;
    otherErr.textContent = "";
    try {
      const got = await readAliases(opts.origin(), shell, raw);
      otherRc = raw;
      cands = got.rcCandidates;
      fillRcOptions(cands);
      refreshBound();
      if (got.otherRc) rcSel.value = got.otherRc;
    } catch (e) {
      otherErr.textContent = copyText("machineAliases.other.failed", { e: String(e) });
    }
    refreshRc();
  };

  const selected = (): StartupFile | undefined => cands.find((c) => c.path === rcSel.value);

  /** 选中那一份的别名块现状上屏。**不发 IPC** —— 现状随读回口的候选一起到了。 */
  const refreshRc = (): void => {
    const c = selected();
    rcBlock.hidden = !c;
    if (!c) return;
    const b = c.block;
    if (c.unreadable) {
      // 在盘上、那台后端却读不了 ⇒ 照实说（别把「读不了」说成「没有别名块」）。
      rcStatus.textContent = copyText("machineAliases.rcStatus.unreadable", { path: c.path, why: c.unreadable });
      rcStatus.className = "ccm-rc-block-status settings-cc-profile-badge settings-cc-badge-warn";
    } else if (b.present) {
      const version = b.version ? `（${b.version}）` : "";
      // 旧版块（PowerShell v2）没有接上别名文件那一行 —— 重装一次就带上。
      rcStatus.textContent = b.outdated
        ? copyText("machineAliases.rcStatus.installedOutdated", { path: c.path, version })
        : c.policy?.loads === false
          ? copyText("machineAliases.rcStatus.installedNotLoaded", { path: c.path, version, ps: psName(c.policy.host) })
          : copyText("machineAliases.rcStatus.installed", { path: c.path, version });
      rcStatus.className =
        c.policy?.loads === false
          ? "ccm-rc-block-status settings-cc-profile-badge settings-cc-badge-warn"
          : "ccm-rc-block-status settings-cc-profile-badge settings-cc-badge-ok";
    } else if (!c.exists) {
      rcStatus.textContent = copyText("machineAliases.rcStatus.newFile", { path: c.path });
      rcStatus.className = "ccm-rc-block-status settings-cc-profile-badge settings-cc-badge-info";
    } else {
      rcStatus.textContent = copyText("machineAliases.rcStatus.absent", { path: c.path, blockWhat: copy.blockWhat });
      rcStatus.className = "ccm-rc-block-status settings-cc-profile-badge settings-cc-badge-warn";
    }
    showPolicy(c.policy ?? null);
    installBtn.textContent = b.present ? copyText("machineAliases.rc.reinstall") : copyText("machineAliases.rc.install");
    uninstallBtn.hidden = !b.present;
    // 「你配置里这几行是旧的」：后端逐行指名，产品自己一个字节都不删。
    rcLegacy.hidden = !b.manualCleanupHint;
    rcLegacy.textContent = b.manualCleanupHint;
    // 勾了「同时装 cc 函数」、而这份文件里块外已有同名函数 ⇒ 装上会盖掉它，先说清。
    rcWarn.textContent = "";
    rcWarn.className = "";
    if (withCc.checked && !b.present && b.conflictingFunctions.length > 0) {
      rcWarn.className = "settings-cc-profile-warn";
      rcWarn.textContent =
        copyText("machineAliases.rcStatus.conflict", { names: b.conflictingFunctions.join(", ") });
    }
    const others = cands.filter((x) => x.block.present && x.path !== c.path).map((x) => x.path);
    rcElsewhere.hidden = others.length === 0;
    rcElsewhere.textContent = others.length
      ? copyText("machineAliases.rcStatus.duplicates", { others: others.join("\n") })
      : "";
  };
  withCc.addEventListener("change", () => refreshRc());

  const runRc = async (
    verb: "install" | "remove", // 原先拿「装」「卸」两个字当动作名再拼进句子里 —— 拆成整句各自进表
    act: (path: string) => Promise<void>,
  ): Promise<void> => {
    const path = rcSel.value;
    if (!path) return;
    installBtn.disabled = true;
    uninstallBtn.disabled = true;
    rcStatus.textContent = verb === "install" ? copyText("machineAliases.runRc.installing") : copyText("machineAliases.runRc.removing");
    let failed: string | null = null;
    try {
      await act(path);
    } catch (e) {
      failed = verb === "install" ? copyText("machineAliases.runRc.installFailed", { e: String(e) }) : copyText("machineAliases.runRc.removeFailed", { e: String(e) });
    }
    installBtn.disabled = false;
    uninstallBtn.disabled = false;
    opts.onBlockDone?.(verb, failed);
    // 成功失败都重读：盘上现在是什么样，就显示什么样（清单那一格不动 —— 人可能还没写入）。
    await readBack(true);
    if (failed) rcStatus.textContent = failed;
    const pol = selected()?.policy ?? null;
    rcNote.textContent =
      failed || verb !== "install"
        ? ""
        : pol === null || pol.loads === true
          ? copy.blockAfterInstall
          : copyText("machineAliases.policy.afterInstall");
  };

  /** 执行策略那一行：只在它会挡住块（或说不清）时出声；不会挡 ⇒ 不占地方。 */
  const showPolicy = (p: ExecPolicy | null): void => {
    const ps = p ? psName(p.host) : "";
    rcPolicy.textContent =
      p === null || p.loads === true
        ? ""
        : p.error !== null
          ? copyText("machineAliases.policy.unknown", { ps, e: p.error })
          : p.loads === null
            ? copyText("machineAliases.policy.unclear", { ps, policy: p.effective ?? "" })
            : p.groupPolicy
              ? copyText("machineAliases.policy.groupPolicy", { ps, policy: p.effective ?? "" })
              : copyText("machineAliases.policy.blocks", { ps, policy: p.effective ?? "" });
    rcPolicy.hidden = rcPolicy.textContent === "";
    allowBtn.hidden = !(p?.loads === false && !p.groupPolicy);
  };

  /** 标准做法那颗按钮：先确认（不代改），再请那台后端设、再重读（现状以它现问的为准）。 */
  const onAllow = async (): Promise<void> => {
    const p = selected()?.policy;
    if (!p) return;
    const ps = psName(p.host);
    if (!(await (opts.confirm ?? askConfirm)(copyText("machineAliases.policy.confirm", { ps })))) return;
    allowBtn.disabled = true;
    let said: string;
    try {
      const r = await allowLocalScripts(opts.origin(), p.host);
      const now = r.policy.effective ?? "";
      said =
        r.policy.loads === true
          ? copyText("machineAliases.policy.setDone", { ps, policy: now })
          : r.policy.groupPolicy
            ? copyText("machineAliases.policy.groupPolicy", { ps, policy: now })
            : copyText("machineAliases.policy.setFailed", { ps, e: r.setError ?? r.policy.error ?? now });
    } catch (e) {
      said = copyText("machineAliases.policy.setFailed", { ps, e: String(e) });
    }
    allowBtn.disabled = false;
    await readBack(true);
    rcNote.textContent = said;
  };

  /** 预览别名块：与装那一下同一份渲染（后端 `plan_install`），方言由选中那份文件定。 */
  const onPreview = async (): Promise<void> => {
    const path = rcSel.value;
    if (!path) return;
    try {
      const code = await renderAliasBlock(opts.origin(), path, withCc.checked);
      showPreviewModal(copyText("machineAliases.preview.title", { path }), code);
    } catch (e) {
      showActionFailureToast(copyText("machineAliases.preview.failed"), String(e));
    }
  };

  const onOpenRc = async (): Promise<void> => {
    const path = rcSel.value;
    if (!path) return;
    try {
      await openPath(path);
    } catch (e) {
      showActionFailureToast(copyText("machineAliases.openRc.failed"), copyText("machineAliases.openRc.failedBody", { e: String(e), path }));
    }
  };

  // 只在人真的换了下拉时刷新 —— `fillRcOptions` 重建选项那一下是程序改值，不触发 `change`。
  rcSel.addEventListener("change", () => refreshRc());
  fillRcOptions([]);
  fillForm(emptyForm(), -1);
  renderList();
  let loaded = false;
  wrap.addEventListener("toggle", () => {
    if (!wrap.open || loaded) return;
    loaded = true;
    if (shell === "powershell" && local) {
      psExtras = buildPsExtras();
      psSlot.append(psExtras.element, buildUserPathBlock());
      psExtras.loadNow();
    }
    void load();
  });
  return wrap;
}

// 这里原来是远端卡那一半「把本机的别名清单复制过去贴」（读本机清单、按 POSIX 渲染给人手贴）。
//   远端卡换成上面同一个 `buildAliasManager`（`origin` = 那台），清单在那台读、在那台写 ⇒ 删。

/**
 * 🔴 `K-R135`（`R85`）：**用户级 PATH 那一格** —— 现在状态 · 一个按钮加 · 一个按钮撤。
 *
 * 用户逐字：「只把那个目录塞进进程内 `$env:PATH` 这是怎么做的，**删除能清干净吗？
 * 应该让用户手动点击加，也能管理删除。就像是 log 数据管理一样。**」
 * ⇒ 形状照他点名的那个范式 `src/frontend/ui/settings/diagnostics-section.ts`
 * （路径 ＋ 现在的读数 ＋ 几个按钮，`refresh()` 在构造与点按钮时跑）。
 *
 * # 🔴 三条纪律，都是这一格特有的
 *
 * 1. **现算，不缓存** —— 每次 `refresh()` 后端真跑一趟 `powershell.exe`（几百 ms）。
 *    ⇒ 只在**构造**与**点按钮**时调，**不轮询**。用户改完 PATH 回来点一下刷新就对了。
 * 2. **探不动 ≠ 不在 PATH 上。** 后端回 `error` 时这一格显示那句原话，
 *    **不许静默成「未安装」** —— 静默的后果是用户去点「加」，那一下同样会失败，
 *    而两次失败之间他学不到任何东西（同本文件 `refreshRc` 那条「扫不动不许静默」）。
 * 3. **两个按钮互斥地禁用**：已经在了就不让点「加」（点了也只是幂等空转，但按钮亮着
 *    等于在说「还没加」）；不在就不让点「撤」。`error` / 不支持时两个都禁。
 *
 * ⚠ **非 Windows 上不隐藏，而是显示一句「这一档只有 Windows 有」** ——
 * 隐藏会让 Linux 上的人以为「我这儿没这个问题」，而事实是**他那边走的是另一条路**
 * （rc 里那个别名块，住「本机 → 工具 → 别名」）。把两条路的关系说出来，比藏起来强。
 */
export function buildUserPathBlock(): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "settings-group ccm-user-path-block";

  const heading = document.createElement("div");
  heading.className = "settings-group-title";
  heading.textContent = copyText("machineAliases.userPath.title");
  wrap.appendChild(heading);

  const hint = document.createElement("div");
  hint.className = "settings-hint";
  hint.textContent =
    copyText("machineAliases.userPath.intro");
  wrap.appendChild(hint);

  const statusRow = document.createElement("div");
  statusRow.className = "settings-row";
  const statusLabel = document.createElement("span");
  statusLabel.className = "settings-label";
  statusLabel.textContent = copyText("machineAliases.userPath.statusLabel");
  statusRow.appendChild(statusLabel);
  const statusValue = document.createElement("span");
  statusValue.className = "settings-cc-stat-value ccm-user-path-status";
  statusValue.textContent = copyText("machineAliases.userPath.empty");
  statusRow.appendChild(statusValue);
  wrap.appendChild(statusRow);

  const dirRow = document.createElement("div");
  dirRow.className = "settings-row settings-row-stack";
  const dirLabel = document.createElement("span");
  dirLabel.className = "settings-label";
  dirLabel.textContent = copyText("machineAliases.userPath.dirLabel");
  dirRow.appendChild(dirLabel);
  const dirValue = document.createElement("span");
  dirValue.className = "settings-cc-autolaunch-path-value ccm-user-path-dir";
  dirValue.style.fontFamily = "var(--font-mono, monospace)";
  dirValue.style.fontSize = "11px";
  dirValue.style.wordBreak = "break-all";
  dirValue.textContent = copyText("machineAliases.userPath.empty");
  dirRow.appendChild(dirValue);
  wrap.appendChild(dirRow);

  const btnRow = document.createElement("div");
  btnRow.className = "settings-cc-profile-buttons";
  const addBtn = document.createElement("button");
  addBtn.type = "button";
  addBtn.className = "settings-btn settings-btn-primary ccm-user-path-add";
  addBtn.textContent = copyText("machineAliases.userPath.add");
  const delBtn = document.createElement("button");
  delBtn.type = "button";
  delBtn.className = "settings-btn ccm-user-path-remove";
  delBtn.textContent = copyText("machineAliases.userPath.remove");
  delBtn.title = copyText("machineAliases.userPath.removeHint");
  const refreshBtn = document.createElement("button");
  refreshBtn.type = "button";
  refreshBtn.className = "settings-btn ccm-user-path-refresh";
  refreshBtn.textContent = copyText("machineAliases.userPath.refresh");
  refreshBtn.title = copyText("machineAliases.userPath.refreshHint");
  btnRow.append(addBtn, delBtn, refreshBtn);
  wrap.appendChild(btnRow);

  // 那两条命令的逐字文本 —— **给不想点按钮的人复制**。
  // 🔴 它与按钮跑的是**同一份字节**（后端同一个 render 函数），所以这里敢这么说。
  const cmdNote = document.createElement("div");
  cmdNote.className = "settings-hint ccm-user-path-cmd-note";
  cmdNote.textContent = copyText("machineAliases.userPath.cmdNote");
  cmdNote.hidden = true;
  wrap.appendChild(cmdNote);
  const cmdPre = document.createElement("pre");
  cmdPre.className = "ccm-user-path-cmd";
  cmdPre.hidden = true;
  wrap.appendChild(cmdPre);

  const setBusy = (busy: boolean): void => {
    addBtn.disabled = busy;
    delBtn.disabled = busy;
    refreshBtn.disabled = busy;
  };

  const refresh = async (): Promise<void> => {
    setBusy(true);
    try {
      const st = await commands.ccm_user_path_status();
      dirValue.textContent = st.dir ?? copyText("machineAliases.userPath.empty");
      if (!st.supported) {
        // 不隐藏，说清它与别名块那一格的关系（见本函数头注第 3 条下面那一段）。
        statusValue.textContent =
          copyText("machineAliases.userPath.notWindows");
        addBtn.hidden = true;
        delBtn.hidden = true;
        cmdNote.hidden = true;
        cmdPre.hidden = true;
        return;
      }
      if (st.error) {
        // 🔴 探不动 ≠ 不在 PATH 上。原话上屏，两个按钮都不给点。
        statusValue.textContent = copyText("machineAliases.userPath.readFailed", { error: st.error });
        addBtn.disabled = true;
        delBtn.disabled = true;
        return;
      }
      statusValue.textContent = st.onUserPath
        ? copyText("machineAliases.userPath.on")
        : copyText("machineAliases.userPath.off");
      addBtn.disabled = st.onUserPath;
      delBtn.disabled = !st.onUserPath;
      const text = st.onUserPath ? st.removeCommand : st.addCommand;
      cmdPre.textContent = text ?? "";
      cmdNote.hidden = !text;
      cmdPre.hidden = !text;
    } catch (e) {
      statusValue.textContent = copyText("machineAliases.userPath.readFailed", { error: String(e) });
      addBtn.disabled = true;
      delBtn.disabled = true;
    } finally {
      refreshBtn.disabled = false;
    }
  };

  const act = async (what: "add" | "remove"): Promise<void> => {
    setBusy(true);
    try {
      if (what === "add") await commands.ccm_user_path_add();
      else await commands.ccm_user_path_remove();
    } catch (e) {
      showActionFailureToast(
        what === "add" ? copyText("machineAliases.userPath.addFailed") : copyText("machineAliases.userPath.removeFailed"),
        String(e),
      );
    }
    // 成功失败都重扫：**盘上现在是什么样，就显示什么样**（不拿我们以为的结果去写界面）。
    await refresh();
  };

  addBtn.addEventListener("click", () => void act("add"));
  delBtn.addEventListener("click", () => void act("remove"));
  refreshBtn.addEventListener("click", () => void refresh());
  void refresh();
  return wrap;
}

// ═══════════════════════════════════════════════════════════════════════════
// PowerShell 那一侧不随启动文件走的两格（原「终端集成」剩下的那一半）
// ═══════════════════════════════════════════════════════════════════════════
//
// 原「终端集成」一整块（`PsTerminalIntegration`）拆成两半：
// - **随启动文件走的那一半**（选哪份 `$PROFILE` · 装 / 卸 / 预览 / 现状 · 块外同名函数 · 「块也装在 profile.ps1 里」）
//   并进了上面那个两种 shell 共用的「别名块」—— 同一个下拉、同一次读回、同一族命令（`aliases_block_*`）。
//   它自己那一份版本预设下拉（PS 5.1 / 7 × CurrentHost / AllHosts）、TS 推 `profile.ps1` 的那一步、记住上次选择的
//   localStorage 都删了：`$PROFILE` 在哪今天只有后端 `shell_dialect.rs` 一处答。
// - **不随启动文件走的那一半**留在这里：已完成拉前握手的终端数（读回口带回来）· 自动打开 monitor。

/** 握手终端数（读回口带回来）＋ 自动打开 monitor 那一格。构造零 I/O；`loadNow()` 才问后端。 */
interface PsExtras {
  element: HTMLElement;
  setBound(n: number): void;
  loadNow(): void;
}

function buildPsExtras(): PsExtras {
  const group = document.createElement("div");
  group.className = "settings-group";

  const heading = document.createElement("div");
  heading.className = "settings-group-title";
  heading.textContent = copyText("machineAliases.ps.title");
  heading.appendChild(
    makeInfoIcon(
      copyText("machineAliases.ps.titleHint"),
    ),
  );
  group.appendChild(heading);

  // 活跃注册数
  const statRow = document.createElement("div");
  statRow.className = "settings-cc-stat-row";
  const statLabel = document.createElement("span");
  statLabel.className = "settings-cc-stat-label";
  statLabel.textContent = copyText("machineAliases.ps.registered");
  statLabel.appendChild(
    makeInfoIcon(
      copyText("machineAliases.ps.registeredHint"),
    ),
  );
  statRow.appendChild(statLabel);
  const regCountSpan = document.createElement("span");
  regCountSpan.className = "settings-cc-stat-value";
  regCountSpan.textContent = copyText("machineAliases.ps.empty");
  statRow.appendChild(regCountSpan);
  group.appendChild(statRow);

  // auto-launch toggle
  const wrap = document.createElement("div");
  wrap.className = "settings-cc-autolaunch";
  const row = document.createElement("label");
  row.className = "settings-row settings-row-checkbox";
  const autoLaunchCheckbox = document.createElement("input");
  autoLaunchCheckbox.type = "checkbox";
  autoLaunchCheckbox.className = "settings-checkbox";
  row.appendChild(autoLaunchCheckbox);
  const label = document.createElement("span");
  label.className = "settings-checkbox-label";
  label.textContent = copyText("machineAliases.ps.autoLaunch");
  row.appendChild(label);
  row.appendChild(
    makeInfoIcon(
      copyText("machineAliases.ps.autoLaunchHint"),
    ),
  );
  wrap.appendChild(row);
  const hint = document.createElement("div");
  hint.className = "settings-cc-autolaunch-path";
  const pathLabel = document.createElement("span");
  pathLabel.textContent = copyText("machineAliases.ps.pathLabel");
  pathLabel.style.color = "var(--text-faint)";
  hint.appendChild(pathLabel);
  const autoLaunchPathSpan = document.createElement("span");
  autoLaunchPathSpan.className = "settings-cc-autolaunch-path-value";
  autoLaunchPathSpan.textContent = copyText("machineAliases.ps.empty");
  hint.appendChild(autoLaunchPathSpan);
  wrap.appendChild(hint);
  group.appendChild(wrap);

  const refreshAutoLaunch = async (): Promise<void> => {
    try {
      const cfg = await commands.cc_get_auto_launch();
      autoLaunchCheckbox.checked = cfg.auto_launch_enabled;
      autoLaunchPathSpan.textContent = cfg.monitor_exe_path ?? copyText("machineAliases.ps.pathUnknown");
      autoLaunchPathSpan.title = cfg.monitor_exe_path ?? "";
    } catch (e) {
      console.warn("cc_get_auto_launch failed:", e);
      // 读不到时别把「不知道」画成「没勾」：复选框禁用、路径那格说读不到。
      autoLaunchCheckbox.disabled = true;
      autoLaunchPathSpan.textContent = copyText("machineAliases.autoLaunch.unreadable", { e: String(e) });
      return;
    }
    autoLaunchCheckbox.disabled = false;
  };
  autoLaunchCheckbox.addEventListener("change", () => {
    const enabled = autoLaunchCheckbox.checked;
    void (async () => {
      try {
        await commands.cc_set_auto_launch({ enabled });
      } catch (e) {
        showActionFailureToast(copyText("machineAliases.ps.saveFailed"), String(e));
        autoLaunchCheckbox.checked = !enabled;
      }
    })();
  });

  return {
    element: group,
    setBound: (n) => {
      regCountSpan.textContent = String(n);
    },
    loadNow: () => void refreshAutoLaunch(),
  };
}

/** 预览一段要装进去的代码（只读的浮层）。 */
function showPreviewModal(titleText: string, code: string): void {
  document.querySelector(".settings-cc-modal-backdrop")?.remove();
  const backdrop = document.createElement("div");
  backdrop.className = "settings-cc-modal-backdrop";
  const modal = document.createElement("div");
  modal.className = "settings-cc-modal";
  const title = document.createElement("div");
  title.className = "settings-cc-modal-title";
  title.textContent = titleText;
  modal.appendChild(title);
  const pre = document.createElement("pre");
  pre.className = "settings-cc-modal-code";
  pre.textContent = code;
  modal.appendChild(pre);
  const closeBtn = document.createElement("button");
  closeBtn.type = "button";
  closeBtn.className = "settings-btn";
  closeBtn.textContent = copyText("machineAliases.preview.close");
  closeBtn.addEventListener("click", () => backdrop.remove());
  const buttons = document.createElement("div");
  buttons.className = "settings-cc-modal-buttons";
  buttons.appendChild(closeBtn);
  modal.appendChild(buttons);
  backdrop.appendChild(modal);
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) backdrop.remove();
  });
  document.body.appendChild(backdrop);
}

/**
 * **本机 `ccm` 那一格的唯一写点**（K-R117 S2 本机半钉在本文件）。问一次本机那一格（判定与那句话在 monitor
 * `ccm_probe::local_ccm_cell`），`ok` 说得清就记账（两件都成 ⇒ ok；有一件不成 ⇒ fail 并照记那句话；说不清 ⇒ 不写）。
 * 调用方：别名管理器读回 · 设置页机器列表（打开时一次）· 本机那一行「重新对齐」（`fresh`：先作废 PATH 探针那份 5 分钟缓存，V149 手动兜底）。
 * Windows 本机同样问（新开的 PowerShell 里敲 `ccm` 走到哪）。
 */
export async function noteLocalCcm(fresh = false): Promise<LocalCcmEntry> {
  const st = await commands.local_ccm_entry_status(fresh);
  if (typeof st?.ok === "boolean") {
    recordFacet(LOCAL_MACHINE_KEY, "ccm", { kind: st.ok ? "ok" : "fail", detail: st.summary });
  }
  return st;
}
