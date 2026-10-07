/**
 * **「恢复」的那一组选项** —— 标签页右键「恢复 ▸」与历史页「恢复 ▾」同一个组件、同一套选项。
 *
 * 两组单选：「账号」（那台可选的号，每个号后面它的 `5h N%`、上次用的那个标「上次」，最后一项是基座）·
 * 「运行于」（在 tmux 里 / 不用 tmux）；最底下「在此目录新建会话」。点单选只挪勾、菜单不关、什么都不起；
 * 起由宿主按勾着的那一组做（历史页旁边的主按钮；标签页没有主按钮 ⇒ 最上面多一行「恢复」）。
 *
 * 只排版：有哪几个号、能不能选号（那一家有没有账号这一维）由那台的事实给；`5h N%` 读额度账的显示态（`quota-read`），
 * 这里不判；选了之后怎么起（问那台判号、起会话）由宿主做。
 */
import { copyText } from "./copy-table";
import { appStore } from "./app-store";
import { refreshQuota } from "./acct-center";
import { fetchAccounts } from "./account-reads";
import { selectableAccounts } from "./accounts";
import { accountAvatarEl } from "./account-color";
import { fiveHourCell } from "./quota-lines";
import { lookupAgentProfile } from "./agent-profile";
import type { Origin } from "./ipc/origin";
import { resumeInTmuxFor } from "./resume-defaults";
import type { MenuItem } from "./kit/menu";
import type { IconName } from "./kit/icon";

const NEW_IN_DIR_ICON: IconName = "plus";

/** 勾着的那一组：`tmux` = 在 tmux 里（否则不用 tmux）· `account` = 点名的号（缺 = 跟随上次的号）· `useBase` = 基座（不隔离）。 */
export interface ResumePick {
  tmux: boolean;
  account: string | undefined;
  useBase: boolean;
}

/** 账号组里的一项。基座的 `name` 是 `null`。 */
export interface ResumeAccount {
  name: string | null;
  label: string;
  /** `5h 41%`；账上没数 ⇒ `null`。 */
  quota: string | null;
  last: boolean;
}

/** 账号组的事实：`list` 那台可选的号 · `none` 这一家没有账号这一维（灰着一行说）· `off` 那台没开多账号 / 读不到（这一组不出）。 */
export type ResumeAccounts = { kind: "list"; items: ResumeAccount[] } | { kind: "none"; agentName: string } | { kind: "off" };

export interface ResumeMenuSpec {
  accounts: ResumeAccounts;
  /** 勾着的那一组（点单选时就地改它）。 */
  pick: ResumePick;
  onChange?: (p: ResumePick) => void;
  /** 有它 ⇒ 最上面一行「恢复」，按勾着的那一组起（标签页右键：没有主按钮）。 */
  run?: (p: ResumePick) => void;
  /** 有它 ⇒ 最底下「在此目录新建会话」。 */
  newInDir?: () => void;
}

/** 默认那一组：上次的号（跟随）· 运行于照通用页那一格；那台说了没有 tmux ⇒ 不用 tmux（没问到 ⇒ 照那一格）。 */
export function defaultPick(origin?: Origin): ResumePick {
  return { tmux: resumeInTmuxFor(origin), account: undefined, useBase: false };
}

/** 勾着的号叫什么；跟随 ⇒ 上次的号（知道的话）；这一家没有账号这一维 ⇒ 那一家的名字。 */
function pickedAccountLabel(accounts: ResumeAccounts, p: ResumePick): string | null {
  if (p.useBase) return copyText("resumeMenu.account.base");
  if (p.account !== undefined) return p.account;
  if (accounts.kind === "none") return accounts.agentName;
  if (accounts.kind !== "list") return null;
  return accounts.items.find((a) => a.last && a.name !== null)?.name ?? null;
}

/** 勾着的那一组怎么说：`personal · tmux 里` / 没有号可说时 `tmux 里`。 */
export function pickWords(accounts: ResumeAccounts, p: ResumePick): string {
  const run = p.tmux ? copyText("resumeMenu.hint.tmux") : copyText("resumeMenu.run.direct");
  const account = pickedAccountLabel(accounts, p);
  return account === null ? run : copyText("resumeMenu.pick.account", { account, run });
}

/** 主按钮的悬停：`恢复 · personal · tmux 里`。 */
export function resumeHint(accounts: ResumeAccounts, p: ResumePick): string {
  return copyText("resumeMenu.hint.pick", { pick: pickWords(accounts, p) });
}

/**
 * 那台的号 ⇒ 账号组（`agentHasAccounts` 是那一行的事实：这一家有没有账号这一维）。
 * 可选的号一个也没有 / 那台没开多账号 ⇒ `off`；`5h N%` 读额度账（那台还没读过 ⇒ 先读一次）。
 */
export async function resumeAccounts(
  origin: Origin,
  agent: string,
  facts: { hasAccounts: boolean; agentName: string; last?: string },
): Promise<ResumeAccounts> {
  if (!facts.hasAccounts) return { kind: "none", agentName: facts.agentName };
  let names: string[];
  try {
    const state = await fetchAccounts(origin);
    if (!state.available) return { kind: "off" };
    names = selectableAccounts(state).map((a) => a.name);
  } catch {
    return { kind: "off" };
  }
  if (names.length === 0) return { kind: "off" };
  if (!appStore.quota.get().has(origin)) await refreshQuota(origin);
  const q = appStore.quota.get().get(origin);
  // 额度账按路由第 1 段（那一家的适配器 id）记号，会话行上是那一家的名字 ⇒ 经那一家的事实表换过去。
  const prof = lookupAgentProfile(agent);
  const five = (account: string): string | null => (prof.known ? fiveHourCell(q, prof.facts.adapterId, account) : null);
  const items: ResumeAccount[] = names.map((name) => ({ name, label: name, quota: five(name), last: name === facts.last }));
  items.push({ name: null, label: copyText("resumeMenu.account.base"), quota: five("_"), last: false });
  return { kind: "list", items };
}

/** 号那一行右侧那一格：`5h 41%` · 上次用的 `5h 41% · 上次`（右对齐，几行的数对得齐）。 */
function accountDetail(a: ResumeAccount): string | undefined {
  if (a.last) return a.quota === null ? copyText("resumeMenu.account.last") : copyText("resumeMenu.account.quotaLast", { quota: a.quota });
  return a.quota ?? undefined;
}

/** 「恢复」那一组的菜单项。 */
export function resumeMenuItems(spec: ResumeMenuSpec): MenuItem[] {
  const { accounts, pick } = spec;
  const changed = (): void => spec.onChange?.(pick);
  const items: MenuItem[] = [];
  if (spec.run) {
    const run = spec.run;
    items.push(
      { label: copyText("history.row.resume"), detailOf: () => pickWords(accounts, pick), onClick: () => run(pick) },
      { label: "", divider: true },
    );
  }
  if (accounts.kind !== "off") {
    items.push({ label: copyText("resumeMenu.group.account"), heading: true });
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
          avatar: a.name === null ? undefined : accountAvatarEl(a.name, { size: 16 }),
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
      label: copyText("resumeMenu.run.tmux"),
      radio: "run",
      checked: pick.tmux,
      onClick: () => {
        pick.tmux = true;
        changed();
      },
    },
    {
      label: copyText("resumeMenu.run.direct"),
      radio: "run",
      checked: !pick.tmux,
      onClick: () => {
        pick.tmux = false;
        changed();
      },
    },
  );
  if (spec.newInDir) {
    items.push({ label: "", divider: true }, { label: copyText("history.menu.newInDir"), icon: NEW_IN_DIR_ICON, onClick: spec.newInDir });
  }
  return items;
}
