/**
 * 路由：切到哪个 tab、谁有权切。
 *
 * - 手动（点 tab / Ctrl+Tab / Ctrl+1..9）：`cycleTarget` / `indexTarget` 算目标；切完 `noteSwitched`
 *   记 5s 手动保护、通知宿主（`onManualSwitch`）。
 * - 自动跟随（用户在终端真敲了一行 ⇒ `userActive`）：`autoFollow` 一个函数回答「放不放行」，
 *   五道跳过条件全在这里。
 * - 记住上次的 tab（`persistLastActive`；viewer / 撕离窗口置 false，防污染主窗口记忆）。
 *
 * 读 `TabStore`（tab 集合 · 顺序 · 当前 tab · 是否在重放批里），不写 tab 集合；零 DOM、零 IPC ——
 * 「切过去之后界面怎么变」是 `TabManager.switchTo` 的编排，「把 monitor 拉到前台」是会话动作那一份的 IPC。
 * 这里只回「该切谁 / 放不放行」，不去切。
 */
import { LS_KEYS, safeSet } from "./local-storage";
import { isResumeOnly } from "./tab-session-state";
import type { BehaviorConfig } from "./behavior";
import type { TabStore } from "./tab-store";

/** `autoFollow` 的三种结论：不动 · 已经是它（只按设置决定要不要把 monitor 拉前）· 切过去。 */
export type AutoFollowDecision = "ignore" | "front-only" | "switch";

export class TabRouter {
  /** 用户在终端真敲了一行 → 自动切到对应 Tab 的开关。默认 true，从 config.json（autoFollowUserActive）加载。 */
  private autoFollowUserActive: boolean = true;
  /** 自动切 tab 时是否同时把 monitor 窗口拉前台。默认 false。 */
  bringMonitorToFront: boolean = false;
  /** 用户手动切过 tab 后 5s 内不自动跟随（「我正主动看另一个 tab，别抢回去」）。0 = 不在保护期；每次手动切更新为 now ＋ 5000。 */
  private manualOverrideUntil: number = 0;
  /** 手动保护期长度（ms）。 */
  private static readonly MANUAL_OVERRIDE_MS = 5000;

  /** switchTo 是否写回 last-active（查看器 / 拆出的窗口置 false）。 */
  persistLastActive = true;

  /** 用户手动切 tab 时回调：main.ts 用它清 pendingStartupActive，免得迟到的远端宣告补切抢走用户已选的焦点。 */
  onManualSwitch: (() => void) | null = null;

  /** `order` = 条上看到的顺序（数字键 / `]` `[` 按它走）；缺省 = 底序 `orderedIds`。 */
  constructor(
    private readonly store: TabStore,
    private readonly order: () => readonly string[] = () => store.orderedIds,
  ) {}

  /** 把 behavior 配置应用到路由：启动时 main.ts 调一次，设置里改了再调一次。 */
  applyBehavior(cfg: BehaviorConfig): void {
    this.autoFollowUserActive = cfg.autoFollowUserActive;
    this.bringMonitorToFront = cfg.bringMonitorToFrontOnUserActive;
  }

  /**
   * 上 / 下一个 Tab 是谁。delta=+1 下一个、-1 上一个。环回。没有 tab 或算出来就是当前那个 ⇒ `null`。
   * 快捷键 Ctrl+Tab / Ctrl+Shift+Tab 用。
   */
  cycleTarget(delta: 1 | -1): string | null {
    const ids = this.order();
    if (ids.length === 0) return null;
    const idx = this.store.activeId ? ids.indexOf(this.store.activeId) : -1;
    const nextIdx = ((idx + delta) % ids.length + ids.length) % ids.length;
    const targetId = ids[nextIdx];
    if (targetId && targetId !== this.store.activeId) return targetId;
    return null;
  }

  /**
   * 第 N 个 Tab 是谁（1 起数，快捷键 Ctrl+1..9 用）。
   * N 大于现有 Tab 数 / N 对应 Tab 已经 active ⇒ `null`。
   */
  indexTarget(oneBasedIdx: number): string | null {
    const ids = this.order();
    if (oneBasedIdx < 1 || oneBasedIdx > ids.length) return null;
    const targetId = ids[oneBasedIdx - 1];
    if (targetId && targetId !== this.store.activeId) return targetId;
    return null;
  }

  /**
   * 认出「用户在终端真敲了一行回车」（type=user 且不是 tool_result 回灌 / CLI 噪声，由 tabs.onLine 的 result.kind 判）时，放不放行自动跟随。
   *
   * 跳过条件（任一命中 ⇒ `ignore`）：
   * 0. 在重放批里（批里的历史 user 消息不该触发自动切）
   * 1. autoFollowUserActive=false（设置面板关了）
   * 2. manualOverrideUntil > now（用户 5s 内手动点过 tab，明确意图保护）
   * 3. sid 不存在 / 已结束（防御）
   * 4. sid 已经是 active ⇒ `front-only`（不切，但开了「拉前 monitor」也照拉）
   *
   * 通过 ⇒ `switch`（宿主调 switchTo(sid, "auto")，可选把 monitor 拉前）。
   */
  autoFollow(sessionId: string): AutoFollowDecision {
    if (this.store.inBatch) return "ignore";
    if (!this.autoFollowUserActive) return "ignore";
    if (Date.now() < this.manualOverrideUntil) return "ignore";
    const tab = this.store.tabs.get(sessionId);
    if (!tab) return "ignore";
    if (isResumeOnly(tab.state)) return "ignore";
    if (this.store.activeId === sessionId) return "front-only";
    return "switch";
  }

  /**
   * 切完之后（`activeId` 已经换了）：
   * - 记住所在 tab（下次启动选它、重放先发它）。查看器 / 拆出的窗口与主窗口共享 localStorage（INVARIANT § 14），
   *   它们置 persistLastActive=false，免得污染主窗口的记忆。
   * - `"manual"`：设置 manualOverrideUntil = now+5s（期间拒绝自动跟随）并通知宿主；
   *   `"auto"` 不更新 override（不然自动切又设 override，自己就被锁了）。
   */
  noteSwitched(sessionId: string, source: "manual" | "auto"): void {
    if (this.persistLastActive) {
      safeSet(LS_KEYS.lastActiveSid, sessionId);
    }
    if (source === "manual") {
      this.manualOverrideUntil = Date.now() + TabRouter.MANUAL_OVERRIDE_MS;
      this.onManualSwitch?.();
    }
  }
}
