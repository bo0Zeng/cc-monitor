/**
 * 快捷键编辑器 modal overlay。
 *
 * 视觉等同独立小窗口（居中浮窗 + 半透明背景遮罩 + 标题栏 + 关闭按钮 + 阴影），
 * 但实际是前端 fixed-position lightbox —— 复用主题样式、零跨窗口 IPC、瞬开瞬关。
 *
 * ## 行为
 *
 * - 表格按 Category 分组列出所有 Action：动作名 / 当前 chord / 操作按钮
 * - [改] → 行内 inline 录制（"按下你想要的组合键…"），监听一次 keydown 拿 chord
 * - 拿到 chord 后：
 *    - 解绑（Esc / Backspace）→ setOverride(id, null)
 *    - 跟其他 action 冲突 → 弹覆盖确认；点确认 → 旧 action 自动变 null
 *    - 改成 default 一致 → setOverride(id, "")（删覆盖恢复默认）
 *    - 正常自定义 → setOverride(id, chord)
 * - [↺] 单条恢复 default
 * - [全部重置默认] 一键清空所有覆盖
 *
 * ## Esc 例外
 *
 * `overlay.close` (默认 Escape) 改起来要带强警告 —— 改了之后用户只能点 X 关弹层。
 *
 * ## 持久化
 *
 * 每次改动立即 `setKeybindings(dispatcher.exportOverrides())` —— 没有 [保存/取消]
 * 按钮，模拟 macOS 风格"改即生效"。dispatcher 立刻 rebind，下次按键就走新映射。
 */

import {
  ACTIONS,
  CATEGORY_LABEL,
  findAction,
  groupByCategory,
  type Action,
  type ActionId,
  type Category,
} from "./actions";
import { dispatcher, KeybindingDispatcher, type OverlayHandle } from "./registry";
import { setKeybindings } from "./store";
// F82a：键位改动落盘后广播，主窗口跨窗热应用（事件名在中立模块，避免与 settings/panel 循环）。
import { emit } from "@tauri-apps/api/event";
import { SETTINGS_APPLIED_EVENT } from "../settings/events";

export class KeybindingsEditor implements OverlayHandle {
  private overlay: HTMLElement;
  private isOpen = false;
  /** 当前录制中的行的 chord 显示单元；非 null 时按键走录制 handler */
  private recordingCell: HTMLElement | null = null;
  /** 各 action 行的 chord cell + 操作按钮的引用，用于 setOverride 后局部刷新 */
  private rowRefs = new Map<
    ActionId,
    { chordCell: HTMLElement; recordBtn: HTMLButtonElement; resetBtn: HTMLButtonElement }
  >();

  constructor() {
    this.overlay = this.build();
    document.body.appendChild(this.overlay);
  }

  /** OverlayHandle 接口：Esc 关闭。但不能"覆盖自己（overlay.close）改成 Esc 时绕过" */
  handleEsc(): void {
    if (this.recordingCell) {
      // 录制模式 Esc 在 dispatcher 那边已经截胡走录制 handler，理论到不了这里
      return;
    }
    this.close();
  }

  open(): void {
    if (this.isOpen) return;
    this.isOpen = true;
    this.overlay.classList.add("open");
    dispatcher.pushOverlay(this);
  }

  close(): void {
    if (!this.isOpen) return;
    // 录制中关 modal 要先取消录制
    if (this.recordingCell) {
      dispatcher.cancelRecording();
      this.cancelInlineRecording();
    }
    this.isOpen = false;
    this.overlay.classList.remove("open");
    dispatcher.popOverlay(this);
  }

  // === DOM ===

  private build(): HTMLElement {
    const overlay = document.createElement("div");
    overlay.className = "kb-editor-overlay";
    overlay.addEventListener("click", (e) => {
      // 点空白遮罩关闭；点 window 内部不关
      if (e.target === overlay) this.close();
    });

    const win = document.createElement("div");
    win.className = "kb-editor-window";
    overlay.appendChild(win);

    // 标题栏
    const titleBar = document.createElement("div");
    titleBar.className = "kb-editor-titlebar";
    const title = document.createElement("span");
    title.className = "kb-editor-title";
    title.textContent = "快捷键";
    titleBar.appendChild(title);
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "kb-editor-close";
    closeBtn.title = "关闭";
    closeBtn.textContent = "×";
    closeBtn.addEventListener("click", () => this.close());
    titleBar.appendChild(closeBtn);
    win.appendChild(titleBar);

    // 提示
    const hint = document.createElement("div");
    hint.className = "kb-editor-hint";
    hint.textContent =
      "点 [改] 后按下你想要的组合键。Esc / Backspace 在录制中表示「解绑」。改动即时生效，无需重启。";
    win.appendChild(hint);

    // 表格容器
    const body = document.createElement("div");
    body.className = "kb-editor-body";
    win.appendChild(body);

    const groups = groupByCategory();
    const ordered: Category[] = ["Tab", "Term", "App", "Beh", "Panel"];
    for (const cat of ordered) {
      const list = groups.get(cat);
      if (!list || list.length === 0) continue;
      body.appendChild(this.buildCategorySection(cat, list));
    }

    // 底部
    const footer = document.createElement("div");
    footer.className = "kb-editor-footer";
    const resetAll = document.createElement("button");
    resetAll.type = "button";
    resetAll.className = "kb-editor-reset-all";
    resetAll.textContent = "全部重置默认";
    resetAll.addEventListener("click", () => void this.onResetAll());
    footer.appendChild(resetAll);
    win.appendChild(footer);

    return overlay;
  }

  private buildCategorySection(cat: Category, actions: Action[]): HTMLElement {
    const section = document.createElement("section");
    section.className = "kb-editor-section";

    const h = document.createElement("h3");
    h.className = "kb-editor-section-title";
    h.textContent = CATEGORY_LABEL[cat];
    section.appendChild(h);

    const table = document.createElement("table");
    table.className = "kb-editor-table";
    const tbody = document.createElement("tbody");
    table.appendChild(tbody);

    for (const a of actions) {
      tbody.appendChild(this.buildActionRow(a));
    }

    section.appendChild(table);
    return section;
  }

  private buildActionRow(action: Action): HTMLElement {
    const tr = document.createElement("tr");
    tr.className = "kb-editor-row";
    if (!action.available) tr.classList.add("kb-editor-row-disabled");

    // 动作名
    const nameCell = document.createElement("td");
    nameCell.className = "kb-editor-name";
    nameCell.textContent = action.label;
    if (!action.available && action.comingSoon) {
      const tag = document.createElement("span");
      tag.className = "kb-editor-tag-coming";
      tag.textContent = `未上线（${action.comingSoon}）`;
      nameCell.appendChild(tag);
    }
    tr.appendChild(nameCell);

    // chord 显示单元
    const chordCell = document.createElement("td");
    chordCell.className = "kb-editor-chord";
    chordCell.textContent = KeybindingDispatcher.prettyChord(
      dispatcher.effectiveChord(action.id as ActionId),
    );
    tr.appendChild(chordCell);

    // 操作按钮
    const opCell = document.createElement("td");
    opCell.className = "kb-editor-ops";
    const recordBtn = document.createElement("button");
    recordBtn.type = "button";
    recordBtn.className = "kb-editor-btn-record";
    recordBtn.textContent = "改";
    recordBtn.disabled = !action.available;
    recordBtn.addEventListener("click", () => this.startRecording(action, chordCell));
    opCell.appendChild(recordBtn);

    const resetBtn = document.createElement("button");
    resetBtn.type = "button";
    resetBtn.className = "kb-editor-btn-reset";
    resetBtn.textContent = "↺";
    resetBtn.title = "恢复默认";
    resetBtn.disabled = !action.available;
    resetBtn.addEventListener("click", () => void this.resetOne(action));
    opCell.appendChild(resetBtn);

    tr.appendChild(opCell);

    this.rowRefs.set(action.id as ActionId, { chordCell, recordBtn, resetBtn });
    return tr;
  }

  // === 录制流程 ===

  private startRecording(action: Action, chordCell: HTMLElement): void {
    // 如果有其他行正在录制，先取消
    if (this.recordingCell) {
      dispatcher.cancelRecording();
      this.cancelInlineRecording();
    }
    this.recordingCell = chordCell;
    chordCell.classList.add("kb-editor-recording");
    chordCell.textContent = "请按下组合键…";

    dispatcher.startRecording((chord) => {
      // chord = null → Esc/Backspace，意味着「清空 / 解绑」
      this.cancelInlineRecording();
      if (chord === null) {
        void this.applyChord(action, null);
      } else {
        void this.applyChord(action, chord);
      }
    });
  }

  private cancelInlineRecording(): void {
    if (this.recordingCell) {
      this.recordingCell.classList.remove("kb-editor-recording");
      this.recordingCell = null;
    }
  }

  /**
   * 把 chord 应用到 action：
   *  - chord === null → 解绑（setOverride 为 null）
   *  - chord 命中 default → 删覆盖（setOverride 为空串）
   *  - chord 冲突 → 弹覆盖确认
   *  - 否则 → setOverride 为 chord
   */
  private async applyChord(action: Action, chord: string | null): Promise<void> {
    const id = action.id as ActionId;

    if (chord === null) {
      // Esc 改 overlay.close 自己时：强警告
      if (id === "overlay.close") {
        const ok = window.confirm(
          "你正在解绑「关闭弹层」的快捷键。解绑后所有弹层只能点 × 关闭，无法用键盘退出。\n\n" +
            "确定要继续吗？",
        );
        if (!ok) {
          this.refreshRow(id);
          return;
        }
      }
      dispatcher.setOverride(id, null);
      await this.persist();
      this.refreshRow(id);
      return;
    }

    // overlay.close 改成非 Escape 时强警告
    if (id === "overlay.close" && chord !== "Escape") {
      const pretty = KeybindingDispatcher.prettyChord(chord);
      const ok = window.confirm(
        `你正在把「关闭弹层」改成 ${pretty}。\n\n` +
          "改后按 Esc 不再自动关弹层，必须按新键或点 × 才能关。\n\n确定吗？",
      );
      if (!ok) {
        this.refreshRow(id);
        return;
      }
    }

    // 冲突检测：这个 chord 现在被谁占着？
    const owner = dispatcher.whoOwns(chord);
    if (owner && owner !== id) {
      const ownerAction = findAction(owner);
      const ownerLabel = ownerAction?.label ?? owner;
      const pretty = KeybindingDispatcher.prettyChord(chord);
      const ok = window.confirm(
        `${pretty} 当前是「${ownerLabel}」。要覆盖吗？\n\n` +
          `「${ownerLabel}」会被解绑（变成「未绑定」）。`,
      );
      if (!ok) {
        this.refreshRow(id);
        return;
      }
      // 解绑旧的
      dispatcher.setOverride(owner, null);
      this.refreshRow(owner);
    }

    // 改成跟 default 一致 → 删覆盖让 default 自然生效
    const defChord = action.default;
    if (chord === defChord) {
      dispatcher.setOverride(id, "");
    } else {
      dispatcher.setOverride(id, chord);
    }
    await this.persist();
    this.refreshRow(id);
  }

  private async resetOne(action: Action): Promise<void> {
    dispatcher.setOverride(action.id as ActionId, "");
    await this.persist();
    this.refreshRow(action.id as ActionId);
  }

  private async onResetAll(): Promise<void> {
    if (!window.confirm("确定恢复全部快捷键到默认？所有自定义会丢失。")) return;
    for (const a of ACTIONS) {
      dispatcher.setOverride(a.id as ActionId, "");
    }
    await this.persist();
    // 全表刷新
    for (const a of ACTIONS) this.refreshRow(a.id as ActionId);
  }

  private refreshRow(id: ActionId): void {
    const refs = this.rowRefs.get(id);
    if (!refs) return;
    refs.chordCell.textContent = KeybindingDispatcher.prettyChord(
      dispatcher.effectiveChord(id),
    );
  }

  private async persist(): Promise<void> {
    try {
      await setKeybindings(dispatcher.exportOverrides());
      // F82a：编辑器现挂在独立设置窗口里，键位改动落盘后广播，主窗口 listen 后
      // applyOverrides 热生效（否则跨窗后「改即生效」退化成要重启）。同窗（本编辑器所在
      // dispatcher）已即时生效，此广播是给**别的**窗口（主窗口）用的。
      void emit(SETTINGS_APPLIED_EVENT);
    } catch (e) {
      console.warn("[keybindings] persist failed:", e);
    }
  }
}
