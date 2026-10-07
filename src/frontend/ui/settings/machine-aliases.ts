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
import { toast } from "../kit/toast"; // `K-R135`：用户级 PATH 那一格的失败要出声
import { buildPasteBlock } from "../paste-block";
import { DEFAULT_AGENT, listAgents } from "../agent-profile";
// 别名六问走通道、那台后端出成品（`../alias-reads`）；类型随成品住那边。
import type {
  AccountShape,
  Alias,
  AliasForm,
  AliasRender,
  ClashWins,
  CwdCase,
  ExecPolicy,
  MissingAlias,
  NameClash,
  PsHost,
  StartupFile,
  Shell,
  TmuxMode,
} from "../alias-reads";
import { confirmDialog, type ConfirmFn } from "../kit/dialog";
import { toggleSwitch } from "../kit/switch";
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
import { cfgRow, type CfgDot, type CfgRow } from "./cfg-row";
import { homeShort } from "../kit/path";
import { accountAvatarEl } from "../account-color";
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


/**
 * 一条别名的那张表单（「＋ 新增别名」与「改」共用）：名字 · 账号（那台的账号表）· 在哪起 · 工作目录（分情况 ＋ 其余情况）·
 * ▸ 更多；下一行实时「会执行：…」与这一条的问题 / 撞名；[保存] 一步写到那台、[取消]。
 * 每改一格（`change`）问一次后端 —— 没有定时器，每一次都是一个人的动作触发的。
 */
interface FormHost {
  shell: Shell;
  hasTmux: boolean;
  /** 表单头上那一句（「新增别名」·「改 alphacc」）。 */
  title: string;
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
  const box = el("div", "cfg-form");
  box.dataset.role = "alias-form";
  const grid = el("div", "cfg-fg");
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
  // 挂钩用 `data-role` 不用类名：这一行的外观就是 `.cfg-hint`。
  const tmuxHint = el("div", "cfg-hint");
  tmuxHint.dataset.role = "tmux-naming";
  const field = (label: string, ...ctl: HTMLElement[]): void => {
    const cell = el("div", "cfg-fc");
    cell.append(...ctl);
    grid.append(el("div", "cfg-fl", label), cell);
  };
  field(copyText("machineAliases.form.labelName"), nameIn, el("div", "cfg-hint", host.shell === "powershell" ? copyText("machineAliases.form.nameHelpPs") : copyText("machineAliases.form.nameHelp")));
  field(copyText("machineAliases.form.labelAccount"), acctSel);
  field(copyText("machineAliases.form.labelWhere"), tmuxSel, tmuxNameIn, tmuxHint);

  // ── 工作目录：分情况 ＋ 其余情况 ──
  const cwdBox = el("div", "cfg-cases");
  cwdBox.dataset.role = "cwd";
  const cases = el("div", "");
  const caseRows: Array<{ at: HTMLInputElement; to: HTMLInputElement; row: HTMLElement }> = [];
  const addCase = (c: CwdCase): void => {
    const row = el("div", "cfg-cr");
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
  const elseRow = el("div", "cfg-cr");
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
  field(copyText("machineAliases.form.cwdTitle"), cwdBox);

  // ── ▸ 更多 ──
  const adv = el("details", "ccm-alias-gen");
  adv.appendChild(el("summary", "", copyText("machineAliases.form.more")));
  const advGrid = el("div", "cfg-adv");
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
  field("", adv);
  const wouldRun = el("pre", "ccm-alias-preview");
  wouldRun.dataset.role = "would-run";
  const wouldLabel = el("div", "cfg-hint", copyText("machineAliases.form.wouldRun"));
  const notes = el("div", "cfg-hint");
  notes.dataset.role = "form-notes";
  const actions = el("div", "cfg-acts");
  const saveBtn = button(copyText("machineAliases.form.save"), "settings-btn-primary", () => void onSave());
  const cancelBtn = button(copyText("machineAliases.form.cancel"), "", () => host.cancel());
  actions.append(saveBtn, cancelBtn, el("span", "cfg-hint", host.shell === "powershell" ? copyText("machineAliases.form.saveHintPs") : copyText("machineAliases.form.saveHint")));
  box.append(el("div", "cfg-form-title", host.title), grid, wouldLabel, wouldRun, notes, actions);

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
 * 已经露出来读过的那几组别名（本机那一组与每张远端卡各一组）。别处改了那台的别名清单（账号增删）、
 * 或回到那一页时按机器重读 —— 不然清单与指纹停在第一次读的那一刻，「加上」拿旧指纹去写就被说「被别处改过」。
 */
const loadedManagers = new Map<HTMLElement, { origin: () => Origin; reread: () => void }>();

/** 重读那台机器上已经读过的别名组（没读过的不读；已不在界面上的顺手清掉）。 */
export function rereadAliases(origin: Origin): void {
  for (const [el, m] of loadedManagers) {
    if (!el.isConnected) loadedManagers.delete(el);
    else if (m.origin() === origin) m.reread();
  }
}

/** 同名那一行里「现在敲它起的是哪一个」（后端给码，界面按码取句）。 */
const CLASH_NOW: Record<ClashWins, () => string> = {
  yours: () => copyText("machineAliases.clash.nowYours"),
  list: () => copyText("machineAliases.clash.nowList"),
  unclear: () => copyText("machineAliases.clash.nowUnclear"),
};

/** 点开那一条时同名那一句。 */
const CLASH_DETAIL: Record<ClashWins, (a: { path: string; line: string; name: string }) => string> = {
  yours: (a) => copyText("machineAliases.clash.detailYours", { path: a.path, line: a.line, name: a.name }),
  list: (a) => copyText("machineAliases.clash.detailList", { path: a.path, line: a.line, name: a.name }),
  unclear: (a) => copyText("machineAliases.clash.detailUnclear", { path: a.path, line: a.line, name: a.name }),
};

/** 别名那一组的入参。两个平台同一份，`platform` 是入参；本机远端同一份，`origin` 是入参。 */
export interface AliasManagerSpec {
  /** 这台机器用哪种 shell 的方言（本机 = [`localShell`]；远端恒 `posix`）。 */
  platform: Shell;
  /** 那台机器（取值函数：远端卡改名后跟着它走）。每一发都带它。 */
  origin: () => Origin;
  /** 接上 / 卸载之后（远端卡拿它记机器列表那一格；`error` 为空 = 成了）。 */
  onBlockDone?: (verb: "install" | "remove", error: string | null) => void;
  /** 改执行策略之前问一句的注入缝（缺省走应用内对话框）。 */
  confirm?: ConfirmFn;
}

/** 「终端」那一组：别名一行（默认展开）＋ 本机 Windows 上的「Windows 终端」一行。`load` 第一次露出来时调，`reread` 之后再露出来时调。 */
export interface AliasManager {
  element: HTMLElement;
  load(): void;
  reread(): void;
}

export function buildAliasManager(opts: AliasManagerSpec): AliasManager {
  const shell = opts.platform;
  const hasTmux = shell === "posix";
  // 只本机挂的几格（平台格 · 本机 ccm 入口 · 用系统编辑器打开）问的是 monitor 这台，远端不挂。
  const local = isLocalOrigin(opts.origin());
  const group = el("div", "cfg");
  const row = cfgRow(copyText("machineAliases.manager.title"), true);
  const wrap = row.element;
  wrap.classList.add("machine-aliases");
  wrap.dataset.shell = shell;
  wrap.dataset.origin = opts.origin();
  group.appendChild(wrap);
  const body = row.body;

  // ═══ 接上终端（在最上，先显示现状）═══
  // 挂钩用 `data-role`：外观就是一列。主窗口 ↗［接上终端］带 `connect-terminal` 锚点跳到这里。
  const access = el("div", "cfg-access");
  access.dataset.role = "access";
  access.dataset.anchor = "connect-terminal";
  // ⚠ 会被 `hidden` 切的几个元素**不走 `el()`**：类名要在这一行上看得见（`css-conventions` S30 ⑦）。
  const pathCcm = document.createElement("pre");
  pathCcm.className = "ccm-path-ccm";
  pathCcm.hidden = true;
  const accessRows = el("div", "");
  accessRows.dataset.role = "access-rows";
  // 接上那一行下面就地展开的那一块：看加了什么 · 卸载前看要拿掉什么 · 换一份文件 · 自己贴。
  const panel = document.createElement("div");
  panel.className = "cfg-panel";
  panel.hidden = true;
  const accessWarn = el("div", "cfg-hint");
  accessWarn.dataset.role = "access-warn";
  const cleanupHint = document.createElement("pre");
  cleanupHint.className = "ccm-rc-block-legacy";
  cleanupHint.hidden = true;
  // 加载这份 `$PROFILE` 的那一代 PowerShell 会不会跑它（执行策略由那台后端现问、判）；不会且不是组策略钉着 ⇒ 给标准做法的按钮。
  const rcPolicy = document.createElement("div");
  rcPolicy.className = "cfg-hint";
  rcPolicy.hidden = true;
  const allowBtn = document.createElement("button");
  allowBtn.type = "button";
  allowBtn.className = "settings-btn";
  allowBtn.textContent = copyText("machineAliases.policy.allow");
  allowBtn.hidden = true;
  allowBtn.addEventListener("click", () => void onAllow());
  const accessNote = el("div", "cfg-hint");
  accessNote.dataset.role = "access-note";
  // 「换一份文件」：候选（单选）＋ 其它文件，选定之后「接到这份」。
  const chooser = document.createElement("div");
  chooser.className = "ccm-rc-block";
  chooser.dataset.role = "chooser";
  const rcSel = el("select", "ccm-acct-alias-rc");
  const otherIn = el("input", "settings-input settings-input-wide ccm-rc-other");
  otherIn.type = "text";
  otherIn.placeholder = copyText("machineAliases.rc.other");
  const otherErr = el("div", "cfg-hint");
  const chooserBtns = el("div", "cfg-acts");
  chooserBtns.append(
    button(copyText("machineAliases.rc.useOther"), "", () => void onOther()),
    ...(local
      ? [Object.assign(button(copyText("machineAliases.rc.open"), "", () => void onOpenRc()), { title: copyText("machineAliases.rc.openHint") })]
      : []),
  );
  chooser.append(rcSel, otherIn, chooserBtns, otherErr);
  // 「自己贴」：给的是接上那几行（与「接上」那一跳同一份渲染），不是整份清单。
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
  pasteBox.appendChild(paste.element);
  access.append(pathCcm, accessRows, panel, accessWarn, cleanupHint, rcPolicy, allowBtn, accessNote);
  body.appendChild(access);

  // 块外与清单同名的那一行：现在敲它起的是哪一个（后端比的先后，界面按码取句）。
  const clashBox = document.createElement("div");
  clashBox.className = "cfg-clash";
  clashBox.hidden = true;
  const clash = el("div", "cfg-line cfg-line-warn");
  clash.dataset.role = "clash";
  clashBox.appendChild(clash);
  body.appendChild(clashBox);

  // ═══ 清单 ═══
  const listHead = el("div", "cfg-list-head");
  listHead.append(
    el("span", "cfg-list-title", copyText("machineAliases.list.title")),
    button(copyText("machineAliases.list.add"), "", () => openForm(null)),
  );
  const status = el("div", "cfg-hint machine-aliases-status");
  const problemsBox = el("div", "cfg-hint machine-aliases-problems");
  const newSlot = el("div", "");
  newSlot.dataset.role = "new-slot";
  const listBox = el("div", "machine-aliases-list");
  const writes = document.createElement("div");
  writes.className = "cfg-writes";
  writes.dataset.role = "writes";
  body.append(listHead, status, problemsBox, newSlot, listBox, writes);

  // ═══ Windows 终端（只本机 PowerShell）═══
  let winRow: CfgRow | null = null;
  let psExtras: PsExtras | null = null;
  if (shell === "powershell" && local) {
    winRow = cfgRow(copyText("machineAliases.win.title"), false);
    winRow.element.dataset.role = "win-row";
    group.appendChild(winRow.element);
  }

  // ── 状态 ──
  let list: Alias[] = [];
  let groups: Array<AccountShape | null> = [];
  let said: string[] = [];
  let accounts: string[] = [];
  let missing: MissingAlias[] = [];
  let fingerprint: string | null = null;
  let cands: StartupFile[] = [];
  let home: string | null = null;
  /** 路径写成家目录打头的短形（家目录由那台后端给）。 */
  const short = (p: string): string => homeShort(p, home);
  let loadedOnce = false;
  /** 接上装进哪一份（人在「换一份文件」里选的；没选 ⇒ 已接上的那份，再没有 ⇒ 第一份候选）。 */
  let chosen: string | null = null;
  /** 人另指的那一份（原样的输入串；后端过围栏）。一旦指过，之后每次读回都带着它。 */
  let otherRc: string | null = null;
  /** 正开着的那张表单：`orig` = 正在改的那一条（新增 ⇒ `null`）。 */
  let form: { orig: Alias | null; el: HTMLElement } | null = null;
  /** 点开看「等于什么 · 会执行什么」的那一条（按名字记，重读后照样开着）。 */
  let selected: string | null = null;
  /** 接上那一行下面开着哪一块。 */
  let panelOpen: "preview" | "uninstall" | "choose" | "paste" | null = null;

  /** 存一份新清单：带读回时的指纹；成了 ⇒ 重读；被别处改过 ⇒ 重读、回那句话（表单留着）。 */
  const store = async (next: Alias[]): Promise<string | null> => {
    try {
      const done = await installAliases(opts.origin(), next, shell, fingerprint);
      // 写进了别名文件 ⇒ 照后端回的那一句说「已开的终端要重读别名」（新开的终端自己认得）。
      if (done.reload) toast(copyText("machineAliases.save.done"), done.reload, { level: "info" });
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
    row.setOpen(true);
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
      title: orig ? copyText("machineAliases.form.titleEdit", { name: orig.name }) : copyText("machineAliases.form.titleNew"),
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

  /** 这一条在块外有没有同名的（取第一处）。 */
  const clashOf = (name: string): { path: string; c: NameClash } | null => {
    for (const s of cands) for (const c of s.block.conflictingFunctions) if (c.name === name) return { path: s.path, c };
    return null;
  };

  /** 点开一条：敲它等于什么 · 那台现算的会执行什么 · 和你写的同名时那一句 ·［改］［删］。 */
  const detail = (a: Alias, i: number, withHead: boolean): HTMLElement => {
    const d = el("div", "cfg-detail");
    d.dataset.role = "alias-detail";
    if (withHead) {
      const h = el("div", "cfg-detail-head");
      h.append(el("code", "cfg-alias-name", a.name), el("span", "cfg-alias-said", said[i] ?? ""));
      h.append(...actions(a, () => d));
      d.appendChild(h);
    }
    const kv = el("div", "cfg-kv");
    kv.append(
      el("span", "", copyText("machineAliases.detail.equals", { name: a.name })),
      el("code", "", copyText("machineAliases.detail.equalsLine", { args: describeArgs(a.args) })),
      el("span", "", copyText("machineAliases.detail.runs")),
    );
    const runs = el("code", "ccm-alias-preview");
    runs.dataset.role = "alias-preview";
    runs.textContent = copyText("machineAliases.aliasPreview.asking");
    kv.appendChild(runs);
    d.append(kv, el("div", "cfg-hint", copyText("machineAliases.aliasPreview.hint")));
    void previewAlias(opts.origin(), a).then((t) => {
      runs.textContent = t;
    });
    const hit = clashOf(a.name);
    if (hit) {
      d.appendChild(
        el("div", "cfg-note cfg-note-warn", CLASH_DETAIL[hit.c.wins]({ path: short(hit.path), line: String(hit.c.line), name: a.name })),
      );
    }
    return d;
  };

  const actions = (a: Alias, after: () => HTMLElement): HTMLElement[] => [
    button(copyText("machineAliases.list.edit"), "cfg-link", () => openForm(a, after())),
    button(copyText("machineAliases.list.delete"), "cfg-link cfg-link-danger", () => {
      void store(list.filter((x) => !sameAlias(x, a))).then((said) => {
        if (said !== null) status.textContent = said;
      });
    }),
  ];

  /** 「其他」里一行：名字 · 人话（被你写的盖 / 和你写的同名）·［改］［删］；点这一行展开它的详情。 */
  const otherRow = (a: Alias, i: number): HTMLElement => {
    const box = el("div", "cfg-arow-wrap");
    const r = el("div", "cfg-arow machine-aliases-row");
    r.dataset.name = a.name;
    r.tabIndex = 0;
    const s = el("span", "cfg-alias-said", said[i] ?? "");
    const hit = clashOf(a.name);
    if (hit) s.appendChild(el("span", "cfg-alias-warn", hit.c.wins === "yours" ? copyText("machineAliases.clash.tagCovered") : copyText("machineAliases.clash.tagSame")));
    r.append(el("code", "cfg-alias-name", a.name), s);
    const acts = el("span", "cfg-arow-acts");
    acts.append(...actions(a, () => box));
    r.appendChild(acts);
    const open = (): void => {
      selected = selected === a.name ? null : a.name;
      renderList();
    };
    r.addEventListener("click", open);
    r.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.isComposing) open();
    });
    r.setAttribute("aria-expanded", String(selected === a.name));
    box.appendChild(r);
    if (selected === a.name) box.appendChild(detail(a, i, false));
    return box;
  };

  /** 账号表：号 × 「在当前终端启动」「在 tmux 里起」（PowerShell 只一列），格子里是别名名字；缺的那一格「＋ 添加 …」。 */
  const accountTable = (): HTMLElement => {
    const box = el("div", "");
    box.dataset.role = "group-accounts";
    const grp = el("div", "cfg-grp");
    grp.append(
      el("span", "cfg-grp-title", copyText("machineAliases.list.groupAccounts")),
      el("span", "cfg-hint", hasTmux ? copyText("machineAliases.list.groupAccountsHint") : copyText("machineAliases.list.groupAccountsHintOne")),
    );
    box.appendChild(grp);
    if (!accounts.length) {
      box.appendChild(el("div", "cfg-hint", copyText("machineAliases.list.noAccounts")));
      return box;
    }
    const tab = el("div", hasTmux ? "cfg-atab" : "cfg-atab cfg-atab-one");
    tab.append(
      el("div", "cfg-th", copyText("machineAliases.list.colAccount")),
      el("div", "cfg-th", shell === "powershell" ? copyText("machineAliases.list.colHereWindow") : copyText("machineAliases.list.colHere")),
    );
    if (hasTmux) tab.appendChild(el("div", "cfg-th", copyText("machineAliases.list.colTmux")));
    let picked: { a: Alias; i: number } | null = null;
    const cell = (acc: string, tmux: boolean): HTMLElement => {
      const td = el("div", "cfg-td");
      list.forEach((a, i) => {
        const g = groups[i];
        if (!g || !inTable(i) || g.account !== acc || g.tmux !== tmux) return;
        const p = button(a.name, "cfg-pill", () => {
          selected = selected === a.name ? null : a.name;
          renderList();
        });
        p.dataset.name = a.name;
        p.setAttribute("aria-pressed", String(selected === a.name));
        if (selected === a.name) picked = { a, i };
        td.appendChild(p);
      });
      if (!td.childElementCount) {
        const m = missing.find((x) => x.account === acc && x.tmux === tmux);
        if (m) {
          const add = button(copyText("machineAliases.list.addMissing", { name: m.alias.name }), "cfg-pill cfg-pill-add", () => {
            void store([...list, m.alias]).then((said) => {
              if (said !== null) status.textContent = said;
            });
          });
          add.dataset.role = "alias-missing";
          td.appendChild(add);
        }
      }
      return td;
    };
    for (const acc of accounts) {
      const who = el("div", "cfg-td cfg-who");
      who.append(accountAvatarEl(acc, { size: 18 }), el("span", "cfg-acct", acc));
      tab.append(who, cell(acc, false));
      if (hasTmux) tab.appendChild(cell(acc, true));
    }
    box.appendChild(tab);
    const p = picked as { a: Alias; i: number } | null;
    if (p) box.appendChild(detail(p.a, p.i, true));
    return box;
  };

  /** 账号表里那一格画不画它：后端认的账号那一形；这台没有 tmux（表只一列）时 tmux 那一形落回「其他」，不藏起来。 */
  const inTable = (i: number): boolean => groups[i] !== null && groups[i] !== undefined && (hasTmux || !groups[i]!.tmux);

  /** 两组：账号（后端认的那一形，排成表）· 其他（其余全部，按文件里的顺序）。 */
  const renderList = (): void => {
    listBox.textContent = "";
    const otherBox = el("div", "");
    otherBox.dataset.role = "group-other";
    otherBox.appendChild(el("div", "cfg-grp cfg-grp-title", copyText("machineAliases.list.groupOther")));
    const rows = el("div", "cfg-arows");
    list.forEach((a, i) => {
      if (!inTable(i)) rows.appendChild(otherRow(a, i));
    });
    if (!rows.childElementCount) rows.appendChild(el("div", "cfg-hint", copyText("machineAliases.list.empty")));
    otherBox.appendChild(rows);
    listBox.append(accountTable(), otherBox);
    if (form?.orig && !form.el.isConnected) closeForm();
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

  /** 接上装进哪一份。 */
  const target = (): StartupFile | undefined =>
    cands.find((c) => c.path === chosen) ?? cands.find((c) => c.block.present) ?? cands[0];

  const linkBtn = (label: string, onClick: () => void, danger = false): HTMLButtonElement =>
    button(label, danger ? "cfg-link cfg-link-danger" : "cfg-link", onClick);

  /** 接上那一行下面那一块（再点同一个就收起）。 */
  const togglePanel = (which: NonNullable<typeof panelOpen>): void => {
    panelOpen = panelOpen === which ? null : which;
    void paintPanel();
  };

  const paintPanel = async (): Promise<void> => {
    const t = target();
    panel.hidden = panelOpen === null;
    panel.replaceChildren();
    if (!panelOpen || !t) return;
    if (panelOpen === "choose") {
      panel.append(el("div", "cfg-panel-title", copyText("machineAliases.rc.chooseTitle")), chooser);
      return;
    }
    if (panelOpen === "paste") {
      panel.appendChild(pasteBox);
      return;
    }
    const code = el("pre", "cfg-code");
    code.dataset.role = "block-text";
    code.textContent = copyText("machineAliases.aliasPreview.asking");
    if (panelOpen === "preview") {
      panel.append(el("div", "cfg-panel-title", t.block.present ? copyText("machineAliases.access.previewOn", { path: short(t.path) }) : copyText("machineAliases.access.previewOff", { path: short(t.path) })), code);
    } else {
      const go = button(copyText("machineAliases.rc.uninstall"), "", () => void runRc("remove", t.path));
      go.title = copyText("machineAliases.rc.uninstallHint");
      const acts = el("div", "cfg-acts");
      acts.append(go, button(copyText("machineAliases.form.cancel"), "", () => togglePanel("uninstall")));
      panel.append(
        el("div", "cfg-sub", shell === "powershell" ? copyText("machineAliases.access.uninstallSubWindow") : copyText("machineAliases.access.uninstallSub")),
        el("div", "cfg-panel-title", copyText("machineAliases.access.uninstallWhat", { path: short(t.path) })),
        code,
        acts,
      );
    }
    try {
      code.textContent = await renderAliasBlock(opts.origin(), t.path);
    } catch (e) {
      code.textContent = copyText("machineAliases.preview.failedLine", { e: String(e instanceof Error ? e.message : e) });
    }
  };

  /** 接上那一格照读回口的候选画：已接上 ⇒ 一行现状 ＋ 看加了什么 · 卸载 ccm；没接上 ⇒ 醒目的「接上 …」＋ 看一眼 · 换一份文件 · 自己贴。不发 IPC。 */
  const renderAccess = (): void => {
    accessRows.textContent = "";
    const on = cands.filter((c) => c.block.present);
    const t = target();
    const lineOf = (dot: CfgDot, text: string, ...tail: HTMLElement[]): HTMLElement => {
      const r = el("div", dot === "ok" ? "cfg-line" : "cfg-line cfg-line-warn");
      r.dataset.role = "access-line";
      const d = el("span", "cfg-dot");
      d.dataset.dot = dot;
      r.append(d, el("span", "cfg-line-text", text), ...tail);
      return r;
    };
    for (const c of on) {
      const p = short(c.path);
      if (c.block.outdated) {
        accessRows.appendChild(
          lineOf("warn", copyText("machineAliases.access.outdated", { path: p }), button(copyText("machineAliases.access.reconnect"), "settings-btn-primary ccm-access-connect", () => void runRc("install", c.path))),
        );
        accessRows.appendChild(el("div", "cfg-hint", copyText("machineAliases.access.outdatedHint")));
        continue;
      }
      const text =
        c.policy?.loads === false
          ? copyText("machineAliases.access.notLoaded", { path: p, ps: psName(c.policy.host) })
          : shell === "powershell"
            ? copyText("machineAliases.access.onWindow", { path: p })
            : copyText("machineAliases.access.on", { path: p });
      accessRows.appendChild(
        lineOf(
          c.policy?.loads === false ? "warn" : "ok",
          text,
          linkBtn(copyText("machineAliases.access.whatAdded"), () => {
            chosen = c.path;
            togglePanel("preview");
          }),
          linkBtn(local ? copyText("machineAliases.rc.uninstall") : copyText("machineCard.aliases.uninstall"), () => {
            chosen = c.path;
            togglePanel("uninstall");
          }, true),
        ),
      );
    }
    if (!on.length || (t && !t.block.present)) {
      const connect = t
        ? button(copyText("machineAliases.access.connectTo", { path: short(t.path) }), "settings-btn-primary ccm-access-connect", () => void runRc("install", t.path))
        : null;
      if (connect) connect.title = shell === "powershell" ? copyText("machineAliases.powershell.blockInstallTitle") : copyText("machineAliases.posix.blockInstallTitle");
      accessRows.appendChild(
        lineOf("off", on.length ? copyText("machineAliases.access.otherFile", { path: short(t!.path) }) : shell === "powershell" ? copyText("machineAliases.access.offWindow") : copyText("machineAliases.access.off"), ...(connect ? [connect] : [])),
      );
      if (t) {
        const hint = el("div", "cfg-hint cfg-access-hint");
        hint.append(
          el("span", "", copyText("machineAliases.access.willAdd", { path: short(t.path) })),
          linkBtn(copyText("machineAliases.access.peek"), () => togglePanel("preview")),
          linkBtn(copyText("machineAliases.access.choose"), () => togglePanel("choose")),
          linkBtn(copyText("machineAliases.access.selfPaste"), () => void onSelfPaste()),
        );
        accessRows.appendChild(hint);
      }
    }
    const warn: string[] = [];
    for (const c of cands) if (c.unreadable) warn.push(copyText("machineAliases.rcStatus.unreadable", { path: c.path, why: c.unreadable }));
    if (on.length > 1) warn.push(copyText("machineAliases.rcStatus.duplicates", { others: on.map((c) => c.path).join("\n") }));
    accessWarn.textContent = warn.join("\n");
    // 「你配置里这几行是旧的」：后端逐行指名，产品自己一个字节都不删。
    const hint = cands.map((c) => c.block.manualCleanupHint).filter(Boolean).join("\n");
    cleanupHint.hidden = !hint;
    cleanupHint.textContent = hint;
    showPolicy((t?.block.present ? t : on[0])?.policy ?? t?.policy ?? null);
    fillRcOptions();
    renderClash();
    renderWrites();
    renderStatus();
    if (panelOpen) void paintPanel();
  };

  /** 同名那一行：名字们 · 在哪份文件 · 现在敲它们起的是哪一个（各自不同 ⇒ 点开那一条看）。 */
  const renderClash = (): void => {
    const hits = new Map<string, { path: string; wins: NameClash["wins"] }>();
    for (const c of cands) for (const f of c.block.conflictingFunctions) if (!hits.has(f.name)) hits.set(f.name, { path: c.path, wins: f.wins });
    clashBox.hidden = hits.size === 0;
    clash.replaceChildren();
    if (!hits.size) return;
    const all = [...hits.values()];
    const wins = new Set(all.map((h) => h.wins));
    const paths = new Set(all.map((h) => h.path));
    const now = wins.size > 1 ? copyText("machineAliases.clash.mixed") : CLASH_NOW[all[0].wins]();
    const d = el("span", "cfg-dot");
    d.dataset.dot = "warn";
    const names = [...hits.keys()].join(copyText("accountsMcp.list.sep"));
    const path = paths.size === 1 ? short(all[0].path) : copyText("machineAliases.clash.manyFiles");
    clash.append(
      d,
      el("span", "cfg-line-text", hits.size > 1 ? copyText("machineAliases.clash.lineMany", { names, path, now }) : copyText("machineAliases.clash.lineOne", { names, path, now })),
    );
  };

  /** 页尾那一句：接上之后会动哪份文件、什么时候动。 */
  const renderWrites = (): void => {
    const t = cands.find((c) => c.block.present) ?? target();
    writes.hidden = !t;
    if (t) writes.textContent = copyText("machineAliases.writes.line", { path: short(t.path) });
  };

  /** 收着时那一行的现状与主动作。 */
  const renderStatus = (): void => {
    const on = cands.filter((c) => c.block.present);
    const n = String(list.length);
    const names = [...new Set(cands.flatMap((c) => c.block.conflictingFunctions.map((f) => f.name)))];
    const tail = names.length ? copyText("machineAliases.status.clashTail", { names: names.join(copyText("accountsMcp.list.sep")) }) : "";
    const t = target();
    let dot: CfgDot;
    let text: string;
    if (on.some((c) => c.block.outdated)) {
      dot = "warn";
      text = copyText("machineAliases.status.outdated", { n, path: short(on.find((c) => c.block.outdated)!.path) });
      row.setAction(button(copyText("machineAliases.status.update"), "settings-btn-primary", () => row.setOpen(true)));
    } else if (on.length) {
      dot = "ok";
      text = shell === "powershell" ? copyText("machineAliases.status.onWindow", { n }) : copyText("machineAliases.status.on", { n });
      row.setAction(button(copyText("machineAliases.status.add"), "", () => openForm(null)));
    } else {
      dot = "off";
      text = shell === "powershell" ? copyText("machineAliases.status.offWindow", { n }) : copyText("machineAliases.status.off", { n });
      row.setAction(t ? button(copyText("machineAliases.status.connect", { path: short(t.path) }), "settings-btn-primary", () => row.setOpen(true)) : null);
    }
    row.setStatus(names.length ? "warn" : dot, text + tail);
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

  /** 读回口：清单 · 归组 · 人话 · 账号表 · 缺的 · 指纹 · 启动文件候选一次到；认不出的行原样说出来（下一次存时它会被去掉）。 */
  const readBack = async (): Promise<void> => {
    try {
      const got = await readAliases(opts.origin(), shell, otherRc);
      home = got.home;
      list = got.aliases;
      groups = got.groups;
      said = got.said;
      accounts = got.accounts;
      missing = got.missing;
      fingerprint = got.fingerprint;
      cands = got.rcCandidates;
      const head = got.exists ? "" : copyText("machineAliases.readBack.missing", { path: short(got.aliasPath) });
      const bad = got.unparsed.map((u) => copyText("machineAliases.readBack.unknownLine", { u }));
      status.textContent = [head, ...bad].filter(Boolean).join("\n");
      refreshBound();
    } catch (e) {
      status.textContent = copyText("machineAliases.readBack.failed", { e: String(e instanceof Error ? e.message : e) });
      row.setStatus("warn", copyText("machineAliases.status.readFailed"));
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

  /** 接上 / 卸载：装 / 卸别名块（方言由那份文件的扩展名定，后端判）。成功失败都重读：盘上现在是什么样，就显示什么样。 */
  const runRc = async (verb: "install" | "remove", path: string): Promise<void> => {
    panelOpen = null;
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

  /** 「自己贴」：问后端要接上那几行（与「接上」那一跳同一份渲染），给人自己贴。 */
  const onSelfPaste = async (): Promise<void> => {
    const t = target();
    if (!t) return;
    try {
      pasteText = await renderAliasBlock(opts.origin(), t.path);
    } catch (e) {
      pasteText = "";
      toast(copyText("machineAliases.preview.failed"), String(e instanceof Error ? e.message : e));
    }
    panelOpen = "paste";
    await paintPanel();
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
    const confirm = {
      title: copyText("machineAliases.policy.title", { ps }),
      action: copyText("machineAliases.policy.action"),
      body: copyText("machineAliases.policy.confirm", { ps }),
    };
    if (!(await (opts.confirm ?? confirmDialog)(confirm))) return;
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
      toast(copyText("machineAliases.openRc.failed"), copyText("machineAliases.openRc.failedBody", { e: String(e), path }));
    }
  };

  row.setStatus("off", copyText("cfgPage.row.reading"));
  renderList();
  const reread = (): void => {
    psExtras?.loadNow();
    void load();
  };
  return {
    element: group,
    load() {
      if (loadedOnce) return reread();
      loadedOnce = true;
      if (winRow) {
        psExtras = buildPsExtras(winRow);
        psExtras.loadNow();
      }
      void load();
      loadedManagers.set(wrap, { origin: opts.origin, reread });
    },
    reread() {
      if (loadedOnce) reread();
    },
  };
}

// 这里原来是远端卡那一半「把本机的别名清单复制过去贴」—— 远端卡换成上面同一个 `buildAliasManager`（`origin` = 那台）⇒ 删。

// ═══════════════════════════════════════════════════════════════════════════
// 「Windows 终端」那一行（只本机 PowerShell）：能切回几个窗口 · 没开时自动打开 · cmd / Git Bash 也认 ccm
// ═══════════════════════════════════════════════════════════════════════════
//
// 别名块那一半（选哪份 `$PROFILE` · 装 / 卸 / 预览 / 现状 · 执行策略）在上面「别名」那一行里，与 POSIX 同一族命令；
// 这一行是不随启动文件走的那三格：
// - 已完成拉前握手的终端数（住 monitor 进程里，另问 monitor）；
// - 用别名起 claude 时 cc-monitor 没开就先打开它（开关，立刻生效）；
// - **用户级 PATH**（`K-R135`：用户逐字「应该让用户手动点击加，也能管理删除」⇒ 开关就是那一下点击，拨回就是删；
//   **现算不缓存**：每次读都真问一趟；**探不动 ≠ 不在 PATH 上**：读不出时开关不给拨、那句原话上屏，不静默成「没加」）。
//   那两条命令的逐字文本给不想拨开关的人看（与开关跑的是同一份字节）。

interface PsExtras {
  setBound(n: number): void;
  loadNow(): void;
}

function buildPsExtras(row: CfgRow): PsExtras {
  const body = row.body;
  let bound: number | null = null;
  let onPath: boolean | null = null;
  const paintStatus = (): void => {
    const n = bound === null ? copyText("machineAliases.ps.empty") : String(bound);
    row.setStatus(
      onPath === null ? "off" : "ok",
      onPath ? copyText("machineAliases.win.statusPathOn", { n }) : copyText("machineAliases.win.statusPathOff", { n }),
    );
  };

  const bind = el("div", "cfg-sw-row");
  bind.dataset.role = "win-bind";
  const bindName = el("span", "cfg-sw-name", copyText("machineAliases.win.bindTitle"));
  const bindHelp = el("span", "cfg-hint", copyText("machineAliases.win.bindHelp", { n: copyText("machineAliases.ps.empty") }));
  bind.append(bindName, bindHelp);

  const auto = toggleSwitch({
    label: copyText("machineAliases.ps.autoLaunch"),
    help: copyText("machineAliases.ps.autoLaunchHint"),
    on: false,
    onChange: async (enabled) => {
      try {
        await commands.cc_set_auto_launch({ enabled });
        return true;
      } catch (e) {
        toast(copyText("machineAliases.ps.saveFailed"), String(e));
        return false;
      }
    },
  });
  auto.root.dataset.role = "win-auto";
  const autoPath = el("div", "cfg-hint");
  autoPath.dataset.role = "win-auto-path";

  const pathSw = toggleSwitch({
    label: copyText("machineAliases.userPath.title"),
    on: false,
    onChange: async (on) => {
      try {
        if (on) await commands.ccm_user_path_add();
        else await commands.ccm_user_path_remove();
      } catch (e) {
        toast(on ? copyText("machineAliases.userPath.addFailed") : copyText("machineAliases.userPath.removeFailed"), String(e));
      }
      // 成功失败都重扫：盘上现在是什么样，就显示什么样（不拿我们以为的结果去写界面）。
      await refreshPath();
    },
  });
  pathSw.root.dataset.role = "win-path";
  const pathHelp = el("div", "cfg-hint ccm-user-path-status");
  const cmdPre = document.createElement("pre");
  cmdPre.className = "ccm-user-path-cmd";
  cmdPre.hidden = true;
  const cmdLink = document.createElement("button");
  cmdLink.type = "button";
  cmdLink.className = "cfg-link";
  cmdLink.textContent = copyText("machineAliases.userPath.cmdNote");
  cmdLink.addEventListener("click", () => {
    cmdPre.hidden = !cmdPre.hidden;
  });
  cmdLink.hidden = true;
  body.append(bind, auto.root, autoPath, pathSw.root, pathHelp, cmdLink, cmdPre);

  const lock = (disabled: boolean): void => {
    if (disabled) pathSw.input.setAttribute("aria-disabled", "true");
    else pathSw.input.removeAttribute("aria-disabled");
  };

  const refreshPath = async (): Promise<void> => {
    try {
      const st = await commands.ccm_user_path_status();
      if (!st.supported) {
        onPath = null;
        lock(true);
        pathHelp.textContent = copyText("machineAliases.userPath.notWindows");
      } else if (st.error) {
        // 🔴 探不动 ≠ 不在 PATH 上：原话上屏，开关不给拨。
        onPath = null;
        lock(true);
        pathHelp.textContent = copyText("machineAliases.userPath.readFailed", { error: st.error });
      } else {
        onPath = st.onUserPath;
        lock(false);
        pathSw.set(st.onUserPath);
        pathHelp.textContent = st.onUserPath
          ? copyText("machineAliases.userPath.on")
          : copyText("machineAliases.userPath.offHelp", { dir: st.dir ?? copyText("machineAliases.userPath.empty") });
        const text = st.onUserPath ? st.removeCommand : st.addCommand;
        cmdPre.textContent = text ?? "";
        cmdLink.hidden = !text;
      }
    } catch (e) {
      onPath = null;
      lock(true);
      pathHelp.textContent = copyText("machineAliases.userPath.readFailed", { error: String(e) });
    }
    paintStatus();
  };

  const refreshAutoLaunch = async (): Promise<void> => {
    try {
      const cfg = await commands.cc_get_auto_launch();
      auto.set(cfg.auto_launch_enabled);
      auto.input.removeAttribute("aria-disabled");
      autoPath.textContent = copyText("machineAliases.win.autoPath", { path: cfg.monitor_exe_path ?? copyText("machineAliases.ps.pathUnknown") });
    } catch (e) {
      console.warn("cc_get_auto_launch failed:", e);
      // 读不到时别把「不知道」画成「关着」：开关不给拨、路径那格说读不到。
      auto.input.setAttribute("aria-disabled", "true");
      autoPath.textContent = copyText("machineAliases.autoLaunch.unreadable", { e: String(e) });
    }
  };

  paintStatus();
  return {
    setBound: (n) => {
      bound = n;
      bindHelp.textContent = copyText("machineAliases.win.bindHelp", { n: String(n) });
      paintStatus();
    },
    loadNow: () => {
      void refreshAutoLaunch();
      void refreshPath();
    },
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
