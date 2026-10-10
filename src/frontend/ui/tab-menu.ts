/**
 * **右键一个标签页，菜单里放哪几项**（会话头「更多」是同一份）—— 以及那几格要异步就绪的项怎么就绪。
 *
 * 分组从上到下：看它（在新窗口打开 · 打开工作目录 · 切到终端 · 看它的终端 · 看一眼终端画面）· 固定 / 分组 ·
 * 恢复这个会话 ▸ · 在终端里打开 · 账号… · 关闭标签页 · 结束会话…（红字，最后一组，在 tmux 里的才有）。
 * 灰着的项第二行写为什么；还在问那台 tmux 的那几项先画出来、右侧转圈，答回来才变可点（`sessions-where`，界面不判）。
 * 状态不明（那台看不见）的标签页：恢复 · 在终端里打开 · 结束 · 账号… 一律灰（同一句），只有关闭标签页可点。
 * 点下去真正做事的住 `tab-session-actions.ts`。
 */
import { toast } from "./kit/toast";
import type { Tab } from "./tab-model";
import { canResume, hasTerminal, isResumeOnly, type SessionState } from "./tab-session-state";
import { copyText } from "./copy-table";
import type { TabCollection } from "./tab-collections";
import { defaultPick, resumeAccounts, resumeMenuItems, type ResumeAccounts, type ResumePick } from "./resume-menu";
import { openNewSession } from "./new-session";
import { askOf } from "./launch-account";
import { runRemoteAttach } from "./remote-launch-run";
// 标签页里的会话都是流跟的那一家（记录树那一家）。
import { agentHasAccounts, displayNameOf } from "./agent-profile";
// 本机 = `LOCAL_ORIGIN`（`"<local>"`）；「是不是本机」只经 `ipc/origin.ts` 判。
import { isLocalOrigin, isRemoteOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { standingOf } from "./sessions-where";
import {
  menuGeneration,
  removeMenuItem,
  openMenu,
  updateMenuItem,
  type MenuItem,
} from "./kit/menu";
import type { TabSessionActions } from "./tab-session-actions";
import { machineName, unavailableSaid } from "./control-said";
import { dispatcher, KeybindingDispatcher } from "./keybindings/registry";
import type { ActionId } from "./keybindings/actions";
import { terminalFrontAvailable } from "./terminal-front";
import { fullTitle } from "./session-face";

/** 菜单项 id ⇒ 它要那台后端做的那条命令（那台握手时说过做不到 ⇒ 置灰并说为什么）。 */
const ITEM_OPS: Readonly<Record<string, string>> = { kill: "kill", "resume-into": "launch" };

/** 按 `origin` 那台的能力事实给一项置灰：做不到 ⇒ 不可点、第二行写为什么。 */
export function gateByOffer(origin: Origin, item: MenuItem): MenuItem {
  const op = item.id !== undefined ? ITEM_OPS[item.id] : undefined;
  if (op === undefined || item.enabled === false || item.pending) return item;
  const why = unavailableSaid(origin, op);
  if (why === null) return item;
  return { ...item, enabled: false, why, onClick: () => {} };
}

/** 动作此刻的键位（菜单项右侧现拼）。 */
function keyOf(id: ActionId): string | undefined {
  const c = dispatcher.effectiveChord(id);
  return c ? KeybindingDispatcher.prettyChord(c) : undefined;
}

/** 去掉打头、收尾与连着的分隔线（某一组整组没有项时）。 */
export function tidyDividers(items: MenuItem[]): MenuItem[] {
  const out: MenuItem[] = [];
  for (const it of items) {
    if (it.divider && (out.length === 0 || out[out.length - 1].divider)) continue;
    out.push(it);
  }
  while (out.length > 0 && out[out.length - 1].divider) out.pop();
  return out;
}

/**
 * 「分组 ▸」那一层（单个右键与多选右键同一份）：新建分组 · 各组（所在的打勾，点别的 ＝ 挪过去）· 移出分组（不在组里灰着）。
 * 多选（`many`）⇒ 每项带能做的个数（今天批量菜单的写法）。新建不问名字：建「分组 N」、组头名字框立刻打开（同拖着建组）。
 */
export function groupSubmenu(o: {
  collections: readonly TabCollection[];
  /** 这一批里能挪进组 `gid` 的个数（不在它里面的）。 */
  movable(gid: string): number;
  /** 这一批里在组里的个数（能移出的）。 */
  grouped: number;
  /** 多选：这一批几个（给了 ⇒ 每项带个数）。 */
  many?: number;
  found(): void;
  join(gid: string): void;
  leave(): void;
  /** 多选时灰着的那几项为什么（单个的不写）。 */
  why?: { inThatGroup: string; notInGroup: string };
}): MenuItem[] {
  const n = o.many;
  const items: MenuItem[] = [
    { label: n === undefined ? copyText("tabMenu.collection.new") : copyText("tabBatch.menu.found", { n }), onClick: o.found },
  ];
  if (o.collections.length > 0) items.push({ label: "", divider: true });
  for (const col of o.collections) {
    const k = o.movable(col.id);
    items.push({
      label: n === undefined ? col.name : copyText("tabBatch.menu.joinOne", { name: col.name, n: k }),
      checked: k === 0,
      enabled: n === undefined || k > 0,
      why: n !== undefined && k === 0 ? o.why?.inThatGroup : undefined,
      onClick: () => {
        if (k > 0) o.join(col.id);
      },
    });
  }
  items.push({ label: "", divider: true });
  items.push({
    label: n === undefined ? copyText("tabMenu.collection.remove") : copyText("tabBatch.menu.leave", { n: o.grouped }),
    enabled: o.grouped > 0,
    why: n !== undefined && o.grouped === 0 ? o.why?.notInGroup : undefined,
    onClick: o.leave,
  });
  return items;
}

/** 菜单要宿主给的读数 / 回调。全是**现读**：菜单项的 `onClick` 在点下去那一刻才调它们。 */
export interface TabMenuHost {
  tab(sid: string): Tab | undefined;
  isAttachable(sid: string): boolean;
  /** 这个实例拉过集合没有（撕离出来的 viewer 窗口从不拉 ⇒ 不给集合入口）。 */
  collectionsLoaded(): boolean;
  collections(): TabCollection[];
  /** 这个 tab 在哪个组（读 `Tab.group`）。下面三个动作改完就重画、给可撤的提示（与拖放同一口）。 */
  groupOf(sid: string): TabCollection | null;
  joinGroup(sids: readonly string[], gid: string): void;
  /** 建「分组 N」把这几个放进去、名字框立刻打开；组数到上界 ⇒ 说一句、什么都不做。 */
  foundGroup(sids: readonly string[]): void;
  leaveGroup(sids: readonly string[]): void;
  /** 同上一条理由：没 `loadPinned` 过就不给固定入口。 */
  pinnedLoaded(): boolean;
  togglePin(sid: string): void;
  /** 关掉这个标签（与批量菜单「关闭标签」、× 同一个动作）。 */
  close(sid: string): void;
  /** 打开工作目录 · 切到终端（Windows）· 看它的终端（底部抽屉终端页）。 */
  openCwd(sid: string): void;
  front(sid: string): void;
  viewTerminal(sid: string): void;
  /** 「账号…」：开这个会话的「账号」面板（与状态栏账号按钮同一个）。 */
  openAccountPanel?: (sid: string) => void;
  /** 面板接上了没有（没接 ⇒ 不给「账号…」）。 */
  accountPanelWired?: () => boolean;
}

export class TabMenu {
  constructor(
    private readonly host: TabMenuHost,
    private readonly actions: TabSessionActions,
  ) {}

  /** 右键一个标签页（开在指针底下）或锚在一颗按钮上（行尾 / 会话头「更多」）⇒ 组这一代菜单、开、再把要问那台的几格发出去。 */
  open(e: MouseEvent | HTMLElement, sid: string, extra: readonly MenuItem[] = []): void {
    const t = this.host.tab(sid);
    const unseen = t?.state.liveness === "unseen";
    const unseenWhy = t && unseen ? copyText("tabMenu.unseen.why", { machine: machineName(t.origin) }) : undefined;
    const grey = (it: MenuItem): MenuItem => (unseenWhy ? { ...it, enabled: false, pending: false, why: unseenWhy, onClick: () => {} } : it);
    const remote = t !== undefined && isRemoteOrigin(t.origin) && t.projectDir ? t.origin : null;
    const local = t !== undefined && isLocalOrigin(t.origin) && hasTerminal(t.state);
    const items: MenuItem[] = [];

    // ① 看它
    items.push({ icon: "popOut", label: copyText("tabMenu.open.openInWindow"), detail: keyOf("tab.pop-out"), onClick: () => void this.actions.openInNewWindow(sid) });
    if (t?.projectDir) items.push({ icon: "folder", label: copyText("tabMenu.open.cwd"), detail: keyOf("tab.open-cwd"), onClick: () => this.host.openCwd(sid) });
    if (t && hasTerminal(t.state) && terminalFrontAvailable()) {
      items.push({ icon: "front", label: copyText("tabMenu.open.front"), detail: keyOf("terminal.bring-front"), onClick: () => this.host.front(sid) });
    }
    if (t) items.push({ icon: "terminal", label: copyText("tabMenu.open.drawer"), detail: keyOf("panel.toggle-terminal"), onClick: () => this.host.viewTerminal(sid) });

    // ② 固定 · 分组
    items.push({ label: "", divider: true });
    if (t && this.host.pinnedLoaded()) {
      items.push({
        icon: "pin",
        label: t.pinned ? copyText("tabMenu.pin.unpin") : copyText("tabMenu.pin.pin"),
        title: t.pinned ? copyText("sessionState.pin.unpinHint") : copyText("sessionState.pin.pinHint"),
        onClick: () => this.host.togglePin(sid),
      });
    }
    if (this.host.collectionsLoaded()) items.push({ icon: "list", label: copyText("tabMenu.collection.add"), submenu: this.groupItems([sid]) });

    // ③ 恢复 · 在终端里打开 · 账号
    items.push({ label: "", divider: true });
    if (t && unseen) items.push(grey({ icon: "history", label: copyText("tabMenu.item.resume") }));
    else if (t && canResume(t.state)) {
      // 恢复 ▸：与历史页「恢复 ▾」同一个组件（`resume-menu.ts`）；账号那一组等那台的号到了再换上（`appendAccountMenuItems`）。
      items.push({ id: "resume", icon: "history", label: copyText("tabMenu.item.resume"), submenu: this.buildResumeSubmenu(sid, { kind: "off" }) });
    }
    if (remote !== null) items.push(grey({ id: "attach", icon: "terminal", label: copyText("tabMenu.attach.label"), pending: true }));
    if (local) items.push({ id: "resume-into", icon: "history", label: copyText("tabMenu.inPlace.label"), pending: true });
    const openPanel = this.host.accountPanelWired?.() ? this.host.openAccountPanel : undefined;
    if (t && openPanel) items.push(grey({ id: "account", icon: "account", label: copyText("acct.menu.open"), onClick: () => openPanel(sid) }));

    // ④ 关闭标签页：已结束 · 记录已不在 · 状态不明的才关得掉；在跑的灰着（先结束）。
    items.push({ label: "", divider: true });
    if (t) {
      const closable = isResumeOnly(t.state);
      items.push({
        icon: "close",
        label: copyText("tabMenu.item.close"),
        enabled: closable,
        why: !closable ? copyText("tabMenu.close.live") : unseen ? copyText("tabMenu.close.unseen", { machine: machineName(t.origin) }) : undefined,
        onClick: () => this.host.close(sid),
      });
    }

    // ⑤ 结束会话…（在 tmux 里的才有；先画出来、问到了再变可点）
    if (remote !== null || local) {
      items.push({ label: "", divider: true });
      items.push(grey({ id: "kill", icon: "failed", label: copyText("tabMenu.kill.label"), danger: true, pending: true }));
    }
    // 会话头「⋯」多出来的那几个开关（流的开关，不归那台机器能不能做）。
    if (extra.length > 0) items.push({ label: "", divider: true }, ...extra);
    // 那台握手时说过做不到的那几项置灰（事实住 monitor 那份 `Offer`）。
    const shown = tidyDividers(t ? items.map((i) => gateByOffer(t.origin, i)) : items);
    openMenu(e instanceof HTMLElement ? { el: e, align: "end" } : { x: e.clientX, y: e.clientY }, shown);
    if (unseen) return;
    if (remote !== null && t) void this.resolveRemoteTmuxItems(remote, sid, fullTitle(t));
    if (local && t) void this.resolveLocalTmuxItems(sid, fullTitle(t));
    // 已结束的会话：那台的号到了再把「恢复 ▸」的账号组换上。
    if (t) void this.appendAccountMenuItems(t.origin, sid, t.state);
  }

  /** 「分组 ▸」那一层，作用于这几个（命令面板「加入分组…」开的也是它；多选 ⇒ 每项带个数）。 */
  private groupItems(sids: readonly string[]): MenuItem[] {
    const tabs = sids.map((x) => this.host.tab(x)).filter((t): t is Tab => t !== undefined);
    const ids = tabs.map((t) => t.sessionId);
    return groupSubmenu({
      collections: this.host.collections(),
      movable: (gid) => tabs.filter((t) => t.group !== gid).length,
      grouped: tabs.filter((t) => t.group !== null).length,
      many: ids.length > 1 ? ids.length : undefined,
      found: () => this.host.foundGroup(ids),
      join: (gid) => this.host.joinGroup(tabs.filter((t) => t.group !== gid).map((t) => t.sessionId), gid),
      leave: () => this.host.leaveGroup(tabs.filter((t) => t.group !== null).map((t) => t.sessionId)),
      why: { inThatGroup: copyText("tabBatch.why.inThatGroup"), notInGroup: copyText("tabBatch.why.notInGroup") },
    });
  }

  /** 命令面板「加入分组…」：只开「分组 ▸」那一层，锚在一颗按钮 / 一行上。没拉过组表 ⇒ 不开。 */
  openGroupMenu(anchor: HTMLElement, sids: readonly string[]): void {
    if (!this.host.collectionsLoaded() || sids.length === 0) return;
    openMenu({ el: anchor, align: "end" }, this.groupItems(sids), { label: copyText("tabMenu.collection.add") });
  }

  /**
   * 会话头 / 「需手动」钉条的［恢复 ▾］：只开恢复那几项（与右键「恢复 ▸」同一份：恢复 · 账号 · 运行于 · 在此目录新建会话；号到了再开）。
   */
  openResumeMenu(anchor: HTMLElement, sid: string): void {
    const t = this.host.tab(sid);
    if (!t || !canResume(t.state)) return;
    void this.resumeAccountsOf(t.origin, t.agent).then((accounts) => {
      if (anchor.isConnected) openMenu({ el: anchor, align: "end" }, this.buildResumeSubmenu(sid, accounts).map((i) => gateByOffer(t.origin, i)));
    });
  }



  /**
   * 会话头 / 钉条的［在终端里打开］（远端、在 tmux 里）：问那台这个会话在哪个 tmux 会话里，接上第一个（命中多个照右键那条说出来）；
   * 那台说没有 ⇒ 说一句，不开窗。
   */
  async attachRemote(sid: string): Promise<void> {
    const t = this.host.tab(sid);
    if (!t || !isRemoteOrigin(t.origin)) return;
    const s = await standingOf(t.origin, sid);
    if (s === undefined || s.kind === "none" || s.kind === "no_tmux" || s.names.length === 0) {
      toast(copyText("sessionHead.attach.none", { machine: t.origin }), "", { level: "warn" });
      return;
    }
    if (s.kind === "ambiguous") toast(copyText("tabMenu.dupes.title"), copyText("tabMenu.dupes.body", { n: s.names.length, name: s.names[0] }), { level: "info" });
    if (t.agent === null) {
      toast(copyText("tabSessionActions.agent.unknown"), "", { level: "info" });
      return;
    }
    void runRemoteAttach(t.origin, t.agent, s.names[0]);
  }

  /** 远端：那台答回这个会话在哪个 tmux 会话里 ⇒ 在终端里打开 · 看一眼画面 · 结束三格就位（菜单已换 / 已关 ⇒ 不动）。 */
  private async resolveRemoteTmuxItems(origin: string, sid: string, title: string): Promise<void> {
    const gen = menuGeneration();
    const s = await standingOf(origin, sid);
    if (gen !== menuGeneration()) return;
    const drop = (...ids: string[]): void => ids.forEach((id) => removeMenuItem(id));
    if (s === undefined) return this.unresolved(["attach", "kill"]);
    if (s.kind === "none" || s.kind === "no_tmux") return drop("attach", "kill");
    const name = s.names[0];
    if (s.kind === "idle" && !this.host.isAttachable(sid)) return drop("attach", "kill");
    const ambiguous = s.kind === "ambiguous";
    const agent = this.host.tab(sid)?.agent ?? null;
    updateMenuItem("attach", {
      id: "attach",
      icon: "terminal",
      label: copyText("tabMenu.attach.label"),
      // 还不知道是哪一家（会话事实没到）⇒ 灰着，不落哪一家；命中多个 ⇒ 接第一个（第二行已经说了）。
      why: agent === null ? copyText("tabSessionActions.agent.unknown") : ambiguous ? copyText("tabMenu.attach.dupesWhy", { n: s.names.length, name }) : undefined,
      ...(agent === null ? { enabled: false } : { onClick: () => void runRemoteAttach(origin, agent, name) }),
    });
    // 命中多个 ⇒ 不给结束（破坏性，选错了不可逆）。
    updateMenuItem("kill", gateByOffer(origin, ambiguous
      ? { id: "kill", icon: "failed", label: copyText("tabMenu.kill.label"), danger: true, enabled: false, why: copyText("tabMenu.kill.dupesWhy", { n: s.names.length }) }
      : { id: "kill", icon: "failed", label: copyText("tabMenu.kill.label"), danger: true, onClick: () => this.actions.killInTmux(origin, sid, name, { idle: s.kind === "idle", title }) }));
  }

  /** 本机：那台答回这个会话在哪个 tmux 会话里 ⇒ 结束 · 就地恢复两格就位（菜单已换 / 已关 ⇒ 不动）。 */
  private async resolveLocalTmuxItems(sid: string, title: string): Promise<void> {
    const gen = menuGeneration();
    const s = await standingOf(LOCAL_ORIGIN, sid);
    if (gen !== menuGeneration()) return;
    if (s === undefined) return this.unresolved(["kill", "resume-into"]);
    // 不在 tmux 里 ⇒ 不留一个假装能用的破坏性项。
    if (s.kind === "none" || s.kind === "no_tmux" || (s.kind === "idle" && !this.host.isAttachable(sid))) {
      removeMenuItem("kill");
      removeMenuItem("resume-into");
      return;
    }
    const name = s.names[0];
    if (s.kind === "idle") {
      // Claude 已退出、tmux 会话在：就地恢复（非破坏性）与结束并列。
      updateMenuItem("resume-into", gateByOffer(LOCAL_ORIGIN, { id: "resume-into", icon: "history", label: copyText("tabMenu.inPlace.label"), onClick: () => void this.actions.resumeLocalInTmux(sid) }));
    } else removeMenuItem("resume-into");
    updateMenuItem("kill", gateByOffer(LOCAL_ORIGIN, s.kind === "ambiguous"
      ? { id: "kill", icon: "failed", label: copyText("tabMenu.kill.label"), enabled: false, danger: true, why: copyText("tabMenu.kill.dupesWhy", { n: s.names.length }) }
      : { id: "kill", icon: "failed", label: copyText("tabMenu.kill.label"), danger: true, onClick: () => this.actions.killInTmux(LOCAL_ORIGIN, sid, name, { idle: s.kind === "idle", title }) }));
  }

  /** 问不到那台（通道不在 / 超时）：那几格停在灰着、第二行写原因（不悄悄摘掉）。 */
  private unresolved(ids: Array<"attach" | "kill" | "resume-into">): void {
    const why = copyText("tabMenu.probe.failed");
    for (const id of ids) {
      const item: MenuItem =
        id === "attach"
          ? { id, icon: "terminal", label: copyText("tabMenu.attach.label") }
          : id === "kill"
            ? { id, icon: "failed", label: copyText("tabMenu.kill.label"), danger: true }
            : { id, icon: "history", label: copyText("tabMenu.inPlace.label") };
      updateMenuItem(id, { ...item, enabled: false, why });
    }
  }

  /** 每个标签页「恢复 ▸」里勾着的那一组（本窗开着时记）。 */
  private resumePicks = new Map<string, ResumePick>();

  /** 那台的号 ⇒ 恢复菜单的账号组（按这个会话的那一家；还不知道是哪一家 ⇒ 不出账号组，不落哪一家）。 */
  private resumeAccountsOf(origin: Origin, agent: string | null): Promise<ResumeAccounts> {
    if (agent === null) return Promise.resolve({ kind: "off" });
    return resumeAccounts(origin, agent, { hasAccounts: agentHasAccounts(agent), agentName: displayNameOf(agent) ?? agent });
  }

  /**
   * 「恢复 ▸」那一层：最上面一行「恢复」按勾着的那一组起（标签页没有主按钮）；账号 · 运行于两组单选；在此目录新建会话。
   * 起法还是标签页这边那几条：tmux ⇒ 那台在 tmux 里起再接进去（本机同一条）；不用 tmux ⇒ 直接恢复。
   */
  private buildResumeSubmenu(sid: string, accounts: ResumeAccounts): MenuItem[] {
    let pick = this.resumePicks.get(sid);
    const t = this.host.tab(sid);
    if (!pick) this.resumePicks.set(sid, (pick = defaultPick(t?.origin)));
    const dir = t?.projectDir ?? "";
    return resumeMenuItems({
      accounts,
      pick,
      run: (p) => void this.runResume(sid, p),
      newInDir: t && dir ? () => void openNewSession({ origin: t.origin, cwd: dir }) : undefined,
    });
  }

  private runResume(sid: string, p: ResumePick): Promise<void> {
    const t = this.host.tab(sid);
    if (!t) return Promise.resolve();
    if (!p.tmux) return this.actions.resumeTab(sid, p.account, p.useBase);
    return isLocalOrigin(t.origin) ? this.actions.resumeLocalInTmux(sid, askOf(p.account, p.useBase)) : this.actions.resumeTabTmux(sid, p.account, p.useBase);
  }

  /** 已结束的会话：菜单开后那台的号到了，把「恢复 ▸」的账号组换上（菜单已换 / 已关 ⇒ 不动）。 */
  private async appendAccountMenuItems(
    origin: string,
    sid: string,
    state: SessionState,
  ): Promise<void> {
    if (state.liveness === "unseen" || !isResumeOnly(state)) return;
    const gen = menuGeneration();
    const accounts = await this.resumeAccountsOf(origin, this.host.tab(sid)?.agent ?? null);
    if (gen !== menuGeneration() || accounts.kind === "off") return;
    updateMenuItem("resume", { id: "resume", icon: "history", label: copyText("tabMenu.item.resume"), submenu: this.buildResumeSubmenu(sid, accounts) });
  }
}
