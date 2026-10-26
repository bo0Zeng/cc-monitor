/**
 * **桌面旧的分组三格迁进各台**（一次性；5.0.3 之后的第一版带它，再下一版整份删，连同 `config.ts` 里登记给本文件的那两格）。
 *
 * 组从前住这台 `config.json`：组表 `tabCollections`（`{id, name, collapsed?}`）· 每个标签自己的组 `tabBar.groupOf.<sid>` ·
 * 固定表 `tabBar.pinned`（`{sid, origin, …}`）。如今组是后端的数据（`history/groups.rs`）：组本身住家那一台、组员与固定是会话注解。
 * 迁法：各台的标签都到齐以后（`TabManager` 那一刻），每组在本机建一份（本机就是家）、组员照标签所在那台写 `join`、固定照记录里那台写 `pinned`；
 * **每一条都写成才删那三格**，有一条没成 ⇒ 原样留着、下次起来再迁（建过的组再建一份，旧的那份没有组员、不出）。
 * 认不出机器的组员（旧组员表只记 sid：那条会话此刻没有标签、也没固定过）· 指着不存在的组的条目丢掉，回给调用方记一行。
 */
import type { Patch } from "./generated/Patch";
import type { Origin } from "./ipc/origin";

/** 盘上读到的旧三格（只取迁要的那几样；盘上可能是任何东西）。 */
export interface OldTabGroups {
  /** 盘上有没有那三格中的任何一格（都没有 ⇒ 不迁、不删）。 */
  present: boolean;
  groups: Array<{ id: string; name: string; collapsed: boolean }>;
  groupOf: Map<string, string>;
  pinned: Array<{ sid: string; origin: Origin }>;
}

function str(v: unknown): string {
  return typeof v === "string" ? v.trim() : "";
}

function obj(v: unknown): Record<string, unknown> | null {
  return v !== null && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
}

/** 从整份 `config.json` 里读旧三格：认不出的条目丢，不抛。 */
export function oldTabGroups(cfg: unknown): OldTabGroups {
  const top = obj(cfg) ?? {};
  const bar = obj(top.tabBar) ?? {};
  const present = "tabCollections" in top || "groupOf" in bar || "pinned" in bar;
  const groups: OldTabGroups["groups"] = [];
  const seen = new Set<string>();
  for (const x of Array.isArray(top.tabCollections) ? top.tabCollections : []) {
    const o = obj(x);
    const id = str(o?.id);
    const name = str(o?.name);
    if (!id || !name || seen.has(id)) continue;
    seen.add(id);
    groups.push({ id, name, collapsed: o?.collapsed === true });
  }
  const groupOf = new Map<string, string>();
  for (const [sid, v] of Object.entries(obj(bar.groupOf) ?? {})) {
    const gid = str(v);
    if (sid && gid) groupOf.set(sid, gid);
  }
  const pinned: OldTabGroups["pinned"] = [];
  for (const x of Array.isArray(bar.pinned) ? bar.pinned : []) {
    const o = obj(x);
    const sid = str(o?.sid);
    const origin = str(o?.origin);
    if (sid && origin && !pinned.some((p) => p.sid === sid)) pinned.push({ sid, origin });
  }
  return { present, groups, groupOf, pinned };
}

/** 迁要的三件事：本机建一组（回新 id）· 在会话那台改注解 · 删盘上那三格。 */
export interface MigrateIo {
  make(name: string): Promise<string>;
  annotate(origin: Origin, sid: string, patch: Patch): Promise<void>;
  dropOld(): Promise<void>;
}

/** 迁的结局：删没删那三格 · 丢了哪几条（sid）· 收着的组的新 id（界面照它记收着）。 */
export interface Migrated {
  done: boolean;
  lost: string[];
  collapsed: string[];
}

/**
 * 迁一次。`origins` ＝ 此刻栏里每条会话的标签在哪台（没有标签的那条再照旧固定表里记的那台；都没有 ⇒ 认不出、丢）。
 * 写的先后：先建组，再并发写各条注解；全成了才删那三格。
 */
export async function migrateTabGroups(
  old: OldTabGroups,
  origins: ReadonlyMap<string, Origin>,
  io: MigrateIo,
): Promise<Migrated> {
  if (!old.present) return { done: false, lost: [], collapsed: [] };
  const lost: string[] = [];
  const ids = new Map<string, string>();
  let ok = true;
  // 只建有组员能落地的那几组：一个组员都写不上的组建了也不出（各台都答了谁也没有那一组 ⇒ 已解散）。
  const landing = new Map<string, Array<{ origin: Origin; sid: string }>>();
  const where = new Map(old.pinned.map((p) => [p.sid, p.origin]));
  for (const [sid, o] of origins) where.set(sid, o);
  for (const [sid, gid] of old.groupOf) {
    const origin = where.get(sid);
    if (!old.groups.some((g) => g.id === gid) || origin === undefined) {
      lost.push(sid);
      continue;
    }
    const list = landing.get(gid) ?? [];
    list.push({ origin, sid });
    landing.set(gid, list);
  }
  for (const g of old.groups) {
    if (!landing.has(g.id)) continue;
    try {
      ids.set(g.id, await io.make(g.name));
    } catch (e) {
      console.warn(`[分组迁移] 组「${g.name}」没建成：`, e);
      ok = false;
    }
  }
  const writes: Array<Promise<void>> = [];
  const put = (origin: Origin, sid: string, patch: Patch): void => {
    writes.push(
      io.annotate(origin, sid, patch).catch((e: unknown) => {
        console.warn(`[分组迁移] ${sid} 没写成：`, e);
        ok = false;
      }),
    );
  };
  for (const [gid, members] of landing) {
    const id = ids.get(gid);
    if (id === undefined) continue;
    for (const m of members) put(m.origin, m.sid, { join: [id] });
  }
  for (const p of old.pinned) put(p.origin, p.sid, { pinned: true });
  await Promise.all(writes);
  if (ok) {
    try {
      await io.dropOld();
    } catch (e) {
      console.warn("[分组迁移] 旧那三格没删掉（下次再迁）：", e);
      ok = false;
    }
  }
  const collapsed = old.groups.filter((g) => g.collapsed && ids.has(g.id)).map((g) => ids.get(g.id) as string);
  return { done: ok, lost, collapsed };
}
