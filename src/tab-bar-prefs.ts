/**
 * 〔U2 · 拆 `tabs.ts` ④〕**tab 栏的三份落盘偏好**：集合（分组）· 固定 · 顺序。
 *
 * 三份都是「用户手写的真相，不是能重算的缓存」（`tab-collections.ts` 立集合落盘的那条理由），
 * 都写 `config.json` 的 `tabCollections` / `tabBar.*`，都守同一条纪律：**先改内存再落盘，落盘失败只记日志**；
 * 集合与固定还都有一道「这个实例拉过没有」的门（撕离出来的 viewer 窗口从不拉 ⇒ 不给入口、不落盘）。
 * 盘上那一层（读 / 写 / 清洗）住 `tab-collections.ts` 与 `tab-bar-state.ts`，这里是它们与 tab 集合之间的那一层。
 *
 * 字段与方法逐字从 `tabs.ts` 搬来，唯一的改写：刷 tab 栏 · 建骨架 tab · resume 三样换成 `this.host.…`。
 * 顺序那份「意图」本身（`savedOrder`）与落位运算住 `TabStore`，这里只管读盘 / 落盘。
 *
 * 〔GRP1 · `设计/99 §1` V140〕分组这一份改了形：组表只存 `{id, name}`，组员关系是 tab 自己的属性（`Tab.group`，
 * 落盘 `tabBar.groupOf.<sid>`）。改组员的全部动作（建组 · 进组 · 出组 · × · 解散 · 改名）住这里，
 * 每个动作「先改内存、再把组表与那几个 tab 的组 id 键装进**一次** `patchConfig`」（`writeGroups`）。
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
  isDegradedPin,
  setPinned,
  setTabOrder,
  type PinnedTab,
} from "./tab-bar-state";
import { patchConfig, type ConfigEdit } from "./config";
import type { Tab } from "./tab-model";
import { ENDED, UNSEEN, isLive } from "./tab-session-state";
import { copyText } from "./copy-table";
import { showActionFailureToast } from "./error-toast";
import type { TabStore } from "./tab-store";
import type { Origin } from "./ipc/origin";

/**
 * 〔TL2 · E13〕集合到上界、这一下没做成 ⇒ 说一句（`设计/01 §5 D4`「一条都不许静默忽略」）。
 * 判定住 `tab-collections.ts::createRefusal`（经 [`TabBarPrefs.foundGroup`] 回给调用方），句子住文案表；
 * 两个入口（右键菜单 · 拖放）都经这一处说。
 */
export function sayCollectionRefusal(r: CollectionRefusal): void {
  const { title, body } = collectionRefusalText(r);
  showActionFailureToast(title, body, { level: "info", durationMs: 6000 });
}

/** 落盘偏好要宿主做的三件事。 */
export interface TabBarPrefsHost {
  refreshTabBar(): void;
  /** 复活固定 tab 用（与活跃清单一到就建的骨架 tab 是同一条路）。 */
  createSkeletonTab(
    sessionId: string,
    cwd: string | null,
    origin: Origin,
    kind: string | null,
    name: string | null,
  ): void;
  /** 复活出来的固定 tab 空态里那颗「Resume 这个会话」（与右键菜单的 Resume 同一个动作）。 */
  resumeTab(sessionId: string): Promise<void>;
}

export class TabBarPrefs {
  /**
   * P7a-3（#61）：标签页集合（组表）。**零自动归组**〔用 08-11「纯手动」〕。
   * 〔GRP1 · V140〕每一项只有 `{id, name}`；谁在组里看 `Tab.group`。
   */
  collections: TabCollection[] = [];
  /**
   * 〔GRP1〕盘上 `tabBar.groupOf` 里、**tab 还没到**的那些组 id（sid → 组 id）—— 一份**意图**，形同 `TabStore.savedOrder`。
   *
   * 为什么不在读的那一拍按存活过滤：`设计/30 §C.3` 同一条理由 —— 读发生在启动那一刻，
   * 「已经没了的会话」与「还没宣告到的会话」长得一模一样。
   * tab 到达（[`adoptGroup`]）⇒ 它的组 id 从这里**挪到** `Tab.group`（此后真相只在 tab 身上一处）；
   * 组没了（× 掉最后一个 · 解散）⇒ 指向它的条目从这里连同盘上一起摘。
   * ⚠ 从没到过的 tab 的条目**留着**（盘上也留着）—— 主会话裁 Q2 取乙（真关窗时摘），但 monitor 主窗今天没有关窗钩子，
   *   加一个就改了退出次序 ⇒ 按裁定退回丙（维持），`GRP1.md` 记着。
   */
  savedGroupOf = new Map<string, string>();
  /**
   * P7a-3 E 阶段补审：**这个实例拉过集合没有。**
   *
   * 撕离出来的 viewer 窗口也用 `TabManager`（`main.ts:938`，tab 栏由 `.viewer-mode` 隐藏），
   * 但它**从不 `loadCollections`** ⇒ `collections` 恒空。右键菜单里若还留着「新建集合…」，
   * 点一下就把「只含这一个」的列表写回 `config.json` —— **用户已有的集合全没了**。
   *
   * 同族先例就在旁边一行：「viewer 窗口共享 localStorage，**禁写 last-active**（防污染主窗口记忆）」。
   * ⇒ 没拉过就不给入口。这不是把功能藏起来，是**没有那份真相就没有资格改它**。
   */
  collectionsLoaded = false;
  /**
   * 〔步 17·B〕**这个实例拉过固定表没有** —— 与 `collectionsLoaded` 同一条理由，
   * 而且这里更要命：`persistPinned` 是**按当前 tab 重算整张表**写回去的，
   * 没拉过就写 ⇒ 用户上次固定的全没了。撕离出来的 viewer 窗口正是这种实例。
   * ⇒ 没拉过就不给入口（右键菜单里那两项不出现），也不落盘。
   */
  pinnedLoaded = false;
  /**
   * 〔步 17·B〕`loadPinned` 那一趟从盘上读到的记录（sid → 条目）。
   *
   * 🔴 **它不是「谁被固定了」的真相** —— 那件事的唯一住址是 `Tab.pinned`
   * （一个事实一个住址）。这里存的是**盘上那份记录的内容**，只有两个用途：
   * ① `§B.6` 第一格的降级判定（`jsonlPath` 为空的那些，点进去要说人话）；
   * ② `lastActiveAt` 的沿用 —— 已经灰了的 tab 什么时候最后活动过，前端没有这个数，
   *    不许在每次落盘时把它刷成 `Date.now()`（那是把「说不清」写成一句假话）。
   */
  pinnedRecords = new Map<string, PinnedTab>();
  /**
   * 〔步 17·B〕复活出来的固定 tab 的**空态提示**（sid → 元素）。
   *
   * 🔴 它是「**不许留一个点了没反应的 tab**」这条纪律的落点：复活的 tab 里
   * 一条内容都没有（`99 §2.5 P3` 裁定「已结束的会话点进去不能看内容，只能 resume」），
   * 不放点东西进去，用户点它就是一片空白 —— 那与坏了没有区别。
   * 复活成 live（真 resume 上了）时摘掉。
   */
  private readonly pinHintEls = new Map<string, HTMLElement>();

  constructor(
    private readonly store: TabStore,
    private readonly host: TabBarPrefsHost,
  ) {}

  /**
   * P7a-3：从 `config.json` 拉一次组表与每个 tab 的组 id，并重画。宿主启动时调一次。
   *
   * 〔GRP1〕`tabBar.groupOf` 里指向**组表里没有的组**的条目丢（认不出就丢）；其余存成意图（`savedGroupOf`），
   * 并立刻对**已经在栏里**的 tab 补一次归位 —— 固定复活的骨架（`loadPinned`，与本函数并排跑）可能先到。
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

  /**
   * 〔GRP1〕一个 tab **到了**（`TabManager.ensureTab` 建出它的那一刻 / `loadCollections` 读完时）：
   * 意图里有它 ⇒ 组 id 从意图**挪到** `tab.group`（挪，不是抄：此后这个事实只住 tab 身上）。
   * 没拉过组表（viewer 窗口）⇒ 意图恒空 ⇒ 什么都不做。
   */
  adoptGroup(tab: Tab): void {
    const gid = this.savedGroupOf.get(tab.sessionId);
    if (gid === undefined) return;
    this.savedGroupOf.delete(tab.sessionId);
    if (tab.group === null && this.collections.some((c) => c.id === gid)) tab.group = gid;
  }

  /** 〔GRP1〕`sid` 所在的组（`null` = 散 tab / 没这个 tab）。 */
  groupOf(sid: string): TabCollection | null {
    const gid = this.store.tabs.get(sid)?.group ?? null;
    return gid === null ? null : (this.collections.find((c) => c.id === gid) ?? null);
  }

  /**
   * 〔GRP1〕右键「新建集合…」/ 拖放「压在一个散 tab 上」：建一个组，把 `sids` 这几个 tab 放进去。
   * 组数到上界 ⇒ 回拒绝原因、**什么都不做**（调用方出声）；名字空 ⇒ 什么都不做（空名不是一个集合）。
   * 盘：组表 ＋ 每个 tab 的组 id 键，**一次**写。
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

  /** 〔GRP1〕右键「加入集合 › X」/ 拖放进组：`sid` 进组 `gid`（原来在别的组 ⇒ 就不在了：`group` 是单值）。 */
  joinGroup(sid: string, gid: string): Promise<void> {
    const t = this.store.tabs.get(sid);
    if (!t || t.group === gid || !this.collections.some((c) => c.id === gid)) return Promise.resolve();
    const old = t.group;
    t.group = gid;
    return this.writeGroups([groupOfEdit(sid, gid), ...this.dropIfEmpty(old)]);
  }

  /** 〔GRP1〕右键「移出」/ 拖出组：`sid` 回到散 tab；组里因此一个在栏里的都不剩 ⇒ 组没（主会话裁 Q1，与 × 同一判定）。 */
  leaveGroup(sid: string): Promise<void> {
    const t = this.store.tabs.get(sid);
    if (!t || t.group === null) return Promise.resolve();
    const old = t.group;
    t.group = null;
    return this.writeGroups([groupOfEdit(sid, null), ...this.dropIfEmpty(old)]);
  }

  /**
   * 🔴 〔GRP1 · V140「x就是没了, 不存在还要移出分组」〕**× 那一刻**：tab 的组关系随 tab 一起没。
   *
   * 调用时机：`closeTab` 已把 tab 从 `store.tabs` 摘掉之后（「组里还剩谁」只数真在栏里的）。
   * - 盘：摘它自己那一键 `tabBar.groupOf.<sid>`；
   * - 它是组里**最后一个在栏里的** tab ⇒ 组也没（V140「组里最后一个 tab 没了组自己消失」）：组表去掉它 ＋ 指向它的意图一并摘；
   * - 它本来不在组里 ⇒ **零写**（没分组的 tab 关一下不该顺手改 `config.json`，与 `closeTab` 摘固定同一条理由）。
   */
  forgetTab(tab: Tab): Promise<void> {
    const gid = tab.group;
    if (gid === null) return Promise.resolve();
    tab.group = null;
    return this.writeGroups([groupOfEdit(tab.sessionId, null), ...this.dropIfEmpty(gid)]);
  }

  /** 〔GRP1〕组头 ×：解散。**只去掉分组，一个 tab 都不动**（集合是个视图，不是容器）。 */
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

  /** 〔GRP1〕组头就地改名。空名 / 与现名相同 ⇒ 零写。 */
  renameGroup(gid: string, name: string): Promise<void> {
    const next = renameCollection(this.collections, gid, name);
    const before = this.collections.find((c) => c.id === gid)?.name;
    const after = next.find((c) => c.id === gid)?.name;
    if (before === undefined || before === after) return Promise.resolve();
    this.collections = next;
    return this.writeGroups([collectionsEdit(next)]);
  }

  /**
   * 〔V140「组里最后一个 tab 没了组自己消失」〕组 `gid` 在栏里一个 tab 都不带了 ⇒ 组没（× · 拖出 · 移出 · 挪组共用这一判定）。
   * 回要落盘的补丁（内存已改）；`null` / 还有成员 ⇒ 空。
   */
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

  /**
   * 分组落盘的唯一出口：这一次改动的全部补丁**一次** `patchConfig`（Rust 一把锁里一起落，不会只落一半）。
   * 〔CFG1 · 4D〕从前落盘失败只记日志（`设计/30 §C.3` 原话）——重启后分组没了、当时一句话都没有（E §3.3）。
   * 主会话 09-25 按 `INVARIANTS §12`（关键失败要出声）认可改成：内存照旧先改 · 落盘失败弹一条 toast · 日志照留。
   */
  private async writeGroups(edits: readonly ConfigEdit[]): Promise<void> {
    if (edits.length === 0) return;
    try {
      await patchConfig(edits);
    } catch (e) {
      console.warn("[tab-collections] 落盘失败:", e);
      showActionFailureToast(copyText("tabBar.persist.collectionsFailed"), String(e));
    }
  }

  // ===== 〔步 17·B · `设计/30 §B`〕固定（pinned）=====

  /**
   * 启动时把固定的 tab **复活**出来。宿主在 `loadCollections` 之后调一次。
   *
   * ⚠ **2026-09-21 订正**：这儿原先写着「必须在 `loadOrder` **之前**」，理由是
   *   「`loadOrder` 用 `getTabOrder(new Set(this.store.orderedIds))` 按今天真的存在的 sid 过滤，
   *   复活的 tab 得先存在，位置才排得回来」。**那个理由连着那道读时过滤一起没了**
   *   （`loadOrder` 头注记着为什么它是个 no-op）：顺序现在是一份**留着的意图**
   *   （`savedOrder`），复活出来的 tab 走 `createSkeletonTab` → `placeInOrder` 时
   *   会**再应用一次** ⇒ 两者谁先谁后都排得回来。
   * ⚠ `main.ts` 今天仍然是「pinned 先、order 后」那个次序 —— 不改它，但那**不再是承重的**。
   *
   * # 复活流程（`§B.5` 逐字）
   * ```
   * 读 tabBar.pinned[] → 逐条 createSkeletonTab(sid, cwd, origin, kind, name)
   *   ├ 标 pinned = true
   *   ├ 标 state = UNSEEN（说不清；那台报完清单 ⇒ 已结束 / 活，见 `TabManager.markOriginSeen`）
   *   │   〔U4b〕那台已经报完了 ⇒ 直接 ENDED（没有活进程；后端 replay 随后宣告它活着 ⇒ 事件流会改回活）
   *   └ 标题直接用存下来的那份（不等读文件）
   * ```
   * 🔴 **不读内容** —— `99 §2.5 P3` 已裁定「已结束的会话点进去不能看内容，只能 resume」。
   *   `replay_session_to_window` 那条路对已结束的会话本来就走不通（它的头注逐字：
   *   「仅活跃 session 的历史在 buffer 里」）。
   *
   * ⚠ **已经存在的 sid 不重建**（后端 replay 可能已经先宣告了它）—— 只补一个 `pinned = true`，
   *   `state` 一个字不碰：那条会话真活着的时候，把它按回已结束是一句假话。
   */
  async loadPinned(): Promise<void> {
    const list = await getPinned();
    this.pinnedRecords = new Map(list.map((p) => [p.sid, p]));
    for (const p of list) {
      const existed = this.store.tabs.get(p.sid);
      if (!existed) {
        this.host.createSkeletonTab(p.sid, p.cwd, p.origin, p.kind, p.name);
        const t = this.store.tabs.get(p.sid);
        if (!t) continue;
        // 没有活进程 ⇒ 灰着。`archiveTab` 那条路要求 tab 已在事件流里，这里是**凭空造**，
        // 所以直接置位；两者最终形态一致（`.tab.ended` 那条 CSS 本来就有；〔U4〕原名 `.tab.archived`）。
        // 〔U4b · 说不清〕那台机器还没把活会话清单报完 ⇒ **说不清**，不是已结束（`设计/30 §3.5.7a`：
        //   `Unseen` 不许被显示成已结束）；报完了（`markOriginSeen` 已经来过）⇒ 已结束 —— 它若活着，
        //   清单里就会有它、tab 早被建成活的了（上面那个 `existed` 分支）。
        t.state = this.store.seenOrigins.has(p.origin) ? ENDED : UNSEEN;
        t.activity = null;
        t.parentPath = p.jsonlPath; // `§B.5`：复活的必需品（resume 与「有没有记录」都靠它）
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

  /**
   * 复活出来的固定 tab 的空态：**说清它是什么 ＋ 给出那唯一的出口**。
   *
   * `§B.5` 复活流程最后一行逐字：「用户点进去那一刻，**出现 resume 入口**（🔴 不读内容）」。
   * `§B.6` 第一格：`jsonlPath` 为空的那种要提示「这个会话没有留下记录」——
   * **不要留一个点了没反应的 tab**。两种情形在这里分叉。
   */
  mountPinHint(tab: Tab): void {
    const sid = tab.sessionId;
    const degraded = this.pinIsDegraded(sid);
    const box = document.createElement("div");
    box.className = "pin-revived-hint";
    const head = document.createElement("strong");
    // 〔U4〕说到会话状态的字住文案表 `sessionState.*`（与 tab 的状态名 / 提示句同一处）。
    //   原句里的「jsonl 路径」「前端」是内部词（`设计/91 §4` R1），一并去掉。
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
      // 与右键菜单的「Resume」同一个动作、同一个住址 —— 这里只是把入口放在用户正看着的地方。
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
   * 把一个 tab 压成一条落盘记录。字段表逐条照 `§B.5`。
   *
   * ⚠ `lastActiveAt`：**live ⇒ 此刻**（「它现在还活着」是个真读数）；
   *   **死了（已结束 / 可重连）⇒ 沿用盘上那份，没有就 `null`** —— 前端 `Tab` 上零时间戳字段（现打），
   *   把它刷成 `Date.now()` 会让「最后活动时刻」变成「最后一次落盘时刻」，那是假话。
   */
  private pinRecordFor(tab: Tab): PinnedTab {
    const prev = this.pinnedRecords.get(tab.sessionId);
    return {
      sid: tab.sessionId,
      jsonlPath: tab.parentPath,
      cwd: tab.cwd,
      origin: tab.origin,
      // `§3.5.7`：缺了 resume 会静默落到默认号。两个源都读不到 ⇒ `null`＝没记到，不是默认号。
      account:
        this.store.sessionAccountsByS.get(tab.sessionId)?.account ??
        this.store.accountLastByS.get(tab.sessionId) ??
        prev?.account ??
        null,
      // 〔U4〕按活性一轴判：可重连的 claude 已经没了 ⇒ 不是「此刻还活着」（改两轴之前它借着 `status: live` 被刷成此刻）。
      lastActiveAt: isLive(tab.state) ? Date.now() : prev?.lastActiveAt ?? null,
      kind: tab.kind,
      name: tab.bgName,
      title: tab.title,
    };
  }

  /**
   * 把「现在哪些 tab 被固定了」整张表写回 `config.json` 的 `tabBar.pinned`。
   *
   * **真相源是 `Tab.pinned`**，这里只是把它压平 ⇒ 不会出现「内存说固定了、盘上没有」。
   *
   * 🔴 **「没 `loadPinned` 过就不写」那道门不在这里，在 `togglePin`** —— 这是死值验逼出来的：
   *   我原本在这里也放了一条 `if (!this.pinnedLoaded) return;`，**刀 9 实测它恒不承重**
   *   （去掉之后一格都不红）。原因是它没有任何可区分的输入：本函数只有两个调用方，
   *   `togglePin` 自己那道门已经挡在前面，而 `closeTab` 只在 `tab.pinned` 为真时才调，
   *   `tab.pinned` 又只能由 `loadPinned`（与 `pinnedLoaded = true` 同一个微任务）
   *   或 `togglePin` 置起来。
   * ⇒ 照 `sanitizeCollections` 那条逐字先例删掉：「留一道任何输入都区分不出的守卫，
   *   就是一条假绿的防线」。真正在承重的那道由死值验刀 10 钉着。
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
      showActionFailureToast(copyText("tabBar.persist.pinnedFailed"), String(e)); // 〔CFG1〕同 `persistCollections`
    }
  }

  /**
   * 右键菜单那一项：翻转固定。**先改内存再落盘**（照 `commitCollections` 的形状）。
   *
   * ⚠ 不做自动固定（`§B.7` 逐字「照 `tab-collections.ts` 那条『手动建，不要自动』的先例」）——
   *   这是唯一的入口。
   */
  togglePin(sid: string): void {
    const tab = this.store.tabs.get(sid);
    if (!tab || !this.pinnedLoaded) return;
    tab.pinned = !tab.pinned;
    this.host.refreshTabBar();
    void this.persistPinned();
  }

  /**
   * `§B.6` 第一格：这条固定记录**点进去也没有东西可看**（`jsonlPath` 为空 ——
   * 骨架 tab 从没收到过带路径的行就被固定了）。
   *
   * 🔴 用途是**不许留一个点了没反应的 tab**：点它的时候要说人话（见点击处的提示）。
   * ⚠ 两个条件都要：盘上那条是降级的 **且** 到现在也没有行回填过 `parentPath`
   *   （真来了行就不再降级 —— 那条 tab 已经有记录可读了）。
   */
  private pinIsDegraded(sid: string): boolean {
    const rec = this.pinnedRecords.get(sid);
    if (!rec || !isDegradedPin(rec)) return false;
    return (this.store.tabs.get(sid)?.parentPath ?? "") === "";
  }

  /** 把当前顺序写进 `config.json` 的 `tabBar.order`。失败出声（toast）＋ 记日志，不打断交互（〔CFG1〕）。 */
  async persistOrder(): Promise<void> {
    // 🔴 **先把内存里那份意图同步掉，再去写盘** —— 用户刚拖出来的这张就是最新的意图。
    //   不同步的话，`savedOrder` 还是启动时读到的那份**旧**顺序，而它每来一个新 tab
    //   就会被再应用一次（`placeInOrder`）⇒ **后到的一个 tab 能把用户刚拖的一下整张撤销**。
    //   放在 `await` 之前：落盘失败也照样同步 —— 内存里那张已经是用户看见的事实了。
    this.store.savedOrder = [...this.store.orderedIds];
    try {
      await setTabOrder(this.store.orderedIds);
    } catch (e) {
      console.warn("[tab-bar] 顺序落盘失败:", e);
      showActionFailureToast(copyText("tabBar.persist.orderFailed"), String(e)); // 〔CFG1〕同 `persistCollections`
    }
  }

  /**
   * 启动时把落盘的顺序拉回来。**宿主在 `loadCollections` 之后调一次。**
   *
   * ⚠ 它**只重排已经存在的 tab，不凭空造 tab**（`§C.3` 逐字）——
   *   盘上的顺序里会有已经不存在的 sid（上次那个会话被删了）。
   * ⚠ **盘上没提到的 tab 排在后面**，保持它们此刻的相对次序 ——
   *   否则「启动后新建的 tab」会被一份旧顺序挤到看不见的地方。
   *
   * # 🔴 2026-09-21：修掉「结构性 no-op」（`99 §4` 步 17 那行的 🟡）
   *
   * 在这之前它是这么写的：
   * ```ts
   * const saved = await getTabOrder(new Set(this.store.orderedIds));  // ← alive = 此刻的 tab 集
   * if (saved.length === 0) return;
   * ```
   * **它在唯一那个调用点上恒等于 no-op。** `main.ts` 里那一行是
   * `loadPinned().finally(() => loadOrder())` ⇒ 跑到这儿的时候会话**还没到**
   * （tab 由随后的 `session_added` / 首行陆续建出来，启动窗口期 30s）⇒
   * `orderedIds` 是空的（顶多只有几个复活出来的 pinned）⇒ `alive` 是空集 ⇒
   * `sanitizeOrder` 把盘上那张**整张**当成死 sid 摘掉 ⇒ 空表 ⇒ 上面那句直接 `return`。
   * 现打：`sanitizeOrder(["c","b","a"], new Set())` == `[]`，而 `alive` 传 `null`
   * 时原样是 `["c","b","a"]` —— **数据一直读得出来，是被自己那道过滤删掉的。**
   *
   * 🔴 **所以这不是「调用点排早了」，挪一挪就好** —— 前端**没有任何一刻**知道
   *   「会话到齐了」（它们从几台远端陆续宣告，还能中途掉线重连）。
   *   「已删的会话」与「还没到的会话」在任何单一时刻都**不可区分**
   *   ⇒ 只要过滤发生在读的那一拍，这个 bug 就还在。
   *
   * ⇒ 改法：把盘上那份顺序**留着**（`savedOrder`，一份意图，不是一次性的动作），
   *   读的时候**不按存活过滤**，过滤改在**每次应用**时按「此刻真的在的 tab」做
   *   （`applySavedOrder`）；tab 陆续到达时由 `placeInOrder` 再应用一次。
   */
  async loadOrder(): Promise<void> {
    // `null` = 这一趟不按存活过滤（理由见 `getTabOrder` 头注与上面那段）。
    this.store.savedOrder = await getTabOrder(null);
    if (this.store.applySavedOrder()) this.host.refreshTabBar();
  }
}
