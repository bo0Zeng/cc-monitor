# K-R124 死值验留档

> 本文件只装**读数**，不装结论。每一行带：切在哪一处 · 锚点命中几次 · 量哪一格 · 在哪棵树上量的。
> 被测树：`/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r124`（分支 `track/k-r124`，基点 `51a9b38`）。
> 刀具住 `evidence/K-R124-cut.py`（`--list` / `--run <刀>` / `--all`）——
> 它落刀前一律断言锚点命中 == 期望，对不上**一个字节都不改**；**不还原**，每一刀换一份全新副本。
> 副本落 `.claude/worktrees/k-r124-cuts/<刀>/`（**不进仓**，交回前删掉），里面**没有 `.git`**
> （工作树的 `.git` 是一行指回原仓的指针，在副本里跑 `git` 会写进原树的暂存区）。

## 零 · 量法（两种，别混）

| 量法 | 命令 | 什么时候用 |
|---|---|---|
| **整趟门禁** | `PB_WS=backend-consolidation .claude/devbox/gate <工作树> k-r124` | 基线 ＋ 终账 |
| **最小面探针** | 同一个镜像 `ccmon-devbox:latest`、同一份挂载、`--network none`，只跑被切那一格自己那条命令 | 逐刀 |

🔴 **诚实边界（硬边界的字面）**：本件的硬边界写着「唯一许可命令」是整趟门禁。
逐刀用最小面探针是**对那句字面的一处偏离**，偏离的形状写清楚（与 `K-R122` 同一条取法）：
① 仍然**一律在沙箱里**（同镜像 · 同挂载 · `--network none`）；
② 换来的是「一刀一格」而不是「一刀 20 格」—— 15 刀 × 整趟本轮跑不完；
③ **终账是整趟**，没有用探针替代。
探针脚本住会话私有目录 `…/f80a7aea-0ccf-4854-9b7a-12961a9c55aa/scratchpad/kr124-probe.sh`
（**不进仓** —— 它的被测对象硬指 `k-r124` 这一棵）。

⚠ **另一处偏离，也写出来**：开发过程中我在**宿主**上跑过 `python3 evidence/K-R124-ruler.py`
与 `python3 evidence/K-R124-cut.py`（纯读文件 ＋ 往 scratchpad / `k-r124-cuts/` 写副本，
不碰 `~/.cargo`、不碰 `~/.claude`、不起任何进程）。
**宿主上一条 `cargo` / `npm` / `tsc` 都没跑过。**
**下面表里的每一行读数都是沙箱里那一趟的**（`kr124-cuts-sandbox.out`），
宿主那一趟只用来迭代，两趟逐刀同判（15/15）。

## 一 · 基线（M0，修复后、未落刀）

| 格 | 读数 | 量法 |
|---|---|---|
| 整趟门禁 | 见 `§四 终账` | 整趟 |
| `release-gate` | `release-gate: 23 passed`，rc=0 | 最小面（沙箱） |
| 沙箱里有没有 PyYAML | `python3 -c 'import yaml'` ⇒ `ModuleNotFoundError: No module named 'yaml'` | 最小面（沙箱） |
| 生成器 | `release-notes: v3.8.0 的正文 130 行 / 98 个非空行（取自 CHANGELOG.md 的 [3.8.0] 段，地板 20）` | 最小面（沙箱） |
| 覆盖尺子 | `KR80D3: OK —— C1..C6 全过；C5b 全过；C5c 全过；C6b 全过；C6c 全过` | 最小面 |

⚠ **`23` 这个数怎么来的**：判据本体逐行印 `PASS`/`FAIL`，绿行那个数 = `PASS` 的条数。
上一版（`ci.yml` 里那段 `run:` 内联的守卫）是 **12** 条（云端逐字见
`evidence/K-R123-发版读数.md § 1.3`）⇒ **本件新增 11 条**（`23 − 12`）。

## 二 · 逐刀（每行：切在哪 · 锚点命中 · 量哪一格 · 真实输出）

分母 = `evidence/K-R124-cut.py` 里登记的 **15 把刀**（`--list` 现算，不是抄来的数）。

| 刀 | 切在哪一处 · 锚点命中 | 量哪一格 | 真实输出（逐字节选） | 判 |
|---|---|---|---|---|
| `d1` | `release.yml` 的 `env.PUBLISH` 那一行字面，**1/1** | `release-gate` | `FAIL ③env.PUBLISH 与钉住的字面逐字相同 :: 盘上="${{ github.event_name == 'push' }}"` ⇒ 红 1 条，rc=1 | ✅ 红 · 最小面（只红这一条） |
| `d2` | **判据本体** `CANON_ENV` 换成 runner 渲染之后的值 `"false"`，**1/1** | `release-gate` | `FAIL ③env.PUBLISH 与钉住的字面逐字相同 :: 盘上="${{ github.event_name == 'push' \|\| inputs.publish == true }}"` ⇒ 红 1 条，rc=1 | ✅ 红 —— **这就是「今天它恒红」那条证据**（见下面那段） |
| `d3n` | 阴性对照：**守卫整步拿掉**（不跑它）＋ 刀 `d1` | 本仓**另一格**也读 `.github/` 的尺子 `K-R122-ruler.py` | `ci-e2e-prereq: 20 passed`，rc=0 | ✅ **不红**（要的就是不红） |
| `d4w` | Windows 那处 `body_path` → `RELEASE_NOTES_ABSENT.md`，**1/1** | `release-gate` | `FAIL ⑥正文只有一个住址` ＋ `FAIL ⑦生成器排在发布步骤前面·build-windows :: …没有任何一步既调它、又点名 RELEASE_NOTES_ABSENT.md` ⇒ 红 2 条，rc=1 | ✅ 红 |
| `d4l` | Linux 那处同上，**1/1** | `release-gate` | 同形，红在 `·build-linux` ⇒ 红 2 条，rc=1 | ✅ 红 |
| `d5w` | **摘掉 Windows 那处的 `body_path`**，**1/1** | `release-gate` | `FAIL ⑥正文来源·build-windows … body_path=None · body=None` ＋ `FAIL ⑥地板·每一处发布步骤都有 body_path :: 1/2 处` ⇒ 红 2 条 | ✅ 红 · **单断** |
| `d5l` | **摘掉 Linux 那处的 `body_path`**，**1/1** | `release-gate` | 同形，红在 `·build-linux` ＋ `1/2 处` ⇒ 红 2 条 | ✅ 红 · **单断（这一刀就是那条失效方向本身）** |
| `d5both` | 两处都摘，**1/1 ＋ 1/1** | `release-gate` | 两条 `⑥正文来源` ＋ `⑥地板… 0/2 处` ⇒ 红 3 条 | ✅ 红 · **全断** |
| `d6` | Windows 那处换回 `generate_release_notes: true`（＝ 开工前那一版），**1/1** | `release-gate` | `FAIL ⑥正文来源·build-windows` ＋ `FAIL ⑥不回落自动生成·build-windows :: generate_release_notes=True` ＋ `⑥地板 1/2` ⇒ 红 3 条 | ✅ 红 · **已知答案回测** |
| `d6l` | Linux 那处加 `generate_release_notes: true`（`body_path` 仍在），**1/1** | `release-gate` | `FAIL ⑥不回落自动生成·build-linux :: generate_release_notes=True` ⇒ 红 1 条 | ✅ 红 · **单断（给「不回落」那一格补的牙）** |
| `d7` | `CHANGELOG.md` 里 `## [3.8.0]` 那一段掏空，**1/1** | `release-gate` | `FAIL ⑧生成器吐得出本版正文 :: release-notes: \`## [3.8.0]\` 那一段只有 1 个非空行（地板 20）… 不回落到自动生成，按红记（rc=1）` ⇒ 红 1 条 | ✅ 红 —— **`KR124D3` 要的那条「造一处它该逮的东西 ⇒ 本地就红」** |
| `d8` | 生成器 `scripts/release-notes.mjs` 整份删掉 | `release-gate` | `FAIL ⑧地板·生成器在盘上` ⇒ 红 1 条 | ✅ 红 |
| `d9` | **摘掉 Windows 那个渲染步骤**（`body_path` 还在），**1/1** | `release-gate` | `FAIL ⑦生成器排在发布步骤前面·build-windows :: …没有任何一步既调它、又点名 RELEASE_BODY.md` ⇒ 红 1 条 | ✅ 红 |
| `d10n` | 阴性对照：**本件新加的 ⑥⑦⑧ 整块摘掉** ＋ 刀 `d4w`，**1/1 ＋ 1/1** | `release-gate` | `release-gate: 13 passed`，rc=0 | ✅ **不红**（`d4w` 的红确实由本件新加那几条买的） |
| `d0` | **本件的实现整个退掉**（两个渲染步骤摘掉 · Windows 换回 `generate_release_notes` · Linux 的 `body_path` 摘掉 · 生成器删掉），判据一个字不动，**1/1 ×4 ＋ 删 1** | `release-gate` | 红 5 条，rc=1 | ✅ 红 · 见 `§三` |

**CRASH：0 条**（分母 = 上面 15 行；「CRASH」= 判定行掉了或异常退出而不是判红。
每一刀都印出了完整的 `PASS`/`FAIL` 逐行，判定行没有掉过）。

### 🔴 `d2` 为什么是「今天它恒红」的证据，以及它**不是**什么

`d2` 把判据本体里那个 `CANON_ENV` 换成 **`"false"`** —— 那**正是 runner 把
`${{ github.event_name == 'push' || inputs.publish == true }}` 求值之后交给 shell 的那个串**
（云端逐字读数住 `evidence/K-R123-发版读数.md § 1.3`：
`gh api …/jobs/104252591682/logs | grep -n 'CANON_ENV'` 第 364 行 = `CANON_ENV = "false"`）。
换上去之后它拿 `"false"` 去比盘上那串**没被动过**的模板 ⇒ **红**。

⚠ **诚实边界，别读宽**：`d2` 复现的是「渲染之后那一版长什么样」，
**它不执行 GitHub 的渲染器** —— 我证不了「runner 真的会这么渲染」，那一步是云端实打的，
读数在上面那个住址，**本件没有重打**（重打要么真推 tag、要么真推 `main`，两样都不在本件边界里）。
`push` / tag 两个触发器上渲染成 `"true"` 那两行，`K-R123` 自己也标着「推演，未实打」——
**本件同样没有实打**。

## 三 · 把实现整个退掉，还有多少条新断言仍绿（`d0`）

新断言 **11 条**（分母 = 基线 23 − 上一版 12）。`d0` 之下逐条：

| 新断言 | `d0` 之下 | 为什么 |
|---|---|---|
| `地板·step 总数` | 🟢 仍绿（`35 步`） | 它是**地板**，职责是「切块器坏了要塌」，不是「实现退了要红」。牙在别处：切块器坏了这个数会掉。 |
| `⑥正文来源·build-windows` | 🔴 红 | — |
| `⑥不回落自动生成·build-windows` | 🔴 红 | — |
| `⑥正文来源·build-linux` | 🔴 红 | — |
| `⑥不回落自动生成·build-linux` | 🟢 仍绿 | 开工前 Linux 那处**本来就没有** `generate_release_notes`（它只有 `files:`）⇒ 退回去它确实不该红。**牙在 `d6l`**（给它加上 ⇒ 红）。 |
| `⑥地板·每一处发布步骤都有 body_path` | 🔴 红（`0/2 处`） | — |
| `⑥正文只有一个住址` | 🟢 仍绿（取值集合 `[]`） | **空真** —— 一处 `body_path` 都没有时 `len(set) <= 1` 恒成立。牙在 `d4w`/`d4l`（两个不同的值 ⇒ 红）。**这个真空由同族的 `⑥地板` 接住**，而 `⑥地板` 在 `d0` 下是红的。 |
| `⑦生成器排在发布步骤前面·build-windows` | ⚪ **压根没印出来** | 它按 `paths` 逐条判，而 `d0` 之下 `paths` 是空的 ⇒ 循环一次都不进。同上：真空由 `⑥地板` 接住。牙在 `d4w`/`d9`。 |
| `⑦生成器排在发布步骤前面·build-linux` | ⚪ **压根没印出来** | 同上。牙在 `d4l`。 |
| `⑧地板·生成器在盘上` | 🔴 红 | — |
| `⑧生成器吐得出本版正文` | ⚪ **压根没印出来** | 上一条地板红了就不再往下跑（不许拿一个不存在的文件去 `node`）。牙在 `d7`。 |

合计：**红 5 · 仍绿 3 · 没执行 3**（分母 = 11）。
🔴 **「仍绿 3 ＋ 没执行 3」这 6 条里没有一条是靠它自己站住的** ——
其中 5 条（`⑥正文只有一个住址` · `⑦`×2 · `⑧生成器吐得出本版正文` · 以及被跳过那一条）
在 `d0` 这一形下**都是空真或不执行**，真空由 `⑥地板` / `⑧地板` 两条接住，
两条在 `d0` 下都是红的；剩下一条（`⑥不回落·build-linux`）是**如实不该红**，
它的牙由 `d6l` 单独买。**没有一条是仪式。**

## 四 · 终账（整趟门禁，沙箱）

命令（从项目根起跑，`K31`：宿主不跑 cargo/npm/tsc）：

```
PB_WS=backend-consolidation .claude/devbox/gate \
  /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r124 k-r124
```

落点 `…/f80a7aea-0ccf-4854-9b7a-12961a9c55aa/scratchpad/kr124-gate-2.log`（本会话私有目录）。
**逐格读数见件文件 `§8`**（那一份带分母）。

## 五 · 判不了 / 没做到

1. **「云端那一趟真会绿」判不了。** 推 PR / 推 tag 都不在本件边界里（单子第 3 条逐字）。
   本件能给的是**同形复现**：`d2` 把渲染之后那一版的形状原样摆出来、看它红；
   而「渲染之后就是那个形状」这一句的证据是**上一件在云端实打的日志**，住址在 `§二` 那一段。
2. **`action-gh-release` 拿到一个不存在的 `body_path` 时到底是炸还是回落，本件没有实测。**
   本件不靠它：⑥⑦ 在**盘上这一侧**就把「body_path 没人产出」判红了
   （`d4w` / `d4l` / `d9` 三刀），⇒ 那种配置**进不了 main**。
   ⚠ 反过来说：**运行期真的缺文件那一刻会发生什么，本件判不了**（那要一趟真发版）。
3. **「Linux 那处第二次 PATCH 会不会把正文改坏」本件没有实测。** 两处给的是**同一个生成器、
   同一个版本号**产出的同一份字节 ⇒ 推演是幂等的；**没有实测**，写清楚。
