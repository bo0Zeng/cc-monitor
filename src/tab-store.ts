/**
 * 〔U2 · 拆 `tabs.ts` ① · `设计/01 §1.5`「一个 store，一个 router」〕**会话状态账**。
 *
 * tab 集合 · 顺序（连同盘上那份顺序意图）· 当前 tab · 是否在重放批里 · 早于 tab 到达的信号暂存
 * （归档 / 灰灯 / 红绿灯）· 不可 attach 的 sid · 账号快照 · 任务快照 —— **收进一处**；
 * 「tab 集合变了」这件事**只有一份订阅**（`subscribe` / `notify`），宿主（状态栏、空态）挂在这上面。
 *
 * 零 DOM、零 IPC：它只存东西、只做「新 tab 落在哪一格」这类纯顺序运算。谁什么时候改它、
 * 改完要刷哪块界面，是 `TabManager`（组装根）与各视图的事。
 * 字段与三个落位方法逐字从 `tabs.ts` 搬来（原先是 `TabManager` 的私有成员）。
 */
import type { SessionAccount } from "./accounts";
import type { TaskEntry } from "./tasks-panel";
import type { Tab, TabsSummary } from "./tab-model";

export class TabStore {
  readonly tabs = new Map<string, Tab>();
  /** 按插入顺序的 sessionId 数组，与 this.tabs.keys() 顺序一致但避免每次 Array.from */
  orderedIds: string[] = [];
  /**
   * 〔步 17·C · 2026-09-21〕**盘上那份顺序（`tabBar.order`），启动读一次之后留着。**
   *
   * 🔴 **它是「一份意图」，不是一次性的动作** —— 这就是那个 no-op 的修法所在
   *   （成因与现打见 `loadOrder` 头注）。tab 是**陆续**到的，所以这份顺序必须活过
   *   整个启动窗口期，每来一个 tab 就再应用一次（`placeInOrder` → `applySavedOrder`）。
   * ⚠ 里面**允许有今天不存在的 sid**（被删的 / 还没宣告到的）——
   *   它们进不了 `orderedIds`（`applySavedOrder` 按 `present` 筛），所以不会造出假 tab；
   *   上界由 `ORDER_CAP` 在读的那一侧管。
   * ⚠ 用户一拖，盘上那份就**过期**了 ⇒ `persistOrder` 落盘的同时把这里同步成新的那张，
   *   否则后到的 tab 会拿一份旧顺序把用户刚拖的一下撤销。
   */
  savedOrder: string[] = [];
  /**
   * E73：sid → **attach 进去对人有没有意义**（来自 pidfile 的 `attachable`，经后端帧透传）。
   *
   * 只记**显式 false** 的那些。缺席 = 可以 —— 存量会话与旧后端一律照旧，零迁移。
   *
   * # 为什么单独一张表而不是 `Tab` 的字段
   *
   * 加字段要动 `ensureTab` 的位置参数列车（R03 刚把那种形状收拾过一轮），而这就是
   * 「某个 sid 的一条会话级元信息」—— 与 `sessionAccountsByS` 同形，放这儿更合身。
   */
  readonly notAttachableSids = new Set<string>();
  /** A3：远端 live 探测的会话账号归属（sid → 探测行）。main.ts 定期喂。 */
  sessionAccountsByS = new Map<string, SessionAccount>();
  /** A3：账号名 → 邮箱（徽章 tooltip 用）。 */
  accountEmailByName = new Map<string, string>();
  /** A4：sid → lastAccount（history-metadata）。徽章源②：live 探测不到时兜底。main.ts 定期喂。 */
  accountLastByS = new Map<string, string>();
  /** A4/§7：账号可查询的远端 origin 集（available）。只有这些 origin 的会话才显徽章。 */
  accountReadyOrigins = new Set<string>();
  /** account-ux U5：origin → 当前账号名。徽章「信息才显」比对：会话账号==它 → 不挂徽章。main.ts 定期喂。
   *  **只放 isSelectable 的账号**（main.ts 侧过滤）：不可选的当前账号对齐必失败，指着它说"你不一致"
   *  是假信息，且与 U1 `resolveFollowAccount`「不可选就下沉」的语义保持一致。 */
  currentByOrigin = new Map<string, string>();
  activeId: string | null = null;
  /**
   * v2.2 (issue #12): 当前是否在 batch 模式（启动重放 jsonl-batch 期间）。
   * batch 模式中 ensureTab 创建的新 Tab 也要把 BranchFolder 设成 batch。
   *
   * P5.2 B 重构：inPrependMode / pendingPrependFragment / source flag 全删 —— 前端
   * 改用 RecordTimeline 按 seq binary-insert，DOM 位置由 seq 决定不受 emit 顺序影响。
   * 仍保留 inBatch 是因为它控两件事：(1) lazy hljs 注册 (2) BranchFolder.batchMode。
   */
  inBatch = false;
  /**
   * issue #11: 每个 sid 当前 task 列表（由 ensureTab 拉初次快照 + task-update 事件
   * 更新）。切 Tab 时把对应 sid 的快照喂给全局 TasksPanel。
   */
  readonly tasksBySid = new Map<string, TaskEntry[]>();
  /**
   * issue #19：归档信号（session-ended）可能早于 replay 把该 sid 的 Tab 建出来。
   * archiveTab 时若 Tab 还不存在，记进这里；ensureTab 建 Tab 时回查、落实归档。
   *
   * issue #20 后 session-ended 已改进 events.ts 的 queue 与行同序处理（否则补发
   * 归档会被后续 drain 的远端行 un-archive 吃掉），正常路径下 ended 不会再早于
   * 行到达——本集合降级为防御层（§ 17a 双层防御），保留兜“ended 先于该 sid 任何
   * 行”的异常序。
   */
  readonly pendingArchive = new Set<string>();
  /**
   * audit-fixes F03.2：灰灯（idle-tmux）信号早于 Tab 建出时暂存（同 pendingArchive 模式）。
   * F5 frontend-ready 重放会重发 SESSION_IDLE，可能早于骨架 remote-added 建 Tab——不暂存则
   * markTmuxIdle no-op、灰灯丢。ensureTab 建 Tab 时落实（除非同时 pendingArchive→归档优先）。
   */
  readonly pendingTmuxIdle = new Set<string>();
  /**
   * issue #23：红绿灯信号早于 Tab 建出来时暂存（同 pendingArchive 的时序竞争模式：
   * session-activity 同步派发，而建 Tab 的行走异步 queue/drain）。ensureTab 时落实。
   */
  readonly pendingActivity = new Map<
    string,
    { status: string; waitingFor: string | null }
  >();

  /** 「tab 集合变了」的订阅者。**全前端这一件事只有这一份**（原先是 `TabManager` 构造参数里的一个回调）。 */
  private readonly listeners = new Set<(summary: TabsSummary) => void>();

  /** 订阅「tab 增 / 减 / 状态变」。返回退订函数。 */
  subscribe(listener: (summary: TabsSummary) => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /** 此刻的数量摘要（总数 · live · archived）。 */
  summary(): TabsSummary {
    let live = 0;
    let archived = 0;
    for (const t of this.tabs.values()) {
      if (t.status === "archived") archived += 1;
      else live += 1;
    }
    return { total: this.tabs.size, live, archived };
  }

  /** 通知全部订阅者。没人订阅就连摘要都不算（原先 `notifyChanged` 的早退，照旧）。 */
  notify(): void {
    if (this.listeners.size === 0) return;
    const s = this.summary();
    for (const l of this.listeners) l(s);
  }

  /**
   * Batch7-F24 树状排序：bg tab 插到同 (cwd, origin) 交互宿主（及其既有 bg 子项）
   * 之后；无宿主则追加末尾。交互 tab 创建时反向重锚——把已存在的同 (cwd, origin)
   * bg tab 拉到自己身后（骨架清单里 bg 可能先于宿主出现）。父子判定 v1 = cwd
   * 归属（pidfile 无 parentSessionId 字段，精确父子留 backlog）。
   */
  /**
   * 〔步 17·C · 2026-09-21〕新 tab 落位 = **先按树摆（`placeInTree`），再按盘上那份顺序摆**。
   *
   * 🔴 **这一层是那个 no-op 的第二半修法**：tab 是陆续到的，而 `loadOrder` 只跑一次
   *   ⇒ 只在 `loadOrder` 里应用一次，**后到的每一个 tab 都会落到末尾**，
   *   盘上给它留的那一格永远用不上（现打：会话到齐后顺序 == 到达序）。
   * ⚠ 包成两层而不是往 `placeInTree` 里塞一句：它有 3 个 `return` 出口，
   *   逐个补一句就是下一次「补漏了一个出口」。
   * ⚠ 这里**只动 `orderedIds`、不碰 DOM** —— 拖拽期间的重画抑制（★ 6d）由
   *   `refreshTabBar` 那道守卫管，与本函数无关（今天 `placeInTree` 也是这个形状）。
   */
  placeInOrder(tab: Tab): void {
    this.placeInTree(tab);
    this.applySavedOrder();
  }

  private placeInTree(tab: Tab): void {
    const isBg = tab.kind !== null && tab.kind !== "interactive";
    const sameHost = (t: Tab | undefined): boolean =>
      !!t && t.cwd !== null && t.cwd === tab.cwd && t.origin === tab.origin;
    if (isBg && tab.cwd) {
      // 找宿主（交互 + 同 cwd/origin）——插到宿主连同其已有 bg 子串之后
      for (let i = 0; i < this.orderedIds.length; i++) {
        const t = this.tabs.get(this.orderedIds[i]);
        if (sameHost(t) && (t!.kind === null || t!.kind === "interactive")) {
          let j = i + 1;
          while (j < this.orderedIds.length) {
            const c = this.tabs.get(this.orderedIds[j]);
            if (sameHost(c) && c!.kind !== null && c!.kind !== "interactive") j++;
            else break;
          }
          this.orderedIds.splice(j, 0, tab.sessionId);
          return;
        }
      }
      this.orderedIds.push(tab.sessionId);
      return;
    }
    // 交互 tab：追加，再把**真孤儿** bg 子项拉到身后（保持原相对序）。
    // 已紧跟在先到宿主（同 cwd/origin 交互 tab）之后的 bg 子串不动——
    // 计划契约"多宿主取第一个"（审计 D-R3：第二个同 cwd 交互会话不许搬走
    // 第一个宿主已挂好的子树）。
    this.orderedIds.push(tab.sessionId);
    if (tab.cwd) {
      const orphans: string[] = [];
      let anchored = false; // 当前扫描位置是否处于"sameHost 宿主的 bg 子串"内
      for (const sid of this.orderedIds) {
        if (sid === tab.sessionId) continue;
        const t = this.tabs.get(sid);
        const isBg = !!t && t.kind !== null && t.kind !== "interactive";
        if (!isBg) {
          anchored = sameHost(t) && (t!.kind === null || t!.kind === "interactive");
          continue;
        }
        if (sameHost(t)) {
          if (!anchored) orphans.push(sid);
          // anchored 保持——宿主的 bg 子串延续
        } else {
          anchored = false; // 异族 bg 打断子串
        }
      }
      if (orphans.length) {
        this.orderedIds = this.orderedIds.filter((sid) => !orphans.includes(sid));
        const at = this.orderedIds.indexOf(tab.sessionId) + 1;
        this.orderedIds.splice(at, 0, ...orphans);
      }
    }
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
