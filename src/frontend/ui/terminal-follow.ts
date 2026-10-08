/**
 * **终端实时预览（界面这一半）**：订那台后端的 `terminal-follow`，收一屏就交给页面画，画完回执；离开就退订。
 *
 * - 先订流 `terminal-screen/<票>`（壳把那台推来的 `terminal_screen` / `terminal_follow_end` 帧原样交进来），再发 `terminal-follow {terminal, ticket}`：
 *   第一帧不会落在没人收的时候。票由这里铸（不透明，后端只回填）。
 * - 一帧在途：后端推一屏之后等回执才推下一帧（有变化才推）。这里收一屏 ⇒ 交页面（同步画完）⇒ 回执那一帧的序号；
 *   **两次回执至少隔 {@link ACK_GAP_MS}**（约每秒 10 帧封顶，节奏归看的这一方，后端不起节拍）。回执时补一格 credit。
 * - 退订：撤流、发 `terminal-unfollow`（幂等，不等结局）；之后来的帧不交、排着的回执不发。
 * - 停了：后端说停（终端没了 · 看着它的路断了 · 一屏太大）· 流断了 · 订不上（那台只能快照 / 别的原因）⇒ 交一次 {@link FollowStop}，不自己重连。
 */
import { chan, ChanError, type Item } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, refusalOf } from "./ipc/chan-caller";
import { isObj } from "./ipc/decode";
import type { Origin } from "./ipc/origin";
import { decodeShot, type TerminalShot } from "./terminal-reads";

/** 那条画面流（与壳 `event_replay.rs::TERMINAL_SCREEN_KIND` 同一个串；后面跟 `/<票>`）。流名刻意不叫命令名（命令是 `terminal-follow`）。 */
export const TERMINAL_SCREEN_KIND = "terminal-screen";

/** 两次回执之间至少隔多久（ms）：实时画面的节奏上限。 */
export const ACK_GAP_MS = 100;

/** 订阅 · 回执 · 退订那几问的期限（订上要起几个 tmux；远端没连着还要握手）。 */
const FOLLOW_BUDGET_MS = 15_000;

/** 一开始给流几格 credit（一帧在途，再留一格给收尾帧）。 */
const INITIAL_WANT = 2;

/** 订阅停了：只能快照（那台不支持实时，带原因）· 停了（带原因）。 */
export type FollowStop =
  | { kind: "snapshotOnly"; why: "old" | "tmux" | "noTmux" }
  | { kind: "stopped"; why: "gone" | "lost" | "tooBig" | "offline" };

/** 页面收的两件事：一屏 · 停了（至多一次）。 */
export interface FollowEvents {
  screen(shot: TerminalShot): void;
  stop(why: FollowStop): void;
}

/** 通道那几样（缺省走通道；测试换成假的）。 */
export interface FollowPort {
  subscribe(origin: Origin, kind: string, want: number, sink: (items: Item[]) => void): Promise<{ want(more: number): void; stop(): void }>;
  follow(origin: Origin, args: { terminal: string; ticket: string }): Promise<unknown>;
  ack(origin: Origin, args: { ticket: string; seq: number }): Promise<unknown>;
  unfollow(origin: Origin, args: { ticket: string }): Promise<unknown>;
}

export const CHANNEL_PORT: FollowPort = {
  subscribe: (origin, kind, want, sink) => chan.subscribe(origin, kind, null, want, sink),
  follow: (origin, args) => {
    const body = jsonBody(args);
    const budget = budgetWithin(FOLLOW_BUDGET_MS);
    return chan.call(origin, "terminal-follow", body, budget);
  },
  ack: (origin, args) => {
    const body = jsonBody(args);
    const budget = budgetWithin(FOLLOW_BUDGET_MS);
    return chan.call(origin, "terminal-follow-ack", body, budget);
  },
  unfollow: (origin, args) => {
    const body = jsonBody(args);
    const budget = budgetWithin(FOLLOW_BUDGET_MS);
    return chan.call(origin, "terminal-unfollow", body, budget);
  },
};

export interface Follow {
  /** 退订（幂等）。 */
  stop(): void;
}

let tickets = 0;

/** 订不上的那一问 ⇒ 停在哪。 */
function stopOfStart(e: unknown): FollowStop {
  if (e instanceof ChanError) {
    const err = e.error;
    if (err.layer === "peer" && err.why === "unsupported") return { kind: "snapshotOnly", why: "old" };
    if (err.layer === "peer" && err.why === "refused") {
      switch (refusalOf(err.body)?.code) {
        case "tmux_too_old":
          return { kind: "snapshotOnly", why: "tmux" };
        case "no_tmux":
          return { kind: "snapshotOnly", why: "noTmux" };
        case "not_known":
        case "ambiguous":
          return { kind: "stopped", why: "gone" };
        default:
          return { kind: "stopped", why: "lost" };
      }
    }
    if (err.layer === "hop") return { kind: "stopped", why: "offline" };
  }
  return { kind: "stopped", why: "lost" };
}

/** 收尾那一格的原因 ⇒ 停在哪。认不出 ⇒ `null`（当读不懂）。 */
function stopOfEnd(end: unknown): FollowStop | null {
  switch (end) {
    case "gone":
      return { kind: "stopped", why: "gone" };
    case "lost":
      return { kind: "stopped", why: "lost" };
    case "too_big":
      return { kind: "stopped", why: "tooBig" };
    default:
      return null;
  }
}

/** 订 `origin` 上 `terminal` 那个终端的实时画面。 */
export function startFollow(origin: Origin, terminal: string, events: FollowEvents, port: FollowPort = CHANNEL_PORT): Follow {
  const ticket = `tf${++tickets}-${Math.random().toString(36).slice(2, 10)}`;
  let over = false;
  let sub: { want(more: number): void; stop(): void } | null = null;
  let ackTimer: ReturnType<typeof setTimeout> | null = null;
  let lastAckAt = -Infinity;

  const close = (): void => {
    over = true;
    if (ackTimer !== null) clearTimeout(ackTimer);
    ackTimer = null;
    sub?.stop();
  };
  /** 停了（后端已经忘了这张票，或根本没订上）：交一次、撤流。 */
  const halt = (why: FollowStop): void => {
    if (over) return;
    close();
    events.stop(why);
  };
  const ack = (seq: number): void => {
    if (ackTimer !== null) clearTimeout(ackTimer);
    const send = (): void => {
      ackTimer = null;
      if (over) return;
      lastAckAt = performance.now();
      sub?.want(1);
      void port.ack(origin, { ticket, seq }).catch(() => {
        // 回执没送到：后端那一侧要么已经停了（随后来收尾帧），要么那台断了（流随之断）⇒ 由那两条路说。
      });
    };
    const wait = Math.max(0, lastAckAt + ACK_GAP_MS - performance.now());
    if (wait === 0) send();
    // 调度：一次性 —— 两次回执至少隔 100 ms（实时画面的节奏上限）；退订 / 停了即清
    else ackTimer = setTimeout(send, wait);
  };
  const onItems = (items: Item[]): void => {
    for (const it of items) {
      if (over) return;
      if (it.t === "frame") {
        let cell: unknown;
        try {
          cell = JSON.parse(it.body);
        } catch {
          cell = null;
        }
        if (isObj(cell) && "end" in cell) {
          halt(stopOfEnd(cell.end) ?? { kind: "stopped", why: "lost" });
          return;
        }
        let shot: TerminalShot;
        try {
          if (!isObj(cell) || typeof cell.seq !== "number") throw new Error("no seq");
          shot = decodeShot(origin, cell.view);
        } catch {
          halt({ kind: "stopped", why: "lost" });
          void port.unfollow(origin, { ticket }).catch(() => {});
          return;
        }
        events.screen(shot);
        ack(cell.seq as number);
      } else if (it.t === "gap" || it.t === "closed" || it.t === "unseen") {
        halt({ kind: "stopped", why: "offline" });
        void port.unfollow(origin, { ticket }).catch(() => {});
        return;
      }
    }
  };

  void (async () => {
    const s = await port.subscribe(origin, `${TERMINAL_SCREEN_KIND}/${ticket}`, INITIAL_WANT, onItems);
    sub = s;
    if (over) {
      s.stop();
      return;
    }
    try {
      await port.follow(origin, { terminal, ticket });
    } catch (e) {
      halt(stopOfStart(e));
    }
  })();

  return {
    stop: () => {
      if (over) return;
      close();
      void port.unfollow(origin, { ticket }).catch(() => {});
    },
  };
}
