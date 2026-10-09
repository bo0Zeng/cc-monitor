/**
 * **起新会话：全产品一个框、一个请求。** 命令面板「新建会话…」· 历史 / 标签页「在此目录新建会话」· 消息流「从这里分叉」·
 * 设置 › 机器卡 ⋯「新建会话…」都开这一个框（[`openNewSession`]）；设置 › 账号页［在 tmux 里登录］不开框、直接走同一个请求（[`startNewSession`]）。
 *
 * 判定都在那台后端（`new-session-reads.ts` 那三问）：目录在不在 · 用哪个号（选不了 ⇒ 不起、带替代号）· 终端名 · 能起哪几家 ·
 * 分叉只在点［新建］那一步写分支记录。这里只排版：那台说哪一格不行，错误句就落在那一格下；整体不行落在框顶。
 *
 * 起了之后：框立刻关；在主窗口里起的 ⇒ 先长出占位标签页、报到了换成真的（`launch-slot.ts`）；
 * 在别的窗口（设置 · 查看）起的 ⇒ 报到了说一句「已启动」＋［切过去］，没报到那一句由主窗口说（`launch-arrival.ts`）。
 */
import { emit } from "@tauri-apps/api/event";
import { formDialog } from "./kit/dialog";
import { banner } from "./kit/banner";
import { button } from "./kit/button";
import { fold } from "./kit/fold";
import { icon } from "./kit/icon";
import { openMenu } from "./kit/menu";
import { select, type SelectOption } from "./kit/select";
import { tag } from "./kit/badge";
import { toast } from "./kit/toast";
import { copyText } from "./copy-table";
import { readRules, type RulesRead } from "./quota-reads";
import { commands } from "./ipc/commands";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { machineName } from "./control-said";
import { DEFAULT_AGENT, agentHasAccounts, defaultLauncherOf, lookupAgentProfile } from "./agent-profile";
import { fetchAccounts } from "./account-reads";
import { isSelectable, type Account } from "./accounts";
import { accountAvatarEl } from "./account-color";
import { appStore } from "./app-store";
import { refreshQuota } from "./acct-center";
import { fiveHourCell } from "./quota-lines";
import { machineModels } from "./account-prefs";
import { resumeCommandFor } from "./remote-config";
import { configuredLauncherFor } from "./launch-requests";
import { chosenAccount, type AccountAsk } from "./launch-account";
import { openWindow } from "./tab-batch-run";
import { awaitArrival, type ArrivalMatch } from "./launch-arrival";
import type { SlotSpec } from "./launch-slot";
import { accountsOf } from "./settings-dest";
import { askDir, askFacts, askNew, type NewFacts, type NewRequest, type NewResult } from "./new-session-reads";
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
  /** 预选哪一家（历史条目的那一家；缺 ⇒ 默认那一家）。 */
  agent?: string;
  /** 分叉：源会话 · 从哪条消息处 · 源会话的标题（框顶那一句）。 */
  fork?: { sid: string; uuid: string; title: string };
}

/** 主窗口：起好了 ⇒ 长出占位标签页（`main.ts` 装一次；别的窗口没有它 ⇒ 等报到、说一句「已启动」）。 */
/** 「轮换」那一行跟随默认那一项的值。 */
const ROT_FOLLOW = "follow";

let placeholder: ((spec: SlotSpec) => void) | null = null;
export function setNewSessionPlaceholder(fn: ((spec: SlotSpec) => void) | null): void {
  placeholder = fn;
}

/** 一台机器那一项：连没连上。 */
interface MachineOpt {
  origin: Origin;
  up: boolean;
}

async function listMachines(): Promise<MachineOpt[]> {
  let origins: string[] = [];
  try {
    const got: unknown = await commands.backend_machines();
    origins = Array.isArray(got) ? got.filter((o): o is string => typeof o === "string") : [];
  } catch {
    origins = [];
  }
  if (!origins.some(isLocalOrigin)) origins.unshift(LOCAL_ORIGIN);
  return Promise.all(
    origins.map(async (origin) => {
      try {
        const st = await commands.backend_status({ origin });
        return { origin, up: st.channel === true };
      } catch {
        return { origin, up: false };
      }
    }),
  );
}

/** 账号那一格的一项。`ready: false` ⇒ 需登录。 */
interface AccountOpt {
  name: string;
  isDefault: boolean;
  ready: boolean;
  quota: string | null;
}

/** 那台的号（账号 0 之外、隔离的那几个）；那台没开多账号 / 读不出 ⇒ `null`（这一行不出）。 */
async function listAccounts(origin: Origin, agent: string): Promise<AccountOpt[] | null> {
  if (!agentHasAccounts(agent)) return null;
  let list: Account[];
  try {
    const st = await fetchAccounts(origin);
    if (!st.available) return null;
    list = st.accounts.filter((a) => a.configDir !== null && a.mode === "isolated");
  } catch {
    return null;
  }
  if (list.length === 0) return null;
  if (!appStore.quota.get().has(origin)) await refreshQuota(origin).catch(() => {});
  const q = appStore.quota.get().get(origin);
  const prof = lookupAgentProfile(agent);
  const opts = list.map((a) => ({
    name: a.name,
    isDefault: a.isDefault,
    ready: isSelectable(a),
    quota: prof.known ? fiveHourCell(q, prof.facts.adapterId, a.name) : null,
  }));
  return [...opts.filter((a) => a.isDefault), ...opts.filter((a) => !a.isDefault)];
}

/** 账号那一项：头像 · 名字 · 灰字（`默认 · 5h 63%` · 需登录）。 */
function accountOption(a: AccountOpt): SelectOption {
  const note: string[] = [];
  if (a.isDefault) note.push(copyText("newSession.account.default"));
  if (a.quota) note.push(a.quota);
  if (!a.ready) note.push(copyText("newSession.account.needLogin"));
  return { value: a.name, label: a.name, note: note.join(copyText("kit.text.sep")), lead: () => accountAvatarEl(a.name, { size: 16 }) };
}

/** 机器那一项：远端带机器标记；连不上的灰着、说「离线」（正选着的那台照样列出、可选）。 */
function machineOption(m: MachineOpt, chosen: Origin): SelectOption {
  const name = machineName(m.origin);
  return {
    value: m.origin,
    label: name,
    lead: isLocalOrigin(m.origin) ? undefined : () => tag(name),
    note: m.up ? undefined : copyText("newSession.machine.down"),
    enabled: m.up || m.origin === chosen,
    why: copyText("newSession.machine.down"),
  };
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (text !== undefined) e.textContent = text;
  return e;
}

/** 一行：左标签 ＋ 右格（格里一个控件框 ＋ 一行说明 / 错误）。 */
function row(label: string, control: HTMLElement, boxed = true): { root: HTMLElement; cell: HTMLElement; note: HTMLElement; setNote: (text: string, tone?: "error" | "warn", acts?: HTMLElement[]) => void } {
  const root = el("div");
  root.className = s.nsRow;
  const lab = el("div", label);
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
    if (text) note.append(el("span", text));
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
  const fromLine = el("div", copyText("newSession.fork.fromBare", { title }));
  fromLine.className = s.nsForkFrom;
  const forkNote = el("div", copyText("newSession.fork.note"));
  forkNote.className = s.nsForkNote;
  text.append(fromLine, forkNote);
  box.appendChild(text);
  return { box, fromLine };
}

/** 工作目录那一行：目录格 ＋「最近的」（那台最近用过的目录，点一项填进目录格）。 */
function cwdField(cwd: string): { cwdInput: HTMLInputElement; recentBtn: HTMLButtonElement; cwdRow: Row } {
  const cwdInput = el("input");
  cwdInput.className = s.nsInput;
  cwdInput.type = "text";
  cwdInput.spellcheck = false;
  cwdInput.value = cwd;
  cwdInput.setAttribute("aria-label", copyText("newSession.label.cwd"));
  const recentBtn = el("button");
  recentBtn.type = "button";
  recentBtn.className = s.nsRecent;
  recentBtn.setAttribute("aria-label", copyText("newSession.recent.label"));
  recentBtn.append(el("span", copyText("newSession.recent.label")), icon("caretDown", "compact"));
  const cwdWrap = el("div");
  cwdWrap.className = s.nsBox;
  cwdWrap.append(cwdInput, recentBtn);
  return { cwdInput, recentBtn, cwdRow: row(copyText("newSession.label.cwd"), cwdWrap, false) };
}

interface PlaceRadio {
  label: HTMLLabelElement;
  input: HTMLInputElement;
  note: HTMLElement;
}

/** 「运行于」那一张：单选 ＋ 名字 ＋ 灰字。 */
function placeRadio(places: HTMLElement, value: "tmux" | "window"): PlaceRadio {
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
  t.append(el("div", value === "tmux" ? copyText("newSession.place.tmux") : copyText("newSession.place.window")), note);
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
function accountPick(forking: boolean, forkAccount: { kind: string; value?: string | null } | undefined, first: string): string {
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
  return spec.lockMachine ? copyText("newSession.title.onMachine", { machine: machineName(origin) }) : copyText("newSession.title.plain");
};

/** 打开起新会话框。取消 ⇒ 什么都不起不写。 */
export async function openNewSession(spec: NewSessionSpec = {}): Promise<void> {
  const fork = spec.fork ?? null;
  let origin: Origin = spec.origin ?? LOCAL_ORIGIN;
  let facts: NewFacts | null = null;
  let factsFailed: string | null = null;
  let agent = spec.agent ?? DEFAULT_AGENT;
  let accounts: AccountOpt[] | null = null;
  let machines: MachineOpt[] = [{ origin, up: true }];
  let forkLine: HTMLElement | null = null;
  let dirSeq = 0;
  let pendingTop: { text: string; acts: HTMLElement[] } | null = null;

  const form = el("div");
  form.className = s.nsForm;
  const top = el("div");
  top.className = s.nsTop;
  form.appendChild(top);

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
    },
  });
  const machineRow = row(copyText("newSession.label.machine"), machineSel.el, false);
  form.appendChild(machineRow.root);

  // 工作目录 ＋「最近的」
  const { cwdInput, recentBtn, cwdRow } = cwdField(spec.cwd ?? "");
  form.appendChild(cwdRow.root);

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
  form.appendChild(agentRow.root);

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
  form.appendChild(accountRow.root);

  // 轮换（稿 §5.4）：跟随默认（默认那条的名字）· 那台的各条规则；没有「本会话」（新会话还没有自己那一份）。读不出那台的规则表 ⇒ 不出这一行。
  const rotSel = select({ label: copyText("newSession.label.rot"), options: [] });
  const rotRow = row(copyText("newSession.label.rot"), rotSel.el, false);
  rotRow.root.hidden = true;
  form.appendChild(rotRow.root);
  let rotSeq = 0;
  const paintRotation = async (): Promise<void> => {
    const my = ++rotSeq;
    let rules: RulesRead | null = null;
    try {
      rules = accounts === null ? null : await readRules(origin);
    } catch {
      rules = null;
    }
    if (my !== rotSeq) return;
    rotRow.root.hidden = rules === null;
    if (rules === null) return;
    const def = rules.rules.find((r) => r.id === rules.defaultRule);
    rotSel.setOptions(
      [
        { value: ROT_FOLLOW, label: copyText("rot.src.followOf", { name: def?.name ?? "" }) },
        ...rules.rules.map((r) => ({
          value: `rule:${r.id}`,
          label: r.name,
          note: r.isDefault ? copyText("rot.src.tagDefault") : undefined,
        })),
      ],
      ROT_FOLLOW,
    );
  };

  // 运行于
  const places = el("div");
  places.className = s.nsPlaces;
  const placeRow = row(copyText("newSession.label.place"), places, false);
  form.appendChild(placeRow.root);
  const tmuxRadio = placeRadio(places, "tmux");
  const windowRadio = placeRadio(places, "window");
  tmuxRadio.note.textContent = copyText("newSession.place.tmuxNote");

  // 更多
  const { tmuxInput, tmuxRow, cmdInput, cmdRow, moreRow } = moreFields(fork !== null);
  form.appendChild(moreRow);

  const place = (): "tmux" | "window" => (tmuxRadio.input.checked && !tmuxRadio.label.hidden ? "tmux" : "window");
  const up = (): boolean => machines.find((m) => m.origin === origin)?.up ?? false;
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
      top.appendChild(banner("error", copyText("newSession.machine.offline", { machine: machineName(origin) }), [again]));
    } else if (factsFailed !== null) {
      top.appendChild(banner("error", factsFailed));
    } else if (pendingTop) {
      top.appendChild(banner("error", pendingTop.text, pendingTop.acts));
    }
  };
  /** 整体不行的那一句落在框顶。 */
  const sayTop = (text: string, acts: HTMLElement[] = []): void => {
    pendingTop = { text, acts };
    paintTop();
  };

  const paintAccountNote = (): void => {
    const a = chosenAccountOpt();
    if (a && !a.ready) {
      accountRow.setNote(copyText("launch.account.notLoggedIn", { name: a.name }), "error", [loginBtn()]);
    } else {
      accountRow.setNote("");
    }
  };

  const paintPlace = (): void => {
    const hasTmux = facts?.tmux ?? true;
    tmuxRadio.label.hidden = !hasTmux;
    windowRadio.note.textContent = windowNote(origin, hasTmux);
    if (!hasTmux) windowRadio.input.checked = true;
    else if (!tmuxRadio.input.checked && !windowRadio.input.checked) tmuxRadio.input.checked = true;
    tmuxRow.root.hidden = place() !== "tmux";
  };

  const paintAgent = (): void => {
    const offered = facts?.agents ?? [];
    agentRow.root.hidden = offered.length <= 1;
    agentSel.setOptions(
      offered.map((k) => ({ value: k, label: k })),
      offered.includes(agent) ? agent : undefined,
    );
    cmdInput.placeholder = copyText("newSession.command.placeholder", { launcher: safeLauncher(agent) });
  };

  const paintAccounts = async (): Promise<void> => {
    accounts = await listAccounts(origin, agent);
    accountRow.root.hidden = accounts === null;
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
  const applyForkFacts = (title: string, ff: NonNullable<NewFacts["fork"]>): void => {
    agent = ff.agent || agent;
    if (forkLine && ff.turn !== null) {
      forkLine.textContent = copyText("newSession.fork.from", { title, n: ff.turn, time: ff.startText ?? "" });
    }
    if (cwdInput.value === "" && ff.launch.cwd.kind === "known") cwdInput.value = ff.launch.cwd.value;
    const t = ff.launch.terminal;
    if (t.kind === "known") (t.value.host === "none" ? windowRadio : tmuxRadio).input.checked = true;
  };

  /** 那台答了能起什么：分叉照源会话预填；否则默认那一家起不了 ⇒ 换成那台能起的第一家。目录空着 ⇒ 最近用过的第一个。 */
  const applyFacts = (f: NewFacts): void => {
    if (fork && f.fork) {
      applyForkFacts(fork.title, f.fork);
    } else if (!f.agents.includes(agent) && f.agents.length > 0 && !f.agents.includes(DEFAULT_AGENT)) {
      agent = f.agents[0];
    }
    if (cwdInput.value === "" && f.recent.length > 0) cwdInput.value = f.recent[0].cwd;
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

  /** 点［新建］交的那一份：目录 · 放在哪 · 点名的号（跟随 ⇒ 不带）· 改过的终端名与命令 · 分叉的两个 id。 */
  const buildRequest = async (): Promise<NewRequest> => {
    const accountValue = accountRow.root.hidden ? null : accountSel.value();
    const account: AccountAsk | undefined =
      accountValue === null || accountValue === ACCOUNT_FOLLOW ? undefined : chosenAccount(accountValue);
    const req: NewRequest = {
      agent,
      cwd: cwdInput.value.trim(),
      place: place(),
      local: isLocalOrigin(origin),
      models: await machineModels(origin).catch(() => ({})),
    };
    if (account) req.account = account;
    if (!fork && place() === "tmux" && tmuxInput.value.trim() !== "") req.tmuxName = tmuxInput.value.trim();
    if (cmdInput.value.trim() !== "") req.command = cmdInput.value.trim();
    if (fork) req.forkFrom = { sid: fork.sid, uuid: fork.uuid };
    const rot = rotRow.root.hidden ? ROT_FOLLOW : rotSel.value();
    if (rot.startsWith("rule:")) req.rotation = { rule: rot.slice("rule:".length) };
    return req;
  };

  const submit = async (): Promise<string | null | false> => {
    pendingTop = null;
    paintTop();
    for (const r of [cwdRow, accountRow, tmuxRow, cmdRow, agentRow, placeRow]) r.setNote("");
    const res = await askNew(origin, await buildRequest());
    if (res.kind === "ok") {
      void afterStart(origin, res);
      return null;
    }
    showFailure(res);
    return false;
  };

  /** 号选不了：那一格下说为什么 ＋［改用 {替代}］（有替代才给）＋［登录…］。 */
  const showAccountUnavailable = (u: NonNullable<Extract<NewResult, { kind: "refused" }>["unavailable"]>): void => {
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
    accountRow.setNote(
      u.pinned ? copyText("newSession.account.pinned") : copyText("launch.account.unavailable", { name: u.requested }),
      "error",
      acts,
    );
  };

  const showFailure = (res: Exclude<NewResult, { kind: "ok" }>): void => {
    const machine = machineName(origin);
    if (res.kind === "timeout") {
      const again = button({ label: copyText("newSession.retry.action"), size: "compact" });
      again.addEventListener("click", () => handle.submit());
      sayTop(copyText("launch.timeout.noAnswer", { machine }), [again]);
      return;
    }
    if (res.kind === "unreachable") {
      sayTop(res.said);
      return;
    }
    if (res.field === "account" && res.unavailable) {
      showAccountUnavailable(res.unavailable);
      return;
    }
    const slot = fieldRow(res.field, { cwd: cwdRow, tmuxName: tmuxRow, command: cmdRow, agent: agentRow, place: placeRow, account: accountRow });
    if (slot && !slot.root.hidden) {
      slot.setNote(fieldSaid(res.code, res.said, machine), "error");
      return;
    }
    sayTop(res.said);
  };

  const handle = formDialog({
    title: dialogTitle(spec, origin),
    action: copyText("newSession.action.create"),
    body: form,
    narrow: true,
    focusAction: true,
    blocked: () => {
      if (!up()) return copyText("newSession.machine.offline", { machine: machineName(origin) });
      if (facts === null) return copyText("newSession.facts.loading");
      const a = chosenAccountOpt();
      if (a && !a.ready && !accountRow.root.hidden) return copyText("launch.account.notLoggedIn", { name: a.name });
      if (cwdInput.value.trim() === "") return copyText("newSession.cwd.empty");
      return null;
    },
    submit,
    dirty: () => true,
  });

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
        : recent.map((r) => ({ label: r.cwd, checked: r.cwd === cwdInput.value.trim(), onClick: () => use(r.cwd) })),
      { label: copyText("newSession.recent.label"), onClose: () => cwdInput.focus() },
    );
  });
  for (const r of [tmuxRadio, windowRadio]) r.input.addEventListener("change", paintPlace);

  // 机器清单（连不上的灰着、第二行「离线」）；分叉 / 机器卡入口锁定那一台。
  void listMachines().then((ms) => {
    machines = ms.some((m) => m.origin === origin) ? ms : [...ms, { origin, up: false }];
    machineSel.setOptions(
      machines.map((m) => machineOption(m, origin)),
      origin,
    );
    machineSel.setDisabled(spec.lockMachine === true || fork !== null);
    void loadMachine();
  });
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

/** 设置里配的启动命令（只给默认那一家；和默认启动器一样 ⇒ 空，表示用默认）。 */
async function configuredCommand(origin: Origin, agent: string): Promise<string> {
  try {
    const configured = await resumeCommandFor(origin);
    const c = configuredLauncherFor(agent, configured).trim();
    return c === safeLauncher(agent) ? "" : c;
  } catch {
    return "";
  }
}

/** ［登录…］：开设置的账号页那一台（登录在那里做）。 */
async function openLogin(origin: Origin): Promise<void> {
  const { openSettingsWindow } = await import("./settings/open-settings");
  await openSettingsWindow(undefined, accountsOf(origin));
}

/** 起了之后：开窗那一形先开窗；等那台报出这个会话 ⇒ 主窗口里切过去，别的窗口（设置 · 查看）说一句「已启动」＋［切过去］。 */
async function afterStart(origin: Origin, res: Extract<NewResult, { kind: "ok" }>): Promise<void> {
  const r = res.reply;
  if (r.outcome === "open" && r.cmd !== null) {
    const failed = await openWindow(origin, r.cmd, r.cwd);
    if (failed !== null) {
      toast(copyText("newSession.window.failed"), failed);
      return;
    }
  }
  const match: ArrivalMatch = r.sid !== null ? { sid: r.sid } : { cwd: r.cwd };
  if (placeholder) {
    placeholder({ origin, cwd: r.cwd, tmuxName: r.session, agent: r.agent, match });
    return;
  }
  const sid = await awaitArrival({ origin, match, tmuxName: r.session, arrived: null });
  if (sid === null) return;
  const name = r.session ?? r.cwd.split("/").filter(Boolean).pop() ?? r.cwd;
  toast(copyText("launch.fromSettings.done", { name, machine: machineName(origin) }), "", {
    level: "info",
    action: {
      label: copyText("newSession.switch.action"),
      run: () => void emit(FOCUS_SESSION_EVENT, { origin, sid }).catch(() => {}),
    },
  });
}

/** 不开框、直接起一个（设置 › 账号页［在 tmux 里登录］）：同一个请求；结局与报错这里说。回「起了没有」。 */
export async function startNewSession(r: { origin: Origin; cwd: string; account: AccountAsk; place: "tmux" | "window"; agent?: string }): Promise<boolean> {
  const res = await askNew(r.origin, {
    agent: r.agent ?? DEFAULT_AGENT,
    cwd: r.cwd,
    account: r.account,
    place: r.place,
    local: isLocalOrigin(r.origin),
  });
  if (res.kind === "ok") {
    void afterStart(r.origin, res);
    return true;
  }
  const machine = machineName(r.origin);
  const said =
    res.kind === "timeout"
      ? copyText("launch.timeout.noAnswer", { machine })
      : res.kind === "unreachable"
        ? res.said
        : fieldSaid(res.code, res.said, machine);
  toast(copyText("newSession.start.failed"), said);
  return false;
}

