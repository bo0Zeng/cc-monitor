/**
 * 每台机器状态成品的严格收法 —— 只收、不画（主窗口订机器推送也用它，不牵出设置窗的画法）。
 * 哪一态、那一句、给哪几颗修法都是后端定的（`deploy::machine`；本机那一行是壳照同一批键出的）。
 */
import type { MachineState } from "./generated/MachineState";
import type { MachineFix } from "./generated/MachineFix";
import type { MachineFixButton } from "./generated/MachineFixButton";
import type { MachineStateKind } from "./generated/MachineStateKind";
import type { MachineStage } from "./generated/MachineStage";
import type { DialWhy } from "./generated/DialWhy";
import type { VersionRelation } from "./generated/VersionRelation";
import { exactKeys, isObj } from "./ipc/decode";

export type { MachineState, MachineFix, MachineFixButton };

const KINDS: readonly MachineStateKind[] = [
  "up",
  "connecting",
  "installing",
  "updating",
  "down",
  "host_key_changed",
  "needs_update",
  "newer",
  "disabled",
  "unsupported",
  "unknown",
];
const RELATIONS: readonly VersionRelation[] = ["same", "older", "newer", "incomparable"];
const FIXES: readonly MachineFix[] = ["retry", "conn_settings", "push_key", "compare_fingerprint", "update", "connect", "install"];
const STAGES: readonly MachineStage[] = ["deploy", "attach"];
const WHYS: readonly DialWhy[] = ["resolve", "unreachable", "timeout", "host_key", "auth", "password", "key_unreadable", "jump", "other"];
const KEYS = ["detail", "expectedHostKey", "fixes", "next", "os", "reason", "said", "seenHostKey", "stage", "state", "version", "versionRelation", "word"];

const strOrNull = (v: unknown): v is string | null => v === null || (typeof v === "string" && v !== "");
const oneOf = <T>(all: readonly T[], v: unknown): boolean => v === null || all.includes(v as T);
const fixButton = (f: unknown): boolean =>
  isObj(f) && exactKeys(f, ["fix", "label"]) && FIXES.includes(f.fix as MachineFix) && typeof f.label === "string" && f.label !== "";

/** 严格收：键集恰好那几格、每格取值在闭集里；收不下 ⇒ `null`（那一行照「没问到」画，不替后端编一态）。 */
export function decodeMachineState(raw: unknown): MachineState | null {
  if (!isObj(raw) || !exactKeys(raw, KEYS)) return null;
  const v = raw;
  if (!KINDS.includes(v.state as MachineStateKind)) return null;
  if (!oneOf(WHYS, v.reason) || !oneOf(STAGES, v.stage) || !oneOf(RELATIONS, v.versionRelation)) return null;
  // 库里记下的那一枚：库里没有同类型的 ⇒ 空串（后端照实说「没有」）。
  if (!strOrNull(v.version) || !strOrNull(v.os) || !strOrNull(v.seenHostKey) || !strOrNull(v.expectedHostKey) || !strOrNull(v.detail) || !strOrNull(v.next)) return null;
  if (typeof v.word !== "string" || typeof v.said !== "string") return null;
  if (!Array.isArray(v.fixes) || !v.fixes.every(fixButton)) return null;
  return v as unknown as MachineState;
}
