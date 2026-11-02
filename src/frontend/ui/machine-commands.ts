/**
 * 命令面板里的「机器」条目：打中一台（本机后端 `machine-find`）⇒ 问它那几样（`machine-menu`），拼成面板那一块；
 * 空输入时每台一行（`machine-read` 那份成品），Enter 只把名字填进框。
 *
 * 认哪台、哪样灰、灰着说什么、副文都是后端写好的；这里只把回包摆成几条 {@link Command}（标题是骨架字）、
 * 按那台挑出会话行（标签页栏的先后），再接上各样动作的去处。
 */
import type { Command, MachineBlock, MachineSource } from "./views/command-bar";
import type { MachineHit } from "./generated/MachineHit";
import type { MachineMenu } from "./generated/MachineMenu";
import type { MenuAct } from "./generated/MenuAct";
import type { MachineState } from "./generated/MachineState";
import type { IconName } from "./kit/icon";
import { machineDot } from "./machine-dot";
import { copyText } from "./copy-table";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { isLocalOrigin, LOCAL, type Machine } from "./ipc/origin";
import { NoTerminalWindow, openLoginShell, sayNoTerminal } from "./terminal-open";
import { openNewSession } from "./new-session";
import { openOnMachine } from "./file-window";
import { openSettingsWindow } from "./open-settings";
import { addMachineOf, machinePageOf } from "./settings-dest";
import { toast } from "./kit/toast";
import { detailOf, saidOf } from "./kit/detail";

/** 各样动作的去处（调用方给；判据给假的）。 */
export interface MachineActions {
  openTerminal(origin: Machine): void;
  newSession(origin: Machine): void;
  files(origin: Machine): void;
  status(origin: Machine): void;
  addMachine(origin: Machine): void;
}

const TITLE: Record<MenuAct, () => string> = {
  open_terminal: () => copyText("commandBar.machine.openTerminal"),
  new_session: () => copyText("commandBar.machine.newSession"),
  files: () => copyText("commandBar.machine.files"),
  machine_status: () => copyText("commandBar.machine.status"),
  add_machine: () => copyText("commandBar.machine.add"),
};

const ICON: Record<MenuAct, IconName> = {
  open_terminal: "terminal",
  new_session: "plus",
  files: "files",
  machine_status: "settings",
  add_machine: "plus",
};

const RUN: Record<MenuAct, (to: MachineActions, origin: Machine) => void> = {
  open_terminal: (to, o) => to.openTerminal(o),
  new_session: (to, o) => to.newSession(o),
  files: (to, o) => to.files(o),
  machine_status: (to, o) => to.status(o),
  add_machine: (to, o) => to.addMachine(o),
};

/**
 * 打中的那台 ⇒ 面板最前那一块：头（不可选）· 动作几样 · 这台的会话（`sessions` 里那台的，照给的先后编号）。
 * 带了第几条、那一条在 ⇒ 选中它；否则选中第一样能做的。纯函数。
 */
export function machineBlock(hit: MachineHit, menu: MachineMenu, sessions: readonly Command[], to: MachineActions): MachineBlock {
  const origin = hit.machine;
  const name = menu.head?.name ?? origin;
  // 没有成品 ＝ 只在 ssh 配置里那一种（后端只在那时不给头）。
  const head: Command = {
    id: `machine-head-${origin}`,
    title: name,
    machine: menu.head
      ? { dot: machineDot(menu.head.state), name, word: menu.head.word, said: menu.head.said, right: menu.head.os ?? "" }
      : { dot: null, name, word: copyText("commandBar.machine.unlisted"), said: "", right: copyText("commandBar.machine.fromSshConfig") },
    run: () => {},
  };
  const actions: Command[] = menu.actions.map((a) => ({
    id: `machine-${a.act}-${origin}`,
    title: TITLE[a.act](),
    icon: ICON[a.act],
    sub: a.sub ?? undefined,
    note: a.note ?? undefined,
    disabled: a.blocked ?? undefined,
    run: () => RUN[a.act](to, origin),
  }));
  const mine = sessions.filter((c) => c.session?.origin === origin).map((c, i) => ({ ...c, hint: String(i + 1) }));
  const pick = hit.nth !== null ? mine[hit.nth - 1] : undefined;
  const sections = [{ title: "", items: [head, ...actions] }];
  if (mine.length > 0) sections.push({ title: copyText("commandBar.group.sessions"), items: mine });
  return {
    sections,
    select: pick?.id ?? actions.find((a) => !a.disabled)?.id ?? null,
    claimed: new Set(mine.map((c) => c.id)),
  };
}

/** 空输入时「机器」那一段：每台一行（本机在前，其余照回包的先后）；Enter 不执行、把名字填进框。纯函数。 */
export function machineRows(states: Readonly<Record<string, MachineState>>): Command[] {
  const keys = Object.keys(states);
  const ordered = [...keys.filter((k) => isLocalOrigin(k)), ...keys.filter((k) => !isLocalOrigin(k))];
  return ordered.map((k) => {
    const m = states[k];
    return {
      id: `machine-row-${k}`,
      group: "machine",
      title: m.name,
      sub: m.word || undefined,
      dot: machineDot(m.state),
      fill: m.name,
      run: () => {},
    };
  });
}

/** 出错那一句照后端 / 壳交回的原话说（面板不另写）。 */
function sayFailed(e: unknown): void {
  toast(saidOf(e), "", { level: "error", detail: detailOf(e) });
}

/** 各样动作照产品里现成的那几处去：开终端（`terminal-open.ts`）· 起新会话框 · 文件窗口 · 设置 › 机器。 */
export const machineActions: MachineActions = {
  openTerminal: (origin) => {
    openLoginShell(origin).catch((e: unknown) => {
      if (e instanceof NoTerminalWindow) sayNoTerminal(e);
      else sayFailed(e);
    });
  },
  newSession: (origin) => void openNewSession({ origin }),
  // 落在那台的主目录（`dir` 空）。
  files: (origin) => void openOnMachine(origin, { dir: "" }).catch(sayFailed),
  status: (origin) => void openSettingsWindow(undefined, machinePageOf(origin)),
  addMachine: (origin) => void openSettingsWindow(undefined, addMachineOf(origin)),
};

/** `machine-find` · `machine-menu` · `machine-read` 等多久（本机后端读几份文件，不拨号）。 */
const ASK_BUDGET_MS = 5_000;

/** 面板问后端的那一份：打的字 ⇒ 那一块；空输入 ⇒ 每台一行。问不到 ⇒ 不出那一块（面板照常列别的）。 */
export function machineSource(to: MachineActions): MachineSource {
  return {
    async rows() {
      const r = await callTyped(LOCAL, "machine-read", {}, budgetWithin(ASK_BUDGET_MS));
      return machineRows(r.machines);
    },
    async block(query, sessions) {
      const { hit } = await callTyped(LOCAL, "machine-find", { query }, budgetWithin(ASK_BUDGET_MS));
      if (!hit) return null;
      const menu = await callTyped(LOCAL, "machine-menu", { machine: hit.machine }, budgetWithin(ASK_BUDGET_MS));
      return machineBlock(hit, menu, sessions, to);
    },
  };
}
