# `tests/scripts/` 目录

| 脚本 | 作用 |
|---|---|
| [`run.ps1`](run.ps1) | 自动注入 MSVC dev shell 环境后跑 tauri 命令 |
| [`gate.sh`](gate.sh) | 出货前的唯一闸门：各格跑一遍，末尾只吐一行 `GATE: OK` / `GATE: PARTIAL` / `GATE: FAIL …`。先跑它、看见 OK，再单独提交；它故意不提供 `--commit` 开关 |
| [`verify-committed-state.sh`](verify-committed-state.sh) | 从提交状态（不是工作树）编一次：工作树里有未提交改动时，「工作树绿」与「提交状态绿」是两件事。本仓不推送，CI 见不到这些提交，所以在本机跑 |
| [`hooks-are-runnable.sh`](hooks-are-runnable.sh) | 门禁 `hooks` 那一格：`tests/hooks/` 下会被 git 执行的每个脚本，盘上有没有可执行位 · 库里记没记那个位 · 语法过不过它自己声明的解释器（本仓 `core.filemode=false`，前两条分开判）。自带 8 条阳性对照 |
| [`assert-coverage-floors.mjs`](assert-coverage-floors.mjs) | 逐文件覆盖率地板 + 0% 文件递减棘轮（聚合阈值看不见单模块归零） |
| [`rust-coverage.sh`](rust-coverage.sh) | Rust 覆盖率量具：rustc 自带的 `-C instrument-coverage` ＋ 系统 `llvm-profdata` / `llvm-cov`，不装任何东西；两侧各跑一遍与门禁同一条 `cargo test`，报生产源码被执行到的行数。是量具不是门禁 |
| [`re-embed.sh`](re-embed.sh) | `BUILD_ID` bump 的同拍步骤：把内嵌的那几份后端字节重编并铺回落点，全仓唯一的本机产字节入口。配方与 `release.yml` 产字节那一步同源（门禁 `release-gate` 那一格两向对拍）；`--check` 只问盘上的字节与源码对不对得上，`--native` 铺本机那一份，`--clean` 删落点 |
| [`quick-check.sh`](quick-check.sh) | 交门禁之前的那一圈（不拿门禁锁、不代替门禁）：两棵 Rust 树的排版 · tsc · 两棵树整套 `cargo test --lib` · 整套 vitest，测试一律在 `.build/` 下的一次性沙箱里跑（换 HOME 与 TMUX_TMPDIR、清掉会话变量）。「加了子命令要打版本号」那一条单列成「待打版本号」，不算红；红了把失败测试的 panic 原话印出来，整趟输出存 `.build/quick-check/` |
| [`gate-clean.sh`](gate-clean.sh) | 全量门禁的入口（打发版标签前那一趟）：清掉会话继承来的 `CCM_*` · `CLAUDE_*` · `ANTHROPIC_*` · `TMUX*`，经 `heavy.sh` 排门禁锁跑 `gate.sh`；拿到锁时负载高于核数 × 0.6 就串跑 |
| [`gate-affected.sh`](gate-affected.sh) | 按改动挑格跑门禁：`git diff <基>...HEAD` 改到的路径照 [`gate-affected/map`](gate-affected/map)（路径 → 格组，每行带理由）挑格，挑格那一半是 [`gate-affected/plan.py`](gate-affected/plan.py)（`--dry-run` 只印计划；表与 `gate.sh` 对不上就停）。`--trust-ci` 是合并树推候选分支之前那一趟：先 `quick-check.sh`，CI 会跑的格交给 CI，只跑恒跑的便宜格 ＋ 被改到的本机才有的格 ＋ 被改到被测对象的 e2e 套件。裁决恒是 PARTIAL，末行 `AFFECTED: OK` / `AFFECTED: 红` |
| [`heavy.sh`](heavy.sh) | 重活（cargo 编测 · quick-check · 门禁 · gradle · 截图台架）一律经它跑：每份一个 `systemd-run --user --scope`、封顶内存，同时最多 `HEAVY_SLOTS` 份、多的排队。没有 `systemd-run --user` 的机器（CI · Windows · 没有 user session）不封顶、直接跑，并说一声 |
| [`push-candidate.sh`](push-candidate.sh) | 推送：`w8/<名>` 强推到候选分支 `next` → 用 `gh` 等 CI 跑完 → 全绿才在主工作树上把 `main` 快进过去并推；有红就停、印红的 job 与失败日志，`main` 不动 |
| [`bump-build-id.sh`](bump-build-id.sh) | 合并那一拍打版本号的机械那一半：改 `src/backend/lib.rs` 的 `BUILD_ID`；子命令集变了就把指纹历史表（只留上一版 ＋ 当前版两行）最老那行删掉、追加新行（新行取自指纹那条红时印的 `NEW-ROW`）。版本谱系那段说明照旧人写，下一步是 `re-embed.sh` |
| [`xvfb-free.sh`](xvfb-free.sh) | 起一台私有 Xvfb：在 `:100–:899` 里挑空号、按 X 的老规矩建 `/tmp/.X<n>-lock` 占住（门禁的网络命名空间里外都认得），号由 Xvfb 写在标准输出第一行；`release <n> <pid>` 收场。文件窗口台架与截图工具共用 |
| [`apt-install.sh`](apt-install.sh) | CI 上装 apt 包的唯一写法（两份 workflow 每一处都调它）：每趟 update / install 套 `timeout`、apt 下载带重试、一趟没成歇一下再来、最多 3 趟；调它的那一步自带 `timeout-minutes` |
| [`release-notes.mjs`](release-notes.mjs) | GitHub Release 的正文生成器：从 `CHANGELOG.md` 里本版那一段生成正文，`release.yml` 两处发布步骤都用它；`--check` 只验不写，门禁 `release-gate` 那一格调它 |
| [`third-party-notices.py`](third-party-notices.py) | 第三方许可声明的生成器：用 cargo-about（版本钉在脚本里，没有就装进 `.build/tools`）与 `node_modules/` 生成仓根 `THIRD-PARTY-NOTICES.txt`（随安装包与 Release 分发）和 `tests/evidence/third-party-not-shipped.txt`；`--check` 只验不写，`release.yml` 发版那一趟调它；加删依赖后重跑一次再提交（门禁 `release-gate` ⑰ 两向核） |
| [`third-party-about.toml`](third-party-about.toml) | 上面那个生成器给 cargo-about 的配置：收哪几种许可 · 按哪几个目标平台收依赖 · 不收开发期依赖 |

跑法与射程都在各脚本自己的头注里。

门禁那几份（`gate-clean.sh` · `gate-affected.sh` · `heavy.sh` · `push-candidate.sh`）共用一个状态目录 `GATE_STATE_DIR`（默认 `${XDG_RUNTIME_DIR:-/tmp}/cc-monitor-gate/`）：门禁锁 `gate.lock` · 槽位 `heavy-slots/` · 推送失败日志 `logs/`，几棵工作树共用同一把锁。私人扫描（`gate-affected.sh` 末一步、`push-candidate.sh` 推之前）的脚本与词表不进仓：取环境变量 `PRIVATE_SCAN` 指的那份，没设就在 PATH 上找同名的扫描脚本，都没有就跳过并说一声。

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
