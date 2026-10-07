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
import { toast } from "./kit/toast";
import { copyText } from "./copy-table";
import { commands } from "./ipc/commands";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { machineName } from "./control-said";
import { DEFAULT_AGENT, agentHasAccounts, defaultLauncherOf, lookupAgentProfile } from "./agent-profile";
import { fetchAccounts } from "./account-reads";
import { isSelectable, type Account } from "./accounts";
import { appStore } from "./app-store";
import { refreshQuota } from "./acct-center";
import { fiveHourCell } from "./quota-lines";
import { machineModels } from "./account-prefs";
import { getBehavior } from "./behavior";
import { resolveResumeCommand } from "./remote-config";
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

function accountLabel(a: AccountOpt): string {
  const parts = [a.name + (a.isDefault ? ` ${copyText("newSession.account.default")}` : "")];
  if (a.quota) parts.push(a.quota);
  if (!a.ready) parts.push(copyText("newSession.account.needLogin"));
  return parts.join(copyText("kit.text.sep"));
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

function option(value: string, label: string, disabled = false): HTMLOptionElement {
  const o = document.createElement("option");
  o.value = value;
  o.textContent = label;
  o.disabled = disabled;
  return o;
}

/** 「第 9 轮 02:05」那一段的时刻（本地钟面，时:分）。 */
function clock(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
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
    const box = el("div");
    box.className = s.nsFork;
    box.appendChild(icon("fork"));
    const text = el("div");
    text.className = s.nsForkText;
    const fromLine = el("div", copyText("newSession.fork.fromBare", { title: fork.title }));
    fromLine.className = s.nsForkFrom;
    const forkNote = el("div", copyText("newSession.fork.note"));
    forkNote.className = s.nsForkNote;
    text.append(fromLine, forkNote);
    box.appendChild(text);
    form.appendChild(box);
    forkLine = fromLine;
  }

  // 机器
  const machineSel = el("select");
  machineSel.className = s.nsSelect;
  machineSel.setAttribute("aria-label", copyText("newSession.label.machine"));
  const machineRow = row(copyText("newSession.label.machine"), machineSel);
  form.appendChild(machineRow.root);

  // 工作目录 ＋「最近的」
  const cwdInput = el("input");
  cwdInput.className = s.nsInput;
  cwdInput.type = "text";
  cwdInput.spellcheck = false;
  cwdInput.value = spec.cwd ?? "";
  cwdInput.setAttribute("aria-label", copyText("newSession.label.cwd"));
  const recentSel = el("select");
  recentSel.className = s.nsRecent;
  recentSel.setAttribute("aria-label", copyText("newSession.recent.label"));
  const cwdWrap = el("div");
  cwdWrap.className = s.nsBox;
  cwdWrap.append(cwdInput, recentSel);
  const cwdRow = row(copyText("newSession.label.cwd"), cwdWrap, false);
  form.appendChild(cwdRow.root);

  // agent（那台能起的多于一家才出）
  const agentSel = el("select");
  agentSel.className = s.nsSelect;
  agentSel.setAttribute("aria-label", copyText("newSession.label.agent"));
  const agentRow = row(copyText("newSession.label.agent"), agentSel);
  agentRow.root.hidden = true;
  form.appendChild(agentRow.root);

  // 账号
  const accountSel = el("select");
  accountSel.className = s.nsSelect;
  accountSel.setAttribute("aria-label", copyText("newSession.label.account"));
  const accountRow = row(copyText("newSession.label.account"), accountSel);
  accountRow.root.hidden = true;
  form.appendChild(accountRow.root);

  // 运行于
  const places = el("div");
  places.className = s.nsPlaces;
  const placeRow = row(copyText("newSession.label.place"), places, false);
  form.appendChild(placeRow.root);
  const radio = (value: "tmux" | "window"): { label: HTMLLabelElement; input: HTMLInputElement; note: HTMLElement } => {
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
  };
  const tmuxRadio = radio("tmux");
  const windowRadio = radio("window");
  tmuxRadio.note.textContent = copyText("newSession.place.tmuxNote");

  // 更多：tmux 会话名（分叉不出）· 启动命令
  const tmuxInput = el("input");
  tmuxInput.className = s.nsInput;
  tmuxInput.type = "text";
  tmuxInput.spellcheck = false;
  tmuxInput.setAttribute("aria-label", copyText("newSession.label.tmuxName"));
  const tmuxRow = row(copyText("newSession.label.tmuxName"), tmuxInput);
  const cmdInput = el("input");
  cmdInput.className = s.nsInput;
  cmdInput.type = "text";
  cmdInput.spellcheck = false;
  cmdInput.setAttribute("aria-label", copyText("newSession.label.command"));
  const cmdRow = row(copyText("newSession.label.command"), cmdInput);
  const more = el("div");
  more.className = s.nsMore;
  if (!fork) more.appendChild(tmuxRow.root);
  more.appendChild(cmdRow.root);
  form.appendChild(
    fold({ title: fork ? copyText("newSession.more.fork") : copyText("newSession.more.plain"), open: false, body: more }),
  );

  const place = (): "tmux" | "window" => (tmuxRadio.input.checked && !tmuxRadio.label.hidden ? "tmux" : "window");
  const up = (): boolean => machines.find((m) => m.origin === origin)?.up ?? false;
  const chosenAccountOpt = (): AccountOpt | null =>
    accounts?.find((a) => a.name === accountSel.value) ?? null;

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

  const paintAccountNote = (): void => {
    const a = chosenAccountOpt();
    if (a && !a.ready) {
      const login = button({ label: copyText("newSession.account.login"), kind: "ghost", size: "compact" });
      login.addEventListener("click", () => void openLogin(origin));
      accountRow.setNote(copyText("launch.account.notLoggedIn", { name: a.name }), "error", [login]);
    } else {
      accountRow.setNote("");
    }
  };

  const paintPlace = (): void => {
    const hasTmux = facts?.tmux ?? true;
    tmuxRadio.label.hidden = !hasTmux;
    windowRadio.note.textContent = isLocalOrigin(origin)
      ? copyText("newSession.place.windowNoteLocal")
      : hasTmux
        ? copyText("newSession.place.windowNote", { machine: machineName(origin) })
        : copyText("newSession.place.noTmux");
    if (!hasTmux) windowRadio.input.checked = true;
    else if (!tmuxRadio.input.checked && !windowRadio.input.checked) tmuxRadio.input.checked = true;
    tmuxRow.root.hidden = place() !== "tmux";
  };

  const paintAgent = (): void => {
    const offered = facts?.agents ?? [];
    agentRow.root.hidden = offered.length <= 1;
    agentSel.replaceChildren(...offered.map((k) => option(k, k)));
    if (offered.includes(agent)) agentSel.value = agent;
    cmdInput.placeholder = copyText("newSession.command.placeholder", { launcher: safeLauncher(agent) });
  };

  const paintAccounts = async (): Promise<void> => {
    accounts = await listAccounts(origin, agent);
    accountRow.root.hidden = accounts === null;
    if (accounts === null) return;
    const opts: HTMLOptionElement[] = [];
    // 分叉、而那台说不出源会话的号 ⇒ 跟随（那台按源会话上次的号判），不拿当前号顶替。
    const forkAccount = facts?.fork?.launch.account;
    if (fork && forkAccount?.kind !== "known") opts.push(option(ACCOUNT_FOLLOW, copyText("newSession.account.follow")));
    opts.push(...accounts.map((a) => option(a.name, accountLabel(a))));
    accountSel.replaceChildren(...opts);
    if (fork && forkAccount?.kind === "known" && forkAccount.value !== null) accountSel.value = forkAccount.value;
    else if (fork) accountSel.value = ACCOUNT_FOLLOW;
    else accountSel.value = accounts[0].name;
    paintAccountNote();
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
    if (fork && facts.fork) {
      agent = facts.fork.agent || agent;
      if (forkLine && facts.fork.turn !== null) {
        forkLine.textContent = copyText("newSession.fork.from", { title: fork.title, n: facts.fork.turn, time: clock(facts.fork.start) });
      }
      if (cwdInput.value === "" && facts.fork.launch.cwd.kind === "known") cwdInput.value = facts.fork.launch.cwd.value;
      const t = facts.fork.launch.terminal;
      if (t.kind === "known") (t.value.host === "none" ? windowRadio : tmuxRadio).input.checked = true;
    } else if (!facts.agents.includes(agent) && facts.agents.length > 0 && !facts.agents.includes(DEFAULT_AGENT)) {
      agent = facts.agents[0];
    }
    recentSel.replaceChildren(
      option("", copyText("newSession.recent.label")),
      ...(facts.recent.length === 0
        ? [option("", copyText("newSession.recent.none"), true)]
        : facts.recent.map((r) => option(r.cwd, r.cwd))),
    );
    recentSel.value = "";
    if (cwdInput.value === "" && facts.recent.length > 0) cwdInput.value = facts.recent[0].cwd;
    cmdInput.value = await configuredCommand(origin, agent);
    paintAgent();
    paintPlace();
    await paintAccounts();
    void checkDir();
    handle.refresh();
  };

  const submit = async (): Promise<string | null | false> => {
    pendingTop = null;
    paintTop();
    for (const r of [cwdRow, accountRow, tmuxRow, cmdRow, agentRow, placeRow]) r.setNote("");
    const accountValue = accountRow.root.hidden ? null : accountSel.value;
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
    const res = await askNew(origin, req);
    if (res.kind === "ok") {
      void afterStart(origin, res);
      return null;
    }
    showFailure(res);
    return false;
  };

  const showFailure = (res: Exclude<NewResult, { kind: "ok" }>): void => {
    const machine = machineName(origin);
    if (res.kind === "timeout") {
      const again = button({ label: copyText("newSession.retry.action"), size: "compact" });
      again.addEventListener("click", () => handle.submit());
      pendingTop = { text: copyText("launch.timeout.noAnswer", { machine }), acts: [again] };
      paintTop();
      return;
    }
    if (res.kind === "unreachable") {
      pendingTop = { text: res.said, acts: [] };
      paintTop();
      return;
    }
    if (res.field === "account" && res.unavailable) {
      const u = res.unavailable;
      const acts: HTMLElement[] = [];
      if (u.alternative) {
        const alt = u.alternative;
        const use = button({ label: copyText("newSession.account.useAlt", { alt }), size: "compact" });
        use.addEventListener("click", () => {
          accountSel.value = alt;
          paintAccountNote();
          handle.submit();
        });
        acts.push(use);
      }
      const login = button({ label: copyText("newSession.account.login"), kind: "ghost", size: "compact" });
      login.addEventListener("click", () => void openLogin(origin));
      acts.push(login);
      accountRow.setNote(
        u.pinned ? copyText("newSession.account.pinned") : copyText("launch.account.unavailable", { name: u.requested }),
        "error",
        acts,
      );
      return;
    }
    const slot =
      res.field === "cwd" ? cwdRow
        : res.field === "tmuxName" ? tmuxRow
          : res.field === "command" ? cmdRow
            : res.field === "agent" ? agentRow
              : res.field === "place" ? placeRow
                : res.field === "account" ? accountRow
                  : null;
    if (slot && !slot.root.hidden) {
      slot.setNote(fieldSaid(res.code, res.said, machine), "error");
      return;
    }
    pendingTop = { text: res.said, acts: [] };
    paintTop();
  };

  const title = fork
    ? copyText("newSession.title.fork")
    : spec.lockMachine
      ? copyText("newSession.title.onMachine", { machine: machineName(origin) })
      : copyText("newSession.title.plain");
  const handle = formDialog({
    title,
    action: copyText("newSession.action.create"),
    body: form,
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
  recentSel.addEventListener("change", () => {
    if (recentSel.value === "") return;
    cwdInput.value = recentSel.value;
    recentSel.value = "";
    cwdRow.setNote("");
    handle.refresh();
    void checkDir();
  });
  agentSel.addEventListener("change", () => {
    agent = agentSel.value;
    paintAgent();
    void configuredCommand(origin, agent).then((c) => (cmdInput.value = c));
    void paintAccounts().then(() => handle.refresh());
  });
  accountSel.addEventListener("change", () => {
    paintAccountNote();
    handle.refresh();
  });
  for (const r of [tmuxRadio, windowRadio]) r.input.addEventListener("change", paintPlace);
  machineSel.addEventListener("change", () => {
    origin = machineSel.value;
    cwdInput.value = "";
    cwdRow.setNote("");
    void loadMachine();
  });

  // 机器清单（连不上的灰着、第二行「离线」）；分叉 / 机器卡入口锁定那一台。
  void listMachines().then((ms) => {
    machines = ms.some((m) => m.origin === origin) ? ms : [...ms, { origin, up: false }];
    machineSel.replaceChildren(
      ...machines.map((m) =>
        option(m.origin, m.up ? machineName(m.origin) : `${machineName(m.origin)}${copyText("kit.text.sep")}${copyText("newSession.machine.down")}`, !m.up && m.origin !== origin),
      ),
    );
    machineSel.value = origin;
    machineSel.disabled = spec.lockMachine === true || fork !== null;
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
    const behavior = await getBehavior();
    const configured = isLocalOrigin(origin) ? behavior.resumeCommandLocal : await resolveResumeCommand(origin, behavior.resumeCommandRemote);
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

