/**
 * **一个会话在主窗口上读成什么**：状态点 · 状态一句 · 需手动 · 悬停卡那一句 · 标题拆成几段。
 * 标签页行、悬停卡、会话头、「需手动」那几处都从这里取，不各拼一份。
 *
 * 只排版：点 · 短名 · 状态一句 · 悬停句都是那台后端判好的整份脸（`Tab.face`；还没收到 ⇒ 不画点）；状态一句会走的那一截
 * （`face.clock`）按此刻填。需手动的详情在会话事实里（`Tab.needs`），脸上那颗点说在等才认。
 */
import type { DotState } from "./kit/status-dot";
import type { BackgroundWork, Needs } from "./session-reads";
import type { Tab } from "./tab-model";
import { clockNow } from "./duration-format";

/** 此刻在等人 ⇒ 等的是什么（会话事实那一份；脸上那颗点说在等才认）；不在等 ⇒ `null`。 */
export function needsOf(tab: Tab): Needs | null {
  return tab.face?.dot === "needs-you" ? tab.needs : null;
}

/** 状态点的名字（读屏名 / 悬停名）：脸上的短名；还没收到脸 ⇒ `null`（不画点，也没有名字）。 */
export function stateWord(tab: Tab): string | null {
  return tab.face?.name ?? null;
}

/** 状态点（那台判好的那一颗）；还没收到脸 ⇒ `null`（不画）。 */
export function dotOf(tab: Tab): DotState | null {
  return tab.face?.dot ?? null;
}

/** 后台任务运行中那一句此刻的字：有会走的那一句 ⇒ 时长那一截按此刻填（那一个读口，与核心对同一份金样）；否则照抄核心写好的那一句。 */
export function backgroundLine(b: BackgroundWork, now: number): string {
  return b.clock ? clockNow(b.clock, now) : b.text;
}


/**
 * 状态一句（会话头 · 悬停卡第三行）：脸上那一句；带会走的那一截 ⇒ 时长按此刻填。还没收到脸 ⇒ 空。
 * `needs` ＝ 这一句是不是「需手动」（琥珀）。
 */
export function stateLine(tab: Tab, now: number): { text: string; needs: boolean } {
  const f = tab.face;
  return { text: f?.clock ? clockNow(f.clock, now) : (f?.line ?? ""), needs: f?.dot === "needs-you" };
}

/** 悬停卡里那一句 peek（脸上那一格：在等 ⇒ 等的那一步 · 在跑 ⇒ 那一步 · 空闲 / 后台 ⇒ 最后一句）；没有 ⇒ `null`（不出这一行）。 */
export function peekLine(tab: Tab): string | null {
  return tab.face?.peek ?? null;
}

/** 窄窗那一格的两个字母：项目名（后端切好的 `Tab.projectName`）里的头两个字母（小写）；没有字母 ⇒ 头两个字。 */
export function abbrOf(tab: Tab): string {
  const name = tab.projectName || tab.label || tab.sessionId;
  const letters = name.replace(/[^a-z]/gi, "");
  return (letters.length >= 2 ? letters : name).slice(0, 2).toLowerCase();
}

/**
 * 「需手动」的下一站：会话在前（`order`）、计划项在后（`planCount` 条）。站在计划项上 ⇒ `planAt` 是第几条，否则看 `active` 那个会话。
 * 走到尽头绕回开头；两边都空 ⇒ `null`。
 */
export function nextNeedsStep(order: readonly string[], active: string | null, planAt: number | null, planCount: number): { sid: string } | { plan: number } | null {
  const first = (): { sid: string } | { plan: number } | null => (order.length > 0 ? { sid: order[0] } : planCount > 0 ? { plan: 0 } : null);
  if (planAt !== null && planAt < planCount) return planAt + 1 < planCount ? { plan: planAt + 1 } : first();
  const i = active === null ? -1 : order.indexOf(active);
  if (i < 0) return first();
  if (i + 1 < order.length) return { sid: order[i + 1] };
  return planCount > 0 ? { plan: 0 } : first();
}
