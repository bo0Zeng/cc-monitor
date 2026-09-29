/**
 * 〔CFG1〕前端测试用的**假 config.json**：`load_config` / `patch_config` 两条命令的内存替身。
 *
 * 补丁语义与 Rust 写口（`src/frontend/shell/src/config.rs::patch_config_at`）同一份金样
 * `tests/__fixtures__/config-patch.golden.json` 对拍（`tests/frontend/ui/config-patch-fake.vitest.ts` 跑这边，
 * `tests/frontend/shell/config_tests.rs::the_golden_cases_hold` 跑那边）⇒ 用它的测试看到的「盘上终态」就是 Rust 会落的那份。
 */
import { vi } from "vitest";

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
      // 〔FIX2 续 · ㊶〕同 Rust `InsertIn`：路上缺的段补成 `{}`、数组缺就建；已有满足 where 的 ⇒ 整批拒（element_exists）。
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
      // 〔FIX2 续 · ㊶〕同 Rust `RemoveIn`：认恰好一个才删，否则整批拒（element_gone）。
      let cur: unknown = obj;
      for (const seg of e.path) cur = isObj(cur) ? cur[seg] : undefined;
      if (!Array.isArray(cur)) throw new ConfigRefused("element_gone");
      const hits = cur.flatMap((x, i) => (matches(x, e.where) ? [i] : []));
      if (hits.length !== 1) throw new ConfigRefused("element_gone");
      cur.splice(hits[0]!, 1);
      continue;
    }
    if (e.op === "setin") {
      // 〔FIX · ㊶〕同 Rust `apply_edit` 的 `SetIn`：认恰好一个元素（每条 where：fields 按序第一个非空字符串 == equals），只改一格。
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
    patch_config: async ({ edits }: { edits: Edit[] }): Promise<void> => {
      disk.patches.push(edits);
      disk.text = applyConfigEdits(disk.text, edits);
      disk.saved.push(JSON.parse(disk.text) as Record<string, unknown>);
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
// ⚠ `patchConfig` / `patchConfigFrom` 刻意**不是** `vi.fn`：`vi.resetAllMocks()` 会把 `vi.fn` 的实现一起清掉。
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
  const patchConfig = async (edits: readonly Edit[]): Promise<void> => {
    fakeCfg.patches([...edits]);
    const base = ((await fakeCfg.load()) ?? {}) as Record<string, unknown>;
    const doc = JSON.parse(applyConfigEdits(JSON.stringify(base), edits)) as Record<string, unknown>;
    // 写完之后再读，读到的是写完的那份（真盘就是这样）。从前旧测试靠的是生产代码**原地改了**
    // `loadConfig` 摆出来的那个对象 —— 那是替身的巧合，不是盘的语义。
    fakeCfg.load.mockResolvedValue(JSON.parse(JSON.stringify(doc)));
    fakeCfg.saved(doc);
  };
  const patchConfigFrom = async (
    build: (cfg: Record<string, unknown>) => readonly Edit[],
  ): Promise<void> => {
    const edits = build(((await fakeCfg.load()) ?? {}) as Record<string, unknown>);
    if (edits.length > 0) await patchConfig(edits);
  };
  return { ...actual, loadConfig: fakeCfg.load, patchConfig, patchConfigFrom };
}
