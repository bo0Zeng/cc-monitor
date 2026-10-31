/**
 * 前端测试用的 `config.json` 替身：`loadConfig` 照测试摆的那一份回，`patchConfig` 只记下交出去的补丁、不应用。
 *
 * 补丁落到盘上是什么样只在 Rust 写口（`src/frontend/shell/src/config.rs::patch_config_at`，金样
 * `tests/__fixtures__/config-patch.golden.json` 由 `tests/frontend/shell/config_tests.rs` 判）。界面测试核对的是
 * 「交出去的补丁」（`fakeCfg.patches`）；要「写完再读」的那几条由测试自己摆下一份（`fakeCfg.load.mockResolvedValue`）。
 *
 * 机器表不在这份里：本机后端那三条（`machines-read` / `-try` / `-set`）的替身是 `machines-fake.ts`（只回后端现打的回包）。
 */
import { vi } from "vitest";
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
