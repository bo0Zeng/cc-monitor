# G-B — vendored bash 进门禁（shellcheck + 它自己的测试进 CI）

> 主计划：`../MASTERPLAN.md` §1 G-B · 账本第 4、5 行
>
> **它是整个 `account-zero` cc-acct-iso 半区的硬前置**（Z01/Z04/**Z06/Z08**）。
> 理由：那 1348 行 bash 被 `include_bytes!` 打进二进制、部署到远端执行，却在 shellcheck
> 门禁之外，它自己那 424 行测试**从没跑过**。**没有网不能改那个工具。**

## 1. 开工前的实测（不照抄主计划）

主计划账本第 4/5 行写了两条待验证的断言，本轮逐条复测：

| 主计划的说法 | 实测结果 | 判定 |
|---|---|---|
| 「实测今天扩进来零告警」 | `--severity=error`（CI 现用档）**0 告警、rc=0**；**默认档（含 warning/info/style）也是 0** | **成立，且比声称的更干净**。准确说法：脚本内自带 `# shellcheck disable=SC2016,SC2034,SC2329,SC2012` 等指令，在此前提下全档零告警 |
| 「先跑一次看今天是否绿——若今天就红，那是 G-B 的第一个发现」 | `run-tests.sh` **171/171 全绿、rc=0**（**有史以来第一次运行**） | 绿。那个「若今天就红」的假设**没命中** |
| 行数 1348 + 424 | `cc-acct-iso` 748 + `lib.sh` 534 + `cc-acct-iso-install.sh` 66 = **1348** ✓；`run-tests.sh` **424** ✓ | 数字全对 |

### ★ G-B 真正的第一个发现：主计划写的 glob pattern **直接用会红**

账本第 4 行把文件清单写成：

```
e2e/*.sh shared/cc-bus/scripts/* shared/ccm src-tauri/vendor/cc-acct-iso/scripts/**
```

`scripts/**` 在 **不开 globstar 的 bash 里等价于 `scripts/*`** ⇒ 会把
`scripts/test`（**一个目录**）喂给 shellcheck：

```
src-tauri/vendor/cc-acct-iso/scripts/test: openBinaryFile: inappropriate type (is a directory)
rc=2
```

⇒ 照账本原文抄进 `ci.yml` 会让这一步**恒红**。改成显式四个文件路径。

### 另一条实测事实：覆盖面 32 → 36 个文件

```
今天：e2e/*.sh(18) + shared/cc-bus/scripts/*(13) + shared/ccm(1) = 32
加上：scripts/cc-acct-iso + scripts/lib.sh + scripts/cc-acct-iso-install.sh
      + scripts/test/run-tests.sh                                  = 36
```

## 2. DoD

- [x] 开工前逐条复测主计划的两条断言（§1）
- [x] `ci.yml` 的 shellcheck 清单扩到 vendored 四个文件（**不用 `**` glob**）
- [x] **覆盖面地板**：文件数少于实测值即红 —— 防「改名/挪走 ⇒ 少扫几个文件 ⇒ 照样绿」，
      这正是本工作区在治的那个病（零断言报绿）
- [x] `run-tests.sh` 进 CI 一步，**带断言条数地板**（同上理由：`F=0` 就 exit 0，
      零断言也会绿）
- [x] **只追加、不重排** `ci.yml` 既有步骤（§3 共享面 1 的跨工作区协议）
- [x] **不改任何触发条件**（在册红线）
- [x] **不改 `src-tauri/vendor/`**（VENDOR.md SS-10）——见 §3 那条刻意偏离
- [x] 变异验收：地板值双向（改小覆盖面必红 / 改小断言数必红）
- [x] 本地用 CI 原样命令跑一遍，全门禁数字不降

**明确不做**：不改那 1348 行 bash 一个字符（本功能只是**给它建网**，改它是 account-zero 的事）·
不做 G-A（八套地板）与 G-C（6 套 e2e 进 CI）——各自独立功能 · 不动 `npm test` 链

## 3. 一处刻意偏离主计划 §2「同源」原则，附论证

主计划 §2 定了：**地板值只写在套件脚本里一处**，`ci.yml` 的标签指向它 + 加一条对拍。
那是给**我们自己拥有的**八套套件设计的。

**`run-tests.sh` 是 vendored 的**，`src-tauri/vendor/cc-acct-iso/VENDOR.md` 的 SS-10 铁律：

> **副本是上游的镜子，不是分身**：**只照上游改，绝不在副本里改出自己的版本**。

往副本里加 `[ "$T" -ge 171 ]` ⇒ 要么违反 SS-10，要么触发一次上游 lockstep
（改 `~/.claude/skills/cc-acct-iso/scripts/test/run-tests.sh` + 按菜谱重算 `.vendor_id`
= 6 个文件按固定顺序 sha256）。用户**已授权**动上游，但那属于「改那个工具」，
**正是 G-B 要先建网再做的事** ⇒ 在 G-B 里做等于绕过自己建的网。

⇒ **地板放 CI 侧**（grep 它的输出拿断言数并断言 ≥ 171）。
**这是范围略窄于理想的取舍，如实记录**：脚本自己仍然可能被改成少跑几条而 CI 侧地板不变，
但那需要有人同时改上游 + 重 vendor + 不动 CI 地板 —— 而 `.vendor_id` 变了 `build.rs` 会
`cargo:warning`，且 account-zero 改那个工具时必然会重跑这条 CI 步骤。
**上游 lockstep 那条路留给 account-zero Z06/Z08**（它们本来就要改上游 + re-vendor）。

## 4. 实现（Phase C）

`ci.yml` 的 `e2e-smoke` job：**既有步骤相对顺序一字未动**，新步骤插在 shellcheck 之后（取局部性，
两条 shellcheck 家族的检查挨着）。触发条件零改动（`git diff` 已核：无 `on:`/`push:`/`branches` 命中）。

**改动 1：shellcheck 清单 + 覆盖面地板**

```yaml
FILES=$(printf '%s\n' e2e/*.sh shared/cc-bus/scripts/* shared/ccm \
  src-tauri/vendor/cc-acct-iso/scripts/cc-acct-iso \
  src-tauri/vendor/cc-acct-iso/scripts/lib.sh \
  src-tauri/vendor/cc-acct-iso/scripts/cc-acct-iso-install.sh \
  src-tauri/vendor/cc-acct-iso/scripts/test/run-tests.sh)
N=$(printf '%s\n' "$FILES" | grep -c .)
[ "$N" -ge 36 ] || { echo "覆盖面缩水：只有 $N 个文件（地板 36）"; exit 1; }
shellcheck --severity=error $FILES
```

**改动 2：vendored 自测进 CI + 断言条数地板**

```yaml
OUT=$(bash src-tauri/vendor/cc-acct-iso/scripts/test/run-tests.sh 2>&1) || { echo "$OUT"; exit 1; }
N=$(printf '%s' "$OUT" | grep -oE '全绿:[0-9]+' | grep -oE '[0-9]+' | tail -1)
[ "${N:-0}" -ge 171 ] || { echo "断言条数缩水：${N:-0}（地板 171）"; exit 1; }
```

**为什么两条都要地板**：这两个检查的天然失效模式都是**静默缩水而非报错**——
shellcheck 少扫几个文件照样 rc=0；`run-tests.sh` 的退出码只看失败数 `F`，
**`F=0` 就 exit 0 ⇒ 一条不跑也会绿**。这正是本工作区存在的理由。

**本地用 CI 原样命令跑过**：步骤 1 报「覆盖 36 个文件」rc=0；步骤 2 报「断言条数 = 171」rc=0。

## 5. 变异验收（Phase D）

**强度：低风险**（只改 CI 配置 + 零源码改动）⇒ 主线程变异 + 全门禁。
（**如实标注**：planned-build 铁律 8 要求 Phase D 并行多 agent 审计；本会话有一条常驻指令
「除非用户要求，不开 agent」⇒ 全程用主线程变异代替。这是**欠了铁律 8 的账**，不是强度裁剪。）

| 变异 | 做了什么 | 结果 |
|---|---|---|
| **A** | 只给 3 个 vendored 文件（覆盖面 36 → 35），模拟"有人把脚本挪走/改名" | **成立**。`覆盖面缩水：只有 35 个文件（地板 36）` rc=1 |
| **B** | 照抄主计划账本的 `scripts/**` pattern | **成立**：`openBinaryFile: inappropriate type (is a directory)` rc=2 ⇒ 实证账本原文会让这一步**恒红** |
| **C** | 把 `run-tests.sh` 收尾的条数报告从 `$T` 改成常量 3（`F` 仍为 0 ⇒ **脚本自己照样 exit 0**） | **成立**。`断言条数缩水：3（地板 171）` rc=1 ⇒ 地板确实挡住了「脚本自己报绿但少跑」这条路 |

**变异 C 之后逐字节复原**：`git diff --numstat src-tauri/vendor/` = **0 行**；
且与上游四个文件逐个 `diff -q` 全部一致（`build.rs::check_acct_iso_vendor_freshness` 看的就是这个）
⇒ **SS-10 未破，`.vendor_id` 未动**。

**全门禁**：`cargo test --all` 611 · daemon 140 · npm 837/56 · tsc 0 · eslint 7 项既有基线 ·
`exec-bit-guard` rc=0 · `py_compile` rc=0 · **新增两步本地 rc=0**。`ci.yml` 经 `yaml.safe_load`
解析：6 个 job 不变、`e2e-smoke` 4→5 步、触发条件逐字未变。

## 6. 工程审计（Phase E）

### 6.1 账本对账

| 账本行 | 本功能做了什么 | 状态 |
|---|---|---|
| **1 `ci.yml`** | 只**追加**（既有步骤相对顺序未动）、**不改触发条件** | ✅ 守住跨工作区协议（`rust-ts-boundary` C05 已先改过它） |
| **4 shellcheck 清单** | 扩到 vendored 四个文件；**账本原文的 `**` pattern 被实测证伪、已改成显式清单** | ✅ 到位 + 订正账本 |
| **5 `run-tests.sh` 进 CI** | 进了，**带断言条数地板** | ✅ 到位。地板位置偏离 §2「同源」，论证见 §3 |
| 2 八套收尾段 | 未触及（G-A 的事） | — |
| 3 `package.json` | 未触及（G-C 的事） | — |

### 6.2 对后续的影响

**① `account-zero` 半区的网建好了。** Z01/Z04/Z06/Z08 现在有两道：改那 1348 行 bash 会过
shellcheck，改坏行为会被那 171 条断言抓。**这就是「没有网不能改那个工具」的解除条件。**

**② 给 Z06/Z08 留了一条明确的后续动作**：它们本来就要改上游 + re-vendor（按 `VENDOR.md`
菜谱重算 `.vendor_id` = 6 个文件固定顺序 sha256）。**届时应顺手把断言地板搬进脚本自己**
（`[ "$T" -ge <N> ]`），从而补上 §3 那处刻意偏离、达成主计划 §2 的「同源」。
**已写进本文件与主计划变更记录，别忘。**

**③ G-A 的一条前置事实**：`run-tests.sh` 的计数变量是 `T`（总数）/ `F`（失败），
与八套自有套件的 `PASS`/`FAIL` 命名不同。G-A 统一命名时**不要把 vendored 这套算进去**
（SS-10）。

### 6.3 一处如实标注的残留

地板在 CI 侧而不在脚本里 ⇒ 理论上「同时改上游脚本少跑几条 + re-vendor + 不动 CI 地板」
能穿过。缓解：`.vendor_id` 变了 `build.rs` 会 `cargo:warning`，且改那个工具时必然重跑这步。
**不假装这是严格证明**（同 `readonly_guard` 头注那种「纵深防御、非严格证明」的自陈风格）。

## 7. 签收

- [x] 通过代码审计（低风险档：三条变异全成立 + 本地跑 CI 原样命令 + vendored 逐字节复原核实）
- [x] 通过工程审计（账本 3 行到位、订正账本第 4 行的 pattern、给 Z06/Z08 留下补「同源」的动作）
- [x] 主计划已据此更新
