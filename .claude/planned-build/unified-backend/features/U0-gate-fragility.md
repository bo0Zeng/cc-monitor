# U0 · 地板的脆点与孤儿

- 工作区：unified-backend · 主计划 §3 第一梯队 · 任务 #88
- 风险档：**中**（动门禁本身，但不动被测代码）
- 由来：U-1 修的是「护栏在扫什么」，U0 修的是「门禁会不会静默失效」。
  两者都必须在 U2/U3 大搬家之前做完 —— 否则重构跑在假绿里。

## Phase B 复查：原 DoD 四条里有两条与现实不符，照实改

| 原 DoD | 复查实况 | 处置 |
|---|---|---|
| ① `ccm-cli.test.sh:206` 的 npx 脆点 | **属实**。`command -v npx` 不成立时 5 条跨语言对拍被 `SKIP` 掉，PASS 从 44 掉到 39，然后被地板判红 —— **诊断写成「断言数缩水」，真因是环境缺工具**。而那 5 条守的正是 `deriveTmuxName` ↔ `shared/ccm::derive_tmux_name` 的跨语言漂移 | **做**。改成 fail-closed 且诊断说真话 |
| ② `graylight-suite` / `f40-suite` 两个孤儿 | **一半不成立**。`graylight-suite` **不是孤儿** —— `e2e/README.md:52-54` 有排除论证（要 GUI runner + 起整个 app），`PHASE-G-REPORT.md:158` 明写「不打算做」，且有 `test:graylight` 脚本。真正的不一致是 **`f40-suite.sh` 连 npm 脚本都没有**（`e2e/*.sh` 里唯一一个 —— Phase D 审计订正：原写「全仓唯一」过头了，`e2e/tier2/` 的 wdio 套件同样没有），只能靠 `bash e2e/f40-suite.sh` 裸跑 | 改成：给 `f40-suite` 补脚本 + 在 README 补它的排除论证（对齐 graylight 的写法）。`daemon-wrapper.sh` / `gen-idle-tmux.sh` 不是套件（一个是 `daemonPath` 包装、一个是夹具生成器），不算孤儿 |
| ③ 16 个 `*.test.ts` **既无断言地板又被 `coverage.exclude` 排掉，双重不设防** | **「双重」不成立**。`coverage.exclude` 里的 `src/**/*.test.ts` 排的是**测试文件自身**（标准做法），被测的生产代码仍在 `include` 里 —— 只是覆盖率只跑 vitest，故那部分显示为未覆盖、**拉低**地板而不是被隐藏。<br>**但「无断言地板」这一条是真的、且是真洞**：16 个套件各自 `let failed=0 … if (failed) throw`，**把测试全删掉 `failed` 仍是 0 ⇒ 静默绿** | 做「无地板」这一条，**不做**不存在的那条 |
| ④ 订正 6/8 误记 | **U-1 时已随 §0.5 落账**（表里逐字写着「实测 8」+ 病根） | 无需再做，本文件登记一句即可 |

## DoD

| # | 项 | 验收 |
|---|---|---|
| ① | npx 缺失不再产生**误导性诊断** | 无 npx 时套件以**说真话**的诊断失败（指明缺 npx、指明被跳过的是跨语言漂移守卫），而不是让地板去报「断言数缩水」。变异：`PATH` 里藏掉 npx ⇒ 诊断里必须出现 `npx`，且**不出现**「断言数」字样<br>〔Phase D 审计 D6：初版诊断自己写了「不是**断言数**缩水」，字面自相矛盾 —— 已改措辞，复跑后 `npx`×2 / 「断言数」×0〕 |
| ② | `f40-suite` 有 npm 脚本 + 有排除论证 | `npm run test:f40` 可跑；`e2e/README.md` 里它的排除理由与 `graylight-suite` 同规格 |
| ③ | 16 个 tsx 套件有**机检地板** | 一张登记表钉住「哪 16 个套件 / 各几条 `test(`」，且与 `package.json` 的 `npm test` 链**互相对拍**。变异 A：删掉某套件里一条 `test(` ⇒ 红；变异 B：把某套件从 `npm test` 链里摘掉 ⇒ 红；变异 C：新增一个 `test:*` 套件但不登记 ⇒ 红 |
| ④ | 全量门禁绿 | daemon `cargo test` + monitor `--lib` + `tsc` + `npm test` + `cargo fmt --check` 两侧 |

**不做**：不给 e2e 套件加新地板（15 套已有，且有元门禁）；不动覆盖率阈值；不把 `graylight`/`f40` 塞进 CI（要 GUI runner，论证已在 README）；不改任何被测代码。

## 与主计划对接（共享面）

- 不碰账本任何一项。U0 只动门禁基础设施（`e2e/`、`package.json`、一个新守卫）。
- **新登记一条共享面 S16**：`npm test` 的套件链 + tsx 套件登记表 —— U8c（两个 TS 渲染器退役）会删测试文件，届时必须同步改登记表，不能靠「反正会红再说」。

## 逐条实现步骤

1. **①**：把 `ccm-cli.test.sh` 的 `if command -v npx` 分支改成：缺 npx ⇒ 打印明确诊断并 `FAIL`（不是 SKIP）。
   *验证*：`PATH=/usr/bin:/bin` 且临时藏掉 npx 跑一次，诊断里出现 `npx`。
2. **②**：`package.json` 加 `test:f40`；`e2e/README.md` 补 f40 的排除论证段。
   *验证*：`npm run test:f40 --dry-run` 形态正确（不真跑，要 Xvfb）；README 里两套排除论证并列。
3. **③**：新增守卫（**最终落点 `src/node-suite-registry-guard.vitest.ts`** —— 计划原写 `src/test-support/` 下，Phase D 审计 D7 指出那里住的全是「同名 helper 的测试」，独立守卫的既定落点是 `src/` 顶层；且原理由「test-support 已被 coverage 排除」只对一半，`vitest.config.ts:21` 已单独排掉所有 `*.vitest.ts`）：
   - 常量表：16 条 `(npm 脚本名, 文件路径, test() 条数)`。
   - 断言 a：每个文件的 `test(` 计数 == 登记数。
   - 断言 b：登记的脚本名集合 == `package.json` 里所有跑 `.test.ts` 的 tsx 脚本集合。
   - 断言 c：`npm test` 那条链里逐个出现每个登记的脚本名。
   *验证*：三个变异各跑一次。
   〔**实做为六条**：a 条数 · **a2 全仓总量地板（走磁盘，不从登记表求和）** · b 集合 · b2 路径逐字 · c 链路**+ `&&` 连接符** · **d 失败收尾**。后三处加粗的都是 Phase D 审计指出、计划里没有的 —— 见「代码审计结果」D1/D3/D4。〕
4. **④ 门禁全跑**。

## 测试策略

变异一律退出码判定，`cp -a` 还原后 `touch`。**报绿先怀疑夹具**。

## 实现期与计划的偏离

### 偏离①：步骤 2 顺带修了一处**文档副本漂移**（计划里没写）

改 `e2e/README.md` 时发现：正文三处写「**13 套**带断言数地板进了 CI」，而**它自己下面那张表列的是 9 + 6 = 15 套**，`ci.yml:318` 的元门禁也是按 15 对的。是 G2 加了两套之后正文没跟。
既然已经在改这个文件、且这正是「副本漂了不会让任何东西变红」那一类（README 自己第 38 行就写着这句提醒），一并订正为 15。**登记在此，不算默默扩范围。**

## 变异验证（Phase C 当时的 4 条；**完整 9 条见文末「变异总表」**）

| # | 变异 | 预期 | 实测 |
|---|---|---|---|
| 1 | 镜像 `/usr/bin` **只少 `npx` 这一个**，跑 `ccm-cli.test.sh` | 说真话的 FAIL | **RC=1，PASS=39 / FAIL=1**，诊断逐字含「找不到 npx（不是断言数缩水，是环境缺工具）」+ 点明被跳过的是 `deriveTmuxName`↔`derive_tmux_name` 的跨语言漂移守卫。<br>改前同一环境是 **PASS=39 / FAIL=0 / RC=0**，再由地板报「断言数缩水」——红绿位置对调即判据成立 |
| 2 | 删掉 `pricing.test.ts` 里一条 `test(` | a 条 ⇒ 红 | **RC=1**：`test:pricing (src/views/pricing.test.ts): 登记 6，实测 5` |
| 3 | 把 `test:pricing` 从 `npm test` 链里摘掉（**文件与登记都不动**） | c 条 ⇒ 红 | **RC=1**：「这些套件登记了、文件也在，但**没挂在 `npm test` 链上** ⇒ 它们根本不跑：test:pricing」 |
| 4 | 加一个 `test:brand-new` tsx 脚本但不登记 | b 条 ⇒ 红 | **RC=1**：「登记表与 package.json 的 tsx 套件集合不一致」 |

变异 3 是这条守卫**最主要的价值**：文件在、登记在、测试也在，唯独从链上掉了 —— 前两条判据都发现不了，而后果是它再也不跑。

## 门禁结果（Phase C 当时；最终值见文末「最终门禁」）

| 项 | 值 |
|---|---|
| `npm test` | **80 文件 / 1152 例**（+1 文件 +4 例 = 新守卫），RC=0；其中 node 纯函数段 `17 passed, 0 failed` |
| `tsc --noEmit` | RC=0 |
| `bash e2e/assert-pass-floor.sh ccm-cli 44` | `PASS=44（地板 44）`，RC=0 |
| `npm run coverage` | S54.14 / B45.15 / F48.73 / L55.64，均高于地板 52/43/46/53，RC=0 |

## 代码审计结果（D，两视角并行：正确性/反安慰剂 + 计划符合度/文档）

### 阻塞（0 项）

两份审计都独立复现了我记的 4 个变异，结论一致、没有伪造。「15 套」这个数经第二份审计四路确证
（`ci.yml` 里 `run: bash e2e/assert-pass-floor.sh` 调用行 15 条 · `:309` 步骤名 · `:316` 的 `-ge 15` ·
`:318-321` 的 15 个 pair），不是我写错。

### 重要（7 项，全部当轮修掉）

| # | 发现 | 处置 |
|---|---|---|
| **D1** | **我补的守卫漏了同一族里更重的那条洞。** 那 16 个套件的静默绿有**两条**路：①测试被删（我补了）②**收尾的 `if (failed > 0) … throw` 被删**。②更重 —— 测试还在跑、还在打 `✗`，**退出码照样 0**。审计实测复现：删 `pricing.test.ts` 的收尾 + 让一条断言必然失败 ⇒ `npm run test:pricing` **RC=0**，而我的守卫全绿。而且我在「它挡不住什么」那段里**没写它** —— 在一条专治安慰剂的守卫里，诚实段不完整本身就是缺陷 | 加**判据 d**：16 个套件的收尾形状完全一致（`if (failed > 0)` + 其后 throw），可机检。变异 H 验证 |
| **D2** | **b 条 `v.startsWith("tsx ")` 是 fail-open**：`"npx tsx …"` 写法能**静默逃逸**（加一个不登记不进链的套件，守卫全绿） | 改成词边界正则 `TSX_SUITE_CMD`，`tsx …` / `npx tsx …` 都认。变异 D 验证 |
| **D3** | **`TOTAL_FLOOR` 与逐套件判据是同一条的两份副本**（drift 为空 ⇒ total 恒等于登记和），结构上不可能失败 | 改成 **a2：走一遍磁盘、统计全仓 `src/**/*.test.ts`**，**不从登记表求和**。它挡的是 a 挡不住的两种：「门禁红了顺手把登记数改小」和「整套下线、登记与脚本一起删干净」。变异 G 验证 |
| **D4** | **c 条不看链的连接符**：把 `&&` 换成 `;` 照样绿，而后果是前面任何一套失败都被最后一条的退出码盖掉 | 加 `not.toMatch(/;|\|\|/)`。变异 I 验证 |
| **D5** | **c 条的反向检查只白名单了 `test:dom`** ⇒ `test:f40`（bash e2e）等正当地挂进链时会以「不在登记表里」误红，把人指向错误方向 | 改成**按命令形态判**（不跑 `.test.ts` 就跳过），不再用名字白名单 |
| **D6** | **DoD ① 的变异判据按字面不成立**：我的新诊断自己含「（不是**断言数**缩水…）」。语义上达成了（`assert-pass-floor.sh:44-47` 在 rc≠0 时直接 exit，根本走不到 `:57` 那条判词），但字面自相矛盾 | 诊断改成「环境缺工具，不是套件退化」。复跑变异：输出含 `npx` ×2、含「断言数」**×0** |
| **D7** | **落点破了目录约定**：`src/test-support/` 里现有的 `.vitest.ts` **全是同名 helper 的测试**，而我这条是独立守卫。仓里独立守卫的既定落点是 `src/` 顶层（`import-cycle-guard` / `generated-boundary-guard` / `paste-block-guard` / `base-flag-contract-guard`），它们 import test-support 但不住在里面。且我给的理由「test-support 已被 coverage 排除」**只对一半** —— `vitest.config.ts:21` 已单独排掉 `src/**/*.vitest.ts`，放哪都一样 | 搬到 `src/node-suite-registry-guard.vitest.ts`，头注理由改成真实的那个 |

### 建议（已采纳）

- **`^test\(` 只认行首**，缩进/循环生成的 `test(` 看不见（审计实测：加一条缩进两格的 ⇒ 全绿）。
  今天 16 个套件里 **0 处**非行首写法，是**潜伏不是现患** ⇒ 写进「它挡不住什么」诚实段。
- **f40 的「14 条」不可核**：两份审计数出来不一样（14 vs 12-15 依环境分支变），且该套件
  **没有 `合计 PASS=` 行**，永远喂不进 `assert-pass-floor.sh` ⇒ 在一个刚因「抄来的数字过期」
  被订正两次的文件里，**刻意不写具体条数**，改写「为什么它喂不进地板」。
- **「全仓唯一没有 npm 脚本的套件」过头**：`e2e/tier2/test/shell-smoke.spec.mjs`（wdio）也没有
  ⇒ 收窄成「`e2e/*.sh` 里唯一」。

### 审计范围之外、我自己撞出来的一个

给 `tmux-target-acceptance.sh` 写诊断时，在**双引号** `echo` 里用了反引号 —— bash 当成命令替换
执行了，输出里冒出 `行 109: =名:: 未找到命令`。改成单引号并在原地留注。
**这条是变异跑出来的，不是审过的** —— 又一次「报绿先怀疑夹具」的反面：报红也要看清红在哪。

## 工程审计结果（E，主线程对账）

- **主计划仍自洽。** U0 只动门禁基础设施与文档，未碰账本任何一项的最终形态。
- **新登记共享面 S16**（见主计划 §2）：`npm test` 套件链 + tsx 套件登记表。
  U8c 退役两个 TS 渲染器时会碰到其中 **3 个**套件，届时登记表四处要同步改：
  | 登记项 | 耦合 | U8c 后 |
  |---|---|---|
  | `test:launch-render-cli`（26） | 直接 import 渲染器 `tryRenderCli` + IR | **整文件删** |
  | `test:launch-dimensions`（28） | 直接 import 维度注册表 + IR + `renderFallback` | **整文件删** |
  | `test:remote-launch`（40） | 间接但真实：`remote-launch.ts` 五处 `renderFallback(plan…)` | **改不删**（`posixQuote`/`isValidSessionId`/`deriveTmuxName` 等与渲染无关） |
  只删一半时守卫的 b/c 会当场红 —— 这正是 S16 要的效果。
- **不在 S16 管辖但 U8c 同样会碰**（走 `test:dom`）：`base-flag-contract-guard.vitest.ts` ·
  `launch-requests.vitest.ts` · `remote-launch-run.vitest.ts` · 生产侧 `accounts.ts`（type-only）。
- **顺带清掉一个长期噪音**：`src-tauri/crates/branch-core/Cargo.lock` 此前**既未跟踪也未 ignore**，
  每次 `git status` 都躺在那里，而本仓红线是「显式 `git add`、绝不 `-A`」正为了别把它扫进去。
  照 `src-tauri/vendor/**/Cargo.lock` 的先例补了 `src-tauri/crates/**/Cargo.lock`。
  **与其靠每次记得，不如让它别出现。**

### 登记待办（不在 U0 范围）

- 「13 套」还有 4 处副本：`PHASE-G-REPORT.md:35` · `CHANGELOG.md:196` ·
  `gate-integrity/features/G-C-e2e-suites-into-ci.md:109`（这三处是**有日期的历史快照**，不追改）·
  **`settings-ia/MASTERPLAN.md:204`**（写在「基线」里当现状用，是活的、已漂 —— 留给 U14）。
- 其余口径过期文档（留 U14 收口）：`CONTRIBUTING.md:300`（14 组 / 595 测 / CI 共 4 job，
  实际 16 组 / CI **7** job）· `README.md:271`（「19 套 e2e 脚本」对不上任何口径；「jsdom 72 文件」实际 80）。
- 不以 `test:` 开头的脚本名整个绕过 b 条（今天无此形态）。

## 签收

- [x] 过代码审计（D，两视角并行）—— 阻塞 0 · 重要 7，全部当轮修完并各自变异复验
- [x] 过工程审计（E，主线程对账）—— 主计划仍自洽；新登记 S16
- [x] 主计划已更新（F）—— 账本加 S16、§7 变更记录追加、U0 行「241 例」订正为 242

## 最终门禁

| 项 | 值 |
|---|---|
| `npm test` | **80 文件 / 1154 例**（node 纯函数段 17 passed / 0 failed），RC=0 |
| `tsc --noEmit` | RC=0 |
| daemon `cargo test` | 194 passed，RC=0（本轮零 daemon 改动，作基线复验） |
| monitor `cargo test --lib` | 661 passed / 3 ignored，RC=0（本轮零 Rust 改动） |
| `cargo fmt --check` 两侧 | OK |
| `assert-pass-floor.sh ccm-cli 44` | `PASS=44（地板 44）` |
| `assert-pass-floor.sh tmux-target 26` | `PASS=26（地板 26）` |
| `shellcheck -S error`（三个改过的 .sh） | OK |
| `npm run coverage` | S54.14 / B45.15 / F48.73 / L55.64，均高于地板 52/43/46/53 |

## 变异总表（8 条，全部退出码判定；`cp -a` 还原后 `touch`）

| # | 变异 | 判据 | 实测 |
|---|---|---|---|
| 1 | 镜像 `/usr/bin` 只少 `npx` | ccm-cli fail-closed | RC=1，PASS=39/FAIL=1；输出含 `npx`×2、含「断言数」**×0** |
| 2 | 删 `pricing.test.ts` 一条 `test(` | a | RC=1「登记 6，实测 5」 |
| 3 | `test:pricing` 从链上摘掉（文件/登记不动） | c | RC=1「没挂在 `npm test` 链上 ⇒ 根本不跑」 |
| 4 | 加 `test:brand-new` 不登记 | b | RC=1「集合不一致」 |
| **5** | **删 `pricing` 的 `if(failed>0) throw` 收尾** | **d** | **RC=1**（旧版此处**全绿**） |
| **6** | **`npx tsx` 写法 + 不登记不进链** | **b** | **RC=1**（旧版此处**全绿**） |
| **7** | **删一条 `test(` 并顺手把登记 6 改成 5** | **a2** | **RC=1**「合计 241 < 地板 242」 |
| **8** | **`npm test` 链的 `&&` 换成 `;`** | **c** | **RC=1**「链必须全用 `&&` 串」 |
| 9 | 镜像 `/usr/bin` 只少 `script(1)` | tmux-target fail-closed | RC=1，PASS=24/FAIL=2，诊断说真因 |

5-8 是 Phase D 审计指出、旧版**放过**的四个 —— 修完逐条见红。
