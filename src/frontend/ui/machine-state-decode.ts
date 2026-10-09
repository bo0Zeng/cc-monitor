/**
 * 每台机器状态成品（`backend_status` 的 `machine`）的严格收法 —— 只收、不画（主窗口订机器推送也用它，不牵出设置窗的画法）。
 * 哪一态、为什么、给哪几颗修法都是后端定的（`machine_state.rs`）。
 */
import type { MachineState } from "./generated/MachineState";
import type { MachineFix } from "./generated/MachineFix";
import type { MachineStateKind } from "./generated/MachineStateKind";
import type { VersionRelation } from "./generated/VersionRelation";
import { exactKeys, isObj } from "./ipc/decode";

export type { MachineState, MachineFix };

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
const FIXES: readonly MachineFix[] = ["retry", "conn_settings", "push_key", "compare_fingerprint", "update", "connect"];
const KEYS = ["detail", "fixes", "os", "reason", "seenHostKey", "stage", "state", "version", "versionRelation"];

const strOrNull = (v: unknown): v is string | null => v === null || (typeof v === "string" && v !== "");

/** 严格收：键集恰好那九格、每格取值在闭集里；收不下 ⇒ `null`（那一行照「没问到」画，不替后端编一态）。 */
export function decodeMachineState(raw: unknown): MachineState | null {
  if (!isObj(raw) || !exactKeys(raw, KEYS)) return null;
  const v = raw;
  if (!KINDS.includes(v.state as MachineStateKind)) return null;
  if (!strOrNull(v.reason) || !strOrNull(v.stage) || !strOrNull(v.version) || !strOrNull(v.os) || !strOrNull(v.seenHostKey) || !strOrNull(v.detail)) return null;
  if (v.versionRelation !== null && !RELATIONS.includes(v.versionRelation as VersionRelation)) return null;
  if (!Array.isArray(v.fixes) || !v.fixes.every((f) => FIXES.includes(f as MachineFix))) return null;
  return v as unknown as MachineState;
}

