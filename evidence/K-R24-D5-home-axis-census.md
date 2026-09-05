# K-R24 下一拍㈡ · `HOME` 这条轴上的人群

> 量于 `fbc32d3`（分支 `track/k-r24b`）· 沙箱 `ccmon-devbox:latest`（`sha256:553f5932…`，建于 09-03T11:17）
> · 四格一律 `--network none` · 本树**未铺** `src-tauri/embedded-daemons/`
> · 量具 `evidence/K-R24-home-axis-ab.py`（`all` 跑读数 · `nff` 补缺口 · `diff` 对拍）

## §1 尺子（逐字沿用上一拍那把，**只换轴**）

> 一条判据，它的**判决**随一条**环境事实**翻转，而它**既不建立、也不检查**那条事实。

判法：**同一棵树、同一个提交、同一个镜像、同一份挂载、同一个网络口径，只换 `HOME` 这一条事实，
看哪些判决翻转。** 🔴 刻意**不切成**「代码里出现了 `HOME` / `dirs::home_dir()`」——
那是词表的形状，只认它记得的写法。

**为什么是这条轴**：`.claude/devbox/gate` 头注**自己写着**〔`K-H2b` `D9` 08-29〕——
容器里 `HOME=/home/zbl` **存在但几乎是空的** ⇒ 任何读真实家目录的判据，沙箱与宿主未必同值。
**仓里有这笔账，但从没人拿尺子量过它上面有几条。**

## §2 四格怎么切的

| 格 | `HOME` | 复刻 gate 那句 `mkdir -p "$HOME/.claude/projects"` |
|---|---|---|
| **A**（基线 = `.claude/devbox/gate` 逐字那套） | `/home/zbl` | 有 |
| **B** | `/home/zbl` | **无** |
| **C** | `/tmp/home-elsewhere` | 有 |
| **D** | `/tmp/home-elsewhere` | **无** |

⚠ C/D 那一格其实动了**两样**：家目录的**路径**变了，而且项目不再落在它下面。
**这是这条轴本身的形状，记在这里，别读成只动了路径。**

## §3 读数一 · 门禁九格（`npm` / `e2e` / `pb check` 只到**格**这一级）

| 格 | cargo | generated | daemon | npm | ccm e2e ×4 | `pb check` |
|---|---|---|---|---|---|---|
| **A** | ok **1387** | ok | ok **538** | ok **1541** | ok 12/8/242/68 | 🔴 `FAIL=16` |
| **B** | 🔴 **退出码 101** | ok | ok 538 | ok 1541 | ok 12/8/242/68 | 🔴 `FAIL=16`（同 A） |
| **C** | ok 1387 | ok | ok 538 | ok 1541 | ok 12/8/242/68 | 🔴 **换了一种死法**（见 `§6`） |
| **D** | 🔴 退出码 101 | ok | ok 538 | ok 1541 | ok 12/8/242/68 | 🔴 同 C |

★ **八个代码格里，只有 `cargo` 那一格随这条轴翻**，而且**只随 `mkdir` 那一维翻**，
不随「家目录在哪」翻。`npm` 与四套 `e2e` 四格**逐格同值**（e2e 那四套是 `exact` 口径，
任何一条翻转都会改数 ⇒ 它们这一格的「没翻」是判得住的）。

⚠ `pb check` 那 `FAIL=16` **一条都不是本件的**：15 条是 `[J1 字段名/acceptor]`，住
**别人的** `drafts/K-W2E.md`；1 条是 `[J3 陈账] INDEX.md 比源文件旧`，清它要跑 `pb index`
—— **那一步归 PM，本拍被明令禁止碰**。⚠ 它还是个**移动靶**：同一棵树同一个提交，
18:34 那趟是 `FAIL=1`，18:40 那趟就是 `FAIL=16` —— **9 路在同时写这个共享计划仓。**

## §4 读数二 · Rust 两格的**逐条**对拍（这一维才看得见是哪一条翻的）

**分母 = 1932**（格 A 逐条采到的不同判据名）。四格**都采到 1932，零缺口**。

| 对拍 | 采到 | **翻转** | 只在一边有 |
|---|---|---|---|
| A ⇄ **B** | 1932 | **1 条** | 0 |
| A ⇄ **C** | 1932 | **0 条** | 0 |
| A ⇄ **D** | 1932 | **1 条**（与 B 同一条） | 0 |

**那唯一一条**：`history::tests::the_delete_entry_point_actually_goes_through_the_fence`
（`src-tauri/src/history.rs:3453`）`ok → FAILED`。

### 🔴 那个 1932 是怎么来的（**别把它当成 1925**）

- `A.cargo.log`：**1397** 个不同名字 = `1387 ok + 10 ignored` —— 与门禁 `cargo` 那格的 **1387** 对得上。
- `A.daemon.log`：539 行带 ` ... `，其中 538 行干净收尾、1 行是 `ignored, <理由>`（本量具不认它）；
  而那 538 行只有 **535 个不同名字** —— **3 个名字各出现两次**
  （`inbound::structure_guards::declaring_zero_fields_needs_a_reason` ·
  `observe::history_query::tests::path_resolution_has_exactly_one_home` ·
  `platform::fallback_guard::tests::the_derived_population_still_covers_every_historically_known_form`）。
  成因：这个二进制里有**把自己当子进程再拉起来**的判据（同一份日志里就有
  `relay::server::tests::relay_child_process_entry_point ... ignored, 子进程入口：只在被父判据用
  CCM_RELAY_TEST_CHILD 拉起时才当中转跑`）⇒ **子进程的 harness 输出回灌进了同一份 stdout**。
  ⇒ **这份日志里「行数」不等于「判决数」。**
- ⇒ `1397 + 535 = 1932`。

★ **门禁那趟真判过的判决合计（沿用上一拍口径）= 3796** = cargo 1387 + daemon 538 + npm 1541
+ ccm e2e 330（12+8+242+68）。**逐条这一维只覆盖 Rust 那两格**；
`npm`(1541) 与 `e2e`(330) 那一半**只判到「格」**，没判到条。

## §5 量具自检（非空对照 —— 不然「C 翻转 0 条」可能是轴根本没动）

🔴 **「C 一条都没翻」这个读数，只有在「轴真的动到了 Rust 那一侧」时才算数。**
本量具**没有靠「`dirs::home_dir()` 应该读 `$HOME`」这句话承重**，而是拿现成的报文对拍：

| 格 | 那条翻转判据报文里的路径 |
|---|---|
| **B**（`HOME=/home/zbl`） | `canonicalize /home/zbl/.claude/projects: No such file or directory` |
| **D**（`HOME=/tmp/home-elsewhere`） | `canonicalize /tmp/home-elsewhere/.claude/projects: No such file or directory` |

⇒ **Rust 那一侧确实跟着 `$HOME` 走。** ⇒ **格 C 的 0 是真的 0**，不是「轴没动出来的 0」。

## §6 那唯一一条，按尺子的三个词逐个判

尺子三个词：**判决翻转** ∧ **不建立** ∧ **不检查**。

| 词 | 判 | 依据（现打） |
|---|---|---|
| **判决翻转** | ✅ **是** | `ok → FAILED`，见 `§4` |
| **建立前提** | ❌ **否** | 建立它的是 `.claude/devbox/gate` 那句 `mkdir -p "$HOME/.claude/projects"` —— **在判据之外**，而且住在一个**没有版本控制**的文件里（`gate` 头注自己写着这一点） |
| **检查前提** | ✅ **是** | 它红出来的话逐字是「拒绝了，但**不是围栏的越界检查**拒的（错误：`canonicalize … No such file or directory`）—— 要么围栏没接上而下游某步偶然报了错（**两者长得一样，只有这句话分得开**），要么围栏的措辞改了而本条没跟」⇒ **原始成因原样带进了报文，而且它没替人断因**（用的是「要么…要么…」） |

### ⇒ 两个数，都要报，别只报一个

- **按尺子逐字（翻转 ∧ 不建立 ∧ 不检查）：`HOME` 这条轴上人群 = 0。**
- **按「判决随这条轴翻转」这一维：1 条。**

★ 差在**「检查」那一格**：这一条正是 `K-R24` `D2` 当初那两条出路里的**乙（检查前提）**已经落地的样子 ——
它**判不了**的时候会说「我判不了」，而不是说「被测的东西坏了」。
⚠ 但它**没有走甲（建立前提）**：前提由**判据之外的、没有版本控制的**那句 `mkdir` 建立
⇒ 换一个不跑那句 `mkdir` 的地方（比如直接 `cargo test`，或本册这个格 B），它当场红。
**⇒ 这不是「已经清干净了」，是「红得说得清」。**

## §7 顺手量到的第九格：`pb check` 自己有一条**没人建立也没人检查**的 `HOME` 前提

格 C/D（`HOME=/tmp/home-elsewhere`）里 `pb check` 那一格死法逐字变成：

```
python3: can't open file '/tmp/home-elsewhere/.claude-accts/z/skills/planned-build/bin/pb.py'
```

成因：`scripts/gate.sh` 用 **`$HOME/.claude-accts/z/…`** 去找 `pb.py`，
而 `.claude/devbox/gate` 是用**宿主那个字面路径**把它挂进来的（`-v "$SKILL:$SKILL:ro"`）。
⇒ **同一个文件被两种方式命名，而两者只在 `HOME=/home/zbl` 时才相等。**

⚠ **如实记边界，别把这条算进上面那个人群**：它是**门禁的一格**，不是一条 `#[test]`；
上一拍已经写明「`pb check` 性质不同，不并进这个数」。**这里只登记，不并入。**

## §8 🔴 射程（别把这个数读大）

- 本册实打**只换了 `HOME` 一条轴**，而且只在**沙箱内部**换。
  「沙箱 ⇄ 宿主」那一对**没量**（红线：宿主上不跑任何测试）——
  而 `gate` 头注说的不等价正是那一对。⇒ **本册答的是「容器内换 `HOME` 会翻几条」，
  不是「沙箱与宿主差几条」。**
- `npm` 与 `e2e` 那 1871 条只判到**格**，没判到条。
- 其余轴（外部二进制 · 有没有非回环地址 · locale · 时区 · 用户 uid …）**一条都没量**。
- ⇒ 结论逐字：**「`HOME` 这条轴上，按尺子逐字人群 = 0；按『判决翻转』这一维 = 1，
  分母 1932（Rust 逐条）/ 3796（门禁那趟全部判决）」**，
  **不是**「这一族清完了」，也**不是**「`HOME` 这条轴清完了」。
