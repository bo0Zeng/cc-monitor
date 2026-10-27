/**
 * **起一组会话的入口**：栏顶拆分按钮「＋ ⌄」与它那张组菜单 · 命令面板「起一组…」开的同一张菜单。
 *
 * - 左半「＋」＝ 新建会话…（开全产品那一个框）；右半「⌄」＝ 这台存好的几组（`profiles-read` 的 `sets`）。窄栏只剩「＋」，点了开同一张菜单。
 * - 每组一项：名字 · 右侧「再起 N / 已齐」（各问一次 `set-plan`，答回来之前转圈）· 第二行按行写「目录 组名 已有/要的」（起不了的那行写原因）。
 *   一组写错 · 那台答不了 ⇒ 那一项灰着、第二行写为什么。点一项 ＝ `set-launch`：不弹框，起成的交占位标签页等报到，起完一句。
 * - 判定（已有几个 · 再起几个 · 组名 · 起不了的原因）全在那台后端，这里只排版。
 */
import { copyText } from "./copy-table";
import { icon } from "./kit/icon";
import { openMenu, updateMenuItem, type MenuItem } from "./kit/menu";
import { attachTooltip } from "./kit/tooltip";
import { toast } from "./kit/toast";
import { detailOf, saidOf } from "./kit/detail";
import { readProfiles } from "./profiles-reads";
import { launchSet, planSet, type SetLaunched, type SetPlan } from "./set-reads";
import { awaitArrival } from "./session-open";
import type { Origin } from "./ipc/origin";
import type { SetOut } from "./generated/SetOut";
import type { Command } from "./views/command-bar";
import s from "./set-menu.module.css";

/** 菜单里一组那一项的样子：还在问 · 那台答了 · 答不了（那一句）。 */
export type PlanState = { kind: "pending" } | { kind: "plan"; plan: SetPlan } | { kind: "failed"; said: string };

/** 菜单要的两只手（宿主交）：开新建框的单个页 · 开一组页（`null` ＝ 新建一组）。 */
export interface SetMenuHost {
  newSession(): void;
  editSets(name: string | null): void;
}

const itemId = (name: string): string => `set:${name}`;

/** 第二行：每行「目录 组名 已有/要的」，起不了的那行「目录 原因」。 */
export function rowsSaid(plan: SetPlan): string {
  return plan.rows
    .map((r) =>
      r.blocked === null
        ? copyText("setMenu.row.counted", { cwd: r.cwd, group: r.group, have: String(r.have), want: String(r.want) })
        : copyText("setMenu.row.blocked", { cwd: r.cwd, why: r.blocked }),
    )
    .join(copyText("setMenu.row.sep"));
}

/** 一组那一项（纯函数）：写错的 · 还在问的 · 答不了的 · 已齐的灰着；能起的右侧「再起 N」、点了起。 */
export function setItem(set: SetOut, state: PlanState, start: () => void): MenuItem {
  const base: MenuItem = { id: itemId(set.name), label: set.name };
  const broken = set.problems[0];
  if (broken !== undefined) return { ...base, enabled: false, why: broken.message };
  if (state.kind === "pending") return { ...base, pending: true };
  if (state.kind === "failed") return { ...base, enabled: false, why: state.said };
  const { plan } = state;
  if (plan.more === 0) return { ...base, detail: copyText("setMenu.item.full"), enabled: false, why: rowsSaid(plan) };
  return { ...base, detail: copyText("setMenu.item.more", { n: String(plan.more) }), body: rowsSaid(plan), onClick: start };
}

/** 起完那一句：全成 ⇒「已启动 名 · n」；有没成的 ⇒ 出错那一句、逐条原因、［重试］（再点一次 ＝ 只起还缺的）。 */
export function launchedSaid(name: string, out: SetLaunched): { title: string; body: string; ok: boolean } {
  if (out.failed === 0) return { title: copyText("setMenu.launched.done", { name, n: String(out.started) }), body: "", ok: true };
  const lines = out.rows.flatMap((r) => r.failed.map((f) => copyText("setMenu.launched.fail", { alias: r.alias, why: f.message })));
  return {
    title: copyText("setMenu.launched.partial", { name, started: String(out.started), all: String(out.started + out.failed) }),
    body: lines.join("\n"),
    ok: false,
  };
}

/** 在飞的那几组：连点不重发（补足本来幂等，省一趟）。 */
const inFlight = new Set<string>();

/** 照存好的那一组补足、立刻起：起成的交占位标签页等报到；起完说一句。 */
export async function startSet(origin: Origin, name: string): Promise<void> {
  const key = `${origin}\u0000${name}`;
  if (inFlight.has(key)) return;
  inFlight.add(key);
  let out: SetLaunched;
  try {
    out = await launchSet(origin, name);
  } catch (e) {
    toast(saidOf(e), "", { detail: detailOf(e) });
    return;
  } finally {
    inFlight.delete(key);
  }
  for (const r of out.rows) {
    for (const o of r.opened) {
      if (o.arrival !== null) awaitArrival(origin, o.arrival, o.cwd, copyText("sessionOpen.arrived.title"));
    }
  }
  const said = launchedSaid(name, out);
  if (said.ok) toast(said.title, "", { level: "info" });
  else toast(said.title, said.body, { action: { label: copyText("setMenu.launched.retry"), run: () => void startSet(origin, name) } });
}

/** 开组菜单：先列「新建会话…」与这台存好的几组（各自转圈），每组的 `set-plan` 答回来就地换成那一项。 */
export async function openSetMenu(anchor: HTMLElement, origin: Origin, host: SetMenuHost): Promise<void> {
  const head: MenuItem[] = [{ label: copyText("main.cmd.newSession"), icon: "plus", onClick: () => host.newSession() }];
  let sets: SetOut[];
  try {
    sets = (await readProfiles(origin)).sets;
  } catch (e) {
    openMenu({ el: anchor, align: "start" }, [...head, { divider: true, label: "" }, { label: copyText("setMenu.list.heading"), enabled: false, why: saidOf(e) }], { label: copyText("setMenu.open.label") });
    return;
  }
  const tail: MenuItem[] = [
    { divider: true, label: "" },
    sets.length === 0
      ? { label: copyText("setMenu.list.create"), onClick: () => host.editSets(null) }
      : { label: copyText("setMenu.list.edit"), onClick: () => host.editSets(null) },
  ];
  const start = (name: string) => () => void startSet(origin, name);
  const items: MenuItem[] = [
    ...head,
    ...(sets.length === 0 ? [] : [{ divider: true, label: "" }, { heading: true, label: copyText("setMenu.list.heading") }]),
    ...sets.map((x) => setItem(x, { kind: "pending" }, start(x.name))),
    ...tail,
  ];
  if (!openMenu({ el: anchor, align: "start" }, items, { label: copyText("setMenu.open.label"), width: 320 })) return;
  for (const x of sets.filter((x) => x.problems.length === 0)) {
    void planSet(origin, { name: x.name }).then(
      (plan) => updateMenuItem(itemId(x.name), setItem(x, { kind: "plan", plan }, start(x.name))),
      (e: unknown) => updateMenuItem(itemId(x.name), setItem(x, { kind: "failed", said: saidOf(e) }, start(x.name))),
    );
  }
}

/** 窄栏（栏收成一列图标）：拆分按钮只剩「＋」，点了开同一张菜单。 */
const narrow = (): boolean => document.body.dataset.tabBar === "folded";

/** 栏顶拆分按钮「＋ ⌄」。`origin` 现取（起在哪台跟着宿主此刻的那台）。 */
export function newSessionSplit(origin: () => Origin, host: SetMenuHost): HTMLElement {
  const root = document.createElement("span");
  root.className = s.split;
  root.dataset.role = "new-split";
  const plus = document.createElement("button");
  plus.type = "button";
  plus.className = `tab-bar-icon ${s.plus}`;
  plus.dataset.role = "new-session";
  plus.setAttribute("aria-label", copyText("main.cmd.newSession"));
  plus.appendChild(icon("plus"));
  attachTooltip(plus, () => copyText("main.cmd.newSession"));
  const caret = document.createElement("button");
  caret.type = "button";
  caret.className = `tab-bar-icon ${s.caret}`;
  caret.dataset.role = "set-menu";
  caret.setAttribute("aria-label", copyText("setMenu.open.label"));
  caret.appendChild(icon("caretDown"));
  attachTooltip(caret, () => copyText("setMenu.open.label"));
  plus.addEventListener("click", () => {
    if (narrow()) void openSetMenu(plus, origin(), host);
    else host.newSession();
  });
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
