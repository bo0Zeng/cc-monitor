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

import { invoke, Channel } from "@tauri-apps/api/core";
import { MessageStream } from "../stream";
import { type JsonlRecord, type RenderContext } from "../cards";
import { BranchFolder } from "../branch-fold";
import { type BranchRecord } from "../branching";
import { RecordTimeline } from "../record-timeline";
import { renderStreamRecord, type StreamSink } from "../render-stream-record";
import { extractBranchRecord } from "../branching";
import { UnrenderedRanges } from "../render-window";

interface JsonlLinePayload {
  session_id: string;
  cwd: string | null;
  path: string;
  /** P5.1：per-file 单调 seq。SessionViewer 一次性 load 时按 seq 排到 timeline。 */
  seq: number;
  message: JsonlRecord;
}

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
   * issue #16：远端来源。undefined=本地（走 stream_read_session_jsonl）；
   * host=远端（走 stream_read_remote_session，经 SSH 拉取，chunk 口径一致）。
   */
  origin?: string;
}

const TAIL_INITIAL = 150; // 首屏渲染的末尾条数(实测 37MB 全量 65s → 首屏秒级)
const BATCH_SIZE = 200; // 上翻每批补渲染条数
const TOP_TRIGGER_PX = 800; // 距顶触发补批阈值

export class SessionViewer {
  private root: HTMLElement;
  private streamEl!: HTMLElement;
  private stream: MessageStream | null = null;
  // Batch13-F39:尾部优先增量渲染状态(load 时重建)
  private payloads: JsonlLinePayload[] = [];
  private unrendered: UnrenderedRanges | null = null;
  private uuidToIdx = new Map<string, number>();
  private renderCtx: RenderContext | null = null;
  private renderSink: StreamSink | null = null;
  private folder: BranchFolder | null = null;
  private branchRecords: BranchRecord[] = [];
  private renderingBatch = false;
  private renderErrors = 0;
  private firstError = "";
  private onScrollFill = (): void => {
    void this.maybeFillAbove();
  };
  private titleEl!: HTMLElement;
  private subtitleEl!: HTMLElement;
  private statusEl!: HTMLElement;
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
   * issue #12: 流式加载。
   *
   * 后端按 100 行一 chunk 通过 Channel 边读边发，前端边收边 renderMessage + append
   * 到 stream，用户 ~500ms 内看到首屏，不再等整文件读完。
   *
   * BranchFolder 重建延后到全部加载完才做一次 —— 避免每 chunk 都 O(N) 重建造成
   * O(N²) 总开销。代价：流式期间 fold 还没应用（用户先看到全部消息，最后才折叠）。
   *
   * 取消：dispose() 时 stream = null，后续 chunk append 走 optional chaining no-op；
   * Channel 在 viewer GC 时随 JS 引用回收，backend 下次 send 返 Err 自然 break。
   */
  async load(opts: ViewerOptions): Promise<void> {
    this.titleEl.textContent = opts.displayTitle;
    this.subtitleEl.textContent = opts.subtitle ?? "";

    this.disposeStream();
    this.streamEl.replaceChildren();
    this.stream = new MessageStream(this.streamEl);

    this.statusEl.textContent = "加载中…";

    // Batch13-F39:lazy hljs(此前 viewer eager 全量高亮,是 65s 的组成部分)
    const ctx: RenderContext = {
      parentPath: opts.jsonlPath,
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      pendingToolResults: new Map(),
      // Batch9-F29（审计三家共识）：远端会话展开 subagent 需 origin 降级
      origin: opts.origin ?? null,
      lazy: true,
    };
    const timeline = new RecordTimeline(this.stream);
    // Batch13-F39:增量渲染期间 renderStreamRecord 会重复 feed branch 记录——
    // branch/queue 数据在收集阶段一次性预提取,sink 的对应回调置 no-op
    const sink: StreamSink = {
      timeline,
      onBranchRecord: () => {},
      onQueueOperation: () => {},
      observeForLazyEnhance: true,
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
    const channel = new Channel<JsonlLinePayload[]>();
    channel.onmessage = (chunk) => {
      if (!this.stream) return; // viewer 已 dispose
      for (const p of chunk) {
        const m = p.message as {
          type?: string;
          operation?: string;
          content?: string;
          uuid?: string;
        };
        if (m.type === "queue-operation") {
          // 与 renderStreamRecord 的路由条件保持一致(issue #36)
          if (m.operation === "enqueue" && m.content) queuedContents.push(m.content);
        } else {
          const br = extractBranchRecord(p.message);
          if (br) this.branchRecords.push(br);
        }
        if (m.uuid) this.uuidToIdx.set(m.uuid, this.payloads.length);
        this.payloads.push(p);
      }
      totalRecords += chunk.length;
      this.statusEl.textContent = `接收中 · 已 ${totalRecords} 条…`;
    };

    try {
      // issue #16：远端会话走 stream_read_remote_session（SSH 拉取，payload 带
      // origin），本地走原 IPC。chunk 结构一致，下游渲染零差异。
      const ipc = opts.origin
        ? "stream_read_remote_session"
        : "stream_read_session_jsonl";
      const finalCount = await invoke<number>(ipc, {
        jsonlPath: opts.jsonlPath,
        // 多机 #30：远端会话带 origin（= 该台 label）按 label 选台；本地 undefined → 省略。
        origin: opts.origin,
        onChunk: channel,
      });
      // **竞态修复**：Channel 和 invoke 是两条独立 IPC 通道，invoke resolve 时
      // 余下 chunk 的 onmessage 可能还排队没跑。等 totalRecords 追上 finalCount
      // 再切到最终状态文，否则会被晚到的 onmessage 又改回"加载中"。
      while (totalRecords < finalCount) {
        await new Promise((r) => setTimeout(r, 0));
        if (!this.stream) return; // viewer 已 dispose
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
      const loadMs = Math.round(performance.now() - t0);
      this.updateStatus(total, loadMs);
      // issue #6：从搜索结果跳进来 → 定位到命中消息；否则默认贴底。
      if (opts.scrollToUuid) {
        this.scrollToMessage(opts.scrollToUuid);
      } else {
        this.stream?.scrollToBottom();
      }
      // 上翻补批:挂在 .stream 滚动容器上(dispose 时随 streamEl 替换自然解绑)
      this.streamEl.addEventListener("scroll", this.onScrollFill, { passive: true });
    } catch (e) {
      this.statusEl.textContent = `加载失败：${String(e)}`;
    }
  }

  /** F39:渲染 payload 下标区间 [lo,hi)(逐条 renderStreamRecord,二分插入保序) */
  private renderRange(lo: number, hi: number): void {
    if (!this.renderCtx || !this.renderSink || !this.unrendered) return;
    // 不变量:二分插入只发生在**摊平**的 DOM 上——邻居若已被 fold wrap 收编,
    // insertBefore 会 NotFoundError(E2E 实测 58 条失败)。先摊平,批后重折。
    this.folder?.unwrapAll();
    const from = Math.max(0, lo);
    const to = Math.min(this.payloads.length, hi);
    for (let i = from; i < to; i++) {
      if (!this.unrendered.contains(i)) continue; // 已渲染(岛重叠)跳过
      const p = this.payloads[i];
      try {
        renderStreamRecord(p, this.renderCtx, this.renderSink);
      } catch (err) {
        this.renderErrors += 1;
        if (!this.firstError) {
          const t = (p as { message?: { type?: string } })?.message?.type ?? "?";
          this.firstError = `seq=${p?.seq} type=${t}: ${String(err)}`;
          console.error("[session-viewer] renderStreamRecord 抛错", p, err);
        }
      }
    }
    this.unrendered.markRendered(from, to);
  }

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

  private updateStatus(total: number, firstScreenMs?: number): void {
    const left = this.unrendered?.remaining ?? 0;
    const shown = total - left;
    const err =
      this.renderErrors > 0 ? `（${this.renderErrors} 条渲染失败，首个 ${this.firstError}）` : "";
    const ms = firstScreenMs !== undefined ? ` · 首屏 ${firstScreenMs}ms` : "";
    this.statusEl.textContent =
      left > 0
        ? `已显示 ${shown}/${total} 条${ms} · 上翻加载更早${err}`
        : `${total} 条记录${ms} · 只读历史视图${err}`;
  }

  /** F39:滚近顶部 → 往上补一批(原生 overflow-anchor 稳视口,F38 已实证该路径) */
  private async maybeFillAbove(): Promise<void> {
    if (this.renderingBatch || !this.unrendered || this.unrendered.isEmpty) return;
    if (this.streamEl.scrollTop > TOP_TRIGGER_PX) return;
    // 最低已渲染下标上方的洞;下标 0 已渲染时为 null(顶部无洞,内部缝由深链岛
    // 文档化为 v1 限制,不在 scroll 顶触发器职责内)
    const gap = this.unrendered.gapAbove(this.unrendered.lowestRenderedIdx());
    if (!gap) return;
    this.renderingBatch = true;
    this.statusEl.textContent = "加载更早消息…";
    try {
      // 让状态文先绘一帧再做同步渲染批
      await new Promise((r) => requestAnimationFrame(() => r(null)));
      if (!this.stream) return;
      const [a, b] = gap;
      this.renderRange(Math.max(a, b - BATCH_SIZE), b);
      this.rebuildFold();
      this.updateStatus(this.payloads.length);
    } finally {
      this.renderingBatch = false;
    }
  }

  /**
   * issue #6：滚动定位到指定 uuid 的卡片并临时高亮。
   * 命中卡片可能被折叠在 ESC 回退段（`<details>`）里 → 先展开所有祖先 details 再滚。
   * 找不到（极少：该 uuid 未渲染成带 data-uuid 的卡）则退化为贴底。
   */
  private scrollToMessage(uuid: string): void {
    // F39:目标还没渲染(非首屏路径调进来,如未来的重复定位)→ 先渲染目标岛
    const idx = this.uuidToIdx.get(uuid);
    if (idx !== undefined && this.unrendered?.contains(idx)) {
      this.renderRange(Math.max(0, idx - 100), Math.min(this.payloads.length, idx + 100));
      this.rebuildFold();
      this.updateStatus(this.payloads.length);
    }
    // CSS.escape 防 uuid 里有特殊字符破坏选择器
    const sel = `[data-uuid="${CSS.escape(uuid)}"]`;
    const el = this.streamEl.querySelector<HTMLElement>(sel);
    if (!el) {
      this.stream?.scrollToBottom();
      return;
    }
    // 展开所有折叠祖先，确保目标可见。注:ESC 回退段是 div.branch-fold-wrap
    // + .expanded 类(非 <details>)——此前只开 details,命中折叠段内的卡会被
    // 0fr 裁剪、flash 不可见(Batch13 D 审计发现的既有 bug)
    let p: HTMLElement | null = el.parentElement;
    while (p && p !== this.streamEl) {
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
    requestAnimationFrame(() =>
      requestAnimationFrame(() => el.scrollIntoView({ block: "center" })),
    );
    el.classList.add("search-hit-flash");
    // 动画结束后移除 class（再次跳同一条还能重放）
    window.setTimeout(() => el.classList.remove("search-hit-flash"), 2200);
  }

  /** 主动释放（HistoryView 卸载本组件时调） */
  dispose(): void {
    this.disposeStream();
  }

  private disposeStream(): void {
    this.streamEl?.removeEventListener("scroll", this.onScrollFill);
    if (this.stream) {
      this.stream.dispose();
      this.stream = null;
    }
    // F39:释放增量渲染状态(payloads 可达 37MB 量级)
    this.payloads = [];
    this.unrendered = null;
    this.uuidToIdx.clear();
    this.renderCtx = null;
    this.renderSink = null;
    this.folder = null;
    this.branchRecords = [];
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
    backBtn.textContent = "← 返回历史";
    backBtn.addEventListener("click", () => this.onBack());
    bar.appendChild(backBtn);

    const titles = document.createElement("div");
    titles.className = "session-viewer-titles";
    this.titleEl = document.createElement("div");
    this.titleEl.className = "session-viewer-title";
    titles.appendChild(this.titleEl);
    this.subtitleEl = document.createElement("div");
    this.subtitleEl.className = "session-viewer-subtitle";
    titles.appendChild(this.subtitleEl);
    bar.appendChild(titles);

    view.appendChild(bar);

    this.statusEl = document.createElement("div");
    this.statusEl.className = "history-status";
    view.appendChild(this.statusEl);

    // 消息流容器（与实时 Tab 用相同的 .stream 样式）
    this.streamEl = document.createElement("div");
    this.streamEl.className = "stream session-viewer-stream";
    view.appendChild(this.streamEl);

    return view;
  }
}
