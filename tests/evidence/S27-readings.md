# S27 读数 —— `99 §4` 步 `23a`：把 crate 条目喂进断网门禁的缓存

> 现打 2026-09-19，量于主树 `/home/zbl/文档/claudecode-frontend/cc-monitor`
> （分支 `wave0/delete-usage-and-fix-gate`，提交 `03ab498f`）。
> 宿主 `cargo 1.98.1` / `rustc 1.98.1`。**本轮没有 commit，没有动任何 `Cargo.lock`，
> 没有动任何 `.rs` / `.ts` 源码。**

---

## 四问四答（一句话版）

| 问 | 答 |
|---|---|
| 1 · 还是 26 条吗 | **是，26 条现打对上了** —— 而且不是我数的，是 `cargo` 自己印的：`Locking 26 packages to latest compatible versions`。⚠ 但 60 那份的**拆法**错了（见 §1） |
| 2 · 哪几条在 / 不在 | 喂之前 **20 条在、6 条不在**。缺的 6 条全是 `wasm-bindgen` 那一族 ＋ `js-sys`（逐条点名见 §2） |
| 3 · 多大 / 从哪喂 | 那 6 份 `.crate` 合计 **332 739 字节 ≈ 325 KiB**；26 条合计 **1 826 305 字节 = 1.74 MiB**。**只能从网上喂**（本机没有可用的 vendor 副本），喂法 = `cargo fetch` **不带 `--target`** |
| 4 · 怎么证明管用 | **真断网跑过了**：`bwrap --unshare-net` 里 `cargo fetch --locked`（**不带 `--offline`**）RC=0；反向对照把一份挪走 ⇒ 当场红且点名（§4） |

🔴 **一条要先说的现状**：今天**门禁的断网沙箱本身是坏的**，与本件无关但影响读数口径 —— 见 §0。

---

## 0 · 先对账：今天「断网门禁」到底在哪

`.claude/devbox/gate`（不在本仓里，住 `/home/zbl/文档/claudecode-frontend/.claude/devbox/`）是那条
`docker run --rm --network none … ccmon-devbox:latest` 的唯一入口。现打三条，**都是坏的**：

| 现打 | 命令 | 结果 |
|---|---|---|
| 镜像在不在 | `docker image inspect ccmon-devbox:latest` | 🔴 `No such image` |
| 缓存卷在不在 | `docker volume inspect ccmon-cargo-registry` | 🔴 `no such volume` |
| 入口的前置检查 | `gate:22` 逐字 `[ -d "$WT/src-tauri" ]` | 🔴 重组之后没有 `src-tauri/`（是 `src/bridge`）⇒ 当场退 3 |
| 入口跑的命令 | `gate:110` 逐字 `bash scripts/gate.sh` | 🔴 重组之后是 `tests/scripts/gate.sh` |

⇒ **今天没有人能靠 `.claude/devbox/gate` 跑出一次断网门禁。**
这四条**不在本件的写区里**（那份文件在仓库外），**没动**，原样报出来。

> 🔴 **〔后续 2026-09-19〕这四条已全部修掉，本节以上的读数到此为止。**
> 修在哪：`调研/真相源/96-门禁自述腐了-坏尺子两头坏.md §五`。
> 镜像重建（`b73a4c31ff5d`，这一版含 `rustfmt`）· 入场守卫 `src-tauri/ → src/bridge/` ·
> 末行 `scripts/gate.sh → tests/scripts/gate.sh` · 顺带摘掉 `$SKILL` 挂载与 `-e PB_WS`
> （`pb check` 那一格 09-18 已整格删除）。播种实测 **1.9 G**，与本节下面那个数吻合。
> ⚠ **本节「没有版本控制」那句话是错的**：`.claude/devbox/` 自己是个 git 仓
> （`5ab839b devbox 有家了`），只是**没有 remote** —— 丢了就没了，这一条仍然未裁。

⇒ 于是「断网门禁的缓存」今天的**真身**是宿主的 `~/.cargo/registry` ——
那个具名卷是**从它播种的派生物**（`gate:45-50`）。本件喂的就是种子，**喂对了地方**。
现打：`~/.cargo/registry` 合计 **1.9 G**（`cache` 176 M · `index` 65 M · `src` 1.6 G），
`cache/index.crates.io-1949cf8c6b5b557f/` 下 `.crate` **1 033 份 → 喂完 1 039 份**。

---

## 1 · 那 26 条现打还是 26 条吗 —— **是**

**量法**（没有动仓库）：把 `src/backend` 整棵复制到 scratch、`../bridge` 用软链补同深度
（8 个 `*-core` 的 `path` 依赖才解得开）、在**副本**的 `[dependencies]` 里加一行
`russh-sftp = "3.0.0"`、跑 `cargo fetch`。cargo 第一行逐字：

```
Locking 26 packages to latest compatible versions
```

随后 26 行 `Adding` / `Updating` 就是 `S27-cache-manifest.md` 那张表。
**跑了两趟**（手工一趟 ＋ `S27-prove-offline.py --prime` 一趟），两趟同一份 26 行。

### ⚠ 但 60 那份的拆法是错的（总数对、构成错）

`调研/真相源/60-触手全图-它碰到你机器上的什么.md` §3 那张表写：

| 60 写的 | 现打 |
|---|---|
| 新增包 **9** 个（后面却列了 **10** 个名字） | **11** 个 `Adding` |
| 被顶版的已有包 **16** 个 | **15** 个 `Updating` |
| 合计 26 | **合计 26 ✅** |

成因看得见：60 把 `syn 3.0.6` 单独列进「`syn` 变成两份」那一行，没算进「新增 9」里；
而「9」与它自己列的 10 个名字本来就对不上。
⇒ **「26」这个数经得起现打，它的拆分账不经**。这与本轮任务交代的「846/386」「50 项基线」
「13 个死规则里 3 个假阳性」是同一族：**数字写下之后没人回来复算**。

### 另外两格顺手复核（都与 60 一致）

- 🟢 `crypto-bigint` **0.7.3 → 0.7.3，没被动**（那颗 yanked 的钉子不受影响）。
- 🟠 `syn` 在新 lock 里确实是**两份**：`2.0.117` ＋ `3.0.6`。
- 副本 lock 的包块数 **265 → 276**（净 +11）；26 条里另外 15 条是同名换版。

---

## 2 · 今天哪几条在缓存里、哪几条不在 —— **20 在 / 6 不在**

**逐条点名（喂之前不在的那 6 条）**：

| crate | 版本 | .crate 字节 |
|---|---|---|
| `js-sys` | 0.3.105 | 113 002 |
| `wasm-bindgen` | 0.2.128 | 70 345 |
| `wasm-bindgen-futures` | 0.4.78 | 8 291 |
| `wasm-bindgen-macro` | 0.2.128 | 9 595 |
| `wasm-bindgen-macro-support` | 0.2.128 | 118 512 |
| `wasm-bindgen-shared` | 0.2.128 | 12 994 |

其余 20 条喂之前就在（逐条与字节数见 `S27-cache-manifest.md` 那张表的「喂前在不在」列）。

**凭什么说是这 6 条**（两把独立的尺子同结论）：
① `cargo fetch` 那一趟只印了 6 行 `Downloaded`，正是这 6 个；
② 缓存里这 6 份 `.crate` 的 mtime 是 `2026-09-19 11:52`（就是那一趟），其余 20 份是 09-15 … 09-18。

⚠ **为什么偏偏是这一族**：它们是 `russh-sftp` 的 `cfg(target_arch = "wasm32")` 依赖 ——
**不为我们任何一个 target 构建**，但**进 lock，就要进缓存**。
`cargo build` / `cargo check` 永远抓不到它们 ⇒ **本机构建全绿、断网门禁照红**。60 点名这一族是对的。

### 顺带一条更要紧的读数：**今天两份 lock 是齐的**

`src/bridge/Cargo.lock` **668** 条 crates.io 条目 · `src/backend/Cargo.lock` **256** 条
（去重并集 **672** 条，`.crate` 合计 **128.2 MiB**）⇒ **缺 0 条**。
⇒ 「门禁今天红在下不到包上」这件事**今天不成立**；`23a` 防的是**落地那一刻**。

---

## 3 · 喂多大、从哪喂

- **多大**：净增 **332 739 字节 ≈ 325 KiB**（6 份 `.crate`）。26 条全量是 1.74 MiB，
  但其中 20 份本来就在 ⇒ 真正的磁盘代价只有那 325 KiB。
  `~/.cargo/registry` 1.9 G 的量级下**可忽略**。
  ⚠ 这只算 `.crate`。真构建时 cargo 还会把它们解包进 `registry/src/`（那一坨今天已经 1.6 G），
  解包是构建时的事，本件不预支。
- **从哪喂**：**只能联网从 crates.io 喂**。
  - 本机没有可用的 vendor 副本：`src/bridge/vendor/` 下只有 `code-picture-core`（`path` 依赖，不走 registry）；
    `参考实现/russh-sftp-3.0.0/` 只是**解开的源码**、不是 `.crate`，而且 `russh-sftp` 本身**本来就在缓存里**。
  - 🔴 **喂法必须是 `cargo fetch` 且不带 `--target`** —— cargo 不带 `--target` 时抓**全 target**，
    `wasm-bindgen` 那一族才躲不过。用 `cargo build` / `cargo fetch --target x86_64-unknown-linux-gnu` 喂
    **会漏掉那 6 条中的全部**，而漏了之后本机一切正常，只有断网门禁会红。
  - 一条命令：`python3 tests/evidence/S27-prove-offline.py --prime`（它就是「喂缓存」本身）。
- **对仓库的影响**：**零**。整趟在 scratch 副本里解析，`--locked` 保证 cargo 不改任何 lock；
  `git status` 只多出本件新建的 4 个文件。

---

## 4 · 🔴 喂完怎么证明它真的管用 —— 真断网跑过了

**为什么不只信 `--offline`**：那是 cargo **自己**答应不上网，证的是「它不想上网」，
不是「上不了网也行」。门禁买的是后者。

**本机能造真断网的只有 `bwrap`**（现打：`unshare -rn` 报 `/proc/self/uid_map: 不允许的操作`；
`docker` 那个镜像已经没了）。于是：

```
bwrap --dev-bind / / --unshare-net --chdir <副本> cargo fetch --locked      # 不带 --offline
```

| 格 | 读数 |
|---|---|
| stage0 · `src/bridge` 今天的 lock | **RC=0** |
| stage0 · `src/backend` 今天的 lock | **RC=0** |
| stage2 · 副本（含 `russh-sftp 3.0.0` ＋ 那 26 条） | **RC=0** ⇒ 26 条断网解析 + 取包全通 |

**🔴 反向对照（否则不知道这把尺子是不是恒绿）**：把 `wasm-bindgen-0.2.128.crate` 从缓存里挪走再跑同一格：

```
error: failed to download from `https://static.crates.io/crates/wasm-bindgen/0.2.128/download`
Caused by:
  [6] Could not resolve hostname (Could not resolve host: static.crates.io)
```

⇒ 两件事同时被这条错买下来：**① 尺子会红；② 那个壳里网是真的死的**（DNS 都解不动），
不是 cargo 在自律。跑完脚本 `finally` 把文件原样放回（读数里逐字印了放回的路径）。

---

## 5 · 判据：「那 26 条还在不在」怎么知道

缓存是会被清掉的（`cargo cache -a`、换机器、卷被删）。**今天喂完、明天没了，而门禁会红在
一条与真实原因无关的诊断上**（`GATE: FAIL —— cargo（退出码 101）`，真话是「下不到包」）。
⇒ 立了判据，两层：

| 件 | 是什么 | 跑法 |
|---|---|---|
| `tests/evidence/S27-cache-manifest.md` | 那 26 条的**唯一真相源**（机器读的表） | — |
| `tests/evidence/S27-offline-cache-check.py` | 量具：两个人群逐条点名 | `python3 tests/evidence/S27-offline-cache-check.py` |
| `tests/offline-cargo-cache.vitest.ts` | 把量具挂进 `npm test`（门禁第 `npm` 格） | `npx vitest run tests/offline-cargo-cache.vitest.ts` |
| `tests/evidence/S27-prove-offline.py` | 真断网跑一趟 ＋ 反向对照 | `python3 tests/evidence/S27-prove-offline.py [--prime\|--negative <file>]` |

**两个人群**（都设了反空真地板 —— 扫到 0 条当**失败**，不是「没问题」）：
- **P1** = 今天两份 lock 的 crates.io 条目去重并集，现打 **672**，地板 500；
- **P2** = manifest 那 26 条，**恒等 26**（不是「≥」）。

### 判据会红 —— 实打，不是推的

| 动作 | 量具 | vitest |
|---|---|---|
| 挪走 `js-sys-0.3.105.crate` ＋ `tokio-util-0.7.19.crate` | RC=**1**，逐条印 `🔴 P2 缺：js-sys 0.3.105` / `🔴 P2 缺：tokio-util 0.7.19`，末行 `S27: FAIL` | **1 failed**，断言消息逐字带缺的名字 ＋ 补法命令 |
| 放回去 | RC=0，`S27: OK` | 2 passed |

⚠ **第一版的 vitest 红得不点名** —— 量具把「缺了几份」和「人群本身坏了」放进了同一个列表，
而 vitest 先断言后者 ⇒ 具名那条永远轮不到开口，报出来是一句「缺 2 份」。
**已修**（`S27-offline-cache-check.py` 里那句 `list(fail)` 的头注记着这一格）。
**这正是本件要治的病的同形**：判据红了，但红在一条不点名的诊断上。

### 诚实段：判据挡不住什么

- **整个缓存被清空** ⇒ 672 条一条都不命中 ⇒ vitest 落进「判不了」⇒ **本条不红**。
  那一格靠门禁 `cargo` 格自己红（红得难看但会红）＋ `S27-prove-offline.py` 的 stage0 兜。
  **这个洞写在 `tests/offline-cargo-cache.vitest.ts` 的头注里，没藏。**
  为什么要留这个口子：`ci.yml` 的 frontend job 跑在 **windows-latest**，那台机器上
  本仓 672 条一条都没有 —— 不留口子它在云端恒红，而那是**假阳性**。
- 只看 `.crate` 在不在，不验 checksum、不验编得过。
- 看的是**宿主的种子缓存**，不是沙箱那个具名卷（卷是派生物；要对齐就把卷删掉让它重播种）。
- manifest 会腐：`russh-sftp` / `serde` / `wasm-bindgen` 任一发新版，重解析的版本就与表不同
  ⇒ 判据会点名「表里有、缓存里没有」。那时**不是补缓存，是重跑 §复算**。

---

## 6 · 没做 / 判不了 / 交回去的

1. 🔴 **判据没有挂进 `tests/scripts/gate.sh`** —— 那份文件本件不许改（要改先报）。
   挂法是一行，照现有格式：
   ```bash
   run_gate offline-cache '不是数出来的数：只有齐/缺两态。人群 = 两份 lock 的 crates.io 条目并集（现打 672）＋ 23a 那 26 条；缺谁当场点名。⚠ 它只看 .crate 在不在，不证编得过' \
            bash -c 'python3 tests/evidence/S27-offline-cache-check.py && echo "offline-cache: 1 passed"'
   ```
2. 🔴 **本件 4 个文件一旦被 commit，`shell_lint_registry` 那条恒等判据会不会红 —— 已经躲开了**。
   第一版把断网证明写成 `tests/evidence/S27-prove-offline.sh`，
   `cargo test -p monitor --lib shell_lint_registry` 当场红，逐字：`left: 69  right: 70`
   （`ci.yml` 的 shellcheck 覆盖面地板被钉成**等号**）。
   ⇒ 改写成 `.py`（`tests/evidence/*.py` 没有同形人群地板，CI 的 `python syntax compile` 只盖 `tests/e2e/*.py`）。
   复核：改完 `shell_lint_registry` **4 passed**。
3. **`russh-sftp` 没有真落地**（`src/backend/Cargo.toml` 一个字没动，`Cargo.lock` 一个字没动）——
   那是 `23a` 之后的事，不是本件。
4. **判不了 · `crypto-bigint 0.7.0…0.7.4` 到底有没有被 yank**：本轮没查 crates.io 的 yank 状态。
   只现打到一条相邻事实：**加 `russh-sftp` 没有动 `crypto-bigint 0.7.3`**，那颗钉子不受影响。
5. **判不了 · 沙箱里跑起来是什么样**：镜像没了（§0）⇒ 本轮所有断网读数都是 `bwrap` 造的壳，
   不是 `ccmon-devbox` 那个容器。两者的 `CARGO_HOME` 不同（容器里是 `/opt/rust/cargo`），
   量具对这一格是**兼容的**（先读 `$CARGO_HOME`，再回落 `~/.cargo`），但**没在容器里实跑过**。

---

## 复算

```bash
cd /home/zbl/文档/claudecode-frontend/cc-monitor

# 1 · 这 26 条今天还是不是 26 条（联网；不动仓库、不动 lock）
python3 tests/evidence/S27-prove-offline.py --prime          # 看 `Locking N packages` 那一行

# 2 · 缓存齐不齐（离线，秒级）
python3 tests/evidence/S27-offline-cache-check.py            # 加 --json 给机器读

# 3 · 真断网跑一趟 + 反向对照（要 bwrap）
python3 tests/evidence/S27-prove-offline.py
python3 tests/evidence/S27-prove-offline.py --negative wasm-bindgen-0.2.128.crate

# 4 · 判据会不会红（挪走两份再放回）
C=~/.cargo/registry/cache/index.crates.io-*/
mv $C/js-sys-0.3.105.crate /tmp/ && python3 tests/evidence/S27-offline-cache-check.py; \
  mv /tmp/js-sys-0.3.105.crate $C

# 5 · 装进 npm test 的那一半
npx vitest run tests/offline-cargo-cache.vitest.ts

# 6 · 确认本件没碰坏既有判据
npx vitest run                                                # 134 files / 1749 tests 全绿（含本件 2 条）
cd src/bridge && cargo test -p monitor --lib shell_lint_registry   # 4 passed
```
