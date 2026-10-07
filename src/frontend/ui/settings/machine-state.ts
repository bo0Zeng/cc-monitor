/**
 * 每台机器的状态成品（`backend_status` 的 `machine`）：严格收，按码排成「点 · 名字旁那个词 · 问题行一句 · 修法按钮」。
 * 哪一态、为什么、给哪几颗修法都是后端定的（`machine_state.rs`）；这里只是码 → 文案表那一条。
 */
import type { MachineState } from "../generated/MachineState";
import type { MachineFix } from "../generated/MachineFix";
import type { MachineStateKind } from "../generated/MachineStateKind";
import type { VersionRelation } from "../generated/VersionRelation";
import type { MachineDotState } from "../kit/status-dot";
import { button } from "../kit/button";
import { icon } from "../kit/icon";
import { spinner } from "../kit/progress";
import { copyText } from "../copy-table";
import { exactKeys, isObj } from "../ipc/decode";

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
const KEYS = ["fixes", "os", "reason", "seenHostKey", "stage", "state", "version", "versionRelation"];

const strOrNull = (v: unknown): v is string | null => v === null || (typeof v === "string" && v !== "");

/** 严格收：键集恰好那八格、每格取值在闭集里；收不下 ⇒ `null`（那一行照「没问到」画，不替后端编一态）。 */
export function decodeMachineState(raw: unknown): MachineState | null {
  if (!isObj(raw) || !exactKeys(raw, KEYS)) return null;
  const v = raw;
  if (!KINDS.includes(v.state as MachineStateKind)) return null;
  if (!strOrNull(v.reason) || !strOrNull(v.stage) || !strOrNull(v.version) || !strOrNull(v.os) || !strOrNull(v.seenHostKey)) return null;
  if (v.versionRelation !== null && !RELATIONS.includes(v.versionRelation as VersionRelation)) return null;
  if (!Array.isArray(v.fixes) || !v.fixes.every((f) => FIXES.includes(f as MachineFix))) return null;
  return v as unknown as MachineState;
}

/** 一台怎么画。`problem` 为空 ＝ 不出问题行。 */
export interface MachineFace {
  dot: MachineDotState;
  /** 名字旁那个词（连着时不说）。 */
  word: string;
  problem: string;
  /** 问题行的样子：出错 · 要你动手 · 正在做（转圈；`bar` ＝ 装 / 更新那一段带进度条）。 */
  tone: "error" | "warn" | "busy";
  bar: boolean;
  fixes: MachineFix[];
  /** 页头「N 台离线 · M 台要更新」那两段各算不算它。 */
  offline: boolean;
  needsUpdate: boolean;
}

const DOT: Record<MachineStateKind, MachineDotState> = {
  up: "up",
  newer: "up",
  connecting: "unknown",
  installing: "unknown",
  updating: "unknown",
  unknown: "unknown",
  down: "failed",
  host_key_changed: "failed",
  unsupported: "failed",
  needs_update: "needs-you",
  disabled: "exited",
};

/** 掉线那一句（按原因码）。 */
function downSaid(reason: string | null, machine: string): string {
  switch (reason) {
    case "resolve":
      return copyText("machineState.down.resolve");
    case "unreachable":
      return copyText("machineState.down.unreachable");
    case "timeout":
      return copyText("machineState.down.timeout");
    case "auth":
      return copyText("machineState.down.auth");
    case "password":
      return copyText("machineState.down.password");
    case "key_unreadable":
      return copyText("machineState.down.keyUnreadable");
    case "jump":
      return copyText("machineState.down.jump");
    case "local":
      return copyText("machineState.down.local");
    default:
      return copyText("machinePage.problem.offline", { machine });
  }
}

export function machineFace(m: MachineState, machine: string): MachineFace {
  const busy = m.state === "connecting" || m.state === "installing" || m.state === "updating";
  const face = (word: string, problem: string): MachineFace => ({
    dot: DOT[m.state],
    word,
    problem,
    tone: busy ? "busy" : DOT[m.state] === "needs-you" ? "warn" : "error",
    bar: m.state === "installing" || m.state === "updating",
    fixes: m.fixes,
    offline: m.state === "down" || m.state === "host_key_changed" || m.state === "unsupported",
    needsUpdate: m.state === "needs_update",
  });
  switch (m.state) {
    case "up":
      return face(m.versionRelation === "incomparable" ? copyText("machineState.word.incomparable") : "", "");
    case "connecting":
      return face(
        copyText("machineState.word.connecting"),
        copyText("machineState.busy.connecting", {
          machine,
          stage: m.stage === "attach" ? copyText("machineState.stage.attach") : copyText("machineState.stage.deploy"),
        }),
      );
    case "installing":
      return face(copyText("machineState.word.installing"), copyText("machineState.busy.installing", { machine }));
    case "updating":
      return face(copyText("machineState.word.updating"), copyText("machineState.busy.updating", { machine }));
    case "unknown":
      return face("", "");
    case "down":
      return face(copyText("machinePage.state.down"), downSaid(m.reason, machine));
    case "host_key_changed":
      return face(copyText("machineState.word.hostKey"), copyText("machineState.problem.hostKey"));
    case "needs_update":
      return face(copyText("machineState.word.needsUpdate"), copyText("machineState.problem.needsUpdate", { machine }));
    case "newer":
      return face(copyText("machineState.word.newer"), copyText("machineState.problem.newer", { machine }));
    case "disabled":
      return face(copyText("machinePage.state.disabled"), copyText("machinePage.problem.disabled"));
    case "unsupported":
      return face(
        copyText("machineState.word.unsupported"),
        m.reason === "no_forwarding" ? copyText("machineState.problem.noForwarding") : copyText("machineState.problem.notUnix", { machine }),
      );
  }
}

/** 修法按钮上的字。 */
export function fixLabel(f: MachineFix): string {
  switch (f) {
    case "retry":
      return copyText("machineState.fix.retry");
    case "conn_settings":
      return copyText("machinePage.conn.toggle");
    case "push_key":
      return copyText("machineCard.build.pushKey");
    case "compare_fingerprint":
      return copyText("machineState.fix.compareFingerprint");
    case "update":
      return copyText("machineState.fix.update");
    case "connect":
      return copyText("machinePage.problem.connect");
  }
}

/**
 * 问题行（列表那一行与卡头同一个画法）：一句话 ＋ 修法按钮；正在连 / 装 / 更新时转圈（装 / 更新另有一条走动的进度条）。
 * `face.problem` 为空 ⇒ 收起。
 */
export function paintProblem(el: HTMLElement, face: MachineFace, onFix: (fix: MachineFix) => void): void {
  el.replaceChildren();
  el.hidden = face.problem === "";
  if (face.problem === "") return;
  el.dataset.severity = face.tone;
  const text = document.createElement("span");
  text.textContent = face.problem;
  el.append(face.tone === "busy" ? spinner() : icon(face.tone === "warn" ? "warning" : "error", "compact"), text);
  if (face.bar) {
    const bar = document.createElement("span");
    bar.className = "machine-problem-bar";
    el.appendChild(bar);
  }
  for (const fix of face.fixes) {
    el.appendChild(
      button({
        label: fixLabel(fix),
        size: "compact",
        kind: fix === "update" ? "primary" : "secondary",
        onClick: (ev) => {
          ev.stopPropagation();
          onFix(fix);
        },
      }),
    );
  }
}
