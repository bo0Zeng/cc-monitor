/**
 * 计划页 ↔ 组：块表「组」那一格 · 按组筛块 · 从标签页组菜单进来先落哪一片。
 * 接手会话在哪几组、那一格写什么都是后端标好的（`PlanBlock.groups`）；这里只照它摆、按 id 筛。
 */
import { attachTooltip } from "../kit/tooltip";
import type { PlanBlock, PlanList } from "../plan-reads";
import type { Origin } from "../ipc/origin";
import s from "./plan.module.css";

/** 块表「组」那一格：照后端写的字；组模块没答 ⇒ 警示色、悬停是那一句。 */
export function groupsCell(b: Pick<PlanBlock, "groups">): HTMLElement {
  const el = document.createElement("span");
  el.className = `${s.pvGroups} plan-groups`;
  el.textContent = b.groups.text;
  const why = b.groups.unread;
  if (why !== null) {
    el.dataset.tone = "warn";
    el.setAttribute("aria-label", why);
    attachTooltip(el, why);
  }
  return el;
}

/** 这一组的会话接手的那几块（后端读到的组里有这一组 id 的）。 */
export function blocksOfGroup<B extends Pick<PlanBlock, "groups">>(blocks: readonly B[], gid: string): B[] {
  return blocks.filter((b) => b.groups.groups.some((x) => x.id === gid));
}

/** 组员接手的块在哪一片：照各台 `plan-list` 的 `bySession`，组员的先后里头一个接了块的那一片（只认同一台的）。 */
export function sliceForGroup(
  lists: ReadonlyMap<Origin, PlanList>,
  members: readonly { origin: Origin; sid: string }[],
): { origin: Origin; workspace: string; name: string } | null {
  for (const m of members) {
    for (const ws of lists.get(m.origin)?.workspaces ?? []) {
      const b = ws.bySession[m.sid];
      if (b) return { origin: m.origin, workspace: ws.workspace, name: b.slice };
    }
  }
  return null;
}
