/**
 * **起会话用哪个号 —— 界面这一侧只剩两件事**：说出要什么（跟随 / 账号 0 / 用户点名），和那台说「选不了」时给的那个显式选择。
 *
 * 判定住会话所在那台的后端（`launch-local` · `launch-render-cli` · `sessions-start` 的 `account` 那一格）：
 * 跟随 ＝ 这条会话上次用的号 → 那台的默认号 → 不指定；上次的号选不了 ⇒ 那台不起、回 `account_unavailable`，
 * 这里照它的 `data` 说清、给「改用某号」的显式选择，点了以点名再起一次。「上次用哪个号起的」由那台自己记。
 */
import type { AccountAsk } from "./generated/AccountAsk";
import type { AccountUnavailable } from "./generated/AccountUnavailable";
import { ChanError } from "../../comms/inward/chan";
import { refusalOf } from "./ipc/chan-caller";
import { ControlError } from "./control-said";
import { isLocalOrigin } from "./ipc/origin";
import { showActionFailureToast } from "./error-toast";
import { copyText } from "./copy-table";

export type { AccountAsk, AccountUnavailable };

/** 跟随：这条会话上次用的号 → 那台的默认号 → 不指定（新起的会话没有上次的号）。 */
export const FOLLOW: AccountAsk = { kind: "follow" };

/**
 * **用户点名的那一个号**（菜单里点的 · 选择框里点的 · 分叉沿用源会话的，那台推出的名字）—— 线上 `named` 只从这里出。
 * `null` = 账号 0；名字 ⇒ 按名字（那台判它选不选得了）。
 */
export function chosenAccount(name: string | null): AccountAsk {
  return name === null ? { kind: "base" } : { kind: "named", name };
}

/** 这次失败是不是那台说「要的号选不了」；是 ⇒ 那一形（`data`）。 */
export function accountUnavailableOf(e: unknown): AccountUnavailable | null {
  const err = e instanceof ControlError ? e.error : e instanceof ChanError ? e.error : undefined;
  if (!err || err.layer !== "peer" || err.why !== "refused") return null;
  const r = refusalOf(err.body);
  const d = r?.code === "account_unavailable" ? r.data : undefined;
  if (d === null || typeof d !== "object") return null;
  const u = d as Record<string, unknown>;
  return typeof u.requested === "string" &&
    typeof u.pinned === "boolean" &&
    typeof u.listKnown === "boolean" &&
    (u.alternative === null || typeof u.alternative === "string")
    ? (u as unknown as AccountUnavailable)
    : null;
}

/**
 * **账号选不了 ⇒ 不起、说清、给一个显式选择** —— 本机远端同一句话、同一个出口（那台回 `account_unavailable` 时调）。
 *
 * 选择：那台给了替代（它的默认号）⇒ 点了用那个号；没有 / 清单读不出 ⇒ 「不指定账号」（账号 0）。
 * `choose` 收的是点名那一形，调用方拿它再起一次。
 */
export function refuseUnavailableAccount(r: {
  machine: string;
  u: AccountUnavailable;
  choose: (account: AccountAsk) => void | Promise<unknown>;
}): void {
  const machine = isLocalOrigin(r.machine) ? copyText("accountPick.machine.local") : r.machine;
  const name = r.u.requested;
  const alt = r.u.listKnown ? r.u.alternative : null;
  const body = !r.u.listKnown
    ? copyText("accountPick.refused.listUnknown", { machine, name })
    : r.u.pinned
      ? alt === null
        ? copyText("accountPick.refused.pinGoneToBase", { name })
        : copyText("accountPick.refused.pinGoneToCurrent", { name, current: alt })
      : alt === null
        ? copyText("accountPick.refused.explicitGoneToBase", { name })
        : copyText("accountPick.refused.explicitGoneToCurrent", { name, current: alt });
  showActionFailureToast(copyText("accountPick.refused.title"), body, {
    level: "error",
    durationMs: 15000,
    onClick: () => void r.choose(chosenAccount(alt)),
  });
}
