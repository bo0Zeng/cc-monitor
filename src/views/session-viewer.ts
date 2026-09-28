/**
 * 只读历史会话查看器。
 *
 * 用法：HistoryView 点击一条历史条目时实例化这个组件，给定 jsonl_path 加载并渲染。
 * 复用 cards/renderMessage 与实时 Tab 同一套渲染逻辑（user 气泡 / assistant full-width /
 * tool_use 折叠条 / tool_result 合并等），只是数据源换成"一次性 IPC 读全文件"。
 *
 * 与 TabManager 的关系：完全独立。这里不创建 Tab、不影响实时流、不调 event_replay。
 * 关闭查看器后状态彻底释放。
 */

import { Channel } from "@tauri-apps/api/core";
import { commands } from "../ipc/commands";
// 〔步 12·C〕本机那个 origin 的**唯一住址**（Rust 侧是 `inbound_client::LOCAL_ORIGIN`，
// 两侧由 `origin_tests::the_sentinel_agrees_with_the_two_existing_homes` 两向钉着）。
import type { Origin } from "../ipc/origin";
import { MessageStream } from "../stream";
import {
  type JsonlRecord,
  type RenderContext,
  reconcilePendingToolResults,
} from "../cards";
import { BranchFolder } from "../branch-fold";
import { type BranchRecord } from "../branching";
import { RecordTimeline } from "../record-timeline";
import { releaseEnhanceRoot } from "../render";
import {
  renderStreamRecord,
  routeMetaAndBranch,
  type MetaSink,
  type StreamSink,
} from "../render-stream-record";
import { UnrenderedRanges } from "../render-window";
// 〔U3b〕查看器接骨架：与实时 tab **同一个** `SkeletonView`（占位 ＋ 只物化可见区）。
import { SkeletonView, ledgerFromIndex } from "../skeleton-view";
import { readSessionIndex, type SessionIndexResult } from "../session-reads";
import { attachBranchButton } from "../branch-button";
import { runForkFlow } from "../fork-flow"; // G6：分叉完把新会话起起来（E78 起连反馈也在里面）
import type { BranchResult } from "../session-writes";
// 〔SE1〕大纲的清单问后端要（判定只住后端），实时 tab 用的是同一个类
import { OutlineSource } from "./outline-source";
// K-R45：清单界面两条路共用一份，只有一个住址
import { UserInputPanel } from "./user-input-panel";
import { copyText } from "../copy-table";

/**
 * **在一条消息流里按 uuid 找到那张卡、展开挡着它的折叠、滚过去并闪一下。**
 * 找不到返回 `null` 且**什么都不做**（怎么兜底由调用方决定）。
 *
 * # 为什么它是导出的（K-R45 第三轮）
 *
 * 它本轮起有**两个**调用方：本文件的 `scrollToMessage`（搜索命中 + 「我说过的 N 句」）
 * 与 `tabs.ts` 的实时窗口（`KR45D2`）。件 `§5.2` 的 B 段现打核过这一段**真能共用**：
 * 两条路的卡由**同一个** `renderStreamRecord` 建，`data-uuid` 由**唯一一份**
 * `markCardUuid` 写。照抄一份到 `tabs.ts` 的代价是从此两处要一起改。
 *
 * 🔴 **它为什么还住在这个文件里，而不是一个中立的 `views/card-jump.ts`** ——
 * 这是一处**登记在案的将就，不是设计**：本仓有一条 Rust 侧判据
 * （`src/bridge/src/polling_registry.rs::every_scheduling_call_site_is_classified`）
 * 按「文件 × API × 处数」精确对账**全部** `requestAnimationFrame` / `setTimeout` 调用点。
 * 把下面这 2 处 rAF + 1 处 setTimeout 搬进新文件，就必须同时改那张表 ——
 * 而 `src/bridge/` 不在本轮写区。**实测过**：搬进 `views/card-jump.ts` 后全量门禁
 * cargo 那格当场红（逐字读数在件 `§5.6`）。
 * ⇒ 照 `brief` 第 2 / 17 条：不越界、不糊过去，**抬上来请裁**。
 *
 * ⚠ 代价是 `tabs.ts` 要 `import` 本文件（实时窗口 import 历史查看器，方向是别扭的）。
 * 它**不成环**（本文件不 import `tabs.ts`），打包面也没变（两者本来都在包里），
 * 但这是一句「今天这样是因为写区，不是因为对」——**别把它读成本仓的惯例**。
 */
export function revealCard(container: HTMLElement, uuid: string): HTMLElement | null {
  // CSS.escape 防 uuid 里有特殊字符破坏选择器
  const key = CSS.escape(uuid);
  // 〔W5-RENDER R11 · `设计/10 §7` 第 10 条〕卡找不到 ⇒ 再找「被并进工具组 / 被注入进 tool_use」的那一块（`data-member-uuid`）
  const el =
    container.querySelector<HTMLElement>(`[data-uuid="${key}"]`) ??
    container.querySelector<HTMLElement>(`[data-member-uuid="${key}"]`);
  if (!el) return null;
  // 展开所有折叠祖先，确保目标可见。注:ESC 回退段是 div.branch-fold-wrap
  // + .expanded 类(非 <details>)——此前只开 details,命中折叠段内的卡会被
  // 0fr 裁剪、flash 不可见(Batch13 D 审计发现的既有 bug)
  let p: HTMLElement | null = el.parentElement;
  while (p && p !== container) {
    if (p instanceof HTMLDetailsElement) p.open = true;
    if (p.classList.contains("branch-fold-wrap") && !p.classList.contains("expanded")) {
      p.classList.add("expanded");
      p.querySelector(".branch-fold-header")?.setAttribute("aria-expanded", "true");
    }
    p = p.parentElement;
  }
  el.scrollIntoView({ block: "center" });
  // Batch13-F38:首次落点基于 content-visibility 估值几何;双 rAF 后周边已
  // 材料化(真实尺寸),幂等重发一次让 block:center 落点精确
  requestAnimationFrame(() => requestAnimationFrame(() => el.scrollIntoView({ block: "center" })));
  el.classList.add("search-hit-flash");
  // 动画结束后移除 class（再次跳同一条还能重放）
  window.setTimeout(() => el.classList.remove("search-hit-flash"), 2200);
  return el;
}

/**
 * 历史会话的 jsonl 文件名**就是** sid（口径同 `remote_history::jsonl_stem`）。
 * 远端分叉只认 sid，而查看器手上只有路径，所以从路径取。取不出来 → 空串，
 * 后端的白名单会拒（fail-closed，不会拿一个残缺 id 去跑）。
 */
function sidFromJsonlPath(p: string): string {
  const name = p.split(/[\\/]/).pop() ?? "";
  return name.endsWith(".jsonl") ? name.slice(0, -".jsonl".length) : "";
}

interface JsonlLinePayload {
  session_id: string;
  cwd: string | null;
  path: string;
  /** P5.1：per-file 单调 seq。SessionViewer 一次性 load 时按 seq 排到 timeline。 */
  seq: number;
  message: JsonlRecord;
}

// C04d 批 6a：手写的 `BranchResult` 镜像已删——**包装层的签名直接提供它**，
// 本文件不再需要本地标注（生成物仍被 ipc/commands.ts 的 import 链消费）。

export interface ViewerOptions {
  jsonlPath: string;
  /** 顶栏标题：custom_title / ai_title / first_user_excerpt 之一 */
  displayTitle: string;
  /** 子标题：项目名 + cwd */
  subtitle?: string;
  /**
   * issue #6：从全文搜索结果跳进来时给定命中消息的 uuid。加载完成后定位到该卡片
   * （展开所在折叠段）滚动居中 + 临时高亮，而非默认贴底。
   */
  scrollToUuid?: string;
  /**
   * issue #16：哪台机器的会话（本机 = `LOCAL_ORIGIN`）。
   * 〔C4a · `设计/05 §8` 步 2〕上一版是「`undefined` = 本地」—— 「没说」被当成本机；
   * 现在**必填**（子 agent 查看器就曾因此把远端子 agent 的文件拿去本机读）。
   */
  origin: Origin;
  /**
   * F62：会话工作目录（= 历史条目 projectPath）。分叉出新会话后作它的起始目录。
   * **G6 订正**：原注释写着"远端会话不建分支，可缺省"——远端现在也能分叉了，
   * 缺了它分叉起会话时会多问一次工作目录。
   */
  cwd?: string;
  /**
   * F77：抑制「从这一轮建分支」按钮。子 agent 记录（点进 agent 看记录）不是可分支的会话——
   * 对子 agent jsonl 建分支会产出残缺/无归属会话，故 F77 传 true 关掉该可操作面。
   */
  suppressBranch?: boolean;
}

const TAIL_INITIAL = 150; // 首屏渲染的末尾条数(实测 37MB 全量 65s → 首屏 1.1s)
const BATCH_SIZE = 200; // 上翻每批补渲染条数(实测 200 条 ≈ 1-2s,肉眼可等的一口)
const TOP_TRIGGER_PX = 800; // 距顶触发补批阈值(约一屏余量,提前于撞顶)

export class SessionViewer {
  private root: HTMLElement;
  private streamEl!: HTMLElement;
  private stream: MessageStream | null = null;
  // Batch13-F39:尾部优先增量渲染状态(load 时重建)
  private payloads: JsonlLinePayload[] = [];
  private unrendered: UnrenderedRanges | null = null;
  private uuidToIdx = new Map<string, number>();
  /**
   * 〔U3b〕骨架层：没渲染的 seq 区间由占位顶住（滚动条一开始就是全会话的），滚到哪物化哪。
   * `null` = 没接上（本机后端不在 / 老后端 / Codex 会话 / seq 对不上）⇒ 行为与之前逐字相同。
   * ⚠ 查看器仍然**全量收正文**（大纲 / 分叉折叠要全量记录，SE1 那一路在把大纲搬到后端）——
   * 骨架在这里买的是滚动条与「只建可见区」，**不是**内存。
   */
  private skeleton: SkeletonView | null = null;
  private renderCtx: RenderContext | null = null;
  private renderSink: StreamSink | null = null;
  private folder: BranchFolder | null = null;
  private branchRecords: BranchRecord[] = [];
  private renderingBatch = false;
  private renderErrors = 0;
  private firstError = "";
  /** D 审计 S1/R 竞态:load 世代号——异步间隙(rAF/Channel)后核对,跨会话残余操作直接丢弃 */
  private loadGeneration = 0;
  private lastFirstScreenMs: number | null = null;
  private onScrollFill = (): void => {
    void this.maybeFillAbove();
  };
  private titleEl!: HTMLElement;
  private subtitleEl!: HTMLElement;
  private statusEl!: HTMLElement;
  // K-R45 甲：用户输入清单（开关在顶栏，面板夹在状态栏与消息流之间）。
  // 界面本体住 `user-input-panel.ts` —— 实时窗口那条路用的是**同一份**。
  private inputs!: UserInputPanel;
  /** 〔SE1〕大纲的数据源；`where` 在 `load` 时换成这一份会话。 */
  private outline!: OutlineSource;
  private outlineWhere: { origin: string; jsonlPath: string } | null = null;
  /** 用户点"返回历史"时调用 */
  private onBack: () => void;

  constructor(onBack: () => void) {
    this.onBack = onBack;
    this.root = this.build();
  }

  get element(): HTMLElement {
    return this.root;
  }

  /**
   * Batch13-F39:两阶段加载(实测 37MB 全量渲染 65.5s → 首屏 1.1s)。
   *
   * 阶段一(收集):后端按 100 行一 chunk 经 Channel 发,前端只收集 payload +
   * 预提取 branch/queue 数据,**不渲染**。
   * 阶段二(增量渲染):收齐后渲染末尾 TAIL_INITIAL 条首屏(+深链岛)→ fold 一次
   * 重建 → 贴底/定位;此后上翻由 maybeFillAbove 按批补渲染,每批先摊平再插入再重折。
   *
   * 取消:dispose() 时 stream = null + loadGeneration 递增,后续 chunk/异步残余
   * 双守卫丢弃;Channel 随 GC 回收,backend 下次 send 返 Err 自然 break。
   */
  async load(opts: ViewerOptions): Promise<void> {
    this.titleEl.textContent = opts.displayTitle;
    this.subtitleEl.textContent = opts.subtitle ?? "";

    this.disposeStream();
    this.outlineWhere = { origin: opts.origin, jsonlPath: opts.jsonlPath };
    const gen = ++this.loadGeneration;
    this.streamEl.replaceChildren();
    this.stream = new MessageStream(this.streamEl);

    this.statusEl.textContent = copyText("sessionViewer.load.loading");

    // Batch13-F39:lazy hljs(此前 viewer eager 全量高亮,是 65s 的组成部分)
    const ctx: RenderContext = {
      parentPath: opts.jsonlPath,
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      pendingToolResults: new Map(),
      // Batch9-F29（审计三家共识）：远端会话展开 subagent 需 origin 降级
      origin: opts.origin,
      lazy: true,
    };
    const timeline = new RecordTimeline(this.stream);
    // Batch13-F39:增量渲染期间 renderStreamRecord 会重复 feed branch 记录——
    // branch/queue 数据在收集阶段一次性预提取,sink 的对应回调置 no-op
    const sink: StreamSink = {
      timeline,
      onBranchRecord: () => {},
      onQueueOperation: () => {},
      enhanceRoot: this.streamEl, // 〔W5-RENDER R5 · `设计/10 §3.5` D2〕IO 的 root = 查看器自己的滚动容器
      // F62 / **G6**：给每张 user/assistant 卡挂「从这一轮分叉」按钮。**远端也挂**——
      // 远端走后端的 `--fork-session`（只认 sid），不再受"远端 jsonl 本机够不着"所限。
      // F77：子 agent 记录 `suppressBranch` 仍关掉（子 agent jsonl 不是可分支的会话）。
      onCardRendered: opts.suppressBranch
        ? undefined
        : (el, msg) =>
            this.attachBranchButton(el, msg, opts.jsonlPath, opts.cwd, opts.origin),
    };
    this.renderCtx = ctx;
    this.renderSink = sink;
    this.payloads = [];
    this.uuidToIdx.clear();
    this.branchRecords = [];
    this.renderErrors = 0;
    this.firstError = "";
    const queuedContents: string[] = [];
    let totalRecords = 0;
    const t0 = performance.now(); // Batch13-F39 实测仪表:首屏耗时常驻状态栏

    // 渲染韧性 + 探针：renderStreamRecord 在 Channel 回调里跑，一旦某条记录渲染
    // 抛错，异常**不会**被下面 load() 的 try/catch 接住（不同事件回合），会导致
    // totalRecords 卡住 → while 循环空转 → 整个查看器空白（已观察到的 bug）。
    // 这里逐条 try/catch：单条失败不影响其余，并记录首个错误供定位 / 显示。
    // F39:Channel 阶段只收集 payload + 预提取 branch/queue 数据,不渲染——
    // 全量渲染 37MB 实测 65s,渲染延后到「尾段首屏 + 上翻增量」
    // F40c(账本收敛,清偿 F39 parity 欠账):meta/branch 提取与渲染路径共用
    // routeMetaAndBranch 单一来源——此前手工复刻曾被 D 审计点名为漂移风险。
    const collectSink: MetaSink = {
      onBranchRecord: (br) => this.branchRecords.push(br),
      onQueueOperation: (content) => queuedContents.push(content),
      onTitleUpdate: () => {}, // viewer 标题静态,不消费 ai-title
    };
    // 〔U3b〕骨架索引与正文**并行**要（索引是另一个后端进程，~0.1 s / 50 MB）；接骨架在首屏之后。
    const origin = opts.origin;
    // 〔C4b〕经通道直接问那台后端（`session-reads.ts`）；要不到 ⇒ `available:false`，它自己不抛。
    const indexP = readSessionIndex(origin, opts.jsonlPath, 0);
    const channel = new Channel<JsonlLinePayload[]>();
    channel.onmessage = (chunk) => {
      if (!this.stream || this.loadGeneration !== gen) return; // 已 dispose / 已换会话
      for (const p of chunk) {
        // 逐条 try/catch:异形 message 抛错不能丢整 chunk 计数,否则下面
        // while totalRecords<finalCount 永久空转(旧版就修过这类卡死)
        try {
          routeMetaAndBranch(p, collectSink);
        } catch (err) {
          console.warn("[session-viewer] 收集阶段单条异常(跳过):", err);
        }
        this.payloads.push(p); // 占位必须 push:下标与 finalCount 对齐(meta 也占位)
      }
      totalRecords += chunk.length;
      this.statusEl.textContent = copyText("sessionViewer.load.receiving", { totalRecords });
    };

    try {
      // 🔴 **〔步 12·C 2026-09-20〕两条命令收成了一条。**
      //
      // **这里的历史值得留着，因为它正好是 `设计/00 §2.5 ①` 的反面教材**：
      // C04a 量到这里是个「动态派发口」（`const ipc = origin ? "A" : "B"`），
      // C04d 批 6a 把它改成两次静态调用，买到的是「两条命令各拿精确签名」——
      // 那是**在两条命令这个前提下**能买到的最好结果。本步把前提换掉：
      // 只有一条命令，`origin` 是它的参数，于是既没有派发口、也没有第二个签名。
      //
      // ⚠ **原先那条编译期保护没有丢**：从前是「给本地命令传 origin」编译错，
      //   现在是「不传 origin」编译错（`origin` 必填）。方向反了，牙没掉。
      const finalCount = await commands.stream_read_session_jsonl({
        origin: opts.origin,
        jsonlPath: opts.jsonlPath,
        onChunk: channel,
      });
      // **竞态修复**：Channel 和 invoke 是两条独立 IPC 通道，invoke resolve 时
      // 余下 chunk 的 onmessage 可能还排队没跑。等 totalRecords 追上 finalCount
      // 再切到最终状态文，否则会被晚到的 onmessage 又改回"加载中"。
      while (totalRecords < finalCount) {
        await new Promise((r) => setTimeout(r, 0));
        if (!this.stream || this.loadGeneration !== gen) return; // dispose / 已换会话
      }
      if (!this.stream) return;
      // F39:排序防御(chunk 应有序,二分插入也容乱序,排序让区间账本与 payload 下标对齐)
      this.payloads.sort((a, b) => a.seq - b.seq);
      this.uuidToIdx.clear();
      this.payloads.forEach((p, i) => {
        const u = (p.message as { uuid?: string }).uuid;
        if (u) this.uuidToIdx.set(u, i);
      });
      this.unrendered = new UnrenderedRanges(this.payloads.length);
      // fold 组件建一次,增量批后幂等重建(branchRecords 全量已知;搬的只是已渲染卡)
      this.folder = new BranchFolder(this.stream.contentElement);
      for (const c of queuedContents) this.folder.addQueuedContent(c);

      // 首屏:深链 → 目标岛 + 尾段;否则只尾段
      const total = this.payloads.length;
      const targetIdx = opts.scrollToUuid
        ? (this.uuidToIdx.get(opts.scrollToUuid) ?? null)
        : null;
      this.renderRange(Math.max(0, total - TAIL_INITIAL), total);
      if (targetIdx !== null && this.unrendered.contains(targetIdx)) {
        this.renderRange(Math.max(0, targetIdx - 100), Math.min(total, targetIdx + 100));
      }
      this.rebuildFold();
      this.lastFirstScreenMs = Math.round(performance.now() - t0);
      this.updateStatus(total);
      // K-R45 甲：清单建在这里。面板默认收着 ⇒ 对下面的定位/贴底**零布局影响**
      // （建完就滚，滚之前插一块可见的东西会把落点顶歪）。
      this.rebuildUserInputs();
      // issue #6：从搜索结果跳进来 → 定位到命中消息；否则默认贴底。
      if (opts.scrollToUuid) {
        this.scrollToMessage(opts.scrollToUuid);
      } else {
        this.stream?.scrollToBottom();
      }
      // 上翻补批:挂在 .stream 滚动容器上(dispose 时随 streamEl 替换自然解绑)
      this.streamEl.addEventListener("scroll", this.onScrollFill, { passive: true });
      // R1(D 审计):短会话首屏不足一屏时永远不会有 scroll 事件——主动踢一脚自链
      requestAnimationFrame(() => void this.maybeFillAbove());
      // 〔U3b〕索引到了就接骨架（首屏已经在了，不等它）
      void indexP.then((res) => this.attachSkeleton(gen, res));
    } catch (e) {
      this.statusEl.textContent = copyText("sessionViewer.load.failed", { e: String(e) });
    }
  }

  /** 升序 payloads 里第一个 `seq >= x` 的下标 */
  private idxAtSeq(x: number): number {
    let l = 0;
    let r = this.payloads.length;
    while (l < r) {
      const m = (l + r) >>> 1;
      if (this.payloads[m].seq < x) l = m + 1;
      else r = m;
    }
    return l;
  }

  /**
   * 〔U3b〕接骨架。先对拍 seq 空间（抽几条 payload，它们的 uuid 在索引里必须落在同一个 seq 上 ——
   * 查看器两条读路子步 1 起按可计行编号；Codex 会话 / 读完之间文件被改写 ⇒ 对不上就不接），
   * 再把 `UnrenderedRanges` 的每个洞翻成 seq 区间画成占位：洞 `[a,b)` 的 seq 区间从上一个已渲染记录的
   * 下一行起、到下一个已渲染记录为止（夹在中间的不可显示行一并归进去，高为 0）。
   */
  private attachSkeleton(
    gen: number,
    res: SessionIndexResult | undefined,
  ): void {
    if (!res || !this.stream || this.loadGeneration !== gen || !this.unrendered || !this.renderCtx) return;
    const got = ledgerFromIndex(res);
    if (!got.ok) {
      console.info(`[session-viewer] 骨架未接：${got.reason}`);
      return;
    }
    const ledger = got.ledger;
    let checked = 0;
    for (const p of this.payloads) {
      const u = (p.message as { uuid?: unknown }).uuid;
      if (typeof u !== "string") continue;
      if (ledger.uuidToSeq.get(u) !== p.seq) {
        console.warn(`[session-viewer] 骨架未接：seq ${p.seq} 在索引里是 ${String(ledger.uuidToSeq.get(u))}`);
        return;
      }
      if (++checked >= 8) break;
    }
    const n = this.payloads.length;
    const gaps = this.unrendered.holes.map(([a, b]): [number, number] => [
      a === 0 ? ledger.base : this.payloads[a - 1].seq + 1,
      b < n ? this.payloads[b].seq : ledger.endSeq,
    ]);
    const view = new SkeletonView(ledger, this.streamEl, this.renderSink!.timeline, {
      materialize: (lo, hi) => {
        this.renderRange(this.idxAtSeq(lo), this.idxAtSeq(hi));
        this.rebuildFold();
      },
    });
    view.attachGaps(gaps);
    this.skeleton = view;
    view.fillVisible();
    this.updateStatus(n);
  }

  /** F39:渲染 payload 下标区间 [lo,hi)(逐条 renderStreamRecord,二分插入保序) */
  private renderRange(lo: number, hi: number): void {
    if (!this.renderCtx || !this.renderSink || !this.unrendered || !this.stream) return;
    // 不变量:二分插入只发生在**摊平**的 DOM 上——邻居若已被 fold wrap 收编,
    // insertBefore 会 NotFoundError(E2E 实测 58 条失败)。先摊平,批后重折。
    this.folder?.unwrapAll();
    const from = Math.max(0, lo);
    const to = Math.min(this.payloads.length, hi);
    // S-7 对齐(Phase G 终审:同协议修复双向回灌)——批内暂停逐卡守卫 snap:
    // 首屏 150 卡逐卡 snap 各读一次 scrollHeight = 150 次强制 reflow,直接摊在
    // 首屏耗时里;批末按粘底状态一次贴底(与 tabs.renderPayloadsBatch 同款)。
    this.stream.batchInsert(() => {
      for (let i = from; i < to; i++) {
        if (!this.unrendered!.contains(i)) continue; // 已渲染(岛重叠)跳过
        const p = this.payloads[i];
        try {
          renderStreamRecord(p, this.renderCtx!, this.renderSink!);
        } catch (err) {
          this.renderErrors += 1;
          if (!this.firstError) {
            const t = (p as { message?: { type?: string } })?.message?.type ?? "?";
            this.firstError = `seq=${p?.seq} type=${t}: ${String(err)}`;
            console.error("[session-viewer] renderStreamRecord 抛错", p, err);
          }
        }
      }
    });
    this.unrendered.markRendered(from, to);
    // R2(D 审计):批缝落在 tool_use/tool_result 配对中间时,result 先渲染成
    // fallback 孤儿卡;上方批把 tool_use 补出来后必须回填合并(TabManager 每批
    // onBatchEnd 都做,viewer 此前从未调过——乱序渲染下是确定性视觉回归)。
    // F40b S-6:孤儿卡出 DOM 的同时出账,防悬空 anchor。
    if (this.renderCtx) {
      for (const el of reconcilePendingToolResults(this.renderCtx)) {
        this.renderSink?.timeline.removeByElement(el);
      }
    }
  }

  /**
   * F62 / G4：给一张 user/assistant 卡挂「从这一轮分叉」按钮。
   *
   * **按钮本体已抽成共享组件** `branch-button.ts`（G4）——历史查看器与实时 tab 用同一份，
   * off-main 的呈现区分也在那里。本方法只负责「这条记录该不该有按钮」和「成功之后干什么」。
   */
  private attachBranchButton(
    cardEl: HTMLElement,
    message: JsonlRecord,
    jsonlPath: string,
    cwd: string | undefined,
    origin: Origin,
  ): void {
    if (message.type !== "user" && message.type !== "assistant") return;
    const uuid = message.uuid;
    if (!uuid) return;
    attachBranchButton(cardEl, {
      uuid,
      // 两条路都只认 sid（〔`K-R88` 09-13〕本机那条也收成 sid 了）。查看器手上没有独立的
      // sid 字段，但历史会话的文件名**就是** sid（`remote_history::jsonl_stem` 是同一口径），
      // 所以从路径取。
      sourceSessionId: sidFromJsonlPath(jsonlPath),
      origin,
      cwd,
      onForked: (res) =>
        void this.startForkedSession(res, sidFromJsonlPath(jsonlPath), cwd ?? null, origin),
    });
  }

  /**
   * G6：分叉产出新会话文件之后 —— **起它**（不碰原会话）。
   * 与实时 tab 那条走**同一个** `runForkFlow`，两处行为不许分裂。
   */
  private async startForkedSession(
    res: BranchResult,
    sourceSessionId: string,
    cwd: string | null,
    origin: Origin,
  ): Promise<void> {
    // E78：与实时 tab 那条走**同一句** —— 查事实、起会话、反馈全在 `runForkFlow` 里。
    // `sourceSessionId` 是**源**会话的（新会话此刻还没起，查它必定"查不到"、白弹一次窗）。
    await runForkFlow({
      origin,
      newSessionId: res.sessionId,
      sourceSessionId,
      cwd,
    });
  }

  // G6：原来这里有个 `resumeBranch`（分叉后弹 toast、用户再点一下才 resume）。
  // 它被 `fork-flow.ts` 顶掉了——分叉之后**直接起**，而且远端/本机、带不带账号、
  // 进不进 tmux 全在那一条路上决定。它那条 F06 纪律（sid 校验先于任何 IPC 往返）
  // **没有丢**，搬进了 `fork-flow.ts` 的 `startLocal`。

  /** F39:增量批后幂等重建 fold(branchRecords 全量;未渲染 uuid 的卡不在 DOM,自然跳过) */
  private rebuildFold(): void {
    if (!this.folder || this.branchRecords.length === 0) return;
    try {
      this.folder.setRecordsAndRebuild(this.branchRecords);
    } catch (err) {
      this.renderErrors += 1;
      if (!this.firstError) this.firstError = `branch-fold: ${String(err)}`;
      console.error("[session-viewer] BranchFolder.setRecordsAndRebuild 抛错", err);
    }
  }

  private updateStatus(total: number): void {
    const left = this.unrendered?.remaining ?? 0;
    const shown = total - left;
    const err =
      this.renderErrors > 0 ? copyText("sessionViewer.status.renderErrors", { renderErrors: this.renderErrors, firstError: this.firstError }) : "";
    const ms = this.lastFirstScreenMs !== null ? copyText("sessionViewer.status.firstScreen", { ms: this.lastFirstScreenMs }) : "";
    // 顶部还有洞 → "上翻加载";只剩深链岛-尾段之间的内部缝 → 如实说(上翻无洞可补)
    const fillable = this.unrendered
      ? this.unrendered.gapAbove(this.unrendered.lowestRenderedIdx()) !== null
      : false;
    this.statusEl.textContent =
      left > 0
        ? copyText("sessionViewer.status.partial", { shown, total, ms, hint: fillable ? copyText("sessionViewer.status.fillable") : copyText("sessionViewer.status.gap"), err })
        : copyText("sessionViewer.status.full", { total, ms, err });
  }

  /** R1:触发判定——不足一屏(无滚动条,事件永远不来)或滚近顶部 */
  private shouldFill(): boolean {
    const el = this.streamEl;
    return el.scrollHeight - el.clientHeight <= 1 || el.scrollTop <= TOP_TRIGGER_PX;
  }

  /**
   * F39:滚近顶部/不足一屏 → 往上补一批。
   * 视口稳定不再依赖原生 overflow-anchor(D 审计 R3:每批 fold 全量重建会销毁
   * 锚点节点致跳视口;且 scrollTop==0 时规范不做补偿、WebKitGTK 根本没有锚定)
   * ——改为手动补偿:突变同一任务内完成,临时关原生锚定,按 scrollHeight 差值回写。
   * 批后自链复检(R1:零高批/短内容场景没有 scroll 事件可依赖)。
   */
  private async maybeFillAbove(): Promise<void> {
    // 〔U3b〕接上骨架 ⇒ 不再「从顶上往上一批批补」，只物化与视口相交的那段占位（不自链）
    if (this.skeleton) {
      if (this.skeleton.fillVisible() > 0) this.updateStatus(this.payloads.length);
      return;
    }
    if (this.renderingBatch || !this.unrendered || this.unrendered.isEmpty) return;
    if (!this.shouldFill()) return;
    // 选区守卫(Phase G 终审:与 tabs.fillAbove 对齐)——补批的 unwrapAll/rebuildFold
    // 会杀进行中的选区,等下次 scroll 再试
    const sel = document.getSelection();
    if (sel && !sel.isCollapsed) return;
    const gap = this.unrendered.gapAbove(this.unrendered.lowestRenderedIdx());
    if (!gap) return;
    const gen = this.loadGeneration;
    this.renderingBatch = true;
    this.statusEl.textContent = copyText("sessionViewer.maybeFillAbove.loadingEarlier");
    try {
      // 让状态文先绘一帧再做同步渲染批
      await new Promise((r) => requestAnimationFrame(() => r(null)));
      // 世代守卫:rAF 间隙里可能已切换会话(旧 gap 套新会话会渲出错乱岛/覆写状态栏)
      if (!this.stream || this.loadGeneration !== gen) return;
      const [a, b] = gap;
      const el = this.streamEl;
      const beforeH = el.scrollHeight;
      const beforeTop = el.scrollTop;
      try {
        el.style.overflowAnchor = "none";
        this.renderRange(Math.max(a, b - BATCH_SIZE), b);
        this.rebuildFold();
        el.scrollTop = beforeTop + (el.scrollHeight - beforeH);
      } finally {
        // 还原必须在 finally(Phase G 终审,三家共识——与 tabs.fillAbove 的
        // F40b-D 修复对齐):renderRange 的 unwrapAll/reconcile 段抛出会留下
        // overflow-anchor:none,该 viewer 会话永久失去原生锚定(§21.2)
        el.style.overflowAnchor = "";
      }
      this.updateStatus(this.payloads.length);
    } finally {
      this.renderingBatch = false;
    }
    // 自链:下一帧复检(补批通常把 scrollTop 顶过阈值自然停;零高批/不足一屏则继续)
    requestAnimationFrame(() => void this.maybeFillAbove());
  }

  /**
   * issue #6：滚动定位到指定 uuid 的卡片并临时高亮。
   * 命中卡片可能被折叠在 ESC 回退段（`<details>`）里 → 先展开所有祖先 details 再滚。
   * 找不到（极少：该 uuid 未渲染成带 data-uuid 的卡）则退化为贴底。
   *
   * 🔴 **本轮（K-R45 第三轮）改了两处形状，逐字记下来**（`KR45D0` 的告诫要求写明）：
   * ① 「找卡 → 展开折叠 → 滚 → 闪」那一段提成了本文件里导出的 `revealCard`
   *    —— 它本轮起有**两个**调用方（这里 + `tabs.ts` 的实时窗口），照抄一份的代价是
   *    从此两处要一起改。提的是**整段、逐字**，一个分支都没改。
   *    （它为什么没搬去一个中立文件，见 `revealCard` 的头注 —— 写区的将就，已请裁。）
   * ② 返回值从 `void` 变成「落到的那张卡 / `null`」—— 调用方本来就要知道跳没跳到
   *    （`user-input-panel.ts` 此前是自己再 `querySelector` 一遍**猜**的）。
   * **留在这里没搬的**是两条路结构上不同的那两段：「没渲染就先渲出来」（`uuidToIdx`
   * + `UnrenderedRanges`，实时窗口没有）与「找不到就退到底部」（实时窗口本来就贴底）。
   * 改之前那一版过不过：`KR45D0` 那 6 格在改前改后都是绿的（读数在件 `§5.5`）。
   */
  private scrollToMessage(uuid: string): HTMLElement | null {
    // F39:目标还没渲染(非首屏路径调进来,如未来的重复定位)→ 先渲染目标岛
    const idx = this.uuidToIdx.get(uuid);
    // 〔U3b〕接上骨架 ⇒ 岛也经骨架物化（占位要跟着切开，不许在占位中间凭空插一段卡）
    const seq = idx !== undefined ? this.payloads[idx]?.seq : undefined;
    if (this.skeleton && seq !== undefined && this.skeleton.isPending(seq)) {
      this.skeleton.ensure(seq, 100);
      this.updateStatus(this.payloads.length);
    } else if (idx !== undefined && this.unrendered?.contains(idx)) {
      this.renderRange(Math.max(0, idx - 100), Math.min(this.payloads.length, idx + 100));
      this.rebuildFold();
      this.updateStatus(this.payloads.length);
    }
    const el = revealCard(this.streamEl, uuid);
    if (!el) {
      this.stream?.scrollToBottom();
      return null;
    }
    return el;
  }

  // ==== K-R45 甲 · 用户输入清单（大纲） ====

  /**
   * 〔SE1 · `设计/10 §2.2b ⑥`〕清单**问后端要**（`--list-user-inputs`），不再扫 `payloads`。
   *
   * 原先这里对全量 `payloads` 调前端那份 `collectUserInputs` —— 口径没错，但它是前端的判定，
   * 后端出了这份清单之后留着它就是「各写一遍判定」（设计逐字禁掉的那一形）⇒ 判定只住后端，
   * 两个宿主（本查看器 / 实时 tab）走同一个 `OutlineSource`。
   *
   * 界面与「跳完回头核一次落点」那一段住 `user-input-panel.ts`（实时窗口同一份）。
   */
  private rebuildUserInputs(): void {
    void this.outline.refresh();
  }

  /** 主动释放（HistoryView 卸载本组件时调） */
  dispose(): void {
    this.disposeStream();
  }

  private disposeStream(): void {
    this.streamEl?.removeEventListener("scroll", this.onScrollFill);
    if (this.streamEl) releaseEnhanceRoot(this.streamEl); // 〔W5-RENDER R5〕上一个会话的卡随 IO 一起放掉
    if (this.stream) {
      this.stream.dispose();
      this.stream = null;
    }
    // 〔U3b〕骨架随会话走
    this.skeleton?.dispose();
    this.skeleton = null;
    // F39:释放增量渲染状态(payloads 可达 37MB 量级)
    this.payloads = [];
    this.unrendered = null;
    this.uuidToIdx.clear();
    this.renderCtx = null;
    this.renderSink = null;
    this.folder = null;
    this.branchRecords = [];
    this.renderingBatch = false;
    this.lastFirstScreenMs = null;
    // K-R45 甲：清单也要跟着释放 —— 留着就是上一个会话的句子挂在下一个会话上，
    // 点下去按 uuid 找不到卡，正好落进「静默跳到看不见的东西上」那一形。
    // 〔SE1〕`reset` 同时让在途那趟回来后不许回写（换会话之后迟到的清单不属于这一份）。
    this.outline?.reset();
  }

  // (旧的 renderAll 被流式 load 替代，删了 —— v2.2 issue #12)

  // === DOM ===

  private build(): HTMLElement {
    const view = document.createElement("div");
    view.className = "session-viewer";

    // 顶栏
    const bar = document.createElement("div");
    bar.className = "session-viewer-bar";

    const backBtn = document.createElement("button");
    backBtn.type = "button";
    backBtn.className = "history-back";
    backBtn.textContent = copyText("sessionViewer.build.back");
    backBtn.addEventListener("click", () => this.onBack());
    bar.appendChild(backBtn);

    // K-R45：清单面板（与实时窗口共用一份实现）。
    // 「怎么跳」与「跳空了怎么解释」是两条路唯一不同的地方，所以只有这两件传进去。
    this.inputs = new UserInputPanel({
      jumpTo: (uuid) => this.scrollToMessage(uuid),
      // 查看器这一侧落空的成因是自陈的那条不等价：渲染会再剥一层 `stripInternalNoise`，
      // 剥空了**不建卡**（后端 `observe/user_inputs.rs` 头注那条「已知不等价」）。`scrollToMessage` 会退到底部。
      unjumpableHint: copyText("sessionViewer.build.unjumpable"),
    });
    this.outline = new OutlineSource(this.inputs, () => this.outlineWhere);
    // 开关塞在顶栏标题右边（标题那块 flex:1 会吃掉余量）。

    const titles = document.createElement("div");
    titles.className = "session-viewer-titles";
    this.titleEl = document.createElement("div");
    this.titleEl.className = "session-viewer-title";
    titles.appendChild(this.titleEl);
    this.subtitleEl = document.createElement("div");
    this.subtitleEl.className = "session-viewer-subtitle";
    titles.appendChild(this.subtitleEl);
    bar.appendChild(titles);
    bar.appendChild(this.inputs.toggle);

    view.appendChild(bar);

    this.statusEl = document.createElement("div");
    this.statusEl.className = "history-status";
    view.appendChild(this.statusEl);

    // K-R45：清单面板。默认收着 ⇒ 不改任何既有布局。
    // 样式住 `styles.css` 的 `.user-inputs`（内联那笔债上一轮还了）。
    view.appendChild(this.inputs.panel);

    // 消息流容器（与实时 Tab 用相同的 .stream 样式）
    this.streamEl = document.createElement("div");
    this.streamEl.className = "stream session-viewer-stream";
    view.appendChild(this.streamEl);

    return view;
  }
}
