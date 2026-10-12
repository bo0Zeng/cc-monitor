/**
 * 「需手动」的另两件事：窗口标题（`cc-monitor · 需手动 2`）· 系统通知（主窗口不在前台时、一个会话**开始**等你就发一条）。
 * 消息流底部那一块（等什么、在这里就地答）住 `answer-block.ts`。
 */
import type { Tab } from "./tab-model";
import type { Needs } from "./session-reads";
import { copyText } from "./copy-table";
import { machineOf, needsOf, fullTitle } from "./session-face";

/**
 * 窗口标题与系统通知：每次标签页栏刷新时喂一次（`observe`）。
 * - 标题：`cc-monitor · 需手动 N`（0 个时只有 `cc-monitor`）；变了才写。
 * - 通知：一个会话**开始**等你（上一次喂的时候它不在等）、主窗口不在前台、设置里开着 ⇒ 发一条：标题 `{title}（{machine}）· 等批准`，正文是那一句。
 *   那一刻会话事实还没到（种类不明）⇒ 等它到了再发（不先发一条「需手动」再补一条）；到不了（老后端）⇒ 最多等 [`NOTIFY_WAIT_MS`] 照发。
 */
export const NOTIFY_WAIT_MS = 3000;

export interface NeedsWatchDeps {
  setTitle(title: string): void;
  isFocused(): boolean;
  enabled(): Promise<boolean>;
  send(title: string, body: string): Promise<void>;
  now(): number;
}

export class NeedsWatch {
  private title: string | null = null;
  /** 在等你的会话 → 这一次等待通知过没有 · 开始等的那一刻（本机时钟）。 */
  private readonly waiting = new Map<string, { notified: boolean; seenAt: number }>();

  constructor(private readonly deps: NeedsWatchDeps) {}

  observe(tabs: Iterable<Tab>): void {
    let n = 0;
    const now = this.deps.now();
    const still = new Set<string>();
    for (const t of tabs) {
      const need = needsOf(t);
      if (!need) continue;
      n++;
      still.add(t.sessionId);
      let w = this.waiting.get(t.sessionId);
      if (!w) {
        w = { notified: false, seenAt: now };
        this.waiting.set(t.sessionId, w);
      }
      if (!w.notified && (t.needs !== null || now - w.seenAt >= NOTIFY_WAIT_MS)) {
        w.notified = true;
        void this.notify(t, need);
      }
    }
    for (const sid of [...this.waiting.keys()]) if (!still.has(sid)) this.waiting.delete(sid);
    const title = n > 0 ? copyText("needs.windowTitle.count", { n }) : copyText("needs.windowTitle.base");
    if (title !== this.title) {
      this.title = title;
      this.deps.setTitle(title);
    }
  }

  private async notify(t: Tab, need: Needs): Promise<void> {
    if (this.deps.isFocused()) return;
    try {
      if (!(await this.deps.enabled())) return;
      await this.deps.send(copyText("needs.notify.title", { title: fullTitle(t), machine: machineOf(t), kind: need.text }), need.what ?? "");
    } catch (e) {
      console.warn("needs-notify: send failed:", e);
    }
  }
}
