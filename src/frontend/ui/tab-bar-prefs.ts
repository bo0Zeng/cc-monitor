/**
 * tab 栏的分组 · 固定 · 顺序。
 *
 * 组与固定是后端的数据（`history/groups.rs`）：组跟着组员走（每台只存自己有组员的那一片），组员与固定是会话注解（住会话那一台）。这里只照组的账（`group-book.ts`）
 * 读回来的那一份画：组表 · 每个 tab 在哪一组（`Tab.group`）· 固没固定（`Tab.pinned`）· 固定了却没有标签的会话复活成灰标签。
 * 改的动作先改内存（栏当场跟着动），再交给那一台；那台推 `changed {groups}`，账重读一次，读回来的就是真相。
 * 一条会话可在几组，栏里一个标签只画在一处：画在读回来的那一份里排在最前、有它的那一组；挪进另一组 ＝ 出原来那组、进新的。
 * 关掉标签不等于出组（会话结束后仍算组员）；出组走「移出分组」。
 * 留在这台 `config.json` 的只有这扇窗自己的看法：顺序 `tabBar.order` · 收着的组 `tabBar.collapsed` · 「已隐藏」那一行收展（`tab-bar-state.ts`）。
 */
import {
  getCollapsed,
  getHiddenOpen,
  getTabOrder,
  ORDER_CAP,
  setCollapsed as writeCollapsed,
  setHiddenOpen as writeHiddenOpen,
  setTabOrder,
} from "./tab-bar-state";
import type { Tab } from "./tab-model";
import { copyText } from "./copy-table";
import { failToast, silentUndo, undoToast } from "./kit/toast";
import type { TabStore } from "./tab-store";
import type { Origin } from "./ipc/origin";
import type { GroupBook } from "./group-book";
import type { GroupCount } from "./generated/GroupCount";
import type { HereGroup } from "./generated/HereGroup";
import type { MemberFacts } from "./generated/MemberFacts";

/** 栏里的一组：名字 · 组头那几颗点（后端数好的）· 收着（这扇窗的看法）。 */
export interface TabGroup {
  id: string;
  name: string;
  counts: GroupCount[];
  /** 收着（组员不显示，组头汇总需手动 / 在跑）；展开 ⇒ 不写这一格。 */
  collapsed?: true;
}

/** 落盘偏好要宿主做的四件事。 */
export interface TabBarPrefsHost {
  refreshTabBar(): void;
  /** 复活固定 tab 用（与活跃清单一到就建的骨架 tab 是同一条路）。 */
  createSkeletonTab(
    sessionId: string,
    projectDir: string | null,
    origin: Origin,
    background: boolean,
  ): void;
  /** 复活出来的固定 tab 空态里那颗「Resume 这个会话」（与右键菜单的 Resume 同一个动作）。 */
  resumeTab(sessionId: string): Promise<void>;
  /** 这个会话的流容器（那颗提示挂在里面；流容器是流视图那一半的，没有这个 tab ⇒ `null`）。 */
  streamElOf(sessionId: string): HTMLElement | null;
}

/** 撤销用的那一刻：顺序 · 组表 · 每个 tab 的组。 */
export interface GroupSnapshot {
  order: string[];
  groups: TabGroup[];
  groupAt: Map<string, string | null>;
}

/** 一条会话在账上的键（同一个 sid 在两台上是两条）。 */
function keyOf(origin: Origin, sid: string): string {
  return `${origin}\n${sid}`;
}

/** 刚建、本机还没回新 id 的组（栏里先画着）。 */
let pendingSeq = 0;
function pendingId(): string {
  pendingSeq += 1;
  return `pending-${pendingSeq}`;
}

/** 几问都收下了没有（一问都没有 ⇒ 收下了）。 */
function all(work: readonly Promise<boolean>[]): Promise<boolean> {
  return Promise.all(work).then((oks) => oks.every(Boolean));
}

export class TabBarPrefs {
  /** 组表：照组的账读回来的那一份（先后同后端：照建的先后）。谁在组里看 `Tab.group`。 */
  collections: TabGroup[] = [];
  /** 读到过一份组的账没有。撕离出来的查看窗不接账 ⇒ 恒 `false` ⇒ 不给分组入口。 */
  collectionsLoaded = false;
  /** 栏底「已隐藏」那一行展开着没有（`tabBar.hiddenOpen`；缺省收着）。 */
  hiddenOpen = false;
  /** 读到过一份组的账没有（固定那一半同一份账；没读到 ⇒ 不给固定入口）。 */
  pinnedLoaded = false;
  /** 收着的组（`tabBar.collapsed`：这扇窗的看法，不进后端）。 */
  private collapsed = new Set<string>();
  /** 组的账（主窗口接上；没接 ⇒ 只画、不写）。 */
  private book: GroupBook | null = null;
  /** 最近一份账里：会话 ⇒ 画在哪一组 · 固定的那几条（带复活那几格）。 */
  private placed = new Map<string, string>();
  private pinRows = new Map<string, { origin: Origin; sid: string; facts: MemberFacts }>();
  /** 每组的整份记录（id · 名字 · 时刻）：进组时随 `move` 抄进组员那一台。照账上的那一份，加上这一趟刚起的。 */
  private records = new Map<string, HereGroup>();
  /** 这扇窗这一趟建的组（占位 id 与本机回的 id）：撤销只撤掉它们，别处建的组不碰。 */
  private founded = new Set<string>();
  /** 这一趟里复活过的固定会话（关掉的不再复活，除非账上又有它）。 */
  private revived = new Set<string>();
  /**
   * 复活出来的固定 tab 的空态提示（sid → 元素）。复活的 tab 里一条内容都没有（已结束的会话只能 resume），
   * 不放点东西进去，点它就是一片空白、与坏了没有区别。复活成活的（真 resume 上了）时摘掉。
   */
  private readonly pinHintEls = new Map<string, HTMLElement>();

  constructor(
    private readonly store: TabStore,
    private readonly host: TabBarPrefsHost,
  ) {}

  /** 接上组的账：每读回来一份就照它重画。 */
  attachBook(book: GroupBook): void {
    this.book = book;
    book.subscribe(() => this.applyRead());
    this.applyRead();
  }

  /** 拉一次这扇窗自己的看法（收着的组 · 「已隐藏」那一行）并重画（宿主启动时调一次）。 */
  async loadCollections(): Promise<void> {
    const [collapsed, hiddenOpen] = await Promise.all([getCollapsed(), getHiddenOpen()]);
    this.collapsed = new Set(collapsed);
    this.hiddenOpen = hiddenOpen;
    this.collections = this.collections.map((c) => this.withCollapsed(c));
    this.host.refreshTabBar();
  }

  private withCollapsed(c: TabGroup): TabGroup {
    const { collapsed: _was, ...rest } = c;
    return this.collapsed.has(c.id) ? { ...rest, collapsed: true } : rest;
  }

  /** 照账上最近那一份重画：组表 · 每个 tab 的组与固定 · 固定了却没有标签的复活成灰标签。 */
  private applyRead(): void {
    const read = this.book?.read();
    if (!read) return;
    this.collections = read.groups.map((g) => this.withCollapsed({ id: g.id, name: g.title, counts: g.counts }));
    for (const g of read.groups) {
      if (g.name !== null) this.records.set(g.id, { id: g.id, name: g.name, made: g.made ?? 0, named: g.named ?? 0 });
    }
    this.placed = new Map();
    for (const g of read.groups) for (const m of g.members) {
      const k = keyOf(m.origin, m.sid);
      if (!this.placed.has(k)) this.placed.set(k, g.id);
    }
    this.pinRows = new Map(read.pinned.map((p) => [keyOf(p.origin, p.sid), { origin: p.origin, sid: p.sid, facts: p.facts }]));
    for (const t of this.store.tabs.values()) this.adoptGroup(t);
    for (const p of this.pinRows.values()) this.revivePinned(p);
    this.collectionsLoaded = true;
    this.pinnedLoaded = true;
    this.host.refreshTabBar();
  }

  /** 一个 tab 到了（`ensureTab` 建出它 / 账读回来一份）：组与固定照账上那一份。 */
  adoptGroup(tab: Tab): void {
    if (!this.collectionsLoaded && this.book?.read() == null) return;
    const k = keyOf(tab.origin, tab.sessionId);
    // 隐藏的不画在组里（收在栏底「已隐藏」那一行）。
    tab.group = tab.hidden != null ? null : (this.placed.get(k) ?? null);
    tab.pinned = this.pinRows.has(k);
  }

  /** 固定了却没有标签：照那台给的那几格造灰标签（只造一次；关掉的 ⇒ 账上已取消固定，不会再来）。 */
  revivePinned(p: { origin: Origin; sid: string; facts: MemberFacts }): void {
    if (this.store.tabs.has(p.sid) || this.revived.has(p.sid)) return;
    this.revived.add(p.sid);
    this.host.createSkeletonTab(p.sid, p.facts.cwd, p.origin, p.facts.bg);
    const t = this.store.tabs.get(p.sid);
    if (!t) return;
    t.parentPath = p.facts.record ?? ""; // resume 与「有没有记录」都靠它
    t.title = p.facts.title; // 骨架期就显示正确标题，不等读文件
    t.pinned = true;
    t.group = this.placed.get(keyOf(p.origin, p.sid)) ?? null;
    this.mountPinHint(t);
  }

  /** `sid` 所在的组（`null` = 散 tab / 没这个 tab）。 */
  groupAt(sid: string): TabGroup | null {
    const gid = this.store.tabs.get(sid)?.group ?? null;
    return gid === null ? null : (this.collections.find((c) => c.id === gid) ?? null);
  }

  /**
   * 右键「新建集合…」/ 拖放压在一个散 tab 上：建一个组、把 `sids` 放进去，回栏里那一组此刻的 id（名字框照它开）。
   * 栏里当场画上（本机回新 id 之前用一个占位的 id），拿到 id 再把这几条写进各自那台（记录带着此刻栏里的名字，期间改过名也跟上）。
   * 名字空 ⇒ 什么都不做、回 `null`。`landed`：本机建成、各台都收下了没有（撤销那一条等它）。
   */
  foundGroup(sids: readonly string[], name: string): { id: string; landed: Promise<boolean> } | null {
    const n = name.trim();
    if (!n) return null;
    const temp = pendingId();
    this.founded.add(temp);
    this.collections = [...this.collections, { id: temp, name: n, counts: [] }];
    const moved: Array<{ tab: Tab; from: string | null }> = [];
    for (const sid of sids) {
      const t = this.store.tabs.get(sid);
      if (!t) continue;
      moved.push({ tab: t, from: t.group });
      t.group = temp;
    }
    const book = this.book;
    if (!book) return { id: temp, landed: Promise.resolve(true) };
    const landed = book
      .make(n)
      .then(async (rec) => {
        const now = this.collections.find((c) => c.id === temp);
        // 本机回话之前已经撤销了（组从栏里没了）⇒ 什么都没写过，就此作罢。
        if (!now) return false;
        const id = rec.id;
        this.records.set(id, { ...rec, name: now.name });
        this.founded.add(id);
        this.collections = this.collections.map((c) => (c.id === temp ? { ...c, id } : c));
        if (this.collapsed.delete(temp)) this.collapsed.add(id);
        for (const { tab } of moved) if (tab.group === temp) tab.group = id;
        this.host.refreshTabBar();
        return all(moved.map(({ tab, from }) => this.move(tab, from, id)));
      })
      .catch(this.refused);
    return { id: temp, landed };
  }

  /** 此刻的分组与顺序（撤销用）：顺序 · 组表 · 栏里每个 tab 的组。 */
  snapshot(): GroupSnapshot {
    const groupAt = new Map<string, string | null>();
    for (const t of this.store.tabs.values()) groupAt.set(t.sessionId, t.group);
    return { order: [...this.store.orderedIds], groups: this.collections.map((c) => ({ ...c })), groupAt };
  }

  /**
   * 撤销（拖着建组 / 进出组 / 排序 · 右键移出 · 解散）：顺序、组员原样退回；期间解散掉的组重建一份（新 id）、组员放回去；
   * 期间新建、退回后栏里没人的组撤掉。期间改过的组名留着；已不在栏里的 tab 不管；期间新到的 tab 排在后面。
   */
  restore(snap: GroupSnapshot): void {
    const now = new Set(this.collections.map((c) => c.id));
    const remade = new Map<string, Promise<string>>();
    for (const g of snap.groups) {
      if (now.has(g.id) || !this.book) continue;
      remade.set(
        g.id,
        this.book.make(g.name).then((rec) => {
          this.records.set(rec.id, rec);
          return rec.id;
        }),
      );
      this.collections = [...this.collections, { ...g }];
    }
    for (const [sid, gid] of snap.groupAt) {
      const t = this.store.tabs.get(sid);
      if (!t || t.group === gid) continue;
      const from = t.group;
      t.group = gid;
      const id = gid === null ? null : remade.get(gid);
      if (id === undefined || id === null) void this.move(t, from, gid);
      else void id.then((real) => {
        this.collections = this.collections.map((c) => (c.id === gid ? { ...c, id: real } : c));
        if (t.group === gid) t.group = real;
        return this.move(t, from, real);
      }).catch((e: unknown) => this.failed(e));
    }
    // 这扇窗期间新建、此刻栏里一个组员都不剩的组：从栏里摘掉（组员都出了，各片已删；还没建成的建成那一刻见栏里没它就作罢）。
    const had = new Set(snap.groups.map((c) => c.id));
    for (const c of [...this.collections]) {
      if (had.has(c.id) || !this.founded.has(c.id) || [...this.store.tabs.values()].some((t) => t.group === c.id)) continue;
      this.forgetGroup(c.id);
    }
    const back = new Set(snap.order);
    const order = [...snap.order.filter((s) => this.store.tabs.has(s)), ...this.store.orderedIds.filter((s) => !back.has(s))];
    const moved = order.some((s, i) => s !== this.store.orderedIds[i]);
    this.store.orderedIds = order;
    if (moved) void this.persistOrder();
    this.host.refreshTabBar();
  }

  /**
   * 做完一步分组 / 顺序改动之后：改了分组 ⇒ 可撤的 toast（`title`），`landed`（那几台的回包）成了才出，没成那一句已由 [`failed`] 说过；
   * 只排了顺序（`null`）⇒ 不出条，`Ctrl+Z` 照样撤。没给 `landed` ⇒ 没问哪一台（只是这扇窗的顺序），当场出。
   */
  offerUndo(title: string | null, before: GroupSnapshot, landed?: Promise<boolean>): void {
    const undo = (): void => this.restore(before);
    if (title === null) silentUndo(undo);
    else if (!landed) undoToast(title, undo, () => {});
    else void landed.then((ok) => ok && undoToast(title, undo, () => {}));
  }

  /** 右键「加入集合 › X」/ 拖放进组：`sid` 进组 `gid`（原来画在别的组 ⇒ 出那一组）。 */
  joinGroup(sid: string, gid: string): Promise<boolean> {
    return this.joinGroupMany([sid], gid);
  }

  /** 批量「加入集合 › X」：每一个同 [`joinGroup`]。 */
  joinGroupMany(sids: readonly string[], gid: string): Promise<boolean> {
    if (!this.collections.some((c) => c.id === gid)) return Promise.resolve(true);
    const work: Array<Promise<boolean>> = [];
    for (const sid of sids) {
      const t = this.store.tabs.get(sid);
      if (!t || t.group === gid) continue;
      const from = t.group;
      t.group = gid;
      work.push(this.move(t, from, gid));
    }
    return all(work);
  }

  /** 右键「移出」/ 拖出组：`sid` 回到散 tab。 */
  leaveGroup(sid: string): Promise<boolean> {
    return this.leaveGroupMany([sid]);
  }

  /** 批量「移出集合」：每一个同 [`leaveGroup`]；组里因此谁都不剩 ⇒ 栏里那一组也摘掉（各片随最后一位出组自己删，不另发解散）。 */
  leaveGroupMany(sids: readonly string[]): Promise<boolean> {
    const left = new Map<string, string[]>();
    const work: Array<Promise<boolean>> = [];
    for (const sid of sids) {
      const t = this.store.tabs.get(sid);
      if (!t || t.group === null) continue;
      const from = t.group;
      t.group = null;
      left.set(from, [...(left.get(from) ?? []), keyOf(t.origin, sid)]);
      work.push(this.move(t, from, null));
    }
    for (const [gid, gone] of left) if (this.emptyAfter(gid, gone)) this.forgetGroup(gid);
    return all(work);
  }

  /** 组 `gid` 去掉 `gone` 那几条以后还有没有组员（账上的 ＋ 栏里的）。 */
  private emptyAfter(gid: string, gone: readonly string[]): boolean {
    const away = new Set(gone);
    for (const t of this.store.tabs.values()) if (t.group === gid) return false;
    for (const g of this.book?.read()?.groups ?? []) {
      if (g.id !== gid) continue;
      if (g.members.some((m) => !away.has(keyOf(m.origin, m.sid)))) return false;
    }
    return true;
  }

  /** 组头 ×：解散。只去掉分组，一个 tab 都不动（集合是视图，不是容器）：本机发给组员所在的每一片，各片摘掉那一组。 */
  dissolveGroup(gid: string): Promise<boolean> {
    const machines = this.machinesOf(gid);
    for (const t of this.store.tabs.values()) if (t.group === gid) t.group = null;
    this.forgetGroup(gid);
    if (!this.book || machines.length === 0 || gid.startsWith("pending-")) return Promise.resolve(true);
    return this.book.dissolve(gid, machines).then(() => true, this.refused);
  }

  /** 组表去掉 `gid`（只是栏里这一格，不写哪一台）。 */
  private forgetGroup(gid: string): void {
    this.collections = this.collections.filter((c) => c.id !== gid);
    this.records.delete(gid);
    if (this.collapsed.delete(gid)) void this.persistCollapsed();
  }

  /** 组 `gid` 的组员在哪几台（账上的 ＋ 栏里画在它里面的），照先见的先后：改名 · 解散发给这几片。 */
  private machinesOf(gid: string): Origin[] {
    const out: Origin[] = [];
    const add = (o: Origin): void => {
      if (!out.includes(o)) out.push(o);
    };
    for (const g of this.book?.read()?.groups ?? []) if (g.id === gid) for (const m of g.members) add(m.origin);
    for (const t of this.store.tabs.values()) if (t.group === gid) add(t.origin);
    return out;
  }

  /** 收起 / 展开这几个组（这扇窗的看法 `tabBar.collapsed`，落盘）。都已是那个样子 ⇒ 零写。 */
  setCollapsed(gids: readonly string[], on: boolean): Promise<void> {
    let changed = false;
    for (const gid of gids) {
      if (this.collapsed.has(gid) === on) continue;
      changed = true;
      if (on) this.collapsed.add(gid);
      else this.collapsed.delete(gid);
    }
    if (!changed) return Promise.resolve();
    this.collections = this.collections.map((c) => this.withCollapsed(c));
    return this.persistCollapsed();
  }

  /** 迁移那一趟带来的收着的组（新 id）：记进这扇窗的看法。 */
  collapseMigrated(gids: readonly string[]): Promise<void> {
    return this.setCollapsed(gids, true);
  }

  private persistCollapsed(): Promise<void> {
    return writeCollapsed([...this.collapsed]).catch((e: unknown) => this.failed(e));
  }

  /** 「已隐藏」那一行收 / 展：先改内存再落盘（落盘失败只记日志）。没接组的账的实例（撕出来的查看窗）不落盘。 */
  setHiddenOpen(on: boolean): Promise<void> {
    if (this.hiddenOpen === on) return Promise.resolve();
    this.hiddenOpen = on;
    if (!this.collectionsLoaded) return Promise.resolve();
    return writeHiddenOpen(on).catch((e: unknown) => console.warn("[tab-bar] 写「已隐藏」收展失败：", e));
  }

  /**
   * 要切到的标签页在收着的组里（Ctrl+J · 命令面板 · 通知）⇒ 那个组展开（不再自动收回）；在收着的「已隐藏」那一行里 ⇒ 那一行展开。
   * 回有没有展开。
   */
  expandFor(sid: string): boolean {
    if (this.store.tabs.get(sid)?.hidden != null) {
      if (this.hiddenOpen) return false;
      void this.setHiddenOpen(true);
      return true;
    }
    const col = this.groupAt(sid);
    if (!col?.collapsed) return false;
    void this.setCollapsed([col.id], false);
    return true;
  }

  /** 组头就地改名（本机发给组员所在的每一片）。空名 / 与现名相同 ⇒ 零写；还没建成的组只改栏里（建成时记录带着这个名字）。 */
  renameGroup(gid: string, name: string): Promise<void> {
    const n = name.trim();
    const g = this.collections.find((c) => c.id === gid);
    if (!g || !n || g.name === n) return Promise.resolve();
    this.collections = this.collections.map((c) => (c.id === gid ? { ...c, name: n } : c));
    const rec = this.records.get(gid);
    if (rec) this.records.set(gid, { ...rec, name: n });
    const machines = this.machinesOf(gid);
    if (!this.book || machines.length === 0 || gid.startsWith("pending-")) return Promise.resolve();
    return this.book.rename(gid, n, machines).catch((e: unknown) => this.failed(e));
  }

  /** 一个 tab 从 `from` 挪进 `to`（`null` ＝ 散着）：会话那台一问（出 · 进一起交，进组带那一组的记录）。 */
  private move(tab: Tab, from: string | null, to: string | null): Promise<boolean> {
    const join = to === null ? null : (this.records.get(to) ?? null);
    const leave = from === null || from === to || from.startsWith("pending-") ? null : from;
    if (!join && leave === null) return Promise.resolve(true);
    if (!this.book) return Promise.resolve(true);
    return this.book.move(tab.origin, tab.sessionId, join, leave).then(() => true, this.refused);
  }

  /** 固定 · 取消固定：会话那台一条注解。回「那台收下了没有」（没收下 ⇒ 已说过、账重读）。 */
  private pin(origin: Origin, sid: string, on: boolean): Promise<boolean> {
    if (!this.book) return Promise.resolve(true);
    return this.book.pin(origin, sid, on).then(
      () => true,
      (e: unknown) => {
        this.failed(e, copyText("tabBar.persist.pinnedFailed"));
        return false;
      },
    );
  }

  /** 分组 · 固定没交成：toast ＋ 记日志；账重读一次（栏回到真相）。 */
  private failed(e: unknown, title: string = copyText("tabBar.persist.collectionsFailed")): void {
    console.warn("[tab-bar] 分组 / 固定没交成：", e);
    failToast(title, e);
    if (this.book) void this.book.refreshAll([]);
  }

  /** 分组那一问没成：同 [`failed`]，回「没收下」。 */
  private readonly refused = (e: unknown): false => {
    this.failed(e);
    return false;
  };

  // ===== 固定（pinned）=====

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
    this.host.streamElOf(sid)?.appendChild(box);
    this.pinHintEls.set(sid, box);
  }

  /** 复活成 live（resume 真的接上了）或 tab 关掉时，摘掉那块空态提示。 */
  clearPinHint(sid: string): void {
    this.pinHintEls.get(sid)?.remove();
    this.pinHintEls.delete(sid);
  }

  /** 右键菜单那一项：翻转固定，先改内存再交会话那台。不自动固定，这是唯一的入口。回「那台收下了没有」（没翻 ⇒ `false`）。 */
  togglePin(sid: string): Promise<boolean> {
    const tab = this.store.tabs.get(sid);
    if (!tab || !this.pinnedLoaded) return Promise.resolve(false);
    return this.setPinnedMany([sid], !tab.pinned);
  }

  /** 批量「固定 / 取消固定」：每一个翻到 `on`，各交会话那台一条注解。回「交了的都收下了」（一个都没交 ⇒ `false`）。 */
  setPinnedMany(sids: readonly string[], on: boolean): Promise<boolean> {
    if (!this.pinnedLoaded) return Promise.resolve(false);
    const work: Array<Promise<boolean>> = [];
    for (const sid of sids) {
      const tab = this.store.tabs.get(sid);
      if (!tab || tab.pinned === on) continue;
      tab.pinned = on;
      work.push(this.pin(tab.origin, sid, on));
    }
    if (work.length > 0) this.host.refreshTabBar();
    return Promise.all(work).then((oks) => oks.length > 0 && oks.every(Boolean));
  }

  /**
   * 点 × 关掉一个 tab（它已不在栏里）：固定的取消固定、在组里的出组，各交会话那台；组里因此谁都不剩 ⇒ 栏里那一组摘掉（那一片随最后一位出组自己删）。
   * （会话自己结束不走这里：进程退出后仍算组员。）
   */
  forgetClosed(tab: Tab): void {
    const k = keyOf(tab.origin, tab.sessionId);
    const gid = tab.group;
    if (tab.pinned) {
      tab.pinned = false;
      this.pinRows.delete(k);
      void this.pin(tab.origin, tab.sessionId, false);
    }
    if (gid === null) return;
    tab.group = null;
    this.placed.delete(k);
    void this.move(tab, gid, null);
    if (this.emptyAfter(gid, [k])) this.forgetGroup(gid);
  }

  /**
   * 这个复活出来的固定 tab 点进去也没有东西可看：那台给的那几格里没有记录（记录已不在），
   * 且到现在也没有行回填过 `parentPath`。点它时要说人话。
   */
  private pinIsDegraded(sid: string): boolean {
    const row = [...this.pinRows.values()].find((p) => p.sid === sid);
    if (!row || row.facts.record !== null) return false;
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
   * 读的那一刻「已删的」与「还没到的」分不开 ⇒ 顺序存成意图（`savedOrder`），每次应用时按此刻在的 tab 过滤
   * （`applySavedOrder`），tab 陆续到达时由 `placeInOrder` 再应用一次。
   */
  async loadOrder(): Promise<void> {
    // `null` ＝ 这一趟不按存活过滤（见上）。
    this.store.savedOrder = await getTabOrder(null);
    if (this.store.applySavedOrder()) this.host.refreshTabBar();
  }
}
