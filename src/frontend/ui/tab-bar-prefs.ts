/**
 * tab 栏的三份落盘偏好：集合（分组）· 固定 · 顺序。
 *
 * 三份都是用户手写的真相（不是能重算的缓存），都写 `config.json` 的 `tabCollections` / `tabBar.*`，
 * 都先改内存再落盘；集合与固定还都有一道「这个实例拉过没有」的门（撕离出来的查看窗从不拉 ⇒ 不给入口、不落盘）。
 * 盘上那一层（读 / 写 / 清洗）住 `tab-collections.ts` 与 `tab-bar-state.ts`；顺序那份意图与落位运算住 `TabStore`。
 *
 * 分组：组表只存 `{id, name}`，组员关系是 tab 自己的属性（`Tab.group`，落盘 `tabBar.groupOf.<sid>`）。
 * 改组员的动作都在这里，每个动作先改内存、再把组表与那几个 tab 的组 id 键装进一次 `patchConfig`（`writeGroups`）。
 */
import {
  collectionRefusalText,
  collectionsEdit,
  createCollection,
  createRefusal,
  deleteCollection,
  getCollections,
  renameCollection,
  type CollectionRefusal,
  type TabCollection,
} from "./tab-collections";
import {
  getGroupOf,
  getPinned,
  getTabOrder,
  groupOfEdit,
  ORDER_CAP,
  isDegradedPin,
  setPinned,
  setTabOrder,
  type PinnedTab,
} from "./tab-bar-state";
import { patchConfig, type ConfigEdit } from "./config";
import type { Tab } from "./tab-model";
import { ENDED, UNSEEN, isLive } from "./tab-session-state";
import { copyText } from "./copy-table";
import { toast, failToast, silentUndo, undoToast } from "./kit/toast";
import type { TabStore } from "./tab-store";
import type { Origin } from "./ipc/origin";

/** 集合到上界、这一下没做成 ⇒ 说一句（判定 `tab-collections.ts::createRefusal`）；右键菜单与拖放都经这一处说。 */
export function sayCollectionRefusal(r: CollectionRefusal): void {
  const { title, body } = collectionRefusalText(r);
  toast(title, body, { level: "info" });
}

/** 落盘偏好要宿主做的三件事。 */
export interface TabBarPrefsHost {
  refreshTabBar(): void;
  /** 复活固定 tab 用（与活跃清单一到就建的骨架 tab 是同一条路）。 */
  createSkeletonTab(
    sessionId: string,
    projectDir: string | null,
    origin: Origin,
    background: boolean,
    name: string | null,
  ): void;
  /** 复活出来的固定 tab 空态里那颗「Resume 这个会话」（与右键菜单的 Resume 同一个动作）。 */
  resumeTab(sessionId: string): Promise<void>;
}

/** 撤销用的那一刻：顺序 · 组表 · 每个 tab 的组。 */
export interface GroupSnapshot {
  order: string[];
  groups: TabCollection[];
  groupOf: Map<string, string | null>;
}

export class TabBarPrefs {
  /** 标签页集合（组表），只有手动建、零自动归组。每一项只有 `{id, name}`；谁在组里看 `Tab.group`。 */
  collections: TabCollection[] = [];
  /**
   * 盘上 `tabBar.groupOf` 里 tab 还没到的那些组 id（sid → 组 id）：一份意图，形同 `TabStore.savedOrder`。
   * 不在读的那一拍按存活过滤：启动那一刻「已经没了的会话」与「还没宣告到的会话」分不开。
   * tab 到达（[`adoptGroup`]）⇒ 组 id 挪到 `Tab.group`；组没了 ⇒ 指向它的条目连同盘上一起摘。从没到过的 tab 的条目留着。
   */
  savedGroupOf = new Map<string, string>();
  /**
   * 这个实例拉过集合没有。撕离出来的查看窗也用 `TabManager` 却从不 `loadCollections`（`collections` 恒空）：
   * 留着「新建集合…」的话，点一下就把只含这一个的列表写回 `config.json`，用户已有的集合全没了 ⇒ 没拉过就不给入口。
   */
  collectionsLoaded = false;
  /**
   * 这个实例拉过固定表没有。`persistPinned` 按当前 tab 重算整张表写回，没拉过就写 ⇒ 上次固定的全没了
   * ⇒ 没拉过就不给入口（右键菜单里那两项不出现）、也不落盘。
   */
  pinnedLoaded = false;
  /**
   * `loadPinned` 从盘上读到的记录（sid → 条目）。不是「谁被固定了」的真相（那只在 `Tab.pinned`），只两个用途：
   * 判降级（`jsonlPath` 为空的点进去要说人话）；沿用 `lastActiveAt`（灰了的 tab 什么时候最后活动，前端没有这个数，
   * 不许每次落盘刷成 `Date.now()`）。
   */
  pinnedRecords = new Map<string, PinnedTab>();
  /**
   * 复活出来的固定 tab 的空态提示（sid → 元素）。复活的 tab 里一条内容都没有（已结束的会话只能 resume），
   * 不放点东西进去，点它就是一片空白、与坏了没有区别。复活成活的（真 resume 上了）时摘掉。
   */
  private readonly pinHintEls = new Map<string, HTMLElement>();

  constructor(
    private readonly store: TabStore,
    private readonly host: TabBarPrefsHost,
  ) {}

  /**
   * 从 `config.json` 拉一次组表与每个 tab 的组 id 并重画（宿主启动时调一次）。指向组表里没有的组的条目丢；
   * 其余存成意图，并立刻对已经在栏里的 tab 补一次归位（固定复活的骨架可能先到）。
   */
  async loadCollections(): Promise<void> {
    const [table, groupOf] = await Promise.all([getCollections(), getGroupOf()]);
    this.collections = table;
    const ids = new Set(table.map((c) => c.id));
    this.savedGroupOf = new Map([...groupOf].filter(([, gid]) => ids.has(gid)));
    this.collectionsLoaded = true;
    for (const t of this.store.tabs.values()) this.adoptGroup(t);
    this.host.refreshTabBar();
  }

  /** 一个 tab 到了（`ensureTab` 建出它 / `loadCollections` 读完）：意图里有它 ⇒ 组 id 挪到 `tab.group`（挪，不是抄）。 */
  adoptGroup(tab: Tab): void {
    const gid = this.savedGroupOf.get(tab.sessionId);
    if (gid === undefined) return;
    this.savedGroupOf.delete(tab.sessionId);
    if (tab.group === null && this.collections.some((c) => c.id === gid)) tab.group = gid;
  }

  /** `sid` 所在的组（`null` = 散 tab / 没这个 tab）。 */
  groupOf(sid: string): TabCollection | null {
    const gid = this.store.tabs.get(sid)?.group ?? null;
    return gid === null ? null : (this.collections.find((c) => c.id === gid) ?? null);
  }

  /**
   * 右键「新建集合…」/ 拖放压在一个散 tab 上：建一个组、把 `sids` 放进去。组数到上界 ⇒ 回拒绝原因、什么都不做（调用方出声）；
   * 名字空 ⇒ 什么都不做。组表 ＋ 每个 tab 的组 id 键一次写。
   */
  foundGroup(sids: readonly string[], name: string, id: string): CollectionRefusal | null {
    const full = createRefusal(this.collections);
    if (full) return full;
    const next = createCollection(this.collections, name, id);
    if (next.length === this.collections.length) return null;
    this.collections = next;
    const edits: ConfigEdit[] = [collectionsEdit(next)];
    const left = new Set<string | null>();
    for (const sid of sids) {
      const t = this.store.tabs.get(sid);
      if (!t) continue;
      left.add(t.group);
      t.group = id;
      edits.push(groupOfEdit(sid, id));
    }
    for (const old of left) edits.push(...this.dropIfEmpty(old));
    void this.writeGroups(edits);
    return null;
  }

  /** 此刻的分组与顺序（撤销用）：顺序 · 组表 · 栏里每个 tab 的组。 */
  snapshot(): GroupSnapshot {
    const groupOf = new Map<string, string | null>();
    for (const t of this.store.tabs.values()) groupOf.set(t.sessionId, t.group);
    return { order: [...this.store.orderedIds], groups: this.collections.map((c) => ({ ...c })), groupOf };
  }

  /**
   * 撤销（拖着建组 / 进出组 / 排序 · 右键移出 · 解散）：顺序、组、组员原样退回（组的位置由组员位置决定，跟着回去）。
   * 期间改过的组名留着；期间新建、此刻还有组员的组留着；已不在栏里的 tab 不管；期间新到的 tab 排在后面。
   */
  restore(snap: GroupSnapshot): void {
    const edits: ConfigEdit[] = [];
    for (const [sid, gid] of snap.groupOf) {
      const t = this.store.tabs.get(sid);
      if (!t || t.group === gid) continue;
      t.group = gid;
      edits.push(groupOfEdit(sid, gid));
    }
    const now = new Map(this.collections.map((c) => [c.id, c]));
    const had = new Set(snap.groups.map((c) => c.id));
    const used = new Set([...this.store.tabs.values()].map((t) => t.group));
    const next = [...snap.groups.map((c) => now.get(c.id) ?? c), ...this.collections.filter((c) => !had.has(c.id) && used.has(c.id))];
    if (next.length !== this.collections.length || next.some((c, i) => c !== this.collections[i])) {
      this.collections = next;
      edits.push(collectionsEdit(next));
    }
    const back = new Set(snap.order);
    const order = [...snap.order.filter((s) => this.store.tabs.has(s)), ...this.store.orderedIds.filter((s) => !back.has(s))];
    const moved = order.some((s, i) => s !== this.store.orderedIds[i]);
    this.store.orderedIds = order;
    void this.writeGroups(edits);
    if (moved) void this.persistOrder();
    this.host.refreshTabBar();
  }

  /** 做完一步分组 / 顺序改动之后：改了分组 ⇒ 可撤的 toast（`title`）；只排了顺序（`null`）⇒ 不出条，`Ctrl+Z` 照样撤。 */
  offerUndo(title: string | null, before: GroupSnapshot): void {
    const undo = (): void => this.restore(before);
    if (title === null) silentUndo(undo);
    else undoToast(title, undo, () => {});
  }

  /** 右键「加入集合 › X」/ 拖放进组：`sid` 进组 `gid`（原来在别的组 ⇒ 就不在了：`group` 是单值）。 */
  joinGroup(sid: string, gid: string): Promise<void> {
    const t = this.store.tabs.get(sid);
    if (!t || t.group === gid || !this.collections.some((c) => c.id === gid)) return Promise.resolve();
    const old = t.group;
    t.group = gid;
    return this.writeGroups([groupOfEdit(sid, gid), ...this.dropIfEmpty(old)]);
  }

  /** 批量「加入集合 › X」：每一个同 [`joinGroup`]，补丁一次写。 */
  joinGroupMany(sids: readonly string[], gid: string): Promise<void> {
    if (!this.collections.some((c) => c.id === gid)) return Promise.resolve();
    const left = new Set<string | null>();
    const edits: ConfigEdit[] = [];
    for (const sid of sids) {
      const t = this.store.tabs.get(sid);
      if (!t || t.group === gid) continue;
      left.add(t.group);
      t.group = gid;
      edits.push(groupOfEdit(sid, gid));
    }
    for (const old of left) edits.push(...this.dropIfEmpty(old));
    return this.writeGroups(edits);
  }

  /** 批量「移出集合」：每一个同 [`leaveGroup`]，补丁一次写。 */
  leaveGroupMany(sids: readonly string[]): Promise<void> {
    const left = new Set<string>();
    const edits: ConfigEdit[] = [];
    for (const sid of sids) {
      const t = this.store.tabs.get(sid);
      if (!t || t.group === null) continue;
      left.add(t.group);
      t.group = null;
      edits.push(groupOfEdit(sid, null));
    }
    for (const old of left) edits.push(...this.dropIfEmpty(old));
    return this.writeGroups(edits);
  }

  /** 右键「移出」/ 拖出组：`sid` 回到散 tab；组里因此一个在栏里的都不剩 ⇒ 组没（与 × 同一判定）。 */
  leaveGroup(sid: string): Promise<void> {
    const t = this.store.tabs.get(sid);
    if (!t || t.group === null) return Promise.resolve();
    const old = t.group;
    t.group = null;
    return this.writeGroups([groupOfEdit(sid, null), ...this.dropIfEmpty(old)]);
  }

  /**
   * × 那一刻：tab 的组关系随 tab 一起没。在 `closeTab` 把 tab 从 `store.tabs` 摘掉之后调（「组里还剩谁」只数真在栏里的）。
   * 摘它那一键；它是组里最后一个在栏里的 ⇒ 组也没；本来不在组里 ⇒ 零写。
   */
  forgetTab(tab: Tab): Promise<void> {
    const gid = tab.group;
    if (gid === null) return Promise.resolve();
    tab.group = null;
    return this.writeGroups([groupOfEdit(tab.sessionId, null), ...this.dropIfEmpty(gid)]);
  }

  /** 各台都报完了活会话清单：意图里还没到的组员不会再来了 ⇒ 按 × 掉处理；组里一个在栏里的都不剩 ⇒ 组没（重启后不留空组头）。 */
  forgetUnarrived(): Promise<void> {
    if (!this.collectionsLoaded) return Promise.resolve();
    const edits: ConfigEdit[] = [];
    for (const sid of this.savedGroupOf.keys()) edits.push(groupOfEdit(sid, null));
    this.savedGroupOf.clear();
    for (const col of [...this.collections]) edits.push(...this.dropIfEmpty(col.id));
    return this.writeGroups(edits);
  }

  /** 组头 ×：解散。只去掉分组，一个 tab 都不动（集合是视图，不是容器）。 */
  dissolveGroup(gid: string): Promise<void> {
    const edits: ConfigEdit[] = [];
    for (const t of this.store.tabs.values()) {
      if (t.group !== gid) continue;
      t.group = null;
      edits.push(groupOfEdit(t.sessionId, null));
    }
    edits.push(...this.dropGroup(gid));
    return this.writeGroups(edits);
  }

  /** 组头就地改名。空名 / 与现名相同 ⇒ 零写。 */
  renameGroup(gid: string, name: string): Promise<void> {
    const next = renameCollection(this.collections, gid, name);
    const before = this.collections.find((c) => c.id === gid)?.name;
    const after = next.find((c) => c.id === gid)?.name;
    if (before === undefined || before === after) return Promise.resolve();
    this.collections = next;
    return this.writeGroups([collectionsEdit(next)]);
  }

  /** 组 `gid` 在栏里一个 tab 都不带了 ⇒ 组没（× · 拖出 · 移出 · 挪组共用）。回要落盘的补丁（内存已改）；还有成员 ⇒ 空。 */
  private dropIfEmpty(gid: string | null): ConfigEdit[] {
    if (gid === null) return [];
    for (const t of this.store.tabs.values()) if (t.group === gid) return [];
    return this.dropGroup(gid);
  }

  /** 组表去掉 `gid`，指向它的意图一并摘 —— 回要落盘的那几条（内存已改）。 */
  private dropGroup(gid: string): ConfigEdit[] {
    this.collections = deleteCollection(this.collections, gid);
    const edits: ConfigEdit[] = [collectionsEdit(this.collections)];
    for (const [sid, g] of this.savedGroupOf) {
      if (g !== gid) continue;
      this.savedGroupOf.delete(sid);
      edits.push(groupOfEdit(sid, null));
    }
    return edits;
  }

  /** 分组落盘的唯一出口：这一次的全部补丁一次 `patchConfig`（不会只落一半）。内存先改；落盘失败弹 toast ＋ 记日志。 */
  private async writeGroups(edits: readonly ConfigEdit[]): Promise<void> {
    if (edits.length === 0) return;
    try {
      await patchConfig(edits);
    } catch (e) {
      console.warn("[tab-collections] 落盘失败:", e);
      failToast(copyText("tabBar.persist.collectionsFailed"), e);
    }
  }

  // ===== 固定（pinned）=====

  /**
   * 启动时把固定的 tab 复活出来（宿主在 `loadCollections` 之后调一次；与 `loadOrder` 谁先谁后都排得回来，
   * 复活的 tab 走 `placeInOrder` 时会再应用一次顺序意图）。逐条 `createSkeletonTab`，标 pinned、标题用存下来的那份；
   * 状态：那台还没报完清单 ⇒ 说不清，报完了 ⇒ 已结束（活着的话事件流会改回活）。不读内容（已结束的会话只能 resume）。
   * 已经存在的 sid 不重建：只补 `pinned = true`，状态一个字不碰（它真活着时按回已结束是假话）。
   */
  async loadPinned(): Promise<void> {
    const list = await getPinned();
    this.pinnedRecords = new Map(list.map((p) => [p.sid, p]));
    for (const p of list) {
      const existed = this.store.tabs.get(p.sid);
      if (!existed) {
        this.host.createSkeletonTab(p.sid, p.cwd, p.origin, p.background, p.name);
        const t = this.store.tabs.get(p.sid);
        if (!t) continue;
        // 凭空造的 tab ⇒ 直接置位（不走 `archiveTab`）。那台还没报完清单 ⇒ 说不清（不许显示成已结束）；
        // 报完了 ⇒ 已结束 —— 它若活着，清单里就有它、tab 早建成活的了（上面 `existed` 那支）。
        t.state = this.store.seenOrigins.has(p.origin) ? ENDED : UNSEEN;
        t.activity = null;
        t.parentPath = p.jsonlPath; // resume 与「有没有记录」都靠它
        t.title = p.title; // 骨架期就显示正确标题，不等读文件
        t.pinned = true;
        this.mountPinHint(t);
      } else {
        existed.pinned = true;
      }
    }
    this.pinnedLoaded = true;
    this.host.refreshTabBar();
  }

  /** 复活出来的固定 tab 的空态：说清它是什么 ＋ 给出唯一的出口（resume）；没有留下记录的那种说「没有记录」。 */
  mountPinHint(tab: Tab): void {
    const sid = tab.sessionId;
    const degraded = this.pinIsDegraded(sid);
    const box = document.createElement("div");
    box.className = "pin-revived-hint";
    const head = document.createElement("strong");
    // 说到会话状态的字住文案表 `sessionState.*`（与 tab 的状态名同一处）。
    head.textContent = degraded
      ? copyText("sessionState.pinnedEmpty.head")
      : copyText("sessionState.pinned.head");
    const body = document.createElement("p");
    body.textContent = degraded
      ? copyText("sessionState.pinnedEmpty.body")
      : copyText("sessionState.pinned.body");
    box.append(head, body);
    if (!degraded) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "pin-revived-hint-btn";
      btn.textContent = copyText("tabBarPrefs.pin.resume");
      // 与右键菜单的「Resume」同一个动作，入口放在用户正看着的地方。
      btn.addEventListener("click", () => void this.host.resumeTab(sid));
      box.appendChild(btn);
    }
    tab.streamEl.appendChild(box);
    this.pinHintEls.set(sid, box);
  }

  /** 复活成 live（resume 真的接上了）或 tab 关掉时，摘掉那块空态提示。 */
  clearPinHint(sid: string): void {
    this.pinHintEls.get(sid)?.remove();
    this.pinHintEls.delete(sid);
  }

  /**
   * 把一个 tab 压成一条落盘记录。`lastActiveAt`：活的 ⇒ 此刻；死了（已结束 / 可重连）⇒ 沿用盘上那份、没有就 `null`
   * （刷成此刻会把「最后活动」写成「最后落盘」）。
   */
  private pinRecordFor(tab: Tab): PinnedTab {
    const prev = this.pinnedRecords.get(tab.sessionId);
    return {
      sid: tab.sessionId,
      jsonlPath: tab.parentPath,
      cwd: tab.projectDir,
      origin: tab.origin,
      // 缺了 resume 会静默落到默认号：两个源都读不到 ⇒ `null`（没记到，不是默认号）。
      account:
        this.store.sessionAccountsByS.get(tab.sessionId)?.account ??
        this.store.accountLastByS.get(tab.sessionId) ??
        prev?.account ??
        null,
      // 按活性一轴判：可重连的 claude 已经没了 ⇒ 不是「此刻还活着」。
      lastActiveAt: isLive(tab.state) ? Date.now() : prev?.lastActiveAt ?? null,
      background: tab.background,
      name: tab.bgName,
      title: tab.title,
    };
  }

  /**
   * 把「现在哪些 tab 被固定了」整张表写回 `tabBar.pinned`（源头是 `Tab.pinned`，这里只压平）。
   * 「没拉过就不写」那道门在 `togglePin`：这里的两个调用方都已被它或 `tab.pinned` 挡在前面，再放一道区分不出任何输入。
   */
  async persistPinned(): Promise<void> {
    const next: PinnedTab[] = [];
    for (const sid of this.store.orderedIds) {
      const tab = this.store.tabs.get(sid);
      if (tab?.pinned) next.push(this.pinRecordFor(tab));
    }
    this.pinnedRecords = new Map(next.map((p) => [p.sid, p]));
    try {
      await setPinned(next);
    } catch (e) {
      console.warn("[tab-bar] 固定落盘失败:", e);
      failToast(copyText("tabBar.persist.pinnedFailed"), e); // 同 `persistCollections`
    }
  }

  /** 右键菜单那一项：翻转固定，先改内存再落盘。不自动固定，这是唯一的入口。 */
  togglePin(sid: string): void {
    const tab = this.store.tabs.get(sid);
    if (!tab || !this.pinnedLoaded) return;
    tab.pinned = !tab.pinned;
    this.host.refreshTabBar();
    void this.persistPinned();
  }

  /** 批量「固定 / 取消固定」：每一个同 [`togglePin`] 翻到 `on`，落盘一次。 */
  setPinnedMany(sids: readonly string[], on: boolean): void {
    if (!this.pinnedLoaded) return;
    let changed = false;
    for (const sid of sids) {
      const tab = this.store.tabs.get(sid);
      if (!tab || tab.pinned === on) continue;
      tab.pinned = on;
      changed = true;
    }
    if (!changed) return;
    this.host.refreshTabBar();
    void this.persistPinned();
  }

  /**
   * 这条固定记录点进去也没有东西可看：盘上那条 `jsonlPath` 为空（骨架 tab 没收到带路径的行就被固定了），
   * 且到现在也没有行回填过 `parentPath`。点它时要说人话。
   */
  private pinIsDegraded(sid: string): boolean {
    const rec = this.pinnedRecords.get(sid);
    if (!rec || !isDegradedPin(rec)) return false;
    return (this.store.tabs.get(sid)?.parentPath ?? "") === "";
  }

  /** 把当前顺序写进 `config.json` 的 `tabBar.order`。失败出声（toast）＋ 记日志，不打断交互。 */
  async persistOrder(): Promise<void> {
    // 先同步内存里那份意图再写盘（`await` 之前，落盘失败也照样同步）：不同步的话旧顺序每来一个新 tab 就再应用一次，
    // 后到的一个 tab 能把用户刚拖的一下整张撤销。
    this.store.savedOrder = this.store.mergedOrder(ORDER_CAP);
    try {
      await setTabOrder(this.store.savedOrder);
    } catch (e) {
      console.warn("[tab-bar] 顺序落盘失败:", e);
      failToast(copyText("tabBar.persist.orderFailed"), e); // 同 `persistCollections`
    }
  }

  /**
   * 启动时把落盘的顺序拉回来（宿主在 `loadCollections` 之后调一次）。只重排已经存在的 tab、不凭空造；
   * 盘上没提到的排在后面、保持此刻的相对次序。
   * 读的时候不按存活过滤：前端没有任何一刻知道「会话到齐了」（几台远端陆续宣告、还会中途掉线重连），
   * 读的那一拍「已删的」与「还没到的」分不开 ⇒ 顺序存成意图（`savedOrder`），每次应用时按此刻在的 tab 过滤
   * （`applySavedOrder`），tab 陆续到达时由 `placeInOrder` 再应用一次。
   */
  async loadOrder(): Promise<void> {
    // `null` ＝ 这一趟不按存活过滤（见上）。
    this.store.savedOrder = await getTabOrder(null);
    if (this.store.applySavedOrder()) this.host.refreshTabBar();
  }
}
