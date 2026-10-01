/**
 * **启动时记住的那一格**（上次所在的 tab）：
 *
 * - 只在**那个会话出现**时恢复（它的 `live` 格到了 ⇒ 切过去）；
 * - **就绪前不许被覆盖**：等它的这一段里，别的 tab 被自动切到不写回「上次所在」（记忆只改在用户手动切的那一下）；
 * - **没等到就明说一句、不静默换**：订了的每台机器都报完了活会话清单、里面都没有它 ⇒ 说一句「上次所在的会话这次没出现」，放下等待。
 *
 * 原先这里是一个 30 s 启动窗口（过了窗口迟到的宣告静默不切，而窗口之内别的 tab 早把记忆写掉了）—— 换成「按事件判」：
 * 零定时器，结论只看会话流里来的格（`live` · `listed`）。
 * 用户手动切过 tab ⇒ 用户的选择优先：放下等待、把此刻所在那一格记下。
 */

export interface StartupIo {
  /** 切到这个 tab（自动，不算用户动作）。 */
  switchTo(sid: string): void;
  /** 「上次所在」要不要跟着切换写回（等待期间 `false`）。 */
  holdMemory(hold: boolean): void;
  /** 把此刻所在那一格写回「上次所在」。 */
  rememberCurrent(): void;
  /** 明说一句：它没出现。 */
  sayGone(sid: string): void;
}

export class StartupActive {
  private pending: string | null;
  private readonly listed = new Set<string>();

  /** `sid` = 记住的那一格（没有 ⇒ `null`）；`present` = 它此刻已经在（起步那一刻就在 ⇒ 当场切，不等）；`machines` = 订了会话流的那几台。 */
  constructor(
    sid: string | null,
    present: boolean,
    private readonly machines: readonly string[],
    private readonly io: StartupIo,
  ) {
    this.pending = sid && !present ? sid : null;
    if (sid && present) io.switchTo(sid);
    if (this.pending) io.holdMemory(true);
  }

  /** 还在等哪一格（没有 ⇒ `null`）。 */
  get waitingFor(): string | null {
    return this.pending;
  }

  private release(): void {
    this.pending = null;
    this.io.holdMemory(false);
  }

  /** 一个会话出现了（`live` 格：本机 `started` · 远端 `remote-added`）。是它 ⇒ 切过去、放下等待。 */
  onAppeared(sid: string): void {
    if (this.pending !== sid) return;
    this.release();
    this.io.switchTo(sid);
  }

  /** 某台报完了活会话清单。每台都报完了、它还没出现 ⇒ 明说、放下等待。 */
  onListed(origin: string): void {
    this.listed.add(origin);
    if (this.pending === null) return;
    if (!this.machines.every((m) => this.listed.has(m))) return;
    const gone = this.pending;
    this.release();
    this.io.sayGone(gone);
  }

  /** 用户手动切了 tab ⇒ 用户的选择优先。 */
  onManualSwitch(): void {
    if (this.pending === null) return;
    this.release();
    this.io.rememberCurrent();
  }
}
