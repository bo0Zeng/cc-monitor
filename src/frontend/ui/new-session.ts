/**
 * **起新会话：全产品一个框、一个请求。** 命令面板「新建会话…」· 历史 / 标签页「在此目录新建会话」· 消息流「从这里分叉」·
 * 设置 › 机器卡 ⋯「新建会话…」都开这一个框（[`openNewSession`]）；设置 › 账号页［在 tmux 里登录］不开框、直接走同一个请求（[`startNewSession`]）。
 *
 * 判定都在那台后端（`new-session-reads.ts` 那两问 ＋ 起的那一问 `session-open`）：目录在不在 · 用哪个号（选不了 ⇒ 不起、带替代号）· 终端名 ·
 * 能起哪几家 · 分叉只在点［新建］那一步写分支记录。这里只排版：那台说哪一格不行，那一句就落在那一格下；整体不行落在框顶。
 *
 * 起了之后框立刻关，开窗 · 等报到交 `session-open.ts`：主窗口里起的先长出占位标签页、报到了换成真的；
 * 在别的窗口（设置 · 查看）起的 ⇒ 报到了说一句「已启动」＋［切过去］，没报到那一句由主窗口说。
 */
import { emit } from "@tauri-apps/api/event";
import { formDialog } from "./kit/dialog";
import { banner, type BannerTone } from "./kit/banner";
import { el } from "./kit/el";
import { button } from "./kit/button";
import { fold } from "./kit/fold";
import { icon } from "./kit/icon";
import { spinner } from "./kit/progress";
import { openMenu } from "./kit/menu";
import { select, type SelectOption } from "./kit/select";
import { tag } from "./kit/badge";
import { toast } from "./kit/toast";
import { copyText } from "./copy-table";
import { commands } from "./ipc/commands";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { machineName, offlineSaid } from "./control-said";
import { DEFAULT_AGENT, agentHasAccounts, defaultLauncherOf } from "./agent-profile";
import { fetchAccounts } from "./account-reads";
import type { Account } from "./accounts";
import { accountAvatarEl } from "./account-color";
import { accountLabel } from "./acct-words";
import { readPrefs } from "./machine-prefs";
import { chosenAccount, type AccountAsk } from "./launch-account";
import { askOpen, landOpened, openSession, type OpenEnd, type OpenHow, type Opened, type SessionOpenReq } from "./session-open";
import { accountsOf } from "./settings-dest";
import { askDir, askFacts, type OpenFacts } from "./new-session-reads";
import type { AccountUnavailable } from "./generated/AccountUnavailable";
import type { NewOpen } from "./generated/NewOpen";
import type { ForkSlot } from "./generated/ForkSlot";
import type { Run } from "./generated/Run";
import { readMachines } from "./machine-feed";
import type { MachineState } from "./generated/MachineState";
import { readStrategies } from "./rot-reads";
import type { RotationStrategiesReply } from "./generated/RotationStrategiesReply";
import { readProfiles } from "./profiles-reads";
import { pageTabs, setPage } from "./new-session-set";
import s from "./new-session.module.css";

/** 设置窗 → 主窗口：切到那个会话（载荷 `{origin, sid}`）。 */
export const FOCUS_SESSION_EVENT = "new-session-focus";

export interface NewSessionSpec {
  /** 预选的那台（缺 ⇒ 本机）。 */
  origin?: Origin;
  /** 机器那一格锁定（机器卡入口）。 */
  lockMachine?: boolean;
  /** 预填的工作目录（缺 ⇒ 那台最近用过的第一个）。 */
  cwd?: string;
  /** 目录那一格锁定（计划页接手：工作区根）：只读、没有［最近的］。 */
  lockCwd?: boolean;
  /** 预填的第一句（计划页接手：交接说明）；起的时候交给那一家，只跟 tmux。 */
  prompt?: string;
  /** 预选哪一家（历史条目的那一家；缺 ⇒ 默认那一家）。 */
  agent?: string;
  /** 分叉：源会话 · 从哪条消息处 · 源会话的标题（框顶那一句）。 */
  fork?: { sid: string; uuid: string; title: string };
  /** 开在「一组」页：存好的那一组的名字（`null` ＝ 新建一组）；缺 ⇒ 单个页。 */
  set?: string | null;
  /**
   * 问图（计划页［问这一块…］，`plan-ask.ts`）：框里只别名 · 目录 · 开口那句；号 · 轮换 · 哪一家随别名，运行于定在 tmux（开口那句只跟 tmux）。
   * `alias` 预选的那条（`null` ⇒ 空着，选了才能起）；改选 ⇒ `relined` 问那台新拼的那句，开口那句前一半跟着换、人打的留着。
   */
  ask?: { title: string; alias: string | null; relined: (alias: string) => Promise<string> };
}

/** 一台机器那一项：连没连上。 */
interface MachineOpt {
  origin: Origin;
  up: boolean;
}

/**
 * 机器清单：本机后端手上各台此刻的状态成品（`machine-read` 一问，一台都不问）；`up` 才算连着，「连接这台」关着的不列。
 * 本机在前；问不到 ⇒ 只有本机、灰着。
 */
async function listMachines(): Promise<MachineOpt[]> {
  let got: [string, MachineState][] = [];
  try {
    got = Object.entries(await readMachines()).filter(([, m]) => m.state !== "disabled");
  } catch (e) {
    console.warn("[new-session] 问不到各台的状态成品：", e);
  }
  const opts = got.map(([origin, m]) => ({ origin, up: m.state === "up" }));
  const local = opts.filter((m) => isLocalOrigin(m.origin));
  return [...(local.length > 0 ? local : [{ origin: LOCAL_ORIGIN, up: false }]), ...opts.filter((m) => !isLocalOrigin(m.origin))];
}

/** 账号那一格的一项。`ready: false` ⇒ 选不了（`why` 是那台写好的那一句，`login` ⇒ 给［登录…］）。 */
interface AccountOpt {
  name: string;
  isDefault: boolean;
  ready: boolean;
  why: string | null;
  login: boolean;
  quota: string | null;
  /** 号在哪一台（头像照那台账号库的号色画）。 */
  origin: Origin;
}

/** 那台的号（那台说进选号表的那几个：`newSession`）；那台没开多账号 / 读不出 ⇒ `null`（这一行不出）。 */
async function listAccounts(origin: Origin, agent: string): Promise<AccountOpt[] | null> {
  if (!agentHasAccounts(agent)) return null;
  let list: Account[];
  let def: string | null;
  try {
    const st = await fetchAccounts(origin);
    if (!st.available) return null;
    def = st.meta?.effectiveDefault ?? null;
    list = st.accounts.filter((a) => a.newSession === true);
  } catch {
    return null;
  }
  if (list.length === 0) return null;
  const opts = list.map((a) => ({
    name: a.name,
    isDefault: a.name === def,
    ready: a.selectable,
    why: a.unselectable ?? null,
    login: a.login ?? false,
    quota: a.quota ?? null,
    origin,
  }));
  return [...opts.filter((a) => a.isDefault), ...opts.filter((a) => !a.isDefault)];
}

/** 账号那一项：头像 · 名字 · 灰字（`默认 · 5h 63%` · 需登录）。 */
function accountOption(a: AccountOpt): SelectOption {
  const note: string[] = [];
  if (a.isDefault) note.push(copyText("newSession.account.default"));
  if (a.quota) note.push(a.quota);
  if (!a.ready && a.why) note.push(a.why);
  return { value: a.name, label: accountLabel(a.name), note: note.join(copyText("kit.text.sep")), lead: () => accountAvatarEl(a.name, { size: 16, origin: a.origin }) };
}

/** 机器那一项：远端带机器标记；断开的灰着、说「离线」（正选着的那台照样列出、可选）。 */
function machineOption(m: MachineOpt, chosen: Origin): SelectOption {
  const name = machineName(m.origin);
  return {
    value: m.origin,
    label: accountLabel(name),
    lead: isLocalOrigin(m.origin) ? undefined : () => tag(name),
    note: m.up ? undefined : copyText("newSession.machine.down"),
    enabled: m.up || m.origin === chosen,
    why: copyText("newSession.machine.down"),
  };
}

/** 一行：左标签 ＋ 右格（格里一个控件框 ＋ 一行说明 / 错误）。 */
function row(label: string, control: HTMLElement, boxed = true): { root: HTMLElement; cell: HTMLElement; note: HTMLElement; setNote: (text: string, tone?: "error" | "warn", acts?: HTMLElement[]) => void } {
  const root = el("div");
  root.className = s.nsRow;
  const lab = el("div", "", label);
  lab.className = s.nsLabel;
  root.append(lab);
  const cell = el("div");
  cell.className = s.nsCell;
  if (boxed) {
    const box = el("div");
    box.className = s.nsBox;
    box.appendChild(control);
    cell.appendChild(box);
  } else {
    cell.appendChild(control);
  }
  const note = el("div");
  note.className = s.nsNote;
  note.hidden = true;
  cell.appendChild(note);
  root.appendChild(cell);
  const setNote = (text: string, tone?: "error" | "warn", acts: HTMLElement[] = []): void => {
    note.replaceChildren();
    note.hidden = text === "" && acts.length === 0;
    if (tone) note.dataset.intent = tone;
    else delete note.dataset.intent;
    if (tone === "error") cell.dataset.error = "true";
    else delete cell.dataset.error;
    if (text) note.append(el("span", "", text));
    note.append(...acts);
  };
  return { root, cell, note, setNote };
}

/** 那台说不行的码 ⇒ 落在那一格下的那一句（句子在文案表，那台的原话兜底）。 */
function fieldSaid(code: string, said: string, machine: string): string {
  switch (code) {
    case "no_dir":
      return copyText("launch.dir.missing", { machine });
    case "tmux_taken":
      return copyText("launch.tmux.taken");
    default:
      return said;
  }
}

const ACCOUNT_FOLLOW = "\u0000follow";

type Row = ReturnType<typeof row>;

/** 一个单行文本格（不拼写检查，读屏名 = 那一行的标签）。 */
function textInput(aria: string): HTMLInputElement {
  const i = el("input");
  i.className = s.nsInput;
  i.type = "text";
  i.spellcheck = false;
  i.setAttribute("aria-label", aria);
  return i;
}

/** 分叉那一形框顶那一块：自哪个会话 ＋ 一句说明；`fromLine` 等那台报出轮次后改写。 */
function forkBox(title: string): { box: HTMLElement; fromLine: HTMLElement } {
  const box = el("div");
  box.className = s.nsFork;
  box.appendChild(icon("fork"));
  const text = el("div");
  text.className = s.nsForkText;
  const fromLine = el("div", "", copyText("newSession.fork.fromBare", { title }));
  fromLine.className = s.nsForkFrom;
  const forkNote = el("div", "", copyText("newSession.fork.note"));
  forkNote.className = s.nsForkNote;
  text.append(fromLine, forkNote);
  box.appendChild(text);
  return { box, fromLine };
}

/** 工作目录那一行：目录格 ＋「最近的」（那台最近用过的目录，点一项填进目录格）；锁定 ⇒ 只读、没有「最近的」。 */
function cwdField(cwd: string, locked: boolean): { cwdInput: HTMLInputElement; recentBtn: HTMLButtonElement; cwdRow: Row } {
  const cwdInput = el("input");
  cwdInput.className = s.nsInput;
  cwdInput.type = "text";
  cwdInput.spellcheck = false;
  cwdInput.value = cwd;
  cwdInput.readOnly = locked;
  cwdInput.setAttribute("aria-label", copyText("newSession.label.cwd"));
  const recentBtn = el("button");
  recentBtn.type = "button";
  recentBtn.className = s.nsRecent;
  recentBtn.setAttribute("aria-label", copyText("newSession.recent.label"));
  recentBtn.append(el("span", "", copyText("newSession.recent.label")), icon("caretDown", "compact"));
  const cwdWrap = el("div");
  cwdWrap.className = s.nsBox;
  cwdWrap.append(cwdInput);
  if (!locked) cwdWrap.append(recentBtn);
  return { cwdInput, recentBtn, cwdRow: row(copyText("newSession.label.cwd"), cwdWrap, false) };
}

interface PlaceRadio {
  label: HTMLLabelElement;
  input: HTMLInputElement;
  note: HTMLElement;
}

/** 「运行于」的几张：托管终端 · tmux 里 · 新的终端窗口（直路）。 */
type Place = "managed" | "tmux" | "window";

/** 勾的那一张 ⇒ 线上 `run` 那一格。 */
const PLACE_RUN: Record<Place, Run> = { managed: "managed", tmux: "tmux", window: "direct" };

/** 「运行于」那一张：单选 ＋ 名字 ＋ 灰字。 */
function placeRadio(places: HTMLElement, value: Place, title: string): PlaceRadio {
  const label = el("label");
  label.className = s.nsPlace;
  const input = el("input");
  input.type = "radio";
  input.name = "ns-place";
  input.value = value;
  const t = el("div");
  t.className = s.nsPlaceText;
  const note = el("div");
  note.className = s.nsPlaceNote;
  t.append(el("div", "", title), note);
  label.append(input, t);
  places.appendChild(label);
  return { label, input, note };
}

/** 「更多」那一折：tmux 会话名（分叉不出）· 启动命令。 */
function moreFields(forking: boolean): { tmuxInput: HTMLInputElement; tmuxRow: Row; cmdInput: HTMLInputElement; cmdRow: Row; moreRow: HTMLElement } {
  const tmuxInput = textInput(copyText("newSession.label.tmuxName"));
  const tmuxRow = row(copyText("newSession.label.tmuxName"), tmuxInput);
  const cmdInput = textInput(copyText("newSession.label.command"));
  const cmdRow = row(copyText("newSession.label.command"), cmdInput);
  const more = el("div");
  more.className = s.nsMore;
  if (!forking) more.appendChild(tmuxRow.root);
  more.appendChild(cmdRow.root);
  const moreRow = el("div");
  moreRow.className = s.nsMoreRow;
  moreRow.appendChild(
    fold({ title: forking ? copyText("newSession.more.fork") : copyText("newSession.more.plain"), open: false, body: more, bare: true }),
  );
  return { tmuxInput, tmuxRow, cmdInput, cmdRow, moreRow };
}

/** 终端窗口那一张的灰字：本机 · 远端有 tmux · 远端没 tmux。 */
function windowNote(origin: Origin, hasTmux: boolean): string {
  if (isLocalOrigin(origin)) return copyText("newSession.place.windowNoteLocal");
  return hasTmux ? copyText("newSession.place.windowNote", { machine: machineName(origin) }) : copyText("newSession.place.noTmux");
}

/** 账号那一格预选哪一项：分叉且那台说得出源会话的号 ⇒ 那个号；分叉说不出 ⇒ 跟随；不分叉 ⇒ 排第一的。 */
function accountPick(forking: boolean, forkAccount: ForkSlot<string | null> | undefined, first: string): string {
  if (forking && forkAccount?.kind === "known" && forkAccount.value != null) return forkAccount.value;
  return forking ? ACCOUNT_FOLLOW : first;
}

/** 那台说哪一格不行 ⇒ 那一格；说不出 ⇒ `null`（落到框顶）。 */
function fieldRow(field: string | null, rows: Record<"cwd" | "tmuxName" | "command" | "agent" | "place" | "account", Row>): Row | null {
  switch (field) {
    case "cwd":
    case "tmuxName":
    case "command":
    case "agent":
    case "place":
    case "account":
      return rows[field];
    default:
      return null;
  }
}

const dialogTitle = (spec: NewSessionSpec, origin: Origin): string => {
  if (spec.fork) return copyText("newSession.title.fork");
  if (spec.ask) return spec.ask.title;
  return spec.lockMachine ? copyText("newSession.title.onMachine", { machine: machineName(origin) }) : copyText("newSession.title.plain");
};

/** 打开起新会话框。取消 ⇒ 什么都不起不写。 */
export async function openNewSession(spec: NewSessionSpec = {}): Promise<void> {
  const fork = spec.fork ?? null;
  let origin: Origin = spec.origin ?? LOCAL_ORIGIN;
  let facts: OpenFacts | null = null;
  let factsFailed: string | null = null;
  let agent = spec.agent ?? DEFAULT_AGENT;
  let accounts: AccountOpt[] | null = null;
  let machines: MachineOpt[] = [{ origin, up: true }];
  let forkLine: HTMLElement | null = null;
  let dirSeq = 0;
  let pendingTop: { text: string; acts: HTMLElement[]; detail: string; tone: BannerTone } | null = null;
  // 这一趟的票：期限到了带同一张再问 ⇒ 那台认出同一趟，起好了回原样那一份、不起第二个。
  // 表单一格没改就再点［新建］⇒ 还是这一张（等于再核一次）；改过任一格 ⇒ 换一张（那是另一趟）。
  let ticket = crypto.randomUUID();
  /** 上一次交出去的那一份（不含票）：比它就知道表单改没改。 */
  let lastAsked: string | null = null;

  const form = el("div");
  form.className = s.nsForm;
  const top = el("div");
  top.className = s.nsTop;
  form.appendChild(top);

  // 「单个 · 一组」页签：分叉 · 计划页接手（锁目录 · 带第一句）只有单个，不出页签。
  const paged = !fork && spec.lockCwd !== true && spec.prompt === undefined;
  /** 此刻露着一组页（否则单个页）。 */
  let onSet = paged && spec.set !== undefined;
  const tabs = pageTabs((p) => {
    onSet = p;
    paintPage();
  });
  if (paged) form.appendChild(tabs.el);

  if (fork) {
    const { box, fromLine } = forkBox(fork.title);
    form.appendChild(box);
    forkLine = fromLine;
  }

  // 机器
  const machineSel = select({
    label: copyText("newSession.label.machine"),
    options: [machineOption({ origin, up: true }, origin)],
    value: origin,
    onChange: (v) => {
      origin = v;
      cwdInput.value = "";
      cwdRow.setNote("");
      void loadMachine();
      void loadAliases();
      setsOf = null;
      if (onSet) paintPage();
    },
  });
  const machineRow = row(copyText("newSession.label.machine"), machineSel.el, false);
  form.appendChild(machineRow.root);

  // 单个页那几格住 `single`；一组页是另一份（`new-session-set.ts`），换页只换哪一份露着。
  const single = el("div", s.nsPage);
  form.appendChild(single);
  const sets = setPage(() => paintPage());
  /** 一组页读的是哪台的（`null` ＝ 还没读）。 */
  let setsOf: Origin | null = null;
  /** 打开时点名的那一组（只第一次读时用）。 */
  let openSet = spec.set ?? null;
  if (paged) form.appendChild(sets.el);

  // 别名（一类 ＝ 一条别名；起的那一行 `ccm @别名`，合下来的几格在那台合）：第一项「不用别名」。别名住在那台 ⇒ 换机器重列。
  // 问图：那一格总在，第一项是「选一条别名」（选了才能起），预选那台记着的那条；改选 ⇒ 问那台新拼的那句。
  const ask = spec.ask ?? null;
  const noAlias = { value: "", label: ask ? copyText("newSession.ask.pick") : copyText("newSession.alias.none") };
  const aliasSel = select({ label: copyText("newSession.label.alias"), options: [noAlias], value: "", onChange: (v) => void relineFor(v) });
  const aliasRow = row(copyText("newSession.label.alias"), aliasSel.el, false);
  aliasRow.root.hidden = ask === null;
  single.appendChild(aliasRow.root);
  const loadAliases = async (): Promise<void> => {
    let names: string[] = [];
    try {
      names = fork ? [] : (await readProfiles(origin)).profiles.filter((p) => p.usable).map((p) => p.name);
    } catch {
      names = [];
    }
    aliasRow.root.hidden = ask === null && names.length === 0;
    const pick = ask && ask.alias !== null && names.includes(ask.alias) ? ask.alias : "";
    aliasSel.setOptions([noAlias, ...names.map((n) => ({ value: n, label: n }))], pick);
    handle.refresh();
  };
  /** 问图：开口那句里那台拼的那一截（改选别名时换掉它，人打的留着）。 */
  let askLine = spec.prompt ?? "";
  const relineFor = async (alias: string): Promise<void> => {
    if (!ask || alias === "") {
      handle.refresh();
      return;
    }
    let line: string;
    try {
      line = await ask.relined(alias);
    } catch (e) {
      aliasRow.setNote(e instanceof Error ? e.message : String(e), "error");
      return;
    }
    if (aliasSel.value() !== alias) return;
    aliasRow.setNote("");
    if (promptInput.value.startsWith(askLine)) promptInput.value = line + promptInput.value.slice(askLine.length);
    askLine = line;
    handle.refresh();
  };

  // 工作目录 ＋「最近的」
  const cwdLocked = spec.lockCwd === true;
  const { cwdInput, recentBtn, cwdRow } = cwdField(spec.cwd ?? "", cwdLocked);
  single.appendChild(cwdRow.root);

  // agent（那台能起的多于一家才出）
  const agentSel = select({
    label: copyText("newSession.label.agent"),
    options: [],
    onChange: (v) => {
      agent = v;
      paintAgent();
      void configuredCommand(origin, agent).then((c) => (cmdInput.value = c));
      void paintAccounts().then(() => handle.refresh());
    },
  });
  const agentRow = row(copyText("newSession.label.agent"), agentSel.el, false);
  agentRow.root.hidden = true;
  single.appendChild(agentRow.root);

  // 账号
  const accountSel = select({
    label: copyText("newSession.label.account"),
    options: [],
    onChange: () => {
      paintAccountNote();
      handle.refresh();
    },
  });
  const accountRow = row(copyText("newSession.label.account"), accountSel.el, false);
  accountRow.root.hidden = true;
  single.appendChild(accountRow.root);

  // 轮换：用哪份策略（缺省新会话用的那份）· 不轮换 · 那台的各份策略（标题 ＝ 文件名，灰字 ＝ 说明第一行）。读不出那台的策略目录 ⇒ 不出这一行。
  const rotSel = select({ label: copyText("rot.new.label"), options: [], menuWidth: 340 });
  const rotRow = row(copyText("rot.new.label"), rotSel.el, false);
  rotRow.root.hidden = true;
  single.appendChild(rotRow.root);
  let rotSeq = 0;
  let forNew = "";
  const paintRotation = async (): Promise<void> => {
    const my = ++rotSeq;
    let list: RotationStrategiesReply | null = null;
    try {
      list = accounts === null ? null : await readStrategies(origin);
    } catch {
      list = null;
    }
    if (my !== rotSeq) return;
    rotRow.root.hidden = list === null || ask !== null;
    if (list === null) return;
    forNew = list.forNew;
    rotSel.setOptions(
      [
        { value: "", label: copyText("rot.none.label"), detail: copyText("rot.none.desc"), note: list.forNew === "" ? copyText("rot.tag.forNew") : undefined },
        ...list.strategies.map((st) => ({ value: st.id, label: st.id, note: st.forNew ? copyText("rot.tag.forNew") : undefined, detail: st.summary })),
      ],
      list.forNew,
    );
  };

  // 运行于
  const places = el("div");
  places.className = s.nsPlaces;
  const placeRow = row(copyText("newSession.label.place"), places, false);
  placeRow.root.hidden = ask !== null;
  single.appendChild(placeRow.root);
  // 只列那台答的 `hosts` 里有的（托管 · tmux；终端窗口恒在）。
  const managedRadio = placeRadio(places, "managed", copyText("newSession.place.managed"));
  const tmuxRadio = placeRadio(places, "tmux", copyText("newSession.place.tmux"));
  const windowRadio = placeRadio(places, "window", copyText("newSession.place.window"));
  managedRadio.note.textContent = copyText("newSession.place.managedNote");
  tmuxRadio.note.textContent = copyText("newSession.place.tmuxNote");
  const radios: Record<Place, PlaceRadio> = { managed: managedRadio, tmux: tmuxRadio, window: windowRadio };

  // 第一句（只跟容器：选了终端窗口 ⇒ 那一格灰着、不交，字留着，切回托管 / tmux 照样交）
  const promptInput = el("textarea");
  promptInput.className = s.nsInput;
  promptInput.rows = 3;
  promptInput.spellcheck = false;
  promptInput.value = spec.prompt ?? "";
  promptInput.setAttribute("aria-label", copyText("newSession.label.prompt"));
  const promptRow = row(copyText("newSession.label.prompt"), promptInput);
  single.appendChild(promptRow.root);
  if (ask) promptRow.setNote(copyText("newSession.ask.note"));

  // 更多
  const { tmuxInput, tmuxRow, cmdInput, cmdRow, moreRow } = moreFields(fork !== null);
  moreRow.hidden = ask !== null;
  single.appendChild(moreRow);

  /** 勾的哪一张；那台的默认还没到、也没人勾 ⇒ `null`（请求不带 `run`，那台照自己的默认）。 */
  const place = (): Place | null =>
    (Object.keys(radios) as Place[]).find((p) => radios[p].input.checked && !radios[p].label.hidden) ?? null;
  const up = (): boolean => machines.find((m) => m.origin === origin)?.up ?? false;
  // 断开时框顶那一句：照那台成品写的离线那一句；成品还没到 ⇒ 那台状态不明（未知不当离线）。
  const offlineLine = (): string => offlineSaid(origin) ?? copyText("settingsNav.dot.unknown", { machine: machineName(origin) });
  const chosenAccountOpt = (): AccountOpt | null =>
    accounts?.find((a) => a.name === accountSel.value()) ?? null;
  const loginBtn = (): HTMLButtonElement => {
    const login = button({ label: copyText("newSession.account.login"), kind: "ghost", size: "compact" });
    login.addEventListener("click", () => void openLogin(origin));
    return login;
  };

  const paintTop = (): void => {
    top.replaceChildren();
    if (!up()) {
      const again = button({ label: copyText("newSession.machine.reconnect"), size: "compact" });
      again.addEventListener("click", () => {
        void commands.backend_start({ origin }).catch(() => {});
      });
      top.appendChild(banner("error", offlineLine(), [again]));
    } else if (factsFailed !== null) {
      top.appendChild(banner("error", factsFailed));
    } else if (pendingTop) {
      top.appendChild(banner(pendingTop.tone, pendingTop.text, pendingTop.acts, pendingTop.detail));
    }
  };
  /** 整体不行的那一句落在框顶。 */
  const sayTop = (text: string, acts: HTMLElement[] = [], detail = "", tone: BannerTone = "error"): void => {
    pendingTop = { text, acts, detail, tone };
    paintTop();
  };

  const paintAccountNote = (): void => {
    const a = chosenAccountOpt();
    if (a && !a.ready) {
      accountRow.setNote(a.why ?? "", "error", a.login ? [loginBtn()] : []);
    } else {
      accountRow.setNote("");
    }
  };

  const paintPlace = (): void => {
    const hasTmux = facts?.hosts.includes("tmux") ?? true;
    tmuxRadio.label.hidden = !hasTmux;
    managedRadio.label.hidden = !(facts?.hosts.includes("managed") ?? false);
    windowRadio.note.textContent = windowNote(origin, hasTmux);
    tmuxRow.root.hidden = place() !== "tmux";
    promptInput.disabled = place() === "window";
  };

  const paintAgent = (): void => {
    const offered = facts?.agents ?? [];
    agentRow.root.hidden = offered.length <= 1 || ask !== null;
    agentSel.setOptions(
      offered.map((a) => ({ value: a.kind, label: a.text })),
      offered.some((a) => a.kind === agent) ? agent : undefined,
    );
    cmdInput.placeholder = copyText("newSession.command.placeholder", { launcher: safeLauncher(agent) });
  };

  const paintAccounts = async (): Promise<void> => {
    accounts = await listAccounts(origin, agent);
    accountRow.root.hidden = accounts === null || ask !== null;
    if (accounts === null) return;
    const opts: SelectOption[] = [];
    // 分叉、而那台说不出源会话的号 ⇒ 跟随（那台按源会话上次的号判），不拿当前号顶替。
    const forkAccount = facts?.fork?.launch.account;
    if (fork && forkAccount?.kind !== "known") opts.push({ value: ACCOUNT_FOLLOW, label: copyText("newSession.account.follow") });
    opts.push(...accounts.map(accountOption));
    accountSel.setOptions(opts, accountPick(fork !== null, forkAccount, accounts[0].name));
    paintAccountNote();
    void paintRotation();
  };

  const checkDir = async (): Promise<void> => {
    const seq = ++dirSeq;
    const cwd = cwdInput.value.trim();
    if (cwd === "" || !up()) return;
    try {
      const d = await askDir(origin, cwd, fork?.sid ?? null);
      if (seq !== dirSeq) return;
      tmuxInput.placeholder = d.tmuxName ?? "";
      if (d.exists) cwdRow.setNote("");
      else cwdRow.setNote(copyText("launch.dir.missing", { machine: machineName(origin) }), "warn");
    } catch {
      // 问不到不挡（点［新建］时那台照样判）。
    }
  };

  /** 分叉照源会话预填：那一家 · 轮次 · 目录 · 终端（那台说得出的才填）。 */
  const applyForkFacts = (title: string, ff: NonNullable<OpenFacts["fork"]>): void => {
    agent = ff.agent || agent;
    if (forkLine && ff.turn !== null) {
      forkLine.textContent = copyText("newSession.fork.from", { title, n: ff.turn, time: ff.startText ?? "" });
    }
    if (cwdInput.value === "" && ff.launch.cwd.kind === "known") cwdInput.value = ff.launch.cwd.value;
  };

  /** 那台答了能起什么：分叉照源会话预填；否则默认那一家起不了 ⇒ 换成那台能起的第一家。目录空着 ⇒ 最近用过的第一个。 */
  const applyFacts = (f: OpenFacts): void => {
    if (fork && f.fork) {
      applyForkFacts(fork.title, f.fork);
    } else {
      const kinds = f.agents.map((a) => a.kind);
      if (!kinds.includes(agent) && kinds.length > 0 && !kinds.includes(DEFAULT_AGENT)) agent = kinds[0];
    }
    if (!cwdLocked && cwdInput.value === "" && f.recent.length > 0) cwdInput.value = f.recent[0].cwd;
    // 默认「运行于」那台定（能托管就托管，分叉的源会话在 tmux 里 ⇒ tmux）。
    // 问图定在 tmux（没有 tmux 的那台在开框之前就拒了，`plan-ask` 的 `no_tmux`）。
    const pick: Place = ask !== null ? "tmux" : f.run === "managed" ? "managed" : f.run === "tmux" ? "tmux" : "window";
    radios[pick].input.checked = true;
  };

  const loadMachine = async (): Promise<void> => {
    facts = null;
    factsFailed = null;
    pendingTop = null;
    paintTop();
    handle.refresh();
    if (!up()) return;
    try {
      facts = await askFacts(origin, fork ? { sid: fork.sid, uuid: fork.uuid } : null);
    } catch (e) {
      factsFailed = copyText("newSession.facts.failed", { machine: machineName(origin), why: String(e) });
      paintTop();
      handle.refresh();
      return;
    }
    applyFacts(facts);
    cmdInput.value = await configuredCommand(origin, agent);
    paintAgent();
    paintPlace();
    await paintAccounts();
    void checkDir();
    handle.refresh();
  };

  /** 点［新建］交的那一份：哪一家 · 目录 · 放在哪 · 点名的号（跟随 ⇒ 不带）· 改过的终端名与命令 · 分叉的两个 id。 */
  const buildRequest = (): SessionOpenReq => {
    const accountValue = accountRow.root.hidden ? null : accountSel.value();
    const account: AccountAsk | undefined =
      accountValue === null || accountValue === ACCOUNT_FOLLOW ? undefined : chosenAccount(accountValue);
    const what: NewOpen = { agent, cwd: cwdInput.value.trim() };
    if (!fork && place() === "tmux" && tmuxInput.value.trim() !== "") what.tmuxName = tmuxInput.value.trim();
    if (cmdInput.value.trim() !== "") what.command = cmdInput.value.trim();
    if (fork) what.fork = { sid: fork.sid, uuid: fork.uuid };
    if (place() !== "window" && promptInput.value.trim() !== "") what.prompt = promptInput.value;
    if (!aliasRow.root.hidden && aliasSel.value() !== "") what.alias = aliasSel.value();
    // 新会话用的那份不写（新会话本来就用它）；挑了别的（含「不轮换」）⇒ 起之前写成那一份。
    const rot = rotRow.root.hidden ? forNew : rotSel.value();
    if (rot !== forNew) what.rotation = { strategy: rot === "" ? null : rot };
    const p = place();
    const req: SessionOpenReq = {
      what: { kind: "new", ...what },
      ...(p === null ? {} : { run: PLACE_RUN[p] }),
      local: isLocalOrigin(origin),
      ticket,
    };
    if (account) req.account = account;
    return req;
  };

  const submit = async (): Promise<{ said: string; detail: string } | null | false> => {
    pendingTop = null;
    paintTop();
    for (const r of [cwdRow, accountRow, tmuxRow, cmdRow, agentRow, placeRow]) r.setNote("");
    const req = buildRequest();
    const asked = JSON.stringify({ ...req, ticket: null });
    if (lastAsked !== null && asked !== lastAsked) {
      ticket = crypto.randomUUID();
      req.ticket = ticket;
    }
    lastAsked = asked;
    let res = await askOpen(origin, req);
    if (res.kind === "timeout") {
      // 结果未知：同一张票自己再核一次（那台认得出是不是同一趟 —— 起好了回原样那一份，落过去；不起第二个）。
      // 还没出错（正在再核）⇒ 中性那一档，不用错误色。
      sayTop(copyText("newSession.timeout.checking", { machine: machineName(origin) }), [spinner()], res.detail, "info");
      res = await askOpen(origin, req);
    }
    if (res.kind === "opened") {
      void landOpened(origin, req, res.opened, landing(origin));
      return null;
    }
    return showFailure(res);
  };

  /** 号选不了：那一格下是那台那一句 ＋［改用 {替代}］（有替代才给）＋［登录…］。 */
  const showAccountUnavailable = (u: AccountUnavailable, said: string): void => {
    const acts: HTMLElement[] = [];
    if (u.alternative) {
      const alt = u.alternative;
      const use = button({ label: copyText("newSession.account.useAlt", { alt }), size: "compact" });
      use.addEventListener("click", () => {
        accountSel.setValue(alt);
        paintAccountNote();
        handle.submit();
      });
      acts.push(use);
    }
    acts.push(loginBtn());
    accountRow.setNote(said, "error", acts);
  };

  /** ［再核一次］：同一张票再交一次（起好了 ⇒ 那台回原样那一份；那一趟根本没到 ⇒ 这次起）。 */
  const recheckBtn = (): HTMLElement => {
    const again = button({ label: copyText("newSession.recheck.action"), size: "compact" });
    again.addEventListener("click", () => handle.submit());
    return again;
  };

  /** 没起成的那几形落在哪：无应答 / 还在起 ⇒ 框顶 ＋［再核一次］（不给重试，免得起第二个）；某一格 ⇒ 那一格下；整体 ⇒ 按钮行上方一行红字 ＋［复制详情］。 */
  const showFailure = (res: Exclude<OpenEnd, { kind: "opened" }>): { said: string; detail: string } | false => {
    const machine = machineName(origin);
    if (res.kind === "timeout") {
      sayTop(copyText("newSession.timeout.unknown", { machine }), [recheckBtn()], res.detail);
      return false;
    }
    if (res.kind === "pending") {
      sayTop(res.said, [recheckBtn()], res.detail);
      return false;
    }
    if (res.kind === "unreachable") return { said: res.said, detail: res.detail };
    if (res.field === "account" && res.unavailable) {
      showAccountUnavailable(res.unavailable, res.said);
      return false;
    }
    const slot = fieldRow(res.field, { cwd: cwdRow, tmuxName: tmuxRow, command: cmdRow, agent: agentRow, place: placeRow, account: accountRow });
    if (slot && !slot.root.hidden) {
      slot.setNote(fieldSaid(res.code, res.said, machine), "error");
      return false;
    }
    return { said: res.said, detail: res.detail };
  };

  const handle = formDialog({
    title: dialogTitle(spec, origin),
    action: copyText("newSession.action.create"),
    body: form,
    narrow: true,
    focusAction: ask === null,
    blocked: () => {
      if (!up()) return offlineLine();
      if (onSet) return sets.blocked();
      if (facts === null) return copyText("kit.state.reading");
      if (ask && !aliasSel.value()) return copyText("newSession.ask.pick");
      const a = chosenAccountOpt();
      if (a && !a.ready && !accountRow.root.hidden) return a.why ?? "";
      if (cwdInput.value.trim() === "") return copyText("newSession.cwd.empty");
      return null;
    },
    submit: () => (onSet ? sets.submit() : submit()),
    dirty: () => true,
  });

  /** 换页 · 一组页的预览变了：露哪一份 · 页签勾哪一个 · 主按钮的字与能不能点。 */
  function paintPage(): void {
    // 一组页第一次露出来（或换了机器）才读那台存好的几组。
    if (onSet && setsOf !== origin) {
      setsOf = origin;
      void sets.load(origin, openSet);
      openSet = null;
    }
    single.hidden = onSet;
    sets.el.hidden = !onSet;
    tabs.set(onSet);
    handle.setAction(onSet ? sets.action() : copyText("newSession.action.create"));
    handle.refresh();
  }

  // 接线
  for (const inp of [cwdInput, tmuxInput, cmdInput]) {
    inp.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.isComposing) {
        ev.preventDefault();
        handle.submit();
      }
    });
  }
  cwdInput.addEventListener("input", () => {
    cwdRow.setNote("");
    handle.refresh();
  });
  cwdInput.addEventListener("blur", () => void checkDir());
  recentBtn.addEventListener("click", () => {
    const recent = facts?.recent ?? [];
    const use = (cwd: string): void => {
      cwdInput.value = cwd;
      cwdRow.setNote("");
      handle.refresh();
      void checkDir();
    };
    openMenu(
      { el: recentBtn, align: "end" },
      recent.length === 0
        ? [{ label: copyText("newSession.recent.none"), enabled: false }]
        : recent.map((r) => ({ label: r.shown, checked: r.cwd === cwdInput.value.trim(), onClick: () => use(r.cwd) })),
      { label: copyText("newSession.recent.label"), onClose: () => cwdInput.focus() },
    );
  });
  for (const r of Object.values(radios)) r.input.addEventListener("change", paintPlace);
  // 问图：打完问题 Enter 就起（Shift+Enter 换行）；光标先停在那台拼的那句末尾。
  if (ask) {
    promptInput.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.shiftKey && !ev.isComposing) {
        ev.preventDefault();
        handle.submit();
      }
    });
    // 调度：一次性 —— 框挂上之后的下一帧把光标放进开口那句末尾，只这一下
    requestAnimationFrame(() => {
      promptInput.focus();
      promptInput.setSelectionRange(promptInput.value.length, promptInput.value.length);
    });
  }

  // 机器清单（断开的灰着、第二行「离线」）；分叉 / 机器卡入口锁定那一台。
  void listMachines().then((ms) => {
    machines = ms.some((m) => m.origin === origin) ? ms : [...ms, { origin, up: false }];
    machineSel.setOptions(
      machines.map((m) => machineOption(m, origin)),
      origin,
    );
    machineSel.setDisabled(spec.lockMachine === true || fork !== null);
    void loadMachine();
  });
  void loadAliases();
  paintPage();
  paintPlace();
  await handle.done;
}

/** 那一家的默认启动器（画像里查不到 ⇒ 空串，不顶替成别的哪一家）。 */
function safeLauncher(agent: string): string {
  try {
    return defaultLauncherOf(agent);
  } catch {
    return "";
  }
}

/** 那台偏好里那一家的启动命令（`prefs-read`；和默认启动器一样 / 没设 / 问不到 ⇒ 空，表示用默认）。 */
async function configuredCommand(origin: Origin, agent: string): Promise<string> {
  try {
    const c = ((await readPrefs(origin)).prefs.launchers[agent] ?? "").trim();
    return c === safeLauncher(agent) ? "" : c;
  } catch {
    return "";
  }
}

/** ［登录…］：开设置的账号页那一台（登录在那里做）。 */
async function openLogin(origin: Origin): Promise<void> {
  const { openSettingsWindow } = await import("./open-settings");
  await openSettingsWindow(undefined, accountsOf(origin));
}

/** 不在主窗口里起的：报到了说一句「已启动」＋［切过去］（主窗口里起的由占位标签页接着）。 */
function landing(origin: Origin): OpenHow {
  return {
    onArrived: (sid: string, o: Opened) => {
      const name = o.session ?? o.cwd?.split("/").filter(Boolean).pop() ?? o.cwd ?? "";
      toast(copyText("launch.fromSettings.done", { name, machine: machineName(origin) }), "", {
        level: "info",
        action: {
          label: copyText("newSession.switch.action"),
          run: () => void emit(FOCUS_SESSION_EVENT, { origin, sid }).catch(() => {}),
        },
      });
    },
  };
}

/** 不开框、直接起一个（设置 › 账号页［在 tmux 里登录］）：同一个请求；结局与报错由 `session-open.ts` 说。回「起了没有」。 */
export function startNewSession(origin: Origin, req: SessionOpenReq): Promise<boolean> {
  return openSession(origin, req, landing(origin));
}
