/**
 * 「那台某样东西变了」那一种流在界面这一侧的全部知识（叶子模块：只 import 通道的格类型、生成的主题类型与那一个解码）。
 * 那台后端发 `changed {topic, key?, rev?, body?}`（主题表只住后端 `wire/topic.rs`）；monitor 按主题交进 `subscribe(origin, "changed/<topic>")`，
 * 格体是帧去掉 `kind` / `topic` 的那一份；界面读格、按主题重问（带了 `body` 的可以不问）。
 * 体的形状按主题有类型（生成的 {@link ChangedFrame}，一个主题一臂）：壳按同一份类型解那一帧，体换了形 ⇒ 那一帧进不来、壳报一次，
 * 到这里的体都对得上那一臂 —— 读体只经 {@link pushedProducts}，读口不各自 `as`。
 * 订只经 `events.ts`（主窗口 `bindEvents` 的 `changed` · 别处 `subscribeTopics`）；本文件只读格。
 */
import type { Item } from "../../comms/inward/chan";
import { tryDecodeJson } from "./ipc/decode";
import type { Changed as ChangedFrame } from "./generated/Changed";
import type { Topic } from "./generated/Topic";
import { CHANGED_KIND } from "./generated/shell-names";

export type { Topic };

/** 那个主题那一臂的体（主题不带体 ⇒ `never`）。 */
export type TopicBody<T extends Topic> = Extract<ChangedFrame, { topic: T }> extends { body?: infer B } ? B : never;

/** 带体的那几个主题。 */
export type BodiedTopic = { [T in Topic]: [TopicBody<T>] extends [never] ? never : T }[Topic];

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
      const v = tryDecodeJson<unknown>(it.body);
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

/** 一个 pb 工作区变成了哪一份（`rev`）、那一刻要你看几条（`changed {plan}` 的 `body.needs`；没给 ⇒ `null`）。 */
export interface PlanMoved {
  workspace: string;
  rev: string;
  needs: number | null;
}

/** `changed/plan` 那一批 ⇒ 变了的工作区（没 `key` / `rev` 的格当整台重问，已在 `all` 里）。 */
export function planMoves(c: Changed): PlanMoved[] {
  const { got } = pushedProducts(c, "plan");
  return c.cells.flatMap((x) => {
    if (x.key === null || x.key === "" || x.rev === null) return [];
    return [{ workspace: x.key, rev: x.rev, needs: got.get(x.key)?.needs ?? null }];
  });
}

/**
 * 一批里帧带来的小成品（{@link TopicBody}：那个主题那一臂的体；现算的那几样就是重问那条命令的回包），带了的进 `got`（`key` ⇒ 成品）；
 * 没带的那几格的 `key` 进 `ask`（照旧重问）。`all` 照传（期间可能漏了 ⇒ 调用方整份重问）。
 * 不带 `key` 的主题（额度账 · 规则表）用 `key === null` 那一项：同一批留后一格。
 * `c` 须是 `changed/<topic>` 那一条流来的（壳已按那一臂的类型解过那一帧，这里是界面里唯一一处把格体看成那一臂的类型）。
 */
export function pushedProducts<T extends BodiedTopic>(c: Changed, _topic: T): { got: Map<string | null, TopicBody<T>>; ask: (string | null)[]; all: boolean } {
  const got = new Map<string | null, TopicBody<T>>();
  const ask: (string | null)[] = [];
  for (const cell of c.cells) {
    if (cell.body === undefined) ask.push(cell.key);
    else got.set(cell.key, cell.body as TopicBody<T>);
  }
  return { got, ask, all: c.all };
}
