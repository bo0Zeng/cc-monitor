/**
 * 「号 ＋ 余量」小件：号牌（那台写好的号牌字与号色）· 号名 · 那台写好的那一格余量（「5h 62% ↻18:30」·「5h 读不到」·
 * 缓存那一份带「上次所知」）。没有那一格（没出过数 · 按量号）⇒ 只露号。额度只在这一个样子里出现，详细看在用量那一页。
 */
import { quotaReadOf } from "./app-store";
import type { Machine } from "./ipc/origin";
import { el } from "./kit/el";
import s from "./account-line.module.css";

/** 那台额度账里这个号那一格（账读不出 ⇒ 顶上那一格「5h 读不到」；没出过数 ⇒ `null`）。字都是那台写好的。 */
export function quotaCellOf(origin: Machine, account: string): string | null {
  const q = quotaReadOf(origin);
  if (!q) return null;
  return q.fiveHour ?? q.accounts.find((a) => a.account === account)?.fiveHour ?? null;
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
