/**
 * 〔U2 · 拆 `tabs.ts` ⑤〕**右键一个 tab，菜单里放哪几项** —— 以及那几格要异步就绪的项怎么就绪。
 *
 * 在新窗口打开 · 加入 / 移出集合 · 固定 · 全景高亮 · Resume（容器 × 账号 flyout）· Attach · 预览 ·
 * 杀死会话 · 就地 resume · 换号重启。项怎么画、菜单怎么开关住 `tab-context-menu.ts`；
 * 点下去真正做事的住 `tab-session-actions.ts`（本文件直接调它，不经 `TabManager` 转一手）。
 *
 * 方法体逐字从 `tabs.ts` 搬来（右键处理器的函数体缩进少了两格，其余一字不差），
 * 唯一的改写是宿主读数：`this.tabs.get(` / 集合 / 固定 / 全景回调 换成 `this.host.…`，
 * 会话动作与 tmux 缓存换成 `this.actions.…` —— **都在点击 / 就绪那一刻现读**，与原先读字段的时机相同。
 */
import { showActionFailureToast } from "./error-toast";
import type { Tab } from "./tab-model";
import { hasTerminal, isResumeOnly, type SessionState } from "./tab-session-state";
import { copyText } from "./copy-table";
import {
  addMember,
  collectionOf,
  createCollection,
  createRefusal,
  memberRefusal,
  newCollectionId,
  removeMember,
  type TabCollection,
} from "./tab-collections";
import { sayCollectionRefusal } from "./tab-bar-prefs";
import {
  enumerateAccountModifiers,
  type AccountModifierOption,
  type NamedAccountModifier,
} from "./launch-menu";
import { runLocalResumeIntoExistingTmux, runRemoteAttach } from "./remote-launch-run";
import { AGENT_PROFILE } from "./agent-profile";
// 〔C4a〕本机 = `LOCAL_ORIGIN`（`"<local>"`）；「是不是本机」只经 `ipc/origin.ts` 判。
import { isLocalOrigin, isRemoteOrigin, LOCAL_ORIGIN } from "./ipc/origin";
import { openPanePreview } from "./views/pane-preview";
import { getBehavior } from "./behavior";
import {
  findClaudeTmuxMatches,
  findClaudeTmux,
  findIdleTmux,
  isCwdFallbackMatch,
} from "./tmux-sessions";
import {
  appendTabContextMenuItem,
  menuGeneration,
  removeTabContextMenuItem,
  showTabContextMenu,
  updateTabContextMenuItem,
  type TabMenuItem,
} from "./tab-context-menu";
import { TMUX_CACHE_TTL_MS, type TabSessionActions } from "./tab-session-actions";

/** F74c(#60-B)：cwd 回退串味风险提示（attach 到可能是同目录别的会话前）。 */
function warnCwdFallbackAttach(): void {
  showActionFailureToast(
    // 〔U2 · 按 `terms.json` 改词〕不说标记（`@ccm_sid` 禁：说后果「认不出是哪个会话」），
    //   不派「重装 ccm 助手」这件用户做了也未必好的活（「ccm 助手」禁；CP1 口径 §2.3）。
    copyText("tabMenu.cwdFallback.title"),
    copyText("tabMenu.cwdFallback.body"),
    { level: "info", durationMs: 8000 },
  );
}

/** 菜单要宿主给的读数 / 回调。全是**现读**：菜单项的 `onClick` 在点下去那一刻才调它们。 */
export interface TabMenuHost {
  tab(sid: string): Tab | undefined;
  isAttachable(sid: string): boolean;
  /** 这个实例拉过集合没有（撕离出来的 viewer 窗口从不拉 ⇒ 不给集合入口）。 */
  collectionsLoaded(): boolean;
  collections(): TabCollection[];
  commitCollections(next: TabCollection[]): Promise<void>;
  /** 同上一条理由：没 `loadPinned` 过就不给固定入口。 */
  pinnedLoaded(): boolean;
  togglePin(sid: string): void;
  /** F70：「在全景高亮本会话改动」—— 宿主注入的回调（没注入就什么都不做）。 */
  requestPanoramaHighlight(sid: string): void;
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
    const here = this.host.collectionsLoaded() ? collectionOf(this.host.collections(), sid) : null;
    const joinItems: TabMenuItem[] = this.host.collections()
      .filter((col) => col.id !== here?.id)
      .map((col) => ({
        label: col.name,
        onClick: () => {
          // 〔TL2 · E13〕那个集合满了 ⇒ 说出来（`addMember` 照旧原样返回，不写盘）。
          const why = memberRefusal(this.host.collections(), col.id, sid);
          if (why) return sayCollectionRefusal(why);
          void this.host.commitCollections(addMember(this.host.collections(), col.id, sid));
        },
      }));
    joinItems.push({
      label: copyText("tabMenu.collection.new"),
      onClick: () => {
        // 〔TL2 · E13〕到上界先说，再问名字（不让用户白填一次）。
        const full = createRefusal(this.host.collections());
        if (full) return sayCollectionRefusal(full);
        const name = window.prompt(copyText("tabMenu.collection.namePrompt"));
        if (!name?.trim()) return;
        const id = newCollectionId();
        const withNew = createCollection(this.host.collections(), name, id);
        // 名字空/到上界时 `createCollection` 原样返回 ⇒ 别再往一个不存在的集合里塞成员。
        if (withNew.length === this.host.collections().length) return;
        void this.host.commitCollections(addMember(withNew, id, sid));
      },
    });
    if (this.host.collectionsLoaded()) items.push({ label: copyText("tabMenu.collection.add"), submenu: joinItems });
    // 〔步 17·B · `§B.7`〕固定 —— 与「加入集合」同级。**这是唯一的入口**（不做自动固定）。
    // ⚠ `pinnedLoaded` 那道门与集合同一条理由：没读过盘就改，等于把用户上次固定的清空。
    if (t && this.host.pinnedLoaded()) {
      items.push({
        label: t.pinned ? copyText("tabMenu.pin.unpin") : copyText("tabMenu.pin.pin"),
        // 〔U4〕说到会话状态的句子住文案表 `sessionState.*`；原句里的「变灰」「灰着」是禁用词（`设计/91 §4`）。
        title: t.pinned
          ? copyText("sessionState.pin.unpinHint")
          : copyText("sessionState.pin.pinHint"),
        onClick: () => this.host.togglePin(sid),
      });
    }
    if (here) {
      items.push({
        label: copyText("tabMenu.collection.remove", { name: here.name }),
        onClick: () => void this.host.commitCollections(removeMember(this.host.collections(), sid)),
      });
    }
    // F70（护城河）：本地会话 + 有改动集 → 「在全景高亮本会话改动」。远端（代码不在本机、
    // code-picture 索引不到）/ 无改动 都不显示（门控之一，另两道在 touchedFilesFor + highlightSession）。
    if (t && isLocalOrigin(t.origin) && t.touchedFiles.size > 0) {
      items.push({
        label: copyText("tabMenu.open.panoramaHighlight"),
        onClick: () => this.host.requestPanoramaHighlight(sid),
      });
    }
    // F37：灰 tab（会话已结束）右键手动 resume——不用绕去历史浏览器。
    // F41 起本地与远端都是一键拉起新终端（远端=wt.exe 跑 ssh -t，失败才回退复制命令）。
    // F09：远端归档 tab 收敛成 1 个「Resume」一级项 + 二级 flyout（容器×账号，MASTERPLAN
    // §2.6）——顶层 tmux/直连两项跟随默认账号（sticky pin，同旧版 plain「Resume（tmux/直连）」
    // 行为逐字节保持）；账号项（基座/具名账号，各自再嵌一层容器子选择）由 showTabContextMenu
    // 后**异步追加**（appendAccountMenuItems→updateTabContextMenuItem，复用 F51 代次守卫），
    // 消除同步 peek 的冷缓存分裂。本地归档仍单「Resume」（无容器/账号轴）。
    if (t && isResumeOnly(t.state)) {
      if (isRemoteOrigin(t.origin)) {
        items.push({
          id: "resume",
          label: "Resume",
          submenu: this.buildResumeSubmenu(sid, []),
        });
      } else {
        items.push({
          label: "Resume",
          onClick: () => void this.actions.resumeTab(sid),
        });
      }
    }
    // F51：远端 tab（有 cwd）——反查该 cwd 正跑 claude 的 tmux 会话 → Attach。
    // 缓存命中同步定夺(无占位闪烁);未命中先禁用占位「检测中」+ 异步查询就绪。
    // 〔C4a〕下面这一段只对**远端** tab：`remote` = 那台远端的名字；本机 tab / 没有这个 tab ⇒ `null`。
    const remote = t !== undefined && isRemoteOrigin(t.origin) ? t.origin : null;
    const cwd = t?.cwd ?? null;
    let needAsyncAttach = false;
    if (remote !== null && cwd) {
      const cached = this.actions.tmuxCache.get(remote);
      if (cached && Date.now() - cached.ts < TMUX_CACHE_TTL_MS) {
        const m = findClaudeTmux(cached.sessions, sid, cwd);
        const viaCwd = isCwdFallbackMatch(cached.sessions, sid); // F74c：回退命中提示串味
        // F04（R10）：同 `resolveAttachMenuItem` 的分级——attach/preview 警告+继续，kill 拒绝。
        const cachedMatches = findClaudeTmuxMatches(cached.sessions, sid);
        const cachedAmbiguous = cachedMatches.length > 1;
        if (m) {
          items.push({
            id: "attach",
            label: cachedAmbiguous
              ? copyText("tabMenu.attach.dupes", { name: m.name, others: cachedMatches.length - 1 })
              : `Attach（tmux: ${m.name}）`,
            onClick: () => {
              if (viaCwd) warnCwdFallbackAttach();
              if (cachedAmbiguous) {
                showActionFailureToast(
                  copyText("tabMenu.dupes.title"),
                  copyText("tabMenu.dupes.body", { n: cachedMatches.length, name: m.name }),
                  { level: "info", durationMs: 8000 },
                );
              }
              void runRemoteAttach(remote, m.name);
            },
          });
          // F60：同一 tmux 会话可只读预览画面（capture-pane 快照，不 attach）——只读，不受影响。
          items.push({
            id: "preview",
            label: copyText("tabMenu.open.preview"),
            onClick: () => void openPanePreview(remote, m.name),
          });
          // F79：杀死会话——命中 ≥2 个时拒绝提供（破坏性，选错代价不可逆）。
          if (cachedAmbiguous) {
            items.push({
              id: "kill",
              label: copyText("tabMenu.kill.dupes", { n: cachedMatches.length }),
              danger: true,
              enabled: false,
              onClick: () => {},
            });
          } else {
            items.push({
              id: "kill",
              label: copyText("tabMenu.kill.plain"),
              danger: true,
              onClick: () => this.actions.killRemoteTmux(remote, m.name, viaCwd),
            });
          }
        } else {
          // audit-fixes F03.3：缓存命中、无活 claude，但有目标 sid 的空 tmux（idle-tmux）→ 同步给 attach。
          // E73：同上——不可 attach 的不算空壳。
          const idle = this.host.isAttachable(sid)
            ? findIdleTmux(cached.sessions, sid)
            : undefined;
          if (idle) {
            items.push({
              id: "attach",
              label: copyText("tabMenu.attach.idle", { name: idle.name }),
              onClick: () => void runRemoteAttach(remote, idle.name),
            });
            // UX 审计 #1：灰态(idle-tmux)也给 kill——杀空 tmux → tab 转归档 → 可 Resume（给死角一个出口）。
            items.push({
              id: "kill",
              label: copyText("tabMenu.kill.idle", { name: idle.name }),
              danger: true,
              onClick: () => this.actions.killRemoteTmux(remote, idle.name, false, { idle: true }),
            });
          }
        }
      } else {
        items.push({
          id: "attach",
          label: copyText("tabMenu.attach.probing"),
          enabled: false,
          onClick: () => {},
        });
        items.push({
          id: "preview",
          label: copyText("tabMenu.preview.probing"),
          enabled: false,
          onClick: () => {},
        });
        items.push({
          id: "kill",
          label: copyText("tabMenu.kill.probing"),
          enabled: false,
          danger: true,
          onClick: () => {},
        });
        needAsyncAttach = true;
      }
    }
    // ★ P3 刀 2 的 UI 半：**本机 tab 也给「杀死会话」**。
    //
    // 只加 kill 这一格 —— attach / 预览那两格本机今天还没有对象可接
    //（前者要本机 attach 路径、后者要 `capture_remote_pane` 的本机对侧），归后面的刀。
    // 一次只开一格，是为了让「哪一格已经通了」这件事在菜单上就是可见的。
    let needAsyncLocalKill = false;
    if (t !== undefined && isLocalOrigin(t.origin) && hasTerminal(t.state)) {
      items.push({
        id: "kill",
        label: copyText("tabMenu.kill.probing"),
        enabled: false,
        danger: true,
        onClick: () => {},
      });
      // P3 刀 3 的占位：查回来是空 tmux 才留下，否则移除。
      items.push({
        id: "resume-into",
        label: copyText("tabMenu.inPlace.probing"),
        enabled: false,
        onClick: () => {},
      });
      needAsyncLocalKill = true;
    }
    showTabContextMenu(e.clientX, e.clientY, items);
    if (needAsyncAttach && remote !== null && cwd) {
      void this.resolveAttachMenuItem(remote, cwd, sid);
    }
    if (needAsyncLocalKill) {
      void this.resolveLocalKillMenuItem(sid);
    }
    // A4/A5：远端 tab → 异步追加账号项（归档=「把此会话切到账号 X（resume）」/ 活=「…（重启）」）。
    // 〔`A3` 第二波〕本机 tab 也进来（`<local>`）—— 只拿「换号重启」那一项，见 appendAccountMenuItems。
    if (t) void this.appendAccountMenuItems(t.origin, sid, t.state);
  }

  /**
   * F51：菜单打开后异步反查 tmux——查该 origin 的会话列表(短缓存),按 `path===cwd &&
   * command==="claude"` 反查该 tab 的 Claude 所在 tmux 会话。命中 → 把禁用占位「检测中」
   * 换成可点的 Attach;无 tmux / 无匹配 / 查询失败 → 移除占位。菜单已关则 update/remove no-op。
   */
  /** ★ P3 刀 2 的 UI 半：本机 tab 的「杀死会话」。
   *
   *  与远端那条（`resolveAttachMenuItem`）**共用同一批判定函数**（`findClaudeTmuxMatches`）——
   *  这就是 `C1`「差别只允许出现在传输这一跳」：读口不同（backend 快照 vs 一次性 SSH），
   *  之后的一切逐字相同。
   *
   *  ⚠ **按 `@ccm_sid` 认，不按名字前缀猜。** 本机会话名今天确实长成 `<sid8>-cc`，
   *  但拿那个去匹配就是「用命名巧合当身份」—— `INVARIANTS §30` 逐字禁的正是这一类
   *  （它禁的是按 cwd 猜，同一个错的另一种写法）。名字会被 `/branch` 漂移、会被用户改名。
   */
  private async resolveLocalKillMenuItem(sid: string): Promise<void> {
    const gen = menuGeneration();
    const got = await this.actions.fetchTmuxFresh(LOCAL_ORIGIN);
    if (gen !== menuGeneration()) return;
    // `undefined` = 读口抛了；`null` = **本机后端通道不在**（不知道，不是「没有」）。
    // 两种都不该留一个假装能用的菜单项 —— 移除它，别让用户点一个必失败的破坏性动作。
    if (got === undefined || got === null) {
      removeTabContextMenuItem("kill");
      removeTabContextMenuItem("resume-into");
      return;
    }
    // ★★ P3 刀 3：**空 tmux（claude 已退、只剩交互 shell）→ 就地 resume**。
    //
    // 先判这一格，因为它与下面那格互斥：有活 claude 就不是空壳。
    // E73 同款前提：明说不可 attach 的会话**不算空壳** —— 它前台不是 claude 恰恰是因为
    // 里面跑着别的东西，不是没人。
    const idle = this.host.isAttachable(sid) ? findIdleTmux(got, sid) : undefined;
    if (idle) {
      const behavior = await getBehavior();
      updateTabContextMenuItem("kill", {
        id: "kill",
        label: copyText("tabMenu.kill.idle", { name: idle.name }),
        danger: true,
        onClick: () => this.actions.killRemoteTmux(LOCAL_ORIGIN, idle.name, false, { idle: true }),
      });
      // 就地 resume 是**非破坏性**的，与 kill 并列给出（远端那侧同样两格并列）。
      updateTabContextMenuItem("resume-into", {
        id: "resume-into",
        label: copyText("tabMenu.inPlace.idle", { name: idle.name }),
        onClick: () =>
          void runLocalResumeIntoExistingTmux(
            sid,
            idle.name,
            behavior.resumeCommandLocal || AGENT_PROFILE.defaultLauncher,
          ),
      });
      return;
    }
    const matches = findClaudeTmuxMatches(got, sid);
    if (matches.length === 0) {
      removeTabContextMenuItem("kill");
      removeTabContextMenuItem("resume-into");
      return;
    }
    removeTabContextMenuItem("resume-into");
    // F04（R10）同款分级：破坏性动作命中 ≥2 个就**拒绝**，不折叠成第一个。
    if (matches.length > 1) {
      updateTabContextMenuItem("kill", {
        id: "kill",
        label: copyText("tabMenu.kill.dupesRefused", { n: matches.length }),
        enabled: false,
        danger: true,
        onClick: () => {},
      });
      return;
    }
    const name = matches[0].name;
    updateTabContextMenuItem("kill", {
      id: "kill",
      label: copyText("tabMenu.kill.named", { name }),
      danger: true,
      onClick: () => this.actions.killRemoteTmux(LOCAL_ORIGIN, name, false),
    });
  }

  private async resolveAttachMenuItem(
    origin: string,
    cwd: string,
    sid: string,
  ): Promise<void> {
    const gen = menuGeneration(); // 捕获发起查询的那一代菜单(R-1 守卫)
    const got = await this.actions.fetchTmuxFresh(origin);
    if (got === undefined) {
      // 查询失败(纯 ssh exec 抖动)→ 移除占位,不缓存。
      if (gen === menuGeneration()) {
        removeTabContextMenuItem("attach");
        removeTabContextMenuItem("preview"); // F60：预览占位一并移除
        removeTabContextMenuItem("kill"); // F79：杀会话占位一并移除
      }
      return;
    }
    const sessions = got;
    // 菜单已换/已关(新代次)→ 别动别的菜单(R-1 跨 tab 串味)。
    if (gen !== menuGeneration()) return;
    const match = findClaudeTmux(sessions, sid, cwd);
    const viaCwd = isCwdFallbackMatch(sessions, sid); // F74c：回退命中 attach 前提示串味风险
    // F04（R10）：命中 ≥2 个精确同 sid 的活会话时——`matches.length>1` 与 `viaCwd` 互斥（后者只在
    // "整张列表无任何会话带 sid"时才可能真，见 `findClaudeTmux`/`isCwdFallbackMatch` 判据），
    // 故两条 caveat 不会同时触发。attach/preview 沿用 resume 的"警告+继续"（非破坏性、可撤销）；
    // kill 沿用 restart 的"拒绝"（破坏性、代价不可逆）——分级理由见 F04 计划 §2 取舍④。
    const matches = findClaudeTmuxMatches(sessions, sid);
    const ambiguous = matches.length > 1;
    if (match) {
      updateTabContextMenuItem("attach", {
        id: "attach",
        label: ambiguous ? copyText("tabMenu.attach.dupes", { name: match.name, others: matches.length - 1 }) : `Attach（tmux: ${match.name}）`,
        onClick: () => {
          if (viaCwd) warnCwdFallbackAttach();
          if (ambiguous) {
            showActionFailureToast(
              copyText("tabMenu.dupes.title"),
              copyText("tabMenu.dupes.body", { n: matches.length, name: match.name }),
              { level: "info", durationMs: 8000 },
            );
          }
          void runRemoteAttach(origin, match.name);
        },
      });
      // F60：预览项与 attach 同门(同一 tmux 会话),一并就绪——只读，不受"命中多个"影响。
      updateTabContextMenuItem("preview", {
        id: "preview",
        label: copyText("tabMenu.resolveAttachMenuItem.preview"),
        onClick: () => void openPanePreview(origin, match.name),
      });
      // F79：杀死会话——命中 ≥2 个时拒绝提供（破坏性操作，选错的代价不可逆，不像 attach 可撤销）。
      if (ambiguous) {
        updateTabContextMenuItem("kill", {
          id: "kill",
          label: copyText("tabMenu.kill.dupes", { n: matches.length }),
          danger: true,
          enabled: false,
          onClick: () => {},
        });
      } else {
        updateTabContextMenuItem("kill", {
          id: "kill",
          label: copyText("tabMenu.kill.plain"),
          danger: true,
          onClick: () => this.actions.killRemoteTmux(origin, match.name, viaCwd),
        });
      }
    } else {
      // audit-fixes F03.3（attach-into-idle）：无活 claude，但目标 sid 的**空 tmux**（@ccm_sid 命中、
      // command≠claude）还在 → 提供 attach 进那个空 shell（用户可在里面自己敲/看，或就地 resume）。
      // E73：明说不可 attach 的会话**不算 idle-tmux** —— 它那个「前台不是 claude」
    // 恰恰是因为里面跑着别的东西（SDK bridge 之类），不是空壳。
    const idle = this.host.isAttachable(sid) ? findIdleTmux(sessions, sid) : undefined;
      if (idle) {
        updateTabContextMenuItem("attach", {
          id: "attach",
          label: copyText("tabMenu.attach.idle", { name: idle.name }),
          onClick: () => void runRemoteAttach(origin, idle.name),
        });
        removeTabContextMenuItem("preview"); // 空 shell 无 claude 画面可预览
        // UX 审计 #1：灰态(idle-tmux)也给 kill——杀空 tmux → tab 转归档 → 可 Resume（给死角一个出口）。
        updateTabContextMenuItem("kill", {
          id: "kill",
          label: copyText("tabMenu.kill.idle", { name: idle.name }),
          danger: true,
          onClick: () => this.actions.killRemoteTmux(origin, idle.name, false, { idle: true }),
        });
      } else {
        removeTabContextMenuItem("attach");
        removeTabContextMenuItem("preview");
        removeTabContextMenuItem("kill");
      }
    }
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
    // 〔U4〕「给 Resume 还是给换号重启」按 `isResumeOnly` 分，与菜单主体那一格同一个谓词（原先是 `status === "archived"`）。
    const resumeOnly = isResumeOnly(state);
    // 〔`A3` 第二波〕本机已结束的 tab 不带账号选择（本机 Resume 走那条会话上次的号，
    // 见 `launch-account.ts::localLaunchAccountSync`）⇒ 本机只进下面「换号重启」那一支。
    if (origin === LOCAL_ORIGIN && resumeOnly) return;
    const gen = menuGeneration(); // 捕获这一代菜单
    const accountOptions = await enumerateAccountModifiers(origin);
    if (gen !== menuGeneration()) return; // 菜单已换/已关
    if (resumeOnly) {
      updateTabContextMenuItem("resume", {
        id: "resume",
        label: "Resume",
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
