/**
 * `quota-changed` 那条流在界面这一侧的全部知识（叶子模块：只 import 通道的格类型）。
 * 那台后端额度账变了发 `quota_changed`、某个会话的轮换 / 账号格变了发 `rotation_changed{sid}`、某个 pb 工作区的计划变了发 `plan_changed{workspace, rev, needs}`；
 * monitor 交进 `subscribe(origin, "quota-changed")`；界面读格、重问 `quota-read` / `rotation-session-read`。
 */
import type { Item } from "../../comms/inward/chan";

/** 与 Rust `event_replay.rs::QUOTA_CHANGED_KIND` 同一个串。 */
export const QUOTA_CHANGED_KIND = "quota-changed";
/** 每条订阅一开始给多少格 credit（每一发请求至多一格），用完一格还一格。 */
export const QUOTA_CHANGED_WINDOW = 32;

/** 一个 pb 工作区变成了哪一份（`rev`）、那一刻要你看几条（后端没给 ⇒ `null`）。 */
export interface PlanMoved {
  workspace: string;
  rev: string;
  needs: number | null;
}

type Body = { quota?: unknown; sid?: unknown; rules?: unknown; plan?: { workspace?: unknown; rev?: unknown; needs?: unknown } | null };

/**
 * 一批格 ⇒ 额度账要不要重读、哪几个会话的轮换要重问、要不要整台重问、哪几个计划工作区变了、还多少 credit（纯函数）。
 * 体 `{"quota":true}` ⇒ 额度账；`{"sid": …}` ⇒ 那个会话；`{"rules":true}` ⇒ 那台的规则表；`{"plan": {workspace, rev, needs}}` ⇒ 那个工作区（同一个留后一格）；
 * 读不出 ⇒ 当整台重问。`seen`（又接上了）· `gap`（丢了几格）⇒ 整台重问。
 */
export function quotaChangedItems(items: readonly Item[]): { quota: boolean; sids: string[]; all: boolean; rules: boolean; plans: PlanMoved[]; frames: number } {
  const sids = new Set<string>();
  const plans = new Map<string, PlanMoved>();
  let quota = false;
  let all = false;
  let rules = false;
  let frames = 0;
  for (const it of items) {
    if (it.t === "frame") {
      frames += 1;
      let body: Body | null = null;
      try {
        body = JSON.parse(it.body) as Body;
      } catch {
        body = null;
      }
      const p = body?.plan;
      if (body?.quota === true) quota = true;
      else if (body?.rules === true) rules = true;
      else if (typeof body?.sid === "string" && body.sid !== "") sids.add(body.sid);
      else if (p && typeof p.workspace === "string" && p.workspace !== "" && typeof p.rev === "string")
        plans.set(p.workspace, { workspace: p.workspace, rev: p.rev, needs: typeof p.needs === "number" ? p.needs : null });
      else all = true;
    } else if (it.t === "seen" || it.t === "gap") {
      all = true;
    }
  }
  return { quota: quota || all, sids: [...sids], all, rules: rules || all, plans: [...plans.values()], frames };
}
