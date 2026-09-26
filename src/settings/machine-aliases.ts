/**
 * 〔AL1 · 2026-09-24〕机器页上的 ②「别名」—— `设计/71`（别名 ＝ ccm 参数附加器）＋ `设计/70 §3.3`（并入机器页）。
 *
 * # 它取代了什么
 *
 * 从前别名有两块、住在「应用 → 行为」里：一块「按账号生成命令」（加了账号就多一条、删了就没一条 ——
 * 把**用户的清单**当成了**账号表的投影**），一块「生成自定义别名」（只吐文本、不落盘）。
 * 账号表单下面还有一句散文把人指过去 ——「用一句散文告诉用户去另一个顶层页找一个功能，
 * 本身就是 IA 失败的自证」（`70 §3.1`）。
 *
 * 今天只有**一类**：一条别名 ＝ 名字 ＋ 一组 ccm 参数；账号只是参数里的一个维度。
 * 清单归用户，「为每个账号加一条」只是个一次性的便利按钮（`71 §8`）。
 *
 * # 两跳（`71 §12.6`）—— 本文件一个字节的 shell 文本都不自己拼
 *
 * ① `aliases_render`（纯）：清单 → 代码 ＋ 每条的问题 ＋ 撞名提示。预览与「复制去手贴」只调它；
 * ② `aliases_install`（唯一的副作用）：同一份渲染落进 `~/.cc-monitor/aliases.sh`（〔RW1〕经本机后端写），
 *    选了 rc 就**只查**它接没接上（〔TL1 · 4C〕`71 §6.1`：接上那一行只住别名块里，不代装）。
 * 读回口 `aliases_read`：打开这一块时先读盘上那份，清单从它开始编辑。
 *
 * # 纪律（`launcher-diagnostics.ts` 头注那条，原样适用）
 *
 * **绝不在用户没要求时改他的配置**：打开这一块只读；「写入」按钮写明写到哪、写什么；
 * 那份 rc 由人在下拉里选，默认项是「不动我的 shell 配置」。
 * **构造零 I/O**：这一块是个 `<details>`，第一次展开才发第一条 IPC（`70 §1.3 B` · `§8` #3）。
 *
 * # 〔AL1c · 第四波 4B〕两个平台一份组件（`设计/71 §7` W5）—— 平台是它的一个输入
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
import { showActionFailureToast } from "../error-toast"; // `K-R135`：用户级 PATH 那一格的失败要出声
import { buildPasteBlock } from "../paste-block";
import { ACTIVE_AGENT, listAgents } from "../agent-profile";
import type { Alias } from "../generated/Alias";
import type { AliasRender } from "../generated/AliasRender";
import type { StartupFile } from "../generated/StartupFile";
import type { Shell } from "../generated/Shell";
import { openPath } from "@tauri-apps/plugin-opener";
import { makeInfoIcon } from "./info-icon";
import { hostOs } from "./host-os";
import { copyText } from "../copy-table";

/**
 * 〔AL1c〕这台机器（monitor 跑在的那台本机）用哪种 shell 的方言。**测不出就按 POSIX**（与 `host-os.ts`
 * 「测不出就照常显示」同向：POSIX 那一侧不发任何 Windows 专用 IPC）。
 */
export function localShell(): Shell {
  return hostOs() === "windows" ? "powershell" : "posix";
}

/** 平台那几格的措辞（写死在一处，组件里按 `platform` 取）。 */
// 〔CP2b〕做成函数、用到时才取文：模块顶层一句取文口调用都不留 —— 顶层有调用，Rollup 就把这份（连同 paste-block / info-icon）
//   从设置窗口的入口 chunk 挪进主窗也加载的共享 chunk，主窗的样式清单就对不上了（entry-graphs 那条判据现打逮到）。
const platformCopy = (): Record<
  Shell,
  {
    pasteTarget: string;
    pasteActivation: string;
    rcLabel: string;
    nameHint: string;
    /** 〔AL1d〕别名块是什么（状态行里「还没有别名块（…）」那一格）。 */
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

/**
 * `K-R49`：一个账号叫什么名字，「为每个账号加一条」给它起的名字就叫什么（`<名>cc`）。
 * 与 `cc-acct-iso shellinit` 生成的那一族逐字同形 —— 从两条路进来的人看到的是同一套名字。
 *
 * ⚠ 两处收窄：① 非法字符**丢掉**而不是换成下划线（`a.b` 与 `a_b` 换完会撞成同一个名字）；
 * ② 以数字开头就前缀一个 `_`（shell 函数名不许数字打头）。整个都非法 ⇒ 空串（调用方跳过它）。
 */
export function suggestAliasName(account: string): string {
  const cleaned = account.replace(/[^A-Za-z0-9_]/g, "");
  if (!cleaned) return "";
  const withCc = `${cleaned}cc`;
  return /^[0-9]/.test(withCc) ? `_${withCc}` : withCc;
}

/** tmux 那一维的四个取值（`71 §4` 第一档）。 */
export type TmuxMode = "none" | "auto" | "named" | "base";

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
  /** `--` 之后原样透传给 agent 的那几个词（按空白切）。 */
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

/** 表单 → 一条别名（原样的 ccm argv）。纯函数。 */
export function formToAlias(f: AliasForm): Alias {
  const args: string[] = [];
  const cwd = f.cwd.trim();
  if (cwd) args.push("--cwd", cwd);
  if (f.account === BASE_CHOICE) args.push("--base");
  else if (f.account.trim()) args.push("--account", f.account.trim());
  const tn = f.tmuxName.trim();
  if (f.tmux === "auto") args.push("--tmux");
  else if (f.tmux === "named" && tn) args.push(`--tmux=${tn}`);
  else if (f.tmux === "base" && tn) args.push("--tmux-base", tn);
  if (f.agent) args.push("--agent", f.agent);
  if (f.model.trim()) args.push("--model", f.model.trim());
  if (f.launcher.trim()) args.push("--launcher", f.launcher.trim());
  if (f.tmux !== "none") {
    if (f.tmuxSize.trim()) args.push("--tmux-size", f.tmuxSize.trim());
    if (f.detach) args.push("--detach");
    if (f.detach && f.busRegister) {
      args.push("--bus-register");
      if (f.busNote.trim()) args.push("--bus-note", f.busNote.trim());
    }
  }
  const rest = f.passthru.trim().split(/\s+/).filter(Boolean);
  if (rest.length) args.push("--", ...rest);
  return { name: f.name.trim(), args };
}

/** 一条别名 → 表单（「改」那一下）。认不出的参数原样塞回透传栏，**不静默丢**。 */
export function aliasToForm(a: Alias): AliasForm {
  const f = emptyForm();
  f.name = a.name;
  const extra: string[] = [];
  const it = a.args[Symbol.iterator]();
  const next = (): string => {
    const r = it.next();
    return r.done ? "" : r.value;
  };
  for (let r = it.next(); !r.done; r = it.next()) {
    const w = r.value;
    if (w === "--") {
      for (let x = it.next(); !x.done; x = it.next()) extra.push(x.value);
      break;
    }
    if (w.startsWith("--tmux=")) {
      f.tmux = "named";
      f.tmuxName = w.slice("--tmux=".length);
      continue;
    }
    switch (w) {
      case "--cwd": f.cwd = next(); break;
      case "--account": f.account = next(); break;
      case "--base": f.account = BASE_CHOICE; break;
      case "--tmux": f.tmux = "auto"; break;
      case "--tmux-base": f.tmux = "base"; f.tmuxName = next(); break;
      case "--agent": f.agent = next(); break;
      case "--model": f.model = next(); break;
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

function button(label: string, variant: string, onClick: () => void): HTMLButtonElement {
  const b = el("button", `settings-btn ${variant}`, label);
  b.type = "button";
  b.addEventListener("click", onClick);
  return b;
}

/**
 * 本机那张卡上的 ②。〔AL1c〕两个平台同一份，`platform` 是入参（见文件头注）。
 *
 * @param platform 这台机器用哪种 shell 的方言（本机 = [`localShell`]）。
 * @param loadAccounts 「为每个账号加一条」要的账号名。由调用方给（本机那条读口）。
 */
export function buildAliasManager(opts: {
  platform: Shell;
  loadAccounts: () => Promise<string[]>;
}): HTMLElement {
  const shell = opts.platform;
  const copy = platformCopy()[shell];
  const wrap = el("details", "ccm-alias-gen machine-aliases");
  wrap.dataset.shell = shell;
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
    ["", copyText("machineAliases.form.agentNone", { agent: ACTIVE_AGENT })],
    ...listAgents().map((a): [string, string] => [a, `agent：${a}`]),
  ]);
  grid.append(nameIn, cwdIn, acctSel, tmuxSel, tmuxNameIn, agentSel);

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
  const clearBtn = button(copyText("machineAliases.form.clear"), "settings-btn-secondary", () => fillForm(emptyForm(), -1));
  const perAcctBtn = button(copyText("machineAliases.form.perAccount"), "settings-btn-secondary", () => void onPerAccount());
  perAcctBtn.title = copyText("machineAliases.form.perAccountHint");
  formRow.append(saveBtn, clearBtn, perAcctBtn);
  wrap.append(grid, adv, formRow);

  // ── 渲染结果（第①跳）────────────────────────────────────────────────────
  const problemsBox = el("div", "settings-hint machine-aliases-problems");
  wrap.appendChild(problemsBox);
  let rendered: AliasRender | null = null;
  const paste = buildPasteBlock({
    text: () => rendered?.code ?? "",
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
  // 〔AL1d · 第四波 4B〕这个下拉是这台机器上**唯一**的启动文件选择器：别名块装进哪份、
  // 「写入」时查哪份接没接上别名文件（〔TL1〕只查不写 —— 接上那一行只住别名块里），都是它（从前 PowerShell 上另有一个版本预设下拉 ＋ 路径框，候选另有来历，`AL1d.md §1.2`）。
  // 候选只来自读回口 `aliases_read`（`$PROFILE` 在哪由后端 `shell_dialect.rs` 一处答），本文件一个路径都不推。
  const rcSel = el("select", "ccm-acct-alias-rc");
  const rcRow = el("label", "settings-row");
  rcRow.append(copy.rcLabel, rcSel);
  // 人另指的一份（原「自定义路径」，两种 shell 通用）：过后端围栏、并进候选。
  const otherRow = el("div", "settings-row");
  const otherIn = el("input", "settings-input settings-input-wide ccm-rc-other");
  otherIn.type = "text";
  otherIn.placeholder = copyText("machineAliases.rc.other");
  const otherBtn = button(copyText("machineAliases.rc.useOther"), "settings-btn-secondary", () => void onOther());
  const otherErr = el("div", "settings-hint");
  otherRow.append(otherIn, otherBtn, otherErr);
  const writeBtn = button(copyText("machineAliases.rc.write"), "settings-btn-primary", () => void onWrite());
  writeBtn.title = copyText("machineAliases.rc.writeHint");
  const result = el("pre", "ccm-acct-alias-out");
  wrap.append(rcRow, otherRow, writeBtn, result);

  // ── 别名块：装 / 卸 / 预览 / 现状 —— 两种 shell 同一块，装进上面那个下拉选中的那份 ──────
  // `K-R62`：本机 POSIX 那一格的装口从前就住这里；〔AL1d〕PowerShell 那一块（`__ccm_bind` ＋ 可选 `cc`，
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
  const installBtn = button(copyText("machineAliases.rc.install"), "settings-btn-secondary", () =>
    void runRc("install", (path) => commands.aliases_block_install({ rcPath: path, withCc: withCc.checked })),
  );
  installBtn.title = copy.blockInstallTitle;
  const uninstallBtn = document.createElement("button");
  uninstallBtn.type = "button";
  uninstallBtn.className = "settings-btn settings-btn-secondary";
  uninstallBtn.textContent = copyText("machineAliases.rc.uninstall");
  uninstallBtn.addEventListener("click", () =>
    void runRc("remove", (path) => commands.aliases_block_remove({ rcPath: path })),
  );
  uninstallBtn.title = copyText("machineAliases.rc.uninstallHint");
  const previewBtn = button(copyText("machineAliases.rc.preview"), "settings-btn-secondary", () => void onPreview());
  previewBtn.title = copyText("machineAliases.rc.previewHint");
  const openBtn = button(copyText("machineAliases.rc.open"), "settings-btn-secondary", () => void onOpenRc());
  openBtn.title = copyText("machineAliases.rc.openHint");
  const rescanBtn = button(copyText("machineAliases.rc.rescan"), "settings-btn-secondary", () => void readBack(true));
  rescanBtn.title = copyText("machineAliases.rc.rescanHint");
  rcButtons.append(installBtn, uninstallBtn, previewBtn, openBtn, rescanBtn);
  const rcLegacy = document.createElement("pre");
  rcLegacy.className = "ccm-rc-block-legacy";
  rcLegacy.hidden = true;
  // 块还装在别的候选里（从前只查 PowerShell 的 `profile.ps1` 那两份「v1.7 装错位置」，今天每份候选都带块的现状，照实说）。
  const rcElsewhere = document.createElement("div");
  rcElsewhere.className = "settings-cc-legacy-warn";
  rcElsewhere.hidden = true;
  const rcNote = el("div", "settings-hint");
  rcBlock.append(rcStatusRow, withCcRow, rcWarn, rcButtons, rcNote, rcLegacy, rcElsewhere);
  wrap.appendChild(rcBlock);
  // 〔AL1c〕PowerShell 那一侧还有两格不随启动文件走：握手的终端数 ＋ 自动打开 monitor（原「终端集成」）· 用户级 PATH。
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

  /** `71 §5` V3 / V4 在控件上：不进 tmux ⇒ 那几格禁用；不 --detach ⇒ 登记那格禁用。 */
  // 〔AL1c〕能力（不是方言）：PowerShell 目标 ⇔ Windows ⇔ 没有 tmux ⇒ tmux 那几格整组不给选
  // （后端 `account_aliases::check_alias` 的能力闸是真判定；这里只是不让人选一个必被拒的组合）。
  const hasTmux = shell === "posix";
  if (!hasTmux) {
    tmuxSel.value = "none";
    tmuxSel.disabled = true;
    tmuxSel.title = copyText("machineAliases.buildAliasManager.noTmux");
  }
  const syncEnabled = (): void => {
    const inTmux = tmuxSel.value !== "none";
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
      row.append(
        button(copyText("machineAliases.list.edit"), "settings-btn-secondary", () => fillForm(aliasToForm(a), i)),
        button(copyText("machineAliases.list.delete"), "settings-btn-secondary", () => {
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
      rendered = await commands.aliases_render({ aliases: list, shell });
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

  const onPerAccount = async (): Promise<void> => {
    let names: string[];
    try {
      names = await opts.loadAccounts();
    } catch (e) {
      problemsBox.textContent = copyText("machineAliases.perAccount.failed", { e: String(e) });
      return;
    }
    const have = new Set(list.map((a) => a.name));
    for (const account of names) {
      const name = suggestAliasName(account);
      if (!name || have.has(name)) continue;
      have.add(name);
      list = [...list, { name, args: ["--account", account] }];
    }
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
   * 读回口：盘上那份就是清单的起点；认不出的行原样说出来 —— 写回去之前人得知道它们会没。
   * 〔AL1d〕同一次读回带回启动文件候选（各带别名块的现状）与握手终端数 ⇒ 别名块那一格不再另问。
   * `keepList`：只刷新盘上的现状，不动人正在编辑的清单（装 / 卸别名块之后、「重新读一遍」）。
   */
  const readBack = async (keepList: boolean): Promise<void> => {
    try {
      const got = await commands.aliases_read({ shell, rcPath: otherRc });
      if (!keepList) list = got.aliases;
      const head = got.exists
        ? copyText("machineAliases.readBack.count", { path: got.aliasPath, n: got.aliases.length })
        : copyText("machineAliases.readBack.missing", { path: got.aliasPath });
      const bad = got.unparsed.map((u) => copyText("machineAliases.readBack.unknownLine", { u }));
      status.textContent = [head, ...bad].join("\n");
      cands = got.rcCandidates;
      fillRcOptions(cands);
      psExtras?.setBound(got.boundTerminals);
    } catch (e) {
      status.textContent = copyText("machineAliases.readBack.failed", { e: String(e) });
    }
    refreshRc();
  };

  const load = async (): Promise<void> => {
    // 本机 ccm 那一格是 POSIX 的读法（`$HOME/.cc-monitor/bin/ccm` 与 PATH 上那一份）；
    // Windows 上「终端找不找得到 ccm」由用户级 PATH 那一格答。
    if (shell === "posix") {
      try {
        const st = await commands.local_ccm_entry_status();
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
      const r = await commands.aliases_install({
        aliases: list,
        rcPath: rcSel.value || null,
        shell,
      });
      result.textContent = [r.wroteAliasFile ? copyText("machineAliases.write.done", { path: r.aliasPath }) : "", ...r.notes]
        .filter(Boolean)
        .join("\n");
    } catch (e) {
      result.textContent = copyText("machineAliases.write.failed", { e: String(e) });
    }
    // 成功失败都重读：**盘上现在是什么样，就显示什么样**。
    await load();
  };

  /** 〔AL1d〕「其它文件」：交给读回口过围栏、并进候选，然后选中它。过不了围栏 ⇒ 原话上屏，候选不动。 */
  const onOther = async (): Promise<void> => {
    const raw = otherIn.value.trim();
    if (!raw) return;
    otherErr.textContent = "";
    try {
      const got = await commands.aliases_read({ shell, rcPath: raw });
      otherRc = raw;
      cands = got.rcCandidates;
      fillRcOptions(cands);
      psExtras?.setBound(got.boundTerminals);
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
    if (b.present) {
      const version = b.version ? `（${b.version}）` : "";
      // 〔TL1 · 4C〕旧版块（PowerShell v2）没有接上别名文件那一行 —— 重装一次就带上（`71 §6.1`）。
      rcStatus.textContent = b.outdated
        ? copyText("machineAliases.rcStatus.installedOutdated", { path: c.path, version })
        : copyText("machineAliases.rcStatus.installed", { path: c.path, version });
      rcStatus.className = "ccm-rc-block-status settings-cc-profile-badge settings-cc-badge-ok";
    } else if (!c.exists) {
      rcStatus.textContent = copyText("machineAliases.rcStatus.newFile", { path: c.path });
      rcStatus.className = "ccm-rc-block-status settings-cc-profile-badge settings-cc-badge-info";
    } else {
      rcStatus.textContent = copyText("machineAliases.rcStatus.absent", { path: c.path, blockWhat: copy.blockWhat });
      rcStatus.className = "ccm-rc-block-status settings-cc-profile-badge settings-cc-badge-warn";
    }
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
    verb: "install" | "remove", // 〔CP2b〕原先拿「装」「卸」两个字当动作名再拼进句子里 —— 拆成整句各自进表
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
    // 成功失败都重读：盘上现在是什么样，就显示什么样（清单那一格不动 —— 人可能还没写入）。
    await readBack(true);
    if (failed) rcStatus.textContent = failed;
    rcNote.textContent = !failed && verb === "install" ? copy.blockAfterInstall : "";
  };

  /** 〔AL1d〕预览别名块：与装那一下同一份渲染（后端 `plan_install`），方言由选中那份文件定。 */
  const onPreview = async (): Promise<void> => {
    const path = rcSel.value;
    if (!path) return;
    try {
      const code = await commands.aliases_block_render({ rcPath: path, withCc: withCc.checked });
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
    if (shell === "powershell") {
      psExtras = buildPsExtras();
      psSlot.append(psExtras.element, buildUserPathBlock());
      psExtras.loadNow();
    }
    void load();
  });
  return wrap;
}

/**
 * 〔MC1 · 2026-09-24〕远端机器卡上 ②「别名」的那一半：**把本机那份清单生成出来，复制去那台机器贴**。
 *
 * 为什么只给「手贴」：`71 §12.6` 的②有两种模式 ——「app 代写」与「用户手贴」。远端的 app 代写
 * 要**叫那台机器的后端自己写**（`71 §12.6.3`），那是把写挪进后端，本路停下报备没做；
 * 也**不走** monitor 这一侧的 SFTP 再长一条写路（那正是 `71 §12.5` 要收掉的第三份）。
 * 渲染这一跳是纯的，本机算出来的 POSIX 文本拿去远端一样能用。
 *
 * 构造零 I/O：第一次展开才读本机清单、问一次渲染。
 */
export function buildRemoteAliasPaste(): HTMLElement {
  const wrap = el("details", "ccm-alias-gen");
  wrap.appendChild(el("summary", "", copyText("machineAliases.remote.copyTitle")));
  const status = el("div", "settings-hint");
  wrap.appendChild(status);
  let code = "";
  let bad = "";
  const paste = buildPasteBlock({
    text: () => code,
    target: copyText("machineAliases.remote.pasteTarget"),
    mergeNote: copyText("machineAliases.remote.mergeNote"),
    activation: copyText("machineAliases.remote.activation"),
    invalidReason: () => bad || (code ? null : copyText("machineAliases.invalid.notYet")),
    multiline: true,
    rows: 6,
    className: "ccm-alias-gen-out",
  });
  wrap.appendChild(paste.element);
  let loaded = false;
  wrap.addEventListener("toggle", () => {
    if (!wrap.open || loaded) return;
    loaded = true;
    void (async () => {
      try {
        // 读的是**本机**那份清单（本机是哪种方言就读哪一份）；远端是 POSIX（后端只发 Linux 的产物）⇒ 按 POSIX 渲染。
        const got = await commands.aliases_read({ shell: localShell() });
        const r = await commands.aliases_render({ aliases: got.aliases, shell: "posix" });
        code = r.code;
        bad = r.problems.length ? copyText("machineAliases.remote.fixFirst") : "";
        status.textContent = copyText("machineAliases.remote.count", { n: got.aliases.length });
      } catch (e) {
        status.textContent = copyText("machineAliases.remote.readFailed", { e: String(e) });
      }
      paste.refresh();
    })();
  });
  return wrap;
}

/**
 * 🔴 `K-R135`（`R85`）：**用户级 PATH 那一格** —— 现在状态 · 一个按钮加 · 一个按钮撤。
 *
 * 用户逐字：「只把那个目录塞进进程内 `$env:PATH` 这是怎么做的，**删除能清干净吗？
 * 应该让用户手动点击加，也能管理删除。就像是 log 数据管理一样。**」
 * ⇒ 形状照他点名的那个范式 `src/settings/diagnostics-section.ts`
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
  delBtn.className = "settings-btn settings-btn-secondary ccm-user-path-remove";
  delBtn.textContent = copyText("machineAliases.userPath.remove");
  delBtn.title = copyText("machineAliases.userPath.removeHint");
  const refreshBtn = document.createElement("button");
  refreshBtn.type = "button";
  refreshBtn.className = "settings-btn settings-btn-secondary ccm-user-path-refresh";
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
// 〔AL1c · 第四波 4B〕PowerShell 那一侧不随启动文件走的两格（原「终端集成」剩下的那一半）
// ═══════════════════════════════════════════════════════════════════════════
//
// 〔AL1d · 第四波 4B〕原「终端集成」一整块（`PsTerminalIntegration`）拆成两半：
// - **随启动文件走的那一半**（选哪份 `$PROFILE` · 装 / 卸 / 预览 / 现状 · 块外同名函数 · 「块也装在 profile.ps1 里」）
//   并进了上面那个两种 shell 共用的「别名块」—— 同一个下拉、同一次读回、同一族命令（`aliases_block_*`）。
//   它自己那一份版本预设下拉（PS 5.1 / 7 × CurrentHost / AllHosts）、TS 推 `profile.ps1` 的那一步、记住上次选择的
//   localStorage 都删了：`$PROFILE` 在哪今天只有后端 `shell_dialect.rs` 一处答（`调研/第四波记录/AL1d.md §2.3`）。
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
    }
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
  closeBtn.className = "settings-btn settings-btn-secondary";
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
