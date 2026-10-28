/**
 * **起一组会话的入口**：栏顶拆分按钮「＋ ⌄」与它那张组菜单 · 命令面板「起一组…」开的同一张菜单。
 *
 * - 左半「＋」＝ 新建会话…（开全产品那一个框）；右半「⌄」＝ 这台存好的几组（`profiles-read` 的 `sets`）。窄栏只剩「＋」，点了开同一张菜单。
 * - 每组一项：名字 · 右侧「再起 N / 已齐」· 第二行那台的短句（几个目录 · 几个会话 · 几行起不了）· 悬停侧卡逐行「目录 组名 已有/要的」（起不了的那行写原因）—— 这几句都是那台 `set-plan` 写好的，
 *   答回来之前转圈。一组写错 · 那台答不了 ⇒ 那一项灰着、第二行写为什么。点一项 ＝ 起那一组（`set-start.ts`）：不弹框。
 * - 判定（已有几个 · 再起几个 · 组名 · 起不了的原因）与那几句全在那台后端，这里只排版。
 */
import { copyText } from "./copy-table";
import { icon } from "./kit/icon";
import { openMenu, updateMenuItem, type MenuItem } from "./kit/menu";
import { attachTooltip } from "./kit/tooltip";
import { saidOf } from "./kit/detail";
import { readProfiles } from "./profiles-reads";
import { planSet, type SetPlan } from "./set-reads";
import { startSet } from "./set-start";
import type { Origin } from "./ipc/origin";
import type { SetOut } from "./generated/SetOut";
import type { Command } from "./views/command-bar";

/** 菜单里一组那一项的样子：还在问 · 那台答了 · 答不了（那一句）。 */
export type PlanState = { kind: "pending" } | { kind: "plan"; plan: SetPlan } | { kind: "failed"; said: string };

/** 菜单要的两只手（宿主交）：开新建框的单个页 · 开一组页（`null` ＝ 新建一组）。 */
export interface SetMenuHost {
  newSession(): void;
  editSets(name: string | null): void;
}

const itemId = (name: string): string => `set:${name}`;

/** 一组那一项（纯函数）：写错的 · 还在问的 · 答不了的 · 已齐的灰着；能起的右侧「再起 N」、点了起。 */
export function setItem(set: SetOut, state: PlanState, start: () => void): MenuItem {
  const base: MenuItem = { id: itemId(set.name), label: set.name };
  const broken = set.problems[0];
  if (broken !== undefined) return { ...base, enabled: false, why: broken.message };
  if (state.kind === "pending") return { ...base, pending: true };
  if (state.kind === "failed") return { ...base, enabled: false, why: state.said };
  const { plan } = state;
  const peek = (): HTMLElement => rowsCard(plan);
  if (plan.more === 0) return { ...base, detail: plan.moreSaid, enabled: false, why: plan.short, peek };
  return { ...base, detail: plan.moreSaid, body: plan.short, peek, onClick: start };
}

/** 悬停侧卡：逐行一句（那台写好的「目录 组名 已有/要的」· 起不了的写原因）。 */
function rowsCard(plan: SetPlan): HTMLElement {
  const card = document.createElement("div");
  for (const r of plan.rows) {
    const line = document.createElement("div");
    line.dataset.role = "set-peek-row";
    if (r.blocked !== null) line.dataset.intent = "error";
    line.textContent = r.said;
    card.appendChild(line);
  }
  return card;
}

/** 开组菜单：先列「新建会话…」与这台存好的几组（各自转圈），每组的 `set-plan` 答回来就地换成那一项。 */
export async function openSetMenu(anchor: HTMLElement, origin: Origin, host: SetMenuHost): Promise<void> {
  const head: MenuItem[] = [{ label: copyText("main.cmd.newSession"), icon: "plus", onClick: () => host.newSession() }];
  const label = copyText("setMenu.open.label");
  let sets: SetOut[];
  try {
    sets = (await readProfiles(origin)).sets;
  } catch (e) {
    openMenu({ el: anchor, align: "start" }, [...head, { divider: true, label: "" }, { label: copyText("setMenu.list.heading"), enabled: false, why: saidOf(e) }], { label });
    return;
  }
  const start = (name: string) => () => void startSet(origin, name);
  const items: MenuItem[] = [
    ...head,
    ...(sets.length === 0 ? [] : [{ divider: true, label: "" }, { heading: true, label: copyText("setMenu.list.heading") }]),
    ...sets.map((x) => setItem(x, { kind: "pending" }, start(x.name))),
    { divider: true, label: "" },
    { label: sets.length === 0 ? copyText("setMenu.list.create") : copyText("setMenu.list.edit"), onClick: () => host.editSets(null) },
  ];
  if (!openMenu({ el: anchor, align: "start" }, items, { label, width: 320 })) return;
  for (const x of sets.filter((x) => x.problems.length === 0)) {
    void planSet(origin, { name: x.name }).then(
      (plan) => updateMenuItem(itemId(x.name), setItem(x, { kind: "plan", plan }, start(x.name))),
      (e: unknown) => updateMenuItem(itemId(x.name), setItem(x, { kind: "failed", said: saidOf(e) }, start(x.name))),
    );
  }
}

/** 栏顶那排里的一颗图标按钮（同栏顶别的入口：`styles.css` 的 `.tab-bar-icon`）。 */
function headIcon(cls: string, role: string, name: Parameters<typeof icon>[0], label: string): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = `tab-bar-icon ${cls}`;
  b.dataset.role = role;
  b.setAttribute("aria-label", label);
  b.appendChild(icon(name));
  attachTooltip(b, () => label);
  return b;
}

/** 栏顶拆分按钮「＋ ⌄」（两半贴着，样子在 `styles.css` 的 `.tab-bar-split`）。`origin` 现取（起在哪台跟着宿主此刻的那台）。 */
export function newSessionSplit(origin: () => Origin, host: SetMenuHost): HTMLElement {
  const root = document.createElement("span");
  root.className = "tab-bar-split";
  root.dataset.role = "new-split";
  const plus = headIcon("tab-bar-split-plus", "new-session", "plus", copyText("main.cmd.newSession"));
  const caret = headIcon("tab-bar-split-caret", "set-menu", "caretDown", copyText("setMenu.open.label"));
  // 「＋」永远开新建框（窄栏也是）；组菜单只在「⌄」与命令面板「起一组…」。
  plus.addEventListener("click", () => host.newSession());
  caret.addEventListener("click", () => void openSetMenu(caret, origin(), host));
  root.append(plus, caret);
  return root;
}

/** 命令面板「起一组…」：开栏顶那张组菜单（同一份菜单、同一个起法），锚在 `anchor()` 交的那颗按钮上。 */
export function startSetCommand(anchor: () => HTMLElement, origin: () => Origin, host: SetMenuHost): Command {
  return {
    id: "start-set",
    group: "open",
    icon: "list",
    title: copyText("main.cmd.startSet"),
    keywords: copyText("main.cmd.startSetKeywords"),
    run: () => void openSetMenu(anchor(), origin(), host),
  };
}
