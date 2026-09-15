# `K-R120` 死值验留档 —— 版本号 `3.8.0` ＋ CHANGELOG 的 breaking 段 ＋ 那道新闸

> 本件两条 DoD：`KR120D1`（七处一次改齐到 `3.8.0`）· `KR120D2`（CHANGELOG 的 breaking 段
> ＋ 给「版本 bump 而 CHANGELOG 没跟」补一道闸）。
> 🔴 **本件不推 tag、不发版** —— 那是 `K-R119`。
>
> **量于**：基点 `0a92892`（`track/k-r120` 从主干 ff 过来那一刻），本件的提交是 `c6d3863`。
> **门禁一律在沙箱里跑**（`K31`）：
> `PB_WS=backend-consolidation .claude/devbox/gate /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r120 k-r120`
> ，从项目根 `/home/zbl/文档/claudecode-frontend` 起跑，镜像 `ccmon-devbox:latest`。
> **宿主上一条 `cargo` / `npm` / `tsc` / `vitest` 都没跑过**；宿主上跑过的只有纯静态 python
> 与只读 `git`（`evidence/K-R120-ruler.py` · `evidence/K-R120-cut.py` · `evidence/K-R118-ruler.py`）。

---

## §A 门禁读数 —— `M0` 基线 / `M1` 终态

两趟都是**整趟 16 格**，同一棵树、同一个 tag、同一个沙箱。

| 格 | `M0`（基点 `0a92892`，写区一个字节未动，只有 3 份未跟踪的 evidence 占位） | `M1`（七处 bump ＋ CHANGELOG 那一节 ＋ 新闸都在盘上） |
|---|---|---|
| hooks | 11 passed | 11 passed |
| copy2 | 11 passed | 11 passed |
| fmt | 1 passed | 1 passed |
| fmt-daemon | 1 passed | 1 passed |
| winchk | 1 passed | 1 passed |
| cargo | **1613** passed（9 个包合计） | **1614** passed（9 个包合计） |
| generated | 与 Rust 源一致 | 与 Rust 源一致 |
| deadcode | 41 passed（本格墙钟 62 秒，冷 target） | 41 passed（本格墙钟 9 秒，热 target） |
| daemon | 762 passed | 762 passed |
| tsc | 366 passed | 366 passed |
| npm | 1726 passed | 1726 passed |
| ccm e2e ×4 | 12 / 8 / 46 / 45 | 12 / 8 / 46 / 45 |
| pb check | `FAIL=0 BROKEN=0` | `FAIL=0 BROKEN=0` |
| **裁决** | `GATE: OK —— 16 格全绿`（RC=0） | `GATE: OK —— 16 格全绿`（RC=0） |

**标签多重集差 = 空**（`M0` 与 `M1` 的格名逐格相同，16 ↔ 16）。
**唯一动了的读数是 `cargo` 1613 → 1614，`+1` 就是本轮新加的那一条判据**
`doc_claim_registry::tests::the_changelog_top_section_is_the_version_we_ship`。
⚠ 两趟墙钟不可比（`M0` 冷 target / `M1` 热 target，`deadcode` 自印的 62 秒 vs 9 秒是旁证）。

⚠ `M0` 的 1613 与 `K-R118` 交回时记的 1611 **不是漂移**：分母不同 ——
那一趟量于 `98a8d51`，本轮量于 `0a92892`（中间并了 `K-R116` / `K-R117` / `K-R118` 三支）。

⚠ 本树**未铺** `src-tauri/embedded-daemons/` ⇒ `cargo` 那一格少「本地后端真的能起来吗」那族 4 条
（门禁自己每趟印这行分母）。

---

## §B `KR120D1` —— 七处逐处的改前 / 改后

**六处的清单不是手数的**：`evidence/K-R120-ruler.py --places` 转调
`evidence/K-R118-ruler.py --places`，后者解析
`doc_claim_registry::the_release_version_is_the_same_in_all_six_places` 的**函数体**，
「六」= 那条判据里 `pick()` 的调用点数。第七处由本轮这把尺子自己补。

| # | 谁 | 文件 | 逐字锚点 | 行 | 改前 | 改后 | 锚点命中 |
|---|---|---|---|---|---|---|---|
| 1 | 权威源 | `package.json` | `\n  "version": "` | 3 | 3.7.0 | **3.8.0** | 1 |
| 2 | | `src-tauri/Cargo.toml` | `\nversion = "` | 26 | 3.7.0 | **3.8.0** | 1 |
| 3 | | `src-tauri/tauri.conf.json` | `\n  "version": "` | 3 | 3.7.0 | **3.8.0** | 1 |
| 4 | | `README.md` 抬头那行 | `当前版本: v` | 11 | 3.7.0 | **3.8.0** | 1 |
| 5 | | `README.md`「项目当前状态」块 | `- **版本**：v` | 328 | 3.7.0 | **3.8.0** | 1 |
| 6 | | `README.en.md` 抬头那行 | `\| Current: v` | 5 | 3.7.0 | **3.8.0** | 1 |
| 7 | 判据够不着 | `src-tauri/Cargo.lock` monitor | `name = "monitor"\nversion = "` | 2828 | 3.7.0 | **3.8.0** | 1 |

- 落刀前**七处的锚点各断言恰好 1 次**，对不上一个字节都不落地（脚本里的 `assert`，见 `§E`）。
- `remote-daemon-proto/Cargo.toml` 的 `version` 现打仍是 `0.0.0` —— **刻意恒定，不参加这次 bump**（`K-R70`）。
- 第七处守它的是门禁 `winchk` 那一格的 `cargo check --locked`
  （`scripts/gate.sh` 那一行逐字带 `--locked`）＋ 发版路上 `release.yml` 的
  `Verify version consistency with tag`（四处对账）—— **不是**那条「六处一致」的 `#[test]`。

**交回这一刻现打**（`python3 evidence/K-R120-ruler.py --places`，从工作树根起跑）：
`六处取到 ['3.8.0'] · 第七处 3.8.0 ⇒ 合起来 1 个不同的值 ['3.8.0']（七处一致）`。

---

## §C `KR120D2` 上半 —— `CHANGELOG.md` 的 `## [3.8.0]` 那一节

**逐字文案住 `CHANGELOG.md:11`–`133`（`c6d3863`）**，本文件不抄第二份（抄一份就会漂）。
这里只放**结构读数**（`python3 evidence/K-R120-ruler.py --changelog`，交回这一刻现打）：

```
- 节标题逐字：`## [3.8.0] — 2026-09-14`（在第 11 行）
- 抠出来的版本号：**3.8.0**
- `src-tauri/Cargo.toml` 的 version：**3.8.0**（锚点命中 1 次） —— 判据比的就是这两个
- 相等吗：✅ 相等
- 那一节的正文 123 行 · 子标题 6 个，顺序是：
    1. ### 六条会改变已有行为的  ← breaking 段
    2. ### 新增 — 用户看得见的
    3. ### 修复
    4. ### 改进 — 本机与远端收成同一条路
    5. ### 项目管理
    6. ### 升级须知
- breaking 段排第 **1** 个（在最前 ✅）
- 地板：全文里带 '会改变已有行为' 的 `### ` 标题 **2** 处（分母 = `CHANGELOG.md` 全部 2379 行；判据只要求 ≥ 1）
```

### §C1 breaking 段那六条 —— 每条写的是「用户会看见什么」

六条与 `§0b` 逐条对得上：删 `daemonless`（`K-R59`）· 删 `cch` 别名 ＋ 不再猜目录（`K-R58`）·
`ccm` 变成后端原生命令 ＋ 删 `shared/ccm`（`K-R48`）· 会话名 `<sid8>-cc` → `<项目名>-cc`（`K-R96`）·
后端自带身份 ⇒ 已装的远端重装一次（`K-R70`）· 删净桌面侧 SSH 回落（`K-R72`）。
每一条的正文都写成「**你看得见的**：…」，而不是「改了什么代码」——
每一条的事实都取自那几个提交自己的提交信息（`8f2f8988` · `cdce27fa` · `3e798cb9`/`e8f9e08e`/`702d0d97` ·
`fe6113f8` · `b6850c3f` · `e206e5b8`），不是凭印象写的。

### §C2 其余按分档挑 —— **分母现打，不抄 `K-R118` 那份**

量具 `evidence/K-R118-ruler.py --commits`，量于 `0a92892`：

| | `K-R118` 量于 `98a8d51` | 本件量于 `0a92892` |
|---|---|---|
| `git rev-list v3.7.0..HEAD` | 242 | **253** |
| 去掉合并 | 160 | **168** |
| 产品面 | 109 | **112** |
| 内部判据与量具 | 27 | **29** |
| 纯文档 | 24 | **27** |

⚠ **分母不同不是漂移** —— 中间并进来了 `K-R116` / `K-R117` / `K-R118` 三支。
⚠ 「产品面」**不等于**「用户可见」：那一层是**人裁的、给不出判别式**
（`K-R118` 交回时逐字写过这句，本件照此办，**不假装它有判别式**）。
那一节最后挑进去的条目分布在五个 `###` 里，**不是 253 条全抄**。

---

## §D `KR120D2` 下半 —— 新那道闸判什么

判据住 `src-tauri/src/doc_claim_registry.rs`（`mod tests` 里，插在那条「六处一致」之后），
名字 `the_changelog_top_section_is_the_version_we_ship`。

**两条判定 ＋ 两条反空真自检**：

1. **最上面那一节标题里的版本号 == `env!("CARGO_PKG_VERSION")`**。
   🔴 **不是**「文件里有没有出现这个串」—— 那个串写在文件任何地方都骗得过去（这是 PM 点名的失效方向）。
2. 最上面那一节里**若有** breaking 段，它必须是**第一个** `###`。
3. 自检：那一节的正文不许是空的（段界读法坏了 ⇒ 下面两条在空转）。
4. 地板：全文里带 `BREAKING_MARK` 的 `###` 标题 ≥ 1（约定被整份抹掉 ⇒ 第 2 条从此零命中地绿）。

### §D1 为什么只跟**一处**比 —— 三段接起来才等于「== 那七处」

| 谁 ↔ 谁 | 由谁守 |
|---|---|
| 权威源 `package.json` ↔ 另外五处 | `the_release_version_is_the_same_in_all_six_places`（同一份文件） |
| `src-tauri/Cargo.toml` ↔ `src-tauri/Cargo.lock` | 门禁 `winchk` 那一格的 `cargo check --locked`；发版路上另有 `release.yml` 的四处对账 |
| **`CHANGELOG.md` 最上一节 ↔ `src-tauri/Cargo.toml`** | **本条**（`env!("CARGO_PKG_VERSION")`，编译期注入） |

⇒ 本条**不抠第二份锚点**（`brief` 13b：同一个值不许长出第二个住址）。
**上面那两行不是背景，是本条结论的承重件** —— 少任何一段，「== 那七处」就不成立。
三段各自的牙由 `d1`（第二段）· `d2`（第一段）· `d3`/`d7`（第三段）分别打过，见 `§E`。

---

## §E 变异表 —— 逐刀真实输出

刀具住 `evidence/K-R120-cut.py`（**被测对象就是本工作树** `.claude/worktrees/k-r120`，
`ROOT = Path(__file__).resolve().parents[1]`）。每一刀：`--apply` 前断言锚点命中数 ⇒
整趟 16 格门禁 ⇒ `--revert`（**重写原文 ＋ `os.utime`**，不用 `copy2`）⇒ `git status --short` 核干净。
**每一趟 `--revert` 之后 `git status --short` 都只剩一行 `?? evidence/K-R120-deathvalue.md`**（本文件）。

| 刀 | 挂 | 切在哪 · 锚点命中 | 门禁裁决 | 红名（逐条） | 绿的格 | 判 |
|---|---|---|---|---|---|---|
| `d1` | `KR120D1` ① | `src-tauri/Cargo.lock` 的 `name = "monitor"\nversion = "`，**1 次**（`3.8.0` → `3.7.0`，另外六处不动） | `GATE: FAIL —— winchk（退出码 101）` | `winchk`，诊断全 2 行逐字 `error: cannot update the lock file … because --locked was passed to prevent this` | **15** | ✅ 已知答案回测 · **最小面**（只红一格） |
| `d1`-v1 | `KR120D1` ① | 同上锚点 **1 次**，但算出来的是 `3.8.-1` | `GATE: FAIL —— winchk；cargo；deadcode`（三格同一句 `failed to parse lock file`） | 三格全红在**解析**上 | 13 | 🔴 **CRASH，不是读数** —— `_patch(cur, -1)` 在 `patch == 0` 时越界（`brief` 第 7 条：类型契约破了 ⇒ 台子炸了）。`_patch` 已改成借位 |
| `d2` | `KR120D1` ② | `package.json` 的 `\n  "version": "`，**1 次**（`3.8.0` → `3.8.1`，另外五处不动）—— 与 `K-R118` `d5` 逐字同一把锚 | `GATE: FAIL —— cargo（退出码 101）` | `1477 passed; 1 failed`，唯一那条 `doc_claim_registry::tests::the_release_version_is_the_same_in_all_six_places`，失败原文**逐处点名**落后的五处 | **15** | ✅ 已知答案回测 · 最小面 |
| `d3` | `KR120D2` ① | 七处锚点各 **1 次**（`3.8.0` → `3.8.1`），`CHANGELOG.md` 一个字节没动 | `GATE: FAIL —— cargo（退出码 101）` | `1477 passed; 1 failed`，唯一那条是**新闸**；诊断逐字「`CHANGELOG.md` 最上面那一节写的是 `3.8.0`，而这棵树要发的是 `3.8.1`」 | **15** | ✅ **本条存在的证据**：`K-R118` 同一刀实打是「16 格全绿、一条没红」 · 最小面 |
| `d4` | `KR120D2` ② | 最上一节里带约定词的 `###` 块，**1 次**（整块删掉第 21–66 行，46 行） | `GATE: OK —— 16 格全绿`（cargo 1614 —— 新闸跑了且过了） | 无 | 16 | ⚠ **不红**。如实登记为「**不在射程**」，理由是判据头注诚实边界第 1 条：机器分不出「这一版真没有破坏性变更」与「有而没写」 |
| `d5` | `KR120D2` ②b | 同一段界，**1 次**（头两个 `###` 块整块对调：`### 六条会改变已有行为的` 46 行 ↔ `### 新增 — 用户看得见的` 22 行） | `GATE: FAIL —— cargo（退出码 101）` | `1477 passed; 1 failed`，唯一那条是新闸的**第二条判定**；诊断逐字「`## [3.8.0] — 2026-09-14` 这一节里，breaking 段排在第 2 个 `### `，不是第一个。」 | **15** | ✅ 第二条判定**有牙** · 最小面 |
| `d6`-v1 | `KR120D2` ③ | 给新闸挂 `#[ignore]`，锚点 **1 次** | `GATE: FAIL —— cargo（退出码 101）` | `1476 passed; 1 failed; 9 ignored`，唯一那条是 `shared_crate_registry::tests::every_ignored_test_still_has_someone_who_triggers_it` | 15 | 🔴 **刀自己招来的红，不是被测对象** —— `#[ignore]` 这条路本仓另有判据在守。阴性对照要的是「一条都不红」⇒ 这一刀**作废**，换 `d6` |
| `d6` | `KR120D2` ③ | 把本轮插进 `doc_claim_registry.rs` 的那**一整块删掉**（两端锚点各 1 次，摘完**整份 md5 == 基点那份** `3f2e8c3c…`，刀自己断言）＋ 刀 `d3` | `GATE: OK —— 16 格全绿`（cargo **1613** —— 回到基线，那 `+1` 正是被摘掉的那条） | 无 | 16 | ✅ **阴性对照成立：一条都不红** —— 这一趟同时在今天这棵树上**重演了 `K-R118` 的 `d7`**（七处全 bump、CHANGELOG 不动、没有那道闸 ⇒ 16 格全绿） |
| `d7` | `KR120D2` 反向 | 七处锚点各 **1 次**（`3.8.0` → `3.7.0`），`CHANGELOG.md` 留在 `3.8.0` | `GATE: FAIL —— cargo（退出码 101）` | `1477 passed; 1 failed`，唯一那条是新闸；诊断逐字「最上面那一节写的是 `3.8.0`，而这棵树要发的是 `3.7.0`」 | **15** | ✅ **两个方向都有牙**（`d3` 是另一个方向） |
| `d8` | 退实现 | 七处各 1 次（`3.8.0` → `3.7.0`）＋ 段界 1 次（删掉 `## [3.8.0]` 整节） | `GATE: OK —— 16 格全绿`（cargo **1614** —— 那道闸在盘上、跑了、过了） | 无 | 16 | ✅ **绿是对的**，理由见 `§F`：它守的是「一致」，退回去之后两侧仍然一致 |

**每一行的分母**：「绿的格」= 那一趟门禁印 `  ok   …` 的行数（口径 = `gate.sh` 自己的判定行，
16 格满分）；「红名」里的 `N passed; M failed` 取自 `cargo test` 自己那行 `test result:`
（分母 = `monitor` 这一个包，不是九包合计）。

---

## §F 「把实现整个退掉，还有多少条新断言仍绿」

本件的「实现」是三块：**(a)** 七处 bump · **(b)** `CHANGELOG.md` 的 `## [3.8.0]` 那一节 ·
**(c)** 那道新闸（`doc_claim_registry.rs` 里插进去的 168 行）。

| 退掉哪几块 | 刀 | 新那条判据 | 为什么 |
|---|---|---|---|
| 只退 (b) | `d3` | **红** | 版本 bump 了而 CHANGELOG 没跟 —— 正是它要守的那一形 |
| 只退 (a) | `d7` | **红** | 反方向：CHANGELOG 超前 |
| 退 (a) ＋ (b)，留 (c) | `d8` | **绿**（`GATE: OK —— 16 格全绿`，cargo 1614） | 🔴 **绿是对的**：它守的是「最上一节 == 那七处」这个**一致性**，不是「必须是 `3.8.0`」。基点 `0a92892` 上七处与 CHANGELOG 最上一节都是 `3.7.0` ⇒ 一致 ⇒ 不该红 |
| 退 (c) ＋ 刀 `d3` | `d6` | 不存在 | 阴性对照：`GATE: OK —— 16 格全绿`，cargo 回到 **1613**，一条都不红 |

⇒ **「仍绿」的那一格（`d8`）不是仪式**：它的牙由 `d3` 与 `d7` 两个相反方向分别证过，
而 `d5` 单独证了第二条判定。**没有任何一条新断言是「怎么切都绿」的。**

---

## §G 改动面

| 文件 | 改法 | 读数 |
|---|---|---|
| `src-tauri/src/doc_claim_registry.rs` | **纯插入**，插在 `mod tests` 里那条「六处一致」之后 | `git diff --numstat` 现打 **168 插入 / 0 删除**；`git diff -U0` 的 hunk 头逐字 `@@ -1914,0 +1915,168 @@ mod tests` |
| 同上 · **强证法** | 把那 168 行**挖掉**之后比整份文件的 md5 | 旧版 `3f2e8c3cf441eb67a3c64437d46fcac2`（3057 行）＝ 挖掉之后 `3f2e8c3cf441eb67a3c64437d46fcac2`（3057 行）⇒ **那一块之外一个字节没动**（`brief` 第 11 条那条更强的证法） |
| `package.json` · `src-tauri/Cargo.toml` · `src-tauri/tauri.conf.json` · `src-tauri/Cargo.lock` | 各 1 行 | `3.7.0` → `3.8.0` |
| `README.md` | 2 行 | 同上（抬头 ＋「项目当前状态」） |
| `README.en.md` | 1 行 | 同上 |
| `CHANGELOG.md` | **纯插入** 124 行（`## [3.8.0]` 那一节 ＋ 分隔线） | `git diff --numstat` 124 插入 / 0 删除 |
| `evidence/K-R120-ruler.py` · `evidence/K-R120-cut.py` | 新建（原来是 27 字节的占位） | 量具 ＋ 刀 |

**没有一个 Rust 函数被改**（本件对 `doc_claim_registry.rs` 是纯插入）⇒ 逐函数 `ast` md5 这一栏
在本件退化成上面那一条「挖掉新增块之后整份 md5 相等」，它比逐函数更强（一次证明「那一块之外一个字节没动」）。

---

## §H 诚实边界（逐条，别读宽）

1. 🔴 **「该不该有 breaking 段」判不了，也没判** —— `d4` 实测：整块删掉那 46 行，16 格全绿。
   **登记为「不在射程」，不是「做到了」**。理由住判据头注诚实边界第 1 条：机器分不出
   「这一版真没有破坏性变更」与「有而没写」；硬要求每节都有 ⇒ 变成谁都会写的一句空话。
2. breaking 段靠 `BREAKING_MARK` 这个**约定词**认。**换一种说法另起一节 ⇒ 判据静默。**
   挡这一形的只有那条「全文里带它的 `###` ≥ 1」的地板 —— 它保证不了
   「**最上面那一节里**那一处还在」。这是已知的漏，不是「没有」。
3. 本条只读 `CHANGELOG.md` 一份文件 ＋ 一个编译期常量。**那一节里写的内容对不对、全不全**
   （六条是不是真六条、有没有漏一条、写的是不是「用户会看见什么」）一个字都不判。
4. **「== 那七处」是三段接起来的**（`§D1`），不是本条一条判出来的。
   `winchk` 那一格哪天被拿掉 / `--locked` 被去掉 ⇒ `Cargo.lock` 那一段当场失守，而本条不会说话。
5. 顶上挂 `## [Unreleased]` 会让本条**红**。本仓至今没用过那种写法 ⇒ 这里**刻意不设豁免档**
   （设了就是一条没有夹具的分支）。真要用：去 `doc/RELEASING.md` 把发版次序重裁，别在判据里加一行豁免。
6. `d1` 那一趟里 `cargo` 那一格**会把 `Cargo.lock` 重新写回去**（它不带 `--locked`）——
   本件的读数不受影响（`winchk` 在它之前跑、且 `--revert` 重写原文），但**别把「`d1` 之后 cargo 仍绿」
   读成「lock 不一致 cargo 也不管」**：那一格是先被 cargo 自己修好了才绿的。
7. 本件**没有**买到「这棵树发得出产物」，也**没有**推 tag、没有发版 —— 那是 `K-R119`。
8. `CHANGELOG` 那一节里的 `253 / 392` 与 `112 / 29 / 27` 都**量于 `0a92892`**；`HEAD` 每轮都变，
   下一轮复打会得到别的数，**那不是漂移**。
9. 「产品面 112 条」里哪几条「用户可见」是**人裁的**，给不出判别式（`K-R118` 交回时逐字写过）。
   ⇒ 那一节挑了哪几条，**没有机检在守**。

---

## §I 三处 `git status`（交回这一刻现打）

- **代码仓主树** `/home/zbl/文档/claudecode-frontend/cc-monitor` —— 本件一个字节没碰它。
- **本工作树** `.claude/worktrees/k-r120` —— 分支 `track/k-r120`，基点 `0a92892`，
  **没合 main、没推 tag**。
- **计划仓** `.claude/planned-build/backend-consolidation` —— 我写的只有件文件那一份，
  **只写不提交**（固定项第 3 条）。

逐字读数写在件文件 `features/K-R120-版本号3.8.0与CHANGELOG.md` 的 `§8` 第 14 条
（那一份是交回那一刻现打的，本文件不抄第二份）。

---

## §J 本件**没做**的

- **不推 tag、不发版**（`§0d`，那是 `K-R119`）。`git tag` 一次都没跑过。
- 不碰 `.github/**` · `scripts/gate.sh` · `src/**` · `remote-daemon-proto/**` —— 现打 0 字节。
- 不改任何功能代码：本件唯一的 Rust 改动是往 `mod tests` 里**插了一条判据**。
