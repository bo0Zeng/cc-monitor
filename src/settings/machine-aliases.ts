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
 * ② `aliases_install`（唯一的副作用）：同一份渲染落进 `~/.cc-monitor/account-aliases.sh`，
 *    可选地往用户**自己选**的那份 rc 装一行 `source`。
 * 读回口 `aliases_read`：打开这一块时先读盘上那份，清单从它开始编辑。
 *
 * # 纪律（`launcher-diagnostics.ts` 头注那条，原样适用）
 *
 * **绝不在用户没要求时改他的配置**：打开这一块只读；「写入」按钮写明写到哪、写什么；
 * 那份 rc 由人在下拉里选，默认项是「不动我的 shell 配置」。
 * **构造零 I/O**：这一块是个 `<details>`，第一次展开才发第一条 IPC（`70 §1.3 B` · `§8` #3）。
 */
import { commands } from "../ipc/commands";
import { buildPasteBlock } from "../paste-block";
import { ACTIVE_AGENT, listAgents } from "../agent-profile";
import type { Alias } from "../generated/Alias";
import type { AliasRender } from "../generated/AliasRender";
import type { ProfileScan } from "../generated/ProfileScan";
import type { AccountAliasRc } from "../generated/AccountAliasRc";
import { hostOs } from "./host-os";

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
 * 本机那张卡上的 ②。Windows 上换成一句说明 —— PowerShell 那种写法的别名还没做
 * （`71 §12.9` 的 W1「先在真 Windows 上把今天这个 `cc` 跑一趟」没满足，不在没验过的基线上加功能）。
 *
 * @param loadAccounts 「为每个账号加一条」要的账号名。由调用方给（本机那条读口）。
 */
export function buildAliasManager(opts: {
  loadAccounts: () => Promise<string[]>;
}): HTMLElement {
  const wrap = el("details", "ccm-alias-gen machine-aliases");
  wrap.appendChild(el("summary", "", "别名"));
  wrap.appendChild(
    el(
      "p",
      "ccm-alias-gen-hint",
      "一条别名是一个名字加一组 ccm 参数：在终端里敲这个名字，就等于敲 ccm 加上这组参数，后面还能再接别的参数。",
    ),
  );
  if (hostOs() === "windows") {
    wrap.appendChild(
      el(
        "p",
        "settings-hint",
        "这台机器是 Windows：PowerShell 写法的别名还没做。这台机器上的 cc 命令在「终端集成」那一块。",
      ),
    );
    return wrap;
  }

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
  const nameIn = text("名字，如 alphacct", "在终端里敲的那个词");
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
    target: "~/.bashrc（或你实际用的 shell 配置文件）",
    mergeNote: "整段贴到文件末尾；以后改了清单，再贴一次换掉上一次那段。",
    activation: "source 它，或开一个新终端。",
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
  rcRow.append("这台机器的 shell 配置（那一行 source 加进哪份）：", rcSel);
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
      rendered = await commands.aliases_render({ aliases: list });
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

  const fillRcOptions = (cands: readonly AccountAliasRc[]): void => {
    const keep = rcSel.value;
    rcSel.textContent = "";
    const none = el("option", "", "不动我的 shell 配置");
    none.value = "";
    rcSel.appendChild(none);
    for (const c of cands) {
      const o = el("option", "", c.sourced ? `${c.path}（已经接上了）` : c.path);
      o.value = c.path;
      rcSel.appendChild(o);
    }
    if ([...rcSel.options].some((o) => o.value === keep)) rcSel.value = keep;
  };

  /** 读回口：盘上那份就是清单的起点。认不出的行原样说出来 —— 写回去之前人得知道它们会没。 */
  const load = async (): Promise<void> => {
    try {
      const st = await commands.local_ccm_entry_status();
      pathCcm.hidden = !st.message;
      pathCcm.textContent = st.message;
    } catch (e) {
      pathCcm.hidden = false;
      pathCcm.textContent = `问不到本机 ccm 这一格：${String(e)}`;
    }
    try {
      const got = await commands.aliases_read();
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
      const r = await commands.aliases_install({ aliases: list, rcPath: rcSel.value || null });
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
    rcBlock.hidden = !path;
    if (!path) return;
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
    void load();
  });
  return wrap;
}
