/**
 * 〔拆 `tabs.ts` ⑤〕**右键一个 tab，菜单里放哪几项** —— 以及那几格要异步就绪的项怎么就绪。
 *
 * 在新窗口打开 · 加入 / 移出集合 · 固定 · 恢复 ▸（与历史页「恢复 ▾」同一个组件）· 关闭标签 · Attach · 预览 ·
 * 杀死会话 · 就地 resume · 换号重启。在 tmux 里那几项（Attach · 预览 · 杀死 · 就地 resume）亮不亮、写哪个名字问那台后端（`sessions-where`）。项怎么画、菜单怎么开关住 `tab-context-menu.ts`；
 * 点下去真正做事的住 `tab-session-actions.ts`（本文件直接调它，不经 `TabManager` 转一手）。
 *
 * 方法体逐字从 `tabs.ts` 搬来（右键处理器的函数体缩进少了两格，其余一字不差），
 * 唯一的改写是宿主读数：`this.tabs.get(` / 集合 / 固定 换成 `this.host.…`，
 * 会话动作与 tmux 缓存换成 `this.actions.…` —— **都在点击 / 就绪那一刻现读**，与原先读字段的时机相同。
 */
import { toast } from "./kit/toast";
import type { Tab } from "./tab-model";
import { canResume, hasTerminal, isResumeOnly, type SessionState } from "./tab-session-state";
import { copyText } from "./copy-table";
import {
  createRefusal,
  newCollectionId,
  type CollectionRefusal,
  type TabCollection,
} from "./tab-collections";
import { sayCollectionRefusal } from "./tab-bar-prefs";
import { defaultPick, resumeAccounts, resumeMenuItems, type ResumeAccounts, type ResumePick } from "./resume-menu";
import { newSessionIn } from "./new-session-in";
import { askOf } from "./launch-account";
import { runRemoteAttach } from "./remote-launch-run";
// 标签页里的会话都是流跟的那一家（记录树那一家）。
import { ACTIVE_AGENT } from "./agent-profile";
// 本机 = `LOCAL_ORIGIN`（`"<local>"`）；「是不是本机」只经 `ipc/origin.ts` 判。
import { isLocalOrigin, isRemoteOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { openPanePreview } from "./views/pane-preview";
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

/** 菜单项 id ⇒ 它要那台后端做的那条命令（那台握手时说过做不到 ⇒ 置灰并说为什么）。 */
const ITEM_OPS: Readonly<Record<string, string>> = { kill: "kill", preview: "terminal-preview", "resume-into": "launch" };

/** 按 `origin` 那台的能力事实给一项置灰：做不到 ⇒ 不可点、字后面带一句为什么。 */
export function gateByOffer(origin: Origin, item: MenuItem): MenuItem {
  const op = item.id !== undefined ? ITEM_OPS[item.id] : undefined;
  if (op === undefined || item.enabled === false) return item;
  const why = unavailableSaid(origin, op);
  if (why === null) return item;
  return { ...item, enabled: false, title: why, label: copyText("tabMenu.item.greyed", { label: item.label, why }), onClick: () => {} };
}
import { askText } from "./kit/dialog";

/** 菜单要宿主给的读数 / 回调。全是**现读**：菜单项的 `onClick` 在点下去那一刻才调它们。 */
export interface TabMenuHost {
  tab(sid: string): Tab | undefined;
  isAttachable(sid: string): boolean;
  /** 这个实例拉过集合没有（撕离出来的 viewer 窗口从不拉 ⇒ 不给集合入口）。 */
  collectionsLoaded(): boolean;
  collections(): TabCollection[];
  /** 这个 tab 在哪个组（读 `Tab.group`）。下面三个动作改完内存就重画、落盘由落盘偏好那一份做。 */
  groupOf(sid: string): TabCollection | null;
  joinGroup(sid: string, gid: string): void;
  /** 建组并把这几个 tab 放进去；组数到上界 ⇒ 回拒绝原因、什么都不做。 */
  foundGroup(sids: string[], name: string, id: string): CollectionRefusal | null;
  leaveGroup(sid: string): void;
  /** 同上一条理由：没 `loadPinned` 过就不给固定入口。 */
  pinnedLoaded(): boolean;
  togglePin(sid: string): void;
  /** 关掉这个标签（与批量菜单「关闭标签」、× 同一个动作）。 */
  close(sid: string): void;
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

  /** 右键一个 tab ⇒ 组这一代菜单的项、开菜单、再把要异步就绪的几格（attach 反查 / 本机 kill / 账号项）发出去。 */
  /** 开在指针底下（右键）或锚在一颗按钮上（行尾 / 会话头「更多」）。 */
  open(e: MouseEvent | HTMLElement, sid: string, extra: readonly MenuItem[] = []): void {
    const t = this.host.tab(sid);
    const items: MenuItem[] = [
      { label: copyText("tabMenu.open.openInWindow"), onClick: () => void this.actions.openInNewWindow(sid) },
    ];
    // P7a-3（#61）：集合 —— **纯手动**〔用 08-11「手动建, 不要自动, 纯手动」〕。
    // 二级 flyout：现有集合各一条 + 「新建集合…」；已归组的再给一条「移出集合」。
    const here = this.host.collectionsLoaded() ? this.host.groupOf(sid) : null;
    // 成员上界随成员名单一起作废 ⇒ 「那一组满了」这一句也没了，加进去就是改这个 tab 的组 id。
    const joinItems: MenuItem[] = this.host.collections()
      .filter((col) => col.id !== here?.id)
      .map((col) => ({
        label: col.name,
        onClick: () => this.host.joinGroup(sid, col.id),
      }));
    joinItems.push({
      label: copyText("tabMenu.collection.new"),
      onClick: () => void (async () => {
        // 到上界先说，再问名字（不让用户白填一次）。
        const full = createRefusal(this.host.collections());
        if (full) return sayCollectionRefusal(full);
        const name = await askText({ title: copyText("tabMenu.collection.title"), label: copyText("tabMenu.collection.namePrompt"), action: copyText("tabMenu.collection.action") });
        if (!name?.trim()) return;
        // 问名字那一会儿里别处又建满了 ⇒ 仍要说出来（判定在落盘偏好那一份里再过一次）。
        const why = this.host.foundGroup([sid], name, newCollectionId());
        if (why) sayCollectionRefusal(why);
      })(),
    });
    if (this.host.collectionsLoaded()) items.push({ label: copyText("tabMenu.collection.add"), submenu: joinItems });
    // 〔`§B.7`〕固定 —— 与「加入集合」同级。**这是唯一的入口**（不做自动固定）。
    // ⚠ `pinnedLoaded` 那道门与集合同一条理由：没读过盘就改，等于把用户上次固定的清空。
    if (t && this.host.pinnedLoaded()) {
      items.push({
        label: t.pinned ? copyText("tabMenu.pin.unpin") : copyText("tabMenu.pin.pin"),
        // 说到会话状态的句子住文案表 `sessionState.*`；原句里的「变灰」「灰着」是禁用词。
        title: t.pinned
          ? copyText("sessionState.pin.unpinHint")
          : copyText("sessionState.pin.pinHint"),
        onClick: () => this.host.togglePin(sid),
      });
    }
    if (here) {
      items.push({
        label: copyText("tabMenu.collection.remove", { name: here.name }),
        onClick: () => this.host.leaveGroup(sid),
      });
    }
    // 「重新读取」不在这里：挪成 tab 栏上常驻的一颗（`tab-bar-view.ts`），一按对所有打开的 tab 生效。
    // F37：灰 tab（会话已结束）右键手动 resume——不用绕去历史浏览器。
    // F41 起本地与远端都是一键拉起新终端（远端=wt.exe 跑 ssh -t，失败才回退复制命令）。
    // F09：远端归档 tab 收敛成 1 个「Resume」一级项 + 二级 flyout（容器×账号，
    // §2.6）——顶层 tmux/直连两项跟随默认账号（sticky pin，同旧版 plain「Resume（tmux/直连）」
    // 行为逐字节保持）；账号项（基座/具名账号，各自再嵌一层容器子选择）由 openMenu
    // 后**异步追加**（appendAccountMenuItems→updateMenuItem，复用 F51 代次守卫），
    // 消除同步 peek 的冷缓存分裂。本地归档仍单「Resume」（无容器/账号轴）。
    if (t && t.state.liveness === "unseen") {
      // 说不清（那台暂时看不见）：会话也许还在跑 ⇒ 不给恢复，说为什么。
      const label = copyText("tabMenu.item.resume");
      const why = copyText("sessionState.unseen.tooltip");
      items.push({ label: copyText("tabMenu.item.greyed", { label, why }), enabled: false, title: why, onClick: () => {} });
    } else if (t && canResume(t.state)) {
      // 恢复 ▸：与历史页「恢复 ▾」同一个组件（`resume-menu.ts`）；账号那一组等那台的号到了再换上（`appendAccountMenuItems`）。
      items.push({ id: "resume", label: copyText("tabMenu.item.resume"), submenu: this.buildResumeSubmenu(sid, { kind: "off" }) });
    }
    // 关闭：已结束 · 记录没了 · 说不清（窄窗里那颗 × 看不见，这一项不靠它）。说不清的悬停说它若还在跑、连上那台之后会回来。
    if (t && isResumeOnly(t.state)) {
      items.push({
        label: copyText("tabMenu.item.close"),
        title: t.state.liveness === "unseen" ? copyText("tabMenu.close.unseenHint", { machine: machineName(t.origin) }) : undefined,
        onClick: () => this.host.close(sid),
      });
    }
    // 这个会话在那台哪个 tmux 会话里（Attach · 预览 · 杀死 · 就地 resume 亮不亮、写哪个名字）：问那台后端（`sessions-where`），
    //   界面不判。先放「检测中」占位，答回来再换成可点的那几项（同一代菜单才换）。
    const remote = t !== undefined && isRemoteOrigin(t.origin) ? t.origin : null;
    if (remote !== null && t?.projectDir) {
      items.push(
        { id: "attach", label: copyText("tabMenu.attach.probing"), enabled: false, onClick: () => {} },
        { id: "preview", label: copyText("tabMenu.preview.probing"), enabled: false, onClick: () => {} },
        { id: "kill", label: copyText("tabMenu.kill.probing"), enabled: false, danger: true, onClick: () => {} },
      );
    }
    const local = t !== undefined && isLocalOrigin(t.origin) && hasTerminal(t.state);
    if (local) {
      items.push(
        { id: "kill", label: copyText("tabMenu.kill.probing"), enabled: false, danger: true, onClick: () => {} },
        { id: "resume-into", label: copyText("tabMenu.inPlace.probing"), enabled: false, onClick: () => {} },
      );
    }
    // 「账号…」：热切换 / 重启切换都在面板里。说不清（那台看不见）⇒ 灰着、悬停说是哪台。
    const openPanel = this.host.accountPanelWired?.() ? this.host.openAccountPanel : undefined;
    if (t && openPanel) {
      const unseen = t.state.liveness === "unseen";
      items.push({
        id: "account",
        label: copyText("acct.menu.open"),
        enabled: !unseen,
        title: unseen ? copyText("acct.menu.unseen", { machine: machineName(t.origin) }) : undefined,
        onClick: () => openPanel(sid),
      });
    }
    // 会话头「⋯」多出来的那几个开关（流的开关，不归那台机器能不能做）。
    if (extra.length > 0) items.push({ label: "", divider: true }, ...extra);
    // 那台握手时说过做不到的那几项置灰（事实住 monitor 那份 `Offer`）。
    openMenu(e instanceof HTMLElement ? { el: e, align: "end" } : { x: e.clientX, y: e.clientY }, t ? items.map((i) => gateByOffer(t.origin, i)) : items);
    if (remote !== null && t?.projectDir) void this.resolveRemoteTmuxItems(remote, sid);
    if (local) void this.resolveLocalTmuxItems(sid);
    // 已结束的会话：那台的号到了再把「恢复 ▸」的账号组换上。
    if (t) void this.appendAccountMenuItems(t.origin, sid, t.state);
  }

  /**
   * 会话头 / 「需要你」钉条的［恢复 ▾］：只开恢复那几项（与右键「恢复 ▸」同一份：恢复 · 账号 · 运行于 · 在此目录新建会话；号到了再开）。
   */
  openResumeMenu(anchor: HTMLElement, sid: string): void {
    const t = this.host.tab(sid);
    if (!t || !canResume(t.state)) return;
    void this.resumeAccountsOf(t.origin).then((accounts) => {
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
    void runRemoteAttach(t.origin, ACTIVE_AGENT, s.names[0]);
  }

  /** 远端：那台答回这个会话的样子 ⇒ Attach · 预览 · 杀死三格就位（菜单已换 / 已关 ⇒ 不动）。 */
  private async resolveRemoteTmuxItems(origin: string, sid: string): Promise<void> {
    const gen = menuGeneration();
    const s = await standingOf(origin, sid);
    if (gen !== menuGeneration()) return;
    const drop = (...ids: string[]): void => ids.forEach((id) => removeMenuItem(id));
    if (s === undefined || s.kind === "none" || s.kind === "no_tmux") return drop("attach", "preview", "kill");
    const name = s.names[0];
    if (s.kind === "idle") {
      // 明说不可 attach 的会话不算空壳（前台不是 agent 是因为里面跑着别的东西）。
      if (!this.host.isAttachable(sid)) return drop("attach", "preview", "kill");
      updateMenuItem("attach", {
        id: "attach",
        label: copyText("tabMenu.attach.idle", { name }),
        onClick: () => void runRemoteAttach(origin, ACTIVE_AGENT, name),
      });
      removeMenuItem("preview"); // 空 shell 没有 agent 画面可看
      updateMenuItem("kill", gateByOffer(origin, {
        id: "kill",
        label: copyText("tabMenu.kill.idle", { name }),
        danger: true,
        onClick: () => this.actions.killInTmux(origin, sid, name, { idle: true }),
      }));
      return;
    }
    const ambiguous = s.kind === "ambiguous";
    updateMenuItem("attach", {
      id: "attach",
      label: ambiguous ? copyText("tabMenu.attach.dupes", { name, others: s.names.length - 1 }) : `Attach（tmux: ${name}）`,
      onClick: () => {
        // 命中多个 ⇒ 接第一个（接回可撤销），但说出来。
        if (ambiguous) {
          toast(
            copyText("tabMenu.dupes.title"),
            copyText("tabMenu.dupes.body", { n: s.names.length, name }),
            { level: "info" },
          );
        }
        void runRemoteAttach(origin, ACTIVE_AGENT, name);
      },
    });
    updateMenuItem("preview", gateByOffer(origin, {
      id: "preview",
      label: copyText("tabMenu.resolveAttachMenuItem.preview"),
      onClick: () => void openPanePreview(origin, name, { terminal: s.terminals[0] }),
    }));
    // 命中多个 ⇒ 不给杀（破坏性，选错了不可逆）。
    updateMenuItem("kill", gateByOffer(origin, ambiguous
      ? { id: "kill", label: copyText("tabMenu.kill.dupes", { n: s.names.length }), danger: true, enabled: false, onClick: () => {} }
      : { id: "kill", label: copyText("tabMenu.kill.plain"), danger: true, onClick: () => this.actions.killInTmux(origin, sid, name) }));
  }

  /** 本机：那台答回这个会话的样子 ⇒ 杀死 · 就地 resume 两格就位（菜单已换 / 已关 ⇒ 不动）。 */
  private async resolveLocalTmuxItems(sid: string): Promise<void> {
    const gen = menuGeneration();
    const s = await standingOf(LOCAL_ORIGIN, sid);
    if (gen !== menuGeneration()) return;
    // 问不到（本机后端通道不在）/ 不在 tmux 里 ⇒ 不留一个假装能用的破坏性项。
    if (s === undefined || s.kind === "none" || s.kind === "no_tmux" || (s.kind === "idle" && !this.host.isAttachable(sid))) {
      removeMenuItem("kill");
      removeMenuItem("resume-into");
      return;
    }
    const name = s.names[0];
    if (s.kind === "idle") {
      updateMenuItem("kill", gateByOffer(LOCAL_ORIGIN, {
        id: "kill",
        label: copyText("tabMenu.kill.idle", { name }),
        danger: true,
        onClick: () => this.actions.killInTmux(LOCAL_ORIGIN, sid, name, { idle: true }),
      }));
      // 就地 resume 是非破坏性的，与杀并列。
      updateMenuItem("resume-into", gateByOffer(LOCAL_ORIGIN, {
        id: "resume-into",
        label: copyText("tabMenu.inPlace.idle", { name }),
        onClick: () => void this.actions.resumeLocalInTmux(sid),
      }));
      return;
    }
    removeMenuItem("resume-into");
    updateMenuItem("kill", gateByOffer(LOCAL_ORIGIN, s.kind === "ambiguous"
      ? { id: "kill", label: copyText("tabMenu.kill.dupesRefused", { n: s.names.length }), enabled: false, danger: true, onClick: () => {} }
      : { id: "kill", label: copyText("tabMenu.kill.named", { name }), danger: true, onClick: () => this.actions.killInTmux(LOCAL_ORIGIN, sid, name) }));
  }

  /** 每个标签页「恢复 ▸」里勾着的那一组（本窗开着时记）。 */
  private resumePicks = new Map<string, ResumePick>();

  /** 那台的号 ⇒ 恢复菜单的账号组（标签页里的会话都是流跟的那一家）。 */
  private resumeAccountsOf(origin: Origin): Promise<ResumeAccounts> {
    return resumeAccounts(origin, ACTIVE_AGENT, { hasAccounts: true, agentName: ACTIVE_AGENT });
  }

  /**
   * 「恢复 ▸」那一层：最上面一行「恢复」按勾着的那一组起（标签页没有主按钮）；账号 · 运行于两组单选；在此目录新建会话。
   * 起法还是标签页这边那几条：tmux ⇒ 那台在 tmux 里起再接进去（本机同一条）；不用 tmux ⇒ 直接恢复。
   */
  private buildResumeSubmenu(sid: string, accounts: ResumeAccounts): MenuItem[] {
    let pick = this.resumePicks.get(sid);
    if (!pick) this.resumePicks.set(sid, (pick = defaultPick()));
    const t = this.host.tab(sid);
    const dir = t?.projectDir ?? "";
    return resumeMenuItems({
      accounts,
      pick,
      run: (p) => void this.runResume(sid, p),
      newInDir: t && dir ? () => void newSessionIn(isLocalOrigin(t.origin) ? undefined : t.origin, dir, ACTIVE_AGENT) : undefined,
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
    const accounts = await this.resumeAccountsOf(origin);
    if (gen !== menuGeneration() || accounts.kind === "off") return;
    updateMenuItem("resume", { id: "resume", label: copyText("tabMenu.item.resume"), submenu: this.buildResumeSubmenu(sid, accounts) });
  }
}
