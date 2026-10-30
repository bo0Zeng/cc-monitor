/**
 * 前端测试用的 `config.json` 替身：`loadConfig` 照测试摆的那一份回，`patchConfig` 只记下交出去的补丁、不应用。
 *
 * 补丁落到盘上是什么样只在 Rust 写口（`src/frontend/shell/src/config.rs::patch_config_at`，金样
 * `tests/__fixtures__/config-patch.golden.json` 由 `tests/frontend/shell/config_tests.rs` 判）。界面测试核对的是
 * 「交出去的补丁」（`fakeCfg.patches`）；要「写完再读」的那几条由测试自己摆下一份（`fakeCfg.load.mockResolvedValue`）。
 *
 * 机器表那三条（`machines-read` / `-try` / `-set`，[`mockedChanCaller`] · [`fakeMachinesCall`]）还是在替身里现算的：
 * 表摆在同一块假盘的 `remote.hosts` 里，写完的整份交 `fakeCfg.saved`。这一半要换成后端现打的回包金样（图上 A17-60-7-1-4）。
 */
import { vi } from "vitest";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { ConfigEdit } from "../../../src/frontend/ui/generated/ConfigEdit";

export type Edit = ConfigEdit;

// ───────────────────────── 整个 `src/config` 模块的替身 ─────────────────────────
//
//   `vi.mock("../../../src/frontend/ui/config", async (orig) => (await import("./config-patch-fake")).mockedConfigModule(orig));`
// `loadConfig` 是 `fakeCfg.load`（`mockResolvedValue` 摆盘上那份）；`patchConfig` 把这一批补丁交 `fakeCfg.patches`（按次序），
// 不改 `load` 读到的那份。`setAt` / `removeAt` / 登记表用真的。
// ⚠ `patchConfig` 刻意**不是** `vi.fn`：`vi.resetAllMocks()` 会把 `vi.fn` 的实现一起清掉。
export const fakeCfg = {
  load: vi.fn(),
  /** 机器表那三条写完之后的整份（按次序）。 */
  saved: vi.fn(),
  /** 每次写收到的补丁（按次序）。 */
  patches: vi.fn(),
};

export async function mockedConfigModule(
  importOriginal: <T>() => Promise<T>,
): Promise<Record<string, unknown>> {
  const actual = await importOriginal<Record<string, unknown>>();
  const patchConfig = async (edits: readonly Edit[]): Promise<void> => {
    fakeCfg.patches([...edits]);
  };
  return { ...actual, loadConfig: fakeCfg.load, patchConfig };
}

/** 交出去的补丁摊平成一列（按次序）。 */
export function sentEdits(): Edit[] {
  return vi.mocked(fakeCfg.patches).mock.calls.flatMap((c) => c[0] as Edit[]);
}

// ───────────────────────── 机器表那三条（还在现算，见文件头） ─────────────────────────

type Where = { fields: string[]; equals: string }[];
/** 后端 `MachineEdit` 那一形。 */
type MachineEditLike =
  | { op: "setin"; where: Where; field: string; value: unknown; ifEmpty?: boolean }
  | { op: "insertin"; where: Where; value: unknown }
  | { op: "removein"; where: Where };

class TableRefused extends Error {
  constructor(readonly kind: "element_gone" | "element_exists") {
    super(`machines refused: ${kind}`);
  }
}

/** 把机器表补丁应用到 `remote.hosts` 上，回新的整份；拒 ⇒ 抛 [`TableRefused`]。 */
function applyTableEdits(base: Record<string, unknown>, edits: readonly MachineEditLike[]): Record<string, unknown> {
  const doc = JSON.parse(JSON.stringify(base)) as { remote?: { hosts?: unknown } } & Record<string, unknown>;
  const isObj = (x: unknown): x is Record<string, unknown> => !!x && typeof x === "object" && !Array.isArray(x);
  const keyOf = (o: Record<string, unknown>, fields: string[]) => fields.map((f) => o[f]).find((v): v is string => typeof v === "string" && v !== "");
  const matches = (x: unknown, where: Where) => isObj(x) && where.every((w) => keyOf(x, w.fields) === w.equals);
  if (!isObj(doc.remote)) doc.remote = {};
  if (doc.remote.hosts === undefined) doc.remote.hosts = [];
  const rows = doc.remote.hosts;
  if (!Array.isArray(rows)) throw new TableRefused("element_gone");
  let exists = false;
  for (const e of edits) {
    const hits = rows.flatMap((x, i) => (matches(x, e.where) ? [i] : []));
    if (e.op === "insertin") {
      if (hits.length > 0) exists = true;
      else rows.push(JSON.parse(JSON.stringify(e.value)));
      continue;
    }
    if (hits.length !== 1) throw new TableRefused("element_gone");
    if (e.op === "removein") {
      rows.splice(hits[0]!, 1);
      continue;
    }
    const row = rows[hits[0]!] as Record<string, unknown>;
    const v = row[e.field];
    const taken = v !== undefined && v !== null && (typeof v !== "string" || v.trim() !== "");
    if (!((e.ifEmpty ?? false) && taken)) row[e.field] = JSON.parse(JSON.stringify(e.value));
  }
  if (exists) throw new TableRefused("element_exists");
  return doc;
}

/** 机器表写口：写完的整份成为之后 `load` 读到的那份，并交 `fakeCfg.saved`。 */
async function writeTable(edits: readonly MachineEditLike[]): Promise<void> {
  fakeCfg.patches([...edits]);
  const base = ((await fakeCfg.load()) ?? {}) as Record<string, unknown>;
  const doc = applyTableEdits(base, edits);
  fakeCfg.load.mockResolvedValue(JSON.parse(JSON.stringify(doc)));
  fakeCfg.saved(doc);
}

/** 后端读回的一行（`{name, host}`）：名字照后端 `Host::name` 那条算（`label` 去首尾空白，空 ⇒ `host` 去首尾空白；规则本身的判据在后端）。 */
function rowOf(h: Record<string, unknown>): { name: string; host: Record<string, unknown> } {
  const t = (k: string) => String(h[k] ?? "").trim();
  return { name: t("label") || t("host"), host: h };
}

/**
 * `machines-read` 那一答（纯函数）：机器表摆在一份假 `config.json` 的 `remote.hosts` 里。没有 `remote` ⇒ `absent`；
 * 不是数组 ⇒ `unreadable`。自己桩 `chan.call` 的测试照它答 `machines-read`（表照旧摆在它们那块盘上）。
 */
export function machinesReadOf(cfg: unknown): { state: string; reason: string | null; detail: null; path: null; hosts: unknown[]; hostKeys: Record<string, string[]> } {
  const remote = (cfg as { remote?: { hosts?: unknown } } | null)?.remote;
  if (remote === undefined) return { state: "absent", reason: null, detail: null, path: null, hosts: [], hostKeys: {} };
  return Array.isArray(remote.hosts)
    ? { state: "ok", reason: null, detail: null, path: null, hosts: (remote.hosts as Record<string, unknown>[]).map(rowOf), hostKeys: {} }
    : { state: "unreadable", reason: "读不懂", detail: null, path: null, hosts: [], hostKeys: {} };
}

/** 本机后端机器表那三条命令的替身本体：回那一答（拒 ⇒ 抛）；不是这三条 ⇒ `undefined`。 */
export async function fakeMachinesCall(cmd: unknown, req: unknown): Promise<unknown> {
  const edits = ((req as { edits?: MachineEditLike[] } | null)?.edits ?? []) as MachineEditLike[];
  const read = async () => machinesReadOf((await fakeCfg.load()) ?? {});
  if (cmd === "machines-read") return read();
  if (cmd === "machines-try") return { fault: await fakeMachineTableTry(edits) };
  if (cmd === "machines-set") {
    const identity = edits.some((e) => e.op === "insertin" || (e.op === "setin" && ["label", "host", "port", "user", "addresses"].includes(e.field)));
    const fault = identity ? await fakeMachineTableTry(edits) : null;
    if (fault) throw new Error(fault.said);
    await writeTable(edits);
    return { machines: await read() };
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

/** 后端写好的那一句（`MachineFault::new`，键同；这里只为替身回包带上那一格）。 */
const faultOf = (code: string, name: string): { code: string; name: string; said: string } => ({
  code,
  name,
  said:
    code === "name_taken" ? copyText("addMachine.err.taken", { name })
    : code === "port" ? copyText("addMachine.err.port")
    : code === "no_host" ? copyText("addMachine.err.host")
    : copyText("addMachine.err.user"),
});

/**
 * 机器表那一道的替身（后端 `ssh-dial/machine.rs::check_table` 的合成版，给「界面按回来的码说那一句」那一族用）：
 * 把补丁应用到 `fakeCfg.load()` 那一份上，按名字不重 · 端口 1–65535 · 地址 / 用户非空判第一处。
 */
export async function fakeMachineTableTry(edits: readonly MachineEditLike[]): Promise<{ code: string; name: string; said: string } | null> {
  const base = ((await fakeCfg.load()) ?? {}) as Record<string, unknown>;
  let doc: { remote?: { hosts?: Record<string, unknown>[] } };
  try {
    doc = applyTableEdits(base, edits) as typeof doc;
  } catch (e) {
    // 往机器表里加一台、同名的已在 ⇒ 同名；别的认不出留给写口去拒。
    const ins = edits.find((x) => x.op === "insertin");
    if (e instanceof TableRefused && e.kind === "element_exists" && ins) return faultOf("name_taken", ins.where[0]?.equals ?? "");
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
