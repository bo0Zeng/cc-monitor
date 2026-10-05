/**
 * **手动对齐**：问那台机器的后端 `resync`（本机与远端同一条 `chan.call`）。
 *
 * 事件驱动漏一拍就一直错（tmux 选项被外部改掉、inotify 换 inode）；后端零定时器，所以兜底是**用户按按钮**，
 * 后端重跑起步那一套、只对差异发帧。三个落点共用本文件这一个口：
 * 设置页机器一行「重新对齐」（整机）· 关卡 2 拒绝提示「对齐后重试」（带 sid，只对那一个会话：重验 ＋ 重打 ＋ 再过一次关卡）·
 * tab 栏「重新读取」（有打开 tab 的每台各一次整机，`resyncMachines`）。
 */
import { copyText } from "./copy-table";
import { ControlError, exactKeys, isObj, settle, unreadable, type Refusals } from "./control-said";
import { toast } from "./kit/toast";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, refusalOf } from "./ipc/chan-caller";
import { isLocalOrigin, type Origin } from "./ipc/origin";

/** 期限：后端要等每份 watcher 对完表、打完标（起几次 tmux）。 */
const RESYNC_BUDGET_MS = 20_000;

/** 一次对齐的差异（后端 `resync` 的成品）。 */
export interface Resynced {
  added: number;
  removed: number;
  retagged: number;
  /** 在跟的会话这趟从游标补读出几行（补出来的行照常经流到达）。 */
  caughtUp: number;
  watchers: number;
}

const COUNTS = ["added", "removed", "retagged", "caught_up", "watchers"] as const;
/** 那台当下的能力事实：monitor 在通道那一跳已经收进 `Offer`（唯一住处），界面只核形状、不读。 */
const FACTS = ["unavailable", "uncancellable"] as const;

/** 成品恰好是五个非负整数 ＋ 两格数组 ⇒ 收；否则抛（两边版本对不上）。 */
export function decodeResynced(origin: Origin, v: unknown): Resynced {
  if (
    !isObj(v) ||
    !exactKeys(v, [...COUNTS, ...FACTS]) ||
    !COUNTS.every((k) => Number.isInteger(v[k]) && (v[k] as number) >= 0) ||
    !FACTS.every((k) => Array.isArray(v[k]))
  ) {
    throw unreadable(origin, "resync", "is not exactly {added, removed, retagged, caught_up, watchers, unavailable, uncancellable}");
  }
  return {
    added: v.added as number,
    removed: v.removed as number,
    retagged: v.retagged as number,
    caughtUp: v.caught_up as number,
    watchers: v.watchers as number,
  };
}

const refusals: Refusals = {
  byCode: (_code, detail) => copyText("resync.call.refused", { detail }),
  noReason: () => copyText("resync.call.noReason"),
};

/** 让 `origin` 那台对齐一次；给了 `sid` 就只对那一个会话。失败 ⇒ 抛 [`ControlError`]。 */
export async function resync(origin: Origin, sid?: string): Promise<Resynced> {
  const payload = jsonBody(sid === undefined ? {} : { sid });
  const budget = budgetWithin(RESYNC_BUDGET_MS);
  const v = await settle(origin, "resync", chan.call(origin, "resync", payload, budget), refusals);
  return decodeResynced(origin, v);
}

/** 对齐的结果 ⇒ 一句话：对齐差异 ＋补读了几条（0 ⇒「没有漏的」）。 */
export function resyncSaid(r: Resynced): string {
  if (r.watchers === 0) return copyText("resync.done.nobody");
  const diff =
    r.added + r.removed + r.retagged === 0
      ? copyText("resync.done.same")
      : copyText("resync.done.diff", { added: r.added, removed: r.removed, retagged: r.retagged });
  const caught = r.caughtUp === 0 ? copyText("resync.done.caughtNone") : copyText("resync.done.caughtUp", { n: r.caughtUp });
  return `${caught}${diff}`;
}

/** 一台的结果：对上了（差异），或没问到（原因）。 */
export type MachineResynced = { origin: Origin; r: Resynced } | { origin: Origin; why: string };

/**
 * tab 栏「重新读取」：每台（去重）发一次整机 `resync`，各台并行、互不等；一台失败不拖别的台。
 * 整机那一趟后端会让每个在跟的会话从游标补读。
 */
export function resyncMachines(origins: Iterable<Origin>): Promise<MachineResynced[]> {
  return Promise.all(
    [...new Set(origins)].map((origin) =>
      resync(origin).then(
        (r): MachineResynced => ({ origin, r }),
        (e: unknown): MachineResynced => ({ origin, why: e instanceof Error ? e.message : String(e) }),
      ),
    ),
  );
}

/** 按台一行：「本机：…」「laptop：没问到（…）」。 */
export function resyncMachinesSaid(rs: readonly MachineResynced[]): string {
  return rs
    .map((m) => {
      const machine = isLocalOrigin(m.origin) ? copyText("resync.machines.local") : m.origin;
      return "r" in m
        ? copyText("resync.machines.ok", { machine, said: resyncSaid(m.r) })
        : copyText("resync.machines.failed", { machine, why: m.why });
    })
    .join("\n");
}

/** 这次失败是不是关卡 2（身份门）拒的：后端回 `wrong_owner`。 */
export function isIdentityRefusal(e: unknown): boolean {
  return e instanceof ControlError && e.error?.layer === "peer" && e.error.why === "refused" && refusalOf(e.error.body)?.code === "wrong_owner";
}

/**
 * 关卡 2 拒了 ⇒ 那条失败提示带「对齐后重试」：点它 ⇒ 只对 `sid` 那一个会话对齐（没有 sid 就整机）⇒ 再做一次 `again`（再过一次关卡）。
 * `said` 是调用方已经说好的那句失败。
 */
export function offerResyncRetry(origin: Origin, sid: string | undefined, headline: string, said: string, again: () => Promise<void>): void {
  toast(headline, `${said}\n${copyText("resync.retry.hint")}`, {
    onClick: () =>
      void (async () => {
        try {
          await resync(origin, sid);
          await again();
        } catch (e) {
          toast(copyText("resync.retry.failed"), e instanceof Error ? e.message : String(e));
        }
      })(),
  });
}
