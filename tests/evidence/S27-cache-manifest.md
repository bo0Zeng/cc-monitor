# S27 · 断网门禁 crate 缓存 —— `russh-sftp 3.0.0` 落地要新增的条目（唯一源头）

> 现打 2026-09-19。**这份表是机器读的**：`tests/evidence/S27-offline-cache-check.py` 与
> `tests/offline-cargo-cache.vitest.ts` 都解析下面那张表，**别改成散文**。
>
> 「附：`russh-sftp` 3.0.0 适配实测 §3 落地代价」那一格（`23a`）。

## 这 26 条是什么

`src/backend`（`cc-monitor-remote`）今天**没有** `russh-sftp`。加上
`russh-sftp = "3.0.0"` 之后，`src/backend/Cargo.lock` 会多出 / 顶掉 26 个
`(name, version)` 对。断网门禁（`.claude/devbox/gate` 默认 `--network none`）
靠本机 cargo 缓存里那份 `<name>-<version>.crate` 供包 ——
**缓存里缺哪怕一条，`cargo` 当场退 101，报的是「下不到包」，与代码对不对毫无关系。**

## 这个数是怎么来的（不是抄文档，是 cargo 自己说的）

把 `src/backend` 整棵复制到 scratch、`../bridge` 用软链补同深度、加那一行依赖、跑 `cargo fetch`，
cargo 第一行逐字印：**`Locking 26 packages to latest compatible versions`**，随后 26 行
`Adding` / `Updating` 就是下表。**没有动仓库，没有动 `Cargo.lock`。**
复算命令住 `S27-readings.md §复算`。

⚠ **文档那格的「26」是对的，但它的拆法是错的**：60 那份写「新增包 **9** 个（列了 **10** 个名字）
＋ 被顶版 **16** 个」；现打是 **新增 11 · 顶版 15**（`syn 3.0.6` 是第 11 个新增包，
60 把它单独列进「`syn` 变成两份」那一行、没算进 9 里）。**总数 26 不变。**

## 表

`变化`：`新增` = lock 里原来没有这个包；`顶版` = 包在、版本被顶到新的一档（旧档仍被今天的 lock 用着，**不许删**）。
`喂前在不在`：2026-09-19 喂之前，本机 `~/.cargo/registry/cache/index.crates.io-*/` 里有没有那份 `.crate`。

| crate | 版本 | 变化 | 旧版本 | .crate 字节 | 喂前在不在 |
|---|---|---|---|---|---|
| `bitflags` | 2.13.2 | 顶版 | 2.11.1 | 51678 | 在 |
| `bytes` | 1.12.1 | 顶版 | 1.11.1 | 75667 | 在 |
| `chrono` | 0.4.45 | 顶版 | 0.4.44 | 240866 | 在 |
| `dashmap` | 6.2.1 | 新增 | — | 26885 | 在 |
| `gloo-timers` | 0.4.0 | 新增 | — | 8849 | 在 |
| `hashbrown` | 0.14.5 | 新增 | — | 141498 | 在 |
| `js-sys` | 0.3.105 | 顶版 | 0.3.98 | 113002 | 🔴 缺 |
| `lock_api` | 0.4.14 | 新增 | — | 29249 | 在 |
| `log` | 0.4.34 | 顶版 | 0.4.29 | 53395 | 在 |
| `parking_lot_core` | 0.9.12 | 新增 | — | 34110 | 在 |
| `redox_syscall` | 0.5.18 | 新增 | — | 30747 | 在 |
| `russh-sftp` | 3.0.0 | 新增 | — | 57472 | 在 |
| `scopeguard` | 1.2.0 | 新增 | — | 11619 | 在 |
| `serde` | 1.0.229 | 顶版 | 1.0.228 | 83669 | 在 |
| `serde_bytes` | 0.11.19 | 新增 | — | 13427 | 在 |
| `serde_core` | 1.0.229 | 顶版 | 1.0.228 | 63100 | 在 |
| `serde_derive` | 1.0.229 | 顶版 | 1.0.228 | 59864 | 在 |
| `syn` | 3.0.6 | 新增 | — | 313821 | 在 |
| `thiserror` | 2.0.20 | 顶版 | 2.0.18 | 28969 | 在 |
| `thiserror-impl` | 2.0.20 | 顶版 | 2.0.18 | 21436 | 在 |
| `tokio-util` | 0.7.19 | 新增 | — | 147245 | 在 |
| `wasm-bindgen` | 0.2.128 | 顶版 | 0.2.121 | 70345 | 🔴 缺 |
| `wasm-bindgen-futures` | 0.4.78 | 顶版 | 0.4.71 | 8291 | 🔴 缺 |
| `wasm-bindgen-macro` | 0.2.128 | 顶版 | 0.2.121 | 9595 | 🔴 缺 |
| `wasm-bindgen-macro-support` | 0.2.128 | 顶版 | 0.2.121 | 118512 | 🔴 缺 |
| `wasm-bindgen-shared` | 0.2.128 | 顶版 | 0.2.121 | 12994 | 🔴 缺 |

**合计 26 条 · `.crate` 合计 1 826 305 字节 = 1.74 MiB。**
喂之前缺 **6** 条（全在 `wasm-bindgen` 那一族 ＋ `js-sys`），那 6 份合计 **332 739 字节 ≈ 325 KiB**。

⚠ **那 6 条为什么会缺，值得记一笔**：它们是 `russh-sftp` 的
`cfg(target_arch = "wasm32")` 依赖 —— **不为我们任何一个 target 构建**，
但 **进 lock，就要进缓存**。`cargo fetch` 不带 `--target` 时抓的是**全 target**，
所以它们躲不过；而人若只跑 `cargo build`，本机永远抓不到它们，
于是「本机构建全绿、断网门禁照红」。60 那份点名这一族是对的。

## 这份表会腐（写在前面）

- 它钉的是**一次解析的结果**。`russh-sftp` / `serde` / `wasm-bindgen` 任何一条发新版，
  重解析出来的版本就与这张表不同 ⇒ 判据会点名「表里有、缓存里没有」的那几条。
  那时**不是补缓存，是重跑 §复算 重生成这张表**。
- 它**不管** `src/bridge`（monitor 那棵）。那棵没有 `russh-sftp`，本件不动它。
- 落地（真把 `russh-sftp` 写进 `src/backend/Cargo.toml`）**不是本件的活**，
  本件只把缓存备齐并立判据。
