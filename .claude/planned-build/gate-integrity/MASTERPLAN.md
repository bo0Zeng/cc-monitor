# 主计划 / MASTERPLAN — gate-integrity（门禁不许在零断言下报绿）

> 所有功能宏观设计的**单一事实来源**。每次修订在末尾「§7 变更记录」追加一行。
>
> **状态：Phase A 已落盘，等用户审批。未动任何代码。**
> **路线图第 ③ 项。规模小（3 个功能），但它保护其余全部工作。**

---

## §0.0 当前事实

**这个工作区治的是本会话被烧最多次的那件事。** 这一整段里我在 commit message 里
**四次**声称过不存在的门禁；Phase G 代码工程视角又实证了一处「零断言报绿」。
根因不是粗心，是**门禁本身没有自检**。

| # | 事实 | 证据 |
|---|---|---|
| 1 | **八套 CI 真机套件全部没有「最小断言数」地板**。每套收尾只有 `[ "$FAIL" -eq 0 ]`（`cc-spawn-uplift` 是 `exit "$fail"`），加上套件内部 **3 处静默 SKIP 分支**（`ccm-cli.test.sh:217` 无 npx · `tmux-target:100` 无 script(1) · `cc-spawn-uplift:22` 已在 Phase G 修掉），**任何一处环境退化都是「少跑若干条 + 绿」** | 全仓搜 `PASS -ge` / 计数自检，只有非 CI 的 `f40-suite.sh:153` 有一处 |
| 2 | **本仓自己的教义在 Rust 侧做到了，价值更高的 shell 侧没做**：`structural_scan.rs:41` 的 `require(min_checked, …)` 把计数自检做成**调用方想忘都忘不掉**（`min_checked == 0` 直接硬失败），头注写「要件 3 缺一不可」 | — |
| 3 | **`vendor/cc-acct-iso/scripts/` 1348 行 bash 进了二进制、部署到远端执行，却在 shellcheck 门禁之外**。而当初把 `shared/ccm` 纳入的理由原话是「它被 `include_str!` 进二进制，是生产件」——**同一条论证一字不改地适用** | `acct_iso_deploy.rs:25-31` 用 `include_bytes!`；`ci.yml` 的 shellcheck 只覆盖 `e2e/*.sh shared/cc-bus/scripts/* shared/ccm`。**实测今天 `shellcheck --severity=error` 零告警 ⇒ 扩进来零成本** |
| 4 | **`vendor/cc-acct-iso/scripts/test/run-tests.sh`（424 行，部署工具自己的测试）在 CI 与 `package.json` 里都不存在** | — |
| 5 | **6 套已自动化的 e2e 套件同时不在 `package.json` 也不在 `ci.yml`**：`graylight-suite` · `graylight-daemon-frames` · `restart-suite` · `restart-daemon-frames` · `resume-suite` · `resume-daemon-frames` | `e2e/README.md` 明写它们「两级跑法（都无需 GUI，全自动）」并记了成绩（restart 命令级 24/0、daemon-frames 5/0）；只需 tmux + 一个 debug daemon，而 CI 的 `daemon` job 已经在编译 `remote-daemon-proto` |
| 6 | **`f40-suite.sh` 不算在内**：它需要 Xvfb + tauri dev，`e2e/README.md` 已论证成本 | — |

**Phase G 已实测的八套断言条数**（本工作区直接用，不必重测）：

```
tmux-target 26 · ccm-cli 44 · ccm-print-parity 12 · ccm-acceptance 15
ccm-pretrust 13 · cc-spawn-uplift 21 · tmux-guarded 14 · usage-probe 7
                                                        合计 152
```
前 7 套尾部已打印 `PASS=<n>`；**`cc-spawn-uplift` 没打印，要先加。**

**Phase G 已经修掉的（不在本工作区范围）**：`cc-spawn-uplift.sh:22` 缺 tmux 时 `exit 0`
（21 条断言一条不跑仍报绿，已实证）+ `tmux-target-acceptance.sh:18` 完全没有守卫 →
两处都改成 `exit 1` 并实测过；`ci.yml` 的断言条数标签按实测订正（8 套 / 152 条）。

---

## §0.1 目标与范围

- **总体目标**：让**门禁自己有自检**。今天的门禁能在「什么都没跑」的情况下报绿；
  目标是**跑得少也算失败**。

- **设计原则**：
  1. **计数自检要做成想忘都忘不掉**（照 `structural_scan::require` 的形状：
     参数为 0 直接硬失败），不是「记得加一行」的约定。
  2. **地板用实测值，不用静态调用数**。Phase G 实测：静态 `ck`/`chk` 调用数与运行期断言数
     **差得很远**（`ccm-cli` 静态 36 / 实测 44；`cc-spawn-uplift` 静态 20 / 实测 21）。
  3. **地板用 `-ge` 不用 `-eq`**。套件长断言是好事，缩水才是坏事。
  4. **生产件一律进 shellcheck**。判据是「它是否被编进二进制 / 部署出去执行」，
     不是「它在哪个目录」。

- **范围内**：八套 CI 真机套件的断言地板 · vendored bash 进 shellcheck +
  它自己的测试进门禁 · 6 套已自动化 e2e 进 CI

- **范围外**：
  - **不改任何套件的断言内容**（本工作区只加自检，不动被测行为）
  - `f40-suite.sh`（需 Xvfb + tauri dev，成本已论证）
  - Rust 覆盖率门禁（BACKLOG **E32**，另排）
  - `*.test.ts` 需手工登记进 `package.json` 那个不对称（BACKLOG 记录，本区不改——
    后缀分流是 `vitest.config.ts` 里论证过的有意设计）

- **整体成功标准**：
  1. **把任意一套套件的断言人为删掉一条，CI 红。** 今天不会。
  2. **把 `cc-spawn-uplift.sh` 里 20 条断言注释掉，它报红而不是「PASS=1 且绿」。**
  3. `shellcheck --severity=error` 覆盖**全部被编进二进制的 bash**
     （`shared/ccm` + `shared/cc-bus/scripts/*` + `vendor/cc-acct-iso/scripts/**`）。
  4. 6 套 e2e 在 CI 里真的跑，且它们的成绩不再只存在于 `e2e/README.md` 里。

---

## §1 功能清单

| ID | 功能 | 一句话目标 | 状态 | 依赖 | 优先级 |
|----|------|-----------|------|------|--------|
| G-A | **八套真机套件加断言数地板** | 每套收尾 `[ "$PASS" -ge <实测值> ]`；`cc-spawn-uplift` 先补 `PASS=` 打印；地板值写进 `ci.yml` 标签 | 待规划 | — | **P0** |
| G-B | **vendored bash 进门禁** | `vendor/cc-acct-iso/scripts/**` 纳入 shellcheck；`run-tests.sh`（424 行）接进 CI | 待规划 | — | P0 |
| G-C | **6 套已自动化 e2e 进 CI** | `graylight`/`restart`/`resume` 三族共 6 套进 `ci.yml`（复用已有的 daemon 编译 job）+ 进 `package.json` | 待规划 | G-A | P1 |

**G-A 为什么排第一**：它是三条里唯一能**回答「其余门禁到底有没有在跑」**的。
G-B/G-C 是扩大覆盖面，G-A 是保证覆盖面不会静默缩水。

**G-B 与 `account-zero` 的关系**：`account-zero` Z01 要改 `vendor/cc-acct-iso/scripts/`，
而它的 STATUS 里已写明「既然要改这个工具，没有网不能改」⇒ **G-B 是 `account-zero` Z01 的前置**。
两个工作区都指向同一件事，**由本工作区做，`account-zero` 只引用**。

---

## §2 架构概览

三层，各自的自检形态：

| 层 | 今天的自检 | 目标 |
|---|---|---|
| Rust 结构性扫描 | **`structural_scan::require(min_checked)`，`0` 直接硬失败** —— 已经是想忘都忘不掉的形状 | 不动，作为样板 |
| shell 真机套件 | 只有 `[ "$FAIL" -eq 0 ]` | 加 `[ "$PASS" -ge N ]`；`PASS` 未定义/为 0 也要红 |
| CI 步骤标签 | 手写条数（Phase G 已订正为实测值） | 与套件里的地板值**同源**，不许两处各写一个数 |

**「同源」怎么做**：地板值只写在套件脚本里一处，`ci.yml` 的标签由注释指向它，
**并加一条对拍**（照 `sftp.rs:1032` 扫 `shared/ccm` 那个结构性扫描的形状：
从脚本源里抠出地板值，与 `ci.yml` 标签比对）。**这条对拍本身要有反向自检**——
改地板值不改标签必须红。

---

## §3 ★共享面账本

| 共享面 | 涉及功能 | 最终形态设计 | 当前状态 | 备注 |
|---|---|---|---|---|
| **1. `ci.yml`** | G-A,G-B,G-C | 三处改动：shellcheck 的文件清单扩到 vendored · 新增 `run-tests.sh` 一步 · 新增 6 套 e2e 步骤。**断言条数标签与套件里的地板值同源** | Phase G 刚把标签订正为「8 套 / 152 条」并逐套列出 | **本工作区最先改 `ci.yml`**，`rust-ts-boundary` C05 与 `local-as-remote` L0/L4 都在它之后追加。**不改任何触发条件** |
| **2. 八套套件脚本的收尾段** | G-A | 统一形态：`printf 'PASS=%s FAIL=%s\n'` + `[ "$FAIL" -eq 0 ]` + `[ "${PASS:-0}" -ge <N> ]`。**`${PASS:-0}` 不是 `$PASS`**——变量名打错时要红而不是 unbound 后静默 | 7 套有 `PASS=` 打印无地板；`cc-spawn-uplift` 连打印都没有 | 八套的计数变量名不统一（`PASS`/`fail` 混用），G-A 要先统一 |
| **3. `package.json` 的 `test:*` 脚本链** | G-C | 6 套加进去。**注意 `npm test` 是 16 个 `&&` 串起来的手工链** | 16/16 今天没漏 | 加进 `npm test` 会让本地 `npm test` 需要 tmux + debug daemon ⇒ **见 §6 开放问题 1** |
| **4. `shellcheck` 的文件清单** | G-B | `e2e/*.sh shared/cc-bus/scripts/* shared/ccm src-tauri/vendor/cc-acct-iso/scripts/**` | 前三项 | 实测今天扩进来零告警 |
| **5. `vendor/cc-acct-iso/scripts/test/run-tests.sh`** | G-B | 进 CI 一步。它是**部署工具自己的测试**，而 `account-zero` 即将改那个工具 | 存在但从不运行 | 先跑一次看今天是否绿——**若今天就红，那是 G-B 的第一个发现** |

---

## §4 依赖图与实现顺序

```
G-A（地板）──── G-C（6 套进 CI，进来就自带地板）
G-B（vendored 进门禁）  独立，可并行
```

1. **G-A 先。** 它给后面新进来的套件定下形态——G-C 那 6 套进来时**直接自带地板**，
   不用回头补。
2. **G-B 可与 G-A 并行**（一个改套件脚本，一个改 CI 的 shellcheck 清单 + 加一步）。
   但 G-B 是 `account-zero` Z01 的前置，**若 `account-zero` 先启动，G-B 要插到它前面**。
3. **G-C 最后**，因为它要 debug daemon（CI 的 `daemon` job 已在编译 `remote-daemon-proto`，
   要把产物传给 e2e job——这是 G-C 唯一的技术不确定性）。

---

## §5 横切关注点与约定

- 不用 emoji · commit 不加 `Co-Authored-By` · `git add` 显式文件清单 · **不改 workflow 触发条件**。
- **门禁基线**（开工时）：`cargo test --all` **536** · `code-picture-core` **25** ·
  `npm test` **814 / 53 files** · clippy 0 · tsc 0 · `npm audit` rc=0 · shellcheck 0 ·
  exec-bit rc=0 · **8 套真机套件 26/44/12/15/13/21/14/7 = 152 条**。
- **本工作区每个功能的验收必须是「人为让它少跑，看它红不红」**——
  这是唯一能证明门禁有牙的方式。**「加上了地板」不算验收，「删一条断言 CI 红」才算。**
- **tmux 纪律**（本机 aya 的默认 socket 上住着真实 CC 实例）：强制 `-L` 守卫 shim
  （无 `-L`/`-S` 一律拒）+ 起飞前 canary 双向自检 + 跑完核对默认 socket 会话清单逐字未变。
  **裸 `tmux kill-server` 是禁用词。**
- **绝不启动真实已认证的 `claude`/`codex`**；启动器一律注入假的。
- **测试纪律**：变异**先 diff 确认落位、再确认它编译得过**，然后才判色 · 反向自检 ·
  计数自检用 `==` 不用 `>=`（**注意：这条说的是「自检本身的计数」，
  而套件地板用 `-ge` 是有意的——见 §0.1 原则 3，两者不矛盾**）·
  **守卫范围恰好等于性质范围** · **源码文本扫描 ≠ 行为测试**。

---

## §6 风险与开放问题

**风险**

1. **地板值设错会造成假红，而假红的门禁会被人关掉**（本会话实证过一次：
   `deploy_paths_use_verified_upload_for_content` 第一版范围比性质宽，当场自己抓出假红）。
   缓解：地板用 **Phase G 实测值** + `-ge` 而非 `-eq`。
2. **G-C 让本地 `npm test` 需要 tmux + debug daemon** ⇒ 抬高本地开发门槛。见开放问题 1。
3. **`run-tests.sh` 今天可能就是红的**（从没跑过）。那不是本工作区制造的问题，
   但会变成一个必须先处理的发现。**如实登记，不隐瞒**。
4. **CI 时长**：加 6 套真机 e2e 会拉长 CI。缓解：它们跑在 ubuntu job（便宜），
   且可与既有 job 并行。

**待用户确认的开放问题**

| # | 问题 | 我的建议 |
|---|---|---|
| 1 | G-C 的 6 套要不要进**本地 `npm test`**（而不只是 CI）？ | 建议**只进 CI，不进 `npm test`**。理由：`npm test` 应该保持「不需要 tmux/daemon 就能跑」，否则每个开发动作都变重。代价是本地改 `shared/ccm` 不会自动触发——**这一条如实写进 `e2e/README.md`** |
| 2 | `run-tests.sh` 若今天就红，修还是先登记？ | 建议**先登记 + 让它在 CI 里以「允许失败」进来一轮**，看清是什么再决定。理由：它是 `account-zero` 即将改的工具的测试，早看见比晚看见好 |
| 3 | 地板值与 `ci.yml` 标签的「同源对拍」要不要做？ | 建议**做**。理由：Phase G 刚发现那些标签是唯一记录期望条数的地方，而它们已经漂过一次（39 vs 44、12 vs 21）。一条对拍就能让它不再漂 |

---

## §7 变更记录

- 01 — 2026-07-29 — 初版，Phase A 主规划完成 — 路线图第 ③ 项。
  由 Phase G 代码工程视角的三条（阻塞 2「八套无断言地板」· 重要 4「vendored bash 在门禁外」·
  重要 5「6 套 e2e 不在 CI」）立项；地板值直接用 Phase G 的实测数（26/44/12/15/13/21/14/7）。
  **G-B 同时是 `account-zero` Z01 的前置。** 等用户审批。
