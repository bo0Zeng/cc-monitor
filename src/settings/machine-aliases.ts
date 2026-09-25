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
 *    可选地往用户**自己选**的那份 rc 装一行 `source`。
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
import type { ProfileScan } from "../generated/ProfileScan";
import type { StartupFile } from "../generated/StartupFile";
import type { Shell } from "../generated/Shell";
import type { LegacyProfileEntry } from "../generated/LegacyProfileEntry";
import type { ProfileKind } from "../generated/ProfileKind";
import { openPath } from "@tauri-apps/plugin-opener";
import { makeInfoIcon, swapFileName } from "./info-icon";
import { LS_KEYS, safeGet, safeSet } from "../local-storage";
import { hostOs } from "./host-os";

/**
 * 〔AL1c〕这台机器（monitor 跑在的那台本机）用哪种 shell 的方言。**测不出就按 POSIX**（与 `host-os.ts`
 * 「测不出就照常显示」同向：POSIX 那一侧不发任何 Windows 专用 IPC）。
 */
export function localShell(): Shell {
  return hostOs() === "windows" ? "powershell" : "posix";
}

/** 平台那几格的措辞（写死在一处，组件里按 `platform` 取）。 */
const PLATFORM_COPY: Record<
  Shell,
  { pasteTarget: string; pasteActivation: string; rcLabel: string; nameHint: string }
> = {
  posix: {
    pasteTarget: "~/.bashrc（或你实际用的 shell 配置文件）",
    pasteActivation: "source 它，或开一个新终端。",
    rcLabel: "这台机器的 shell 配置（那一行 source 加进哪份）：",
    nameHint: "名字，如 zcct",
  },
  powershell: {
    pasteTarget: "$PROFILE（PowerShell 启动时读的那份脚本）",
    pasteActivation: "开一个新的 PowerShell 窗口。",
    rcLabel: "这台机器的 PowerShell 配置（那一行加进哪份 $PROFILE）：",
    nameHint: "名字，如 zcc",
  },
};

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
  if (!args.length) return "（不带参数）";
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
  const copy = PLATFORM_COPY[shell];
  const wrap = el("details", "ccm-alias-gen machine-aliases");
  wrap.dataset.shell = shell;
  wrap.appendChild(el("summary", "", "别名"));
  wrap.appendChild(
    el(
      "p",
      "ccm-alias-gen-hint",
      "一条别名是一个名字加一组 ccm 参数：在终端里敲这个名字，就等于敲 ccm 加上这组参数，后面还能再接别的参数。",
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
  const nameIn = text(copy.nameHint, "在终端里敲的那个词");
  const cwdIn = text("工作目录（留空＝当前目录）");
  const acctSel = select([
    ["", "账号：不指定"],
    [BASE_CHOICE, "账号：显式不带（--base）"],
  ]);
  const tmuxSel = select([
    ["none", "不进 tmux"],
    ["auto", "进 tmux，自动取名"],
    ["named", "进 tmux，用指定的名字"],
    ["base", "进 tmux，以某个名字为底"],
  ]);
  const tmuxNameIn = text("tmux 会话名");
  const agentSel = select([
    ["", `agent：不指定（默认 ${ACTIVE_AGENT}）`],
    ...listAgents().map((a): [string, string] => [a, `agent：${a}`]),
  ]);
  grid.append(nameIn, cwdIn, acctSel, tmuxSel, tmuxNameIn, agentSel);

  const adv = el("details", "ccm-alias-gen");
  adv.appendChild(el("summary", "", "更多参数"));
  const advGrid = el("div", "ccm-alias-gen-grid");
  const modelIn = text("--model（留空＝不带）");
  const launcherIn = text("--launcher（留空＝不带）");
  const sizeIn = text("--tmux-size，如 200x50");
  const detachCk = el("input", "");
  detachCk.type = "checkbox";
  const detachLabel = el("label", "");
  detachLabel.append(detachCk, " 建完就返回（--detach）");
  const busCk = el("input", "");
  busCk.type = "checkbox";
  const busLabel = el("label", "");
  busLabel.append(busCk, " 登记到 cc-bus（--bus-register）");
  const busNoteIn = text("cc-bus 备注");
  const passIn = text("透传给 agent 的参数（-- 之后）");
  advGrid.append(modelIn, launcherIn, sizeIn, detachLabel, busLabel, busNoteIn, passIn);
  adv.appendChild(advGrid);

  const formRow = el("div", "settings-row settings-row-actions");
  const saveBtn = button("加进清单", "settings-btn-primary", () => onSave());
  const clearBtn = button("清空表单", "settings-btn-secondary", () => fillForm(emptyForm(), -1));
  const perAcctBtn = button("为每个账号加一条", "settings-btn-secondary", () => void onPerAccount());
  perAcctBtn.title = "一次性的：每个还没有别名的账号加一条「<名>cc → --account <名>」。以后加了账号不会自动多一条。";
  formRow.append(saveBtn, clearBtn, perAcctBtn);
  wrap.append(grid, adv, formRow);

  // ── 渲染结果（第①跳）────────────────────────────────────────────────────
  const problemsBox = el("div", "settings-hint machine-aliases-problems");
  wrap.appendChild(problemsBox);
  let rendered: AliasRender | null = null;
  const paste = buildPasteBlock({
    text: () => rendered?.code ?? "",
    target: copy.pasteTarget,
    mergeNote: "整段贴到文件末尾；以后改了清单，再贴一次换掉上一次那段。",
    activation: copy.pasteActivation,
    invalidReason: () =>
      rendered === null
        ? "还没生成。"
        : rendered.problems.length
          ? "清单里有不合格的，先改好。"
          : null,
    multiline: true,
    rows: 6,
    className: "ccm-alias-gen-out",
  });
  wrap.appendChild(paste.element);

  // ── 写入（第②跳）────────────────────────────────────────────────────────
  const rcSel = el("select", "ccm-acct-alias-rc");
  const rcRow = el("label", "settings-row");
  rcRow.append(copy.rcLabel, rcSel);
  const writeBtn = button("写入", "settings-btn-primary", () => void onWrite());
  writeBtn.title = "写到 cc-monitor 自己那份别名文件；选了 shell 配置的话，再往里加一行 source。";
  const result = el("pre", "ccm-acct-alias-out");
  wrap.append(rcRow, writeBtn, result);

  // ── 别名块（cc / cct ＋ 接上别名文件那一行）：装 / 卸 / 旧行 ─────────────
  // `K-R62`：本机 POSIX 那一格的装口住这里（「终端集成」那一块整篇是 PowerShell）。
  // 默认什么都不做：下拉停在「不动我的 shell 配置」时整块藏着，一条 IPC 都不发。
  const rcBlock = document.createElement("div");
  rcBlock.className = "ccm-rc-block";
  rcBlock.hidden = true;
  const rcStatus = el("div", "ccm-rc-block-status");
  const installBtn = button("装别名块", "settings-btn-secondary", () =>
    void runRc("装", (path) =>
      commands.cc_integration_install({ path, commandName: "cc", includeCcFunction: false }),
    ),
  );
  installBtn.title =
    "把 cc / cct 与「接上别名文件」那一行（src/shared/ccm-aliases.sh）装进你选的那份配置：" +
    "只动 cc-monitor 那一小块，写前先备份、写后回读比对、不对就恢复。";
  const uninstallBtn = document.createElement("button");
  uninstallBtn.type = "button";
  uninstallBtn.className = "settings-btn settings-btn-secondary";
  uninstallBtn.textContent = "卸载别名块";
  uninstallBtn.addEventListener("click", () =>
    void runRc("卸", (path) => commands.cc_integration_uninstall({ path })),
  );
  uninstallBtn.title = "只删 cc-monitor 那一小块，你写的任何一行都不动。";
  const rcLegacy = document.createElement("pre");
  rcLegacy.className = "ccm-rc-block-legacy";
  rcLegacy.hidden = true;
  rcBlock.append(rcStatus, installBtn, uninstallBtn, rcLegacy);
  wrap.appendChild(rcBlock);
  // 〔AL1c〕PowerShell 那一侧的「别名块」是终端集成块（`__ccm_bind` ＋ 可选 `cc`）＋ 用户级 PATH 那一格。
  // 两块一构造就问后端 ⇒ **第一次展开才建**（见下面 `toggle`），守住这一块「构造零 I/O」。
  const psSlot = el("div", "ccm-ps-slot");
  wrap.appendChild(psSlot);

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
    tmuxSel.title = "这台机器上没有 tmux";
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
    const o = el("option", "", `账号：${name}`);
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
    saveBtn.textContent = index >= 0 ? "更新这一条" : "加进清单";
    syncEnabled();
  };

  const renderList = (): void => {
    listBox.textContent = "";
    if (!list.length) {
      listBox.appendChild(el("div", "settings-hint", "清单是空的。"));
      return;
    }
    list.forEach((a, i) => {
      const row = el("div", "settings-row machine-aliases-row");
      row.append(el("code", "", a.name), el("span", "settings-hint", describeArgs(a.args)));
      row.append(
        button("改", "settings-btn-secondary", () => fillForm(aliasToForm(a), i)),
        button("删", "settings-btn-secondary", () => {
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
      problemsBox.textContent = `生成失败：${String(e)}`;
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
      problemsBox.textContent = `读不到这台机器上的账号：${String(e)}`;
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

  const fillRcOptions = (cands: readonly StartupFile[]): void => {
    const keep = rcSel.value;
    rcSel.textContent = "";
    const none = el("option", "", "不动我的 shell 配置");
    none.value = "";
    rcSel.appendChild(none);
    for (const c of cands) {
      const label = c.sourced
        ? `${c.path}（已经接上了）`
        : c.exists
          ? c.path
          : `${c.path}（还不存在，写入时新建）`;
      const o = el("option", "", label);
      o.value = c.path;
      rcSel.appendChild(o);
    }
    if ([...rcSel.options].some((o) => o.value === keep)) rcSel.value = keep;
  };

  /** 读回口：盘上那份就是清单的起点。认不出的行原样说出来 —— 写回去之前人得知道它们会没。 */
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
        pathCcm.textContent = `问不到本机 ccm 这一格：${String(e)}`;
      }
    }
    try {
      const got = await commands.aliases_read({ shell });
      list = got.aliases;
      const head = got.exists
        ? `${got.aliasPath}：${got.aliases.length} 条`
        : `${got.aliasPath} 还不存在`;
      const bad = got.unparsed.map((u) => `认不出这一行，写入时它会被去掉：${u}`);
      status.textContent = [head, ...bad].join("\n");
      fillRcOptions(got.rcCandidates);
    } catch (e) {
      status.textContent = `读不了这台机器上的别名文件：${String(e)}`;
    }
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
      result.textContent = [r.wroteAliasFile ? `已写入 ${r.aliasPath}。` : "", ...r.notes]
        .filter(Boolean)
        .join("\n");
    } catch (e) {
      result.textContent = `写入失败：${String(e)}`;
    }
    // 成功失败都重读：**盘上现在是什么样，就显示什么样**。
    await load();
  };

  /** `K-R62`：把选中那份 rc 的现状扫一遍并上屏。只读。 */
  const renderScan = (scan: ProfileScan): void => {
    rcStatus.textContent = scan.has_ccm_block
      ? `✓ ${scan.path}：别名块已经装了`
      : `✗ ${scan.path}：还没有别名块（cc / cct 这一族）`;
    installBtn.textContent = scan.has_ccm_block ? "重装别名块" : "装别名块";
    uninstallBtn.hidden = !scan.has_ccm_block;
    // 「你配置里这几行是旧的」：后端逐行指名，产品自己一个字节都不删。
    rcLegacy.hidden = !scan.manual_cleanup_hint;
    rcLegacy.textContent = scan.manual_cleanup_hint;
  };

  const refreshRc = async (): Promise<void> => {
    const path = rcSel.value;
    rcBlock.hidden = !path || shell !== "posix";
    if (rcBlock.hidden) return;
    try {
      renderScan(await commands.cc_integration_scan_path({ path, commandName: "cc" }));
    } catch (e) {
      rcStatus.textContent = `扫不动 ${path}：${String(e)}`;
      rcLegacy.hidden = true;
    }
  };

  const runRc = async (
    verb: "装" | "卸",
    act: (path: string) => Promise<void>,
  ): Promise<void> => {
    const path = rcSel.value;
    if (!path) return;
    installBtn.disabled = true;
    uninstallBtn.disabled = true;
    rcStatus.textContent = `${verb}别名块中…`;
    try {
      await act(path);
    } catch (e) {
      rcStatus.textContent = `${verb}失败：${String(e)}`;
      installBtn.disabled = false;
      uninstallBtn.disabled = false;
      return;
    }
    installBtn.disabled = false;
    uninstallBtn.disabled = false;
    await refreshRc();
  };

  // 只在人真的换了下拉时扫 —— `fillRcOptions` 重建选项那一下是程序改值，不触发 `change`。
  rcSel.addEventListener("change", () => void refreshRc());
  fillRcOptions([]);
  fillForm(emptyForm(), -1);
  renderList();
  let loaded = false;
  wrap.addEventListener("toggle", () => {
    if (!wrap.open || loaded) return;
    loaded = true;
    if (shell === "powershell") {
      const term = new PsTerminalIntegration();
      psSlot.append(term.element, buildUserPathBlock());
      term.loadNow();
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
  wrap.appendChild(el("summary", "", "把本机的别名清单复制过去"));
  const status = el("div", "settings-hint");
  wrap.appendChild(status);
  let code = "";
  let bad = "";
  const paste = buildPasteBlock({
    text: () => code,
    target: "这台机器的 ~/.bashrc（或它实际用的 shell 配置文件）",
    mergeNote: "整段贴到文件末尾；以后本机的清单改了，再贴一次换掉上一次那段。",
    activation: "在那台机器上 source 它，或重连一次 ssh。",
    invalidReason: () => bad || (code ? null : "还没生成。"),
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
        bad = r.problems.length ? "本机那份清单里有不合格的，先在「本机 → 工具 → 别名」里改好。" : "";
        status.textContent = `本机那份清单：${got.aliases.length} 条。`;
      } catch (e) {
        status.textContent = `读不了本机的别名清单：${String(e)}`;
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
  heading.textContent = "这台机器的 ccm 命令（用户级 PATH）";
  wrap.appendChild(heading);

  const hint = document.createElement("div");
  hint.className = "settings-hint";
  hint.textContent =
    "把 cc-monitor 放 ccm 的那个目录加到你的用户级 PATH 上，三种终端（PowerShell / cmd / Git Bash）就都敲得到 ccm。" +
    "只改你自己的用户级 PATH：不需要管理员、不碰系统 PATH、不碰别的用户。改完要重开终端才生效。";
  wrap.appendChild(hint);

  const statusRow = document.createElement("div");
  statusRow.className = "settings-row";
  const statusLabel = document.createElement("span");
  statusLabel.className = "settings-label";
  statusLabel.textContent = "现在状态";
  statusRow.appendChild(statusLabel);
  const statusValue = document.createElement("span");
  statusValue.className = "settings-cc-stat-value ccm-user-path-status";
  statusValue.textContent = "—";
  statusRow.appendChild(statusValue);
  wrap.appendChild(statusRow);

  const dirRow = document.createElement("div");
  dirRow.className = "settings-row settings-row-stack";
  const dirLabel = document.createElement("span");
  dirLabel.className = "settings-label";
  dirLabel.textContent = "那个目录";
  dirRow.appendChild(dirLabel);
  const dirValue = document.createElement("span");
  dirValue.className = "settings-cc-autolaunch-path-value ccm-user-path-dir";
  dirValue.style.fontFamily = "var(--font-mono, monospace)";
  dirValue.style.fontSize = "11px";
  dirValue.style.wordBreak = "break-all";
  dirValue.textContent = "—";
  dirRow.appendChild(dirValue);
  wrap.appendChild(dirRow);

  const btnRow = document.createElement("div");
  btnRow.className = "settings-cc-profile-buttons";
  const addBtn = document.createElement("button");
  addBtn.type = "button";
  addBtn.className = "settings-btn settings-btn-primary ccm-user-path-add";
  addBtn.textContent = "加到用户级 PATH";
  const delBtn = document.createElement("button");
  delBtn.type = "button";
  delBtn.className = "settings-btn settings-btn-secondary ccm-user-path-remove";
  delBtn.textContent = "从用户级 PATH 撤掉";
  delBtn.title = "只摘掉我们自己那一格，你 PATH 上别的东西一个字节都不动。";
  const refreshBtn = document.createElement("button");
  refreshBtn.type = "button";
  refreshBtn.className = "settings-btn settings-btn-secondary ccm-user-path-refresh";
  refreshBtn.textContent = "刷新";
  refreshBtn.title = "重新读一次你的用户级 PATH（现算，不缓存）";
  btnRow.append(addBtn, delBtn, refreshBtn);
  wrap.appendChild(btnRow);

  // 那两条命令的逐字文本 —— **给不想点按钮的人复制**。
  // 🔴 它与按钮跑的是**同一份字节**（后端同一个 render 函数），所以这里敢这么说。
  const cmdNote = document.createElement("div");
  cmdNote.className = "settings-hint ccm-user-path-cmd-note";
  cmdNote.textContent = "不想点按钮？下面这段就是按钮会跑的那一段，复制到 PowerShell 里自己跑一次也一样：";
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
      dirValue.textContent = st.dir ?? "—";
      if (!st.supported) {
        // 不隐藏，说清它与别名块那一格的关系（见本函数头注第 3 条下面那一段）。
        statusValue.textContent =
          "这一档只有 Windows 有。这台机器上让终端找到 ccm 的，是「本机 → 工具 → 别名」里那个别名块。";
        addBtn.hidden = true;
        delBtn.hidden = true;
        cmdNote.hidden = true;
        cmdPre.hidden = true;
        return;
      }
      if (st.error) {
        // 🔴 探不动 ≠ 不在 PATH 上。原话上屏，两个按钮都不给点。
        statusValue.textContent = `读不出来：${st.error}`;
        addBtn.disabled = true;
        delBtn.disabled = true;
        return;
      }
      statusValue.textContent = st.onUserPath
        ? "✓ 已经在你的用户级 PATH 上"
        : "✗ 还不在你的用户级 PATH 上（三种终端里都敲不到 ccm）";
      addBtn.disabled = st.onUserPath;
      delBtn.disabled = !st.onUserPath;
      const text = st.onUserPath ? st.removeCommand : st.addCommand;
      cmdPre.textContent = text ?? "";
      cmdNote.hidden = !text;
      cmdPre.hidden = !text;
    } catch (e) {
      statusValue.textContent = `读不出来：${String(e)}`;
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
        what === "add" ? "加到用户级 PATH 失败" : "从用户级 PATH 撤掉失败",
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
// 〔AL1c · 第四波 4B〕PowerShell 那一侧的「别名块」：终端集成（原 `cc_integration.ts` 整块搬来）
// ═══════════════════════════════════════════════════════════════════════════
//
// 它从前是本机页上单独一块「终端集成」（只在 Windows 上构造，S9）。并进「别名」之后，
// 它就是 PowerShell 那一侧的「别名块」—— 与 POSIX 那一侧 `cc` / `cct` 片段同一个位置，由 `platform` 选。
// 行为一个字没改（`$PROFILE` 预设 · 装 / 卸 / 预览 · 已注册数 · 旧位置提示 · 自动打开 monitor）。
//
// 设置面板：PowerShell 集成区（v1.7.9 简化版）。
//  - 命令名固定 "cc"（不再让用户输自由文本，避免填错 / 跟 claude.exe 同名等坑）
//  - "同时安装 cc wrapper" 改成复选框，**默认不勾选**（默认只装 helper，不覆盖用户已有 wrapper）
//  - 所有冗长说明 → ! 图标 hover tooltip，UI 干净
// v1.7.0-1.7.1 的 bug：默认 profile 文件名搞成 `profile.ps1`（CurrentUserAllHosts），
// 但 PowerShell 默认 `$PROFILE` 指向 `Microsoft.PowerShell_profile.ps1`（CurrentUserCurrentHost）
// —— PowerShell 启动不读 `profile.ps1`，cc 集成形同虚设。v1.7.2 改回正确文件名并扫描旧位置遗留。

/** v1.7.12: 前端用的预设 id，覆盖 PS 版本 × profile scope 矩阵 */
type PresetId =
  | "Ps51-CurrentHost"
  | "Ps51-AllHosts"
  | "Ps7-CurrentHost"
  | "Ps7-AllHosts"
  | "Custom";

const PRESET_OPTIONS: Array<{ id: PresetId; label: string }> = [
  { id: "Ps51-CurrentHost", label: "PowerShell 5.1 - $PROFILE（默认）" },
  { id: "Ps51-AllHosts", label: "PowerShell 5.1 - 所有 host（profile.ps1）" },
  { id: "Ps7-CurrentHost", label: "PowerShell 7.x - $PROFILE" },
  { id: "Ps7-AllHosts", label: "PowerShell 7.x - 所有 host" },
  { id: "Custom", label: "自定义路径..." },
];

/** 固定命令名 —— 不让用户改。`claude` 跟 claude.exe 同名会无限递归；其他名字没必要让用户折腾 */
const CC_COMMAND_NAME = "cc";

class PsTerminalIntegration {
  private root: HTMLElement;
  private versionSelect!: HTMLSelectElement;
  private pathInput!: HTMLInputElement;
  private statusBadge!: HTMLSpanElement;
  private warnArea!: HTMLDivElement;
  private legacyArea!: HTMLDivElement;
  private regCountSpan!: HTMLSpanElement;
  private wrapperCheckbox!: HTMLInputElement;
  private installBtn!: HTMLButtonElement;
  private uninstallBtn!: HTMLButtonElement;
  private autoLaunchCheckbox!: HTMLInputElement;
  private autoLaunchPathSpan!: HTMLSpanElement;
  /** 当前从后端拿到的推荐路径（仅 PS 版本，CurrentHost 那条）。AllHosts 路径前端推算同目录的 profile.ps1 */
  private recommended: Record<ProfileKind, string | null> = {
    Ps51: null,
    Ps7: null,
    Custom: null,
  };

  constructor() {
    this.root = this.build();
    // ST1「延后加载」：构造期不读 —— 见 `loadNow()`。
  }

  /**
   * ST1「延后加载」：构造期不发 I/O；〔AL1c〕宿主是本文件的 `buildAliasManager`，
   * 「别名」那一块**第一次展开**时才建它、再调这一下（`70 §8` #3）。
   */
  loadNow(): void {
    void this.refresh();
    void this.refreshAutoLaunch();
  }

  get element(): HTMLElement {
    return this.root;
  }

  private build(): HTMLElement {
    const group = document.createElement("div");
    group.className = "settings-group";

    // 标题 + 旁边 ! 图标显示原理
    const heading = document.createElement("div");
    heading.className = "settings-group-title";
    heading.textContent = "PowerShell 集成";
    heading.appendChild(
      makeInfoIcon(
        "在 PowerShell profile 里装 __ccm_bind helper：启动 claude 时把当前终端 HWND 注册给 monitor，" +
          "之后 Tab ↗ 跳焦能精确拉对应终端窗口。不装也能用 monitor，但拉前不工作。",
      ),
    );
    group.appendChild(heading);

    // PowerShell 版本下拉
    const rowVer = document.createElement("div");
    rowVer.className = "settings-row";
    const lblVer = document.createElement("span");
    lblVer.className = "settings-label";
    lblVer.textContent = "PowerShell";
    rowVer.appendChild(lblVer);
    this.versionSelect = document.createElement("select");
    this.versionSelect.className = "settings-input";
    for (const opt of PRESET_OPTIONS) {
      const o = document.createElement("option");
      o.value = opt.id;
      o.textContent = opt.label;
      this.versionSelect.appendChild(o);
    }
    this.versionSelect.value = "Ps51-CurrentHost";
    this.versionSelect.addEventListener("change", () => this.onVersionChange());
    rowVer.appendChild(this.versionSelect);
    rowVer.appendChild(
      makeInfoIcon(
        "AllHosts (profile.ps1)：所有 PowerShell host 都读 —— powershell.exe / pwsh.exe / VSCode 终端 / ISE / SSH 都生效。\n\n" +
          "$PROFILE / CurrentHost (Microsoft.PowerShell_profile.ps1)：只有 powershell.exe / pwsh.exe 控制台读，VSCode/ISE 不读自己的同名文件。\n\n" +
          "推荐 AllHosts —— cc 函数在哪个终端都用。",
      ),
    );
    group.appendChild(rowVer);

    // profile 路径
    const rowPath = document.createElement("div");
    rowPath.className = "settings-row settings-row-stack";
    const lblPath = document.createElement("span");
    lblPath.className = "settings-label";
    lblPath.textContent = "Profile";
    lblPath.appendChild(
      makeInfoIcon(
        "PowerShell 启动时读的脚本文件。默认填 $PROFILE 实际指向的 " +
          "Microsoft.PowerShell_profile.ps1（CurrentUserCurrentHost）。在 PS 里跑 $PROFILE 看你机器上具体路径。",
      ),
    );
    rowPath.appendChild(lblPath);
    this.pathInput = document.createElement("input");
    this.pathInput.type = "text";
    this.pathInput.className = "settings-input settings-input-wide";
    this.pathInput.placeholder = "...\\Documents\\WindowsPowerShell\\Microsoft.PowerShell_profile.ps1";
    this.pathInput.addEventListener("change", () => {
      // 用户手编路径：保存到 localStorage，下次打开记得
      try {
        safeSet(LS_KEYS.profilePath, this.pathInput.value.trim());
        safeSet(LS_KEYS.profilePreset, "Custom");
      } catch {}
      void this.scanCurrentPath();
    });
    rowPath.appendChild(this.pathInput);
    group.appendChild(rowPath);

    // ★ v1.7.9：是否同时装 wrapper 复选框（默认不勾选）
    const wrapperRow = document.createElement("label");
    wrapperRow.className = "settings-row settings-row-checkbox";
    this.wrapperCheckbox = document.createElement("input");
    this.wrapperCheckbox.type = "checkbox";
    this.wrapperCheckbox.className = "settings-checkbox";
    this.wrapperCheckbox.checked = false; // 默认不覆盖
    this.wrapperCheckbox.addEventListener("change", () => void this.scanCurrentPath());
    wrapperRow.appendChild(this.wrapperCheckbox);
    const wrapperLabel = document.createElement("span");
    wrapperLabel.className = "settings-checkbox-label";
    wrapperLabel.textContent = "同时安装 cc wrapper";
    wrapperRow.appendChild(wrapperLabel);
    wrapperRow.appendChild(
      makeInfoIcon(
        "默认不勾选：只装 __ccm_bind helper。你需要在自己已有的 claude 启动 wrapper（function cc / function claude / 别名等）开头加一行 __ccm_bind 来触发绑定。\n\n" +
          "勾选：额外装 function cc { __ccm_bind; & claude $args }。\n" +
          "⚠ 如果你 profile 里已有 function cc 会被替换（cc-monitor 自己的块在 profile 后端，PowerShell function 后定义的会覆盖前面同名的）。",
      ),
    );
    group.appendChild(wrapperRow);

    // 状态行
    const rowStatus = document.createElement("div");
    rowStatus.className = "settings-cc-profile-status";
    this.statusBadge = document.createElement("span");
    this.statusBadge.className = "settings-cc-profile-badge";
    this.statusBadge.textContent = "...";
    rowStatus.appendChild(this.statusBadge);
    group.appendChild(rowStatus);

    // 冲突警告
    this.warnArea = document.createElement("div");
    group.appendChild(this.warnArea);

    // 按钮行
    const btnRow = document.createElement("div");
    btnRow.className = "settings-cc-profile-buttons";

    const previewBtn = document.createElement("button");
    previewBtn.type = "button";
    previewBtn.className = "settings-btn settings-btn-secondary";
    previewBtn.textContent = "预览代码";
    previewBtn.title = "看一眼即将写入 profile 的 BEGIN/END 块内容";
    previewBtn.addEventListener("click", () => void this.openPreview());
    btnRow.appendChild(previewBtn);

    const scanBtn = document.createElement("button");
    scanBtn.type = "button";
    scanBtn.className = "settings-btn settings-btn-secondary";
    scanBtn.textContent = "重新扫描";
    scanBtn.title = "重新读 profile 文件，刷新安装状态";
    scanBtn.addEventListener("click", () => void this.scanCurrentPath(true));
    btnRow.appendChild(scanBtn);

    const openBtn = document.createElement("button");
    openBtn.type = "button";
    openBtn.className = "settings-btn settings-btn-secondary";
    openBtn.textContent = "打开 profile";
    openBtn.title = "用系统默认编辑器打开当前 profile 文件";
    openBtn.addEventListener("click", () => void this.openProfileInEditor());
    btnRow.appendChild(openBtn);

    this.installBtn = document.createElement("button");
    this.installBtn.type = "button";
    this.installBtn.className = "settings-btn";
    this.installBtn.textContent = "安装";
    this.installBtn.title =
      "v1.7.10+：写入前自动备份原 profile 到同目录 <profile>.ccm-backup-<时间戳>，写入失败自动回滚。" +
      "用 Win32 ReplaceFileW 保留原文件 ACL，不会出现 v1.7.9 那种用户读不了自己 profile 的事故。";
    this.installBtn.addEventListener("click", () => void this.install());
    btnRow.appendChild(this.installBtn);

    this.uninstallBtn = document.createElement("button");
    this.uninstallBtn.type = "button";
    this.uninstallBtn.className = "settings-btn settings-btn-secondary";
    this.uninstallBtn.textContent = "卸载";
    this.uninstallBtn.style.display = "none";
    this.uninstallBtn.addEventListener("click", () => void this.uninstall());
    btnRow.appendChild(this.uninstallBtn);
    group.appendChild(btnRow);

    // 活跃注册数
    const statRow = document.createElement("div");
    statRow.className = "settings-cc-stat-row";
    const statLabel = document.createElement("span");
    statLabel.className = "settings-cc-stat-label";
    statLabel.textContent = "已注册 PowerShell session";
    statLabel.appendChild(
      makeInfoIcon(
        "正在跟 monitor 握手成功、可被 Tab ↗ 拉前的 PowerShell 进程数。\n" +
          "数字 0 ≠ 没装好：只要你那个 PS 窗口最近没跑过 cc/__ccm_bind，就不会出现在这里。",
      ),
    );
    statRow.appendChild(statLabel);
    this.regCountSpan = document.createElement("span");
    this.regCountSpan.className = "settings-cc-stat-value";
    this.regCountSpan.textContent = "—";
    statRow.appendChild(this.regCountSpan);
    group.appendChild(statRow);

    // v1.7.0-1.7.1 旧位置遗留警告
    this.legacyArea = document.createElement("div");
    group.appendChild(this.legacyArea);

    // auto-launch toggle
    group.appendChild(this.buildAutoLaunchRow());

    return group;
  }

  private buildAutoLaunchRow(): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "settings-cc-autolaunch";

    const row = document.createElement("label");
    row.className = "settings-row settings-row-checkbox";

    this.autoLaunchCheckbox = document.createElement("input");
    this.autoLaunchCheckbox.type = "checkbox";
    this.autoLaunchCheckbox.className = "settings-checkbox";
    this.autoLaunchCheckbox.addEventListener("change", () => {
      void this.toggleAutoLaunch(this.autoLaunchCheckbox.checked);
    });
    row.appendChild(this.autoLaunchCheckbox);

    const label = document.createElement("span");
    label.className = "settings-checkbox-label";
    label.textContent = "用 cc 启动 claude 时自动打开 monitor";
    row.appendChild(label);
    row.appendChild(
      makeInfoIcon(
        "勾选后：跑 cc / __ccm_bind 时如果 monitor 没在跑，PowerShell 会自动启动它（路径下方显示）。\n" +
          "不勾选：必须先手动开 monitor 再跑 cc，否则握手超时（3s）。",
      ),
    );

    wrap.appendChild(row);

    const hint = document.createElement("div");
    hint.className = "settings-cc-autolaunch-path";
    const pathLabel = document.createElement("span");
    pathLabel.textContent = "monitor 路径: ";
    pathLabel.style.color = "var(--text-faint)";
    hint.appendChild(pathLabel);
    this.autoLaunchPathSpan = document.createElement("span");
    this.autoLaunchPathSpan.className = "settings-cc-autolaunch-path-value";
    this.autoLaunchPathSpan.textContent = "—";
    hint.appendChild(this.autoLaunchPathSpan);
    wrap.appendChild(hint);

    return wrap;
  }

  private wantsWrapper(): boolean {
    return this.wrapperCheckbox.checked;
  }

  private async openProfileInEditor(): Promise<void> {
    const p = this.pathInput.value.trim();
    if (!p) {
      showActionFailureToast("请先选 PS 版本", "或在自定义里填一个 profile 路径", { level: "info" });
      return;
    }
    try {
      await openPath(p);
    } catch (e) {
      showActionFailureToast("打开失败", `${e}\n手动路径：${p}`);
    }
  }

  /** 打开面板时调用：拿推荐路径 + 填默认值 + 扫一遍当前路径状态 */
  private async refresh(): Promise<void> {
    try {
      const status = await commands.cc_integration_status({ commandName: CC_COMMAND_NAME });
      this.recommended.Ps51 = null;
      this.recommended.Ps7 = null;
      for (const p of status.profiles) {
        this.recommended[p.kind] = p.path;
      }

      // v1.7.12: 优先恢复用户上次的选择 (localStorage)；没有就默认 PS 5.1 CurrentHost
      let savedPreset: PresetId | null = null;
      let savedPath: string | null = null;
      try {
        savedPreset = safeGet(LS_KEYS.profilePreset) as PresetId | null;
        savedPath = safeGet(LS_KEYS.profilePath);
      } catch {}
      if (savedPreset && PRESET_OPTIONS.some((o) => o.id === savedPreset)) {
        this.versionSelect.value = savedPreset;
        if (savedPreset === "Custom" && savedPath) {
          this.pathInput.value = savedPath;
        } else {
          this.pathInput.value = this.pathForPreset(savedPreset) ?? "";
        }
      } else {
        this.versionSelect.value = "Ps51-CurrentHost";
        this.pathInput.value = this.pathForPreset("Ps51-CurrentHost") ?? "";
      }

      this.regCountSpan.textContent = String(status.active_registrations);
      this.renderLegacy(status.legacy_profile_paths_with_block);
      await this.scanCurrentPath();
    } catch (e) {
      this.statusBadge.textContent = `扫描失败: ${e}`;
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-warn";
    }
  }

  /** 根据下拉选项 id 推算实际 profile 路径。CurrentHost 用后端给的推荐，AllHosts 用同目录 profile.ps1 */
  private pathForPreset(id: PresetId): string | null {
    switch (id) {
      case "Ps51-CurrentHost":
        return this.recommended.Ps51;
      case "Ps7-CurrentHost":
        return this.recommended.Ps7;
      case "Ps51-AllHosts":
        return this.recommended.Ps51 ? swapFileName(this.recommended.Ps51, "profile.ps1") : null;
      case "Ps7-AllHosts":
        return this.recommended.Ps7 ? swapFileName(this.recommended.Ps7, "profile.ps1") : null;
      case "Custom":
        return null; // 由用户输入
    }
  }

  private async scanCurrentPath(notify = false): Promise<void> {
    const p = this.pathInput.value.trim();
    if (!p) {
      this.statusBadge.textContent = "（请选 PS 版本或填路径）";
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-info";
      return;
    }
    try {
      const scan = await commands.cc_integration_scan_path({
        path: p,
        commandName: CC_COMMAND_NAME,
      });
      this.renderScanResult(scan);
      if (notify) {
        this.flashScan();
      }
    } catch (e) {
      this.statusBadge.textContent = `扫描失败: ${e}`;
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-warn";
    }
  }

  private renderScanResult(scan: ProfileScan): void {
    this.warnArea.innerHTML = "";
    if (!scan.exists) {
      this.statusBadge.textContent = "○ profile 文件不存在（安装时自动创建）";
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-info";
    } else if (scan.has_ccm_block) {
      const ver = scan.ccm_block_version ? ` (${scan.ccm_block_version})` : "";
      this.statusBadge.textContent = `✓ 已安装${ver}`;
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-ok";
    } else {
      this.statusBadge.textContent = "✗ 未安装";
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-warn";
    }
    // 命令名固定 cc：如果 profile 已有同名 function 且用户勾了 "同时安装 wrapper"，警告会覆盖
    if (
      scan.conflicting_functions.length > 0 &&
      !scan.has_ccm_block &&
      this.wantsWrapper()
    ) {
      const warn = document.createElement("div");
      warn.className = "settings-cc-profile-warn";
      warn.textContent =
        `⚠ profile 已有 function ${scan.conflicting_functions.join(", ")}。` +
        ` 你勾了 "同时安装 cc wrapper"，安装后 PowerShell 会用 cc-monitor 块里的版本（因为它在 profile 后端）。` +
        ` 想保留你自己的：取消勾选，然后在自己的 function 开头加 __ccm_bind。`;
      this.warnArea.appendChild(warn);
    }
    this.installBtn.textContent = scan.has_ccm_block ? "重新安装" : "安装";
    this.uninstallBtn.style.display = scan.has_ccm_block ? "" : "none";
  }

  private renderLegacy(entries: LegacyProfileEntry[]): void {
    this.legacyArea.innerHTML = "";
    if (entries.length === 0) return;
    const warn = document.createElement("div");
    warn.className = "settings-cc-legacy-warn";
    const head = document.createElement("div");
    head.style.fontWeight = "600";
    head.textContent = "ℹ 在 profile.ps1 (AllHosts) 也检测到 cc-monitor 块";
    warn.appendChild(head);
    const body = document.createElement("div");
    body.style.marginTop = "4px";
    body.textContent =
      "profile.ps1（CurrentUserAllHosts）是合法的 PowerShell profile 位置——所有 host 都会读它。" +
      "如果你是故意装在那里（比如想让 VSCode 终端 / ISE / SSH 也用 cc），保留即可。" +
      "如果是 v1.7.0/1.7.1 残留 或 重复安装（同时也在 $PROFILE 装了一份），建议清理其中一份避免重复定义：";
    warn.appendChild(body);
    const list = document.createElement("ul");
    list.style.marginTop = "4px";
    list.style.marginLeft = "16px";
    list.style.fontFamily = "var(--font-mono, monospace)";
    list.style.fontSize = "11px";
    for (const e of entries) {
      const li = document.createElement("li");
      li.textContent = e.path;
      list.appendChild(li);
    }
    warn.appendChild(list);
    this.legacyArea.appendChild(warn);
  }

  private flashScan(): void {
    const prev = this.statusBadge.style.outline;
    this.statusBadge.style.outline = "2px solid var(--accent, #c25b3b)";
    window.setTimeout(() => {
      this.statusBadge.style.outline = prev;
    }, 500);
  }

  private onVersionChange(): void {
    const id = this.versionSelect.value as PresetId;
    // 持久化用户选择，下次打开面板恢复
    try {
      safeSet(LS_KEYS.profilePreset, id);
    } catch {}
    if (id === "Custom") {
      // Custom 不强填路径，让用户自己输
      this.pathInput.focus();
      return;
    }
    const path = this.pathForPreset(id);
    if (path) {
      this.pathInput.value = path;
      try {
        safeSet(LS_KEYS.profilePath, path);
      } catch {}
      void this.scanCurrentPath();
    } else {
      this.pathInput.value = "";
      const isPs7 = id.startsWith("Ps7");
      this.statusBadge.textContent = `（${isPs7 ? "PS 7.x" : "PS 5.1"} 没自动检测到）`;
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-info";
    }
  }

  private async openPreview(): Promise<void> {
    try {
      const resp = await commands.cc_integration_preview({
        commandName: CC_COMMAND_NAME,
        includeCcFunction: this.wantsWrapper(),
      });
      this.showPreviewModal(resp.code);
    } catch (e) {
      console.error("preview failed:", e);
    }
  }

  private showPreviewModal(code: string): void {
    document.querySelector(".settings-cc-modal-backdrop")?.remove();
    const backdrop = document.createElement("div");
    backdrop.className = "settings-cc-modal-backdrop";
    const modal = document.createElement("div");
    modal.className = "settings-cc-modal";
    const title = document.createElement("div");
    title.className = "settings-cc-modal-title";
    title.textContent = this.wantsWrapper()
      ? "将写入 profile（helper + function cc）"
      : "将写入 profile（仅 helper）";
    modal.appendChild(title);

    const pre = document.createElement("pre");
    pre.className = "settings-cc-modal-code";
    pre.textContent = code;
    modal.appendChild(pre);

    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "settings-btn settings-btn-secondary";
    closeBtn.textContent = "关闭";
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

  private async install(): Promise<void> {
    const p = this.pathInput.value.trim();
    if (!p) {
      showActionFailureToast("请先选 PS 版本", "或在自定义里填一个 profile 路径", { level: "info" });
      return;
    }
    const includeCc = this.wantsWrapper();
    try {
      await commands.cc_integration_install({
        path: p,
        commandName: CC_COMMAND_NAME,
        includeCcFunction: includeCc,
      });
      await this.scanCurrentPath();
      const tail = includeCc
        ? "请重启 PowerShell，cc 命令立即可用。"
        : "下一步：[打开 profile]，在自己的 claude wrapper 开头加 __ccm_bind 再重启 PowerShell。";
      showActionFailureToast("已写入 profile", tail, { level: "info", durationMs: 8000 });
    } catch (e) {
      showActionFailureToast("安装失败", String(e));
    }
  }

  private async uninstall(): Promise<void> {
    if (!confirm("确认卸载？BEGIN/END 块会被整块删除，profile 其他内容不动。")) return;
    try {
      await commands.cc_integration_uninstall({
        path: this.pathInput.value.trim(),
      });
      await this.scanCurrentPath();
    } catch (e) {
      showActionFailureToast("卸载失败", String(e));
    }
  }

  private async refreshAutoLaunch(): Promise<void> {
    try {
      const cfg = await commands.cc_get_auto_launch();
      this.autoLaunchCheckbox.checked = cfg.auto_launch_enabled;
      this.autoLaunchPathSpan.textContent =
        cfg.monitor_exe_path ?? "(未记录，启动一次 monitor 后自动记录)";
      this.autoLaunchPathSpan.title = cfg.monitor_exe_path ?? "";
    } catch (e) {
      console.warn("cc_get_auto_launch failed:", e);
    }
  }

  private async toggleAutoLaunch(enabled: boolean): Promise<void> {
    try {
      await commands.cc_set_auto_launch({ enabled });
    } catch (e) {
      showActionFailureToast("保存失败", String(e));
      this.autoLaunchCheckbox.checked = !enabled;
    }
  }
}
