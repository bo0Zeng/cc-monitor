/**
 * 〔拆 `tabs.ts` ⑤〕**右键一个 tab，菜单里放哪几项** —— 以及那几格要异步就绪的项怎么就绪。
 *
 * 在新窗口打开 · 加入 / 移出集合 · 固定 · Resume（容器 × 账号 flyout）· 关闭标签 · Attach · 预览 ·
 * 杀死会话 · 就地 resume · 换号重启。在 tmux 里那几项（Attach · 预览 · 杀死 · 就地 resume）亮不亮、写哪个名字问那台后端（`sessions-where`）。项怎么画、菜单怎么开关住 `tab-context-menu.ts`；
 * 点下去真正做事的住 `tab-session-actions.ts`（本文件直接调它，不经 `TabManager` 转一手）。
 *
 * 方法体逐字从 `tabs.ts` 搬来（右键处理器的函数体缩进少了两格，其余一字不差），
 * 唯一的改写是宿主读数：`this.tabs.get(` / 集合 / 固定 换成 `this.host.…`，
 * 会话动作与 tmux 缓存换成 `this.actions.…` —— **都在点击 / 就绪那一刻现读**，与原先读字段的时机相同。
 */
import { showActionFailureToast } from "./error-toast";
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
import {
  enumerateAccountModifiers,
  type AccountModifierOption,
  type NamedAccountModifier,
} from "./launch-menu";
import { runRemoteAttach } from "./remote-launch-run";
// 标签页里的会话都是流跟的那一家（记录树那一家）。
import { ACTIVE_AGENT } from "./agent-profile";
// 本机 = `LOCAL_ORIGIN`（`"<local>"`）；「是不是本机」只经 `ipc/origin.ts` 判。
import { isLocalOrigin, isRemoteOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { openPanePreview } from "./views/pane-preview";
import { standingOf } from "./sessions-where";
import {
  appendTabContextMenuItem,
  menuGeneration,
  removeTabContextMenuItem,
  showTabContextMenu,
  updateTabContextMenuItem,
  type TabMenuItem,
} from "./tab-context-menu";
import type { TabSessionActions } from "./tab-session-actions";
import { machineName, unavailableSaid } from "./control-said";

/** 菜单项 id ⇒ 它要那台后端做的那条命令（那台握手时说过做不到 ⇒ 置灰并说为什么）。 */
const ITEM_OPS: Readonly<Record<string, string>> = { kill: "kill", preview: "terminal-preview", "resume-into": "launch" };

/** 按 `origin` 那台的能力事实给一项置灰：做不到 ⇒ 不可点、字后面带一句为什么。 */
export function gateByOffer(origin: Origin, item: TabMenuItem): TabMenuItem {
  const op = item.id !== undefined ? ITEM_OPS[item.id] : undefined;
  if (op === undefined || item.enabled === false) return item;
  const why = unavailableSaid(origin, op);
  if (why === null) return item;
  return { ...item, enabled: false, title: why, label: copyText("tabMenu.item.greyed", { label: item.label, why }), onClick: () => {} };
}
import { askText } from "./ask-dialog";

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
}

export class TabMenu {
  constructor(
    private readonly host: TabMenuHost,
    private readonly actions: TabSessionActions,
  ) {}

  /** 右键一个 tab ⇒ 组这一代菜单的项、开菜单、再把要异步就绪的几格（attach 反查 / 本机 kill / 账号项）发出去。 */
  open(e: MouseEvent, sid: string): void {
    const t = this.host.tab(sid);
    const items: TabMenuItem[] = [
      { label: copyText("tabMenu.open.openInWindow"), onClick: () => void this.actions.openInNewWindow(sid) },
    ];
    // P7a-3（#61）：集合 —— **纯手动**〔用 08-11「手动建, 不要自动, 纯手动」〕。
    // 二级 flyout：现有集合各一条 + 「新建集合…」；已归组的再给一条「移出集合」。
    const here = this.host.collectionsLoaded() ? this.host.groupOf(sid) : null;
    // 成员上界随成员名单一起作废 ⇒ 「那一组满了」这一句也没了，加进去就是改这个 tab 的组 id。
    const joinItems: TabMenuItem[] = this.host.collections()
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
        const name = await askText(copyText("tabMenu.collection.namePrompt"));
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
    // 行为逐字节保持）；账号项（基座/具名账号，各自再嵌一层容器子选择）由 showTabContextMenu
    // 后**异步追加**（appendAccountMenuItems→updateTabContextMenuItem，复用 F51 代次守卫），
    // 消除同步 peek 的冷缓存分裂。本地归档仍单「Resume」（无容器/账号轴）。
    if (t && t.state.liveness === "unseen") {
      // 说不清（那台暂时看不见）：会话也许还在跑 ⇒ 不给恢复，说为什么。
      const label = copyText("tabMenu.item.resume");
      const why = copyText("sessionState.unseen.tooltip");
      items.push({ label: copyText("tabMenu.item.greyed", { label, why }), enabled: false, title: why, onClick: () => {} });
    } else if (t && canResume(t.state)) {
      if (isRemoteOrigin(t.origin)) {
        items.push({
          id: "resume",
          label: copyText("tabMenu.item.resume"),
          submenu: this.buildResumeSubmenu(sid, []),
        });
      } else {
        items.push({
          label: copyText("tabMenu.item.resume"),
          onClick: () => void this.actions.resumeTab(sid),
        });
      }
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
    // 那台握手时说过做不到的那几项置灰（事实住 monitor 那份 `Offer`）。
    showTabContextMenu(e.clientX, e.clientY, t ? items.map((i) => gateByOffer(t.origin, i)) : items);
    if (remote !== null && t?.projectDir) void this.resolveRemoteTmuxItems(remote, sid);
    if (local) void this.resolveLocalTmuxItems(sid);
    // A4/A5：远端 tab → 异步追加账号项（归档=「把此会话切到账号 X（resume）」/ 活=「…（重启）」）。
    // 本机 tab 也进来（`<local>`）—— 只拿「换号重启」那一项，见 appendAccountMenuItems。
    if (t) void this.appendAccountMenuItems(t.origin, sid, t.state);
  }

  /** 远端：那台答回这个会话的样子 ⇒ Attach · 预览 · 杀死三格就位（菜单已换 / 已关 ⇒ 不动）。 */
  private async resolveRemoteTmuxItems(origin: string, sid: string): Promise<void> {
    const gen = menuGeneration();
    const s = await standingOf(origin, sid);
    if (gen !== menuGeneration()) return;
    const drop = (...ids: string[]): void => ids.forEach((id) => removeTabContextMenuItem(id));
    if (s === undefined || s.kind === "none" || s.kind === "no_tmux") return drop("attach", "preview", "kill");
    const name = s.names[0];
    if (s.kind === "idle") {
      // 明说不可 attach 的会话不算空壳（前台不是 agent 是因为里面跑着别的东西）。
      if (!this.host.isAttachable(sid)) return drop("attach", "preview", "kill");
      updateTabContextMenuItem("attach", {
        id: "attach",
        label: copyText("tabMenu.attach.idle", { name }),
        onClick: () => void runRemoteAttach(origin, ACTIVE_AGENT, name),
      });
      removeTabContextMenuItem("preview"); // 空 shell 没有 agent 画面可看
      updateTabContextMenuItem("kill", gateByOffer(origin, {
        id: "kill",
        label: copyText("tabMenu.kill.idle", { name }),
        danger: true,
        onClick: () => this.actions.killInTmux(origin, sid, name, { idle: true }),
      }));
      return;
    }
    const ambiguous = s.kind === "ambiguous";
    updateTabContextMenuItem("attach", {
      id: "attach",
      label: ambiguous ? copyText("tabMenu.attach.dupes", { name, others: s.names.length - 1 }) : `Attach（tmux: ${name}）`,
      onClick: () => {
        // 命中多个 ⇒ 接第一个（接回可撤销），但说出来。
        if (ambiguous) {
          showActionFailureToast(
            copyText("tabMenu.dupes.title"),
            copyText("tabMenu.dupes.body", { n: s.names.length, name }),
            { level: "info", durationMs: 8000 },
          );
        }
        void runRemoteAttach(origin, ACTIVE_AGENT, name);
      },
    });
    updateTabContextMenuItem("preview", gateByOffer(origin, {
      id: "preview",
      label: copyText("tabMenu.resolveAttachMenuItem.preview"),
      onClick: () => void openPanePreview(origin, name, { terminal: s.terminals[0] }),
    }));
    // 命中多个 ⇒ 不给杀（破坏性，选错了不可逆）。
    updateTabContextMenuItem("kill", gateByOffer(origin, ambiguous
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
      removeTabContextMenuItem("kill");
      removeTabContextMenuItem("resume-into");
      return;
    }
    const name = s.names[0];
    if (s.kind === "idle") {
      updateTabContextMenuItem("kill", gateByOffer(LOCAL_ORIGIN, {
        id: "kill",
        label: copyText("tabMenu.kill.idle", { name }),
        danger: true,
        onClick: () => this.actions.killInTmux(LOCAL_ORIGIN, sid, name, { idle: true }),
      }));
      // 就地 resume 是非破坏性的，与杀并列。
      updateTabContextMenuItem("resume-into", gateByOffer(LOCAL_ORIGIN, {
        id: "resume-into",
        label: copyText("tabMenu.inPlace.idle", { name }),
        onClick: () => void this.actions.resumeLocalInTmux(sid),
      }));
      return;
    }
    removeTabContextMenuItem("resume-into");
    updateTabContextMenuItem("kill", gateByOffer(LOCAL_ORIGIN, s.kind === "ambiguous"
      ? { id: "kill", label: copyText("tabMenu.kill.dupesRefused", { n: s.names.length }), enabled: false, danger: true, onClick: () => {} }
      : { id: "kill", label: copyText("tabMenu.kill.named", { name }), danger: true, onClick: () => this.actions.killInTmux(LOCAL_ORIGIN, sid, name) }));
  }

  /**
   * F09：给「Resume」一级菜单项造 flyout——顶层 tmux/直连两项跟随默认账号（sticky pin，同旧版
   * plain「Resume（tmux/直连）」逐字节保持）；若传入 `accountGroup`（异步账号数据已就绪），
   * 追加基座与每个可选账号入口，各自再嵌一层 tmux/直连子选择——账号×容器真正正交（此前
   * `resumeTabTmux` 不支持显式账号，是本功能顺带补上的实现缺口，见 §0/features/
   * F09-ui-convergence.md「实现期修正」）。
   */
  private buildResumeSubmenu(sid: string, accountOptions: AccountModifierOption[]): TabMenuItem[] {
    const containerLeaves = (accountName: string | undefined, useBase: boolean): TabMenuItem[] => [
      { label: "tmux", onClick: () => void this.actions.resumeTabTmux(sid, accountName, useBase) },
      { label: copyText("tabMenu.containerLeaves.direct"), onClick: () => void this.actions.resumeTab(sid, accountName, useBase) },
    ];
    const items: TabMenuItem[] = [...containerLeaves(undefined, false)];
    if (accountOptions.length > 0) {
      // F09 Phase D 审计（UX，建议）：纯展示性分隔线——把上面"跟随默认账号"两项和下面"换账号"
      // 一组视觉分开，降低扫描成本（不增加点击次数，审计原话："综合任务时间…新版很可能相当
      // 甚至更快，不建议再加独立一级项，折中是加视觉分组"）。
      items.push({ label: "", divider: true });
      // R05：判别联合取代了 `opt.id === "__base__"` 这个跨文件字符串比较。
      for (const opt of accountOptions) {
        items.push({
          label: opt.label,
          submenu:
            opt.kind === "base" ? containerLeaves(undefined, true) : containerLeaves(opt.name, false),
        });
      }
    }
    return items;
  }

  /** F09：给「Restart」一级菜单项造 flyout——重启没有容器轴（对齐 §0 Plan agent 共识：restart
   *  是 kill+resume 编排，作用于会话现有的后端，不经 `LaunchAction`/两个渲染器），只有账号轴，
   *  每个账号再嵌一层「直接重启/先压缩再重启」（danger，§5）。入参已收窄成 `NamedAccountModifier[]`
   *  （`kind === "account"` 那一支），基座在类型上就进不来——
   *  重启从不提供基座逃生口（旧版行为，restart 面对的是已在某账号下运行的活会话，不是老会话）。 */
  private buildRestartSubmenu(sid: string, accounts: NamedAccountModifier[]): TabMenuItem[] {
    return accounts.map((a) => ({
      label: a.label,
      danger: true,
      // F09 Phase D 审计（UX，建议）：这里没有「基座」选项——不是遗漏，是有意为之（restart 面对
      // 的是已在某账号下运行的活会话，不是待迁移的老会话）；hover 到账号名这一层就能看到解释，
      // 不用先读设计文档才知道这不是 bug。
      title: copyText("tabMenu.restart.accountHint", { label: a.label }),
      submenu: [
        {
          label: copyText("tabMenu.restart.direct"),
          danger: true,
          title: copyText("tabMenu.restart.directHint", { name: a.name }),
          onClick: () => void this.actions.restartTabWithAccount(sid, a.name, false),
        },
        {
          label: copyText("tabMenu.restart.compactFirst"),
          danger: true,
          title: copyText("tabMenu.restart.compactFirstHint"),
          onClick: () => void this.actions.restartTabWithAccount(sid, a.name, true),
        },
      ],
    }));
  }

  /** A4/A5：远端 tab 菜单开后**异步追加/更新**账号相关 flyout——归档 tab → 更新「Resume」项的
   *  submenu（补基座+具名账号入口）；活 tab → 账号数 ≥2 时追加一个「Restart」一级项 + flyout
   *  （旧版从不给活会话基座逃生口，见 buildRestartSubmenu）。复用 F51 代次守卫（gen !==
   *  menuGeneration() 则菜单已换/已关，整体 no-op，防 R-1 跨 tab 串味）。账号库不可用（§7
   *  旧/未启用）→ `enumerateAccountModifiers` 内部已容错返回空数组，本方法
   *  据此自然不追加任何东西（默认 Resume 仍在）。异步 fetch 用新鲜值，无冷缓存分裂。 */
  private async appendAccountMenuItems(
    origin: string,
    sid: string,
    state: SessionState,
  ): Promise<void> {
    // 「给 Resume 还是给换号重启」按 `isResumeOnly` 分，与菜单主体那一格同一个谓词（原先是 `status === "archived"`）。
    // 说不清的两样都不给（会话也许还在跑）。
    if (state.liveness === "unseen") return;
    const resumeOnly = isResumeOnly(state);
    // 本机已结束的 tab 不带账号选择（本机 Resume 走那条会话上次的号，
    // 见 `launch-account.ts::localLaunchAccountSync`）⇒ 本机只进下面「换号重启」那一支。
    if (isLocalOrigin(origin) && resumeOnly) return;
    const gen = menuGeneration(); // 捕获这一代菜单
    const accountOptions = await enumerateAccountModifiers(origin);
    if (gen !== menuGeneration()) return; // 菜单已换/已关
    if (resumeOnly) {
      updateTabContextMenuItem("resume", {
        id: "resume",
        label: copyText("tabMenu.item.resume"),
        submenu: this.buildResumeSubmenu(sid, accountOptions),
      });
      return;
    }
    // 活会话重启：旧版阈值——只在 ≥2 个可选具名账号（`kind === "account"`）时才提供，
    // 从不给基座（restart 面对的是已在某账号下运行的活会话，不是待迁移的老会话）。
    // R05：判别联合让"排除基座"变成类型收窄，`realAccounts` 因此是 `NamedAccountModifier[]`
    // ——`a.name` 在类型上可见，不再需要把 `id` 当账号名用。
    const realAccounts = accountOptions.filter(
      (o): o is NamedAccountModifier => o.kind === "account",
    );
    // **这条实际不可达**（R05 Phase D 审计变异 M8 实测存活）：`realAccounts` 来自
    // `enumerateAccountModifiers`，而具名账号只在 `selectable.length >= 2` 时被**整批** push
    // （`launch-menu.ts`），故 `realAccounts.length ∈ {0} ∪ [2, ∞)`，永远不可能是 1。
    // 保留作 belt-and-braces（阈值真正的执行方在 launch-menu 侧），但别以为这里在独立执行阈值。
    if (realAccounts.length < 2) return;
    // F09 Phase D 审计（UX，重要）：⇄ 按钮删除前，重启中的会话至少有"⇄ 立刻置灰"这个视觉信号；
    // 现在这是唯一入口，若不禁用，点了会静默命中 restartTabWithAccount 的 in-flight 守卫、
    // 什么反应都没有——菜单直接呈现"当前不可点"，而不是点了才知道（守卫本身仍在，这里只是让
    // UI 提前说实话）。
    appendTabContextMenuItem({
      id: "restart",
      label: copyText("tabMenu.restart.menu"),
      danger: true,
      enabled: !this.actions.restartingSids.has(sid),
      submenu: this.buildRestartSubmenu(sid, realAccounts),
    });
  }
}
