/**
 * 会话状态账。
 *
 * tab 集合 · 顺序（连同盘上那份顺序意图）· 当前 tab · 是否在重放批里 · 早于 tab 到达的脸与名字暂存 · 不可 attach 的 sid · 账号快照 · 任务快照 —— **收进一处**；
 * 「tab 集合变了」这件事**只有一份订阅**（`subscribe` / `notify`），宿主（状态栏、空态）挂在这上面。
 *
 * 零 DOM、零 IPC：它只存东西、只做「新 tab 落在哪一格」这类纯顺序运算。谁什么时候改它、
 * 改完要刷哪块界面，是 `TabManager`（组装根）与各视图的事。
 */
import type { TaskEntry } from "./tasks-panel";
import type { SessionFace } from "./generated/SessionFace";
import type { Tab, TabsSummary } from "./tab-model";
import { faceOf, hasTerminal, isLive } from "./face-can";
import { Slice } from "./app-store";
import { dotOf, needsOf } from "./session-face";
import { barRows } from "./tab-drop";
import type { UsageFact } from "./session-reads";

/** 当前 tab 那一格对外的样子：HUD 要的 usage 与「会话事实要不到」的原因。 */
export interface ActiveView {
  sid: string | null;
  /** 当前 tab 的标题（`Tab.title`，照核心给的名字排好的）：改了名会话头 · 窗口标题跟着刷（不带它 ⇒ 同值不通知，会话头停在旧名）。 */
  title: string | null;
  /** 当前会话的用量与上下文（后端那一份成品，照抄）。 */
  usage: UsageFact | null;
  /** 会话事实要不到的原因（`null` = 可用）。 */
  unavailable: string | null;
  /** 当前 tab 的项目目录（`Tab.projectDir`，后端给的那一格；独立窗口的顶栏标题读它）。 */
  projectDir: string | null;
  /** 当前会话的模型写给人看的字（`Tab.modelText`，后端那一格；HUD 浮层里那一句照抄）。 */
  modelText: string | null;
  /** 当前会话跑在哪个号下（那台在脸上说的；没说 ⇒ `null`）：状态栏账号按钮读它。 */
  account: string | null;
}

const NO_ACTIVE: ActiveView = { sid: null, title: null, usage: null, unavailable: null, projectDir: null, modelText: null, account: null };
const sameActive = (a: ActiveView, b: ActiveView): boolean =>
  a.sid === b.sid &&
  a.title === b.title &&
  JSON.stringify(a.usage) === JSON.stringify(b.usage) &&
  a.unavailable === b.unavailable &&
  a.projectDir === b.projectDir &&
  a.modelText === b.modelText &&
  a.account === b.account;
const sameSummary = (a: TabsSummary, b: TabsSummary): boolean =>
  a.total === b.total && a.live === b.live && a.dead === b.dead && a.faces === b.faces;

export class TabStore {
  readonly tabs = new Map<string, Tab>();
  /** 按插入顺序的 sessionId 数组，与 this.tabs.keys() 顺序一致但避免每次 Array.from */
  orderedIds: string[] = [];
  /**
   * **盘上那份顺序（`tabBar.order`），启动读一次之后留着。**
   *
   * 它是一份意图，不是一次性的动作：tab 是陆续到的，这份顺序要活过整个启动期，每来一个 tab 就再应用一次（`placeInOrder` → `applySavedOrder`）。
   * ⚠ 里面**允许有今天不存在的 sid**（被删的 / 还没宣告到的）——
   *   它们进不了 `orderedIds`（`applySavedOrder` 按 `present` 筛），所以不会造出假 tab；
   *   上界由 `ORDER_CAP` 在读的那一侧管。
   * ⚠ 用户一拖，盘上那份就**过期**了 ⇒ `persistOrder` 落盘的同时把这里同步成新的那张，
   *   否则后到的 tab 会拿一份旧顺序把用户刚拖的一下撤销。
   */
  savedOrder: string[] = [];
  /**
   * sid → attach 进去对人有没有意义（pidfile 的 `attachable`，经后端帧透传）。只记显式 false 的那些；缺席 = 可以。
   * 单独一张表：它是「某个 sid 的一条会话级元信息」。
   */
  readonly notAttachableSids = new Set<string>();
  /** 账号名 → 邮箱（徽章悬停用）。 */
  accountEmailByName = new Map<string, string>();
  /** sid → lastAccount（history-metadata）：宣告没说号时徽章的兜底来源。 */
  accountLastByS = new Map<string, string>();
  /** 账号可查询的远端 origin 集（available）。只有这些 origin 的会话才显徽章。 */
  accountReadyOrigins = new Set<string>();
  /** origin → 当前账号名：会话账号 == 它 → 不挂徽章。只放选得了的账号（`selectable`）（main.ts 过滤）：拿不可选的号说「你不一致」是假信息。 */
  currentByOrigin = new Map<string, string>();
  activeId: string | null = null;
  /** 是否在批模式（启动重放期间）：惰性高亮、旧记录收纳不建卡。 */
  inBatch = false;
  /** 每个 sid 当前的任务列表（ensureTab 拉初次快照，之后按通道的任务变更重问）。切 Tab 时把那个 sid 的快照喂给 TasksPanel。 */
  readonly tasksBySid = new Map<string, TaskEntry[]>();
  /** 脸早于建 Tab 到（那台说的会话还没有 Tab）⇒ 暂存，建 Tab 时落实；同一 sid 后到的覆盖先到的。 */
  readonly pendingFace = new Map<string, SessionFace>();
  /** 已经把活会话清单报完了的机器（会话流的 `listed` 格）：「没回来的组员」等每台都报完了才认。 */
  readonly seenOrigins = new Set<string>();
  /** Tab 还没建时先到的名字（`updateName`），建 Tab 时落实。 */
  readonly pendingName = new Map<string, { label: string; labelProject: string | null }>();

  /** 「tab 集合变了」那一格。建在唯一的 pub-sub 原语上（`app-store.ts::Slice`），同值不通知。 */
  private readonly tabsSlice = new Slice<TabsSummary>({ total: 0, live: 0, dead: 0, faces: "" }, sameSummary);

  /** 「当前 tab 变了」那一格（切 tab · 当前 tab 的 usage / 事实可用性变了都写这里；HUD 订阅它）。 */
  readonly active = new Slice<ActiveView>(NO_ACTIVE, sameActive);

  /** 订阅「tab 增 / 减 / 状态变」（状态变：活性 · 点 · 在等什么 · 在跑哪一步）。返回退订函数。 */
  subscribe(listener: (summary: TabsSummary) => void): () => void {
    return this.tabsSlice.subscribe(listener);
  }

  /** 此刻的数量摘要（总数 · 活 · 死）。按进程在不在分：Claude 已退出、tmux 会话还在的算死。 */
  summary(): TabsSummary {
    let live = 0;
    const faces: string[] = [];
    for (const t of this.tabs.values()) {
      if (isLive(faceOf(t))) live += 1;
      // 需手动那一份：种类 ＋ 挂着的请求（答题块按它换）＋ 只能在终端里答那一句 —— 种类不变、请求挂上 / 答掉也要通知。
      const n = needsOf(t);
      // 隐藏那一格 · 血缘：会话头的「已隐藏」「由 X 启动」「子会话 N」跟着换。
      const lin = t.lineage ? `${t.lineage.startedBy?.sid ?? ""}\u0002${t.lineage.children.map((c) => `${c.sid}:${c.face.dot}:${c.hidden?.by ?? ""}`).join(",")}` : "";
      faces.push(`${t.sessionId}\u0001${dotOf(t)}\u0001${faceOf(t).line}\u0001${n?.kind ?? ""}\u0001${n?.ask?.id ?? ""}\u0001${n?.manualText ?? ""}\u0001${t.pending[0]?.name ?? ""}\u0001${t.hidden?.said ?? ""}\u0001${lin}`);
    }
    return { total: this.tabs.size, live, dead: this.tabs.size - live, faces: faces.join("\u0000") };
  }

  /** 通知订阅者（摘要没变 ⇒ 不通知）。 */
  notify(): void {
    this.tabsSlice.set(this.summary());
  }

  /**
   * **条上看到的顺序**：组与散的混排在 `orderedIds` 里，每个组的组员聚到它第一个组员那一格（`tab-drop.ts::barRows`）。
   * Shift 连选 · 「需手动」读这一个顺序；给了 `skipCollapsed` ⇒ 跳过那些收着的组里的（数字键 · `]` `[` · 关掉后落到哪）。
   */
  visibleOrder(groupIds: readonly string[], skipCollapsed?: ReadonlySet<string>, skipHidden = false): string[] {
    const out: string[] = [];
    for (const r of barRows(this.barIds(), (sid) => this.tabs.get(sid)?.group ?? null, groupIds, skipCollapsed)) {
      if (r.kind === "tab" && !r.hidden) out.push(r.sid);
    }
    // 「已隐藏」那一行排在栏的最后（`skipHidden` ＝ 那一行收着，看不见的不走）。
    if (!skipHidden) out.push(...this.hiddenRow());
    return out;
  }

  /** 占标签页的那些（不隐藏的），按 `orderedIds`。 */
  barIds(): string[] {
    return this.orderedIds.filter((sid) => this.tabs.get(sid)?.hidden == null);
  }

  /**
   * 「已隐藏」那一行里列的：隐藏的、还没结束的（结束了就不列，要看去历史）；当前看着的那一个已结束了也先留着，切走再收（`TabManager.reapHidden`）。
   * 按 `orderedIds`。隐藏不隐藏只看 `Tab.hidden`（后端那一格），这里不判。
   */
  hiddenRow(): string[] {
    return this.orderedIds.filter((sid) => {
      const t = this.tabs.get(sid);
      return t !== undefined && t.hidden != null && (hasTerminal(faceOf(t)) || sid === this.activeId);
    });
  }

  /**
   * 新 tab 落位 = **追加到末尾，再按盘上那份顺序摆**。
   *
   * 每来一个都按盘上顺序再摆一次：只在 `loadOrder` 里摆一次的话，后到的 tab 都落到末尾、盘上给它留的那一格用不上。
   * 只动 `orderedIds`、不碰 DOM（拖拽期间的重画抑制由 `refreshTabBar` 那道守卫管）。
   * bg 会话与普通 tab 走同一条落位，零处按 kind 分叉（不自动归组；`tests/frontend/ui/bg-flat.vitest.ts` 钉着）。
   */
  placeInOrder(tab: Tab): void {
    this.orderedIds.push(tab.sessionId);
    this.applySavedOrder();
  }

  /**
   * 用户拖完之后要落盘的那一张：此刻栏里的顺序，**再把还没到的那些按它们在 `savedOrder` 里的相对位置并回去**
   * （各跟在它原来前面那个此刻在栏里的 sid 之后；前面一个都没有 ⇒ 排在最前）。摘掉还没到的，晚到的远端 tab 就永远落到末尾了。
   * 超过 `cap` ⇒ 先保住栏里的，还没到的按原序留到满为止。
   */
  mergedOrder(cap: number): string[] {
    const here = new Set(this.orderedIds);
    const after = new Map<string | null, string[]>();
    let anchor: string | null = null;
    let budget = Math.max(0, cap - this.orderedIds.length);
    for (const sid of this.savedOrder) {
      if (here.has(sid)) {
        anchor = sid;
        continue;
      }
      if (budget === 0) continue;
      budget--;
      const run = after.get(anchor) ?? [];
      run.push(sid);
      after.set(anchor, run);
    }
    const out = [...(after.get(null) ?? [])];
    for (const sid of this.orderedIds) out.push(sid, ...(after.get(sid) ?? []));
    return out;
  }

  /**
   * 把 `savedOrder`（盘上那份意图）应用到**此刻真的存在**的 tab 上。
   * 返回「顺序有没有真的变」—— 没变就别让调用方白重画一次栏。
   *
   * 🔴 **`present` 这道过滤是「不凭空造 tab」那条的落点**：盘上提到而今天不在的 sid
   *   （被删的会话 / **还没宣告到的会话**）在这里被跳过 —— 它**留在 `savedOrder` 里**，
   *   等它真的到了，`placeInOrder` 再应用一次就会把它放回自己那一格。
   *   ⚠ 这正是它不能在读的那一刻被摘掉的原因：摘掉就再也等不到了。
   * ⚠ **盘上没提到的排在后面**、且保持它们此刻的相对次序（`loadOrder` 头注逐字）。
   */
  applySavedOrder(): boolean {
    if (this.savedOrder.length === 0) return false;
    const present = new Set(this.orderedIds);
    const head = this.savedOrder.filter((sid) => present.has(sid));
    if (head.length === 0) return false;
    const inHead = new Set(head);
    const next = [...head, ...this.orderedIds.filter((sid) => !inHead.has(sid))];
    // 恒等就早退。`next` 与 `orderedIds` 必然同长（两边都是 `orderedIds` 的重排），
    // 所以逐位比一遍就够。
    if (next.every((x, i) => x === this.orderedIds[i])) return false;
    this.orderedIds = next;
    return true;
  }
}
