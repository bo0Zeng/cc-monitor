/**
 * 「那台某样东西变了」那一种流在界面这一侧的全部知识（叶子模块：只 import 通道的格类型与生成的主题类型）。
 * 那台后端发 `changed {topic, key?, rev?, body?}`（主题表只住后端 `stream/topic.rs`）；monitor 按主题交进 `subscribe(origin, "changed/<topic>")`，
 * 格体是帧去掉 `kind` / `topic` 的那一份；界面读格、按主题重问（带了 `body` 的可以不问）。
 */
import type { Item } from "../../comms/inward/chan";
import type { Topic } from "./generated/Topic";

export type { Topic };

/** 与 Rust `event_replay.rs::CHANGED_KIND` 同一个串；流名是 `changed/<topic>`。 */
export const CHANGED_KIND = "changed";
/** 每条订阅一开始给多少格 credit（每一次变化至多一格），用完一格还一格。 */
export const CHANGED_WINDOW = 32;

/** 一个主题的流名。 */
export function changedStream(topic: Topic): string {
  return `${CHANGED_KIND}/${topic}`;
}

/** 一格：哪一个（`key`）· 变成了哪一版（`rev`）· 小成品（`body`，没带 ⇒ `undefined`）。 */
export interface ChangedCell {
  key: string | null;
  rev: string | null;
  body: unknown;
}

/** 一批格的意思：变了的那几格（同一个 `key` 留后一格）· 期间可能漏了（`all`：又接上 · 丢了几格 · 格读不懂 ⇒ 整台重问）。 */
export interface Changed {
  cells: ChangedCell[];
  all: boolean;
}

/** 一批格 ⇒ {@link Changed} ＋ 占了几格 credit（纯函数）。`unseen` / `closed` 不算变（断着问不到；连上时会有 `seen`）。 */
export function changedItems(items: readonly Item[]): Changed & { frames: number } {
  const byKey = new Map<string | null, ChangedCell>();
  let all = false;
  let frames = 0;
  for (const it of items) {
    if (it.t === "frame") {
      frames += 1;
      let v: unknown = null;
      try {
        v = JSON.parse(it.body);
      } catch {
        v = null;
      }
      const o = v !== null && typeof v === "object" && !Array.isArray(v) ? (v as { key?: unknown; rev?: unknown; body?: unknown }) : null;
      const okKey = o !== null && (o.key === undefined || typeof o.key === "string");
      const okRev = o !== null && (o.rev === undefined || typeof o.rev === "string");
      if (o === null || !okKey || !okRev) {
        all = true;
        continue;
      }
      const key = typeof o.key === "string" ? o.key : null;
      byKey.delete(key);
      byKey.set(key, { key, rev: typeof o.rev === "string" ? o.rev : null, body: o.body });
    } else if (it.t === "seen" || it.t === "gap") {
      all = true;
    }
  }
  return { cells: [...byKey.values()], all, frames };
}

/** 变了的那几格里带 `key` 的（会话 id · 工作区根）。 */
export function changedKeys(c: Changed): string[] {
  return c.cells.flatMap((x) => (x.key === null ? [] : [x.key]));
}

/** 一个 pb 工作区变成了哪一份（`rev`）、那一刻要你看几条（`changed {plan}` 的 `body.needs`；没给 ⇒ `null`）。 */
export interface PlanMoved {
  workspace: string;
  rev: string;
  needs: number | null;
}

/** `changed/plan` 那一批 ⇒ 变了的工作区（没 `key` / `rev` 的格当整台重问，已在 `all` 里）。 */
export function planMoves(c: Changed): PlanMoved[] {
  return c.cells.flatMap((x) => {
    if (x.key === null || x.key === "" || x.rev === null) return [];
    const b = x.body as { needs?: unknown } | undefined;
    return [{ workspace: x.key, rev: x.rev, needs: typeof b?.needs === "number" ? b.needs : null }];
  });
}
