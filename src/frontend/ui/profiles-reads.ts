/**
 * 设置窗「别名与配置文件」那一页的五问**走通道，那台后端出成品**（后端 `assets/aliases/page.rs`）：
 *
 * | 做什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 读回配置文件整份（每段自己写的 · 能不能用 · 链接 / 终端函数 · 表单回填 · 空态预览 · 迁移说明） | `profiles-read` | [`ProfilesBook`] |
 * | 一段合下来的合并表与「等于」那一行（可按未存的表单、假设在某个目录敲） | `profiles-resolve` | [`Resolved`] |
 * | 这几处改动会连带哪几段（改前改后） | `profiles-impact` | [`Affected`]`[]` |
 * | 「基于」下拉能选的几段 | `profiles-bases` | [`Base`]`[]` |
 * | 按条目改配置文件（带读回时的指纹；盘上被别处改过 ⇒ 对端拒、码 `stale`） | `profiles-write` | [`WriteDone`] |
 *
 * 合并 · 校验 · 标签 · 值怎么说全在那台后端。入参 · 回包都是后端类型生成的（`generated/`），
 * 回包形状由握手时的形状指纹管，这里不逐格判。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Machine } from "./ipc/origin";
import type { ProfileForm as ProfileFormWire } from "./generated/ProfileForm";
import type { ProfileSlotId } from "./generated/ProfileSlotId";
import type { ProfileRow as ProfileRowWire } from "./generated/ProfileRow";
import type { ProfilesBook as ProfilesBookWire } from "./generated/ProfilesBook";
import type { ProfileResolved } from "./generated/ProfileResolved";
import type { ProfileAffected } from "./generated/ProfileAffected";
import type { ProfileBase } from "./generated/ProfileBase";
import type { ProfileOp as ProfileOpWire } from "./generated/ProfileOp";
import type { ProfilesWriteDone } from "./generated/ProfilesWriteDone";

/** 一段的表单：`null` / `false` ＝ 继承（没写进这一段）。 */
export type ProfileForm = ProfileFormWire;

/** 表单里可以写的几格（`detach` 是开关，其余是值）。 */
export type SlotId = ProfileSlotId;

/** 树里的一段（`said` 是后端按表单同一套词写好的那一行；`accountShape` 是「账号那一形」，其余 `null`）。 */
export type ProfileRow = ProfileRowWire;
/** `profiles-read` 的成品（`editedAt` 后端写好；`tmux` 是那台后端判的，查不动 ⇒ `null`）。 */
export type ProfilesBook = ProfilesBookWire;
/** `profiles-resolve` 的成品：`line` 整条（「复制整条」交它）· `lineShort` 那一格的短形（照抄）。 */
export type Resolved = ProfileResolved;
export type Affected = ProfileAffected;
export type Base = ProfileBase;
export type ProfileOp = ProfileOpWire;
export type WriteDone = ProfilesWriteDone;

/** 五问的期限：读一份小文件、合并几层、写一份（秒级）。给 30 秒。 */
const PROFILES_BUDGET_MS = 30_000;

// 下面几问失败一律原样抛通道那个错（那一句 · 码 · 详情都在）。

export async function readProfiles(origin: Machine): Promise<ProfilesBook> {
  return await callTyped(origin, "profiles-read", {}, budgetWithin(PROFILES_BUDGET_MS));
}

export async function resolveProfile(origin: Machine, name: string, edit: ProfileForm | null, at: string | null): Promise<Resolved> {
  return await callTyped(origin, "profiles-resolve", { name, edit, at }, budgetWithin(PROFILES_BUDGET_MS));
}

export async function profileImpact(origin: Machine, changes: ProfileOp[]): Promise<Affected[]> {
  const { affected } = await callTyped(origin, "profiles-impact", { changes }, budgetWithin(PROFILES_BUDGET_MS));
  return affected;
}

export async function profileBases(origin: Machine, name: string): Promise<Base[]> {
  const { bases } = await callTyped(origin, "profiles-bases", { name }, budgetWithin(PROFILES_BUDGET_MS));
  return bases;
}

/** 按条目改：结局交统一写口（`ipc/write.ts`）判 —— 盘上被别处改过是对端拒、码 `stale`（一个字节没写）。 */
export function writeProfiles(origin: Machine, changes: ProfileOp[], ifRev: string | null): Promise<WriteDone> {
  return callTyped(origin, "profiles-write", { changes, ifRev }, budgetWithin(PROFILES_BUDGET_MS));
}
