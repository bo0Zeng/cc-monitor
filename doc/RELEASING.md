# 发版 SOP + CHANGELOG 写作规范

## 1. 发版前 checklist

详 [CONTRIBUTING.md § 1.5](CONTRIBUTING.md#15-发版前)。摘要：

- [ ] 改 **版本号：`release.yml` 卡得住的四处**（`package.json` + `src-tauri/Cargo.toml` +
      `src-tauri/tauri.conf.json` + `src-tauri/Cargo.lock` 里 `name = "monitor"` 紧跟的 version 行）
      > ⚠ **原文写的是「三处」，漏了 `Cargo.lock`。** `release.yml` 的
      > `Verify version consistency with tag` 卡的是**四处**（2026-09-09 现打，读的是
      > 那一步的 PowerShell 本体）：漏一处 ⇒ 那一步 fail ⇒ **而 tag 已经推出去了**。
- [ ] `Cargo.lock` 提交（上一条改的就是它；`cargo` 不会替你 `git add`）
- [ ] **`package-lock.json` 顶上那两处 `"version"`（`:3` 与 `:9`）—— 没有任何东西卡它。**
      2026-09-09 现打仍是 **3.2.0**，而 `package.json` 已经 3.6.0。
      > **它不会炸，但别当它不存在**：`v3.6.0` 那次发版的 lock 顶上就写着 `3.2.0`，
      > 而 release run 里 `npm ci` **照样过**（`git show v3.6.0:package-lock.json` +
      > 那次 run 现打核过）⇒ 版本号这一格与 `npm ci` 的同步性检查无关。
      > 修法是**跑一次 `npm install`**（bump 完 `package.json` 之后），它会顺手对齐；
      > 纯手改版本号一定漏掉这一处。
- [ ] [CHANGELOG.md](../CHANGELOG.md) 加新版本段（写法见 § 3）
- [ ] **改 README 的版本号**。⚠ **不是「两处」——2026-09-09 现打是 `README.md` 三处 +
      `README.en.md` 一处**（尺子：`git grep -c '3\.6\.0' -- README.md README.en.md`，
      量于 `04745b3`）。逐处：
      `README.md` ① 抬头那行「当前版本: vX.Y.Z」· ② 「项目状态」那段里「当前发布 **vX.Y.Z**」·
      ③ 「项目当前状态」块的「- **版本**：vX.Y.Z（Released）」；
      `README.en.md` ④ 抬头那行 `Current: vX.Y.Z`。
      > 🔴 **`README.en.md` 里 ② 的对侧今天就是漂的**：那句写着 `current release **v3.2.0**`，
      > 而中文侧同一句已经是 v3.6.0 —— **落后四个版本，`v3.6.0` 那个 tag 上就已经是这样**
      >（`git show v3.6.0:README.en.md` 现打核过）。原来那句「两处」少数的正是 ②，
      > 于是英文侧的 ② 从来没人改。**本轮把它一起补上，别只改抬头那行。**
      > **这一条是补出来的，别删。** v3.1→v3.4 **连续四次**发版漏改 README，
      > 于是 README 的「当前版本」长期落后一个大版本；BACKLOG 早把「checklist 里没有
      > README 这一条」点名为**机制性根因**，而根因没修 ⇒ 第四次照样复发。
      > 版本号是最便宜的一条，**测试数 / CI job 数 / 代码行数那几处不要求每版跟**
      >（它们在 README 里有 4-5 份副本、注定会漂；真要治是给它们找单一落点，见 BACKLOG E65）。
- [ ] **README 的「平台」与功能列表**：本版若**新增了平台或用户可感知的大功能**，
      抬头那行的「平台」和状态段的一句话概述要跟上（例：v3.4.0 首发 `.deb`，
      而 README 到 v3.5.0 才补上「Linux」——用户读完会以为不支持）
- [ ] `cargo fmt --all --check + cargo clippy --workspace --exclude code-picture-core --all-targets + cargo test --workspace --exclude code-picture-core + cargo test -p code-picture-core + npm test + npm run coverage + npm run build` 全绿（fmt 不过 CI 会红；`npm test` 含 16 组 node 纯函数 + vitest DOM = 前端 job）。⚠ **G2（2026-08-04）订正**：原来这里列的是 `cargo test --all` 外加 `-p code-picture-core` / `-p branch-core` 两条补丁，理由写着「两者都是 path 依赖非 workspace 成员，`--all` 测不到」。**那句对 `branch-core` 已经不成立** —— `src-tauri/Cargo.toml` 现在有 `[workspace]`，六个共享 crate 都是真成员（`--workspace` 覆盖，实测 922 = monitor 882 + 六 crate 40）。**只有 vendor 的 `code-picture-core` 仍要单列**（它是成员的 path 依赖，`exclude` 对 path 依赖不生效 ⇒ CI 用 `--exclude` 排除它、再单独 `-p` 跑，**不动 vendor 一个字节**）。远端 daemon `cargo test`（`remote-daemon-proto/`，含 `cargo fmt --check`）**仍是独立一处** —— 它刻意不入 workspace，那条隔离是真架构约束。+ Linux app 构建 + 那几个 e2e job 都是**独立 CI job**，别漏跑）。⚠ **CI 有几个 job 这里刻意不写死**（同上一条纪律：这个数在仓里已经漂过一次 —— 原文写着「共 **7** job」，而 2026-09-09 现打是 **8** 个：`rust` / `frontend` / `daemon` / `linux-app-build` / `e2e-smoke` / `e2e-tmux` / `e2e-tmux-rust` / `weak-net`，尺子 `sed -n '/^jobs:/,$p' .github/workflows/ci.yml | grep -cE '^  [a-z0-9-]+:$'`，量于 `04745b3`。⚠ **尺子必须先切到 `jobs:` 段**：不切的话 `on:` 底下的 `push:` 与 `defaults:` 底下的 `run:` 也会被数进去，得 10 —— 本仓最高频的那类错「量具的作用域对不上事实」）。**引用前现打，别抄这个数**；权威是 `.github/workflows/ci.yml` 本身
- [ ] **若本版动过滚动/渲染管线**（stream/tabs/session-viewer/branch-fold/render-*）：跑一遍 `npm run test:f40`（= `e2e/f40-suite.sh`；Linux Xvfb + 一个正在跑的 `tauri dev`，前置见 e2e/README）+ Windows 真机把 e2e/README「人工场景」的 WebView2 复核过一遍（WebKitGTK 无 overflow-anchor，两端补批语义不同）
- [ ] **若本版改过 daemon 源码**（BUILD_ID 应已随改动 bump）：走 tag 发版由 release.yml 的 build-daemons job 从源码重编内嵌二进制（官方渠道恒一致）；**本地手工打包分发**则必须先重编并**同步更新 `src-tauri/embedded-daemons/` 里的二进制和同名 `.build_id` 清单**——否则装出去的是旧 daemon，连接后无限重装循环。
      > **这一格 2026-08-01（U-1）从 warning 升成编译期 panic。** 原来只有一条比 mtime 的
      > `cargo:warning`，而真实事故是**半 bump**：源码已 `p1v-`、清单还是 `p1u-`，二进制 mtime 反而更新
      > ⇒ mtime 判据完全不响。现在 `build.rs` 直接 panic 的有三种：抠不到源码 `const BUILD_ID`、
      > **有二进制但缺 `.build_id` 清单**、清单与源码不符。三条都不是「慢一点」——monitor 判过期的
      > 唯一判据就是 build_id 字符串不等，装上去会**永远判 StaleBuild 并无限重装**。
      > 出路二选一：① 重编 + 同步清单（**不必装 zig**，`rust-lld` 即可，命令见 [REMOTE-PHASE0-DEPLOY.md § 发版构建](REMOTE-PHASE0-DEPLOY.md#发版构建交叉编译--内嵌-daemon-二进制f08b)）；
      > ② `rm -rf src-tauri/embedded-daemons/`——自动部署诚实关闭、编译立刻恢复（目录本就 gitignore，删除零代价）。
      > **⚠ 三条都以「目录里真有二进制」为前提**（Phase E 审计 R3 订正）：干净 clone / CI 里该目录不存在，
      > 走的是优雅降级、`DAEMON_BUILD_ID` 静默变 `"unknown"`；兜那一档的是 monitor 侧的
      > `ssh_source.rs::embedded_build_id_single_source_wired`，不是 `build.rs`。
- [ ] **需要人手跑的 e2e 套件**：权威清单是判据
      `shared_crate_registry.rs::every_test_script_is_either_run_by_ci_or_registered_as_manual`
      的 `MANUAL` 表 —— **这里不抄第二份**。那张表会因「仓里有套件没人跑」而变红，
      而抄下来的清单只会漂（v3.1→v3.4 那四次漏改 README 就是抄的那份漂了）。
      > 建这条时实测：`e2e/graylight-suite.sh` 是一整套跨进程整链 e2e，**CI 不跑、本清单也没有它**，
      > 唯一触发条件是「有人想起来」。它已进 `MANUAL` 表。
- [ ] [CONTRIBUTING.md § 1.5](CONTRIBUTING.md#15-发版前) 列出的关键 UI 入口手测

---

## 2. Git 操作 + CI

```powershell
git commit -m "release: vX.Y.Z"            # 不加 Claude coauthor
git tag vX.Y.Z
git push origin main                       # ← 先推 main，等 CI 绿
git push origin vX.Y.Z                     # tag push 触发 release.yml
```

### 2.1 发版的第一道门：CI 必须绿（2026-09-09 起）

`release.yml` 的第一个 job 是 **`ci-gate`**：它按 **`head_sha`** 查同一个 commit 上
`ci.yml` 的 run，**没有一条绿的就不放行**，后面三个 job（daemon 交叉编译 / Windows 打包 /
Linux 打包）全部不起。

> **在这之前是零。** `release.yml` 里一条测试门都没有（`cargo test` / `npm test` /
> `tsc --noEmit` / e2e 在那份文件里零命中），而跑测试的 `ci.yml` 与它互不相干
> ⇒ **主干红成什么样，推个 tag 都照发**。发版标准逐字是「新用户可以稳定装上各种功能」，
> 这道门是那句话在流水线上的最低形态。

对发版流程的实际影响，三条：

1. **先推 `main` 等它绿，再推 tag** —— 这道门接受这个 commit 上**任意一条**绿的 CI run，
   而 main 那次与 tag 那次跑的是同一棵树、同一份 `ci.yml`。main 已经绿了的话它**立刻放行**。
2. **CI 红 ⇒ 发不出去，这是设计意图，不是故障。** 修 CI，然后**重跑 `ci-gate` 这一个 job**
  （GitHub Release run 页面的 Re-run failed jobs），不必重打 tag。
3. **超时也拦**：查不到 CI run 等 10 分钟、CI 还在跑等 45 分钟，到点判红。
   CI 整条实测 3-6 分钟（v3.6.0 那次 3m18s、09-10 那次 5m43s），余量全是排队用的。

### 2.2 产物

整条 release run 实测 **~17 分钟**（v3.6.0：`build-daemons` 1m50s → `build-windows` 9m16s →
`build-linux` 5m36s；`ci-gate` 加在最前面，main 已绿时它几秒就过）。

**Windows**（`build-windows`）：

- `cc-monitor_X.Y.Z_x64-setup.exe` — NSIS 安装器
- `cc-monitor_X.Y.Z_x64_en-US.msi` — MSI（⚠ 后缀是 **`en-US`**，不是 `zh-CN`：
  `tauri.conf.json` 没配 WiX 语言 ⇒ 走默认。v3.6.0 实际产物逐字 `cc-monitor_3.6.0_x64_en-US.msi`）
- `monitor.exe` — 裸 exe（⚠ 名字是 **cargo 包名 `monitor`**，不是 productName `cc-monitor`）
- `SHA256SUMS.txt` — 校验和（⚠ **带 `.txt`**）

**Linux**（`build-linux`，v3.4.0 起）：

- `cc-monitor_X.Y.Z_amd64.deb`
- `monitor` — 裸二进制（⚠ **v3.6.0 那次还没有它**：这条上传是 08-02 `ae18878` 补的，
  之后一直没发过版 ⇒ **它第一次真的出现会是下一个 tag**）
- `SHA256SUMS-linux.txt`
- `cc-monitor-remote-x86_64` / `cc-monitor-remote-aarch64` + 各自的 `.build_id`
  —— 远端 daemon 的 musl 静态二进制（DN-8：外部项目自部署要拿它）

> ⚠ 上面这张表是 **2026-09-09 读 `v3.6.0` 那个 release 的真实资产清单**现打出来的
>（`gh api repos/bo0Zeng/cc-monitor/releases/tags/v3.6.0 --jq '.assets[].name'`），
> 不是照 `release.yml` 推的。**引用前重打一遍** —— 权威是 `release.yml` 里那两处
> `files:`，这张表是它的快照。
> **订正了什么**：原文写 `SHA256SUMS`（实际 `SHA256SUMS.txt`）、`cc-monitor.exe`
>（实际 `monitor.exe`）、`_zh-CN.msi`（实际 `_en-US.msi`），并且整张表**只有 Windows**
> —— 照它核校验和的新用户会当场卡住，Linux 用户则以为没有产物。

发布到 https://github.com/bo0Zeng/cc-monitor/releases/tag/vX.Y.Z

---

## 3. CHANGELOG 写作规范

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。版本遵循 [SemVer](https://semver.org/)。

### 3.1 模板

```markdown
## [X.Y.Z] — YYYY-MM-DD

### 修复（如果有）
- **<一句话标题>**：根因 + 修法。如果是修了"前面版本带病的 bug"，写清"vX 起的回归"。

### 改进（如果有）
- **<功能名>**：做了什么 + 用户感知变化。

### 新增（如果有）
- **<新功能>**：用户看见的 UI / 命令 / 行为。

### 改动（如果是 breaking 或 UX 变化）
- **<变更>**：跟之前不同的地方。

### 项目管理（可选，体积小）
- 删未用依赖 / 文档更新等
```

### 3.2 写作要点

**入 CHANGELOG 的内容**：用户能感知的变化 — 新功能 / 修复 / 行为变更 / 用户能看到的 UI 改动。

**不入 CHANGELOG**：
- 内部重构（除非影响用户感知）
- 单纯文档更新（除非加了关键文档）
- 加 / 改测试
- 代码风格调整

### 3.3 何时 patch / minor / major

- **patch (X.Y.Z+1)**：bugfix 不引入新功能
- **minor (X.Y+1.0)**：新增功能 / UI 改动，向下兼容
- **major (X+1.0.0)**：breaking change（数据格式 / 跨进程协议不兼容 / 卸载需要清旧数据）

当前 v1.x 系列大部分是 patch（cc 集成调试期），偶有 minor（新增功能 / 改 UI）。还没有 major。

### 3.4 关于修 bug 的 "如何写"

修 bug 的段必须说清：

1. **症状**：用户看到什么坏了
2. **根因**：技术上为什么坏（具体到代码 / API 行为）
3. **修法**：动了哪个文件 / API / 算法
4. **回归性**：从哪个版本开始坏的（如有）

例（v1.7.10 ACL 事故段的精简版）：

> **profile_installer 可能写坏用户 profile**
>
> 症状：装完 cc 集成后 PowerShell 启动报 `Access to the path … is denied`。
> 根因：atomic_write 走 `write tmp → remove path → rename tmp path` 三步；tmp 文件 ACL（继承父目录）会替换掉 dst 上原有的 explicit ACE。Documents 重定向到非默认盘的用户原 explicit ACE 丢失。
> 修法：atomic_write 改用 Win32 `ReplaceFileW` 保留 dst 的 ACL / ADS / 创建时间；加 backup + 写后校验。

不要写成"我们这次修了 v1.7.9 的 bug" 这种自指叙事。

### 3.5 应急步骤段

**如果**修复需要用户做额外操作（不只是装新版），加 "受影响用户的应急步骤" 段：

```markdown
### 受影响用户的应急步骤

如果你在 v1.7.0-1.7.9 装过 cc 集成后 PowerShell 启动报 `Access to the path … is denied`：

**情况 A**：用文件资源管理器把 Microsoft.PowerShell_profile.ps1 改名加 `.broken-bak` 后缀；重启 PowerShell。

**情况 B**：管理员 PowerShell 跑 `icacls "<profile>" /grant "$env:USERDOMAIN\$env:USERNAME:(F)"` 加 explicit Full Control。
```

---

## 4. Hot-fix 流程

如果发版后 CI 出包 / 上传后**才**发现严重 bug：

1. **不要 revert tag** — 已经在 GitHub Releases 上的版本不要删（用户可能正在下）
2. 修 bug → bump patch（X.Y.Z → X.Y.Z+1）→ 走标准发版流程
3. 旧版 GitHub Release 描述里加一行 "⚠ 此版本存在 \<问题\>，请下载 vX.Y.Z+1"，链到新 release

历史例子 v1.7.8 → v1.7.9 → ... → v1.7.13 一连串 patch 都是这种模式。

---

## 5. Release Notes

GitHub Releases 描述用 [CHANGELOG.md](../CHANGELOG.md) 对应版本段的复制 + 加：

```markdown
**下载**
- `cc-monitor_X.Y.Z_x64-setup.exe` — Windows 普通用户（NSIS）
- `cc-monitor_X.Y.Z_x64_en-US.msi` — 企业 IT 部署（MSI）
- `monitor.exe` — Windows 裸 exe（需 WebView2 + 自管路径）
- `cc-monitor_X.Y.Z_amd64.deb` — Linux（Debian / Ubuntu 系）
- `monitor` — Linux 裸二进制
- `SHA256SUMS.txt` / `SHA256SUMS-linux.txt` — 校验和（Windows / Linux 各一份）

完整 CHANGELOG 见 [CHANGELOG.md](https://github.com/bo0Zeng/cc-monitor/blob/main/CHANGELOG.md)
```

⚠ 名字与 § 2.2 那张表**同源**（都是从真实 release 资产读出来的），改一处要两处一起改 ——
这份模板存在的意义就是让 Release 页上的名字与用户下到的文件逐字相同。
