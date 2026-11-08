/**
 * 「号 ＋ 余量」小件：号牌（那台写好的号牌字与号色）· 号名 · 那台写好的那一格余量（「5h 62% ↻18:30」·「5h 读不到」·
 * 缓存那一份带「上次所知」）。没有那一格（没出过数）⇒ 只露号。额度只在这一个样子里出现，详细看在用量那一页。
 * 号名与余量都照那台 `accounts-list` 那一行（`label` · `slots`，后端 `with_quota` 写）：哪台的号照哪台那一行，界面不另取字。
 */
import { appStore } from "./app-store";
import type { Account } from "./accounts";
import type { Machine } from "./ipc/origin";
import { el } from "./kit/el";
import s from "./account-line.module.css";

/** 那台 `accounts-list` 里这个号那一行；那台没读过 · 没有这个号 ⇒ `undefined`。 */
function rowOf(origin: Machine, account: string): Account | undefined {
  return appStore.accounts.get().get(origin)?.accounts.find((a) => a.name === account);
}

/** 号在那台叫什么（那一行的 `label`）；那一行没有 ⇒ 原名。 */
export function labelOf(origin: Machine, account: string): string {
  return rowOf(origin, account)?.label ?? account;
}

/** 那台那一行的第一格余量（字都是那台写好的）；没有 ⇒ `null`。 */
export function quotaCellOf(origin: Machine, account: string): string | null {
  return rowOf(origin, account)?.slots?.[0]?.text ?? null;
}

/** `name` 给元素 ⇒ 照它摆（调用方要给它挂 id · 样式时）；给字 ⇒ 这里包一层。 */
export function accountLine(spec: { plate: HTMLElement; name: string | HTMLElement; quota: string | null }): HTMLElement {
  const line = el("span", s.acctLine);
  line.dataset.role = "account-line";
  const name = typeof spec.name === "string" ? el("span", "", spec.name) : spec.name;
  name.classList.add(...s.acctLineName.split(" "));
  line.append(spec.plate, name);
  if (spec.quota) {
    const q = el("span", s.acctLineQuota, spec.quota);
    q.dataset.part = "quota";
    line.appendChild(q);
  }
  return line;
}
