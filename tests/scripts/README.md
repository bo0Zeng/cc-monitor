# `tests/scripts/` 目录

| 脚本 | 作用 |
|---|---|
| [`run.ps1`](run.ps1) | 自动注入 MSVC dev shell 环境后跑 tauri 命令 |
| [`gate.sh`](gate.sh) | 全量门禁，格的唯一名单：各格跑一遍，末尾只吐一行 `GATE: OK` / `GATE: PARTIAL` / `GATE: FAIL …`；`GATE_ONLY=<格>` 只跑点名的（CI 的 Linux job 这么调它）。平时经 `gate-affected.sh` 挑格跑，全量只在打发版标签前经 `gate-clean.sh` 跑一趟；它故意不提供 `--commit` 开关 |
| [`verify-committed-state.sh`](verify-committed-state.sh) | 从提交状态（不是工作树）编一次：工作树里有未提交改动时，「工作树绿」与「提交状态绿」是两件事。提交推上候选分支之前 CI 见不到它们，所以在本机跑 |
| [`hooks-are-runnable.sh`](hooks-are-runnable.sh) | 门禁 `hooks` 那一格：`tests/hooks/` 下会被 git 执行的每个脚本，盘上有没有可执行位 · 库里记没记那个位 · 语法过不过它自己声明的解释器（本仓 `core.filemode=false`，前两条分开判）。自带 8 条阳性对照 |
| [`assert-coverage-floors.mjs`](assert-coverage-floors.mjs) | 逐文件覆盖率地板 + 0% 文件递减棘轮（聚合阈值看不见单模块归零） |
| [`rust-coverage.sh`](rust-coverage.sh) | Rust 覆盖率量具：rustc 自带的 `-C instrument-coverage` ＋ 系统 `llvm-profdata` / `llvm-cov`，不装任何东西；两侧各跑一遍与门禁同一条 `cargo test`，报生产源码被执行到的行数。是量具不是门禁 |
| [`re-embed.sh`](re-embed.sh) | `BUILD_ID` bump 的同拍步骤：把内嵌的那几份后端字节重编并铺回落点，全仓唯一的本机产字节入口。配方与 `release.yml` 产字节那一步同源（门禁 `release-gate` 那一格两向对拍）；`--check` 只问盘上的字节与源码对不对得上，`--native` 铺本机那一份，`--clean` 删落点 |
| [`quick-check.sh`](quick-check.sh) | 交门禁之前的那一圈（不拿门禁锁、不代替门禁）。恒跑便宜的：两棵 Rust 树的排版 · tsc · 门禁里只读盘上文本的那几格（经 `gate.sh` 的 `GATE_ONLY`，收据另存）· 后端那棵改到的包跑 clippy · 末尾核生成物（`generated-drift.sh`）。重的四段（后端 · 壳两棵整套 `cargo test --lib` · 整套 vitest（逐条耗时另存 `.build/quick-check/vitest-times.json`）· 手机那一套）照 `gate-affected/map` 挑：相对起点（`base.sh`）改到的路径挑上那一段对应的格才跑 —— 前三段扫全仓、表里恒跑，手机那一段按改动挑；表不认识的 · 找不到起点 ⇒ 全跑；跑没跑、为什么逐段印出。测试一律在 `.build/` 下的一次性沙箱里跑（每一步经 `session-scrub.sh` 摘掉会话变量，再换 HOME 与 TMUX_TMPDIR）。「加了子命令要打版本号」那一条单列成「待打版本号」，不算红。`--all` 四段全跑（`gate-affected --trust-ci` 这样调），`--plan` 只印跑不跑，`--only <段>` 只跑那一段 |
| [`session-vars`](session-vars) | 会话继承来的环境变量表，全仓只这一张：Claude Code 与 Codex 的 · tmux 的 · cc-monitor 自己的（连 cc-bus）· 上游与代理的；族里照留的判据输入（`CCM_E2E_*` · `CCM_SHOTS_*` …）以 `!` 标出。`session-scrub.sh`（`gate.sh` · `sandbox-env.sh` · 手机台架照它的 `--list`）与 `src/common/test-home`（测试二进制起来时）读它 |
| [`session-scrub.sh`](session-scrub.sh) | 摘掉表里的变量再起命令：`heavy.sh` 每个出口、`quick-check.sh` 每一步都经它，于是经它们跑的一切测试与门禁都看不见外面会话的配置目录 · tmux · 真端口 · 上游；`--list` 只印这一刻会摘哪几个 |
| [`base.sh`](base.sh) | 「这棵树自己改了哪些」的起点：HEAD 与分支上游（没设上游就 origin/main，再没有就 main）的 merge-base。quick-check 挑段、gate-affected 挑格都用它 |
| [`bless-golden.sh`](bless-golden.sh) | 重打金样的唯一入口：金样判据只比不写，红时把现打的那份落在 `.build/golden/<仓相对路径>`（`tests/backend/golden.rs`）；对过之后跑它，逐份拷回仓里原处 |
| [`generated-drift.sh`](generated-drift.sh) | 生成物有没有落在提交外：每个 `generated/` 目录下与 HEAD 不同或没跟踪的文件逐份印出、退 1。quick-check 末尾与门禁 `generated` 格调它（cargo test 现写生成物，只在工作树里对、没提交的，合并以后才红） |
| [`gate-clean.sh`](gate-clean.sh) | 全量门禁的入口（打发版标签前那一趟）：经 `heavy.sh`（摘会话变量）排门禁锁跑 `gate.sh`；拿到锁时负载高于核数 × 0.6 就串跑 |
| [`gate-affected.sh`](gate-affected.sh) | 按改动挑格跑门禁：`git diff <基>...HEAD`（基默认取 `base.sh`，`--base` 可指）改到的路径照 [`gate-affected/map`](gate-affected/map)（路径 → 格组，每行带理由）挑格，挑格那一半是 [`gate-affected/plan.py`](gate-affected/plan.py)（`--dry-run` 只印计划；表与 `gate.sh` 对不上就停）。`--trust-ci` 是合并用的工作树推候选分支之前那一趟：先 `quick-check.sh --all`，CI 会跑的格交给 CI，只跑恒跑的便宜格 ＋ 被改到的本机才有的格 ＋ 被改到被测对象的 e2e 套件。裁决恒是 PARTIAL，末行 `AFFECTED: OK` / `AFFECTED: 红` |
| [`heavy.sh`](heavy.sh) | 重活（cargo 编测 · quick-check · 门禁 · gradle · 截图台架）一律经它跑：起命令前经 `session-scrub.sh` 摘掉会话变量；每份一个 `systemd-run --user --scope`、封顶内存，同时最多 `HEAVY_SLOTS` 份、多的按先来后到排队（排最前的那一个才去问槽位）；已经在一份重活里面（外层经它跑的）就直接跑，不再排槽位。没有 `systemd-run --user` 的机器（CI · Windows · 没有 user session）不封顶、直接跑，并说一声 |
| [`push-candidate.sh`](push-candidate.sh) | 推送：`w8/<名>` 强推到候选分支 `next` → 用 `gh` 等 CI 跑完 → 全绿才在主工作树上把 `main` 快进过去并推；有红就停、印红的 job 与失败日志，`main` 不动 |
| [`bump-build-id.sh`](bump-build-id.sh) | 发版时打版本号的机械那一半：改 `src/backend/lib.rs` 的 `BUILD_ID`；子命令集变了就把指纹历史表（只留上一版 ＋ 当前版两行）最老那行删掉、追加新行（新行取自指纹那条红时印的 `NEW-ROW`）。下一步是 `re-embed.sh` |
| [`xvfb-free.sh`](xvfb-free.sh) | 起一台私有 Xvfb：在 `:100–:899` 里挑空号、按 X 的老规矩建 `/tmp/.X<n>-lock` 占住（门禁的网络命名空间里外都认得），号由 Xvfb 写在标准输出第一行；`release <n> <pid>` 收场。文件窗口台架与截图工具共用 |
| [`apt-install.sh`](apt-install.sh) | CI 上装 apt 包的唯一写法（两份 workflow 每一处都调它）：每趟 update / install 套 `timeout`、apt 下载带重试、一趟没成歇一下再来、最多 3 趟；调它的那一步自带 `timeout-minutes` |
| [`release-notes.mjs`](release-notes.mjs) | GitHub Release 的正文生成器：从 `CHANGELOG.md` 里本版那一段生成正文，`release.yml` 两处发布步骤都用它；`--check` 只验不写，门禁 `release-gate` 那一格调它 |
| [`third-party-notices.py`](third-party-notices.py) | 第三方许可声明的生成器：用 cargo-about（版本钉在脚本里，没有就装进 `.build/tools`）与 `node_modules/` 生成仓根 `THIRD-PARTY-NOTICES.txt`（随安装包与 Release 分发）和 `tests/evidence/third-party-not-shipped.txt`；`--check` 只验不写，`release.yml` 发版那一趟调它；加删依赖后重跑一次再提交（门禁 `release-gate` ⑰ 两向核） |
| [`third-party-about.toml`](third-party-about.toml) | 上面那个生成器给 cargo-about 的配置：收哪几种许可 · 按哪几个目标平台收依赖 · 不收开发期依赖 |

跑法与射程都在各脚本自己的头注里。

## 分层：哪一层跑哪些判据

| 层 | 入口 | 跑什么 | 不在这一层 |
|---|---|---|---|
| 写的时候 | 增量编 ＋ 只跑改到的那几块（`cargo test --lib -- <模块>` · `vitest run <文件>`） | 改到的那几条 | 其余一切 |
| 交回前 | `quick-check.sh` | 两棵树排版 · tsc · 只读盘上文本的七格 · 两棵树整套 `cargo test --lib` · 整套 vitest · 后端那棵改到的包的 clippy · 生成物没提交；改到手机读的东西才加 ktlint · detekt · 手机 JVM 单测 | e2e · 编译格 · 设备测 |
| 推候选分支前 | `gate-affected.sh --trust-ci` | 上一层 ＋ 恒跑的便宜格 ＋ 被改到的本机才有的格（改了 `src/mobile/` 就含 `android-device`）＋ 被改到被测对象的 e2e 套件 | CI 会跑的格 |
| 发版前 | `gate-clean.sh`（全量） | `gate.sh` 每一格，含 `android-device`：模拟器上的设备测（androidTest，见下） | 只能手跑的那几样 |
| CI | `.github/workflows/ci.yml` | 两棵树 cargo · clippy · vitest · 覆盖率 · e2e · 手机 JVM 单测与 release 构建 · Windows 真测 | 设备测（runner 上没有 KVM 模拟器） |
| 只能手跑（登记着理由） | 各自的入口 | `SshConnectionTest`（要一台真 SSH 服务端；设备测那一格用 instrumentation 过滤跳过它，`InstrumentedTestInventoryTest` 钉着条数）· `f40-suite` / `graylight-suite`（要 Xvfb 上真开着的窗口）· 容器台架 `tests/e2e/local-backend-container/` · 速度台 `tests/benches/`（读数，不进门禁）· 截图台架 `tests/shots/run.mjs`（发版前与合并时手跑） | — |

**设备测那一格（`android-device`，10-11 起）**：经 `tests/mobile-smoke/emu.sh` 起一台无头 KVM 模拟器（自己一个 `systemd-run --user --scope`，MemoryMax 10G，不设 MemoryHigh，不占 heavy 的槽位；adb 用私有端口，不碰本机正在跑的 adb 与真手机），
跑设备测唯一的入口 `src/mobile/scripts/android-test.sh`（gradle 一律 `--no-daemon`：先装上 app、`pm clear` 清它的数据 —— 退出码 0 且回 `Success` 才往下跑，清不掉就红；
所以「从没连过服务器 ⇒ 落在服务器列表」那一支每趟都走得到），收场关模拟器。AVD 默认建在这棵树的 `.build/android-device/avd`，`GATE_ANDROID_AVD_HOME` 可指别处（要 SDK 里装着 `emulator` 与 `system-images;android-36;google_apis;x86_64`）。
没有 `systemd-run --user` 或 KVM 的机器上这一格按红记（判不了 ≠ 绿）。

**按墙钟判的上限只有一个数**（10-11 起）：测试里「等多久算挂了」一律是兜底的 300 秒 —— Rust `guard_core::wall::CEILING` · TS `tests/test-support/wall-ceiling.ts`（vitest 的 `testTimeout` / `hookTimeout` 就是它）· Kotlin 测试源里的 `WALL_CEILING_MS` · e2e 的 `$WALL_CEILING_S` 与 `wall_until`（`tests/e2e/sandbox-env.sh`）。
判快慢的改计步数 / 计调用次数 / 注入钟（假定时器、虚拟时间），真要看速度的进速度台、不进门禁；要分「自己回来」与「挂住被掐」看退出码，不看用了几秒。
判据 `tests/common/guard-core/wall_ceiling_tests.rs` 扫门禁跑的测试源，别处写数字期限就红，例外逐条登记理由。

**会话变量**：在 Claude Code 会话或 tmux 里起的 shell 继承着 `CLAUDE_CONFIG_DIR` · `TMUX` · `TMUX_PANE` · `CCM_*` 等，单跑测试只换 HOME 时会读到真号的状态。
表只在 `session-vars` 一处，枚举环境的只有 `session-scrub.sh`：经 `heavy.sh` / `quick-check.sh` 起的命令先经它摘掉；`gate.sh` · e2e 的 `sandbox-env.sh` · 手机台架的登录口照它的 `--list` 摘；测试二进制起来时 `src/common/test-home` 再按同一张表摘一次（`HOME` 是真家时）。
`TMUX_TMPDIR` 不在表里：摘掉只是回到默认那台，私有的那个由调用方给（quick-check 的沙箱；门禁换掉 `/tmp/tmux-<uid>`，并自己 unset 它）。
抓依赖用的代理走各工具自己的配置（`CARGO_HOME` 下的 cargo 配置 · `npm_config_proxy` · `gradle.properties`），通用的 `HTTP(S)_PROXY` 一族照摘。

**起点**（`base.sh`）：「这棵树自己改了哪些」= HEAD 与分支上游的 merge-base。施工分支从合并用的分支开时带上游
（`git worktree add --track -b <分支> <目录> <合并用的分支>`，或事后 `git branch --set-upstream-to=<合并用的分支>`）⇒ 只算这条分支自己的提交，不必先合 main；
没设上游 ⇒ 对 origin/main（合并用的分支上别的分支的改动也算进来，只会多跑、不会漏跑）。合并用的分支自己不设上游，对的就是 origin/main。

**快检里的静态格**：门禁里只读盘上文本、合计十来秒的那几格进快检（`static_cells`，经 `gate.sh` 的 `GATE_ONLY` 跑同一份命令）。
量过的数（24 核、负载二十多时）：`hooks` 0.3 秒 · `copy2` 1–5 秒 · `e2e-smoke` 0.2 秒 · `release-gate` 1 秒 · `platform` 0.2 秒 · `installface` 0.8 秒 ·
`shellcheck` 九十来份串跑 20–57 秒、按核数并起后 4–5 秒（门禁那一格也是并起的）；门禁脚本自己起跑约 3 秒。
不进快检的：`worktree-clean`（快检本来就跑在没提交的树上）· `audit`（要联网）· `tmux-default` 与 `ccbus-twophase`（要起真进程）· 编译类各格；
Windows 交叉编译与死码（`winchk` · `winchk-backend` · `winlink` · `deadcode` · `deadcode-backend`）要编 Windows 目标或整棵再编一遍，一格几分钟，留给 gate-affected。

门禁那几份（`gate-clean.sh` · `gate-affected.sh` · `heavy.sh` · `push-candidate.sh`）共用一个状态目录 `GATE_STATE_DIR`（默认 `${XDG_RUNTIME_DIR:-/tmp}/cc-monitor-gate/`）：门禁锁 `gate.lock` · 槽位 `heavy-slots/` · 推送失败日志 `logs/`，几棵工作树共用同一把锁。私人扫描（`gate-affected.sh` 末一步、`push-candidate.sh` 推之前）的脚本与词表不进仓，只认 PATH 上的 `private-scan.sh`：找不到时门禁开跑前就停（没判的不许说绿），推送拒推。

PowerShell 别名块的模板是 `src/shared/cc.ps1.tpl`（不在本目录；后端编译时 `include_str!` 进去）：设置里装别名块时把它（`__ccm_bind` 自动打开 monitor · 窗口标签 ＋ 带标签的 `ssh` · 接上别名文件）写进用户 `$PROFILE` 的 `# === cc-monitor BEGIN === ... # === cc-monitor END ===` 块内。

---

## run.ps1

### 用途

Tauri 在 Windows 上要靠 MSVC link.exe / cl.exe 编译 Rust 后端。如果当前 PowerShell 没注入 vcvars，会出现各种诡异错误（最常见：链接到 Git Bash 的 GNU coreutils 假冒 link，崩在链接阶段）。

`run.ps1` 通过 `vswhere.exe` 自动定位 MSVC 安装位置，调 `Launch-VsDevShell.ps1` 注入 PATH/LIB/INCLUDE，然后跑 tauri 命令。

### 用法

```powershell
powershell -NoProfile -File tests\scripts\run.ps1 [dev|build|check|clean]
```

| 子命令 | 等价于 |
|---|---|
| `dev` | `npx tauri dev`（弹 1100x800 窗口，HMR） |
| `build` | `npx tauri build`（产 msi + nsis + exe） |
| `check` | `cargo check`（不编 release） |
| `clean` | `cargo clean`（清仓根 `.build/shell/`） |

### 前置依赖

- **Visual Studio Build Tools 2022** + **VCTools workload**（提供 link.exe / cl.exe / Windows SDK）
- `vswhere.exe` 默认在 `C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe`（VS 安装时自动放）

脚本会在缺失依赖时抛错并提示安装。

### 工作原理

```powershell
1. vswhere.exe -latest -requires Microsoft.VisualCpp.Tools.HostX64.TargetX64
   → 找到 MSVC 安装路径，比如 D:\Microsoft Visual Studio\2022\BuildTools

2. & "<install>\Common7\Tools\Launch-VsDevShell.ps1" -SkipAutomaticLocation -Arch amd64
   → 注入 PATH / LIB / INCLUDE / INCLUDE 等环境变量到当前 PS session

3. Set-Location <repo root>
4. npx tauri $Cmd
```

详 [`src/doc/DEVELOPMENT.md`](../../src/doc/DEVELOPMENT.md)。
