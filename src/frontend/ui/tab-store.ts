/**
 * 会话状态账。
 *
 * tab 集合 · 顺序（连同盘上那份顺序意图）· 当前 tab · 是否在重放批里 · 早于 tab 到达的信号暂存
 * （已结束 / 可重连 / 红绿灯）· 不可 attach 的 sid · 账号快照 · 任务快照 —— **收进一处**；
 * 「tab 集合变了」这件事**只有一份订阅**（`subscribe` / `notify`），宿主（状态栏、空态）挂在这上面。
 *
 * 零 DOM、零 IPC：它只存东西、只做「新 tab 落在哪一格」这类纯顺序运算。谁什么时候改它、
 * 改完要刷哪块界面，是 `TabManager`（组装根）与各视图的事。
 */
import type { SessionAccount } from "./accounts";
import type { TaskEntry } from "./tasks-panel";
import type { Tab, TabsSummary } from "./tab-model";
import { isLive, type ContainerEvent } from "./tab-session-state";
import { Slice } from "./app-store";
import { barRows } from "./tab-drop";
import type { UsageFact } from "./session-reads";

/** 当前 tab 那一格对外的样子：HUD 要的 usage 与「会话事实要不到」的原因。 */
export interface ActiveView {
  sid: string | null;
  model: string | null;
  promptTokens: number | null;
  /** 上下文上限（后端定的）。 */
  contextLimit: number | null;
  /** 上限从哪来（后端那一格）。 */
  limitFrom: UsageFact["limitFrom"];
  /** 会话事实要不到的原因（`null` = 可用）。 */
  unavailable: string | null;
  /** 当前 tab 的项目目录（`Tab.projectDir`，后端给的那一格；独立窗口的顶栏标题读它）。 */
  projectDir: string | null;
}

const NO_ACTIVE: ActiveView = { sid: null, model: null, promptTokens: null, contextLimit: null, limitFrom: "assumed", unavailable: null, projectDir: null };
const sameActive = (a: ActiveView, b: ActiveView): boolean =>
  a.sid === b.sid &&
  a.model === b.model &&
  a.promptTokens === b.promptTokens &&
  a.contextLimit === b.contextLimit &&
  a.limitFrom === b.limitFrom &&
  a.unavailable === b.unavailable &&
  a.projectDir === b.projectDir;
const sameSummary = (a: TabsSummary, b: TabsSummary): boolean =>
  a.total === b.total && a.live === b.live && a.dead === b.dead;

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
   * 单独一张表：它是「某个 sid 的一条会话级元信息」，与 `sessionAccountsByS` 同形。
   */
  readonly notAttachableSids = new Set<string>();
  /** 远端 live 探测的会话账号归属（sid → 探测行）。main.ts 喂。 */
  sessionAccountsByS = new Map<string, SessionAccount>();
  /** 账号名 → 邮箱（徽章悬停用）。 */
  accountEmailByName = new Map<string, string>();
  /** sid → lastAccount（history-metadata）：live 探测不到时徽章的兜底来源。main.ts 喂。 */
  accountLastByS = new Map<string, string>();
  /** 账号可查询的远端 origin 集（available）。只有这些 origin 的会话才显徽章。 */
  accountReadyOrigins = new Set<string>();
  /** origin → 当前账号名：会话账号 == 它 → 不挂徽章。只放 isSelectable 的账号（main.ts 过滤）：拿不可选的号说「你不一致」是假信息。 */
  currentByOrigin = new Map<string, string>();
  activeId: string | null = null;
  /** 是否在批模式（启动重放期间）。控两件事：惰性高亮、BranchFolder.batchMode（批里新建的 Tab 也设成 batch）。 */
  inBatch = false;
  /**
   * 此刻喂进 `onLine` 的是**取回来的历史**（按偏移 / 按行号，`TabStreamView.feedHistoryRows`），
   * 不是实时行。远端 tab「见行就从死翻回活」（`tabs.ts::ensureTab` 的 `remote-line` 那一格）只认实时行：
   * 在一个已结束的远端 tab 上往上翻、取回几条旧行，不许把它翻活。
   */
  historyFeed = false;
  /** 每个 sid 当前的任务列表（ensureTab 拉初次快照，之后按通道的任务变更重问）。切 Tab 时把那个 sid 的快照喂给 TasksPanel。 */
  readonly tasksBySid = new Map<string, TaskEntry[]>();
  /**
   * 归档信号（ended 格）早于这个 sid 的 Tab 建出来时记在这里，ensureTab 建 Tab 时落实。
   * ended 与行同一条队列，正常路径下不会先到；这一层兜「ended 先于该 sid 任何行」的异常序（§ 17a）。
   */
  readonly pendingArchive = new Set<string>();
  /**
   * 灰灯（idle-tmux）信号早于 Tab 建出时暂存（同 pendingArchive）：F5 后重发的 idle 可能早于骨架建 Tab，不暂存灰灯就丢了。
   * ensureTab 建 Tab 时落实（同时有 pendingArchive ⇒ 归档优先）。
   */
  readonly pendingTmuxIdle = new Set<string>();
  /** 红绿灯信号早于 Tab 建出来时暂存（activity 格当场派，而建 Tab 的行走异步队列）。ensureTab 时落实。 */
  /**
   * 容器事实（`container` 格）早于 Tab 建出时暂存。ensureTab 建 Tab 时落实
   * （建出来就是活的；早到的死亡信号优先 —— 那时容器一格由死的那一刻的裁决说了算）。
   */
  readonly pendingContainer = new Map<string, ContainerEvent>();
  /**
   * 已经把活会话清单报完了的机器（会话流的 `listed` 格）。
   * 固定复活时据它分：报完了 ⇒ 已结束（它不在清单里，不然 tab 早就被建成活的了）；没报完 ⇒ 说不清。
   */
  readonly seenOrigins = new Set<string>();
  readonly pendingActivity = new Map<string, NonNullable<Tab["activity"]>>();

  /** 「tab 集合变了」那一格。建在唯一的 pub-sub 原语上（`app-store.ts::Slice`），同值不通知。 */
  private readonly tabsSlice = new Slice<TabsSummary>({ total: 0, live: 0, dead: 0 }, sameSummary);

  /** 「当前 tab 变了」那一格（切 tab · 当前 tab 的 usage / 事实可用性变了都写这里；HUD 订阅它）。 */
  readonly active = new Slice<ActiveView>(NO_ACTIVE, sameActive);

  /** 订阅「tab 增 / 减 / 状态变」。返回退订函数。 */
  subscribe(listener: (summary: TabsSummary) => void): () => void {
    return this.tabsSlice.subscribe(listener);
  }

  /**
   * 此刻的数量摘要（总数 · 活 · 死）。按**活性**一轴分：可重连的会话 claude 已经没了 ⇒ 算死
   * （改两轴之前它借着 `status: live` 被算进「活跃」，状态栏的「活跃 N」多数了它）。
   */
  summary(): TabsSummary {
    let live = 0;
    for (const t of this.tabs.values()) {
      if (isLive(t.state)) live += 1;
    }
    return { total: this.tabs.size, live, dead: this.tabs.size - live };
  }

  /** 通知订阅者（摘要没变 ⇒ 不通知）。 */
  notify(): void {
    this.tabsSlice.set(this.summary());
  }

  /**
   * **条上看到的顺序**：组与散的混排在 `orderedIds` 里，每个组的组员聚到它第一个组员那一格（`tab-drop.ts::barRows`）。
   * Shift 连选 · 「需要你」读这一个顺序；给了 `skipCollapsed` ⇒ 跳过那些收着的组里的（数字键 · `]` `[` · 关掉后落到哪）。
   */
  visibleOrder(groupIds: readonly string[], skipCollapsed?: ReadonlySet<string>): string[] {
    const out: string[] = [];
    for (const r of barRows(this.orderedIds, (sid) => this.tabs.get(sid)?.group ?? null, groupIds, skipCollapsed)) {
      if (r.kind === "tab" && !r.hidden) out.push(r.sid);
    }
    return out;
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
