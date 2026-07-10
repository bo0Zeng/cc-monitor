/**
 * Batch14-F42：Claude 完成一轮系统通知。
 *
 * 判定：实时到达（非批量重放）的 assistant 行带 `stop_reason=="end_turn"`
 * （权威判据同 aterm/HANDOFF；messages.rs F42 起透传该字段）→ 窗口在后台时
 * 发系统通知把用户叫回来。
 *
 * 误报防线（顺序即短路序，前五道全同步、零热路径成本）：
 * 1. inBatch 跳过——启动重放 / SSH 重连 chunked 重放 / 历史灌入全走批量路径；
 * 2. 非 assistant / 非 end_turn 跳过；
 * 3. 时间戳新鲜度（|now−ts| ≤ 90s）——批外漏网的旧行（如乱序补发）兜底；
 * 4. per-会话防抖 10s；
 * 5. 窗口聚焦跳过（用户正看着，不打扰）；
 * 6.（异步尾）设置开关 `notifyTurnEnd`——只在真 turn-end 才读配置，无需缓存失效;
 *    通知权限懒检查一次，拒绝后记忆、静默降级不再弹。
 *
 * 依赖注入（deps）纯为可测：生产用默认实现，vitest 全量替换。
 */
import { getBehavior } from "./behavior";

/** onLine payload 的最小形状（tabs.ts 的 LinePayload 超集兼容）。 */
export interface TurnNotifyPayload {
  message?: {
    type?: string;
    timestamp?: string;
    /** 旧版 CC 会把 subagent 行写进主文件——子 agent 完成≠主轮结束，须跳过。 */
    isSidechain?: boolean;
    message?: { stop_reason?: string | null };
  };
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
      send: pluginSend,
      ...deps,
    };
  }

  /** tabs.onLine 每行调用；内部自筛，非 turn-end 的行零开销返回。 */
  observe(sid: string, tabTitle: string, payload: TurnNotifyPayload, inBatch: boolean): void {
    if (this.disabled || inBatch) return;
    const rec = payload?.message;
    if (!rec || rec.type !== "assistant") return;
    if (rec.isSidechain) return; // 旧版 CC 的 subagent 行：子 agent 完成≠主轮结束
    if (rec.message?.stop_reason !== "end_turn") return;
    const now = this.deps.now();
    const ts = rec.timestamp ? Date.parse(rec.timestamp) : NaN;
    if (!Number.isFinite(ts) || Math.abs(now - ts) > FRESH_MS) return;
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
      await this.deps.send(`Claude 完成一轮 — ${tabTitle}`, "会话已回到等待输入状态。");
    } catch (e) {
      console.warn("turn-notify: send failed:", e);
    }
  }
}

// --- 默认 send：Tauri notification 插件（动态 import，不进主 bundle 关键路径）---
// 权限检查收敛为共享 Promise：并发首次触发只查一次（无竞态吞通知）；
// 查询/请求本身抛异常（瞬时错误）→ 重置为 null 下次重试，不永久锁死成"拒绝"。
let permPromise: Promise<boolean> | null = null;

type NotifyMod = typeof import("@tauri-apps/plugin-notification");

function ensurePermission(mod: NotifyMod): Promise<boolean> {
  permPromise ??= (async () => {
    try {
      if (await mod.isPermissionGranted()) return true;
      return (await mod.requestPermission()) === "granted"; // 用户拒绝 → 记忆，不反复弹
    } catch (e) {
      console.warn("turn-notify: permission check failed (will retry):", e);
      permPromise = null;
      return false;
    }
  })();
  return permPromise;
}

async function pluginSend(title: string, body: string): Promise<void> {
  const mod = await import("@tauri-apps/plugin-notification");
  if (!(await ensurePermission(mod))) return;
  mod.sendNotification({ title, body });
}

/** 生产单例（tabs.ts 用）。 */
export const turnEndNotifier = new TurnEndNotifier();
