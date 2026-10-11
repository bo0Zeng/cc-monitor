/**
 * 测试里「等多久算挂了」的唯一上限（10-11 起），与 Rust 的 `guard_core::wall::CEILING_SECS` 同一个数。
 *
 * 判据不许拿墙钟判「够不够快」：等一件事发生（`vi.waitFor`）、给整条用例封顶（`testTimeout`），一律用它；
 * 量快慢的改计步数 / 计调用次数 / 假定时器。量纲是「机器忙」：门禁并发时负载到核数的四倍、单条慢十来倍也要还绿。
 * 扫测试源的判据：`tests/common/guard-core/wall_ceiling_tests.rs`（别处写数字期限 ⇒ 红）。
 */
export const WALL_CEILING_MS = 300_000;

/** `vi.waitFor(fn, UNTIL_CEILING)`：等到了就走，等满上限才算挂。 */
export const UNTIL_CEILING = { timeout: WALL_CEILING_MS, interval: 20 } as const;
