/**
 * 代理完成一轮的系统通知：实时到达（非批量重放）的回复记录带 `endsTurn`（判定只在后端，与帧 `turn_end` 同一个）→ 窗口在后台时发系统通知把用户叫回来。
 *
 * 误报防线（顺序即短路序，前五道全同步、零热路径成本）：
 * 1. inBatch 跳过——启动重放 / SSH 重连 chunked 重放 / 历史灌入全走批量路径；
 * 2. 不是回复 / 不是一轮结束跳过；
 * 3. 时间戳新鲜度（|now−ts| ≤ 90s）——批外漏网的旧行（如乱序补发）兜底；
 * 4. per-会话防抖 10s；
 * 5. 窗口聚焦跳过（用户正看着，不打扰）；
 * 6.（异步尾）设置开关 `notifyTurnEnd`——只在真 turn-end 才读配置，无需缓存失效;
 *
 * 依赖注入（deps）纯为可测：生产用默认实现，vitest 全量替换。
 */
import { getBehavior } from "./behavior";
import { commands } from "./ipc/commands";
import { copyText } from "./copy-table";

/**
 * onLine payload 的最小形状（生成的 `JsonlLinePayload` 结构兼容）。
 * 不直接用生成的那个：这里只要「够不够判一轮结束」那几格（最小契约，依赖注入全量替换才可测）。
 * 子运行的记录不走主会话的行（它们进运行表），这里见到的全是主运行的记录。
 */
export interface TurnNotifyPayload {
  record?: { t?: string; at?: string; atMs?: number; endsTurn?: boolean };
}

export interface TurnNotifyDeps {
  isFocused(): boolean;
  now(): number;
  enabled(): Promise<boolean>;
  send(title: string, body: string): Promise<void>;
}

// 已知限制：新鲜度用本机时钟对记录时间戳——远端主机时钟漂移 >90s 时该主机的
// 通知会被整体吞掉（只漏报不误报，方向安全）。
const FRESH_MS = 90_000;
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

  /** tabs.onLine 每行调用；内部自筛，非 turn-end 的行零开销返回。 */
  observe(sid: string, tabTitle: string, payload: TurnNotifyPayload, inBatch: boolean): void {
    if (this.disabled || inBatch) return;
    const rec = payload?.record;
    if (!rec || rec.t !== "reply" || rec.endsTurn !== true) return;
    const now = this.deps.now();
    // 那条记录的时刻：后端解好的毫秒（`atMs`），界面不解析 `at`。
    const ts = rec.atMs;
    if (ts === undefined || Math.abs(now - ts) > FRESH_MS) return;
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
