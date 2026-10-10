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
import { isLocalOrigin } from "../ipc/origin";
import { failToast } from "../kit/toast"; // `K-R135`：用户级 PATH 那一格的失败要出声
import { markChore } from "./data-reads";
import { SETTINGS_GO_EVENT } from "./events";
// 接入那几问走通道、那台后端出成品（`../alias-reads`）；类型随成品住那边。
import type { ExecPolicy, PsHost, StartupFile, Shell } from "../alias-reads";
import { confirmDialog, type ConfirmFn } from "../kit/dialog";
import { toggleSwitch } from "../kit/switch";
import { allowLocalScripts, installAliasBlock, readAliases, removeAliasBlock, renderAliasBlock } from "../alias-reads";
import { buildProfilesList, type ProfileClash } from "./profiles-list";
import type { Origin } from "../generated/Origin";
import { openPath } from "@tauri-apps/plugin-opener";
import { cfgRow, type CfgDot, type CfgRow } from "./cfg-row";
import { homeShort } from "../kit/path";
import { hostFacts } from "./host-os";
import { copyText } from "../copy-table";
import type { LocalCcmEntry } from "../generated/LocalCcmEntry";
import { sayFailure, sayWithDetail } from "../kit/detail";

/** 界面上怎么叫那一代 PowerShell（两代的执行策略分开存）。 */
const psName = (h: PsHost): string =>
  h === "pwsh" ? copyText("machineAliases.policy.hostPwsh") : copyText("machineAliases.policy.hostPowershell");

/**
 * 这台机器（monitor 跑在的那台本机）用哪种 shell 的方言。
 * **认不出就不猜**（`null`）：别名那一格明说「认不出这台的系统」、安装入口置灰（[`buildUnknownOsAliasBlock`]）。
 */
export function localShell(): Shell | null {
  return hostFacts().shellDialect;
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

/** 别名那一组的入参。两个平台同一份，`platform` 是入参；本机远端同一份，`origin` 是入参。 */
export interface AliasManagerSpec {
  /** 这台机器用哪种 shell 的方言（本机 = [`localShell`]；远端恒 `posix`）。 */
  platform: Shell;
  /** 那台机器（取值函数：远端卡改名后跟着它走）。每一发都带它。 */
  origin: () => Origin;
  /** 改执行策略之前问一句的注入缝（缺省走应用内对话框）。 */
  confirm?: ConfirmFn;
}

/** 「终端」那一组：别名一行（默认展开）＋ 本机 Windows 上的「Windows 终端」一行。`load` 第一次露出来时调，`reread` 之后再露出来时调。 */
export interface AliasManager {
  element: HTMLElement;
  load(): void;
  reread(): void;
  /** 「待办」里「让终端认得 ccm 和别名」那一件的态（这台没选自己贴 ⇒ `null`）。 */
  setSelfPaste(c: { state: string } | null): void;
}

/** 「终端接入」那一格的几个元素（构造时一次建好，之后只改内容与 `hidden`）。 */
interface AccessDom {
  access: HTMLElement;
  pathCcm: HTMLPreElement;
  accessRows: HTMLElement;
  panel: HTMLDivElement;
  accessWarn: HTMLElement;
  rcPolicy: HTMLDivElement;
  allowBtn: HTMLButtonElement;
  accessNote: HTMLElement;
  chooser: HTMLDivElement;
  rcSel: HTMLSelectElement;
  otherIn: HTMLInputElement;
  otherErr: HTMLElement;
}

/** 建「终端接入」那一格的骨架；三颗按钮的动作由调用方给。 */
function buildAccessDom(local: boolean, on: { allow(): void; other(): void; openRc(): void }): AccessDom {
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
  // 加载这份 `$PROFILE` 的那一代 PowerShell 会不会跑它（执行策略由那台后端现问、判）；不会且不是组策略钉着 ⇒ 给标准做法的按钮。
  const rcPolicy = document.createElement("div");
  rcPolicy.className = "cfg-hint";
  rcPolicy.hidden = true;
  const allowBtn = document.createElement("button");
  allowBtn.type = "button";
  allowBtn.className = "settings-btn";
  allowBtn.textContent = copyText("machineAliases.policy.allow");
  allowBtn.hidden = true;
  allowBtn.addEventListener("click", () => on.allow());
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
    button(copyText("machineAliases.rc.useOther"), "", () => on.other()),
    ...(local
      ? [Object.assign(button(copyText("machineAliases.rc.open"), "", () => on.openRc()), { title: copyText("machineAliases.rc.openHint") })]
      : []),
  );
  chooser.append(rcSel, otherIn, chooserBtns, otherErr);
  access.append(pathCcm, accessRows, panel, accessWarn, rcPolicy, allowBtn, accessNote);
  return { access, pathCcm, accessRows, panel, accessWarn, rcPolicy, allowBtn, accessNote, chooser, rcSel, otherIn, otherErr };
}

/** 接入那一格里的一行：一个点 ＋ 一句 ＋ 行尾的按钮。 */
function accessLine(dot: CfgDot, text: string, ...tail: HTMLElement[]): HTMLElement {
  const r = el("div", dot === "ok" ? "cfg-line" : "cfg-line cfg-line-warn");
  r.dataset.role = "access-line";
  const d = el("span", "cfg-dot");
  d.dataset.dot = dot;
  r.append(d, el("span", "cfg-line-text", text), ...tail);
  return r;
}

const linkBtn = (label: string, onClick: () => void, danger = false): HTMLButtonElement =>
  button(label, danger ? "cfg-link cfg-link-danger" : "cfg-link", onClick);

/** 执行策略那一行的话：不会挡（或没有这一项）⇒ 空串。 */
function policyText(p: ExecPolicy | null): string {
  if (p === null || p.loads === true) return "";
  const ps = psName(p.host);
  if (p.error !== null) return copyText("machineAliases.policy.unknown", { ps, e: p.error });
  if (p.loads === null) return copyText("machineAliases.policy.unclear", { ps, policy: p.effective ?? "" });
  if (p.groupPolicy) return copyText("machineAliases.policy.groupPolicy", { ps, policy: p.effective ?? "" });
  return copyText("machineAliases.policy.blocks", { ps, policy: p.effective ?? "" });
}

/** 已接上那一行的话（加载它的那一代 PowerShell 不跑它 ⇒ 说没生效）。 */
function onLineText(c: StartupFile, p: string, shell: Shell): string {
  if (c.policy?.loads === false) return copyText("machineAliases.access.notLoaded", { path: p, ps: psName(c.policy.host) });
  return shell === "powershell" ? copyText("machineAliases.access.onWindow", { path: p }) : copyText("machineAliases.access.on", { path: p });
}

/** 接上 / 卸载成了之后那一句：接上了再看执行策略挡不挡（失败那一句由调用方就地说）。 */
function afterRcText(verb: "install" | "remove", pol: ExecPolicy | null, shell: Shell): string {
  if (verb !== "install") return "";
  if (pol !== null && pol.loads !== true) return copyText("machineAliases.policy.afterInstall");
  return shell === "powershell" ? copyText("machineAliases.powershell.blockAfterInstall") : copyText("machineAliases.posix.blockAfterInstall");
}

/** 请那台设执行策略之后那一句（现状以它重问的为准）。 */
function allowResultText(ps: string, r: { policy: ExecPolicy; setError: string | null }): string {
  const now = r.policy.effective ?? "";
  if (r.policy.loads === true) return copyText("machineAliases.policy.setDone", { ps, policy: now });
  if (r.policy.groupPolicy) return copyText("machineAliases.policy.groupPolicy", { ps, policy: now });
  return copyText("machineAliases.policy.setFailed", { ps, e: r.setError ?? r.policy.error ?? now });
}

/** 下拉里一份候选的字：路径 ＋（已接上 / 已被加载；不存在）。 */
function rcOptionLabel(c: StartupFile): string {
  const tags: string[] = [];
  if (c.block.present) tags.push(copyText("machineAliases.rc.tagBlock"));
  else if (c.sourced) tags.push(copyText("machineAliases.rc.tagSourced"));
  if (!c.exists) tags.push(copyText("machineAliases.rc.tagNew"));
  return tags.length ? `${c.path}（${tags.join("；")}）` : c.path;
}

/** 接入那一格底下的提醒：读不了的候选 · 同一份块接在了不止一处。 */
function accessWarnText(cands: StartupFile[], on: StartupFile[]): string {
  const warn: string[] = [];
  for (const c of cands) if (c.unreadable) warn.push(copyText("machineAliases.rcStatus.unreadable", { path: c.path, why: c.unreadable }));
  if (on.length > 1) warn.push(copyText("machineAliases.rcStatus.duplicates", { others: on.map((c) => c.path).join("\n") }));
  return warn.join("\n");
}

export function buildAliasManager(opts: AliasManagerSpec): AliasManager {
  const shell = opts.platform;
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
  const dom = buildAccessDom(local, { allow: () => void onAllow(), other: () => void onOther(), openRc: () => void onOpenRc() });
  const { pathCcm, accessRows, panel, accessWarn, rcPolicy, allowBtn, accessNote, chooser, rcSel, otherIn, otherErr } = dom;
  body.appendChild(dom.access);

  // ═══ 清单（配置文件按「基于」排成树）═══
  const profiles = buildProfilesList({
    origin: opts.origin,
    local,
    confirm: opts.confirm,
    onHead: (text) => {
      headText = text;
      renderStatus();
    },
  });
  const writes = document.createElement("div");
  writes.className = "cfg-writes";
  writes.dataset.role = "writes";
  body.append(profiles.element, writes);

  // ═══ Windows 终端（只本机 PowerShell）═══
  let winRow: CfgRow | null = null;
  let psExtras: PsExtras | null = null;
  if (shell === "powershell" && local) {
    winRow = cfgRow(copyText("machineAliases.win.title"), false);
    winRow.element.dataset.role = "win-row";
    group.appendChild(winRow.element);
  }

  // ── 状态 ──
  /** 头部那一行（清单读回之后由它给：`N 条 · 规则住 … · 改了下次起会话就生效`）。 */
  let headText = "";
  let cands: StartupFile[] = [];
  let home: string | null = null;
  /** 路径写成家目录打头的短形（家目录由那台后端给）。 */
  const short = (p: string): string => homeShort(p, home);
  let loadedOnce = false;
  /** 接上装进哪一份（人在「换一份文件」里选的；没选 ⇒ 已接上的那份，再没有 ⇒ 第一份候选）。 */
  let chosen: string | null = null;
  /** 人另指的那一份（原样的输入串；后端过围栏）。一旦指过，之后每次读回都带着它。 */
  let otherRc: string | null = null;
  /** 接上那一行下面开着哪一块。 */
  let panelOpen: "preview" | "uninstall" | "choose" | null = null;
  /** 「待办」里「让终端认得 ccm 和别名」那一件（这台选了自己贴才有；那台后端答的）。 */
  let selfPaste: { state: string } | null = null;
  let allowHost: PsHost | null = null;

  /** 接上装进哪一份。 */
  const target = (): StartupFile | undefined =>
    cands.find((c) => c.path === chosen) ?? cands.find((c) => c.block.present) ?? cands[0];

  /** 接上那一行下面那一块（再点同一个就收起）。 */
  const togglePanel = (which: NonNullable<typeof panelOpen>): void => {
    panelOpen = panelOpen === which ? null : which;
    void paintPanel();
  };

  /** 「卸载」那一块的头与按钮（要拿掉的那几行跟在 `code` 里）。 */
  const uninstallPanel = (t: StartupFile, code: HTMLElement): HTMLElement[] => {
    const go = button(copyText("machineAliases.rc.uninstall"), "", () => void runRc("remove", t.path));
    go.title = copyText("machineAliases.rc.uninstallHint");
    const acts = el("div", "cfg-acts");
    acts.append(go, button(copyText("machineAliases.form.cancel"), "", () => togglePanel("uninstall")));
    return [
      el("div", "cfg-sub", shell === "powershell" ? copyText("machineAliases.access.uninstallSubWindow") : copyText("machineAliases.access.uninstallSub")),
      el("div", "cfg-panel-title", copyText("machineAliases.access.uninstallWhat", { path: short(t.path) })),
      code,
      acts,
    ];
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
    const code = el("pre", "cfg-code");
    code.dataset.role = "block-text";
    code.textContent = copyText("machineAliases.aliasPreview.asking");
    if (panelOpen === "preview") {
      panel.append(el("div", "cfg-panel-title", t.block.present ? copyText("machineAliases.access.previewOn", { path: short(t.path) }) : copyText("machineAliases.access.previewOff", { path: short(t.path) })), code);
    } else {
      panel.append(...uninstallPanel(t, code));
    }
    try {
      code.textContent = await renderAliasBlock(opts.origin(), t.path);
    } catch (e) {
      sayFailure(code, copyText("machineAliases.preview.failedLine"), e);
    }
  };

  /** 已接上的每一份一行：现状 ＋ 看加了什么 · 卸载。 */
  const renderOnRows = (on: StartupFile[]): void => {
    for (const c of on) {
      const p = short(c.path);
      accessRows.appendChild(
        accessLine(
          c.policy?.loads === false ? "warn" : "ok",
          onLineText(c, p, shell),
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
  };

  /** 选中那份没接上：醒目的「接上 …」＋ 会加几行 · 看一眼 · 换一份文件 · 自己贴。 */
  const renderOffRow = (on: StartupFile[], t: StartupFile | undefined): void => {
    const connect = t
      ? button(copyText("machineAliases.access.connectTo", { path: short(t.path) }), "settings-btn-primary ccm-access-connect", () => void runRc("install", t.path))
      : null;
    if (connect) connect.title = shell === "powershell" ? copyText("machineAliases.powershell.blockInstallTitle") : copyText("machineAliases.posix.blockInstallTitle");
    accessRows.appendChild(
      accessLine("off", on.length ? copyText("machineAliases.access.otherFile", { path: short(t!.path) }) : shell === "powershell" ? copyText("machineAliases.access.offWindow") : copyText("machineAliases.access.off"), ...(connect ? [connect] : [])),
    );
    if (!t) return;
    const hint = el("div", "cfg-hint cfg-access-hint");
    hint.append(
      el("span", "", copyText("machineAliases.access.willAdd", { path: short(t.path), n: t.blockLines })),
      linkBtn(copyText("machineAliases.access.peek"), () => togglePanel("preview")),
      linkBtn(copyText("machineAliases.access.choose"), () => togglePanel("choose")),
      linkBtn(copyText("machineAliases.access.selfPaste"), () => void onSelfPaste()),
    );
    accessRows.appendChild(hint);
  };

  /** 接上那一格照读回口的候选画：已接上 ⇒ 一行现状 ＋ 看加了什么 · 卸载 ccm；没接上 ⇒ 醒目的「接上 …」＋ 看一眼 · 换一份文件 · 自己贴。不发 IPC。 */
  const renderAccess = (): void => {
    accessRows.textContent = "";
    const on = cands.filter((c) => c.block.present);
    const t = target();
    renderOnRows(on);
    if (!on.length && t && selfPaste && selfPaste.state !== "done") {
      const tp = t.path;
      accessRows.appendChild(
        accessLine("warn", copyText("machineAliases.selfPaste.waiting"), button(copyText("machineAliases.selfPaste.go"), "", goChores), linkBtn(copyText("machineAliases.selfPaste.undo"), () => void onUnSelfPaste(tp))),
      );
    } else if (!on.length || (t && !t.block.present)) {
      renderOffRow(on, t);
    }
    accessWarn.textContent = accessWarnText(cands, on);
    showPolicy((t?.block.present ? t : on[0])?.policy ?? t?.policy ?? null);
    fillRcOptions();
    renderClash();
    renderWrites();
    renderStatus();
    if (panelOpen) void paintPanel();
  };

  /** 块外与清单同名的那几条：交给清单那一块画（三个选择）。 */
  const renderClash = (): void => {
    const hits = new Map<string, ProfileClash>();
    for (const c of cands) for (const f of c.block.conflictingFunctions) if (!hits.has(f.name)) hits.set(f.name, { name: f.name, path: c.path, line: f.line, wins: f.wins });
    profiles.setClashes([...hits.values()]);
  };

  /** 页尾那一句：接上之后会动哪份文件、什么时候动。 */
  const renderWrites = (): void => {
    const t = cands.find((c) => c.block.present) ?? target();
    writes.hidden = !t;
    if (t) writes.textContent = copyText("machineAliases.writes.line", { path: short(t.path), n: t.blockLines });
  };

  /** 收着时那一行：清单给的那一句（条数 · 规则住哪 · 下次起会话生效）＋ 同名提示；点的颜色与主动作看接入那一格。 */
  const renderStatus = (): void => {
    const on = cands.filter((c) => c.block.present);
    const names = [...new Set(cands.flatMap((c) => c.block.conflictingFunctions.map((f) => f.name)))];
    const tail = names.length ? copyText("machineAliases.status.clashTail", { names: names.join(copyText("accountsMcp.list.sep")) }) : "";
    const t = target();
    let dot: CfgDot;
    if (on.length) {
      dot = "ok";
      row.setAction(null);
    } else {
      dot = "off";
      row.setAction(t ? button(copyText("machineAliases.status.connect", { path: short(t.path) }), "settings-btn-primary", () => row.setOpen(true)) : null);
    }
    row.setStatus(names.length ? "warn" : dot, headText + tail);
  };

  const fillRcOptions = (): void => {
    rcSel.textContent = "";
    for (const c of cands) {
      const o = el("option", "", rcOptionLabel(c));
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

  /** 读回口：启动文件候选（接入那一格）。清单那一块自己读配置文件（`profiles-read`）。 */
  const readBack = async (): Promise<void> => {
    try {
      const got = await readAliases(opts.origin(), shell, otherRc);
      home = got.home;
      cands = got.rcCandidates;
    } catch (e) {
      sayFailure(accessWarn, copyText("machineAliases.readBack.failed"), e);
      row.setStatus("warn", copyText("machineAliases.status.readFailed"));
    }
    renderAccess();
  };

  /** 本机 ccm 那一格：我们那一份装下来了没有 ＋ 终端里敲 `ccm` 走到的是不是它（Windows 上问新开的 PowerShell）。 */
  const paintLocalCcm = async (): Promise<void> => {
    try {
      const st = await askLocalCcm();
      pathCcm.hidden = !st.message;
      pathCcm.textContent = st.message;
    } catch (e) {
      pathCcm.hidden = false;
      sayFailure(pathCcm, copyText("machineAliases.load.ccmFailed"), e);
    }
  };

  const load = async (): Promise<void> => {
    wrap.dataset.origin = opts.origin();
    if (local) await paintLocalCcm();
    await Promise.all([readBack(), profiles.load()]);
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
    } catch (e) {
      sayFailure(otherErr, copyText("machineAliases.other.failed"), e);
    }
    renderAccess();
  };

  /** 接上 / 卸载：装 / 卸别名块（方言由那份文件的扩展名定，后端判）。成功失败都重读：盘上现在是什么样，就显示什么样。 */
  const runRc = async (verb: "install" | "remove", path: string): Promise<void> => {
    panelOpen = null;
    accessNote.textContent = verb === "install" ? copyText("machineAliases.runRc.installing") : copyText("machineAliases.runRc.removing");
    let failed: { title: string; e: unknown } | null = null;
    try {
      if (verb === "install") await installAliasBlock(opts.origin(), path);
      else await removeAliasBlock(opts.origin(), path);
    } catch (e) {
      failed = { title: verb === "install" ? copyText("machineAliases.runRc.installFailed") : copyText("machineAliases.runRc.removeFailed"), e };
    }
    await readBack();
    if (failed !== null) sayFailure(accessNote, failed.title, failed.e);
    else accessNote.textContent = afterRcText(verb, cands.find((c) => c.path === path)?.policy ?? null, shell);
  };

  /** 「我自己贴」：交那台记下，去「待办」里那一件（要贴的几行、贴在哪、存盘后自己认出都在那里）。 */
  const goChores = (): void => {
    wrap.dispatchEvent(new CustomEvent(SETTINGS_GO_EVENT, { bubbles: true, detail: { page: "data", anchor: `chores:${opts.origin()}` } }));
  };
  /** 记一条「这台自己贴」/ 撤掉它；记不下 ⇒ 出声、返回 false。 */
  const mark = async (spec: Parameters<typeof markChore>[1]): Promise<boolean> => {
    try {
      await markChore(opts.origin(), spec);
      return true;
    } catch (e) {
      failToast(copyText("machineAliases.selfPaste.failed"), e, { level: "error" });
      return false;
    }
  };
  const onSelfPaste = async (): Promise<void> => {
    const t = target();
    if (!t) return;
    if (!(await mark({ op: "selfPaste", rc: t.path }))) return;
    selfPaste = { state: "todo" };
    renderAccess();
    goChores();
  };
  /** 改回让 cc-monitor 接上：撤掉那条记录，再照常接上。 */
  const onUnSelfPaste = async (path: string): Promise<void> => {
    if (!(await mark({ op: "unselfPaste" }))) return;
    selfPaste = null;
    await runRc("install", path);
  };

  /** 执行策略那一行：只在它会挡住块（或说不清）时出声；不会挡 ⇒ 不占地方。 */
  const showPolicy = (p: ExecPolicy | null): void => {
    rcPolicy.textContent = policyText(p);
    rcPolicy.hidden = rcPolicy.textContent === "";
    allowBtn.hidden = !(p?.loads === false && !p.groupPolicy);
    allowHost = p?.host ?? null;
  };

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
      said = allowResultText(ps, await allowLocalScripts(opts.origin(), host));
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
      failToast(copyText("machineAliases.openRc.failed"), e, { fact: path });
    }
  };

  row.setStatus("off", copyText("cfgPage.row.reading"));
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
    setSelfPaste(c) {
      selfPaste = c;
      if (loadedOnce) renderAccess();
    },
  };
}


// ═══════════════════════════════════════════════════════════════════════════
// 「Windows 终端」那一行（只本机 PowerShell）：没开时自动打开 · cmd / Git Bash 也认 ccm
// ═══════════════════════════════════════════════════════════════════════════
//
// 别名块那一半（选哪份 `$PROFILE` · 装 / 卸 / 预览 / 现状 · 执行策略）在上面「别名」那一行里，与 POSIX 同一族命令；
// 这一行是不随启动文件走的那两格：
// - 用别名起 claude 时 cc-monitor 没开就先打开它（开关，立刻生效）；
// - 用户级 PATH：开关就是手动加的那一下，拨回就是删；每次读都真问一趟（不缓存），探不动 ≠ 不在 PATH 上 ——
//   读不出时开关不给拨、那句原话上屏。那两条命令的逐字文本给不想拨开关的人看（与开关跑的是同一份字节）。

interface PsExtras {
  loadNow(): void;
}

function buildPsExtras(row: CfgRow): PsExtras {
  const body = row.body;
  let onPath: boolean | null = null;
  const paintStatus = (): void => {
    row.setStatus(onPath === null ? "off" : "ok", onPath ? copyText("machineAliases.win.statusPathOn") : copyText("machineAliases.win.statusPathOff"));
  };

  const auto = toggleSwitch({
    label: copyText("machineAliases.ps.autoLaunch"),
    help: copyText("machineAliases.ps.autoLaunchHint"),
    on: false,
    onChange: async (enabled) => {
      try {
        await commands.cc_set_auto_launch({ enabled });
        return true;
      } catch (e) {
        failToast(copyText("machineAliases.ps.saveFailed"), e);
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
        failToast(on ? copyText("machineAliases.userPath.addFailed") : copyText("machineAliases.userPath.removeFailed"), e);
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
  body.append(auto.root, autoPath, pathSw.root, pathHelp, cmdLink, cmdPre);

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
        sayWithDetail(pathHelp, st.error.said, st.error.detail);
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
      sayFailure(pathHelp, copyText("machineAliases.userPath.readFailed"), e);
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
      sayFailure(autoPath, copyText("machineAliases.autoLaunch.unreadable"), e);
    }
  };

  paintStatus();
  return {
    loadNow: () => {
      void refreshAutoLaunch();
      void refreshPath();
    },
  };
}

/**
 * 问一次本机 `ccm` 那一格（判定与那句话在 monitor `ccm_probe::local_ccm_cell`）。调用方：别名管理器读回 ·
 * 本机那一行「重新对齐」（`fresh`：先作废 PATH 探针那份 5 分钟缓存）。Windows 本机问新开的 PowerShell 里敲 `ccm` 走到哪。
 */
export function askLocalCcm(fresh = false): Promise<LocalCcmEntry> {
  return commands.local_ccm_entry_status(fresh);
}
