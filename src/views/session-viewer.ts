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

import { invoke } from "@tauri-apps/api/core";
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

  async load(opts: ViewerOptions): Promise<void> {
    this.titleEl.textContent = opts.displayTitle;
    this.subtitleEl.textContent = opts.subtitle ?? "";

    this.disposeStream();
    this.streamEl.replaceChildren();
    this.stream = new MessageStream(this.streamEl);

    this.statusEl.textContent = "加载中…";
    try {
      const payloads = await invoke<JsonlLinePayload[]>("read_session_jsonl", {
        jsonlPath: opts.jsonlPath,
      });
      this.renderAll(payloads);
      this.statusEl.textContent = `${payloads.length} 条记录 · 只读历史视图`;
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

  /**
   * 渲染全部 payload。逻辑与 TabManager.onLine 一致 ——
   * 普通卡片就 append、连续 tool-group 合并到外层折叠卡。
   */
  private renderAll(payloads: JsonlLinePayload[]): void {
    if (!this.stream) return;
    const ctx: RenderContext = {
      parentPath: payloads[0]?.path ?? "",
      toolUseNames: new Map(),
      toolUseElements: new Map(),
    };
    let pendingToolGroup: ToolGroup | null = null;
    // issue #8: 收集所有带 uuid+parentUuid 的链节点（user / assistant / attachment / system）。
    // attachment + system 不渲染卡，但占 8% 链节点，少了它们 parent 链断 → 主线全错。
    const branchRecords: BranchRecord[] = [];

    for (const p of payloads) {
      const result = renderMessage(p.message, ctx);

      // 收集 branch record（无视 render 结果）—— 链完整性优先
      const branchRec = extractBranchRecord(p.message);
      if (branchRec) branchRecords.push(branchRec);

      switch (result.kind) {
        case "skip":
          continue;
        case "card":
          pendingToolGroup = null;
          // 仅 card 类型的 user/assistant 拿 data-uuid（BranchFolder DOM 扫描看这个）
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
            // issue #8: tool-group root 也写 data-uuid（首个贡献者）。详见 tabs.ts 同处理。
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

    // issue #8: 一次性算主线 + 重建 fold UI
    const folder = new BranchFolder(this.stream.contentElement);
    folder.setRecordsAndRebuild(branchRecords);

    // 渲染完贴底（只读视图打开时让用户先看到最新对话尾部）
    this.stream.scrollToBottom();
  }

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
