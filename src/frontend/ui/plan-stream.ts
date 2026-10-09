/**
 * `plan-changed` 那条流在界面这一侧的全部知识（叶子模块：只 import 通道的格类型）。
 * 那台后端盯 pb 工作区、计划变了发 `plan_changed{workspace, rev, needs}`；monitor 交进 `subscribe(origin, "plan-changed")`；
 * 界面读格、重问 `plan-read`（或 `plan-list`）。
 */
import type { Item } from "../../comms/inward/chan";

/** 与 Rust `event_replay.rs::PLAN_CHANGED_KIND` 同一个串。 */
export const PLAN_CHANGED_KIND = "plan-changed";
/** 每条订阅一开始给多少格 credit（稀：计划仓写一次一格），用完一格还一格。 */
export const PLAN_CHANGED_WINDOW = 16;

/** 一个工作区变成了哪一份（`rev`）、那一刻要你看几条（后端没给 ⇒ `null`）。 */
export interface PlanMoved {
  workspace: string;
  rev: string;
  needs: number | null;
}

/**
 * 一批格 ⇒ 哪几个工作区变了（同一个工作区留最后一格）、要不要整台重问、还多少 credit（纯函数）。
 * 体读不出工作区 ⇒ 当整台重问（不猜）；`seen`（又接上了）· `gap`（丢了几格）⇒ 整台重问。
 */
export function planChangedItems(items: readonly Item[]): { moved: PlanMoved[]; all: boolean; frames: number } {
  const moved = new Map<string, PlanMoved>();
  let all = false;
  let frames = 0;
  for (const it of items) {
    if (it.t === "frame") {
      frames += 1;
      let body: { workspace?: unknown; rev?: unknown; needs?: unknown } | null = null;
      try {
        body = JSON.parse(it.body) as { workspace?: unknown; rev?: unknown; needs?: unknown };
      } catch {
        body = null;
      }
      if (body && typeof body.workspace === "string" && body.workspace !== "" && typeof body.rev === "string") {
        moved.set(body.workspace, { workspace: body.workspace, rev: body.rev, needs: typeof body.needs === "number" ? body.needs : null });
      } else all = true;
    } else if (it.t === "seen" || it.t === "gap") {
      all = true;
    }
  }
  return { moved: [...moved.values()], all, frames };
}
