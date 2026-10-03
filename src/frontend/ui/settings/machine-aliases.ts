/**
 * 机器页上的 ②「别名」：上面「终端接入」（先显示现状），下面「清单」（两组，就地增改，一步存到那台）。
 *
 * # 界面只画、只发命令 —— 判定全在那台后端
 *
 * - 清单 · 每条的归组（账号那一形）· 这台的账号表 · 哪个号缺哪一条 · 文件指纹：读回口 `aliases-read` 一次带来，
 *   本文件**不按名字或参数自己认组**；
 * - 存：`aliases-install` 收整份清单 ＋ 读回时的指纹，盘上被别处改过 ⇒ 后端拒（[`AliasesStale`]），这里重读、表单留着让人再存；
 * - 合不合格 · 撞名 · 和系统命令重名：`aliases-render`；会执行什么：`ccm-print`；
 * - 表单 ⇄ 参数：`aliases-to-form` / `aliases-from-form`（那台后端按 ccm 自己的解析器转；「改」那一下把原来那条一起交回，没动的原样留）；
 * - 接入（别名块装 / 断开 / 我自己贴）：`aliases-block-*`，块外同名函数与执行策略随候选一起到。
 *
 * 本文件一个 ccm 参数都不认、一个字节的 shell 文本都不拼：表单那几格原样交后端、后端回一条别名；写进 shell 的那一份也由后端渲染。
 *
 * # 纪律
 *
 * **构造零 I/O**：这一块是个 `<details>`，第一次展开才发第一条 IPC。**绝不在用户没要求时改他的配置**：
 * 接入那一格只有人点了「接入」才写他的启动文件；清单是 cc-monitor 自己那份别名文件，改了立刻存。
 *
 * # 两个平台一份组件 —— 平台是它的一个输入
 *
 * 差在平台那几格：tmux 那一族在 PowerShell 上不给（Windows 没有 tmux —— 能力，不是方言）；
 * PowerShell 那一侧的握手终端数 · 执行策略 · 用户级 PATH · 自动打开 monitor 收在「终端接入」里。
 */
import { commands } from "../ipc/commands";
import { chan } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "../ipc/chan-caller";
import { isLocalOrigin } from "../ipc/origin";
import { showActionFailureToast } from "../error-toast"; // `K-R135`：用户级 PATH 那一格的失败要出声
import { buildPasteBlock } from "../paste-block";
import { DEFAULT_AGENT, listAgents } from "../agent-profile";
// 别名六问走通道、那台后端出成品（`../alias-reads`）；类型随成品住那边。
import type {
  AccountShape,
  Alias,
  AliasForm,
  AliasRender,
  CwdCase,
  ExecPolicy,
  MissingAlias,
  PsHost,
  StartupFile,
  Shell,
  TmuxMode,
} from "../alias-reads";
import { askConfirm, type ConfirmFn } from "../ask-dialog";
import {
  AliasesStale,
  aliasFromForm,
  aliasToForm,
  allowLocalScripts,
  emptyForm,
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
 * **认不出就不猜**（`null`）：别名那一格明说「认不出这台的系统」、安装入口置灰（[`buildUnknownOsAliasBlock`]）。
 */
export function localShell(): Shell | null {
  const os = hostOs();
  if (os === "unknown") return null;
  return os === "windows" ? "powershell" : "posix";
}

/** 认不出本机系统时别名那一格：说清为什么没有，接入入口在、但置灰（出声不静默）。 */
export function buildUnknownOsAliasBlock(): HTMLElement {
  const wrap = el("details", "ccm-alias-gen machine-aliases");
  wrap.dataset.shell = "unknown";
  wrap.appendChild(el("summary", "", copyText("machineAliases.manager.title")));
  wrap.appendChild(el("p", "settings-hint", copyText("machineAliases.unknownOs.said")));
  const install = el("button", "settings-btn", copyText("machineAliases.access.connect"));
  install.type = "button";
  install.disabled = true;
  install.title = copyText("machineAliases.unknownOs.said");
  wrap.appendChild(install);
  return wrap;
}

/** 问一次预览（`ccm-print`）最多等多久：读一份账号库 ＋ 问一次会话快照，秒级内。 */
const PREVIEW_BUDGET_MS = 10_000;

/**
 * **一条别名实际会执行什么**：问那台机器的后端（帧命令 `ccm-print`，与终端里 `ccm -- --ccm-print` 同一个计划函数；
 * 语境是「家目录里的一个新终端」）。`line` 原样上屏，本文件一个字节的 shell 都不拼。
 */
export async function previewAlias(origin: Origin, a: Alias): Promise<string> {
  try {
    const budget = budgetWithin(PREVIEW_BUDGET_MS);
    const body = jsonBody({ args: a.args });
    const reply = await chan.call(origin, "ccm-print", body, budget);
    const got = readJson(reply) as { line?: unknown };
    return copyText("machineAliases.aliasPreview.line", {
      line: typeof got.line === "string" ? got.line : "",
    });
  } catch (e) {
    return copyText("machineAliases.aliasPreview.failed", {
      reason: saidOf(e, copyText("machineAliases.aliasPreview.tooOld")),
    });
  }
}

/**
 * 「在哪起」各项**撞名时会怎样**（下拉的选项只有名字，没有一句说撞了会怎样）。
 *
 * 规则不在这里：取名与退让住后端 `control/ccm/plan.rs::build`。这里只把那几条路的态度说成人话 ——
 * `stepsAside` 那一格与后端逐条对拍（`tests/frontend/ui/settings/machine-aliases-naming.vitest.ts` 读后端原文，两向相等），
 * 说明里「依次试」出现 ⇔ 它为真；不取名的两项（当前终端 · 接回）记 `null`。
 */
// ⚠ `text` 是取文函数、不是模块加载时就取好的串：模块顶层调 `copyText` 会让打包器把本模块挪进主窗口也要的共享块。
export const TMUX_NAMING: Record<TmuxMode, { stepsAside: boolean | null; text: () => string }> = {
  none: { stepsAside: null, text: () => copyText("machineAliases.tmuxNaming.none") },
  auto: { stepsAside: true, text: () => copyText("machineAliases.tmuxNaming.auto") },
  named: { stepsAside: false, text: () => copyText("machineAliases.tmuxNaming.named") },
  base: { stepsAside: true, text: () => copyText("machineAliases.tmuxNaming.base") },
  attach: { stepsAside: null, text: () => copyText("machineAliases.tmuxNaming.attach") },
};

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

// `variant` 空串 = 默认那一种按钮。
function button(label: string, variant: string, onClick: () => void): HTMLButtonElement {
  const b = el("button", variant ? `settings-btn ${variant}` : "settings-btn", label);
  b.type = "button";
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    onClick();
  });
  return b;
}

/** 账号下拉里「不用任何账号」那一项的值：只是个记号（那一项按元素认，见表单），不是 ccm 参数。 */
const BASE_OPTION = "(base)";

/** 同一条别名（名字 ＋ 参数 ＋ 交给谁逐格相等）。 */
const sameAlias = (a: Alias, b: Alias): boolean =>
  a.name === b.name && a.restTo === b.restTo && a.args.length === b.args.length && a.args.every((w, i) => w === b.args[i]);

/** 账号组里一行说的是什么（`tmux` ⇒ 在 tmux 里起，否则当前终端）。 */
const shapeWhat = (tmux: boolean): string =>
  tmux ? copyText("machineAliases.list.shapeTmux") : copyText("machineAliases.list.shapeHere");

/**
 * 一条别名的那张表单（「＋ 新增别名」与「改」共用）：名字 · 账号（那台的账号表）· 在哪起 · 工作目录（分情况 ＋ 其余情况）·
 * ▸ 更多；下一行实时「会执行：…」与这一条的问题 / 撞名；[保存] 一步写到那台、[取消]。
 * 每改一格（`change`）问一次后端 —— 没有定时器，每一次都是一个人的动作触发的。
 */
interface FormHost {
  shell: Shell;
  hasTmux: boolean;
  accounts: () => readonly string[];
  /** 表单 → 一条别名（那台后端拼；「改」时带着原来那条）。拼不出 ⇒ 抛那句话。 */
  render: (f: AliasForm) => Promise<Alias>;
  /** 这一条按表单现在的样子放进清单后，问后端这一条的问题与撞名。 */
  check: (a: Alias) => Promise<string[]>;
  preview: (a: Alias) => Promise<string>;
  save: (a: Alias) => Promise<string | null>;
  cancel: () => void;
}

function buildAliasForm(initial: AliasForm, host: FormHost): HTMLElement {
  const box = el("div", "");
  box.dataset.role = "alias-form";
  const grid = el("div", "ccm-alias-gen-grid");
  const text = (placeholder: string, value = "", title = ""): HTMLInputElement => {
    const i = el("input", "");
    i.type = "text";
    i.placeholder = placeholder;
    i.value = value;
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
  const nameIn = text(copyText("machineAliases.form.nameHint"), initial.name);
  nameIn.dataset.role = "name";
  const acctSel = select([
    ["", copyText("machineAliases.form.accountNone")],
    [BASE_OPTION, copyText("machineAliases.form.accountBase")],
    ...host.accounts().map((a): [string, string] => [a, copyText("machineAliases.form.accountNamed", { name: a })]),
  ]);
  acctSel.dataset.role = "account";
  // 「不用任何账号」那一项按元素认、不按值认（值只是个记号，不与任何号名相撞）。
  const baseOpt = acctSel.options[1];
  const isBase = (): boolean => acctSel.options[acctSel.selectedIndex] === baseOpt;
  // 别名里写的号不在这台的账号表里（手编的 / 号删了）：照原样摆一项，不悄悄变成「不指定」。
  let picked = initial.account
    ? [...acctSel.options].find((o) => o !== baseOpt && o.value === initial.account)
    : initial.base
      ? baseOpt
      : acctSel.options[0];
  if (!picked) {
    picked = el("option", "", copyText("machineAliases.form.accountNamed", { name: initial.account }));
    picked.value = initial.account;
    acctSel.appendChild(picked);
  }
  picked.selected = true;
  const tmuxSel = select([
    ["none", copyText("machineAliases.form.tmuxNone")],
    ["auto", copyText("machineAliases.form.tmuxAuto")],
    ["named", copyText("machineAliases.form.tmuxNamed")],
    ["base", copyText("machineAliases.form.tmuxBase")],
    ["attach", copyText("machineAliases.form.tmuxAttach")],
  ]);
  tmuxSel.dataset.role = "where";
  for (const o of [...tmuxSel.options]) o.title = TMUX_NAMING[o.value as TmuxMode].text();
  // 能力（不是方言）：PowerShell 目标 ⇔ Windows ⇔ 没有 tmux ⇒ tmux 那一族（含接回）整组不给选（后端能力闸是真判定）。
  if (!host.hasTmux) {
    for (const o of [...tmuxSel.options]) if (o.value !== "none") o.disabled = true;
    tmuxSel.title = copyText("machineAliases.buildAliasManager.noTmux");
  }
  tmuxSel.value = host.hasTmux ? initial.tmux : "none";
  const tmuxNameIn = text(copyText("machineAliases.form.tmuxName"), initial.tmuxName);
  grid.append(nameIn, acctSel, tmuxSel, tmuxNameIn);
  // 挂钩用 `data-role` 不用类名：这一行的外观就是 `.settings-hint`。
  const tmuxHint = el("div", "settings-hint");
  tmuxHint.dataset.role = "tmux-naming";

  // ── 工作目录：分情况 ＋ 其余情况 ──
  const cwdBox = el("div", "settings-row settings-row-stack");
  cwdBox.dataset.role = "cwd";
  cwdBox.appendChild(el("span", "settings-label", copyText("machineAliases.form.cwdTitle")));
  const cases = el("div", "");
  const caseRows: Array<{ at: HTMLInputElement; to: HTMLInputElement; row: HTMLElement }> = [];
  const addCase = (c: CwdCase): void => {
    const row = el("div", "settings-row");
    row.dataset.role = "cwd-case";
    const at = text(copyText("machineAliases.form.cwdAt"), c.at);
    const to = text(copyText("machineAliases.form.cwdTo"), c.to);
    const entry = { at, to, row };
    const drop = button(copyText("machineAliases.form.cwdDrop"), "", () => {
      caseRows.splice(caseRows.indexOf(entry), 1);
      row.remove();
      changed();
    });
    drop.title = copyText("machineAliases.form.cwdDropHint");
    row.append(
      el("span", "", copyText("machineAliases.form.cwdIn")),
      at,
      el("span", "", copyText("machineAliases.form.cwdArrow")),
      to,
      drop,
    );
    for (const i of [at, to]) i.addEventListener("change", () => changed());
    caseRows.push(entry);
    cases.appendChild(row);
  };
  for (const c of initial.cwdIf) addCase(c);
  const addCaseBtn = button(copyText("machineAliases.form.cwdAdd"), "", () => {
    addCase({ at: "", to: "" });
  });
  const elseRow = el("div", "settings-row");
  elseRow.dataset.role = "cwd-else";
  const radio = (label: string, checked: boolean): HTMLInputElement => {
    const r = el("input", "");
    r.type = "radio";
    r.name = `cwd-else-${Math.random().toString(36).slice(2)}`;
    r.checked = checked;
    const l = el("label", "");
    l.append(r, label);
    elseRow.appendChild(l);
    return r;
  };
  elseRow.appendChild(el("span", "", copyText("machineAliases.form.cwdElse")));
  const hereR = radio(copyText("machineAliases.form.cwdHere"), initial.cwd === "");
  const dirR = radio(copyText("machineAliases.form.cwdDir"), initial.cwd !== "");
  dirR.name = hereR.name;
  const cwdIn = text(copyText("machineAliases.form.cwd"), initial.cwd);
  elseRow.appendChild(cwdIn);
  cwdBox.append(cases, addCaseBtn, elseRow);

  // ── ▸ 更多 ──
  const adv = el("details", "ccm-alias-gen");
  adv.appendChild(el("summary", "", copyText("machineAliases.form.more")));
  const advGrid = el("div", "ccm-alias-gen-grid");
  const agentSel = select([
    ["", copyText("machineAliases.form.agentNone", { agent: DEFAULT_AGENT })],
    ...listAgents().map((a): [string, string] => [a, `agent：${a}`]),
  ]);
  // 写的 agent 不是注册表里的一家：照原样摆一项（不可选），存的时候后端会拒、说出认得的几家。
  if (initial.agent && !listAgents().includes(initial.agent)) {
    const o = el("option", "", copyText("machineAliases.form.agentUnknown", { agent: initial.agent }));
    o.value = initial.agent;
    o.disabled = true;
    o.dataset.role = "agent-unknown";
    agentSel.appendChild(o);
  }
  agentSel.value = initial.agent;
  const modelIn = text(copyText("machineAliases.form.model"), initial.model);
  const launcherIn = text(copyText("machineAliases.form.launcher"), initial.launcher);
  const sizeIn = text(copyText("machineAliases.form.tmuxSize"), initial.tmuxSize);
  const detachCk = el("input", "");
  detachCk.type = "checkbox";
  detachCk.checked = initial.detach;
  const detachLabel = el("label", "");
  detachLabel.append(detachCk, copyText("machineAliases.form.detach"));
  const busCk = el("input", "");
  busCk.type = "checkbox";
  busCk.checked = initial.busRegister;
  const busLabel = el("label", "");
  busLabel.append(busCk, copyText("machineAliases.form.busRegister"));
  const busNoteIn = text(copyText("machineAliases.form.busNote"), initial.busNote);
  const passIn = text(copyText("machineAliases.form.passthru"), initial.passthru);
  advGrid.append(agentSel, modelIn, launcherIn, sizeIn, detachLabel, busLabel, busNoteIn, passIn);
  adv.appendChild(advGrid);

  // ── 会执行：… · 这一条的问题 / 撞名 · [保存] [取消] ──
  const wouldRun = el("pre", "");
  wouldRun.dataset.role = "would-run";
  const notes = el("div", "settings-hint");
  notes.dataset.role = "form-notes";
  const actions = el("div", "settings-row settings-row-actions");
  const saveBtn = button(copyText("machineAliases.form.save"), "settings-btn-primary", () => void onSave());
  const cancelBtn = button(copyText("machineAliases.form.cancel"), "", () => host.cancel());
  actions.append(saveBtn, cancelBtn);
  box.append(grid, tmuxHint, cwdBox, adv, wouldRun, notes, actions);

  const read = (): AliasForm => ({
    name: nameIn.value,
    cwdIf: caseRows.map((c) => ({ at: c.at.value, to: c.to.value })),
    cwd: dirR.checked ? cwdIn.value : "",
    account: isBase() ? "" : acctSel.value,
    base: isBase(),
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
    // 表单没有格子的 ccm 参数：原样带着走（那台后端放回原位）。
    ccmOther: initial.ccmOther,
  });

  /** 控件上把必被拒的组合先关掉（后端校验才是真判定）：接回会话只单放 `--attach`；不进 tmux ⇒ 容器那几格关；不 --detach ⇒ 登记关。 */
  const syncEnabled = (): void => {
    const mode = tmuxSel.value as TmuxMode;
    const attach = mode === "attach";
    const inTmux = mode !== "none" && !attach;
    tmuxHint.textContent = TMUX_NAMING[mode].text();
    tmuxNameIn.disabled = !(mode === "named" || mode === "base");
    for (const c of [acctSel, agentSel, modelIn, launcherIn, passIn, cwdIn, hereR, dirR, addCaseBtn]) c.disabled = attach;
    for (const c of caseRows) c.at.disabled = c.to.disabled = attach;
    cwdIn.disabled = attach || !dirR.checked;
    sizeIn.disabled = !inTmux;
    detachCk.disabled = !inTmux;
    busCk.disabled = !inTmux || !detachCk.checked;
    busNoteIn.disabled = busCk.disabled || !busCk.checked;
  };

  let seq = 0;
  const changed = (): void => {
    syncEnabled();
    const mine = ++seq;
    void host.render(read()).then(
      (a) => {
        if (mine !== seq) return;
        void host.preview(a).then((t) => {
          if (mine === seq) wouldRun.textContent = t;
        });
        void host.check(a).then((lines) => {
          if (mine === seq) notes.textContent = lines.join("\n");
        });
      },
      (e: unknown) => {
        if (mine !== seq) return;
        wouldRun.textContent = "";
        notes.textContent = e instanceof Error ? e.message : String(e);
      },
    );
  };
  for (const c of [nameIn, acctSel, tmuxSel, tmuxNameIn, cwdIn, hereR, dirR, agentSel, modelIn, launcherIn, sizeIn, detachCk, busCk, busNoteIn, passIn]) {
    c.addEventListener("change", () => changed());
  }

  const onSave = async (): Promise<void> => {
    saveBtn.disabled = true;
    let said: string | null;
    try {
      said = await host.save(await host.render(read()));
    } catch (e) {
      said = e instanceof Error ? e.message : String(e);
    }
    saveBtn.disabled = false;
    if (said !== null) notes.textContent = said;
  };

  syncEnabled();
  changed();
  return box;
}

/**
 * 机器卡上的 ②。两个平台同一份，`platform` 是入参；本机远端同一份，`origin` 是入参。
 *
 * @param platform 这台机器用哪种 shell 的方言（本机 = [`localShell`]；远端恒 `posix`）。
 * @param origin 那台机器（取值函数：远端卡改名后跟着它走）。每一发都带它。
 * @param onBlockDone 接入 / 断开之后（远端卡拿它记机器列表那一格；`error` 为空 = 成了）。
 */
export function buildAliasManager(opts: {
  platform: Shell;
  origin: () => Origin;
  onBlockDone?: (verb: "install" | "remove", error: string | null) => void;
  /** 改执行策略之前问一句的注入缝（缺省走应用内对话框）。 */
  confirm?: ConfirmFn;
}): HTMLElement {
  const shell = opts.platform;
  const hasTmux = shell === "posix";
  // 只本机挂的几格（平台格 · 本机 ccm 入口 · 用系统编辑器打开）问的是 monitor 这台，远端不挂。
  const local = isLocalOrigin(opts.origin());
  const wrap = el("details", "ccm-alias-gen machine-aliases");
  wrap.dataset.shell = shell;
  wrap.dataset.origin = opts.origin();
  wrap.appendChild(el("summary", "", copyText("machineAliases.manager.title")));
  wrap.appendChild(el("p", "ccm-alias-gen-hint", copyText("machineAliases.manager.intro")));

  // ═══ 终端接入（在最上，先显示现状）═══
  // 挂钩用 `data-role`：这几块不是设置页的「一组」（那一层的标题由设置页数），外观就是一列。
  const access = el("div", "");
  access.dataset.role = "access";
  access.appendChild(el("div", "settings-label", copyText("machineAliases.access.title")));
  // ⚠ 会被 `hidden` 切的几个元素**不走 `el()`**：类名要在这一行上看得见（`css-conventions` S30 ⑦）。
  const pathCcm = document.createElement("pre");
  pathCcm.className = "ccm-path-ccm";
  pathCcm.hidden = true;
  const accessRows = el("div", "");
  accessRows.dataset.role = "access-rows";
  const accessWarn = el("div", "settings-hint");
  accessWarn.dataset.role = "access-warn";
  const cleanupHint = document.createElement("pre");
  cleanupHint.className = "ccm-rc-block-legacy";
  cleanupHint.hidden = true;
  // 加载这份 `$PROFILE` 的那一代 PowerShell 会不会跑它（执行策略由那台后端现问、判）；不会且不是组策略钉着 ⇒ 给标准做法的按钮。
  const rcPolicy = document.createElement("div");
  rcPolicy.className = "settings-hint";
  rcPolicy.hidden = true;
  const allowBtn = document.createElement("button");
  allowBtn.type = "button";
  allowBtn.className = "settings-btn";
  allowBtn.textContent = copyText("machineAliases.policy.allow");
  allowBtn.hidden = true;
  allowBtn.addEventListener("click", () => void onAllow());
  const accessNote = el("div", "settings-hint");
  accessNote.dataset.role = "access-note";
  // 「不是这份文件？[换一份…]」：候选下拉 ＋ 其它文件，选定之后「接入」装进它。
  const chooser = document.createElement("div");
  chooser.className = "ccm-rc-block";
  chooser.hidden = true;
  const chooseRow = el("div", "settings-row");
  chooseRow.append(
    el("span", "settings-hint", copyText("machineAliases.access.notThisFile")),
    button(copyText("machineAliases.access.choose"), "", () => {
      chooser.hidden = !chooser.hidden;
    }),
    el("span", "settings-hint", copyText("machineAliases.access.selfPasteAsk")),
    button(copyText("machineAliases.access.selfPaste"), "", () => void onSelfPaste()),
  );
  const rcSel = el("select", "ccm-acct-alias-rc");
  const otherIn = el("input", "settings-input settings-input-wide ccm-rc-other");
  otherIn.type = "text";
  otherIn.placeholder = copyText("machineAliases.rc.other");
  const otherErr = el("div", "settings-hint");
  const chooserBtns = el("div", "settings-cc-profile-buttons");
  chooserBtns.append(
    button(copyText("machineAliases.rc.useOther"), "", () => void onOther()),
    ...(local
      ? [Object.assign(button(copyText("machineAliases.rc.open"), "", () => void onOpenRc()), { title: copyText("machineAliases.rc.openHint") })]
      : []),
  );
  chooser.append(rcSel, otherIn, chooserBtns, otherErr);
  // 「我自己贴」：给的是接入那两三行（与「接入」那一跳同一份渲染），不是整份清单。
  let pasteText = "";
  const paste = buildPasteBlock({
    text: () => pasteText,
    target: shell === "powershell" ? copyText("machineAliases.powershell.pasteTarget") : copyText("machineAliases.posix.pasteTarget"),
    mergeNote: copyText("machineAliases.paste.mergeNote"),
    activation: shell === "powershell" ? copyText("machineAliases.powershell.pasteActivation") : copyText("machineAliases.posix.pasteActivation"),
    invalidReason: () => (pasteText ? null : copyText("machineAliases.invalid.notYet")),
    multiline: true,
    rows: 4,
    className: "ccm-alias-gen-out",
  });
  const pasteBox = document.createElement("div");
  pasteBox.className = "ccm-rc-paste";
  pasteBox.hidden = true;
  pasteBox.appendChild(paste.element);
  // PowerShell 那一侧不随启动文件走的两格：握手终端数 ＋ 自动打开 monitor · 用户级 PATH（一构造就问后端 ⇒ 第一次展开才建）。
  const psSlot = el("div", "ccm-ps-slot");
  access.append(pathCcm, accessRows, accessWarn, cleanupHint, rcPolicy, allowBtn, accessNote, chooseRow, chooser, pasteBox, psSlot);
  wrap.appendChild(access);
  let psExtras: PsExtras | null = null;

  // ═══ 清单 ═══
  const listHead = el("div", "settings-row settings-row-actions");
  listHead.append(
    el("span", "settings-label", copyText("machineAliases.list.title")),
    button(copyText("machineAliases.list.add"), "settings-btn-primary", () => openForm(null)),
    Object.assign(button(copyText("machineAliases.rc.rescan"), "", () => void readBack()), {
      title: copyText("machineAliases.rc.rescanHint"),
    }),
  );
  const status = el("div", "settings-hint machine-aliases-status");
  const problemsBox = el("div", "settings-hint machine-aliases-problems");
  const newSlot = el("div", "");
  newSlot.dataset.role = "new-slot";
  const listBox = el("div", "machine-aliases-list");
  wrap.append(listHead, status, problemsBox, newSlot, listBox, el("p", "settings-hint", copyText("machineAliases.list.saveHint")));

  // ── 状态 ──
  let list: Alias[] = [];
  let groups: Array<AccountShape | null> = [];
  let accounts: string[] = [];
  let missing: MissingAlias[] = [];
  let fingerprint: string | null = null;
  let cands: StartupFile[] = [];
  /** 接入装进哪一份（人在「换一份」里选的；没选 ⇒ 已接入的那份，再没有 ⇒ 第一份候选）。 */
  let chosen: string | null = null;
  /** 人另指的那一份（原样的输入串；后端过围栏）。一旦指过，之后每次读回都带着它。 */
  let otherRc: string | null = null;
  /** 正开着的那张表单：`orig` = 正在改的那一条（新增 ⇒ `null`）。 */
  let form: { orig: Alias | null; el: HTMLElement } | null = null;

  /** 存一份新清单：带读回时的指纹；成了 ⇒ 重读；被别处改过 ⇒ 重读、回那句话（表单留着）。 */
  const store = async (next: Alias[]): Promise<string | null> => {
    try {
      await installAliases(opts.origin(), next, shell, fingerprint);
    } catch (e) {
      await readBack();
      return e instanceof AliasesStale
        ? copyText("machineAliases.save.stale", { why: e.message })
        : copyText("machineAliases.write.failed", { e: String(e instanceof Error ? e.message : e) });
    }
    await readBack();
    return null;
  };

  const closeForm = (): void => {
    form?.el.remove();
    form = null;
  };

  /**
   * 「＋ 新增别名」（`orig = null`，在清单顶上展开）与「改」（在那一行下面展开）共用同一张表单。
   * 「改」先问那台后端把这一条摊成表单（打不开 ⇒ 状态行说一句）；新增是一张空表单，就地展开。
   */
  let opening = 0;
  const openForm = (orig: Alias | null, after?: HTMLElement): void => {
    closeForm();
    const ticket = ++opening;
    if (!orig) {
      mountForm(null, emptyForm());
      return;
    }
    void aliasToForm(opts.origin(), orig).then(
      (initial) => {
        if (ticket === opening) mountForm(orig, initial, after);
      },
      (e: unknown) => {
        if (ticket === opening)
          status.textContent = copyText("machineAliases.form.openFailed", { e: e instanceof Error ? e.message : String(e) });
      },
    );
  };
  const mountForm = (orig: Alias | null, initial: AliasForm, after?: HTMLElement): void => {
    closeForm();
    const f = buildAliasForm(initial, {
      shell,
      hasTmux,
      accounts: () => accounts,
      render: (form) => aliasFromForm(opts.origin(), form, orig),
      check: async (a) => {
        const at = orig ? list.findIndex((x) => sameAlias(x, orig)) : -1;
        const next = at >= 0 ? list.map((x, i) => (i === at ? a : x)) : [...list, a];
        try {
          const r = await renderAliases(opts.origin(), next, shell);
          return [
            ...r.problems.filter((p) => p.name === a.name).map((p) => `✗ ${p.message}`),
            ...r.collisions.filter((c) => c.includes(a.name)).map((c) => `⚠ ${c}`),
          ];
        } catch (e) {
          return [copyText("machineAliases.changed.failed", { e: String(e instanceof Error ? e.message : e) })];
        }
      },
      preview: (a) => previewAlias(opts.origin(), a),
      save: async (a) => {
        const at = form?.orig ? list.findIndex((x) => sameAlias(x, form!.orig!)) : -1;
        const next = at >= 0 ? list.map((x, i) => (i === at ? a : x)) : [...list, a];
        const said = await store(next);
        if (said === null) closeForm();
        return said;
      },
      cancel: closeForm,
    });
    form = { orig, el: f };
    if (after) after.after(f);
    else newSlot.appendChild(f);
  };

  /** 清单的一行：名字 · 参数；点这一行展开「会执行什么」；[改] [删]。 */
  const row = (a: Alias): HTMLElement => {
    const r = el("div", "settings-row machine-aliases-row");
    r.append(el("code", "", a.name), el("span", "settings-hint", describeArgs(a.args)));
    // 点了才建、才挂（不用 `hidden` 切：这一格的外观没有自己的规则）。
    const previewOut = document.createElement("pre");
    previewOut.dataset.role = "alias-preview";
    r.title = copyText("machineAliases.aliasPreview.hint");
    r.addEventListener("click", () => {
      if (previewOut.isConnected) {
        previewOut.remove();
        return;
      }
      r.after(previewOut);
      previewOut.textContent = copyText("machineAliases.aliasPreview.asking");
      void previewAlias(opts.origin(), a).then((t) => {
        previewOut.textContent = t;
      });
    });
    r.append(
      button(copyText("machineAliases.list.edit"), "", () => openForm(a, previewOut.isConnected ? previewOut : r)),
      button(copyText("machineAliases.list.delete"), "", () => {
        void store(list.filter((x) => !sameAlias(x, a))).then((said) => {
          if (said !== null) status.textContent = said;
        });
      }),
    );
    return r;
  };

  /** 两组：账号（后端认的那一形，按账号表的顺序、同一个号先当前终端后 tmux；缺的给「加上」）· 其他（其余全部，按文件里的顺序）。 */
  const renderList = (): void => {
    listBox.textContent = "";
    const acctBox = el("div", "");
    acctBox.dataset.role = "group-accounts";
    acctBox.append(
      el("div", "settings-label", copyText("machineAliases.list.groupAccounts")),
      el("div", "settings-hint", copyText("machineAliases.list.groupAccountsHint")),
    );
    const otherBox = el("div", "");
    otherBox.dataset.role = "group-other";
    otherBox.appendChild(el("div", "settings-label", copyText("machineAliases.list.groupOther")));
    const order = (acc: string): number => {
      const i = accounts.indexOf(acc);
      return i < 0 ? accounts.length : i;
    };
    const mine = list
      .map((a, i) => ({ a, g: groups[i] ?? null }))
      .filter((x): x is { a: Alias; g: AccountShape } => x.g !== null);
    const keyOf = (acc: string, tmux: boolean): [number, string, number] => [order(acc), acc, tmux ? 1 : 0];
    const cmp = (x: [number, string, number], y: [number, string, number]): number =>
      x[0] - y[0] || x[1].localeCompare(y[1]) || x[2] - y[2];
    const acctRows: Array<{ key: [number, string, number]; el: HTMLElement }> = [
      ...mine.map(({ a, g }) => ({ key: keyOf(g.account, g.tmux), el: row(a) })),
      ...missing.map((m) => {
        const r = el("div", "settings-row machine-aliases-row");
        r.dataset.role = "alias-missing";
        r.append(
          el("span", "settings-hint", copyText("machineAliases.list.missing", { account: m.account, name: m.alias.name, what: shapeWhat(m.tmux) })),
          button(copyText("machineAliases.list.addMissing"), "", () => {
            void store([...list, m.alias]).then((said) => {
              if (said !== null) status.textContent = said;
            });
          }),
        );
        return { key: keyOf(m.account, m.tmux), el: r };
      }),
    ];
    acctRows.sort((x, y) => cmp(x.key, y.key));
    for (const r of acctRows) acctBox.appendChild(r.el);
    if (!acctRows.length) acctBox.appendChild(el("div", "settings-hint", copyText("machineAliases.list.noAccounts")));
    list.forEach((a, i) => {
      if (!groups[i]) otherBox.appendChild(row(a));
    });
    if (!list.some((_, i) => !groups[i])) otherBox.appendChild(el("div", "settings-hint", copyText("machineAliases.list.empty")));
    listBox.append(acctBox, otherBox);
  };

  /** 整份清单的问题与撞名（只出声、不拦：`cc` 在多数机器上是 C 编译器，盖不盖由人定）。 */
  const renderProblems = async (): Promise<void> => {
    let r: AliasRender;
    try {
      r = await renderAliases(opts.origin(), list, shell);
    } catch (e) {
      problemsBox.textContent = copyText("machineAliases.changed.failed", { e: String(e instanceof Error ? e.message : e) });
      return;
    }
    problemsBox.textContent = [
      ...r.problems.map((p) => `✗ ${p.name}：${p.message}`),
      ...r.collisions.map((c) => `⚠ ${c}`),
    ].join("\n");
  };

  /** 接入装进哪一份。 */
  const target = (): StartupFile | undefined =>
    cands.find((c) => c.path === chosen) ?? cands.find((c) => c.block.present) ?? cands[0];

  /** 接入那一格照读回口的候选画：已接入的每份一行 ＋ [断开]；一份都没有 ⇒ 醒目的「接入 …」。不发 IPC。 */
  const renderAccess = (): void => {
    accessRows.textContent = "";
    const on = cands.filter((c) => c.block.present);
    for (const c of on) {
      const r = el("div", "settings-cc-profile-status");
      const ver = c.block.version ? `（${c.block.version}）` : "";
      const badge = el(
        "span",
        "settings-cc-profile-badge",
        c.block.outdated
          ? copyText("machineAliases.access.outdated", { path: c.path, version: ver })
          : c.policy?.loads === false
            ? copyText("machineAliases.access.notLoaded", { path: c.path, ps: psName(c.policy.host) })
            : copyText("machineAliases.access.on", { path: c.path }),
      );
      badge.classList.add(c.block.outdated || c.policy?.loads === false ? "settings-cc-badge-warn" : "settings-cc-badge-ok");
      r.appendChild(badge);
      if (c.block.outdated) r.appendChild(button(copyText("machineAliases.access.reconnect"), "", () => void runRc("install", c.path)));
      const off = button(local ? copyText("machineAliases.rc.uninstall") : copyText("machineCard.aliases.uninstall"), "", () => void runRc("remove", c.path));
      off.title = copyText("machineAliases.rc.uninstallHint");
      r.appendChild(off);
      accessRows.appendChild(r);
    }
    const t = target();
    if (!on.length) {
      const r = el("div", "settings-cc-profile-status");
      r.appendChild(el("span", "settings-cc-profile-badge settings-cc-badge-warn", copyText("machineAliases.access.off")));
      if (t) {
        const b = button(copyText("machineAliases.access.connectTo", { path: t.path }), "settings-btn-primary", () => void runRc("install", t.path));
        b.title = shell === "powershell" ? copyText("machineAliases.powershell.blockInstallTitle") : copyText("machineAliases.posix.blockInstallTitle");
        r.appendChild(b);
      }
      accessRows.appendChild(r);
    } else if (t && !t.block.present) {
      // 人在「换一份」里另选了一份 ⇒ 也给它一颗「接入」。
      accessRows.appendChild(button(copyText("machineAliases.access.connectTo", { path: t.path }), "", () => void runRc("install", t.path)));
    }
    const warn: string[] = [];
    for (const c of cands) {
      if (c.unreadable) warn.push(copyText("machineAliases.rcStatus.unreadable", { path: c.path, why: c.unreadable }));
      for (const f of c.block.conflictingFunctions)
        warn.push(copyText("machineAliases.access.clash", { path: c.path, line: String(f.line), name: f.name }));
    }
    if (on.length > 1) warn.push(copyText("machineAliases.rcStatus.duplicates", { others: on.map((c) => c.path).join("\n") }));
    accessWarn.textContent = warn.join("\n");
    // 「你配置里这几行是旧的」：后端逐行指名，产品自己一个字节都不删。
    const hint = cands.map((c) => c.block.manualCleanupHint).filter(Boolean).join("\n");
    cleanupHint.hidden = !hint;
    cleanupHint.textContent = hint;
    showPolicy((t?.block.present ? t : on[0])?.policy ?? t?.policy ?? null);
    fillRcOptions();
  };

  const fillRcOptions = (): void => {
    rcSel.textContent = "";
    for (const c of cands) {
      const tags: string[] = [];
      if (c.block.present) tags.push(copyText("machineAliases.rc.tagBlock"));
      else if (c.sourced) tags.push(copyText("machineAliases.rc.tagSourced"));
      if (!c.exists) tags.push(copyText("machineAliases.rc.tagNew"));
      const o = el("option", "", tags.length ? `${c.path}（${tags.join("；")}）` : c.path);
      o.value = c.path;
      rcSel.appendChild(o);
    }
    const t = target();
    if (t) rcSel.value = t.path;
  };
  // 只在人真的换了下拉时刷新 —— `fillRcOptions` 重建选项那一下是程序改值，不触发 `change`。
  rcSel.addEventListener("change", () => {
    chosen = rcSel.value || null;
    pasteBox.hidden = true;
    renderAccess();
  });

  /** 握手终端数住 monitor 进程里（不是那台盘上的事实）⇒ 另问 monitor；只有本机 PowerShell 那一格显示它。 */
  const refreshBound = (): void => {
    if (!psExtras || !local) return;
    void commands.bound_terminal_count().then(
      (n) => psExtras?.setBound(n),
      () => undefined,
    );
  };

  /** 读回口：清单 · 归组 · 账号表 · 缺的 · 指纹 · 启动文件候选一次到；认不出的行原样说出来（下一次存时它会被去掉）。 */
  const readBack = async (): Promise<void> => {
    try {
      const got = await readAliases(opts.origin(), shell, otherRc);
      list = got.aliases;
      groups = got.groups;
      accounts = got.accounts;
      missing = got.missing;
      fingerprint = got.fingerprint;
      cands = got.rcCandidates;
      const head = got.exists
        ? copyText("machineAliases.readBack.count", { path: got.aliasPath, n: got.aliases.length })
        : copyText("machineAliases.readBack.missing", { path: got.aliasPath });
      const bad = got.unparsed.map((u) => copyText("machineAliases.readBack.unknownLine", { u }));
      status.textContent = [head, ...bad].join("\n");
      refreshBound();
    } catch (e) {
      status.textContent = copyText("machineAliases.readBack.failed", { e: String(e instanceof Error ? e.message : e) });
    }
    renderList();
    renderAccess();
    await renderProblems();
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
    await readBack();
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
      if (got.otherRc) chosen = got.otherRc;
      refreshBound();
    } catch (e) {
      otherErr.textContent = copyText("machineAliases.other.failed", { e: String(e instanceof Error ? e.message : e) });
    }
    renderAccess();
  };

  /** 接入 / 断开：装 / 卸别名块（方言由那份文件的扩展名定，后端判）。成功失败都重读：盘上现在是什么样，就显示什么样。 */
  const runRc = async (verb: "install" | "remove", path: string): Promise<void> => {
    accessNote.textContent = verb === "install" ? copyText("machineAliases.runRc.installing") : copyText("machineAliases.runRc.removing");
    let failed: string | null = null;
    try {
      if (verb === "install") await installAliasBlock(opts.origin(), path);
      else await removeAliasBlock(opts.origin(), path);
    } catch (e) {
      const why = String(e instanceof Error ? e.message : e);
      failed =
        verb === "install"
          ? copyText("machineAliases.runRc.installFailed", { e: why })
          : copyText("machineAliases.runRc.removeFailed", { e: why });
    }
    opts.onBlockDone?.(verb, failed);
    await readBack();
    const pol = cands.find((c) => c.path === path)?.policy ?? null;
    accessNote.textContent =
      failed ??
      (verb !== "install"
        ? ""
        : pol === null || pol.loads === true
          ? shell === "powershell"
            ? copyText("machineAliases.powershell.blockAfterInstall")
            : copyText("machineAliases.posix.blockAfterInstall")
          : copyText("machineAliases.policy.afterInstall"));
  };

  /** 「我自己贴」：问后端要接入那几行（与「接入」那一跳同一份渲染），给人自己贴。 */
  const onSelfPaste = async (): Promise<void> => {
    const t = target();
    if (!t) return;
    try {
      pasteText = await renderAliasBlock(opts.origin(), t.path);
    } catch (e) {
      pasteText = "";
      showActionFailureToast(copyText("machineAliases.preview.failed"), String(e instanceof Error ? e.message : e));
    }
    pasteBox.hidden = false;
    paste.refresh();
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
    allowHost = p?.host ?? null;
  };
  let allowHost: PsHost | null = null;

  /** 标准做法那颗按钮：先确认（不代改），再请那台后端设、再重读（现状以它现问的为准）。 */
  const onAllow = async (): Promise<void> => {
    if (!allowHost) return;
    const host = allowHost;
    const ps = psName(host);
    if (!(await (opts.confirm ?? askConfirm)(copyText("machineAliases.policy.confirm", { ps })))) return;
    allowBtn.disabled = true;
    let said: string;
    try {
      const r = await allowLocalScripts(opts.origin(), host);
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
    await readBack();
    accessNote.textContent = said;
  };

  const onOpenRc = async (): Promise<void> => {
    const path = target()?.path;
    if (!path) return;
    try {
      await openPath(path);
    } catch (e) {
      showActionFailureToast(copyText("machineAliases.openRc.failed"), copyText("machineAliases.openRc.failedBody", { e: String(e), path }));
    }
  };

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

// 这里原来是远端卡那一半「把本机的别名清单复制过去贴」—— 远端卡换成上面同一个 `buildAliasManager`（`origin` = 那台）⇒ 删。

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

/**
 * **本机 `ccm` 那一格的唯一写点**（K-R117 S2 本机半钉在本文件）。问一次本机那一格（判定与那句话在 monitor
 * `ccm_probe::local_ccm_cell`），`ok` 说得清就记账（两件都成 ⇒ ok；有一件不成 ⇒ fail 并照记那句话；说不清 ⇒ 不写）。
 * 调用方：别名管理器读回 · 设置页机器列表（打开时一次）· 本机那一行「重新对齐」（`fresh`：先作废 PATH 探针那份 5 分钟缓存，手动兜底）。
 * Windows 本机同样问（新开的 PowerShell 里敲 `ccm` 走到哪）。
 */
export async function noteLocalCcm(fresh = false): Promise<LocalCcmEntry> {
  const st = await commands.local_ccm_entry_status(fresh);
  if (typeof st?.ok === "boolean") {
    recordFacet(LOCAL_MACHINE_KEY, "ccm", { kind: st.ok ? "ok" : "fail", detail: st.summary });
  }
  return st;
}
