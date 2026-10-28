/**
 * **「各台都报完了」那一刻**（会话流的 `listed` 格）：迁旧分组 · 「启动时记住的那一格」没等到就明说，都只认这一处。
 *
 * 每台一条会话流。壳说「各台都报完」（`listed.all`）是壳那一本账的判断，落在**某一台**那条流上；别的台那条流上
 * 它自己的宣告与「报完了」可能还在路上（就绪点重放一条订阅接一条交：本机那条的终局带着 `all`，远端那条的骨架排在它后面）。
 * ⇒ 壳说过、**而且**订着的每一台都在自己那条流上报完了，才算那一刻；回调一次。
 */
import type { Origin } from "./ipc/origin";

export class AllListed {
  /** 订着会话流的那几台；空 ⇒ 只等壳那一刻（查看窗等不订整台的）。 */
  private machines: Origin[] = [];
  /** 在自己那条流上报完了的那几台。 */
  private readonly seen = new Set<Origin>();
  /** 壳说过「各台都报完」。 */
  private shellSaid = false;
  private fired = false;

  /** `onSettled`：那一刻（只一次）。 */
  constructor(private readonly onSettled: () => void) {}

  /** 订的那几台换了（起步 · 机器表变了）。摘掉的正是唯一没报完的那台 ⇒ 当场算。 */
  setMachines(origins: readonly Origin[]): void {
    this.machines = [...origins];
    this.check();
  }

  /** 那台在自己那条流上报完了（那条流上来了一格 `listed`）。 */
  reported(origin: Origin): void {
    this.seen.add(origin);
    this.check();
  }

  /** 壳说各台都报完了（那一格带 `all`）。 */
  shellSays(): void {
    this.shellSaid = true;
    this.check();
  }

  /** 那一刻到了没有。 */
  get settled(): boolean {
    return this.shellSaid && this.machines.every((o) => this.seen.has(o));
  }

  private check(): void {
    if (this.fired || !this.settled) return;
    this.fired = true;
    this.onSettled();
  }
}
