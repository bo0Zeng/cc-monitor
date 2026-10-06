/**
 * 〔拆 `tabs.ts` ③〕**实时流视图** —— 每个 tab 的那条流怎么建、怎么按 seq 门控建卡、
 * 尾部窗口 / 骨架 / 上翻补批 / 哨兵 / 大纲、重放批的开与收。
 *
 * 读写的会话状态全在 `TabStore`（同一个实例）；要别处做的事只有六样（`TabStreamHost`）：
 * 按偏移取回的历史行走 `onLine` 全套 · 刷 tab 栏（立即 / 帧末合批）· 应用 ai-title ·
 * 真用户输入上屏时的自动跟随 · 分叉出来的新会话怎么起。
 *
 * 方法体逐字从 `tabs.ts` 搬来（原是 `TabManager` 的私有方法，或 `ensureTab` / `closeTab` /
 * `switchTo` / `onBatchEnd` / `onLine` 里的整段），唯一的改写：上面六样换成 `this.host.…`，
 * 三个静态常量的类名换成本类。`replay-tail-keep.vitest.ts` 按原文抽 `MATERIALIZE_TAIL_K` 与
 * `MATERIALIZE_ROUNDS_PER_CALL`（原先是 `materializeUntilFilled` 循环里的字面量 `4`）对拍 Rust 侧的 `REPLAY_TAIL_KEEP`。
 */
import { MessageStream } from "./stream";
import { reconcilePendingToolResults, type RenderContext } from "./cards";
import { BranchFolder } from "./branch-fold";
import { attachBranchButton } from "./branch-button"; // G4：实时会话的分叉入口
import type { BranchResult } from "./session-writes";
import type { JsonlLinePayload } from "./events";
import { RecordTimeline } from "./record-timeline";
import { SeqSet, TailWindow, type SkeletonLedger, type TakeBudget } from "./live-window";
import { HeightRefiner, workerMeasure } from "./height-refiner";
// 〔骨架〕骨架层（占位 ＋ 只物化可见区）。接入点全部带「骨架」字样，搜得到。
import { SkeletonView, ledgerFromIndex } from "./skeleton-view";
import { eagerBodyChars, setInjectedShown, skeletonKind } from "./height-estimate";
// K-R45 乙（`KR45D2`）：「大纲」。界面 / 跳 与历史查看器共用同一份；清单问后端要（`OutlineSource`）。
// 大纲并进会话内查找面板（`SessionFindPanel`：搜索 / 大纲两个模式，跳只有一个住址）。
import type { UserInputPanel, JumpResult } from "./views/user-input-panel";
import { OutlineSource, outlineSeedFromIndex } from "./views/outline-source";
import { SessionFindPanel } from "./views/session-find";
import { TurnFold, injectedShownDefault, processExpandedDefault, setInjectedShownDefault, setProcessExpandedDefault } from "./turn-fold";
import { TurnRail } from "./turn-rail";
import { dispatcher, KeybindingDispatcher } from "./keybindings/registry";
import type { MenuItem } from "./kit/menu";
// ⚠ **实时窗口 import 历史查看器，方向是别扭的 —— 这是写区逼出来的将就，不是惯例。**
// 共用的只有 `revealCard`（找卡→展开→滚，两条路的卡由同一份渲染器建）。把它搬进中立文件
// 要同时改 `src/frontend/shell/src/polling_registry.rs` 的调度点分类账（rAF/setTimeout 按文件精确对账），
// 而 `src/frontend/shell/` 不在本轮写区 —— 实测搬了就红。理由与读数在 `revealCard` 的头注 + 件 `§5.6`。
// 这条原住 `tabs.ts`，随 `revealCard` 的唯一用处（大纲的「跳」）一起搬来。
import { revealCard } from "./views/session-viewer";
import {
  renderContentRecord,
  routeMetaAndBranch,
  type MetaSink,
  type StreamSink,
} from "./render-stream-record";
import type { BranchRecord } from "./branching";
import { releaseEnhanceRoot } from "./render";
import { remaining } from "../../comms/inward/chan";
import { budgetWithin } from "./ipc/chan-caller";
import { findInSession, readSessionIndex } from "./session-reads";
import { readLines, readRange } from "./record-reads";
import type { Tab } from "./tab-model";
import { isResumeOnly } from "./tab-session-state";
import type { TabStore } from "./tab-store";
import { copyText } from "./copy-table";

/** 只问「这条是不是 meta」、不喂任何账的空 sink（骨架按偏移取回**见过**的行时用）。 */
const NOOP_META: MetaSink = { onBranchRecord: () => {}, onQueueOperation: () => {} };

/** 流视图要宿主做的六件事（全是回调；状态本身在 `TabStore`）。 */
export interface TabStreamHost {
  /** 按偏移取回的「没见过」的历史行：走 `onLine` 全套（去重、旁路记账、门控）。 */
  onLine(payload: JsonlLinePayload): void;
  /** 立即整刷 tab 栏（批末 flush 中部缓冲后，攒下的未读徽标一次刷新）。 */
  refreshTabBar(): void;
  /** 帧末合批刷 tab 栏（live 路上后台 tab 来一行 ⇒ 未读 ＋1）。 */
  scheduleTabBarRefresh(): void;
  applyAiTitle(tab: Tab, aiTitle: string): void;
  /** 真用户输入上屏 ⇒ 自动跟随（路由那一半）。 */
  userActive(sessionId: string): void;
  /** G6：分叉产出新会话文件之后 —— 起它。 */
  startForkedSession(tab: Tab, res: BranchResult): Promise<void>;
}

/** `mountTabDom` 建出来、要写进 `Tab` 的那几样。 */
export interface TabStreamDom {
  streamEl: HTMLElement;
  stream: MessageStream;
  branchFolder: BranchFolder;
  timeline: RecordTimeline;
  inputsEl: HTMLElement;
  inputsPanel: UserInputPanel;
  outline: OutlineSource;
  turnFold: TurnFold;
  turnRail: TurnRail;
}

/**
 * 秤 6(表第 6 行):读 `BranchFolder.records` 的**条数**,给 `debugSnapshot`。
 *
 * # 为什么是按结构读,不是加一个 getter
 *
 * `records` 是 `BranchFolder` 的 private 字段,而本轮的写区**不含** `branch-fold.ts`
 * (同一棵树上还有别路 agent 在写)。TS 的 `private` 只活在编译期,运行时它就是个普通
 * 字段 ⇒ 这里按名字读一次。**只给 DEV 探针用,无副作用、不改任何行为。**
 * 哪天 `branch-fold.ts` 可写了,把这里换成一个 `get recordCount()` 是纯收窄。
 *
 * # 读不到时返 -1,不返 0
 *
 * 字段一旦改名,返 0 会被读成「**账本是空的**」—— 那是一句假话,而且是**朝着"看起来
 * 一切正常"的方向**假(同篇 `§6` 反复点名的那一族:坏掉的尺子把真缺陷一起藏起来)。
 * 返 -1 在读数里一眼就是「这根尺子断了」。`tests/frontend/ui/scale6-memory-ledger.vitest.ts`
 * 有一格专钉「它不许是 -1」。
 *
 * # 它量的是条数,不是字节
 *
 * 一条 `BranchRecord` 是 `{uuid, parentUuid, timestamp}` 三个短字符串,**不含正文** ——
 * 这正是下面那句判词的一半依据。想要字节得另外称,本秤不称。
 */
function branchRecordCount(folder: BranchFolder): number {
  const inner = folder as unknown as { records?: unknown };
  return Array.isArray(inner.records) ? inner.records.length : -1;
}

export class TabStreamView {
  /** Batch13-F40a:物化/后台 tab 尾段条数(与 F39 viewer TAIL_INITIAL 同语义) */
  private static readonly MATERIALIZE_TAIL_K = 150;
  /** `materializeUntilFilled` **一次同步调用**最多跑几轮（原 `round < 4` 的那个 4；它不再是「到此为止」，没满就下一帧接着补）。 */
  private static readonly MATERIALIZE_ROUNDS_PER_CALL = 4;
  /** F40b:上翻补批批量/触发距离(沿用 F39 实测值) */
  private static readonly FILL_BATCH = 200;
  /**
   * 一批（物化尾段 / 上翻补批）的第二道闸：急路要当场物化的正文字符（`eagerBodyChars`）。
   * 64 Ki 字符：W5-RENDER 普查按文件序连续 150 条窗口的正文字符 p50 6.5 K、p99 136 K ⇒ 常态批碰不到它，只截那几份长尾批
   * （一条 617 KB 正文的 assistant 就是一整批）。截下来的下一帧接着补（`materializeUntilFilled` / `fillAbove` 的 rAF 自链）。
   */
  private static readonly BATCH_BODY_CHARS = 64 * 1024;
  private static readonly BATCH_BUDGET: TakeBudget = {
    weight: (p) => eagerBodyChars(p.message),
    max: TabStreamView.BATCH_BODY_CHARS,
  };
  private static readonly TOP_TRIGGER_PX = 800;
  /**
   * 往上翻那一问的期限：60 秒 —— 与它上一个住址（monitor `frame_query::PAGE_BUDGET`，
   * 一次 `read_session_lines` 各拿一份）同值。一件一问。
   */
  private static readonly BELOW_BUDGET_MS = 60_000;
  /**
   * 会话流丢格之后「往后补到末尾」那一**件**的总期限：120 秒（= 一次性远端那一趟的天花板
   * `dial_host::ONE_SHOT_DEADLINE`，monitor 那一侧读整段 `frame_query::READ_LINES_BUDGET` 同值）。
   * 开头造一次，之后每一问交剩下的（`remaining`）—— 不再每页各拿一整份（那一形遇上一页一页慢慢吐的对端停不下来）。
   */
  private static readonly GAP_FILL_BUDGET_MS = 120_000;
  /** F40b:补批防重入(补偿测量期间嵌套触发会算错差值) */
  private renderingFill = false;
  /** 每个 tab 的查找面板（关 tab 时摘掉）。`Tab` 上挂的是它的两半：`inputsEl`（整块）与 `inputsPanel`（大纲）。 */
  private readonly finds = new Map<string, SessionFindPanel>();
  /**
   * 每个 tab 在途的「按偏移取正文」（`fetchMissingRows` 发的那几趟）。
   * 「跳」要等它们落完再找卡 —— 骨架接上之后，没物化的那段正文多半不在前端账本里（U3b `keepHighest`），
   * `ensure` 只是把取正文的请求发出去，同步那一下去找卡必然落空。
   */
  private readonly rangeFetches = new WeakMap<Tab, Set<Promise<void>>>();

  constructor(
    private readonly store: TabStore,
    private readonly streamRootEl: HTMLElement,
    private readonly host: TabStreamHost,
    /** 第二级估高（Worker 精算）；环境里没有 Worker 时它自己不开。 */
    private readonly refiner: HeightRefiner = new HeightRefiner(workerMeasure()),
  ) {
    this.applyInjected(injectedShownDefault()); // 「显示系统注入」这扇窗上次的样子
  }

  /** 视口上下几屏之内的占位行交第二级（「窗口附近上下各 N 屏优先精算」）。 */
  private static readonly REFINE_SCREENS = 2;
  /** 一趟最多精算几行（视口里估高荒谬地偏小时，一屏装下几千行也只交这么多）。 */
  private static readonly REFINE_MAX_ROWS = 200;
  /** 每个骨架问过第二级的行（问过就不再问：排不出 / 不值得精算的也不重问）与在途标记。 */
  private readonly refineAsked = new WeakMap<SkeletonView, Set<number>>();
  private readonly refining = new WeakSet<SkeletonView>();

  /**
   * 〔「列宽变化只重算已精算过的」〕「列宽变了」：消息流尺寸变了（`MessageStream.onViewportResize`）时
   * 现量这一列（`.stream-content`）有多宽，与骨架账本当前那一列差出 1px ⇒ 账本按新列宽重估、占位改高（`SkeletonView.relayout`），
   * 作废的精算行重交第二级。量不到宽（tab 还没布局）⇒ 不动。
   */
  private relayoutOnColumnChange(tab: Tab): void {
    const sk = tab.skeleton;
    if (!sk) return;
    const w = tab.stream.contentElement.getBoundingClientRect().width;
    if (w > 0 && sk.relayout(w)) this.refineNearby(tab);
  }

  /**
   * 〔第二级「按需 ＋ 后台」〕视口上下 `REFINE_SCREENS` 屏之内还在占位里、没精算过的行：
   * 正文先从账本借（不出账），没有的按索引字节边界取一次（与 `fetchMissingRows` 同一条命令，回来的只交 Worker、不建卡），
   * 交 `HeightRefiner`，回来换进账本、占位改高（视口钉住）。一个骨架同时只一趟。
   */
  private refineNearby(tab: Tab): void {
    const sk = tab.skeleton;
    if (!sk || !this.refiner.enabled || this.refining.has(sk) || !tab.parentPath) return;
    let asked = this.refineAsked.get(sk);
    if (!asked) this.refineAsked.set(sk, (asked = new Set()));
    // 列宽变了作废的精算行排在前面（不论离视口多远）；一趟交不完的留在骨架里等下一趟
    const stale = sk.takeStale(TabStreamView.REFINE_MAX_ROWS);
    const mine = new Set(stale);
    const fresh = sk.nearbyUnrefined(TabStreamView.REFINE_SCREENS).filter((s) => !asked!.has(s) && !mine.has(s));
    const want = [...stale, ...fresh];
    if (want.length === 0) return;
    const seqs = new Set(want.slice(0, TabStreamView.REFINE_MAX_ROWS));
    for (const s of seqs) asked.add(s);
    const taken = [...seqs];
    const known = tab.window.peekSeqs(seqs);
    for (const p of known) seqs.delete(p.seq);
    const runs: Array<[number, number]> = [];
    for (const s of [...seqs].sort((a, b) => a - b)) {
      const f = sk.ledger.factsOf(s);
      if (!f || skeletonKind(f) !== "card") continue;
      const last = runs[runs.length - 1];
      if (last && last[1] === s) last[1] = s + 1;
      else runs.push([s, s + 1]);
    }
    const origin = tab.origin;
    const jsonlPath = tab.parentPath;
    this.refining.add(sk);
    const fetched = runs.map(([a, b]) => {
      const first = sk.ledger.factsOf(a)!;
      const lastRow = sk.ledger.factsOf(b - 1)!;
      return readRange(origin, jsonlPath, first.o, lastRow.o + lastRow.n, a).catch((): JsonlLinePayload[] => []);
    });
    void Promise.all(fetched)
      .then((pages) => {
        if (this.store.tabs.get(tab.sessionId) !== tab || tab.skeleton !== sk) return;
        const rows = [...known, ...pages.flat()].map((p) => ({ seq: p.seq, rec: p.message }));
        return this.refiner.refine(sk, rows).then((applied) => {
          // 算的途中列宽变了 ⇒ 这一批作废，放回待重交
          if (!applied) sk.returnStale(taken);
        });
      })
      .catch((e: unknown) => console.warn(`[tabs] 第二级估高失败（${tab.sessionId.slice(0, 8)}）：`, e))
      .finally(() => {
        this.refining.delete(sk);
        // 还有待重交的（在途时列宽又变了 / 一趟没交完）⇒ 接着交
        if (sk.staleCount > 0 && this.store.tabs.get(tab.sessionId) === tab && tab.skeleton === sk) this.refineNearby(tab);
      });
  }

  /**
   * 建一个 tab 的流 DOM：`.stream` 容器 ＋ 消息流 ＋ 折叠层 ＋ 时间线 ＋ 查找面板（含大纲）与大纲的数据源。
   * 重放期建的新 tab 顺带进 batch 模式。原是 `ensureTab` 里的一段（逐字）。
   */
  mountTabDom(sessionId: string): TabStreamDom {
    const streamEl = document.createElement("div");
    streamEl.className = "stream"; // 默认 .stream 已含 visibility:hidden（见 styles.css）
    this.streamRootEl.appendChild(streamEl);

    const stream = new MessageStream(streamEl);
    const branchFolder = new BranchFolder(stream.contentElement);
    const timeline = new RecordTimeline(stream);

    // 本 tab 的查找面板：搜索 ／ 大纲两个模式，**跳只有一个住址**（`jumpInTab`）。宿主自己的三件事：
    // ① 怎么查 —— 问后端（经通道直接说帧命令 `history-find`，`session-reads.ts`），问的是这个 tab 的那份会话；
    // ② 怎么跳 —— 见 `jumpInTab`（没加载的那一段先取回来再跳）；
    // ③ 落空怎么说 —— 取回来了也找不到那张卡（被并进别的卡 / 渲染成空）。
    const find = new SessionFindPanel({
      search: async (query, includeTools, skip) => {
        const t = this.store.tabs.get(sessionId);
        if (!t?.parentPath) {
          return { available: false, reason: copyText("tabStreamView.search.noFile"), hits: [], total: 0 };
        }
        return findInSession(t.origin, t.parentPath, query, includeTools, skip);
      },
      jumpTo: (uuid) => this.jumpInTab(sessionId, streamEl, uuid),
      unjumpableHint: copyText("tabStreamView.search.notLoaded"),
    });
    this.finds.set(sessionId, find);
    const inputsEl = find.el;
    const inputsPanel = find.outline;
    this.streamRootEl.appendChild(inputsEl);
    // 大纲的数据源：路径可能要等首条行回填（骨架 tab），所以每次要的时候现取
    const outline = new OutlineSource(inputsPanel, () => {
      const t = this.store.tabs.get(sessionId);
      return t?.parentPath ? { origin: t.origin, jsonlPath: t.parentPath } : null;
    });
    // 按轮折叠：一轮的边界与结论问后端（`history-turns`），路径同大纲每次现取。
    const turnFold = new TurnFold(stream.contentElement, streamEl, () => {
      const t = this.store.tabs.get(sessionId);
      return t?.parentPath ? { origin: t.origin, jsonlPath: t.parentPath } : null;
    });
    // 轮次刻度：同一份轮；跳与查找 / 大纲同一个住址（`jumpInTab`）。挂在流外（不随流滚），随 tab 同进同出。
    const turnRail = new TurnRail(streamEl, stream.contentElement, {
      turns: () => turnFold.all,
      waiting: () => this.store.tabs.get(sessionId)?.needs != null,
      jump: (uuid) => void this.jumpInTab(sessionId, streamEl, uuid),
    });
    turnFold.onTurns = () => turnRail.render();
    this.streamRootEl.appendChild(turnRail.el);
    // v2.2 issue #12: 重放期创建的新 Tab 也进 batch 模式，避免每条 record 都
    // 触发 O(N) computeMainBranch。批结束时 onBatchEnd 会统一 flush。
    if (this.store.inBatch) {
      branchFolder.setBatchMode(true);
    }
    return { streamEl, stream, branchFolder, timeline, inputsEl, inputsPanel, outline, turnFold, turnRail };
  }

  /** 给刚建好的 tab 挂上翻补批的滚动监听与视口变大的补批（原是 `ensureTab` 里的一段，逐字）。 */
  wireTab(tab: Tab): void {
    const sessionId = tab.sessionId;
    const streamEl = tab.streamEl;
    const stream = tab.stream;
    // F40b:上翻补批触发器(passive 只读滚动位置;handler 内判 active,后台 tab
    // 的程序化滚动/尺寸变化不触发补批)
    const fillHandler = (): void => {
      if (this.store.activeId !== sessionId) return;
      const t = this.store.tabs.get(sessionId);
      // 〔骨架〕接上了 ⇒ 占位可能在任何位置（拖滚动条到中部），**每次滚动**都看一眼
      // 视口里有没有占位 —— 不能沿用「离顶 800px 内才补」那道门（那是尾部窗口单洞后缀的假设）
      if (t?.skeleton) {
        t.skeleton.fillVisible();
        this.refineNearby(t); // 第二级：视口附近的占位行交 Worker 精算
        return;
      }
      if (t && t.streamEl.scrollTop <= TabStreamView.TOP_TRIGGER_PX) this.fillAbove(t);
    };
    streamEl.addEventListener("scroll", fillHandler, { passive: true });
    // 收尾时视口落在过程里、先没收的那一轮：滚出去了再收（同一个监听，摘的时候一起摘）。
    streamEl.addEventListener("scroll", tab.turnFold.releaseOnScroll, { passive: true });
    streamEl.addEventListener("scroll", tab.turnRail.onScroll, { passive: true }); // 刻度上「当前」那一格
    tab.fillHandler = fillHandler;
    // ★ 步 3：**视口自己变大 ⇒ 重新补批。**
    //
    // `fillHandler` 挂在 scroll 上，而**不可滚的元素根本不产生 scroll 事件** ——
    // 把窗口从半屏拉到全屏时，多出来的那块空白之前没有任何入口去填。
    // `MessageStream` 那边现在也观察 `scrollEl`（见 stream.ts），这里是它的消费端。
    // ⚠ 三道门都不可少：① 只给 active tab 补（后台 tab 0×0 → 真实尺寸那一跳不是
    // 「用户拉窗口」）；② 账本空了不补；③ 已经满屏了不补（否则每次 RO 都白干一轮）。
    stream.onViewportResize = (): void => {
      // 列宽可能变了：骨架账本按新列宽重估（后台 tab 同样有布局宽，一并跟上）
      const cur = this.store.tabs.get(sessionId);
      if (cur) this.relayoutOnColumnChange(cur);
      cur?.turnRail.render(); // 列窄到 900 以下不出刻度
      if (this.store.activeId !== sessionId) return;
      const t = this.store.tabs.get(sessionId);
      if (!t || t.window.pendingCount === 0) return;
      // 〔骨架〕接上了 ⇒ 视口变大露出的是占位，只物化露出来的那段
      if (t.skeleton) {
        t.skeleton.fillVisible();
        return;
      }
      if (this.contentReachesBottom(t)) return;
      this.materializeUntilFilled(t);
      this.updateSentinel(t);
    };
  }

  /** 关 tab 时拆它的流 DOM、断大对象引用、摘监听（原是 `closeTab` 里的一段，逐字）。 */
  disposeTab(tab: Tab): void {
    releaseEnhanceRoot(tab.streamEl); // 本 tab 那一个 IO 断开
    tab.stream.dispose();
    tab.streamEl.remove();
    // K-R45 乙：大纲跟着走（`reset` 也让在途那趟回来后不许回写）。
    // 查找面板是 `streamRootEl` 的直接子节点，不随 `streamEl.remove()` 一起走。
    tab.outline.reset();
    tab.facts.reset(); // 会话事实同理：在途那趟回来后不许回写
    tab.streamEl.removeEventListener("scroll", tab.turnFold.releaseOnScroll);
    tab.streamEl.removeEventListener("scroll", tab.turnRail.onScroll);
    tab.turnRail.el.remove(); // 刻度挂在流外，不随 `streamEl.remove()` 一起走
    tab.turnFold.dispose(); // 按轮折叠：断观察、在途那趟作废
    this.finds.get(tab.sessionId)?.reset(); // 在途的查找作废、出弹层栈
    this.finds.delete(tab.sessionId);
    tab.inputsEl.remove();
    // 显式清 Map：释放对已卸载 DOM 节点的强引用，让 GC 可早回收
    // （Map 本身也会随 Tab 对象一起回收，但显式 clear 让 DOM 引用计数立即归零）
    tab.toolUseNames.clear();
    tab.toolUseElements.clear();
    tab.runCards.clear();
    tab.pendingToolResults.clear();
    tab.seenSeqs.clear();
    // F40a/b:窗口账本与缓冲持整段历史 payload(大会话数十 MB 级),断引用;摘 fill listener
    tab.window.dispose();
    tab.skeleton?.dispose(); // 〔骨架〕
    tab.skeleton = null;
    tab.midBatchBuffer = [];
    if (tab.fillHandler) tab.streamEl.removeEventListener("scroll", tab.fillHandler);
    tab.timeline.dispose();
    tab.branchFolder.dispose();
  }

  /**
   * 记录文件从头重读了（后端行号从 0 重数）⇒ 这个 tab 的内容整份重来：拆掉流 DOM 与全部账本
   * （时间线 · 去重集 · 尾部窗口 · 骨架 · 大纲 · 查找面板 · 会话事实），按新建的样子再装一份；身份 / 标题 / 状态 / 固定照留。
   * **换一个新的 `Tab` 对象进表**：在途那几趟（骨架索引 · 按偏移取正文 · 往下 / 往后补）回来时认的是「表里还是不是它」，
   * 认不出就自己作废 —— 旧的一代的行不会落进新的一代。
   */
  restartContent(old: Tab): Tab {
    const wasActive = this.store.activeId === old.sessionId;
    this.disposeTab(old);
    const tab: Tab = {
      ...old,
      ...this.mountTabDom(old.sessionId),
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      pendingToolResults: new Map(),
      seenSeqs: new SeqSet(),
      window: new TailWindow(),
      skeleton: null,
      skeletonFetch: "idle",
      midBatchBuffer: [],
      fillHandler: null,
    };
    this.store.tabs.set(tab.sessionId, tab);
    this.wireTab(tab);
    if (wasActive) this.showOnly(tab.sessionId);
    return tab;
  }

  /** 切 tab：只让这一条流（连同它的查找面板）可见（原是 `switchTo` 开头那一段）。 */
  showOnly(sessionId: string): void {
    for (const [sid, t] of this.store.tabs) {
      t.streamEl.classList.toggle("active", sid === sessionId);
      // K-R45 乙：面板与它那条流**同进同出**。漏掉这一句 = 所有 tab 的面板
      // 一起挂在屏幕上，而且点下去找的是别人的流（`revealCard` 只在自己的 streamEl 里找）。
      t.inputsEl.classList.toggle("active", sid === sessionId);
      // 切走的 tab 收起面板（出弹层栈）—— 不然 Esc 去关的是一块看不见的面板。
      if (sid !== sessionId) this.finds.get(sid)?.close();
      if (sid !== sessionId) t.turnFold.release(true); // 先没收的那一轮：切走了就收
      t.turnRail.el.classList.toggle("active", sid === sessionId);

    }
  }

  /**
   * Ctrl+F（动作 `session.find`）：当前 tab 的查找面板打开到「搜索」、焦点进输入框。
   * 没有 active tab ⇒ 什么都不做。
   */
  openFind(): void {
    const sid = this.store.activeId;
    if (sid === null) return;
    this.finds.get(sid)?.open("search");
  }

  /**
   * **跳（大纲行、查找命中行与轮次刻度共用这一个住址）**：
   * - 这条已经建了卡 ⇒ 直接找卡（`revealCard`）；
   * - 骨架接上了、还在占位里 ⇒ `ensure` 物化它附近那一段；账本里有的当场建卡，没有的按偏移取回（`fetchMissingRows`）——
   *   **等这个 tab 在途的取正文全部落完**再找卡；取失败 ⇒ reject 带原因（那一行写原因 ＋［重试］）；
   * - 骨架没接、还收在尾部窗口里 ⇒ 从它往下整段建卡（尾部窗口只认后缀），再找卡；
   * - 都不是（骨架还没到）⇒ 踢一次要骨架、reject「索引未就绪」（重试时多半已经接上）。
   * 等的期间 tab 被关掉 ⇒ 落空（`null`）。
   */
  private jumpInTab(sessionId: string, streamEl: HTMLElement, uuid: string): JumpResult {
    const tab = this.store.tabs.get(sessionId);
    if (!tab) return null;
    const sk = tab.skeleton;
    const seq = sk?.ledger.uuidToSeq.get(uuid);
    if (sk && seq !== undefined && sk.isPending(seq)) {
      sk.ensure(seq);
      const inflight = this.rangeFetches.get(tab);
      if (!inflight || inflight.size === 0) return revealCard(streamEl, uuid);
      return Promise.allSettled([...inflight]).then((done) => {
        if (this.store.tabs.get(sessionId) !== tab) return null;
        const el = revealCard(streamEl, uuid);
        const failed = done.find((r): r is PromiseRejectedResult => r.status === "rejected");
        if (!el && failed) throw failed.reason instanceof Error ? failed.reason : new Error(String(failed.reason));
        return el;
      });
    }
    const built = revealCard(streamEl, uuid);
    if (built || sk) return built;
    const pending = tab.window.peek(tab.window.pendingCount);
    const at = pending.findIndex((p) => (p.message as { uuid?: unknown }).uuid === uuid);
    if (at >= 0) {
      this.renderPayloadsBatch(tab, tab.window.takeTail(pending.length - at));
      this.updateSentinel(tab);
      return revealCard(streamEl, uuid);
    }
    this.requestSkeleton(tab);
    return Promise.reject(new Error(copyText("tabStreamView.search.notReady")));
  }

  /** 切进来的 tab：物化 / 哨兵 / 骨架索引 / 大纲 / 不可滚时踢一次补批（原是 `switchTo` 中段，逐字）。 */
  activate(next: Tab): void {
    // 上次按行号往下问失败了的，切进来时允许再问一次（失败不自动重问，见 `BelowState` 头注）。
    if (next) next.window.retryBelow();
    // Batch13-F40a:命中 virgin tab(启动重放全收纳,还没建过卡)→ 同步物化尾段,
    // 避免切过去一片空白(R-3:有界循环补到可滚,防工具密集会话一轮近空屏)。
    // 非 virgin tab 不动(上翻补批属 F40b;它可能有滚动位置要保,走下面那一脚带补偿的 `fillAbove`)。
    const virginFill = !!next && next.window.floorSeq === null && next.window.pendingCount > 0;
    if (virginFill) this.materializeUntilFilled(next);
    // F40b:切入即刷新哨兵(非 virgin 但账本非空的 tab 也要见到「还有 N 条」)
    if (next) this.updateSentinel(next);
    // 〔骨架〕切进来的 tab 要索引（上面刚物化过尾段 ⇒ floor 已钉）
    if (next) this.requestSkeleton(next);
    if (next?.outline.needsFetch) this.refreshOutline(next); // 大纲：有新行才要
    // D 审计 R-2:非 virgin + 不可滚 + 账本有余的 tab 没有 fill 入口(不可滚元素
    // 不产生 scroll 事件,哨兵可见却"上翻物理不可达")——切入时踢一次,rAF 自链
    // 接管直到可滚或账尽。
    // 账本空了但下面可能还有（`wantsBelow`）同样没有 scroll 入口 ⇒ 同一脚。
    // 这一脚原来只按 `scrollHeight` 判「不可滚」：没渲染过的卡贡献的是估值，
    // 估值把 `scrollHeight` 撑成「滚得动」时它就不踢 ⇒ 钉过水位、真实只有半屏的 tab 停在半屏，只剩用户往上翻一条路。
    // ⇒ 再问一句真实布局（`contentReachesBottom`）：没满一屏也踢。
    // 上面刚为 virgin tab 跑过 `materializeUntilFilled`、账本还有余的不再踢：它补不满时自己排了
    // 下一帧的接续，这里再同步补一批就把「一次同步调用有界」破了。账本已空（只剩「下面可能还有」）的照旧踢。
    const continuing = virginFill && next.window.pendingCount > 0;
    if (next && !continuing && (next.window.pendingCount > 0 || next.window.wantsBelow)) {
      const el = next.streamEl;
      if (el.scrollHeight - el.clientHeight <= 1 || !this.contentReachesBottom(next)) this.fillAbove(next);
    }
  }

  /**
   * 重放批结束：逐 tab 挂批期缓冲、折叠层 flush、孤儿 tool_result 对账；当前 tab 补到一屏，
   * 其余 virgin 后台 tab 进空闲物化队列（原是 `onBatchEnd` 的主体，逐字）。返回当前 tab。
   */
  batchEnd(): Tab | undefined {
    for (const t of this.store.tabs.values()) {
      // F40b R-1:先把批期缓冲的中部插入一次挂载(内含 unwrapAll/rebuildNow),
      // 再走既有 flushPending/reconcile
      this.flushMidBatchBuffer(t);
      t.branchFolder.flushPending();
      t.branchFolder.setBatchMode(false);
      // 切块场景下，老块的 tool_use 现在已渲染 → 重试匹配早到的 fallback result
      const ctx: RenderContext = {
        parentPath: t.parentPath,
        origin: t.origin,
        toolUseNames: t.toolUseNames,
        toolUseElements: t.toolUseElements,
        runCards: t.runCards,
        pendingToolResults: t.pendingToolResults,
      };
      // S-6:孤儿卡出 DOM 的同时出账,防悬空 anchor
      for (const el of reconcilePendingToolResults(ctx)) t.timeline.removeByElement(el);
    }
    // Batch13-F40a:active tab 不足一屏(或还是 virgin)→ 立即补物化到可见;
    // 其余 virgin 后台 tab 进空闲物化队列(逐个串行,避免并发建卡风暴)。
    const active = this.store.activeId !== null ? this.store.tabs.get(this.store.activeId) : undefined;
    if (active && active.window.pendingCount > 0) {
      // 步 3：「够不够一屏」改读**真实布局**（见 `contentReachesBottom` 的头注）。
      const notFilled = !this.contentReachesBottom(active);
      if (active.window.floorSeq === null || notFilled) {
        this.materializeUntilFilled(active);
        active.stream.scrollToBottom();
      }
    }
    // D 审计 S-5:已结束的死会话不进后台物化队列(纯浪费;switchTo 命中 virgin
    // 已有同步物化兜底)。
    // 原来只收 `floorSeq === null`（virgin）；钉过水位、账本还压着历史、
    // **真实布局**没满一屏的后台 tab 同样进队（后台 tab 是 `visibility:hidden`，几何照在）。
    this.materializeQueue = [...this.store.tabs.entries()]
      .filter(
        ([sid, t]) =>
          sid !== this.store.activeId &&
          !isResumeOnly(t.state) &&
          t.window.pendingCount > 0 &&
          (t.window.floorSeq === null || !this.contentReachesBottom(t)),
      )
      .map(([sid]) => sid);
    this.scheduleIdleMaterialize();
    // F40b:active tab 未走物化分支(尾块已可滚)时也要挂哨兵
    if (active) this.updateSentinel(active);
    // 〔骨架〕active tab 此刻一定有渲染后缀了 ⇒ 要索引（在途/要过就不重复）
    if (active) this.requestSkeleton(active);
    if (active?.outline.needsFetch) this.refreshOutline(active); // 大纲
    // 会话事实：凡是「没要过或又长了」的 tab 都要一次（F5 之后每个 tab 首次整份扫，之后只读新写的一截）。
    for (const t of this.store.tabs.values()) {
      if (t.facts.needsFetch) void t.facts.refresh();
    }
    // 按轮折叠：每个 tab 要一次（续取从还没收尾的那一轮起）。
    for (const t of this.store.tabs.values()) void t.turnFold.refresh();
    return active;
  }

  /**
   * 一条已经过了去重、事实也记过了的记录：喂 meta / 分支账，按 seq 门控决定建卡还是收纳，
   * 建卡了而不是当前 tab ⇒ 未读 ＋1（帧末合批刷徽标）。原是 `onLine` 的后半（逐字）。
   */
  ingest(tab: Tab, payload: JsonlLinePayload): void {
    const sink: StreamSink = {
      timeline: tab.timeline,
      onBranchRecord: (rec: BranchRecord) => tab.branchFolder.recordAdded(rec),
      // issue #36：队列消息内容 → 折叠豁免集合
      onQueueOperation: (content: string) => tab.branchFolder.addQueuedContent(content),
      onTitleUpdate: (title: string) => this.host.applyAiTitle(tab, title),
      onRealUserInput: (sid: string) => {
        this.host.userActive(sid);
        this.refreshOutline(tab); // 真用户输入上屏 ⇒ 大纲要新的一截
      },
      enhanceRoot: this.store.inBatch ? tab.streamEl : null, // 批期 lazy ⇒ 交本 tab 的滚动容器
      // G4（branch-anywhere）：实时会话也挂「从这一轮分叉」按钮。
      // 钩子本来就在共享的 `render-stream-record.ts` 里，此前**只有历史查看器传了它**
      // ⇒ 实时 tab 上没有入口。按钮本体是共享组件（off-main 的呈现区分也在那里）。
      // **G6 起远端也挂**；本机那条也收 sid 之后，
      // 「本机拿不到 jsonl 路径」这道门对两条路都不再是门槛。
      onCardRendered: (el, msg) => {
        if (msg.type !== "user" && msg.type !== "assistant") return;
        if (!msg.uuid) return;
        attachBranchButton(el, {
          uuid: msg.uuid,
          sourceSessionId: tab.sessionId,
          origin: tab.origin,
          cwd: tab.projectDir ?? undefined,
          onForked: (res) => void this.host.startForkedSession(tab, res),
        });
      },
    };

    // Batch13-F40a:meta/branch 收集与渲染解耦——收纳(不建卡)的记录也要喂
    // title/queue/branch 数据(routeMetaAndBranch 是两条路径的单一来源,账本 §3)。
    if (routeMetaAndBranch(payload, sink) === "consumed") return;

    // 门控(单洞后缀不变量,纯 seq 判定):
    // - virgin + batch:active tab 首条 content 钉 floor(尾块直渲,进步式首屏
    //   与 deferMode 时代一致);后台 tab 恒收纳(virgin,批后空闲物化)。
    // - virgin + live:新开 tab 直渲并钉 floor。
    // - seq >= floor:渲染(live 追加/尾块);seq < floor:收纳(旧块/F30 尾部
    //   优先回填/迟到块——无论 inBatch,与 batch 哨兵解耦)。
    // D 审计 C-1(近 virgin 竞态):批后 rIC 队列还没轮到该 tab 就来了真 live 行——
    // 若直接 pinFloor(新行 seq),账本里整段历史会被钉死滞留(F40a 无再物化入口)。
    // 先物化尾段(takeTail 顺带钉 floor),live 行(seq 恒更新)再照常 admit。
    if (tab.window.floorSeq === null && !this.store.inBatch && tab.window.pendingCount > 0) {
      this.materializeTail(tab);
    }
    const floor = tab.window.floorSeq;
    let render: boolean;
    if (floor === null) {
      render = !this.store.inBatch || this.store.activeId === tab.sessionId;
      if (render) tab.window.pinFloor(payload.seq);
    } else {
      render = tab.window.admit(payload.seq);
      // 〔骨架〕floor 之下、但已被物化过的那段（岛）里迟到的行 ⇒ 就地建卡，不收纳
      if (!render && tab.skeleton && !tab.skeleton.isPending(payload.seq)) render = true;
    }
    // F40b R-1:批期落在渲染窗口内的**中部**插入(seq≥floor 且 <已渲染最高 seq
    // ——大增量批的老块)→ 缓冲,onBatchEnd 一次挂载,消逐帧上方插入(§21)。
    // 离线期真新消息:unread 照计(粒度=记录,与逐条渲染的 inserted 判定在
    // tool-group 合并上略有偏差,99+ 封顶下可接受)。
    if (render && this.store.inBatch && payload.seq < tab.timeline.maxSeq) {
      tab.midBatchBuffer.push(payload);
      // 徽标刷新攒到批末 flush 一次(D 审计:600 条缓冲 = 600 次全 bar 巡检)
      if (this.store.activeId !== tab.sessionId) tab.unread += 1;
      return;
    }
    if (!render) {
      tab.window.defer(payload);
      // 仪表(jsdom 单测无 main.ts,须防 undefined)
      if (window.__ccmPerf) window.__ccmPerf.recordsDeferred = (window.__ccmPerf.recordsDeferred ?? 0) + 1;
      // 收纳不计 unread——重放历史不是"未读新消息"(修 backlog S-2)
      return;
    }

    const ctx: RenderContext = {
      parentPath: tab.parentPath,
      origin: tab.origin,
      toolUseNames: tab.toolUseNames,
      toolUseElements: tab.toolUseElements,
      runCards: tab.runCards,
      pendingToolResults: tab.pendingToolResults,
      needs: tab.needs, // 在等批准的那一步建出来就画成「在等你批准」
      // P5.5：batch 期间走 lazy hljs（代码块占位 + IntersectionObserver 触发再补跑）
      lazy: this.store.inBatch,
    };
    const beforeSize = tab.timeline.size;
    renderContentRecord(payload, ctx, sink);
    const inserted = tab.timeline.size > beforeSize;

    // unread 计数：只有真新 entry 入 timeline 才算（tool-group 合并到旧 group 不算）
    if (inserted && this.store.activeId !== tab.sessionId) {
      tab.unread += 1;
      // ★ F15：**帧末合批**，不是逐行整刷。
      // 这里是 live 路上每来一行都会走到的地方，而 `refreshTabBar` 是整条 bar 的重刷；
      // 徽标上的数字攒到帧末一次性更新，用户看到的结果一模一样。
      // ⚠ 只合批**这一处**。这里原先的理由是「其余十几个 `refreshTabBar()` 调用点都是用户动作」——
      // **事实不对**：UP1 现数 25 处同步整刷，其中 12 处是事件驱动的（状态帧 / 会话增删 / 账号变更…），不是用户点的。
      // 结论不变（其余照旧同步整刷、不合批），理由换成现在这条：UP1 之后整刷对**没变的 tab 零 DOM 写**
      // （`tab-bar-view.ts` 先算出要画成什么样、与上次画出去的一样就不写， / P7），
      // ⇒ 一次整刷的代价只剩「变了的那几颗按钮」，不合批也不贵；合批反而会把「点完立刻看到」变成「下一帧才看到」。
      // 这一处还合批，是因为它**每来一行 live 记录就走一次**（频率是逐行的，不是逐事件的）。
      this.host.scheduleTabBarRefresh();
    }
  }

  /**
   * 一条新记录到了 —— 大纲与会话事实各记一笔「又长了」（O(1)，**不读记录**：判不判、算什么都在后端）。
   * - 大纲：本 tab 是 active、非批期、而且**还一次都没要过**（首个 tab 建出来时路径可能还没到）⇒ 要一次；
   * - 会话事实：非批期 ⇒ 要（不只 active：分叉 `↳` 在 tab 栏上、监控板每格都显示 context% 与 agent 数）；
   *   在途时再叫只并成一趟（`FactsSource.refresh`），带着上一份成品只读新写的那一截。批期不要 —— 批结束统一要（`batchEnd`）。
   */
  noteGrew(tab: Tab): void {
    tab.outline.markStale();
    tab.facts.markStale();
    if (!tab.outline.everFetched && !this.store.inBatch && this.store.activeId === tab.sessionId) {
      void tab.outline.refresh();
    }
    if (!this.store.inBatch) void tab.facts.refresh();
    if (!this.store.inBatch) void tab.turnFold.refresh();
  }

  /** `Ctrl+O` · 会话头开关：过程默认展开与否（每扇窗一份，所有 tab 一起换）。 */
  toggleProcessExpanded(): void {
    const on = !processExpandedDefault();
    setProcessExpandedDefault(on);
    for (const t of this.store.tabs.values()) t.turnFold.setDefault(on);
  }

  /** `Alt+↑` / `Alt+↓`：当前 tab 上 / 下一轮。 */
  stepTurn(dir: -1 | 1): void {
    const t = this.store.activeId !== null ? this.store.tabs.get(this.store.activeId) : undefined;
    t?.turnRail.step(dir);
  }

  /**
   * 「显示系统注入」：流根上一个类切显隐（不重渲染）；估高跟着换（关着 0、开着一条细条），骨架账本按新口径重排。
   * 切之前钉住视口里第一张看得见的卡，切完把它放回原来的屏幕位置（视口不跳）。
   */
  toggleInjected(): void {
    const on = !injectedShownDefault();
    setInjectedShownDefault(on);
    const t = this.store.activeId !== null ? this.store.tabs.get(this.store.activeId) : undefined;
    const pin = t ? firstVisibleCard(t.streamEl) : null;
    const before = pin?.getBoundingClientRect().top ?? 0;
    this.applyInjected(on);
    for (const tab of this.store.tabs.values()) {
      const w = tab.stream.contentElement.getBoundingClientRect().width;
      if (tab.skeleton && w > 0) tab.skeleton.relayout(w, true);
    }
    if (t && pin?.isConnected) t.streamEl.scrollTop += pin.getBoundingClientRect().top - before;
  }

  /** 开关落到界面：流根的类 ＋ 估高口径。启动时也走一次。 */
  applyInjected(on: boolean): void {
    this.streamRootEl.classList.toggle("show-injected", on);
    setInjectedShown(on);
  }

  /** 会话头「⋯」里流的开关（接在这个标签页的右键菜单后面）。右侧灰字是那个动作此刻绑的键。 */
  streamToggles(): MenuItem[] {
    const chord = dispatcher.effectiveChord("session.toggle-process");
    return [
      { label: copyText("stream.injected.toggle"), checked: injectedShownDefault(), onClick: () => this.toggleInjected() },
      {
        label: copyText("stream.proc.toggle"),
        checked: processExpandedDefault(),
        detail: chord ? KeybindingDispatcher.prettyChord(chord) : undefined,
        onClick: () => this.toggleProcessExpanded(),
      },
    ];
  }

  /**
   * 大纲：向后端要新的一截（从上次的 `end` 接着要；在途就合并成一趟）。
   * 批期不要 —— 批结束时 active tab 统一要一次，后台 tab 切进来再要（`needsFetch`）。
   */
  private refreshOutline(tab: Tab): void {
    if (this.store.inBatch) return;
    void tab.outline.refresh();
  }

  /**
   * ★ 步 3：**「够不够一屏」改读真实布局。**
   *
   * # 旧判据错在哪
   *
   * 原来两处都写的是 `el.scrollHeight - el.clientHeight > 1`（「滚得动吗」）。
   * 那**不是**「屏幕填满了吗」—— 两者在这个仓里经常不是同一件事：
   * - 每张顶层卡都带 `content-visibility: auto` + `contain-intrinsic-size: auto <估值>`
   *   （`height-estimate.ts`）。**没渲染过的卡贡献的是估值**，`scrollHeight` 里掺着一笔
   *   与屏幕上看到的东西无关的账 ⇒ 估高了就"看起来滚得动"，而屏幕仍是半屏。
   * - 工具密集会话一轮物化可能只产出几张 34px 的合并卡，`scrollHeight` 差一两像素就
   *   越过 `>1` 这条线 ⇒ 补批当场停手。这正是用户报的「上下半屏」。
   *
   * # 新判据
   *
   * **最后一张卡的 `getBoundingClientRect().bottom` 有没有够到滚动容器的下沿。**
   * 这两个数都来自真实布局，不吃估值。
   *
   * ⚠ **没有布局时必须退回旧判据**：jsdom 无布局引擎，所有 rect 恒为 0
   * （容器 rect 高度也是 0）。这时按新判据算恒等于「满了」⇒ 补批整条路在测试环境里
   * 被静默关掉。所以这里显式探一次「这台机器给不给布局」，给不了就走老的算术判据 ——
   * **降级要写出来，不能靠恰好**。
   */
  private contentReachesBottom(tab: Tab): boolean {
    const el = tab.streamEl;
    const view = el.getBoundingClientRect();
    if (view.height <= 0) {
      // 拿不到真实布局（jsdom / 还没插进文档 / tab 不可见）⇒ 退回旧的算术判据。
      return el.scrollHeight - el.clientHeight > 1;
    }
    const last = tab.stream.contentElement.lastElementChild;
    if (!last) return false; // 一张卡都没有 ⇒ 肯定没满
    // 1px 容差：HiDPI 分数像素下 rect 是小数，卡刚好贴到下沿时会差零点几像素。
    return last.getBoundingClientRect().bottom >= view.bottom - 1;
  }

  /**
   * D 审计 R-3:一次 150 条 payload 可能只产出几张卡(tool-group 合并成单卡 34px、
   * skip 记录占配额不产卡)——工具密集会话一轮物化后屏幕仍近空,而 F40a 没有上翻
   * 补批兜底。补到**一屏填满**或账本弹尽。
   * 停手条件从「滚得动」换成 `contentReachesBottom`——理由见它的头注。
   *
   * **轮数封顶不再是停手条件**。原来 `round < 4` 一到就收手：
   * 秤 3 量过最稀疏那一档（每 150 条只出 5 张细条卡）4 轮只补得出 20 张 ⇒ 半屏，而且批结束 / 视口变大这两条入口
   * 之后**没有任何东西**再补（不可滚的元素不产生 scroll 事件）。现在：一次同步调用仍只跑
   * {@link MATERIALIZE_ROUNDS_PER_CALL} 轮（每一下的量有界，不把主线程占住），没满、账本还有 ⇒
   * 下一帧接着补（`scheduleFillContinuation`，与上翻补批同一条 rAF 自链），直到满一屏或账本空。
   */
  private materializeUntilFilled(tab: Tab): void {
    if (tab.skeleton) {
      tab.skeleton.fillVisible(); // 〔骨架〕同上
      return;
    }
    for (let round = 0; round < TabStreamView.MATERIALIZE_ROUNDS_PER_CALL; round++) {
      if (tab.window.pendingCount === 0) return;
      if (round > 0 && this.contentReachesBottom(tab)) return;
      this.materializeTail(tab);
    }
    if (tab.window.pendingCount > 0 && !this.contentReachesBottom(tab)) this.scheduleFillContinuation(tab);
  }

  /**
   * Batch13-F40a/b:批量渲染内核(物化 / 上翻补批 / R-1 缓冲 flush 共用)。
   * - sink 不接 onRealUserInput(历史 user 卡不得触发自动切 tab)、branch/queue/title
   *   为 no-op(收纳/缓冲时 routeMetaAndBranch 已喂过,BranchFolder.seenUuids 双保险);
   * - 不计 unread(重放历史不是"未读新消息",修 backlog S-2;R-1 缓冲的 unread
   *   在缓冲时已计);
   * - lazy hljs + observe(批量渲染的是历史内容,滚入视口再高亮);
   * - 插卡前 unwrapAll 摊平(邻居可能在 fold wrap 内,F39 实证的不变量)、批内
   *   暂停逐卡 snap(S-7,防 150 次强制 reflow)、插完 reconcile 孤儿 tool_result
   *   (S-6 同步出账)、rebuildNow 无条件重折(flushPending 的 setsEqual 短路会把
   *   摊平永久化)。
   */
  private renderPayloadsBatch(tab: Tab, payloads: JsonlLinePayload[]): void {
    if (payloads.length === 0) return;
    const ctx: RenderContext = {
      parentPath: tab.parentPath,
      origin: tab.origin,
      toolUseNames: tab.toolUseNames,
      toolUseElements: tab.toolUseElements,
      runCards: tab.runCards,
      pendingToolResults: tab.pendingToolResults,
      needs: tab.needs, // 在等批准的那一步建出来就画成「在等你批准」
      lazy: true,
    };
    const sink: StreamSink = {
      timeline: tab.timeline,
      onBranchRecord: () => {},
      onQueueOperation: () => {},
      enhanceRoot: tab.streamEl, // IO 的 root = 本 tab 的滚动容器
      // G6：**远端也挂**。本机那条命令也收 sid 了 ⇒
      // **两条路都只要 sid**，「本机拿不到 jsonl 路径就不能分叉」这道门跟着没了
      // （原先那个随迭代更新的路径游标也一并去掉：没人再要那个值）。
      onCardRendered: (el, msg) => {
        if (msg.type !== "user" && msg.type !== "assistant") return;
        if (!msg.uuid) return;
        attachBranchButton(el, {
          uuid: msg.uuid,
          sourceSessionId: tab.sessionId,
          origin: tab.origin,
          cwd: tab.projectDir ?? undefined,
          onForked: (res) => void this.host.startForkedSession(tab, res),
        });
      },
    };
    tab.branchFolder.unwrapAll();
    tab.stream.batchInsert(() => {
      for (const p of payloads) {
        try {
          renderContentRecord(p, ctx, sink);
        } catch (e) {
          console.error("[tabs] 批量渲染单条失败,跳过:", p.seq, e);
        }
      }
    });
    for (const el of reconcilePendingToolResults(ctx)) tab.timeline.removeByElement(el);
    tab.branchFolder.rebuildNow();
  }

  /**
   * Batch13-F40a:物化 tab 的尾段——从窗口账本弹出 seq 最高的 ≤k 条建卡。
   * 物化目标是 virgin/近 virgin tab(无滚动位置可保),不需要滚动补偿——上翻补批
   * 的手动补偿在 fillAbove(F40b)。
   */
  private materializeTail(tab: Tab, k = TabStreamView.MATERIALIZE_TAIL_K): void {
    this.renderPayloadsBatch(tab, tab.window.takeTail(k, TabStreamView.BATCH_BUDGET));
    this.updateSentinel(tab);
  }

  /**
   * 〔骨架〕向后端要这个会话的**骨架索引**，到了就接骨架。
   *
   * 只对**已经有渲染后缀**的 tab 要（`floor !== null`：骨架顶的是 `[0, floor)`）；
   * 每个 tab 只要一次（`skeletonFetch`）；瞬时失败 ⇒ 下一次触发点再问**一次**（有界、零定时器）。
   * 调用点只有两处：批结束时的 active tab、`switchTo` 切进来的那个 tab ⇒ 后台 tab 不花这一次。
   */
  private requestSkeleton(tab: Tab): void {
    if ((tab.skeletonFetch !== "idle" && tab.skeletonFetch !== "again") || !tab.parentPath) return;
    if (tab.window.floorSeq === null) return;
    const retry = tab.skeletonFetch === "again";
    // 瞬时失败的第一次 ⇒ `again`；结构性（老后端）或已经重问过 ⇒ `done`。
    const failed = (transient: boolean): void => {
      tab.skeletonFetch = transient && !retry ? "again" : "done";
    };
    tab.skeletonFetch = "pending";
    const jsonlPath = tab.parentPath;
    const origin = tab.origin;
    const first = readSessionIndex(origin, jsonlPath, 0);
    // 〔欠账〕大纲**等这一趟**：索引顺带出清单（后端 `IndexRow::x`）⇒ 首屏同一份文件
    // 只读一遍；带不回（老后端 / 零条 / 失败）⇒ 它自己照旧从 0 要一份清单。
    tab.outline.awaitSeed(first.then(outlineSeedFromIndex));
    void first
      .then(async (res) => {
        if (this.store.tabs.get(tab.sessionId) !== tab) return; // 期间关掉了
        tab.skeletonFetch = "done";
        const got = ledgerFromIndex(res);
        if (!got.ok) {
          failed(res.failure !== "oldBackend");
          console.info(`[tabs] 骨架未接（${tab.sessionId.slice(0, 8)}）：${got.reason}`);
          return;
        }
        // 索引拉回来之前 tab 可能又长了：floor 之下还有索引没覆盖到的行 ⇒ **续传**（从上次的 end 接着要）
        const floor = tab.window.floorSeq ?? 0;
        if (floor > got.ledger.endSeq) {
          const more = await readSessionIndex(origin, jsonlPath, got.end);
          if (more.available) got.ledger.append(more.rows);
        }
        if (this.store.tabs.get(tab.sessionId) !== tab) return;
        this.attachSkeleton(tab, got.ledger);
      })
      .catch((e: unknown) => {
        failed(true);
        console.warn(`[tabs] 骨架索引拉取失败（${tab.sessionId.slice(0, 8)}）：`, e);
      });
  }

  /**
   * 接骨架：`[0, floor)` 画成占位。**接之前先对拍 seq 空间** —— 抽几条还在 pending 的记录，
   * 它们的 uuid 在索引里必须落在同一个 seq 上；对不上（本地截断重读换过 seq，INVARIANTS §25）
   * 就**不接**，退回尾部窗口（不许硬对）。
   */
  private attachSkeleton(tab: Tab, ledger: SkeletonLedger): void {
    const floor = tab.window.floorSeq;
    if (floor === null || tab.skeleton) return;
    for (const p of tab.window.peek(8)) {
      const u = (p.message as { uuid?: unknown }).uuid;
      if (typeof u !== "string") continue;
      const at = ledger.uuidToSeq.get(u);
      if (at !== p.seq) {
        console.warn(
          `[tabs] 骨架未接（${tab.sessionId.slice(0, 8)}）：seq ${p.seq} 在索引里是 ${String(at)} —— seq 空间对不上`,
        );
        return;
      }
    }
    const view = new SkeletonView(ledger, tab.streamEl, tab.timeline, {
      materialize: (lo, hi) => {
        const taken = tab.window.takeRange(lo, hi);
        this.renderPayloadsBatch(tab, taken);
        this.fetchMissingRows(tab, ledger, lo, hi, new Set(taken.map((p) => p.seq)));
      },
    });
    // 在视口上方插一块高占位：同 `fillAbove` 的纪律 —— 关原生锚定、同一个同步任务里按 ΔscrollHeight 补偿
    const el = tab.streamEl;
    const beforeH = el.scrollHeight;
    const beforeTop = el.scrollTop;
    try {
      el.style.overflowAnchor = "none";
      view.attach(floor);
      el.scrollTop = beforeTop + (el.scrollHeight - beforeH);
    } finally {
      el.style.overflowAnchor = "";
    }
    tab.skeleton = view;
    this.updateSentinel(tab);
    if (this.store.activeId === tab.sessionId) {
      view.fillVisible();
      this.refineNearby(tab); // 第二级
    }
    // 骨架接上 ⇒ 正文不必再驻留：丢掉的那些滚到时按偏移要回来。
    // 前端账本只留离已渲染尾巴最近的一批（第一次上翻不用等 IPC）。
    // monitor 的重放缓冲那一半不用再登记了：它对**每个**会话都只留尾巴（`event_replay·rs` 头注「容量」）。
    tab.window.keepHighest(TabStreamView.FILL_BATCH);
  }

  /**
   * 〔骨架〕**按偏移取正文**：物化 `[lo, hi)` 时，账本里没有、也还没到过的那些
   * 会建卡的行（`seenSeqs` 里没有、索引说它不是「不建卡」的那种）⇒ 按索引里的字节边界向后端要
   * （`record-reads.ts::readRange` = 那台后端 `history-page` 带 `until`），回来的行**走 `onLine` 全套**
   * （去重、旁路记账、门控 —— 这段已经不在占位里了，门控会就地建卡），与重放来的行一视同仁。
   *
   * 今天它补的是「重放还没推到」的那一截（远端尾部优先快照的回填期、大会话启动重放的在途期）；
   * 它也是「骨架不带正文」那条路的另一半 —— 等 `EventReplay.history` 加上界，
   * 没推过来的历史就全靠它取。连续缺的行并成一段、一段一次 IPC；同一段不会被要两次
   * （骨架把它标成已物化之后就不会再交给宿主）。
   */
  private fetchMissingRows(
    tab: Tab,
    ledger: SkeletonLedger,
    lo: number,
    hi: number,
    taken: ReadonlySet<number>,
  ): void {
    if (!tab.parentPath) return;
    const runs: Array<[number, number]> = [];
    for (let s = lo; s < hi; s++) {
      const f = ledger.factsOf(s);
      // 「缺」= 会建卡、而这一次没从账本里取到 —— 两种来历：重放没推过来（没见过），
      // 或见过、但骨架接上之后被丢出账本（`keepHighest`）。两种回来之后喂法不同，见下。
      const missing = f !== undefined && skeletonKind(f) !== "none" && !taken.has(s);
      if (!missing) continue;
      const last = runs[runs.length - 1];
      if (last && last[1] === s) last[1] = s + 1;
      else runs.push([s, s + 1]);
    }
    const origin = tab.origin;
    let inflight = this.rangeFetches.get(tab);
    if (!inflight && runs.length > 0) {
      inflight = new Set();
      this.rangeFetches.set(tab, inflight);
    }
    for (const [a, b] of runs) {
      const first = ledger.factsOf(a)!;
      const lastRow = ledger.factsOf(b - 1)!;
      const fetched: Promise<void> = readRange(origin, tab.parentPath, first.o, lastRow.o + lastRow.n, a)
        .then((payloads) => {
          if (this.store.tabs.get(tab.sessionId) !== tab) return;
          // 没见过的 ⇒ 走 `onLine` 全套（旁路记账、去重、门控）；
          // 见过的 ⇒ 旁路账早记过了、去重会把它拒掉 ⇒ 只建卡（meta 那几类照旧不建）。
          const fresh = payloads.filter((p) => !tab.seenSeqs.has(p.seq));
          const again = payloads.filter(
            (p) => tab.seenSeqs.has(p.seq) && routeMetaAndBranch(p, NOOP_META) === "content",
          );
          this.feedHistoryRows(tab, fresh);
          tab.seenSeqs.addRange(a, b); // 这一段整段到过（不可显示的也算）
          if (again.length > 0) this.renderPayloadsBatch(tab, again);
        })
        .catch((e: unknown) => {
          console.warn(`[tabs] 按偏移取正文失败 [${a},${b})：`, e);
          throw e instanceof Error ? e : new Error(String(e)); // 「跳」要说得出没取到的原因
        });
      // 记进在途集合（「跳」等它落完），落完自己出列；没人等它时失败也不算没接住
      const set = inflight!;
      set.add(fetched);
      fetched.catch(() => {});
      void fetched.finally(() => set.delete(fetched)).catch(() => {});
    }
  }

  /**
   * 按偏移取回的**历史**行喂进 `onLine` —— 必须按**重放**的语义喂，不能按 live：
   * live 语义下历史 user 卡会触发 `userActive`（自动切 tab / 拉前 monitor）、
   * 历史的轮次结束会弹系统通知、每条 `recordAdded` 都重算一次主线。
   * ⇒ 对这一个 tab 走一遍批：`inBatch` 置位（`userActive` / `turnEndNotifier` 都认它）、
   * 折叠层进批模式；喂完把批期缓冲的中部插入一次挂载、折叠层 flush。
   * 若此刻本来就在一个真批里（启动重放未完），只喂不收 —— 真批的 `onBatchEnd` 会收。
   */
  private feedHistoryRows(tab: Tab, payloads: JsonlLinePayload[]): void {
    if (payloads.length === 0) return;
    const wasBatch = this.store.inBatch;
    this.store.inBatch = true;
    // 取回来的是历史：远端 tab「见行就翻活」那一格不认它（`TabStore.historyFeed` 头注）。
    this.store.historyFeed = true;
    tab.branchFolder.setBatchMode(true);
    try {
      for (const p of payloads) this.host.onLine(p);
    } finally {
      this.store.historyFeed = false;
      this.store.inBatch = wasBatch;
      if (!wasBatch) {
        this.flushMidBatchBuffer(tab);
        tab.branchFolder.flushPending();
        tab.branchFolder.setBatchMode(false);
      }
    }
  }

  /**
   * **按行号往下取一批**：账本空了、渲染窗口最老那一条
   * 不是第 0 行 ⇒ 问 `[floor − FILL_BATCH, floor)`（`record-reads.ts::readLines`，后端 `history-lines`）。
   *
   * 这是**没接骨架**的 tab 的取回路（接了骨架的按字节取，`fetchMissingRows`）。monitor 的重放缓冲从此每个会话
   * 只留尾巴（`event_replay·rs::REPLAY_TAIL_KEEP`），F5 之后更早的就从这里要回来；没被修剪过的会话问一次就到顶。
   *
   * 回来的行：没见过的 ⇒ `feedHistoryRows`（批语义 ＋ 不复活远端 tab，落进账本）；见过而被修剪出账本的 ⇒
   * 直接放回账本（`restore`，旁路账早记过了）。然后照旧从账本补到屏上（`fillAbove`）。
   * 一个 tab 同时只问一批（`fetching`）；失败 ⇒ 下一次上翻再问一次，连续第二次才停（`BelowState` 头注）。
   */
  private fetchBelow(tab: Tab): void {
    const range = tab.window.belowRange(TabStreamView.FILL_BATCH);
    if (!range || !tab.parentPath) return;
    tab.window.markFetchingBelow(range.until);
    this.updateSentinel(tab);
    void readLines(tab.origin, tab.parentPath, range.from, range.until, TabStreamView.BELOW_BUDGET_MS)
      .then((page) => {
        if (this.store.tabs.get(tab.sessionId) !== tab) return; // 期间关掉了
        const fresh = page.payloads.filter((p) => !tab.seenSeqs.has(p.seq));
        for (const p of page.payloads) {
          if (tab.seenSeqs.has(p.seq) && routeMetaAndBranch(p, NOOP_META) === "content") {
            tab.window.restore(p);
          }
        }
        this.feedHistoryRows(tab, fresh);
        tab.seenSeqs.addRange(page.from, page.next); // 同上
        tab.window.markFetchedBelow(range.from);
        this.updateSentinel(tab);
        if (this.store.activeId === tab.sessionId && tab.streamEl.scrollTop <= TabStreamView.TOP_TRIGGER_PX) {
          this.fillAbove(tab);
        }
      })
      .catch((e: unknown) => {
        if (this.store.tabs.get(tab.sessionId) !== tab) return;
        tab.window.markBelowFailed(e instanceof Error ? e.message : String(e));
        this.updateSentinel(tab);
        console.warn(`[tabs] 按行号取更早的消息失败 [${range.from},${range.until})：`, e);
      });
  }

  /** 每个 tab 在途的「往后补」（`recoverFromGap`）—— 同一个 tab 同时只补一趟。 */
  private readonly forwardFills = new WeakSet<Tab>();

  /**
   * **会话流丢过格之后补这一个 tab**（`TabManager.onStreamGap`）：
   *
   * ① 账本（还没上屏的）整份出账（`dropPending`）—— 之后往上翻按行号取回（`fetchBelow`）；
   * ② 从「见过的最大行号 + 1」起按行号往后取到末尾（`readLines` 不给 `until`，一段 ≤ 1 MiB，取到 `eof`）——
   *    丢在已上屏那一段之后的新行从这里回来；多取的（其实到过的）由 `(sid, seq)` 去重吃掉。
   * 取回来的走 `feedHistoryRows`（批语义、不复活远端 tab）。一行都没见过的 tab 不往后取（那会把整份会话拉一遍；
   * 它的内容等下一次宣告 / 下一行，或往上翻按行号取 —— 如实登记）。
   *
   * 往后补是**一件事**：开头造一次期限（{@link TabStreamView.GAP_FILL_BUDGET_MS}），每一问交剩下的；
   * 到点了还没到末尾 ⇒ 停、记一行（这个 tab 缺的那一截等下一次宣告 / 往上翻再要）。
   */
  recoverFromGap(tab: Tab): void {
    tab.window.dropPending();
    this.updateSentinel(tab);
    if (!tab.parentPath || tab.seenSeqs.isEmpty || this.forwardFills.has(tab)) return;
    const max = tab.seenSeqs.max;
    this.forwardFills.add(tab);
    const jsonlPath = tab.parentPath;
    const budget = budgetWithin(TabStreamView.GAP_FILL_BUDGET_MS);
    const step = (from: number): void => {
      const leftMs = remaining(budget);
      if (leftMs <= 0) {
        this.forwardFills.delete(tab);
        console.warn(
          `[tabs] 会话流丢格之后往后补没在期限内补完（${tab.sessionId.slice(0, 8)}，停在第 ${from} 行）`,
        );
        return;
      }
      void readLines(tab.origin, jsonlPath, from, undefined, leftMs)
        .then((page) => {
          if (this.store.tabs.get(tab.sessionId) !== tab) return this.forwardFills.delete(tab);
          this.feedHistoryRows(
            tab,
            page.payloads.filter((p) => !tab.seenSeqs.has(p.seq)),
          );
          tab.seenSeqs.addRange(page.from, page.next); // 同上
          if (page.eof || page.next <= from) return this.forwardFills.delete(tab);
          step(page.next);
          return true;
        })
        .catch((e: unknown) => {
          this.forwardFills.delete(tab);
          console.warn(`[tabs] 会话流丢格之后往后补失败（${tab.sessionId.slice(0, 8)}，从第 ${from} 行）：`, e);
        });
    };
    step(max + 1);
  }

  /**
   * F40b R-1:批期缓冲的「窗口内中部插入」(大增量批老块)一次性挂载。
   * 排序后走渲染内核(含 unwrapAll/rebuildNow——不能依赖随后 flushPending,
   * 它的 setsEqual 短路会把摊平永久化)。在 onBatchEnd 的 flushPending/reconcile
   * 之前调。
   */
  private flushMidBatchBuffer(tab: Tab): void {
    if (tab.midBatchBuffer.length === 0) return;
    const payloads = tab.midBatchBuffer.sort((a, b) => a.seq - b.seq);
    tab.midBatchBuffer = [];
    // 已知取舍(D 审计):批末 flush 不做选区守卫——unwrap/rebuild 会杀进行中选区,
    // 但 flush 不可延迟(数据必须落),且触发面(选中文本时恰逢远端大增量批)极窄。
    this.renderPayloadsBatch(tab, payloads);
    this.host.refreshTabBar(); // 缓冲期攒下的 unread 徽标一次刷新
  }

  /**
   * F40b:上翻补批。守卫:防重入 / 账空 / 选区进行中(补批 unwrap/rebuild 会杀
   * 进行中的选区,等下次 scroll 再试)。补偿:临时关原生锚定(防 WebView2 与手动
   * 补偿 double-shift;WebKitGTK 本就无锚定),测量→渲染→scrollTop 回写在同一
   * 同步任务内(不许 await/rAF 打断)。rAF 自链:零高批/不足一屏无 scroll 事件
   * (F39-R1 场景),补完复检直到离开触发区或账尽。
   */
  private fillAbove(tab: Tab): void {
    // 〔骨架〕接上了 ⇒ 不再「从尾巴往上一批批补」，只物化与视口相交的那段占位
    if (tab.skeleton) {
      tab.skeleton.fillVisible();
      return;
    }
    if (this.renderingFill) return;
    if (tab.window.pendingCount === 0) {
      // 账本空了、渲染窗口最老那一条不是第 0 行 ⇒ 按行号往下问一批（`fetchBelow`）。
      if (tab.window.wantsBelow) this.fetchBelow(tab);
      return;
    }
    const sel = document.getSelection();
    if (sel && !sel.isCollapsed) return;
    const el = tab.streamEl;
    this.renderingFill = true;
    try {
      el.style.overflowAnchor = "none";
      const beforeH = el.scrollHeight;
      const beforeTop = el.scrollTop;
      this.renderPayloadsBatch(tab, tab.window.takeTail(TabStreamView.FILL_BATCH, TabStreamView.BATCH_BUDGET));
      // 哨兵刷新必须在补偿回写**之前**:账尽移除的 ±30px 计入 Δ 一并吃掉——
      // 移除若在补偿后,dev(无锚定)会在"会话第一条"处一次性跳 30px(D 审计)。
      this.updateSentinel(tab);
      el.scrollTop = beforeTop + (el.scrollHeight - beforeH);
    } finally {
      // 还原必须在 finally:渲染内核抛出时留下 overflow-anchor:none = 该 tab 永久
      // 失去原生锚定,违反 §21.2 且无自愈(D 审计,两家共识)
      el.style.overflowAnchor = "";
      this.renderingFill = false;
    }
    this.scheduleFillContinuation(tab);
  }

  /**
   * 上翻补批 / 补满一屏的**下一帧复检**（rAF 自链的那一跳）：补完一批下一帧再看一眼，仍在触发区 / 仍不可滚 /
   * 仍没满一屏且账本有余就再补一批（`fillAbove`）；切走了（`activeId` 守卫）或账尽即停。
   * 从 `fillAbove` 末尾抽出来，`materializeUntilFilled` 一次调用补不满时也从这里接着补；
   * 「仍没满一屏」那一问用真实布局（`contentReachesBottom`），不只看滚不滚得动。
   */
  private scheduleFillContinuation(tab: Tab): void {
    requestAnimationFrame(() => {
      if (this.store.activeId !== tab.sessionId) return;
      const t = this.store.tabs.get(tab.sessionId);
      if (!t || (t.window.pendingCount === 0 && !t.window.wantsBelow)) return;
      const e = t.streamEl;
      if (
        e.scrollTop <= TabStreamView.TOP_TRIGGER_PX ||
        e.scrollHeight - e.clientHeight <= 1 ||
        !this.contentReachesBottom(t)
      ) {
        this.fillAbove(t);
      }
    });
  }

  /**
   * F40c DEV 探针用:active tab 状态一行 JSON——无 devtools 环境下 E2E 断言的
   * 唯一出口(经 e2e-probe 热键 → fe_perf 日志)。生产不接线,方法本身无副作用。
   *
   * 🔴 秤 6(表第 6 行)在这里加了**三个账本的条数**:
   * `branchRecords` / `userInputs` / `pending`。口径与「量不到什么」写在
   * `branchRecordCount` 的头注与 `tests/evidence/S6-memory-ledger.md` 里。
   */
  debugSnapshot(): string {
    const tab = this.store.activeId !== null ? this.store.tabs.get(this.store.activeId) : undefined;
    if (!tab) return JSON.stringify({ active: null });
    const el = tab.streamEl;
    const statusText = document.getElementById("status-bar")?.textContent ?? "";
    return JSON.stringify({
      sid: tab.sessionId.slice(0, 8),
      scrollTop: Math.round(el.scrollTop),
      scrollHeight: el.scrollHeight,
      clientHeight: el.clientHeight,
      distBottom: Math.round(el.scrollHeight - el.scrollTop - el.clientHeight),
      // 秤 6 的三个账本(`pending` 本来就在,不重复开一个字段):
      // ① `TailWindow.pending` —— 还没上屏的整条 payload,**这一份是真的文本驻留**
      pending: tab.window.pendingCount,
      // ② `BranchFolder.records` —— 每条一个 {uuid,parentUuid,timestamp} 三元组,不含正文
      branchRecords: branchRecordCount(tab.branchFolder),
      // ③ 大纲的条数 —— 清单问后端要，前端只留面板上那几行（每行一份 80 字摘要）
      userInputs: tab.outline.count,
      midBuffer: tab.midBatchBuffer.length,
      // 〔骨架〕接上没有 · 索引总条数 · 还在占位里的行数 · 占位块数（`timeline` 里含占位条目）
      skeleton: tab.skeleton
        ? {
            rows: tab.skeleton.ledger.count,
            pendingRows: tab.skeleton.pendingRows,
            gaps: tab.skeleton.gapCount,
          }
        : tab.skeletonFetch,
      timeline: tab.timeline.size,
      foldWraps: tab.stream.contentElement.querySelectorAll(":scope > .branch-fold-wrap").length,
      sentinel:
        tab.stream.contentElement.querySelector(":scope > .stream-more-above")?.textContent ??
        null,
      err: statusText.startsWith("ERR") || statusText.startsWith("REJ") ? statusText : null,
    });
  }

  /**
   * F40b:顶端哨兵——账本非空时置顶「还有 N 条更早消息」,账尽移除。
   * 非 timeline 实体、无 data-uuid(BranchFolder 视作断 run,天然免疫 fold);
   * 二分插入的 anchor 恒为 timeline 元素,最老卡 insertBefore(首卡) 自然落哨兵后。
   */
  updateSentinel(tab: Tab): void {
    const content = tab.stream.contentElement;
    let el = content.querySelector(":scope > .stream-more-above") as HTMLElement | null;
    const n = tab.window.pendingCount;
    // 账本空了：下面还可能有（按行号取）⇒ 哨兵说的是「取」那一格的状态。
    const below = tab.window.belowState;
    const floor = tab.window.floorSeq;
    const belowText =
      n > 0 || floor === null || floor <= 0
        ? null
        : below.kind === "maybe"
          ? copyText("tabStreamView.sentinel.more")
          : below.kind === "fetching"
            ? copyText("tabStreamView.sentinel.loading")
            : below.kind === "failed"
              ? copyText("tabStreamView.sentinel.failed", { reason: below.reason })
              : null;
    // 〔骨架〕接上了 ⇒ 占位本身就是「上面还有」，哨兵退场
    if ((n === 0 && belowText === null) || tab.skeleton) {
      el?.remove();
      return;
    }
    if (!el) {
      el = document.createElement("div");
      el.className = "stream-more-above";
      content.prepend(el);
    }
    el.textContent = n > 0 ? copyText("tabStreamView.sentinel.moreCount", { n }) : (belowText ?? "");
  }

  /** F40a:virgin 后台 tab 的空闲物化队列(串行;rIC 缺失时 setTimeout 兜底) */
  materializeQueue: string[] = [];
  private materializeScheduled = false;

  private scheduleIdleMaterialize(): void {
    if (this.materializeScheduled) return;
    const sid = this.materializeQueue.shift();
    if (sid === undefined) return;
    this.materializeScheduled = true;
    const run = (): void => {
      this.materializeScheduled = false;
      const tab = this.store.tabs.get(sid);
      // virgin 的物化尾段(switchTo 可能已同步物化过);二次 batch 开始则原样跳过,
      // 账本继续收纳,批结束会重新排队。
      // 钉过水位、账本有余、真实布局没满一屏的 ⇒ 补一批（`fillAbove`：带选区守卫与滚动补偿，
      // 后台 tab 不自链 —— 它的 rAF 复检有 `activeId` 守卫；切进来时 `activate` 那一脚接着补）。
      if (tab && !this.store.inBatch && tab.window.pendingCount > 0) {
        if (tab.window.floorSeq === null) this.materializeTail(tab);
        else if (!this.contentReachesBottom(tab)) this.fillAbove(tab);
      }
      this.scheduleIdleMaterialize();
    };
    if (typeof window.requestIdleCallback === "function") {
      window.requestIdleCallback(run, { timeout: 2000 });
    } else {
      window.setTimeout(run, 200);
    }
  }

}

/** 视口里第一张看得见的卡（流的顶层子节点里、底边在视口上沿之下的第一张；切显隐时钉它）。 */
function firstVisibleCard(scroller: HTMLElement): HTMLElement | null {
  const top = scroller.getBoundingClientRect().top;
  const content = scroller.querySelector<HTMLElement>(".stream-content") ?? scroller;
  for (const el of Array.from(content.children)) {
    if (!(el instanceof HTMLElement) || el.classList.contains("card-injected")) continue;
    const r = el.getBoundingClientRect();
    if (r.height > 0 && r.bottom > top) return el;
  }
  return null;
}
