/**
 * `session-tasks` 那条流在界面这一侧的全部知识（叶子模块：只 import 通道的格类型）。
 * 那台后端盯任务目录、变了发 `tasks_changed{sid}`；monitor 交进 `subscribe(origin, "session-tasks")`；界面读格、重问 `tasks-list`。
 */
import type { Item } from "../../comms/inward/chan";

/** 那台机器上「某个会话的任务清单变了」那条流（与 Rust `event_replay.rs::SESSION_TASKS_KIND` 同一个串）。 */
export const SESSION_TASKS_KIND = "session-tasks";
/** 每条 `session-tasks` 订阅一开始给多少格 credit（稀：人敲一条命令级），用完一格还一格。 */
export const SESSION_TASKS_WINDOW = 16;

/**
 * `session-tasks` 流里的一批格 ⇒ 哪几个会话要重问、要不要整台重问、还多少 credit（纯函数）。
 * `frame` 体是 `{"sid": …}`（读不出 sid ⇒ 当整台重问，不猜）；`seen`（那台又接上了）· `gap`（丢了几格）⇒ 整台重问（期间的变更可能漏了）。
 */
export function tasksChangedItems(items: readonly Item[]): { sids: string[]; all: boolean; frames: number } {
  const sids = new Set<string>();
  let all = false;
  let frames = 0;
  for (const it of items) {
    if (it.t === "frame") {
      frames += 1;
      let sid: unknown = null;
      try {
        sid = (JSON.parse(it.body) as { sid?: unknown }).sid;
      } catch {
        sid = null;
      }
      if (typeof sid === "string" && sid !== "") sids.add(sid);
      else all = true;
    } else if (it.t === "seen" || it.t === "gap") {
      all = true;
    }
  }
  return { sids: [...sids], all, frames };
}
