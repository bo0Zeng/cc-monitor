/**
 * 实时流视图：每个 tab 的那条流怎么建、怎么按 seq 门控建卡，尾部窗口 / 骨架 / 上翻补批 / 哨兵 / 大纲，重放批的开与收。
 *
 * 会话状态全在 `TabStore`（同一个实例）；要别处做的事经 `TabStreamHost`：按偏移取回的历史行走 `onLine` 全套 ·
 * 刷 tab 栏（立即 / 帧末合批）· 应用 ai-title · 真用户输入上屏时的自动跟随 · 起分叉出来的新会话。
 * `replay-tail-keep.vitest.ts` 按原文抽 `MATERIALIZE_TAIL_K` 与 `MATERIALIZE_ROUNDS_PER_CALL` 对拍 Rust 侧的 `REPLAY_TAIL_KEEP`。
 */
import { speakerNameOf } from "./agent-profile";
import { MessageStream } from "./stream";
import { reconcilePendingToolResults, type RenderContext } from "./cards";
import { BranchFolder } from "./branch-fold";
import { attachBranchButton } from "./branch-button"; // 实时会话的分叉入口
import type { JsonlLinePayload } from "./events";
import { RecordTimeline } from "./record-timeline";
import { SeqSet, TailWindow, type SkeletonLedger, type TakeBudget } from "./live-window";
import { HeightRefiner, workerMeasure } from "./height-refiner";
// 〔骨架〕骨架层（占位 ＋ 只物化可见区）。接入点全部带「骨架」字样，搜得到。
import { SkeletonView, ledgerFromIndex } from "./skeleton-view";
import { eagerBodyChars, setInjectedShown, skeletonKind } from "./height-estimate";
// 「大纲」与历史查看器共用同一份界面与跳转；清单问后端要（`OutlineSource`）。
// 大纲在会话内查找面板里（`SessionFindPanel`：搜索 / 大纲两个模式，跳只有一个住址）。
import type { UserInputPanel, JumpResult } from "./views/user-input-panel";
import { OutlineSource, outlineSeedFromIndex } from "./views/outline-source";
import { SessionFindPanel } from "./views/session-find";
import { TurnFold, injectedShownDefault, processExpandedDefault, setInjectedShownDefault, setProcessExpandedDefault } from "./turn-fold";
import { TurnRail } from "./turn-rail";
import { dispatcher, KeybindingDispatcher } from "./keybindings/registry";
import type { MenuItem } from "./kit/menu";
// 实时窗口 import 历史查看器（方向别扭，不是惯例）：共用的只有 `revealCard`（找卡 → 展开 → 滚，两条路的卡由同一份渲染器建）。
import { revealCard } from "./views/session-viewer";
import {
  renderContentRecord,
  routeMeta,
  type MetaSink,
  type StreamSink,
} from "./render-stream-record";
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
const NOOP_META: MetaSink = {};

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
  /** 分叉产出新会话文件之后 —— 起它。 */
  /** 从这条消息处分叉：开起新会话框的分叉那一形。 */
  forkFrom(tab: Tab, uuid: string): void;
  /** 这个会话的运行表里那个运行叫什么（agent 来话的事件条起名用；表里没有 ⇒ `undefined`，用来话自带的名字）。 */
  runLabelOf(sid: string, run: string): string | undefined;
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

export class TabStreamView {
  /** 物化 / 后台 tab 尾段条数（与查看器的 TAIL_INITIAL 同义） */
  private static readonly MATERIALIZE_TAIL_K = 150;
  /** `materializeUntilFilled` 一次同步调用最多跑几轮（不是「到此为止」：没满就下一帧接着补）。 */
  private static readonly MATERIALIZE_ROUNDS_PER_CALL = 4;
  /** 上翻补批的批量 / 触发距离 */
  private static readonly FILL_BATCH = 200;
  /**
   * 一批（物化尾段 / 上翻补批）的第二道闸：急路要当场物化的正文字符（`eagerBodyChars`）。
   * 64 Ki 字符：常态批碰不到它，只截长尾批（一条几百 KB 正文的 assistant 就是一整批）。截下来的下一帧接着补（`materializeUntilFilled` / `fillAbove` 的 rAF 自链）。
   */
  private static readonly BATCH_BODY_CHARS = 64 * 1024;
  private static readonly BATCH_BUDGET: TakeBudget = {
    weight: (p) => eagerBodyChars(p.record),
    max: TabStreamView.BATCH_BODY_CHARS,
  };
  private static readonly TOP_TRIGGER_PX = 800;
  /**
   * 切进来之后在眼前停多久才算「停下来看」（ms）：到点才要骨架索引 / 接骨架 / 刷大纲。
   * 150：比键盘自动重复（30–40 ms 一下）与连点（台架 200 ms 一下的那一串也会停够）长、比人能察觉的延迟短 ——
   * 首屏是已建好的尾巴，索引只管往上翻的占位，晚 150 ms 到看不出来。
   */
  private static readonly STAY_MS = 150;
  /**
   * 往上翻那一问的期限：60 秒 —— 与它上一个住址（monitor `frame_query::PAGE_BUDGET`，
   * 一次 `read_session_lines` 各拿一份）同值。一件一问。
   */
  private static readonly BELOW_BUDGET_MS = 60_000;
  /**
   * 会话流丢格之后「往后补到末尾」那一**件**的总期限：120 秒（= 一次性远端那一趟的天花板
   * `dial_host::ONE_SHOT_DEADLINE`，monitor 那一侧读整段 `frame_query::READ_LINES_BUDGET` 同值）。
   * 开头造一次，之后每一问交剩下的（`remaining`）—— 每页各拿一整份的话，遇上一页一页慢慢吐的对端停不下来。
   */
  private static readonly GAP_FILL_BUDGET_MS = 120_000;
  /** 补批防重入（补偿测量期间嵌套触发会算错差值） */
  private renderingFill = false;
  /** 每个 tab 的查找面板（关 tab 时摘掉）。`Tab` 上挂的是它的两半：`inputsEl`（整块）与 `inputsPanel`（大纲）。 */
  private readonly finds = new Map<string, SessionFindPanel>();
  /**
   * 每个 tab 在途的「按偏移取正文」（`fetchMissingRows` 发的那几趟）。
   * 「跳」要等它们落完再找卡 —— 骨架接上之后，没物化的那段正文多半不在前端账本里（`keepHighest`），
   * `ensure` 只是把取正文的请求发出去，同步那一下去找卡必然落空。
   */
  private readonly rangeFetches = new WeakMap<Tab, Set<Promise<void>>>();
  /**
   * 索引回来时 tab 已经切走了 ⇒ 不在后台接骨架，账本停在这里，切回来的下一帧再接（`activate`）。
   * 接骨架要插占位、量几何、补可见区 —— 在收起的 tab 里做是白干还逼排版；快速连切时一串旧切换的活全堆在后面。
   */
  private readonly parkedSkeletons = new WeakMap<Tab, SkeletonLedger>();

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
        const rows = [...known, ...pages.flat()].map((p) => ({ seq: p.seq, rec: p.record }));
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
    stream.park(true); // 新建的 tab 先收着（`.stream` 默认收起），切进来时 `showOnly` 翻出
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
    // 按轮折叠：一轮的边界与结论问后端（`history-turns`），路径同大纲每次现取；占位里有没有下一轮开头问这个 tab 的骨架。
    const turnFold = new TurnFold(
      stream.contentElement,
      streamEl,
      () => {
        const t = this.store.tabs.get(sessionId);
        return t?.parentPath ? { origin: t.origin, jsonlPath: t.parentPath } : null;
      },
      undefined,
      () => this.store.tabs.get(sessionId)?.skeleton ?? null,
    );
    // 轮次刻度：同一份轮；跳与查找 / 大纲同一个住址（`jumpInTab`）。挂在流外（不随流滚），随 tab 同进同出。
    const turnRail = new TurnRail(streamEl, stream.contentElement, {
      turns: () => turnFold.all,
      waiting: () => this.store.tabs.get(sessionId)?.needs != null,
      speaker: () => speakerNameOf(this.store.tabs.get(sessionId)?.agent ?? null),
      jump: (uuid) => void this.jumpInTab(sessionId, streamEl, uuid),
      inFront: () => this.store.activeId === sessionId,
    });
    turnFold.onTurns = () => turnRail.render();
    this.streamRootEl.appendChild(turnRail.el);
    return { streamEl, stream, branchFolder, timeline, inputsEl, inputsPanel, outline, turnFold, turnRail };
  }

  /** 给刚建好的 tab 挂上翻补批的滚动监听与视口变大的补批（原是 `ensureTab` 里的一段，逐字）。 */
  wireTab(tab: Tab): void {
    const sessionId = tab.sessionId;
    const streamEl = tab.streamEl;
    const stream = tab.stream;
    // 上翻补批触发器（passive 只读滚动位置；handler 内判 active，后台 tab 的程序化滚动 / 尺寸变化不触发）
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
    // 大纲跟着走（`reset` 也让在途那趟回来后不许回写）。
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
    // 窗口账本与缓冲持整段历史 payload（大会话数十 MB 级）⇒ 断引用；摘 fill listener
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
      t.stream.park(sid !== sessionId); // 收起期间几何不作数（`MessageStream.park`）
      // 面板与它那条流同进同出：漏掉的话所有 tab 的面板一起挂在屏幕上，点下去找的是别人的流。
      t.inputsEl.classList.toggle("active", sid === sessionId);
      // 切走的 tab 收起面板（出弹层栈）—— 不然 Esc 去关的是一块看不见的面板。
      if (sid !== sessionId) this.finds.get(sid)?.close();
      if (sid !== sessionId) t.turnFold.release(true); // 先没收的那一轮：切走了就收
      t.turnRail.el.classList.toggle("active", sid === sessionId);
      if (sid === sessionId) t.turnRail.shown(); // 收起期间没量过 ⇒ 下一帧量一次

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
    const at = pending.findIndex((p) => p.record.id === uuid);
    if (at >= 0) {
      this.renderPayloadsBatch(tab, tab.window.takeTail(pending.length - at));
      this.updateSentinel(tab);
      return revealCard(streamEl, uuid);
    }
    this.requestSkeleton(tab);
    return Promise.reject(new Error(copyText("tabStreamView.search.notReady")));
  }

  /** 这个 tab 此刻还在眼前：是当前的、还是表里那一个（没被关掉 / 重来过）、它的流还挂在页上。排到后面的活到点先问这一句。 */
  private inFront(tab: Tab): boolean {
    return this.store.activeId === tab.sessionId && this.store.tabs.get(tab.sessionId) === tab && tab.streamEl.isConnected;
  }

  /** 切进来的 tab：物化 / 哨兵 / 骨架索引 / 大纲 / 不可滚时踢一次补批（原是 `switchTo` 中段，逐字）。 */
  activate(next: Tab): void {
    // 上次按行号往下问失败了的，切进来时允许再问一次（失败不自动重问，见 `BelowState` 头注）。
    if (next) next.window.retryBelow();
    // 命中 virgin tab（启动重放全收纳、还没建过卡）→ 同步物化尾段，免得切过去一片空白（有界循环补到一屏）。
    // 非 virgin tab 可能有滚动位置要保，走下面那一脚带补偿的 `fillAbove`。
    const virginFill = !!next && next.window.floorSeq === null && next.window.pendingCount > 0;
    if (virginFill) this.materializeUntilFilled(next);
    // 切入即刷新哨兵（非 virgin 但账本非空的 tab 也要见到「还有 N 条」）
    if (next) this.updateSentinel(next);
    // 〔骨架〕要索引 / 接上次停着的那一份 / 刷大纲：都是给「停下来看」的人准备的（索引那一问是后端整份读那个会话的记录文件，
    // 接骨架要插占位、量几何）—— 在眼前停住了（`STAY_MS`）才做；按住「下一个 tab」路过的一概不发、不接。
    if (next) {
      // 调度：一次性 —— 停留判定：到点时还是它在眼前（没被下一下切走）才要索引 / 接骨架 / 刷大纲
      setTimeout(() => {
        if (!this.inFront(next)) return;
        const parked = this.parkedSkeletons.get(next);
        if (parked) {
          this.parkedSkeletons.delete(next);
          this.attachSkeleton(next, parked);
        }
        this.requestSkeleton(next); // 上面刚物化过尾段 ⇒ floor 已钉；要过的不重要
        if (next.outline.needsFetch) this.refreshOutline(next); // 大纲：有新行才要
      }, TabStreamView.STAY_MS);
    }
    // 账本有余却没满一屏的 tab 没有补批入口（不可滚的元素不产生 scroll 事件）⇒ 切入时踢一次，rAF 自链接管到满或账尽。
    // 账本空了但下面可能还有（`wantsBelow`）同样踢。「满没满」问真实布局（`contentReachesBottom`），不只看 `scrollHeight`（掺着估值）。
    // 刚为 virgin tab 跑过 `materializeUntilFilled`、账本还有余的不再踢：它补不满时自己排了下一帧的接续（一次同步调用要有界）。
    // 「满没满」要读几何 ⇒ 挪到下一帧再问：同步段里一读，浏览器就得当场把刚翻出来的整个 tab 样式与布局算完（点击处理被拖长、这一帧更晚画出来）；
    // 下一帧的回调里读，排的就是那一帧本来要排的那一份。期间又切走（`activeId` 守卫）⇒ 不补。
    const continuing = virginFill && next.window.pendingCount > 0;
    if (next && !continuing && (next.window.pendingCount > 0 || next.window.wantsBelow)) {
      // 调度：一次性 —— 切进来的下一帧问一次满没满，没满踢一脚补批（之后由补批自己的 rAF 自链接管）
      requestAnimationFrame(() => {
        if (!this.inFront(next)) return;
        const el = next.streamEl;
        if (el.scrollHeight - el.clientHeight <= 1 || !this.contentReachesBottom(next)) this.fillAbove(next);
      });
    }
  }

  /**
   * 重放批结束：逐 tab 挂批期缓冲、折叠层 flush、孤儿 tool_result 对账；当前 tab 补到一屏，
   * 其余 virgin 后台 tab 进空闲物化队列（原是 `onBatchEnd` 的主体，逐字）。返回当前 tab。
   */
  batchEnd(): Tab | undefined {
    for (const t of this.store.tabs.values()) {
      // 先把批期缓冲的中部插入一次挂载（内含 unwrapAll / rebuildNow），再按清单重折 / reconcile
      this.flushMidBatchBuffer(t);
      t.branchFolder.rebuildNow();
      // 切块场景下，老块的 tool_use 现在已渲染 → 重试匹配早到的 fallback result
      const ctx: RenderContext = {
        parentPath: t.parentPath,
        speaker: speakerNameOf(t.agent),
        origin: t.origin,
        toolUseNames: t.toolUseNames,
        toolUseElements: t.toolUseElements,
        runCards: t.runCards,
        pendingToolResults: t.pendingToolResults,
      };
      // 孤儿卡出 DOM 的同时出账，防悬空锚点
      for (const el of reconcilePendingToolResults(ctx)) t.timeline.removeByElement(el);
    }
    // active tab 不足一屏（或还是 virgin）→ 立即物化到可见；其余后台 tab 进空闲物化队列（逐个串行，免得并发建卡风暴）。
    const active = this.store.activeId !== null ? this.store.tabs.get(this.store.activeId) : undefined;
    if (active && active.window.pendingCount > 0) {
      // 步 3：「够不够一屏」改读**真实布局**（见 `contentReachesBottom` 的头注）。
      const notFilled = !this.contentReachesBottom(active);
      if (active.window.floorSeq === null || notFilled) {
        this.materializeUntilFilled(active);
        active.stream.scrollToBottom();
      }
    }
    // 已结束的会话不进后台物化队列（切过去时有同步物化兜底）。
    // virgin 的、以及钉过水位而真实布局没满一屏的后台 tab 进队（后台 tab 是 `visibility:hidden`，几何照在）。
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
    // active tab 没走物化分支（尾块已可滚）时也要挂哨兵
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
      onTitleUpdate: (title: string) => this.host.applyAiTitle(tab, title),
      onRealUserInput: (sid: string) => {
        this.host.userActive(sid);
        this.refreshOutline(tab); // 真用户输入上屏 ⇒ 大纲要新的一截
      },
      enhanceRoot: this.store.inBatch ? tab.streamEl : null, // 批期 lazy ⇒ 交本 tab 的滚动容器
      // 实时会话也挂「从这一轮分叉」按钮（本机远端都挂，两条路都只要 sid）；按钮本体是共享组件。
      onCardRendered: (el, rec) => {
        if (rec.t !== "said" && rec.t !== "reply") return;
        attachBranchButton(el, {
          uuid: rec.id,
          onFork: (uuid) => this.host.forkFrom(tab, uuid),
        });
      },
    };

    // 收纳（不建卡）的记录也要喂标题（routeMeta 是两条路共用的一份）。
    if (routeMeta(payload, sink) === "consumed") return;

    // 门控（单洞后缀不变量，纯按 seq）：
    // - virgin ＋ 批：active tab 首条 content 钉 floor（尾块直渲）；后台 tab 一律收纳（批后空闲物化）；
    // - virgin ＋ 实时：新开 tab 直渲并钉 floor；
    // - seq ≥ floor：渲染；seq < floor：收纳（旧块 / 尾部优先回填 / 迟到块，与批哨兵无关）。
    // 批后空闲物化还没轮到这个 tab 就来了实时行：先物化尾段（takeTail 顺带钉 floor），再照常收这一行 ——
    //   直接按新行钉 floor 会把账本里整段历史钉死在下面。
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
    // 批期落在渲染窗口内中部的插入（seq ≥ floor 且 < 已渲染最高 seq：大增量批的老块）→ 缓冲，onBatchEnd 一次挂载（不逐帧往上方插，§21）。
    // 离线期的真新消息照计未读（按记录计，与工具组合并略有出入，99+ 封顶下可接受）。
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
      // 收纳不计未读 —— 重放历史不是「未读新消息」
      return;
    }

    const ctx: RenderContext = {
      parentPath: tab.parentPath,
      speaker: speakerNameOf(tab.agent),
      origin: tab.origin,
      toolUseNames: tab.toolUseNames,
      toolUseElements: tab.toolUseElements,
      runCards: tab.runCards,
      pendingToolResults: tab.pendingToolResults,
      needs: tab.needs, // 在等你的那一步建出来就画成「在等你」
      stepWait: (call) => tab.pending.find((p) => p.id === call),
      retryOutcome: (id) => tab.retries.get(id),
      runLabelOf: (run) => this.host.runLabelOf(tab.sessionId, run),
      // 批期间走惰性高亮（代码块占位 ＋ IntersectionObserver 触发再补）
      lazy: this.store.inBatch,
    };
    const beforeSize = tab.timeline.size;
    renderContentRecord(payload, ctx, sink);
    const inserted = tab.timeline.size > beforeSize;
    if (inserted) tab.branchFolder.cardsAdded(); // 新卡可能落在回退掉的那一段里 ⇒ 帧末按清单重折

    // unread 计数：只有真新 entry 入 timeline 才算（tool-group 合并到旧 group 不算）
    if (inserted && this.store.activeId !== tab.sessionId) {
      tab.unread += 1;
      // 帧末合批：这里每来一行实时记录就走一次，徽标数字攒到帧末一次更新。
      // 只合批这一处：其余整刷对没变的 tab 零 DOM 写（`tab-bar-view.ts` 与上次画的一样就不写），不贵；合批反而让「点完立刻看到」变成下一帧。
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
   * 够不够一屏：最后一张卡的 `getBoundingClientRect().bottom` 有没有够到滚动容器的下沿（真实布局，不吃估值）。
   * 不用「滚得动吗」（`scrollHeight − clientHeight > 1`）：没渲染过的卡贡献的是估值，估高了看起来滚得动、屏幕仍是半屏。
   * 没有布局时（jsdom，rect 恒为 0）退回算术判据 —— 不然补批整条路在测试里被静默关掉。
   * 「够到」把那张卡的下外边距与容器的下内边距算进去：贴底看时最后一张卡下面本来就留着这两截（真浏览器 28 ＋ 48px），
   * 不算的话贴底的 tab 永远「没满」，每切进来一次补一批。
   */
  private contentReachesBottom(tab: Tab): boolean {
    const el = tab.streamEl;
    const view = el.getBoundingClientRect();
    if (view.height <= 0) {
      // 拿不到真实布局（jsdom / 还没插进文档 / tab 不可见）⇒ 退回算术判据。
      return el.scrollHeight - el.clientHeight > 1;
    }
    const last = tab.stream.contentElement.lastElementChild;
    if (!last) return false; // 一张卡都没有 ⇒ 肯定没满
    // 1px 容差：HiDPI 分数像素下 rect 是小数，卡刚好贴到下沿时会差零点几像素。
    const gap = (parseFloat(getComputedStyle(last).marginBottom) || 0) + (parseFloat(getComputedStyle(el).paddingBottom) || 0);
    return last.getBoundingClientRect().bottom + gap >= view.bottom - 1;
  }

  /**
   * 补到一屏填满（`contentReachesBottom`）或账本弹尽：一批 150 条可能只产出几张卡（工具组合并、skip 记录占配额）。
   * 一次同步调用只跑 {@link MATERIALIZE_ROUNDS_PER_CALL} 轮（每一下有界）；没满、账本还有 ⇒ 下一帧接着补
   * （`scheduleFillContinuation`，与上翻补批同一条 rAF 自链）—— 不可滚的元素不产生 scroll，没有别的入口会再补。
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
   * 批量渲染内核（物化 / 上翻补批 / 缓冲 flush 共用）：
   * - sink 不接 onRealUserInput（历史 user 卡不触发自动切 tab）；branch / queue / title 为空操作（收纳时已喂过）；
   * - 不计未读（重放历史不是新消息；缓冲那份在缓冲时已计）；
   * - 惰性高亮 ＋ observe（滚入视口再高亮）；
   * - 插卡前 unwrapAll 摊平（邻居可能在 fold wrap 里）、批内暂停逐卡 snap（免得逐卡强制 reflow）、
   *   插完 reconcile 孤儿 tool_result（同步出账）、rebuildNow 无条件重折（flushPending 在主线没变时会跳过）。
   */
  private renderPayloadsBatch(tab: Tab, payloads: JsonlLinePayload[]): void {
    if (payloads.length === 0) return;
    const ctx: RenderContext = {
      parentPath: tab.parentPath,
      speaker: speakerNameOf(tab.agent),
      origin: tab.origin,
      toolUseNames: tab.toolUseNames,
      toolUseElements: tab.toolUseElements,
      runCards: tab.runCards,
      pendingToolResults: tab.pendingToolResults,
      needs: tab.needs, // 在等你的那一步建出来就画成「在等你」
      stepWait: (call) => tab.pending.find((p) => p.id === call),
      retryOutcome: (id) => tab.retries.get(id),
      runLabelOf: (run) => this.host.runLabelOf(tab.sessionId, run),
      lazy: true,
    };
    const sink: StreamSink = {
      timeline: tab.timeline,
      enhanceRoot: tab.streamEl, // IO 的 root = 本 tab 的滚动容器
      // 本机远端都挂（两条路都只要 sid）。
      onCardRendered: (el, rec) => {
        if (rec.t !== "said" && rec.t !== "reply") return;
        attachBranchButton(el, {
          uuid: rec.id,
          onFork: (uuid) => this.host.forkFrom(tab, uuid),
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

  /** 物化 tab 的尾段：从窗口账本弹出 seq 最高的 ≤ k 条建卡。对象是没有滚动位置要保的 tab，不做滚动补偿（那在 fillAbove）。 */
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
        if (this.store.activeId !== tab.sessionId) {
          this.parkedSkeletons.set(tab, got.ledger); // 已经切走：停着，切回来再接
          return;
        }
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
      const u = p.record.id;
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
            (p) => tab.seenSeqs.has(p.seq) && routeMeta(p, NOOP_META) === "content",
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
   * 历史的轮次结束会弹系统通知。
   * ⇒ 对这一个 tab 走一遍批：`inBatch` 置位（`userActive` / `turnEndNotifier` 都认它）；
   * 喂完把批期缓冲的中部插入一次挂载、按已有的主线外清单重折（增量读不知道之后的回退，清单由实时帧给）。
   * 若此刻本来就在一个真批里（启动重放未完），只喂不收 —— 真批的 `onBatchEnd` 会收。
   */
  private feedHistoryRows(tab: Tab, payloads: JsonlLinePayload[]): void {
    if (payloads.length === 0) return;
    const wasBatch = this.store.inBatch;
    this.store.inBatch = true;
    // 取回来的是历史：远端 tab「见行就翻活」那一格不认它（`TabStore.historyFeed` 头注）。
    this.store.historyFeed = true;
    try {
      for (const p of payloads) this.host.onLine(p);
    } finally {
      this.store.historyFeed = false;
      this.store.inBatch = wasBatch;
      if (!wasBatch) {
        this.flushMidBatchBuffer(tab);
        tab.branchFolder.rebuildNow();
      }
    }
  }

  /**
   * **按行号往下取一批**：账本空了、渲染窗口最老那一条
   * 不是第 0 行 ⇒ 问 `[floor − FILL_BATCH, floor)`（`record-reads.ts::readLines`，后端 `history-lines`）。
   *
   * 这是没接骨架的 tab 的取回路（接了骨架的按字节取，`fetchMissingRows`）。monitor 的重放缓冲每个会话
   * 只留尾巴（`event_replay·rs::REPLAY_TAIL_KEEP`），F5 之后更早的从这里要回来；没被修剪过的会话问一次就到顶。
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
          if (tab.seenSeqs.has(p.seq) && routeMeta(p, NOOP_META) === "content") {
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
   * 它的内容等下一次宣告 / 下一行，或往上翻按行号取）。
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
   * 批期缓冲的「窗口内中部插入」（大增量批的老块）一次挂载：排序后走渲染内核（含 unwrapAll / rebuildNow ——
   * 随后的 flushPending 在主线没变时会跳过重折）。在 onBatchEnd 的 flushPending / reconcile 之前调。
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
   * 上翻补批。守卫：防重入 / 账空 / 选区进行中（unwrap / rebuild 会杀掉进行中的选区，等下次 scroll 再试）。
   * 补偿：临时关原生锚定（免得与手动补偿叠加），测量 → 渲染 → scrollTop 回写在同一个同步任务里（不许 await / rAF 打断）。
   * rAF 自链：零高批 / 不足一屏时没有 scroll 事件，补完复检直到离开触发区或账尽。
   */
  private fillAbove(tab: Tab): void {
    // 骨架接上了 ⇒ 不从尾巴往上一批批补，只物化与视口相交的那段占位
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
    // 调度：自链 —— 向上补料批末复检：仍在触发区且账本有余就再补，切走了或账尽即停
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
   * DEV 探针用：active tab 状态一行 JSON（没有 devtools 时 E2E 断言的出口，经 e2e-probe 热键 → fe_perf 日志）。生产不接线，无副作用。
   * 含三个账本的条数：`branchRecords` / `userInputs` / `pending`（口径见 `branchRecordCount` 头注）。
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
      // ② 主线外清单的条数（后端给的整份 id 集合，不含正文）
      branchOff: tab.branchFolder.offCount,
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
   * 顶端哨兵：账本非空时置顶「还有 N 条更早消息」，账尽移除。
   * 不是 timeline 实体、没有 data-uuid（BranchFolder 当它断段，不会被折）；二分插入的锚点恒为 timeline 元素，最老卡自然落在哨兵后。
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

  /** 后台 tab 的空闲物化队列（串行；没有 rIC 时 setTimeout 兜底） */
  materializeQueue: string[] = [];
  private materializeScheduled = false;

  /** 后台空闲物化一截建几条（建完一截看一眼空闲期限还剩没剩）。 */
  private static readonly IDLE_CHUNK = 20;
  /** 没有 requestIdleCallback 的引擎（WebKitGTK）走 setTimeout 兜底：一次最多连着建这么久（ms）就让出来。 */
  private static readonly IDLE_SLICE_MS = 8;
  /** virgin 后台 tab 的尾段这一轮已经建了几条（分截建，凑满 `MATERIALIZE_TAIL_K` 为止）。 */
  private readonly idleTaken = new WeakMap<Tab, number>();

  private scheduleIdleMaterialize(): void {
    if (this.materializeScheduled) return;
    const sid = this.materializeQueue.shift();
    if (sid === undefined) return;
    this.materializeScheduled = true;
    const run = (deadline?: IdleDeadline): void => {
      this.materializeScheduled = false;
      const tab = this.store.tabs.get(sid);
      // virgin 的物化尾段(switchTo 可能已同步物化过);二次 batch 开始则原样跳过,
      // 账本继续收纳,批结束会重新排队。
      // 钉过水位、账本有余、真实布局没满一屏的 ⇒ 补一批（`fillAbove`：带选区守卫与滚动补偿，
      // 后台 tab 不自链 —— 它的 rAF 复检有 `activeId` 守卫；切进来时 `activate` 那一脚接着补）。
      // virgin 的尾段**分截建**：一截 `IDLE_CHUNK` 条，空闲期限用完（兜底那一路按 `IDLE_SLICE_MS`）就停、这个 tab 排回队首 ——
      // 一口气建满 150 条就是开窗那几秒里的一串长任务（人一开窗就去点，点下去要等它跑完）。
      if (tab && !this.store.inBatch && tab.window.pendingCount > 0) {
        const virgin = tab.window.floorSeq === null || this.idleTaken.has(tab);
        if (virgin) {
          const until = performance.now() + (deadline ? deadline.timeRemaining() : TabStreamView.IDLE_SLICE_MS);
          let taken = this.idleTaken.get(tab) ?? 0;
          do {
            const before = tab.window.pendingCount;
            this.materializeTail(tab, Math.min(TabStreamView.IDLE_CHUNK, TabStreamView.MATERIALIZE_TAIL_K - taken));
            taken += before - tab.window.pendingCount;
          } while (taken < TabStreamView.MATERIALIZE_TAIL_K && tab.window.pendingCount > 0 && performance.now() < until);
          if (taken < TabStreamView.MATERIALIZE_TAIL_K && tab.window.pendingCount > 0) {
            this.idleTaken.set(tab, taken);
            this.materializeQueue.unshift(sid); // 没建完：下一个空闲期接着建它
          } else this.idleTaken.delete(tab);
        } else if (!this.contentReachesBottom(tab)) this.fillAbove(tab);
      }
      this.scheduleIdleMaterialize();
    };
    if (typeof window.requestIdleCallback === "function") {
      // 调度：自链 —— 后台标签页空闲物化队列：处理一个再排自己，队列空即停
      window.requestIdleCallback(run, { timeout: 2000 });
    } else {
      // 调度：自链 —— 上面那条队列在没有 rIC 时的兜底，同一条链（接着建同一个 tab 的下一截只隔一帧，换下一个 tab 隔 200 ms）
      window.setTimeout(() => run(), this.idleTaken.has(this.store.tabs.get(sid) as Tab) ? 16 : 200);
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
