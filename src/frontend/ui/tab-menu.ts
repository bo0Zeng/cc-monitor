/**
 * **右键一个标签页，菜单里放哪几项**（会话头「更多」是同一份）—— 以及那几格要异步就绪的项怎么就绪。
 *
 * 分组从上到下：看它（在新窗口打开 · 打开工作目录 · 切到终端 · 看它的终端 · 看一眼终端画面）· 改标题 / 固定 / 分组 / 隐藏
 *（「已隐藏」那一行里的：只有取消隐藏 —— 隐藏的会话不是标签页，不固定、不分组）·
 * 恢复这个会话 ▸ · 在终端里打开 · 账号… · 关闭标签页 · 结束会话…（红字，最后一组，在 tmux 里的才有）。
 * 灰着的项第二行写为什么；还在问那台 tmux 的那几项先画出来、右侧转圈，答回来才变可点（`sessions-where`，界面不判）。
 * 状态不明（那台看不见）的标签页：恢复 · 在终端里打开 · 结束 · 账号… 一律灰（同一句），只有关闭标签页可点。
 * 点下去真正做事的住 `tab-session-actions.ts`。
 */
import type { Tab } from "./tab-model";
import { canAttach, canResume, closesFromMenu, frontWhy, hasTerminal, isLive } from "./face-can";
import type { SessionFace } from "./generated/SessionFace";
import { copyText } from "./copy-table";
import type { TabGroup } from "./tab-bar-prefs";
import { defaultPick, pickAsk, resumeAccounts, resumeMenuItems, type ResumeAccounts, type ResumePick } from "./resume-menu";
import { openNewSession } from "./new-session";
import { openSession } from "./session-open";
// 标签页里的会话都是流跟的那一家（记录树那一家）。
import { agentHasAccounts, displayNameOf } from "./agent-profile";
// 本机 = `LOCAL`（`"<local>"`）；「是不是本机」只经 `ipc/origin.ts` 判。
import { isLocalOrigin, isRemoteOrigin, type Machine } from "./ipc/origin";
import { standingOf, type Standing } from "./sessions-where";
import type { Launch } from "./generated/Launch";
import {
  menuGeneration,
  openMenu,
  updateMenuItem,
  type MenuItem,
} from "./kit/menu";
import type { TabSessionActions } from "./tab-session-actions";
import { machineName, unavailableSaid } from "./control-said";
import { dispatcher, KeybindingDispatcher } from "./keybindings/registry";
import type { ActionId } from "./keybindings/actions";
import { renameTab } from "./tab-rename";

/** 菜单项 id ⇒ 它要那台后端做的那条命令（那台握手时说过做不到 ⇒ 置灰并说为什么）。 */
const ITEM_OPS: Readonly<Record<string, string>> = { end: "sessions-stop" };

/** 按 `origin` 那台的能力事实给一项置灰：做不到 ⇒ 不可点、第二行写为什么。 */
export function gateByOffer(origin: Machine, item: MenuItem): MenuItem {
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
  collections: readonly TabGroup[];
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
  /** 这个实例拉过集合没有（撕离出来的 viewer 窗口从不拉 ⇒ 不给集合入口）。 */
  collectionsLoaded(): boolean;
  collections(): TabGroup[];
  /** 这个 tab 在哪个组（读 `Tab.group`）。下面三个动作改完就重画、给可撤的提示（与拖放同一口）。 */
  groupAt(sid: string): TabGroup | null;
  joinGroup(sids: readonly string[], gid: string): void;
  /** 建「分组 N」把这几个放进去、名字框立刻打开；组数到上界 ⇒ 说一句、什么都不做。 */
  foundGroup(sids: readonly string[]): void;
  leaveGroup(sids: readonly string[]): void;
  /** 同上一条理由：没读到过组的账就不给固定入口。 */
  pinnedLoaded(): boolean;
  togglePin(sid: string): void;
  /** 关掉这个标签（与批量菜单「关闭标签」、× 同一个动作）。 */
  close(sid: string): void;
  /** 「无人答时自动答」那一格（`history-annotate {autoAnswer}`；`0` ＝ 关）：只交命令（那台推帧、会话事实跟着重读），没交成说一句。 */
  autoAnswer(sid: string, min: number): void;
  /** 隐藏 · 取消隐藏（与批量菜单同一口：交命令、给撤销 toast；栏上那一格等推帧来了才挪）。 */
  hide(sids: readonly string[]): void;
  unhide(sids: readonly string[]): void;
  /** 打开工作目录 · 切到终端（Windows）· 看它的终端（底部抽屉终端页）。 */
  openCwd(sid: string): void;
  front(sid: string): void;
  viewTerminal(sid: string): void;
  /** 「账号…」：开这个会话的「账号」面板（与状态栏账号按钮同一个）。 */
  openAccountPanel?: (sid: string) => void;
  /** 面板接上了没有（没接 ⇒ 不给「账号…」）。 */
  accountPanelWired?: () => boolean;
}

/** 「无人答时自动答 ▸」那一组单选：照会话事实 `autoAnswerChoices` 列（几档与字都是核心给的），勾在 `min == autoAnswer`（关 ＝ `0`）那一项上。 */
export function autoAnswerItems(t: Pick<Tab, "autoAnswer" | "autoAnswerChoices">, set: (min: number) => void): MenuItem[] {
  const now = t.autoAnswer ?? 0;
  return t.autoAnswerChoices.map(({ min, text }) => ({
    label: text,
    radio: "autoAnswer",
    checked: min === now,
    onClick: () => {
      if (min !== now) set(min);
    },
  }));
}

export class TabMenu {
  constructor(
    private readonly host: TabMenuHost,
    private readonly actions: TabSessionActions,
  ) {}

  /**
   * 会话头借这份菜单里的一项画成一颗按钮（ui3「会话头按钮」：字、能不能点、点了做什么都是这一项，不另写一份）。
   * 这个会话此刻没有这一项 ⇒ `null`。只给同步就定下来的那几项（在新窗口打开 · 账号 · 固定 · 分组 · 恢复 · 关闭标签页）。
   */
  itemFor(sid: string, id: "popOut" | "account" | "pin" | "group" | "resume" | "close"): MenuItem | null {
    return this.build(sid).items.find((i) => i.id === id && !i.divider) ?? null;
  }

  /** 这个标签页的菜单项（右键与会话头「⋯」同一份；要问那台的几格先画成转圈）。 */
  private build(sid: string): { items: MenuItem[]; t: Tab | undefined; unseen: boolean; attach: boolean } {
    const t = this.host.tab(sid);
    const unseen = t !== undefined && t.face?.dot === "unknown";
    // 为什么灰：脸上那一句（后端照那台离线那一句写好，`SessionFace::unseen`）。
    const unseenWhy = t && unseen ? (t.face?.line ?? "") : undefined;
    const grey = (it: MenuItem): MenuItem => (unseenWhy !== undefined ? { ...it, enabled: false, pending: false, why: unseenWhy, onClick: () => {} } : it);
    const attach = t !== undefined && canAttach(t.face, isRemoteOrigin(t.origin));
    const items: MenuItem[] = [];

    // ① 看它
    items.push({ id: "popOut", icon: "popOut", label: copyText("history.menu.openWindow"), detail: keyOf("tab.pop-out"), onClick: () => void this.actions.openInNewWindow(sid) });
    if (t?.projectDir) items.push({ icon: "folder", label: copyText("tabBarView.tab.cwdHint"), detail: keyOf("tab.open-cwd"), onClick: () => this.host.openCwd(sid) });
    // ↗ 一直在：做不了灰着、右边写那一句（A2-17-12-1：壳说这台 ↗ 不是真的 · 那台说没有终端可去）。
    if (t) {
      const why = frontWhy(t.face);
      const front: MenuItem = { icon: "front", label: copyText("terminal.head.front"), detail: keyOf("terminal.bring-front"), onClick: () => this.host.front(sid) };
      items.push(why === null ? front : { ...front, enabled: false, why, onClick: () => {} });
    }
    if (t) items.push({ icon: "terminal", label: copyText("sessionHead.act.terminal"), detail: keyOf("panel.toggle-terminal"), onClick: () => this.host.viewTerminal(sid) });

    // ② 改标题 · 固定 · 分组 · 隐藏（隐藏的会话只有「取消隐藏」：它不占标签页，不固定、不分组）
    items.push({ label: "", divider: true });
    if (t) items.push({ id: "rename", icon: "text", label: copyText("history.menu.rename"), detail: keyOf("tabBar.rename"), onClick: () => renameTab(t) });
    const hidden = t?.hidden ?? null;
    if (t && hidden !== null) {
      items.push({
        id: "hide",
        icon: "hidden",
        label: copyText("history.menu.unhide"),
        title: hidden.by === "self" ? undefined : copyText("tabBar.unhide.ruleHint", { said: hidden.said }),
        onClick: () => this.host.unhide([sid]),
      });
    }
    if (t && hidden === null && this.host.pinnedLoaded()) {
      items.push({
        id: "pin",
        icon: "pin",
        label: t.pinned ? copyText("tabMenu.pin.unpin") : copyText("tabMenu.pin.pin"),
        title: t.pinned ? copyText("sessionState.pin.unpinHint") : copyText("sessionState.pin.pinHint"),
        onClick: () => this.host.togglePin(sid),
      });
    }
    if (hidden === null && this.host.collectionsLoaded()) items.push({ id: "group", icon: "list", label: copyText("tabMenu.collection.add"), submenu: this.groupItems([sid]) });
    if (t && hidden === null) items.push({ id: "hide", icon: "hidden", label: copyText("history.menu.hide"), title: copyText("tabBar.hidden.tip"), onClick: () => this.host.hide([sid]) });
    if (t && (isLive(t.face) || hasTerminal(t.face))) items.push(grey({ id: "autoAnswer", icon: "question", label: copyText("tabMenu.autoAnswer.label"), title: copyText("tabMenu.autoAnswer.tip"), submenu: autoAnswerItems(t, (min) => this.host.autoAnswer(sid, min)) }));

    // ③ 恢复 · 在终端里打开 · 账号
    items.push({ label: "", divider: true });
    if (t && unseen) items.push(grey({ icon: "history", label: copyText("history.row.resume") }));
    else if (t && canResume(t.face)) {
      // 恢复 ▸：与历史页「恢复 ▾」同一个组件（`resume-menu.ts`）；账号那一组等那台的号到了再换上（`appendAccountMenuItems`）。
      items.push({ id: "resume", icon: "history", label: copyText("history.row.resume"), submenu: this.buildResumeSubmenu(sid, { kind: "off" }) });
    }
    // 在终端里打开：出不出照脸上的 `can.attach`（`canAttach`）；命中几个那一句问到了再写。
    if (attach) items.push(grey({ id: "attach", icon: "terminal", label: copyText("sessionHead.act.openTerm"), pending: true }));
    const openPanel = this.host.accountPanelWired?.() ? this.host.openAccountPanel : undefined;
    if (t && openPanel) items.push(grey({ id: "account", icon: "account", label: copyText("acct.menu.open"), onClick: () => openPanel(sid) }));

    // ④ 关闭标签页：已结束 · 记录已不在 · 状态不明的才关得掉；在跑的灰着（先结束）。
    items.push({ label: "", divider: true });
    if (t) {
      const closable = closesFromMenu(t.face);
      items.push({
        id: "close",
        icon: "close",
        label: copyText("launch.slot.close"),
        enabled: closable,
        why: !closable ? copyText("history.delete.liveHint") : unseen ? copyText("tabMenu.close.unseen", { machine: machineName(t.origin) }) : undefined,
        onClick: () => this.host.close(sid),
      });
    }

    // ⑤ 结束会话…：能不能照脸上的 `can.end`（灰着右边写那台那一句）；命中几个 · 在哪个终端里交了由那台判。
    if (t?.face) {
      const end = t.face.can.end;
      items.push({ label: "", divider: true });
      items.push({
        id: "end",
        icon: "failed",
        label: copyText("tabMenu.kill.label"),
        danger: true,
        enabled: end.can === "yes",
        why: end.can === "no" ? end.why : undefined,
        onClick: () => this.actions.endSession(t.origin, sid, t.title),
      });
    }
    // 那台握手时说过做不到的那几项置灰（事实住 monitor 那份 `Offer`）。
    return { items: t ? items.map((i) => gateByOffer(t.origin, i)) : items, t, unseen, attach };
  }

  /**
   * 右键一个标签页（开在指针底下）或锚在一颗按钮上（行尾 / 会话头「更多」）⇒ 组这一代菜单、开、再把要问那台的几格发出去。
   * 会话头「⋯」多交两组：`top` ＝ 会话头放不下收进来的（排最上面）；`bottom` ＝ 流的开关 ＋「调整会话头按钮…」（排最后）。
   */
  open(e: MouseEvent | HTMLElement, sid: string, extra: { top?: readonly MenuItem[]; bottom?: readonly MenuItem[] } = {}): void {
    const { items: base, t, unseen, attach } = this.build(sid);
    const items = [...(extra.top ?? []), ...base];
    // 会话头「⋯」多出来的那几个开关（流的开关，不归那台机器能不能做）。
    if (extra.bottom && extra.bottom.length > 0) items.push({ label: "", divider: true }, ...extra.bottom);
    const shown = tidyDividers(items);
    openMenu(e instanceof HTMLElement ? { el: e, align: "end" } : { x: e.clientX, y: e.clientY }, shown);
    if (unseen || !t) return;
    // 这一代菜单问那台一次 `sessions-where`：几格 tmux 项与「恢复 ▸」的勾共用这一答。
    let asked: Promise<Standing | undefined> | null = null;
    const where = (): Promise<Standing | undefined> => (asked ??= standingOf(t.origin, sid));
    if (attach) void this.resolveAttach(t.origin, sid, where);
    // 已结束的会话：那台的号与默认放在哪到了再把「恢复 ▸」换上。
    if (t.face !== null) void this.appendAccountMenuItems(t.origin, sid, t.face, where);
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
    if (!t || !canResume(t.face)) return;
    void Promise.all([this.resumeAccountsOf(t.origin, t.agent), standingOf(t.origin, sid)]).then(([accounts, s]) => {
      if (anchor.isConnected) openMenu({ el: anchor, align: "end" }, this.buildResumeSubmenu(sid, accounts, s?.run).map((i) => gateByOffer(t.origin, i)));
    });
  }



  /** 会话头 / 钉条 / 报错卡 / ↗ 浮层的［在终端里打开］：交那台接回（接哪个 · 命中几个都是那台判、那台说；出不出照 `canAttach`）。 */
  async attachTo(sid: string): Promise<void> {
    const t = this.host.tab(sid);
    if (!t) return;
    await this.attach(t.origin, sid);
  }

  /** 交那台接回：那一行那台渲（tmux 会话 ⇒ 接会话名 · 托管终端 ⇒ 瘦客户端 `--attach <句柄>`；远端经 SSH），这边开终端窗口跑它。 */
  private async attach(origin: Machine, sid: string): Promise<void> {
    await openSession(origin, { what: { kind: "look", sid }, local: isLocalOrigin(origin) });
  }

  /**
   * 那台答回这个会话在哪个终端里 ⇒［在终端里打开］就位（菜单已换 / 已关 ⇒ 不动）：出不出已照脸定了，这一问只补「命中几个」那一句。
   * 接回走 `session-open look`，那台两种宿主都认。
   */
  private async resolveAttach(origin: Machine, sid: string, where: () => Promise<Standing | undefined>): Promise<void> {
    const gen = menuGeneration();
    const s = await where();
    if (gen !== menuGeneration()) return;
    if (s === undefined) return this.unresolved();
    updateMenuItem("attach", this.attachItem(origin, sid, s));
  }

  /** 就位的［在终端里打开］那一项：命中多个 ⇒ 那台接第一个（第二行先说了）。 */
  private attachItem(origin: Machine, sid: string, s: Standing): MenuItem {
    const ambiguous = s.kind === "ambiguous";
    return {
      id: "attach",
      icon: "terminal",
      label: copyText("sessionHead.act.openTerm"),
      why: ambiguous ? copyText("tabMenu.attach.dupesWhy", { n: s.names.length, name: s.names[0] }) : undefined,
      onClick: () => void this.attach(origin, sid),
    };
  }

  /** 问不到那台（通道不在 / 超时）：［在终端里打开］停在灰着、第二行写原因（不悄悄摘掉）。 */
  private unresolved(): void {
    updateMenuItem("attach", { id: "attach", icon: "terminal", label: copyText("sessionHead.act.openTerm"), enabled: false, why: copyText("tabMenu.probe.failed") });
  }

  /** 每个标签页「恢复 ▸」里勾着的那一组（本窗开着时记）。 */
  private resumePicks = new Map<string, ResumePick>();

  /** 那台的号 ⇒ 恢复菜单的账号组（按这个会话的那一家；还不知道是哪一家 ⇒ 不出账号组，不落哪一家）。 */
  private resumeAccountsOf(origin: Machine, agent: string | null): Promise<ResumeAccounts> {
    if (agent === null) return Promise.resolve({ kind: "off" });
    return resumeAccounts(origin, agent, { hasAccounts: agentHasAccounts(agent), agentName: displayNameOf(agent) ?? agent });
  }

  /**
   * 「恢复 ▸」那一层：最上面一行「恢复」按勾着的那一组起（标签页没有主按钮）；账号 · 运行于两组单选；在此目录新建会话。
   * 起法还是标签页这边那几条：tmux ⇒ 那台在 tmux 里起再接进去（本机同一条）；不用 tmux ⇒ 直接恢复。
   */
  private buildResumeSubmenu(sid: string, accounts: ResumeAccounts, machineRun?: Launch): MenuItem[] {
    let pick = this.resumePicks.get(sid);
    const t = this.host.tab(sid);
    if (!pick) this.resumePicks.set(sid, (pick = defaultPick()));
    const dir = t?.projectDir ?? "";
    return resumeMenuItems({
      accounts,
      pick,
      machineRun,
      run: (p) => void this.runResume(sid, p),
      newInDir: t && dir ? () => void openNewSession({ origin: t.origin, cwd: dir }) : undefined,
    });
  }

  private runResume(sid: string, p: ResumePick): Promise<void> {
    return this.actions.resume(sid, pickAsk(p));
  }

  /** 已结束的会话：菜单开后那台的号到了，把「恢复 ▸」的账号组换上（菜单已换 / 已关 ⇒ 不动）。 */
  private async appendAccountMenuItems(
    origin: string,
    sid: string,
    face: SessionFace,
    where: () => Promise<Standing | undefined>,
  ): Promise<void> {
    if (!canResume(face)) return;
    const gen = menuGeneration();
    const [accounts, s] = await Promise.all([this.resumeAccountsOf(origin, this.host.tab(sid)?.agent ?? null), where()]);
    if (gen !== menuGeneration() || (accounts.kind === "off" && s === undefined)) return;
    updateMenuItem("resume", { id: "resume", icon: "history", label: copyText("history.row.resume"), submenu: this.buildResumeSubmenu(sid, accounts, s?.run) });
  }
}
