/**
 * 设置窗「别名与配置文件」那一页的五问**走通道，那台后端出成品**（后端 `assets/aliases/page.rs`）：
 *
 * | 做什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 读回配置文件整份（每段自己写的 · 能不能用 · 链接 / 终端函数 · 表单回填 · 空态预览 · 迁移说明） | `profiles-read` | [`ProfilesBook`] |
 * | 一段合下来的合并表与「等于」那一行（可按未存的表单、假设在某个目录敲） | `profiles-resolve` | [`Resolved`] |
 * | 这几处改动会连带哪几段（改前改后） | `profiles-impact` | [`Affected`]`[]` |
 * | 「基于」下拉能选的几段 | `profiles-bases` | [`Base`]`[]` |
 * | 按条目改配置文件（带读回时的指纹；盘上被别处改过 ⇒ [`ProfilesStale`]） | `profiles-write` | [`WriteDone`] |
 *
 * 合并 · 校验 · 标签 · 值怎么说全在那台后端。入参 · 回包都是后端类型生成的（`generated/`），
 * 回包形状由握手时的形状指纹管，这里不逐格判。
 */
import { budgetWithin, callTyped, refusalIn } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { ProfileForm as ProfileFormWire } from "./generated/ProfileForm";
import type { ProfileSlotId } from "./generated/ProfileSlotId";
import type { ProfileRow as ProfileRowWire } from "./generated/ProfileRow";
import type { ProfilesBook as ProfilesBookWire } from "./generated/ProfilesBook";
import type { ProfileResolved } from "./generated/ProfileResolved";
import type { ProfileAffected } from "./generated/ProfileAffected";
import type { ProfileBase } from "./generated/ProfileBase";
import type { ProfileOp as ProfileOpWire } from "./generated/ProfileOp";
import type { ProfilesWriteDone } from "./generated/ProfilesWriteDone";
import { saidOf } from "./kit/detail";

/** 一段的表单：`null` / `false` ＝ 继承（没写进这一段）。 */
export type ProfileForm = ProfileFormWire;

/** 表单里可以写的几格（`detach` / `busRegister` 是开关，其余是值）。 */
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

/** 存的那一刻盘上那份不是读回时那一份（被别处改过）⇒ 一个字节没写；界面重读、表单留着。 */
export class ProfilesStale extends Error {}

/** 五问的期限：读一份小文件、合并几层、写一份（秒级）。给 30 秒。 */
const PROFILES_BUDGET_MS = 30_000;

const said = (e: unknown): Error => new Error(saidOf(e));

export async function readProfiles(origin: Origin): Promise<ProfilesBook> {
  try {
    return await callTyped(origin, "profiles-read", {}, budgetWithin(PROFILES_BUDGET_MS));
  } catch (e) {
    throw said(e);
  }
}

export async function resolveProfile(origin: Origin, name: string, edit: ProfileForm | null, at: string | null): Promise<Resolved> {
  try {
    return await callTyped(origin, "profiles-resolve", { name, edit, at }, budgetWithin(PROFILES_BUDGET_MS));
  } catch (e) {
    throw said(e);
  }
}

export async function profileImpact(origin: Origin, changes: ProfileOp[]): Promise<Affected[]> {
  try {
    const { affected } = await callTyped(origin, "profiles-impact", { changes }, budgetWithin(PROFILES_BUDGET_MS));
    return affected;
  } catch (e) {
    throw said(e);
  }
}

export async function profileBases(origin: Origin, name: string): Promise<Base[]> {
  try {
    const { bases } = await callTyped(origin, "profiles-bases", { name }, budgetWithin(PROFILES_BUDGET_MS));
    return bases;
  } catch (e) {
    throw said(e);
  }
}

/** 按条目改：盘上被别处改过 ⇒ [`ProfilesStale`]（一个字节没写）。 */
export async function writeProfiles(origin: Origin, changes: ProfileOp[], fingerprint: string | null): Promise<WriteDone> {
  try {
    return await callTyped(origin, "profiles-write", { changes, fingerprint }, budgetWithin(PROFILES_BUDGET_MS));
  } catch (e) {
    if (refusalIn(e)?.code === "stale") {
      throw new ProfilesStale(saidOf(e));
    }
    throw said(e);
  }
}
