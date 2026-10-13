/**
 * 前端测试用的**假 config.json**：`load_config` / `patch_config` 两条命令的内存替身（`set` / `remove`）。
 *
 * 补丁语义与 Rust 写口（`src/frontend/shell/src/config.rs::patch_config_at`）同一份金样
 * `tests/__fixtures__/config-patch.golden.json` 对拍（`tests/frontend/ui/config-patch-fake.vitest.ts` 跑这边，
 * `tests/frontend/shell/config_tests.rs::the_golden_cases_hold` 跑那边）⇒ 用它的测试看到的「盘上终态」就是 Rust 会落的那份。
 *
 * 机器表住本机后端（`machines.json`）：[`mockedChanCaller`] 是 `machines-read` / `-try` / `-set` 的替身，表摆在同一块假盘的
 * `remote.hosts` 里（旧测试照旧用 `loadConfig` 摆机器）；按键认那一台的语义同后端 `control/machines.rs`（判据 `machines_tests.rs`）。
 */
import { vi } from "vitest";
import { copyText } from "../../../src/frontend/ui/copy-table";

export type Edit =
  | { op: "set"; path: string[]; value: unknown }
  | { op: "remove"; path: string[] }
  | { op: "setin"; path: string[]; where: { fields: string[]; equals: string }[]; field: string; value: unknown; ifEmpty: boolean }
  | { op: "insertin"; path: string[]; where: { fields: string[]; equals: string }[]; value: unknown }
  | { op: "removein"; path: string[]; where: { fields: string[]; equals: string }[] };

export class ConfigRefused extends Error {
  constructor(readonly kind: "bad_edit" | "unreadable" | "element_gone" | "element_exists") {
    super(`config patch refused: ${kind}`);
  }
}

/** 把一批补丁应用到盘上原文，回新原文。拒 ⇒ 抛 [`ConfigRefused`]（调用方的原文不变）。 */
export function applyConfigEdits(text: string, edits: readonly Edit[]): string {
  if (edits.some((e) => e.path.length === 0)) throw new ConfigRefused("bad_edit");
  if (edits.some((e) => "where" in e && e.where.length === 0)) throw new ConfigRefused("bad_edit");
  let root: unknown;
  try {
    root = JSON.parse(text);
  } catch {
    throw new ConfigRefused("unreadable");
  }
  if (!root || typeof root !== "object" || Array.isArray(root)) throw new ConfigRefused("unreadable");
  const obj = root as Record<string, unknown>;
  const isObj = (x: unknown): x is Record<string, unknown> => !!x && typeof x === "object" && !Array.isArray(x);
  const keyOf = (o: Record<string, unknown>, fields: string[]) =>
    fields.map((f) => o[f]).find((v): v is string => typeof v === "string" && v !== "");
  const matches = (x: unknown, where: { fields: string[]; equals: string }[]) =>
    isObj(x) && where.every((w) => keyOf(x, w.fields) === w.equals);
  let exists = false;
  for (const e of edits) {
    if (e.op === "insertin") {
      // 同 Rust `InsertIn`：路上缺的段补成 `{}`、数组缺就建；已有满足 where 的 ⇒ 整批拒（element_exists）。
      let cur = obj;
      for (const seg of e.path.slice(0, -1)) {
        if (!isObj(cur[seg])) cur[seg] = {};
        cur = cur[seg] as Record<string, unknown>;
      }
      const last = e.path[e.path.length - 1]!;
      if (cur[last] === undefined) cur[last] = [];
      const arr = cur[last];
      if (!Array.isArray(arr)) throw new ConfigRefused("element_gone");
      if (arr.some((x) => matches(x, e.where))) exists = true;
      else arr.push(JSON.parse(JSON.stringify(e.value)));
      continue;
    }
    if (e.op === "removein") {
      // 同 Rust `RemoveIn`：认恰好一个才删，否则整批拒（element_gone）。
      let cur: unknown = obj;
      for (const seg of e.path) cur = isObj(cur) ? cur[seg] : undefined;
      if (!Array.isArray(cur)) throw new ConfigRefused("element_gone");
      const hits = cur.flatMap((x, i) => (matches(x, e.where) ? [i] : []));
      if (hits.length !== 1) throw new ConfigRefused("element_gone");
      cur.splice(hits[0]!, 1);
      continue;
    }
    if (e.op === "setin") {
      // 同 Rust `apply_edit` 的 `SetIn`：认恰好一个元素（每条 where：fields 按序第一个非空字符串 == equals），只改一格。
      let cur: unknown = obj;
      for (const seg of e.path) cur = cur && typeof cur === "object" && !Array.isArray(cur) ? (cur as Record<string, unknown>)[seg] : undefined;
      if (!Array.isArray(cur)) throw new ConfigRefused("element_gone");
      const keyOf = (o: Record<string, unknown>, fields: string[]) =>
        fields.map((f) => o[f]).find((v): v is string => typeof v === "string" && v !== "");
      const hits = cur.filter(
        (x): x is Record<string, unknown> =>
          !!x && typeof x === "object" && !Array.isArray(x) && e.where.every((w) => keyOf(x as Record<string, unknown>, w.fields) === w.equals),
      );
      if (hits.length !== 1) throw new ConfigRefused("element_gone");
      const v = hits[0]![e.field];
      const taken = v !== undefined && v !== null && (typeof v !== "string" || v.trim() !== "");
      if (!(e.ifEmpty && taken)) hits[0]![e.field] = JSON.parse(JSON.stringify(e.value));
      continue;
    }
    let cur = obj;
    const parents = e.path.slice(0, -1);
    const last = e.path[e.path.length - 1]!;
    let gone = false;
    for (const seg of parents) {
      const next = cur[seg];
      if (!next || typeof next !== "object" || Array.isArray(next)) {
        if (e.op === "remove") {
          gone = true;
          break;
        }
        cur[seg] = {};
      }
      cur = cur[seg] as Record<string, unknown>;
    }
    if (gone) continue;
    if (e.op === "set") cur[last] = JSON.parse(JSON.stringify(e.value));
    else delete cur[last];
  }
  // 同 Rust：认不出（上面当场抛）先于「已存在」判。
  if (exists) throw new ConfigRefused("element_exists");
  return JSON.stringify(obj);
}

/** 替身盖的「指纹记于哪一天」（壳 `config.rs::stamp_pinned_day` 盖写的那一天；替身用一个定死的日子）。 */
export const FAKE_PINNED_DAY = "2026-03-04";

/**
 * 同壳的补丁口：写进了指纹（改那一格 · 新增一台带指纹）⇒ 那台的 `hostKeyPinnedAt` 跟着盖上（指纹清空 ⇒ 清空），回话里说盖成了什么。
 * 回 [补上盖日子那几条之后的补丁, 回话]。
 */
export function withPinnedDays(edits: readonly Edit[]): [Edit[], { pinned: { origin: string; day: string }[] }] {
  const out: Edit[] = [];
  const pinned: { origin: string; day: string }[] = [];
  const hosts = (p: string[]) => p.length === 2 && p[0] === "remote" && p[1] === "hosts";
  for (const e of edits) {
    out.push(e);
    const fp = e.op === "setin" && hosts(e.path) && e.field === "hostKeyFingerprint" ? e.value : e.op === "insertin" && hosts(e.path) ? (e.value as Record<string, unknown>).hostKeyFingerprint : undefined;
    if (fp === undefined || !("where" in e)) continue;
    const given = typeof fp === "string" && fp.trim() !== "";
    if (!given && e.op === "insertin") continue;
    const day = given ? FAKE_PINNED_DAY : "";
    out.push({ op: "setin", path: e.path, where: e.where, field: "hostKeyPinnedAt", value: day, ifEmpty: false });
    pinned.push({ origin: e.where[0]!.equals, day });
  }
  return [out, { pinned }];
}

/**
 * 一块假盘 ＋ 它的两条命令。`patches` 记下每一次 `patch_config` 收到的补丁（判「写者只交自己的键」用）。
 * `saved` 是每次写完之后的整份（给「写完盘上是什么」那一类旧断言用）。
 */
export function makeFakeConfigDisk(initial: Record<string, unknown> = {}) {
  const disk = {
    text: JSON.stringify(initial),
    patches: [] as Edit[][],
    saved: [] as Record<string, unknown>[],
    set(cfg: Record<string, unknown>): void {
      disk.text = JSON.stringify(cfg);
      disk.patches = [];
      disk.saved = [];
    },
    get cfg(): Record<string, unknown> {
      return JSON.parse(disk.text) as Record<string, unknown>;
    },
    load_config: async (): Promise<Record<string, unknown>> => JSON.parse(disk.text),
    patch_config: async ({ edits }: { edits: Edit[] }): Promise<{ pinned: { origin: string; day: string }[] }> => {
      disk.patches.push(edits);
      const [all, reply] = withPinnedDays(edits);
      disk.text = applyConfigEdits(disk.text, all);
      disk.saved.push(JSON.parse(disk.text) as Record<string, unknown>);
      return reply;
    },
  };
  return disk;
}

// ───────────────────────── 整个 `src/config` 模块的替身 ─────────────────────────
//
// 给「`vi.mock("../../../src/frontend/ui/config")` 之后看写完盘上是什么」那一族旧测试用：
//   `vi.mock("../../../src/frontend/ui/config", async (orig) => (await import("./config-patch-fake")).mockedConfigModule(orig));`
// `loadConfig` 是 `fakeCfg.load`（照旧 `mockResolvedValue` 摆盘上那份）；每次写经 [`applyConfigEdits`]
// 算出**写完之后的整份**交给 `fakeCfg.saved`（`saved.mock.calls[i][0]`），并成为之后 `load` 读到的那份。`setAt` / `removeAt` / 登记表用真的。
// ⚠ `patchConfig` 刻意**不是** `vi.fn`：`vi.resetAllMocks()` 会把 `vi.fn` 的实现一起清掉。
export const fakeCfg = {
  load: vi.fn(),
  saved: vi.fn(),
  /** 每次写收到的补丁（按次序）。 */
  patches: vi.fn(),
};

export async function mockedConfigModule(
  importOriginal: <T>() => Promise<T>,
): Promise<Record<string, unknown>> {
  const actual = await importOriginal<Record<string, unknown>>();
  const patchConfig = fakePatch;
  return { ...actual, loadConfig: fakeCfg.load, patchConfig };
}

/** 假盘的写口（`patchConfig` 的替身本体；机器表那一条也落在这块盘上）。 */
async function fakePatch(edits: readonly Edit[]): Promise<{ pinned: { origin: string; day: string }[] }> {
  {
    fakeCfg.patches([...edits]);
    const base = ((await fakeCfg.load()) ?? {}) as Record<string, unknown>;
    const [all, reply] = withPinnedDays(edits);
    const doc = JSON.parse(applyConfigEdits(JSON.stringify(base), all)) as Record<string, unknown>;
    // 写完之后再读，读到的是写完的那份（真盘就是这样）。从前旧测试靠的是生产代码**原地改了**
    // `loadConfig` 摆出来的那个对象 —— 那是替身的巧合，不是盘的语义。
    fakeCfg.load.mockResolvedValue(JSON.parse(JSON.stringify(doc)));
    fakeCfg.saved(doc);
    return reply;
  }
}

/** 后端 `MachineEdit` 那一形（没有 `path`）⇒ 替身的补丁（落在假盘的 `remote.hosts`）。 */
type MachineEditLike =
  | { op: "setin"; where: { fields: string[]; equals: string }[]; field: string; value: unknown; ifEmpty?: boolean }
  | { op: "insertin"; where: { fields: string[]; equals: string }[]; value: unknown }
  | { op: "removein"; where: { fields: string[]; equals: string }[] };

function onTable(edits: readonly MachineEditLike[]): Edit[] {
  const path = ["remote", "hosts"];
  return edits.map((e) => (e.op === "setin" ? { ...e, path, ifEmpty: e.ifEmpty ?? false } : { ...e, path })) as Edit[];
}

/** 本机后端机器表那三条命令的替身本体：回那一答（拒 ⇒ 抛）；不是这三条 ⇒ `undefined`。 */
export async function fakeMachinesCall(cmd: unknown, req: unknown): Promise<unknown> {
  const edits = ((req as { edits?: MachineEditLike[] } | null)?.edits ?? []) as MachineEditLike[];
  const read = async () => {
    const cfg = ((await fakeCfg.load()) ?? {}) as { remote?: { hosts?: unknown } };
    const hosts = cfg.remote?.hosts;
    if (cfg.remote === undefined) return { state: "absent", reason: null, detail: null, path: null, hosts: [] };
    return Array.isArray(hosts)
      ? { state: "ok", reason: null, detail: null, path: null, hosts }
      : { state: "unreadable", reason: "读不懂", detail: null, path: null, hosts: [] };
  };
  if (cmd === "machines-read") return read();
  if (cmd === "machines-try") return { fault: await fakeMachineTableTry(onTable(edits)) };
  if (cmd === "machines-set") {
    const all = onTable(edits);
    const identity = edits.some((e) => e.op === "insertin" || (e.op === "setin" && ["label", "host", "port", "user"].includes(e.field)));
    const fault = identity ? await fakeMachineTableTry(all) : null;
    if (fault) throw new Error(fault.said);
    const r = await fakePatch(all);
    return { machines: await read(), pinned: r.pinned, kept: 0 };
  }
  return undefined;
}

/** 「`vi.mock(".../ipc/chan-caller")`」用：本机后端机器表那三条命令走假盘，别的命令照真的。 */
export async function mockedChanCaller(
  importOriginal: <T>() => Promise<T>,
): Promise<Record<string, unknown>> {
  const actual = await importOriginal<Record<string, unknown>>();
  const real = actual.callTyped as (...a: unknown[]) => Promise<unknown>;
  const callTyped = async (origin: unknown, cmd: unknown, req: unknown, ...rest: unknown[]): Promise<unknown> =>
    String(cmd).startsWith("machines-") ? fakeMachinesCall(cmd, req) : real(origin, cmd, req, ...rest);
  return { ...actual, callTyped };
}

/**
 * 机器表那一道的替身（后端 `control/machines.rs::check_table` 的合成版，给「界面按回来的码说那一句」那一族用）：
 * 把补丁应用到 `fakeCfg.load()` 那一份上，按名字不重 · 端口 1–65535 · 地址 / 用户非空判第一处。
 * 规则本身的判据在后端那一侧（`tests/backend/control/machines_tests.rs`）。
 */
/** 后端写好的那一句（`control/machines.rs::MachineFault::new`，键同；这里只为替身回包带上那一格）。 */
const faultOf = (code: string, name: string): { code: string; name: string; said: string } => ({
  code,
  name,
  said:
    code === "name_taken" ? copyText("addMachine.err.taken", { name })
    : code === "port" ? copyText("addMachine.err.port")
    : code === "no_host" ? copyText("addMachine.err.host")
    : copyText("addMachine.err.user"),
});

export async function fakeMachineTableTry(edits: readonly Edit[]): Promise<{ code: string; name: string; said: string } | null> {
  const base = ((await fakeCfg.load()) ?? {}) as Record<string, unknown>;
  let doc: { remote?: { hosts?: Record<string, unknown>[] } };
  try {
    doc = JSON.parse(applyConfigEdits(JSON.stringify(base), edits)) as typeof doc;
  } catch (e) {
    // 往机器表里加一台、同名的已在 ⇒ 同名（同壳那一侧）；别的认不出留给写口去拒。
    const ins = edits.find((x) => x.op === "insertin" && x.path[0] === "remote");
    if (e instanceof ConfigRefused && e.kind === "element_exists" && ins && "where" in ins) return faultOf("name_taken", ins.where[0]?.equals ?? "");
    return null;
  }
  const seen = new Set<string>();
  for (const h of doc.remote?.hosts ?? []) {
    const t = (k: string) => String(h[k] ?? "").trim();
    const name = t("label") || t("host");
    if (t("host") === "") return faultOf("no_host", name);
    if (t("user") === "") return faultOf("no_user", name);
    const port = h.port === undefined || h.port === null ? 22 : Number(h.port);
    if (!Number.isInteger(port) || port < 1 || port > 65535) return faultOf("port", name);
    if (seen.has(name)) return faultOf("name_taken", name);
    seen.add(name);
  }
  return null;
}
