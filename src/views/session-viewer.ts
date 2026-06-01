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
}

export class SessionViewer {
  private root: HTMLElement;
  private streamEl!: HTMLElement;
  private stream: MessageStream | null = null;
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

    const ctx: RenderContext = {
      parentPath: opts.jsonlPath,
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      pendingToolResults: new Map(),
    };
    // P5.2 B 重构：SessionViewer 改用 renderStreamRecord（跟 TabManager 共享管线）。
    // BranchFolder 延后 — 流式期间不算，全部到齐后 setRecordsAndRebuild 一次（O(N²)→O(N)）。
    const timeline = new RecordTimeline(this.stream);
    const branchRecords: BranchRecord[] = [];
    const sink: StreamSink = {
      timeline,
      onBranchRecord: (rec) => branchRecords.push(rec),
      // SessionViewer 不更新 tab 标题、不触发 user-active、无 batch lazy
    };
    let totalRecords = 0;

    // 渲染韧性 + 探针：renderStreamRecord 在 Channel 回调里跑，一旦某条记录渲染
    // 抛错，异常**不会**被下面 load() 的 try/catch 接住（不同事件回合），会导致
    // totalRecords 卡住 → while 循环空转 → 整个查看器空白（已观察到的 bug）。
    // 这里逐条 try/catch：单条失败不影响其余，并记录首个错误供定位 / 显示。
    let renderErrors = 0;
    let firstError = "";
    const channel = new Channel<JsonlLinePayload[]>();
    channel.onmessage = (chunk) => {
      if (!this.stream) return; // viewer 已 dispose
      for (const p of chunk) {
        try {
          renderStreamRecord(p, ctx, sink);
        } catch (err) {
          renderErrors += 1;
          if (!firstError) {
            const t = (p as { message?: { type?: string } })?.message?.type ?? "?";
            firstError = `seq=${p?.seq} type=${t}: ${String(err)}`;
            console.error("[session-viewer] renderStreamRecord 抛错", p, err);
          }
        }
      }
      totalRecords += chunk.length;
      this.statusEl.textContent = `加载中 · 已 ${totalRecords} 条…`;
    };

    try {
      const finalCount = await invoke<number>("stream_read_session_jsonl", {
        jsonlPath: opts.jsonlPath,
        onChunk: channel,
      });
      // **竞态修复**：Channel 和 invoke 是两条独立 IPC 通道，invoke resolve 时
      // 余下 chunk 的 onmessage 可能还排队没跑。等 totalRecords 追上 finalCount
      // 再切到最终状态文，否则会被晚到的 onmessage 又改回"加载中"。
      while (totalRecords < finalCount) {
        await new Promise((r) => setTimeout(r, 0));
        if (!this.stream) return; // viewer 已 dispose
      }
      // 全部到齐后一次 BranchFolder 重建（避免 O(N²)）。单独 try/catch：分支折叠
      // 抛错不应让已渲染的消息全没（旧版整段 catch 会把成功渲染的也吞成"加载失败"）。
      if (this.stream && branchRecords.length > 0) {
        try {
          const folder = new BranchFolder(this.stream.contentElement);
          folder.setRecordsAndRebuild(branchRecords);
        } catch (err) {
          renderErrors += 1;
          if (!firstError) firstError = `branch-fold: ${String(err)}`;
          console.error("[session-viewer] BranchFolder.setRecordsAndRebuild 抛错", err);
        }
      }
      this.statusEl.textContent =
        renderErrors > 0
          ? `${finalCount} 条记录（${renderErrors} 条渲染失败，首个 ${firstError}）`
          : `${finalCount} 条记录 · 只读历史视图`;
      // issue #6：从搜索结果跳进来 → 定位到命中消息；否则默认贴底。
      if (opts.scrollToUuid) {
        this.scrollToMessage(opts.scrollToUuid);
      } else {
        this.stream?.scrollToBottom();
      }
    } catch (e) {
      this.statusEl.textContent = `加载失败：${String(e)}`;
    }
  }

  /**
   * issue #6：滚动定位到指定 uuid 的卡片并临时高亮。
   * 命中卡片可能被折叠在 ESC 回退段（`<details>`）里 → 先展开所有祖先 details 再滚。
   * 找不到（极少：该 uuid 未渲染成带 data-uuid 的卡）则退化为贴底。
   */
  private scrollToMessage(uuid: string): void {
    // CSS.escape 防 uuid 里有特殊字符破坏选择器
    const sel = `[data-uuid="${CSS.escape(uuid)}"]`;
    const el = this.streamEl.querySelector<HTMLElement>(sel);
    if (!el) {
      this.stream?.scrollToBottom();
      return;
    }
    // 展开所有折叠祖先，确保目标可见
    let p: HTMLElement | null = el.parentElement;
    while (p && p !== this.streamEl) {
      if (p instanceof HTMLDetailsElement) p.open = true;
      p = p.parentElement;
    }
    el.scrollIntoView({ block: "center" });
    el.classList.add("search-hit-flash");
    // 动画结束后移除 class（再次跳同一条还能重放）
    window.setTimeout(() => el.classList.remove("search-hit-flash"), 2200);
  }

  /** 主动释放（HistoryView 卸载本组件时调） */
  dispose(): void {
    this.disposeStream();
  }

  private disposeStream(): void {
    if (this.stream) {
      this.stream.dispose();
      this.stream = null;
    }
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
