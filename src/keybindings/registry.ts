/**
 * KeybindingDispatcher —— 全局快捷键派发 + 弹层栈管理。
 *
 * ## 责任
 *
 * 1. 维护 chord ↔ ActionId 映射（默认 + 用户覆盖）
 * 2. 维护 ActionId ↔ callback 映射（main.ts 启动时 `bind()` 注册）
 * 3. 挂一个 window keydown listener，dispatch 命中的 callback
 * 4. 维护 overlay stack —— `overlay.close` action（默认 Esc）触发时关栈顶
 *
 * ## 单例
 *
 * 全 app 只有一个 dispatcher 实例。在 main.ts 创建并通过模块默认导出共享。
 *
 * ## chord 规范化
 *
 * `Ctrl+Shift+Alt+Meta+<code>`：modifier 固定顺序、用 `KeyboardEvent.code`
 * 不用 `key`。`code` 是物理键位（KeyW / Digit1 / Backquote / Comma），不受
 * 键盘布局影响。`key` 是当前布局解释（在法语键盘 `e.key === ","` 永远触发不了
 * 因为 `,` 是 Shift+逗号）。
 *
 * 显示给用户时用 `prettyChord()` 转友好名：`Ctrl+Shift+KeyW` → `Ctrl + Shift + W`。
 */

import { ACTIONS, findAction, type ActionId } from "./actions";

export interface OverlayHandle {
  /** 栈顶时按 Esc 调用。返回 true 表示"已处理"，false 表示让 dispatcher 继续 pop */
  handleEsc(): boolean | void;
}

type Callback = () => void;

export class KeybindingDispatcher {
  /** ActionId → callback。bind() 注册时填，未 bind 的 action 视为不存在 */
  private callbacks = new Map<string, Callback>();
  /**
   * chord 规范化串 → ActionId。两个来源 merge：
   *  - ACTIONS 表里的 default
   *  - 用户在编辑器里改的覆盖
   *
   * 覆盖优先：同 chord 用户改了就用用户的。
   */
  private chordToAction = new Map<string, ActionId>();
  /**
   * ActionId → 用户覆盖的 chord。`null` = 用户显式解绑（覆盖 default 为不绑）。
   * 不在 map 里 = 用 default。
   */
  private overrides = new Map<ActionId, string | null>();
  /**
   * 弹层栈。push 顺序 = 打开顺序；Esc 命中时 pop 栈顶调 handleEsc。
   * 多个弹层共存（如设置上面又开了快捷键编辑器）按 LIFO 关。
   */
  private overlayStack: OverlayHandle[] = [];
  private started = false;
  /** 当前是否处于"录制中"——录制时绕过常规 dispatch 直接给 editor 拿 chord */
  private recordingHandler: ((chord: string | null) => void) | null = null;

  /** 把 KeyboardEvent 转成规范化 chord 串 */
  static normalizeChord(e: KeyboardEvent): string | null {
    // 单按 modifier 不算 chord（避免 Shift 单按生成 "Shift" 误命中）
    if (e.key === "Control" || e.key === "Shift" || e.key === "Alt" || e.key === "Meta") {
      return null;
    }
    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.shiftKey) parts.push("Shift");
    if (e.altKey) parts.push("Alt");
    if (e.metaKey) parts.push("Meta");
    // Escape / Tab / Backquote / KeyW / Digit1 / F1...F12 都直接走 e.code
    // e.code 在 numpad 上是 NumpadX，跟主键区 DigitX 区分（用户改了也是物理键）
    parts.push(e.code);
    return parts.join("+");
  }

  /** 把规范化 chord 串转给用户看的友好名 */
  static prettyChord(chord: string | null): string {
    if (!chord) return "未绑定";
    const parts = chord.split("+");
    const out: string[] = [];
    for (const p of parts) {
      if (p === "Ctrl" || p === "Shift" || p === "Alt" || p === "Meta") {
        out.push(p);
        continue;
      }
      // KeyW → W；Digit1 → 1；Backquote → `；Comma → ,
      if (p.startsWith("Key") && p.length === 4) out.push(p.slice(3));
      else if (p.startsWith("Digit") && p.length === 6) out.push(p.slice(5));
      else if (p === "Backquote") out.push("`");
      else if (p === "Comma") out.push(",");
      else if (p === "Period") out.push(".");
      else if (p === "Slash") out.push("/");
      else if (p === "Semicolon") out.push(";");
      else if (p === "Quote") out.push("'");
      else if (p === "Minus") out.push("-");
      else if (p === "Equal") out.push("=");
      else if (p === "BracketLeft") out.push("[");
      else if (p === "BracketRight") out.push("]");
      else if (p === "Backslash") out.push("\\");
      else out.push(p); // Escape / Tab / F1-F12 / Arrow* 保持原样
    }
    return out.join(" + ");
  }

  /** 主程序调：把一个 action 绑回调。重复 bind 会覆盖（无意义但合法） */
  bind(id: ActionId, callback: Callback): void {
    this.callbacks.set(id, callback);
  }

  /**
   * 应用用户覆盖（启动时调一次，编辑器保存后调）。
   *
   * 输入：完整的 keybindings 字段（含 null 表示显式解绑，缺失字段走 default）。
   * 完了之后 chordToAction 表反映"当前生效的绑定"。
   */
  applyOverrides(overrides: Record<string, string | null>): void {
    this.overrides.clear();
    for (const [k, v] of Object.entries(overrides)) {
      // 只接受 ACTIONS 里有的 id；陌生 key 静默丢
      if (findAction(k)) this.overrides.set(k as ActionId, v);
    }
    this.rebuildChordTable();
  }

  /**
   * 单条改：编辑器里点击 [改] 选了一个新 chord 后调。
   *
   * - `chord = null` 表示显式解绑
   * - `chord = ""` 表示恢复默认（删覆盖）
   */
  setOverride(id: ActionId, chord: string | null | ""): void {
    if (chord === "") this.overrides.delete(id);
    else this.overrides.set(id, chord);
    this.rebuildChordTable();
  }

  /** 获取某 action 当前实际生效的 chord */
  effectiveChord(id: ActionId): string | null {
    if (this.overrides.has(id)) return this.overrides.get(id)!;
    return findAction(id)?.default ?? null;
  }

  /** 导出供持久化的 overrides 字段（只含跟默认不同的） */
  exportOverrides(): Record<string, string | null> {
    const out: Record<string, string | null> = {};
    for (const [id, chord] of this.overrides.entries()) {
      out[id] = chord;
    }
    return out;
  }

  /** 查找某 chord 当前绑了谁；用于冲突检测 */
  whoOwns(chord: string): ActionId | null {
    return this.chordToAction.get(chord) ?? null;
  }

  /** 弹层 push：模块 open 时调。栈顶时按 Esc 会先关到它 */
  pushOverlay(h: OverlayHandle): void {
    // 防重复 push
    const i = this.overlayStack.indexOf(h);
    if (i >= 0) this.overlayStack.splice(i, 1);
    this.overlayStack.push(h);
  }

  /** 弹层 pop：模块 close 时调 */
  popOverlay(h: OverlayHandle): void {
    const i = this.overlayStack.indexOf(h);
    if (i >= 0) this.overlayStack.splice(i, 1);
  }

  /**
   * 录制模式：editor 里点 [改] 后调，下一次按键不走常规 dispatch，
   * 而是把 chord 喂给 handler 然后自动退出录制。
   *
   * - 按 modifier-only 不结束录制（等用户按完整组合）
   * - 按 Escape 结束录制并回调 null（取消）
   * - 按 Backspace 也回调 null（清空 / 解绑）
   * - 其他按键 → 回调 chord 串
   *
   * editor 显示完后调 `cancelRecording()` 兜底（万一用户没按任何键就关 modal）
   */
  startRecording(handler: (chord: string | null) => void): void {
    this.recordingHandler = handler;
  }

  cancelRecording(): void {
    this.recordingHandler = null;
  }

  /** 挂 window keydown 监听。main.ts 启动时调一次。 */
  start(): void {
    if (this.started) return;
    this.started = true;
    window.addEventListener("keydown", this.onKeyDown, true);
  }

  // === 私有 ===

  private rebuildChordTable(): void {
    this.chordToAction.clear();
    for (const a of ACTIONS) {
      const chord = this.overrides.has(a.id)
        ? this.overrides.get(a.id)!
        : a.default;
      if (chord == null) continue;
      // 冲突时后定义的赢；编辑器保存前会先做冲突检测，正常情况不会冲突到这里
      this.chordToAction.set(chord, a.id);
    }
  }

  private onKeyDown = (e: KeyboardEvent): void => {
    // 录制模式：所有按键截胡给 editor
    if (this.recordingHandler) {
      // 单按 modifier 不算结束录制
      if (e.key === "Control" || e.key === "Shift" || e.key === "Alt" || e.key === "Meta") {
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      const handler = this.recordingHandler;
      this.recordingHandler = null;
      // Escape / Backspace → 清空意图
      if (e.code === "Escape" || e.code === "Backspace") {
        handler(null);
        return;
      }
      const chord = KeybindingDispatcher.normalizeChord(e);
      handler(chord);
      return;
    }

    const chord = KeybindingDispatcher.normalizeChord(e);
    if (!chord) return;

    const id = this.chordToAction.get(chord);
    if (!id) return;

    // 单键快捷键守卫：焦点在**可编辑文本**元素（历史搜索框 / 设置输入 / 重命名 / select 等）
    // 时，除 overlay.close（Esc，用来关搜索/弹层）外一律不触发——否则默认的单键快捷键
    // （h/m/t/数字…）会在打字时被误触发。详 isEditableTarget。
    if (id !== "overlay.close" && isEditableTarget()) return;

    // overlay.close 特殊：交给栈顶 overlay 处理
    if (id === "overlay.close") {
      if (this.overlayStack.length === 0) return;
      e.preventDefault();
      const top = this.overlayStack[this.overlayStack.length - 1];
      const handled = top.handleEsc();
      // overlay 自己 close 时会调 popOverlay；这里不主动 pop（避免双弹）
      // handled = false 时让事件继续传播（少见，目前没人用）
      if (handled === false) return;
      return;
    }

    // 未上线 action 即使 chord 命中也不触发
    const action = findAction(id);
    if (!action?.available) return;

    const cb = this.callbacks.get(id);
    if (!cb) return; // 注册了 chord 但 main.ts 没 bind callback（不该发生）

    e.preventDefault();
    cb();
  };
}

/**
 * 当前焦点是否在**可编辑文本**元素里（会"打字"的 input/textarea/select/contenteditable）。
 * 单键快捷键守卫用：这些元素聚焦时不触发快捷键，否则 h/m/t/数字… 会在搜索/设置/重命名里误触发。
 * **只挡可打字控件**：checkbox/radio/range/color（设置面板的勾选 / 取色器）不算，在它们上
 * 单键导航仍可用；readonly/disabled 也不算。
 */
function isEditableTarget(): boolean {
  const el = document.activeElement;
  if (!(el instanceof HTMLElement)) return false;
  // 不可见的输入不算"正在打字"：弹层关闭后仍滞留焦点的隐藏输入（display:none / 脱离
  // 渲染树）若被当成可编辑，会吞掉所有单键快捷键。getClientRects() 为空 = 没有渲染盒；
  // 可见的 fixed 定位输入仍有 rect，不会误判（历史搜索框聚焦时照常拦截单键）。
  if (el.getClientRects().length === 0) return false;
  if (el.isContentEditable) return true;
  const tag = el.tagName;
  if (tag === "TEXTAREA") {
    const ta = el as HTMLTextAreaElement;
    return !ta.readOnly && !ta.disabled;
  }
  if (tag === "SELECT") return !(el as HTMLSelectElement).disabled;
  if (tag === "INPUT") {
    const inp = el as HTMLInputElement;
    if (inp.readOnly || inp.disabled) return false;
    const t = (inp.type || "text").toLowerCase();
    return ["text", "search", "url", "email", "password", "number", "tel"].includes(t);
  }
  return false;
}

/** 全 app 单例 */
export const dispatcher = new KeybindingDispatcher();
