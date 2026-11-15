/**
 * **「恢复」的那一组选项** —— 标签页右键「恢复 ▸」与历史页「恢复 ▾」同一个组件、同一套选项。
 *
 * 两组单选：「账号」（那台可选的号，每个号后面它的 `5h N%`、上次用的那个标「上次」，最后一项是基座）·
 * 「运行于」（在 tmux 里 / 不用 tmux）；最底下「在此目录新建会话」。点单选只挪勾、菜单不关、什么都不起；
 * 起由宿主按勾着的那一组做（历史页旁边的主按钮；标签页没有主按钮 ⇒ 最上面多一行「恢复」）。
 *
 * 只排版：有哪几个号、能不能选号（那一家有没有账号这一维）由那台的事实给；号是哪一家的、进不进选号表（`newSession`）·
 * 号名与 `5h N%` 都是那台 `accounts-list` 照这条会话那一家写好的（`label` · `slots` 第一格），这里不判；选了之后怎么起（问那台判号、起会话）由宿主做。
 */
import { copyText } from "./copy-table";
import { button, setDisabled } from "./kit/button";
import { fetchAccountsOf } from "./account-reads";
import { accountAvatarEl } from "./account-color";
import type { Machine } from "./ipc/origin";
import type { AccountsState } from "./accounts";
import type { Launch } from "./generated/Launch";
import type { AccountAsk } from "./generated/AccountAsk";
import { chosenAccount } from "./launch-account";
import type { MenuItem } from "./kit/menu";
import type { IconName } from "./kit/icon";

const NEW_IN_DIR_ICON: IconName = "plus";

/** 认不出是哪一家的那一行（那台判成 `no`、不带 `agent`）的［恢复］：没有那一家可问号、可起 ⇒ 一颗灰着的按钮照摆那台写好的那一句（历史页内容头 · 独立查看窗同一颗）。 */
export function resumeOff(why: string | null): HTMLButtonElement {
  const b = button({ label: copyText("history.row.resume"), kind: "primary" });
  setDisabled(b, why);
  return b;
}

/** 勾着的那一组：`run` = 放在哪（`null` ＝ 照这台的设置）· `account` = 点名的号（缺 = 跟随上次的号）· `useBase` = 基座（不隔离）。 */
export interface ResumePick {
  run: Launch | null;
  account: string | undefined;
  useBase: boolean;
}

/** 账号组里的一项。基座的 `name` 是 `null`。 */
export interface ResumeAccount {
  name: string | null;
  label: string;
  /** 那一行的第一格（`5h 41% ↻18:30`）；没有 · 基座 ⇒ `null`。 */
  quota: string | null;
  last: boolean;
}

/**
 * 账号组的事实：`list` 那台这一家可选的号 ＋ 基座（这一家一个号也没有 ⇒ 只剩基座）· `none` 这一家没有账号这一维（灰着一行说）·
 * `off` 那台没开多账号 / 读不到（这一组不出）。
 */
export type ResumeAccounts = { kind: "list"; items: ResumeAccount[]; /** 号在哪一台（头像照那台账号库的号色画）；缺 ⇒ 本机。 */ origin?: Machine } | { kind: "none"; agentName: string } | { kind: "off" };

export interface ResumeMenuSpec {
  accounts: ResumeAccounts;
  /** 勾着的那一组（点单选时就地改它）。 */
  pick: ResumePick;
  /** 这台恢复默认放在哪（`sessions-where` 答的）；`run` 没动过时勾画在它上面。没问到 ⇒ 两项都不勾。 */
  machineRun?: Launch;
  onChange?: (p: ResumePick) => void;
  /** 有它 ⇒ 最上面一行「恢复」，按勾着的那一组起（标签页右键：没有主按钮）。 */
  run?: (p: ResumePick) => void;
  /** 有它 ⇒ 最底下「在此目录新建会话」。 */
  newInDir?: () => void;
}

/** 默认那一组：上次的号（跟随）· 放在哪照这台（那台判）。 */
export function defaultPick(): ResumePick {
  return { run: null, account: undefined, useBase: false };
}

/** 勾着的那一组 ⇒ 请求里那两格：人没动过的不带（那台照跟随 · 照这台的设置）。 */
export function pickAsk(p: ResumePick): { account?: AccountAsk; run?: Launch } {
  const account = p.useBase ? chosenAccount(null) : p.account !== undefined ? chosenAccount(p.account) : undefined;
  return { ...(account ? { account } : {}), ...(p.run !== null ? { run: p.run } : {}) };
}

/** 勾着的号叫什么；跟随 ⇒ 上次的号（知道的话）；这一家没有账号这一维 ⇒ 那一家的名字。 */
function pickedAccountLabel(accounts: ResumeAccounts, p: ResumePick): string | null {
  if (p.useBase) return copyText("sessionOpen.account.useBase");
  if (p.account !== undefined) return p.account;
  if (accounts.kind === "none") return accounts.agentName;
  if (accounts.kind !== "list") return null;
  return accounts.items.find((a) => a.last && a.name !== null)?.name ?? null;
}

/** 勾着的那一组怎么说：`personal · tmux 里` / 没有号可说时 `tmux 里`；放在哪没动过、也没问到这台 ⇒ 照这台的设置。 */
export function pickWords(accounts: ResumeAccounts, p: ResumePick, machineRun?: Launch): string {
  const r = p.run ?? machineRun;
  const run =
    r === undefined
      ? copyText("resumeMenu.run.machine")
      : r === "managed"
        ? copyText("resumeMenu.hint.managed")
        : r === "tmux"
          ? copyText("resumeMenu.hint.tmux")
          : copyText("resumeMenu.run.direct");
  const account = pickedAccountLabel(accounts, p);
  return account === null ? run : copyText("resumeMenu.pick.account", { account, run });
}

/** 主按钮的悬停：`恢复 · personal · tmux 里`。 */
export function resumeHint(accounts: ResumeAccounts, p: ResumePick, machineRun?: Launch): string {
  return copyText("resumeMenu.hint.pick", { pick: pickWords(accounts, p, machineRun) });
}

/**
 * 那台的号 ⇒ 账号组：照这条会话的那一家（`agent`）问那台（新会话框同一问）；`hasAccounts` 是那一行的事实（这一家有没有账号这一维）。
 * 列那台说进选号表（`newSession`：号是不是这一家的只在那台判）、又选得了的那几行，最后是基座；那台没开多账号 / 读不到 ⇒ `off`；
 * 开了、这一家一个也没有 ⇒ 只剩基座（不拿别家的号顶）。号名与 `5h N%` 都照那台那一行（那一行已按那一家对好额度账）。
 */
export async function resumeAccounts(
  origin: Machine,
  facts: { agent: string; hasAccounts: boolean; agentName: string; last?: string },
): Promise<ResumeAccounts> {
  if (!facts.hasAccounts) return { kind: "none", agentName: facts.agentName };
  let rows: AccountsState["accounts"];
  try {
    const state = await fetchAccountsOf(origin, facts.agent);
    if (!state.available || !state.meta?.enabled) return { kind: "off" };
    rows = state.accounts.filter((a) => a.newSession && a.selectable);
  } catch {
    return { kind: "off" };
  }
  // 号名与号旁那一格都是那台那一行写好的（`label` · `slots` 第一格）；基座落在哪份登录说不准，不带格。
  const items: ResumeAccount[] = rows.map((a) => ({ name: a.name, label: a.label ?? a.name, quota: a.slots?.[0]?.text ?? null, last: a.name === facts.last }));
  items.push({ name: null, label: copyText("sessionOpen.account.useBase"), quota: null, last: false });
  return { kind: "list", items, origin };
}

/** 号那一行右侧那一格：`5h 41%` · 上次用的 `5h 41% · 上次`（右对齐，几行的数对得齐）。 */
function accountDetail(a: ResumeAccount): string | undefined {
  if (a.last) return a.quota === null ? copyText("resumeMenu.account.last") : copyText("resumeMenu.account.quotaLast", { quota: a.quota });
  return a.quota ?? undefined;
}

/** 「恢复」那一组的菜单项。 */
export function resumeMenuItems(spec: ResumeMenuSpec): MenuItem[] {
  const { accounts, pick, machineRun } = spec;
  const placed = pick.run ?? machineRun;
  const changed = (): void => spec.onChange?.(pick);
  const items: MenuItem[] = [];
  if (spec.run) {
    const run = spec.run;
    items.push(
      { label: copyText("history.row.resume"), detailOf: () => pickWords(accounts, pick, machineRun), onClick: () => run(pick) },
      { label: "", divider: true },
    );
  }
  if (accounts.kind !== "off") {
    items.push({ label: copyText("beProfile.slot.account"), heading: true });
    if (accounts.kind === "none") {
      items.push({ label: copyText("history.resume.noAccounts", { agent: accounts.agentName }), enabled: false });
    } else {
      const followed = pick.account === undefined && !pick.useBase;
      for (const a of accounts.items) {
        const on = a.name === null ? pick.useBase : pick.account === a.name || (followed && a.last);
        items.push({
          label: a.label,
          radio: "account",
          checked: on,
          avatar: a.name === null ? undefined : accountAvatarEl(a.name, { size: 16, origin: accounts.origin }),
          detail: accountDetail(a),
          onClick: () => {
            pick.account = a.name ?? undefined;
            pick.useBase = a.name === null;
            changed();
          },
        });
      }
    }
    items.push({ label: "", divider: true });
  }
  items.push(
    { label: copyText("resumeMenu.group.run"), heading: true },
    {
      label: copyText("resumeMenu.run.managed"),
      radio: "run",
      checked: placed === "managed",
      onClick: () => {
        pick.run = "managed";
        changed();
      },
    },
    {
      label: copyText("resumeMenu.run.tmux"),
      radio: "run",
      checked: placed === "tmux",
      onClick: () => {
        pick.run = "tmux";
        changed();
      },
    },
    {
      label: copyText("resumeMenu.run.direct"),
      radio: "run",
      checked: placed === "direct",
      onClick: () => {
        pick.run = "direct";
        changed();
      },
    },
  );
  if (spec.newInDir) {
    items.push({ label: "", divider: true }, { label: copyText("history.menu.newInDir"), icon: NEW_IN_DIR_ICON, onClick: spec.newInDir });
  }
  return items;
}
