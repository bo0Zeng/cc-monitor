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
import {
  renderMessage,
  buildToolGroup,
  addToToolGroup,
  type JsonlRecord,
  type RenderContext,
  type ToolGroup,
} from "../cards";
import { BranchFolder } from "../branch-fold";
import { extractBranchRecord, type BranchRecord } from "../branching";

interface JsonlLinePayload {
  session_id: string;
  cwd: string | null;
  path: string;
  message: JsonlRecord;
}

export interface ViewerOptions {
  jsonlPath: string;
  /** 顶栏标题：custom_title / ai_title / first_user_excerpt 之一 */
  displayTitle: string;
  /** 子标题：项目名 + cwd */
  subtitle?: string;
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
    };
    let pendingToolGroup: ToolGroup | null = null;
    const branchRecords: BranchRecord[] = [];
    let totalRecords = 0;

    const channel = new Channel<JsonlLinePayload[]>();
    channel.onmessage = (chunk) => {
      if (!this.stream) return; // viewer 已 dispose
      for (const p of chunk) {
        const result = renderMessage(p.message, ctx);

        // 收集 branch record（无视 render 结果）—— 链完整性优先
        const branchRec = extractBranchRecord(p.message);
        if (branchRec) branchRecords.push(branchRec);

        switch (result.kind) {
          case "skip":
            continue;
          case "card":
            pendingToolGroup = null;
            if (p.message.type === "user" || p.message.type === "assistant") {
              result.element.setAttribute("data-uuid", p.message.uuid);
              if (p.message.parentUuid) {
                result.element.setAttribute("data-parent-uuid", p.message.parentUuid);
              }
            }
            this.stream.append(result.element);
            break;
          case "tool-group":
            if (pendingToolGroup) {
              addToToolGroup(pendingToolGroup, result.units);
            } else {
              const group = buildToolGroup(result.timestamp);
              addToToolGroup(group, result.units);
              pendingToolGroup = group;
              if (p.message.type === "user" || p.message.type === "assistant") {
                group.root.setAttribute("data-uuid", p.message.uuid);
                if (p.message.parentUuid) {
                  group.root.setAttribute("data-parent-uuid", p.message.parentUuid);
                }
              }
              this.stream.append(group.root);
            }
            break;
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
      // 全部到齐后一次 BranchFolder 重建（避免 O(N²)）
      if (this.stream && branchRecords.length > 0) {
        const folder = new BranchFolder(this.stream.contentElement);
        folder.setRecordsAndRebuild(branchRecords);
      }
      this.statusEl.textContent = `${finalCount} 条记录 · 只读历史视图`;
      this.stream?.scrollToBottom();
    } catch (e) {
      this.statusEl.textContent = `加载失败：${String(e)}`;
    }
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
