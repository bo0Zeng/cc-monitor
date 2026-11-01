/**
 * 本机后端机器表那三条（`machines-read` / `-try` / `-set`）的界面测试替身：只回录好的回包。
 *
 * 机器表有自己的一块假盘（[`fakeMachines.disk`]：一张表 · 没有那份 · 读不懂的原文），不在 `config.json` 里。
 * 每一问的输入（命令 · 盘上那一份 · 补丁）到金样 `tests/__fixtures__/machines-replies.golden.json` 里找同一组，交它录好的
 * `reply`，或照 `refused`（码 · 那一句 · data）拒 —— 那一份由后端判据（`tests/backend/control/machines_replay_tests.rs`）跑真的
 * 读口 · 试算口 · 写口现打：补丁怎么落 · 名字怎么算 · 机器表那一道判哪一处 · 拒的那一句都是后端的，这里一处都不判。
 * `machines-set` 成了 ⇒ 盘换成回包里读回的那一张。金样里没有这一组 ⇒ 抛（这一组同时记进本树
 * `.build/golden-missing/machines-replies.jsonl`）：跑那条后端判据，它把这一组收进现打的那份，对过后 bless 拷回。
 */
import { appendFileSync, mkdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { vi } from "vitest";
import { ChanError } from "../../../src/comms/inward/chan";
import type { Host } from "../../../src/frontend/ui/generated/Host";
import type { MachineEdit } from "../../../src/frontend/ui/generated/MachineEdit";
import { REPO_ROOT } from "../../test-support/repo-root";

/** 盘上那份机器表：那几台 · 读不懂的原文；`undefined` ⇒ 没有那份。 */
export type MachinesDisk = readonly Partial<Host>[] | { text: string } | undefined;

/** 拒绝体（后端 `{code, said, data?}` 那一形）。 */
export interface Refusal {
  code: string;
  said: string;
  data?: unknown;
}

/** 一问的结局：回包 · 拒。 */
export type MachinesAnswer = { reply: unknown } | { refused: Refusal };

export const fakeMachines = {
  /** 盘上那份（`mockReturnValue` 摆；`vi.resetAllMocks()` 之后 ⇒ 没有那份）。`machines-set` 成了由替身换成读回的那一张。 */
  disk: vi.fn<() => MachinesDisk>(),
  /**
   * `machines-try` / `-set` 每次交出去的补丁（按次序）。测试让它回一份拒绝体 ⇒ 这一问照它拒、不查金样
   * （演写口没写成那一类盘上摆不出来的结局，如盘满）。
   */
  sent: vi.fn<(cmd: string, edits: MachineEdit[]) => Refusal | undefined>(),
  /** `machines-set` 成了之后盘上那一张（按次序）。 */
  written: vi.fn<(hosts: Host[]) => void>(),
};

interface Case {
  cmd: string;
  disk: unknown;
  edits?: unknown;
  reply?: unknown;
  refused?: Refusal;
}

const GOLDEN = "tests/__fixtures__/machines-replies.golden.json";
const MISSING = ".build/golden-missing/machines-replies.jsonl";

/** 键排好再写（金样拷回时键序可能变，比的是内容）。 */
function canon(v: unknown): string {
  return JSON.stringify(v ?? null, (_k, x: unknown) =>
    x && typeof x === "object" && !Array.isArray(x) ? Object.fromEntries(Object.entries(x as Record<string, unknown>).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) : x,
  );
}

let cases: Case[] | null = null;

/** 这一组输入录好的结局（纯查表）；没录 ⇒ 抛。 */
export function recordedMachines(cmd: string, disk: MachinesDisk, edits?: readonly MachineEdit[]): MachinesAnswer {
  cases ??= (JSON.parse(readFileSync(path.join(REPO_ROOT, GOLDEN), "utf8")) as { cases: Case[] }).cases;
  const input = { cmd, disk: disk ?? null, edits: edits ?? null };
  const want = canon(input);
  const hit = cases.find((c) => canon({ cmd: c.cmd, disk: c.disk, edits: c.edits ?? null }) === want);
  if (hit?.refused) return { refused: hit.refused };
  if (hit && hit.reply !== undefined) return { reply: hit.reply };
  const line = JSON.stringify(input);
  try {
    mkdirSync(path.join(REPO_ROOT, path.dirname(MISSING)), { recursive: true });
    appendFileSync(path.join(REPO_ROOT, MISSING), line + "\n");
  } catch {
    // 记不下只少一份方便，照样抛。
  }
  throw new Error(`机器表替身：${GOLDEN} 里没录这一组输入；跑 machines_replay_tests 现打后 bless：${line}`);
}

/** 照此刻那块假盘答一问（记下交出去的补丁；`machines-set` 成了 ⇒ 盘换成读回的那一张）。 */
export function answerMachines(cmd: string, req: unknown): MachinesAnswer {
  const edits = cmd === "machines-read" ? undefined : [...((req as { edits?: MachineEdit[] } | null)?.edits ?? [])];
  const staged = edits ? fakeMachines.sent(cmd, edits) : undefined;
  if (staged) return { refused: staged };
  const got = recordedMachines(cmd, fakeMachines.disk(), edits);
  if (cmd === "machines-set" && "reply" in got) {
    const m = (got.reply as { machines: { state: string; hosts: { host: Host }[] } }).machines;
    const hosts = m.hosts.map((r) => r.host);
    fakeMachines.disk.mockReturnValue(hosts);
    fakeMachines.written(hosts);
  }
  return got;
}

const enc = new TextEncoder();

/** 壳那一层（`commands.chan_call`）的替身用：成 ⇒ 回包字节；拒 ⇒ 照 monitor 交回来的那一形 reject（`Refused` ＋ 拒绝体 ＋ 那一句）。 */
export function machinesChanCall(op: string, req: unknown): Promise<ArrayBuffer> {
  let got: MachinesAnswer;
  try {
    got = answerMachines(op, req);
  } catch (e) {
    return Promise.reject(e);
  }
  if ("refused" in got) {
    return Promise.reject({ err: "Refused", body: [...enc.encode(JSON.stringify(got.refused))], said: got.refused.said, detail: "" });
  }
  const u = enc.encode(JSON.stringify(got.reply));
  return Promise.resolve(u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength));
}

/** 「`vi.mock(".../ipc/chan-caller")`」用：本机后端机器表那三条照金样答（拒 ⇒ 对端拒了的 `ChanError`），别的命令照真的。 */
export async function mockedChanCaller(importOriginal: <T>() => Promise<T>): Promise<Record<string, unknown>> {
  const actual = await importOriginal<Record<string, unknown>>();
  const real = actual.callTyped as (...a: unknown[]) => Promise<unknown>;
  const callTyped = async (origin: unknown, cmd: unknown, req: unknown, ...rest: unknown[]): Promise<unknown> => {
    if (!String(cmd).startsWith("machines-")) return real(origin, cmd, req, ...rest);
    const got = answerMachines(String(cmd), req);
    if ("refused" in got) throw new ChanError({ layer: "peer", why: "refused", body: enc.encode(JSON.stringify(got.refused)) }, got.refused.said, "");
    return got.reply;
  };
  return { ...actual, callTyped };
}
