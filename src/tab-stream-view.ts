/**
 * 〔U2 · 拆 `tabs.ts` ③〕**实时流视图** —— 每个 tab 的那条流怎么建、怎么按 seq 门控建卡、
 * 尾部窗口 / 骨架 / 上翻补批 / 哨兵 / 大纲、重放批的开与收。
 *
 * 读写的会话状态全在 `TabStore`（同一个实例）；要别处做的事只有六样（`TabStreamHost`）：
 * 按偏移取回的历史行走 `onLine` 全套 · 刷 tab 栏（立即 / 帧末合批）· 应用 ai-title ·
 * 真用户输入上屏时的自动跟随 · 分叉出来的新会话怎么起。
 *
 * 方法体逐字从 `tabs.ts` 搬来（原是 `TabManager` 的私有方法，或 `ensureTab` / `closeTab` /
 * `switchTo` / `onBatchEnd` / `onLine` 里的整段），唯一的改写：上面六样换成 `this.host.…`，
 * 三个静态常量的类名换成本类。**`materializeUntilFilled` 与 `MATERIALIZE_TAIL_K` 的写法一字未动** ——
 * `replay-tail-keep.vitest.ts` 按原文抽这两个数对拍 Rust 侧的 `REPLAY_TAIL_KEEP`。
 */
import { MessageStream } from "./stream";
import { reconcilePendingToolResults, type RenderContext } from "./cards";
import { BranchFolder } from "./branch-fold";
import { attachBranchButton } from "./branch-button"; // G4：实时会话的分叉入口
import type { BranchResult } from "./generated/BranchResult";
import type { JsonlLinePayload } from "./events";
import { RecordTimeline } from "./record-timeline";
import type { SkeletonLedger } from "./live-window";
// 〔`设计/10` 骨架 · 子步 4〕骨架层（占位 ＋ 只物化可见区）。接入点全部带「骨架」字样，搜得到。
import { SkeletonView, ledgerFromIndex } from "./skeleton-view";
import { skeletonKind } from "./height-estimate";
// K-R45 乙（`KR45D2`）：「大纲」。界面 / 跳 与历史查看器共用同一份；〔SE1〕清单问后端要（`OutlineSource`）。
// 〔SE2〕大纲并进会话内查找面板（`SessionFindPanel`：搜索 / 大纲两个模式，跳只有一个住址）。
import type { UserInputPanel, JumpResult } from "./views/user-input-panel";
import { OutlineSource, outlineSeedFromIndex } from "./views/outline-source";
import { SessionFindPanel } from "./views/session-find";
// ⚠ **实时窗口 import 历史查看器，方向是别扭的 —— 这是写区逼出来的将就，不是惯例。**
// 共用的只有 `revealCard`（找卡→展开→滚，两条路的卡由同一份渲染器建）。把它搬进中立文件
// 要同时改 `src/bridge/src/polling_registry.rs` 的调度点分类账（rAF/setTimeout 按文件精确对账），
// 而 `src/bridge/` 不在本轮写区 —— 实测搬了就红。理由与读数在 `revealCard` 的头注 + 件 `§5.6`。
// 〔U2〕这条原住 `tabs.ts`，随 `revealCard` 的唯一用处（大纲的「跳」）一起搬来。
import { revealCard } from "./views/session-viewer";
import {
  renderContentRecord,
  routeMetaAndBranch,
  type MetaSink,
  type StreamSink,
} from "./render-stream-record";
import type { BranchRecord } from "./branching";
import { commands } from "./ipc/commands";
import type { Tab } from "./tab-model";
import type { TabStore } from "./tab-store";

/** 〔U3b〕只问「这条是不是 meta」、不喂任何账的空 sink（骨架按偏移取回**见过**的行时用）。 */
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
}

/**
 * 秤 6(`设计/17 §6` 表第 6 行):读 `BranchFolder.records` 的**条数**,给 `debugSnapshot`。
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
 * 返 -1 在读数里一眼就是「这根尺子断了」。`tests/scale6-memory-ledger.vitest.ts`
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
  /** F40b:上翻补批批量/触发距离(沿用 F39 实测值) */
  private static readonly FILL_BATCH = 200;
  private static readonly TOP_TRIGGER_PX = 800;
  /** F40b:补批防重入(补偿测量期间嵌套触发会算错差值) */
  private renderingFill = false;
  /** 〔SE2〕每个 tab 的查找面板（关 tab 时摘掉）。`Tab` 上挂的是它的两半：`inputsEl`（整块）与 `inputsPanel`（大纲）。 */
  private readonly finds = new Map<string, SessionFindPanel>();
  /**
   * 〔SE2〕每个 tab 在途的「按偏移取正文」（`fetchMissingRows` 发的那几趟）。
   * 「跳」要等它们落完再找卡 —— 骨架接上之后，没物化的那段正文多半不在前端账本里（U3b `keepHighest`），
   * `ensure` 只是把取正文的请求发出去，同步那一下去找卡必然落空。
   */
  private readonly rangeFetches = new WeakMap<Tab, Set<Promise<void>>>();

  constructor(
    private readonly store: TabStore,
    private readonly streamRootEl: HTMLElement,
    private readonly host: TabStreamHost,
  ) {}

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

    // 〔SE2 · `设计/10 §2.2b ④`〕本 tab 的查找面板：搜索 ／ 大纲两个模式，**跳只有一个住址**（`jumpInTab`）。
    // 原先的独立悬浮层 `.live-user-inputs`（K-R45 乙 · 步 2 止血形）整块由它取代。宿主自己的三件事：
    // ① 怎么查 —— 问后端（`find_in_session` ⇒ `--find-in-session`），问的是这个 tab 的那份会话；
    // ② 怎么跳 —— 见 `jumpInTab`；
    // ③ 跳空了怎么解释 —— 实时这一侧的成因与查看器**不是同一件事**：那边是「渲染时被剥成空卡」
    //    （永久），这边是「还收纳在 `TailWindow` 里没建卡」（**上翻补一批就好了**）。
    const find = new SessionFindPanel({
      search: async (query, includeTools) => {
        const t = this.store.tabs.get(sessionId);
        if (!t?.parentPath) {
          return { available: false, reason: "这个会话的文件位置还没收到", hits: [], total: 0 };
        }
        return commands.find_in_session({
          origin: t.origin,
          jsonlPath: t.parentPath,
          query,
          includeTools,
        });
      },
      jumpTo: (uuid) => this.jumpInTab(sessionId, streamEl, uuid),
      unjumpableHint: "这一条还没加载出来 —— 往上翻到更早的消息之后再点",
    });
    this.finds.set(sessionId, find);
    const inputsEl = find.el;
    const inputsPanel = find.outline;
    this.streamRootEl.appendChild(inputsEl);
    // 〔SE1〕大纲的数据源：路径可能要等首条行回填（骨架 tab），所以每次要的时候现取
    const outline = new OutlineSource(inputsPanel, () => {
      const t = this.store.tabs.get(sessionId);
      return t?.parentPath ? { origin: t.origin, jsonlPath: t.parentPath } : null;
    });
    // v2.2 issue #12: 重放期创建的新 Tab 也进 batch 模式，避免每条 record 都
    // 触发 O(N) computeMainBranch。批结束时 onBatchEnd 会统一 flush。
    if (this.store.inBatch) {
      branchFolder.setBatchMode(true);
    }
    return { streamEl, stream, branchFolder, timeline, inputsEl, inputsPanel, outline };
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
      // 〔`设计/10` 骨架〕接上了 ⇒ 占位可能在任何位置（拖滚动条到中部），**每次滚动**都看一眼
      // 视口里有没有占位 —— 不能沿用「离顶 800px 内才补」那道门（那是尾部窗口单洞后缀的假设）
      if (t?.skeleton) {
        t.skeleton.fillVisible();
        return;
      }
      if (t && t.streamEl.scrollTop <= TabStreamView.TOP_TRIGGER_PX) this.fillAbove(t);
    };
    streamEl.addEventListener("scroll", fillHandler, { passive: true });
    tab.fillHandler = fillHandler;
    // ★ 步 3：**视口自己变大 ⇒ 重新补批。**
    //
    // `fillHandler` 挂在 scroll 上，而**不可滚的元素根本不产生 scroll 事件** ——
    // 把窗口从半屏拉到全屏时，多出来的那块空白之前没有任何入口去填。
    // `MessageStream` 那边现在也观察 `scrollEl`（见 stream.ts），这里是它的消费端。
    // ⚠ 三道门都不可少：① 只给 active tab 补（后台 tab 0×0 → 真实尺寸那一跳不是
    // 「用户拉窗口」）；② 账本空了不补；③ 已经满屏了不补（否则每次 RO 都白干一轮）。
    stream.onViewportResize = (): void => {
      if (this.store.activeId !== sessionId) return;
      const t = this.store.tabs.get(sessionId);
      if (!t || t.window.pendingCount === 0) return;
      // 〔`设计/10` 骨架〕接上了 ⇒ 视口变大露出的是占位，只物化露出来的那段
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
    tab.stream.dispose();
    tab.streamEl.remove();
    // K-R45 乙：大纲跟着走（〔SE1〕`reset` 也让在途那趟回来后不许回写）。
    // 查找面板是 `streamRootEl` 的直接子节点，不随 `streamEl.remove()` 一起走。
    tab.outline.reset();
    this.finds.get(tab.sessionId)?.reset(); // 〔SE2〕在途的查找作废、出弹层栈
    this.finds.delete(tab.sessionId);
    tab.inputsEl.remove();
    // 显式清 Map：释放对已卸载 DOM 节点的强引用，让 GC 可早回收
    // （Map 本身也会随 Tab 对象一起回收，但显式 clear 让 DOM 引用计数立即归零）
    tab.toolUseNames.clear();
    tab.toolUseElements.clear();
    tab.pendingToolResults.clear();
    tab.seenSeqs.clear();
    tab.processedUuids.clear();
    // F40a/b:窗口账本与缓冲持整段历史 payload(大会话数十 MB 级),断引用;摘 fill listener
    tab.window.dispose();
    tab.skeleton?.dispose(); // 〔`设计/10` 骨架〕
    tab.skeleton = null;
    tab.midBatchBuffer = [];
    if (tab.fillHandler) tab.streamEl.removeEventListener("scroll", tab.fillHandler);
    tab.timeline.dispose();
    tab.branchFolder.dispose();
  }

  /** 切 tab：只让这一条流（连同它的查找面板）可见（原是 `switchTo` 开头那一段）。 */
  showOnly(sessionId: string): void {
    for (const [sid, t] of this.store.tabs) {
      t.streamEl.classList.toggle("active", sid === sessionId);
      // K-R45 乙：面板与它那条流**同进同出**。漏掉这一句 = 所有 tab 的面板
      // 一起挂在屏幕上，而且点下去找的是别人的流（`revealCard` 只在自己的 streamEl 里找）。
      t.inputsEl.classList.toggle("active", sid === sessionId);
      // 〔SE2〕切走的 tab 收起面板（出弹层栈）—— 不然 Esc 去关的是一块看不见的面板。
      if (sid !== sessionId) this.finds.get(sid)?.close();
    }
  }

  /**
   * 〔SE2 · `设计/10 §6 步 6`〕Ctrl+F（动作 `session.find`）：当前 tab 的查找面板打开到「搜索」、焦点进输入框。
   * 没有 active tab ⇒ 什么都不做。
   */
  openFind(): void {
    const sid = this.store.activeId;
    if (sid === null) return;
    this.finds.get(sid)?.open("search");
  }

  /**
   * 〔SE2〕**跳（大纲行与查找命中行共用这一个住址）**：
   * - 骨架没接上 / 这条已经物化 ⇒ 直接找卡（`revealCard`）；
   * - 还在占位里 ⇒ `ensure` 物化它附近那一段；账本里有的当场建卡，没有的按偏移取回（`fetchMissingRows`）——
   *   **等这个 tab 在途的取正文全部落完**再找卡。同步那一下去找必然落空（U3b 之后前端账本只留尾巴 200 条）。
   * 等的期间 tab 被关掉 ⇒ 落空（`null`）。
   */
  private jumpInTab(sessionId: string, streamEl: HTMLElement, uuid: string): JumpResult {
    const tab = this.store.tabs.get(sessionId);
    const sk = tab?.skeleton;
    const seq = sk?.ledger.uuidToSeq.get(uuid);
    if (!tab || !sk || seq === undefined || !sk.isPending(seq)) return revealCard(streamEl, uuid);
    sk.ensure(seq);
    const inflight = this.rangeFetches.get(tab);
    if (!inflight || inflight.size === 0) return revealCard(streamEl, uuid);
    return Promise.allSettled([...inflight]).then(() =>
      this.store.tabs.get(sessionId) === tab ? revealCard(streamEl, uuid) : null,
    );
  }

  /** 切进来的 tab：物化 / 哨兵 / 骨架索引 / 大纲 / 不可滚时踢一次补批（原是 `switchTo` 中段，逐字）。 */
  activate(next: Tab): void {
    // Batch13-F40a:命中 virgin tab(启动重放全收纳,还没建过卡)→ 同步物化尾段,
    // 避免切过去一片空白(R-3:有界循环补到可滚,防工具密集会话一轮近空屏)。
    // 非 virgin tab 不动(上翻补批属 F40b)。
    if (next && next.window.floorSeq === null && next.window.pendingCount > 0) {
      this.materializeUntilFilled(next);
    }
    // F40b:切入即刷新哨兵(非 virgin 但账本非空的 tab 也要见到「还有 N 条」)
    if (next) this.updateSentinel(next);
    // 〔`设计/10` 骨架〕切进来的 tab 要索引（上面刚物化过尾段 ⇒ floor 已钉）
    if (next) this.requestSkeleton(next);
    if (next?.outline.needsFetch) this.refreshOutline(next); // 〔SE1〕大纲：有新行才要
    // D 审计 R-2:非 virgin + 不可滚 + 账本有余的 tab 没有 fill 入口(不可滚元素
    // 不产生 scroll 事件,哨兵可见却"上翻物理不可达")——切入时踢一次,rAF 自链
    // 接管直到可滚或账尽。
    if (next && next.window.pendingCount > 0) {
      const el = next.streamEl;
      if (el.scrollHeight - el.clientHeight <= 1) this.fillAbove(next);
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
    // D 审计 S-5:archived 死会话不进后台物化队列(纯浪费;switchTo 命中 virgin
    // 已有同步物化兜底)。
    this.materializeQueue = [...this.store.tabs.entries()]
      .filter(
        ([sid, t]) =>
          sid !== this.store.activeId &&
          t.status !== "archived" &&
          t.window.floorSeq === null &&
          t.window.pendingCount > 0,
      )
      .map(([sid]) => sid);
    this.scheduleIdleMaterialize();
    // F40b:active tab 未走物化分支(尾块已可滚)时也要挂哨兵
    if (active) this.updateSentinel(active);
    // 〔`设计/10` 骨架〕active tab 此刻一定有渲染后缀了 ⇒ 要索引（在途/要过就不重复）
    if (active) this.requestSkeleton(active);
    if (active?.outline.needsFetch) this.refreshOutline(active); // 〔SE1〕大纲
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
        this.refreshOutline(tab); // 〔SE1〕真用户输入上屏 ⇒ 大纲要新的一截
      },
      observeForLazyEnhance: this.store.inBatch,
      // G4（branch-anywhere）：实时会话也挂「从这一轮分叉」按钮。
      // 钩子本来就在共享的 `render-stream-record.ts` 里，此前**只有历史查看器传了它**
      // ⇒ 实时 tab 上没有入口。按钮本体是共享组件（off-main 的呈现区分也在那里）。
      // **G6 起远端也挂**；〔`K-R88` 09-13〕本机那条也收 sid 之后，
      // 「本机拿不到 jsonl 路径」这道门对两条路都不再是门槛。
      onCardRendered: (el, msg) => {
        if (msg.type !== "user" && msg.type !== "assistant") return;
        if (!msg.uuid) return;
        attachBranchButton(el, {
          uuid: msg.uuid,
          sourceSessionId: tab.sessionId,
          origin: tab.origin,
          cwd: tab.cwd ?? undefined,
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
      // 〔`设计/10` 骨架〕floor 之下、但已被物化过的那段（岛）里迟到的行 ⇒ 就地建卡，不收纳
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
      pendingToolResults: tab.pendingToolResults,
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
      // ⚠ 只合批**这一处**。〔UP1 订正〕这里原先的理由是「其余十几个 `refreshTabBar()` 调用点都是用户动作」——
      // **事实不对**：UP1 现数 25 处同步整刷，其中 12 处是事件驱动的（状态帧 / 会话增删 / 账号变更…），不是用户点的。
      // 结论不变（其余照旧同步整刷、不合批），理由换成现在这条：UP1 之后整刷对**没变的 tab 零 DOM 写**
      // （`tab-bar-view.ts` 先算出要画成什么样、与上次画出去的一样就不写，`设计/30 §3` P2 / P7），
      // ⇒ 一次整刷的代价只剩「变了的那几颗按钮」，不合批也不贵；合批反而会把「点完立刻看到」变成「下一帧才看到」。
      // 这一处还合批，是因为它**每来一行 live 记录就走一次**（频率是逐行的，不是逐事件的）。
      this.host.scheduleTabBarRefresh();
    }
  }

  /**
   * 〔SE1〕大纲：一条 live 记录到了 —— 记一笔「又长了」（O(1)，不判是不是用户输入）。
   * 本 tab 是 active、非批期、而且**还一次都没要过**（首个 tab 建出来时路径可能还没到）⇒ 要一次。
   */
  noteOutlineLine(tab: Tab): void {
    tab.outline.markStale();
    if (!tab.outline.everFetched && !this.store.inBatch && this.store.activeId === tab.sessionId) {
      void tab.outline.refresh();
    }
  }

  /**
   * 〔SE1〕大纲：向后端要新的一截（从上次的 `end` 接着要；在途就合并成一趟）。
   * 批期不要 —— 批结束时 active tab 统一要一次，后台 tab 切进来再要（`needsFetch`）。
   */
  private refreshOutline(tab: Tab): void {
    if (this.store.inBatch) return;
    void tab.outline.refresh();
  }

  /**
   * ★ 步 3（`设计/10 §6`）：**「够不够一屏」改读真实布局。**
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
   * 补批兜底。有界循环补到**一屏填满**或账本弹尽(≤4 轮防病态会话空转)。
   * 〔步 3〕停手条件从「滚得动」换成 `contentReachesBottom`——理由见它的头注。
   */
  private materializeUntilFilled(tab: Tab): void {
    if (tab.skeleton) {
      tab.skeleton.fillVisible(); // 〔`设计/10` 骨架〕同上
      return;
    }
    for (let round = 0; round < 4; round++) {
      if (tab.window.pendingCount === 0) return;
      if (round > 0 && this.contentReachesBottom(tab)) return;
      this.materializeTail(tab);
    }
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
      pendingToolResults: tab.pendingToolResults,
      lazy: true,
    };
    const sink: StreamSink = {
      timeline: tab.timeline,
      onBranchRecord: () => {},
      onQueueOperation: () => {},
      observeForLazyEnhance: true,
      // G6：**远端也挂**。〔`K-R88` 09-13〕本机那条命令也收 sid 了 ⇒
      // **两条路都只要 sid**，「本机拿不到 jsonl 路径就不能分叉」这道门跟着没了
      // （原先那个随迭代更新的路径游标也一并去掉：没人再要那个值）。
      onCardRendered: (el, msg) => {
        if (msg.type !== "user" && msg.type !== "assistant") return;
        if (!msg.uuid) return;
        attachBranchButton(el, {
          uuid: msg.uuid,
          sourceSessionId: tab.sessionId,
          origin: tab.origin,
          cwd: tab.cwd ?? undefined,
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
    this.renderPayloadsBatch(tab, tab.window.takeTail(k));
    this.updateSentinel(tab);
  }

  /**
   * 〔`设计/10` 骨架 · 子步 4〕向后端要这个会话的**骨架索引**，到了就接骨架。
   *
   * 只对**已经有渲染后缀**的 tab 要（`floor !== null`：骨架顶的是 `[0, floor)`）；
   * 每个 tab 只要一次（`skeletonFetch`），成不成都不重拉 —— 免得每切一次 tab 起一次本机后端进程。
   * 调用点只有两处：批结束时的 active tab、`switchTo` 切进来的那个 tab ⇒ 后台 tab 不花这一次。
   */
  private requestSkeleton(tab: Tab): void {
    if (tab.skeletonFetch !== "idle" || !tab.parentPath) return;
    if (tab.window.floorSeq === null) return;
    tab.skeletonFetch = "pending";
    const jsonlPath = tab.parentPath;
    const origin = tab.origin;
    const first = commands.read_session_index({ origin, jsonlPath, fromOffset: 0 });
    // 〔SE2 · `设计/10 §9.5` 欠账〕大纲**等这一趟**：索引顺带出清单（后端 `IndexRow::x`）⇒ 首屏同一份文件
    // 只读一遍；带不回（老后端 / 零条 / 失败）⇒ 它自己照旧 `list_user_inputs(0)`。
    tab.outline.awaitSeed(first.then(outlineSeedFromIndex));
    void first
      .then(async (res) => {
        if (this.store.tabs.get(tab.sessionId) !== tab) return; // 期间关掉了
        tab.skeletonFetch = "done";
        const got = ledgerFromIndex(res);
        if (!got.ok) {
          console.info(`[tabs] 骨架未接（${tab.sessionId.slice(0, 8)}）：${got.reason}`);
          return;
        }
        // 索引拉回来之前 tab 可能又长了：floor 之下还有索引没覆盖到的行 ⇒ **续传**（从上次的 end 接着要）
        const floor = tab.window.floorSeq ?? 0;
        if (floor > got.ledger.endSeq) {
          const more = await commands.read_session_index({ origin, jsonlPath, fromOffset: got.end });
          if (more.available) got.ledger.append(more.rows);
        }
        if (this.store.tabs.get(tab.sessionId) !== tab) return;
        this.attachSkeleton(tab, got.ledger);
      })
      .catch((e: unknown) => {
        tab.skeletonFetch = "done";
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
    if (this.store.activeId === tab.sessionId) view.fillVisible();
    // 〔U3b · `设计/10` 步 8〕骨架接上 ⇒ 正文不必再驻留：丢掉的那些滚到时按偏移要回来。
    // ① 前端账本只留离已渲染尾巴最近的一批（第一次上翻不用等 IPC）；
    // ② monitor 的重放缓冲只留尾巴（F5 之后也只重放尾巴，其余同样按偏移要）。
    tab.window.keepHighest(TabStreamView.FILL_BATCH);
    void commands
      .replay_keep_tail_only({ sessionId: tab.sessionId })
      .catch((e: unknown) => console.warn(`[tabs] 重放缓冲留尾巴失败（${tab.sessionId.slice(0, 8)}）：`, e));
  }

  /**
   * 〔`设计/10` 骨架 · 子步 5〕**按偏移取正文**：物化 `[lo, hi)` 时，账本里没有、也还没到过的那些
   * 会建卡的行（`seenSeqs` 里没有、索引说它不是「不建卡」的那种）⇒ 按索引里的字节边界向后端要
   * （`read_session_range` = `--read-session-from-offset … --until`），回来的行**走 `onLine` 全套**
   * （去重、旁路记账、门控 —— 这段已经不在占位里了，门控会就地建卡），与重放来的行一视同仁。
   *
   * 今天它补的是「重放还没推到」的那一截（远端尾部优先快照的回填期、大会话启动重放的在途期）；
   * 它也是「骨架不带正文」那条路的另一半 —— 等 `EventReplay.history` 加上界（`设计/10 步 8`），
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
      // 〔U3b〕「缺」= 会建卡、而这一次没从账本里取到 —— 两种来历：重放没推过来（没见过），
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
      const fetched: Promise<void> = commands
        .read_session_range({
          origin,
          jsonlPath: tab.parentPath,
          offset: first.o,
          until: lastRow.o + lastRow.n,
          seqBase: a,
          lineCount: b - a,
        })
        .then((payloads) => {
          if (this.store.tabs.get(tab.sessionId) !== tab) return;
          // 没见过的 ⇒ 走 `onLine` 全套（旁路记账、去重、门控）；
          // 见过的 ⇒ 旁路账早记过了、去重会把它拒掉 ⇒ 只建卡（meta 那几类照旧不建）。
          const fresh = payloads.filter((p) => !tab.seenSeqs.has(p.seq));
          const again = payloads.filter(
            (p) => tab.seenSeqs.has(p.seq) && routeMetaAndBranch(p, NOOP_META) === "content",
          );
          this.feedHistoryRows(tab, fresh);
          if (again.length > 0) this.renderPayloadsBatch(tab, again);
        })
        .catch((e: unknown) => console.warn(`[tabs] 按偏移取正文失败 [${a},${b})：`, e));
      // 〔SE2〕记进在途集合（「跳」等它落完），落完自己出列
      const set = inflight!;
      set.add(fetched);
      void fetched.finally(() => set.delete(fetched));
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
    tab.branchFolder.setBatchMode(true);
    try {
      for (const p of payloads) this.host.onLine(p);
    } finally {
      this.store.inBatch = wasBatch;
      if (!wasBatch) {
        this.flushMidBatchBuffer(tab);
        tab.branchFolder.flushPending();
        tab.branchFolder.setBatchMode(false);
      }
    }
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
    // 〔`设计/10` 骨架〕接上了 ⇒ 不再「从尾巴往上一批批补」，只物化与视口相交的那段占位
    if (tab.skeleton) {
      tab.skeleton.fillVisible();
      return;
    }
    if (this.renderingFill) return;
    if (tab.window.pendingCount === 0) return;
    const sel = document.getSelection();
    if (sel && !sel.isCollapsed) return;
    const el = tab.streamEl;
    this.renderingFill = true;
    try {
      el.style.overflowAnchor = "none";
      const beforeH = el.scrollHeight;
      const beforeTop = el.scrollTop;
      this.renderPayloadsBatch(tab, tab.window.takeTail(TabStreamView.FILL_BATCH));
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
    requestAnimationFrame(() => {
      if (this.store.activeId !== tab.sessionId) return;
      const t = this.store.tabs.get(tab.sessionId);
      if (!t || t.window.pendingCount === 0) return;
      const e = t.streamEl;
      if (e.scrollTop <= TabStreamView.TOP_TRIGGER_PX || e.scrollHeight - e.clientHeight <= 1) {
        this.fillAbove(t);
      }
    });
  }

  /**
   * F40c DEV 探针用:active tab 状态一行 JSON——无 devtools 环境下 E2E 断言的
   * 唯一出口(经 e2e-probe 热键 → fe_perf 日志)。生产不接线,方法本身无副作用。
   *
   * 🔴 秤 6(`设计/17 §6` 表第 6 行)在这里加了**三个账本的条数**:
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
      // ③ 大纲的条数 —— 〔SE1〕清单问后端要，前端只留面板上那几行（每行一份 80 字摘要）
      userInputs: tab.outline.count,
      midBuffer: tab.midBatchBuffer.length,
      // 〔`设计/10` 骨架〕接上没有 · 索引总条数 · 还在占位里的行数 · 占位块数（`timeline` 里含占位条目）
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
    // 〔`设计/10` 骨架〕接上了 ⇒ 占位本身就是「上面还有」，哨兵退场
    if (n === 0 || tab.skeleton) {
      el?.remove();
      return;
    }
    if (!el) {
      el = document.createElement("div");
      el.className = "stream-more-above";
      content.prepend(el);
    }
    el.textContent = `↑ 还有 ${n} 条更早消息 · 上翻加载`;
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
      // 只物化仍是 virgin 的(switchTo 可能已同步物化过);二次 batch 开始则原样跳过,
      // 账本继续收纳,批结束会重新排队。
      if (tab && !this.store.inBatch && tab.window.floorSeq === null) {
        this.materializeTail(tab);
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
