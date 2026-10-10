/**
 * 代理完成一轮的系统通知：会话流里的 `turn_end` 格（那台后端的帧 `turn_end`，在不在看都来 —— 没在看的会话行不上流，
 * 通知不能再认行）→ 窗口在后台时发系统通知把用户叫回来。一轮结束的判定只在后端（`agents/<名>/turn.rs`）。
 *
 * 误报防线（顺序即短路序）：
 * 1. per-会话防抖 10s；
 * 2. 窗口聚焦跳过（用户正看着，不打扰）；
 * 3.（异步尾）设置开关 `notifyTurnEnd`——只在真 turn-end 才读配置，无需缓存失效;
 * 那一格不进留存、不重放 ⇒ 启动重放 · 按行号取回的历史都不会经过这里（从前认行时要靠批模式 · 时间戳新鲜度挡）。
 *
 * 依赖注入（deps）纯为可测：生产用默认实现，vitest 全量替换。
 */
import { getBehavior } from "./behavior";
import { commands } from "./ipc/commands";
import { copyText } from "./copy-table";

export interface TurnNotifyDeps {
  isFocused(): boolean;
  now(): number;
  enabled(): Promise<boolean>;
  send(title: string, body: string): Promise<void>;
}

const DEBOUNCE_MS = 10_000;

export class TurnEndNotifier {
  private deps: TurnNotifyDeps;
  /** sid → 上次通知（或已判定要通知）时刻 ms。 */
  private lastNotify = new Map<string, number>();
  /**
   * 独立 viewer 窗口置 true：viewer 是独立 webview（自带一份本单例），广播行照收
   * ——不禁用则主窗口+viewer 各发一条（跨窗口防抖不互通），且用户正聚焦 viewer 读
   * 该会话时主窗口 hasFocus()===false 仍会发。只让主窗口发，一举消掉两个病态。
   */
  private disabled = false;

  /** viewer 窗口 bootstrap 时调用：本窗口永不发通知。 */
  disable(): void {
    this.disabled = true;
  }

  constructor(deps?: Partial<TurnNotifyDeps>) {
    this.deps = {
      isFocused: () => document.hasFocus(),
      now: () => Date.now(),
      enabled: async () => (await getBehavior()).notifyTurnEnd,
      send: notifySend,
      ...deps,
    };
  }

  /** 会话流的 `turn_end` 格（`TabManager.onTurnEnd`）：那个会话一轮结束了。 */
  observe(sid: string, tabTitle: string): void {
    if (this.disabled) return;
    const now = this.deps.now();
    const last = this.lastNotify.get(sid) ?? 0;
    if (now - last < DEBOUNCE_MS) return;
    if (this.deps.isFocused()) return;
    // 先记账再进异步尾：异步窗口期同会话再来一条也不会重发。
    this.lastNotify.set(sid, now);
    void this.finish(tabTitle);
  }

  private async finish(tabTitle: string): Promise<void> {
    try {
      if (!(await this.deps.enabled())) return;
      await this.deps.send(copyText("turnNotify.finish.title", { tab: tabTitle }), copyText("turnNotify.finish.body"));
    } catch (e) {
      console.warn("turn-notify: send failed:", e);
    }
  }
}

// --- 默认 send：壳的 `notify_desktop`（平台那一半在 `platform/notify.rs`：Linux 上连接留到通知关掉，GNOME 才不当场收走）---

/** 发一条系统通知（「需手动」那条也走这里）。 */
export async function notifySend(title: string, body: string): Promise<void> {
  await commands.notify_desktop({ title, body });
}

/** 生产单例（tabs.ts 用）。 */
export const turnEndNotifier = new TurnEndNotifier();
