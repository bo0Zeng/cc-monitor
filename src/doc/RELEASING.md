# 发版 SOP + CHANGELOG 写作规范

## 1. 发版前 checklist

- [ ] 改**版本号**，`release.yml` 的 `Verify version consistency with tag` 卡这四处：`package.json` · `src/frontend/shell/Cargo.toml` · `src/frontend/shell/tauri.conf.json` · `src/frontend/shell/Cargo.lock` 里 `name = "monitor"` 紧跟的那一行 version。漏一处，那一步就失败，而那时 tag 已经推出去了。
- [ ] `Cargo.lock` 提交（上一条改的就是它，`cargo` 不会替你 `git add`）。
- [ ] `package-lock.json` 顶上两处 `"version"`：bump 完 `package.json` 后跑一次 `npm install` 顺手对齐，纯手改一定漏这两处（`the_npm_lockfile_claims_the_version_we_ship` 查它们）。
- [ ] [CHANGELOG.md](../../CHANGELOG.md) 加本版那一段（写法见 § 3）。流水线从这一段生成 Release 正文，没有就红在渲染那一步。
- [ ] 改 **README 的版本号**，共五处：
      `README.md` ① 抬头那行「当前版本: vX.Y.Z」· ②「项目当前状态」里「当前发布 **vX.Y.Z**」· ③ 同一块的「- **版本**：vX.Y.Z（Released）」；
      `README.en.md` ④ 抬头那行 `Current: vX.Y.Z` · ⑤「Project status」里 `current release **vX.Y.Z**`。
      > 五处都有判据：①③④ 与三份清单由 `the_release_version_is_the_same_in_all_six_places` 对拍 `package.json`，②⑤ 由 `the_docs_self_reported_release_is_the_version_we_ship` 对拍这棵树要发的版本。数**落点**，别拿 `git grep -c '<上一版版本号>'` 当尺子——漂得越久，尺子越看不见它。
      > 版本号必须每版跟；测试数、CI job 数、代码行数那几处不写在 README 里，不用跟。
- [ ] **README 的「平台」与功能列表**：本版新增了平台或用户看得见的大功能，抬头那行的「平台」与功能段要跟上。
- [ ] 本地全绿：壳 workspace（`src/frontend/shell`）`cargo fmt --all --check` · `cargo clippy --workspace --all-targets` · `cargo test --workspace`；后端（`src/backend`）`cargo fmt --check` · `cargo test`——它不是 workspace 成员，是独立的一处；前端 `npm test`（node 纯函数 ＋ vitest）· `npm run coverage` · `npm run build`。fmt 不过 CI 会红。Linux 应用构建与那几个 e2e 是 CI 里各自独立的 job；CI 有哪些 job 以 `.github/workflows/ci.yml` 为准，引用前现数，别抄数。
- [ ] **若本版动过滚动 / 渲染管线**（stream · tabs · 会话查看器 · 分支折叠 · render-*）：在 Windows 真机按 `tests/e2e/README.md` 的「人工场景」复核 WebView2（WebKitGTK 没有 `overflow-anchor`，两端补批语义不同）。
- [ ] **若本版改过后端源码**：`src/backend/lib.rs` 的 `BUILD_ID` 已随改动 bump。走 tag 发版时 `release.yml` 的 `build-backends` job 从源码重编内嵌字节，官方渠道恒一致；**本地手工打包分发**则必须先重编并换掉 `src/frontend/shell/embedded-backends/` 里的字节（`bash tests/scripts/re-embed.sh`），否则装出去的是旧后端，连上后无限重装。
      > 身份住在字节自己里（`lib.rs::CC_MONITOR_BUILD_STAMP`，一段 `#[used] static`），`src/frontend/shell/build.rs` 编译时直接扫。三种情况直接让编译失败：抠不到源码的 `BUILD_ID`、内嵌字节里问不出身份戳、字节自报的身份与源码不符。出路二选一：① `bash tests/scripts/re-embed.sh` 重编重铺；② `bash tests/scripts/re-embed.sh --clean` 删掉落点，自动部署诚实关闭、编译立刻恢复（目录本就在 `.gitignore` 里）。三条都以「目录里真有字节」为前提；干净 clone 与 CI 里没有那个目录，走的是优雅降级，兜那一档的是 `stream_source/stream_flag_gate_tests.rs::embedded_build_id_single_source_wired`。
- [ ] **需要人手跑的 e2e 套件**：权威清单是 `shared_crate_registry_tests.rs::every_test_script_is_either_run_by_ci_or_registered_as_manual` 的 `MANUAL` 表，这里不抄第二份。表里的 `test:f40` 例外：它会往真的 `~/.claude/` 写 fixture，发版前不跑。
- [ ] **关键入口手测**：
  - [ ] 启动 monitor，本机正在跑的会话自动出现 tab
  - [ ] 点 tab 上的 ↗、按 `` ` ``，对应终端到前台
  - [ ] `H` 打开历史浏览器，resume 一个历史会话
  - [ ] `,` 打开设置，悬停各个 `?`，提示框在视口内
  - [ ] 机器页给一个有自定义内容的 profile / rc 装别名块：块外原内容保留，旁边生成 `.ccm-backup-…` 备份
  - [ ] Windows：用 `cc` 起 claude，走通 `ps-await` → `ps-registry` → `sid-hwnd-cache`
  - [ ] Windows：Windows Terminal 开两个窗口，各新开一个 PowerShell 标签页 `ssh` 到远端起会话；两个 tab 的 ↗ 各切到自己那个窗口

---

## 2. Git 操作 + CI

```powershell
git commit -m "release: vX.Y.Z"            # 不加 AI 署名行
git tag vX.Y.Z
git push origin main                       # ← 先推 main，等 CI 绿
git push origin vX.Y.Z                     # tag push 触发 release.yml
```

### 2.1 发版的第一道门：CI 必须绿

`release.yml` 的第一个 job 是 **`ci-gate`**：它按 `head_sha` 查同一个 commit 上 `ci.yml` 的 run，没有一条绿的就不放行，后面三个 job（后端交叉编译 / Windows 打包 / Linux 打包）全部不起。

对发版流程的影响，三条：

1. **先推 `main` 等它绿，再推 tag**——这道门接受这个 commit 上任意一条绿的 CI run，main 那次与 tag 那次跑的是同一棵树、同一份 `ci.yml`；main 已经绿了，它立刻放行。
2. **CI 红 ⇒ 发不出去，这是设计意图，不是故障。** 修 CI，然后在 GitHub 的 Release run 页面只重跑 `ci-gate` 这一个 job（Re-run failed jobs），不必重打 tag。
3. **超时也拦**：查不到 CI run 等 10 分钟、CI 还在跑等 45 分钟，到点判红。

### 2.2 产物

**Windows**（`build-windows`）：

- `cc-monitor_X.Y.Z_x64-setup.exe` — NSIS 安装器
- `cc-monitor_X.Y.Z_x64_en-US.msi` — MSI（后缀是 `en-US`：`tauri.conf.json` 没配 WiX 语言 ⇒ 走默认）
- `cc-monitor.exe` — 裸 exe（名字是 `src/frontend/shell/Cargo.toml` 里主 `[[bin]]` 的名字；包名仍是 `monitor`）
- `SHA256SUMS.txt` — 校验和

**Linux**（`build-linux`）：

- `cc-monitor_X.Y.Z_amd64.deb`
- `cc-monitor` — 裸二进制
- `SHA256SUMS-linux.txt`
- `cc-monitor-backend-x86_64` / `cc-monitor-backend-aarch64` — 远端后端的 musl 静态二进制（外部项目自部署要拿它）。身份在字节自己里（`<<ccm-build-id:…:ccm-build-id>>`），要问它是谁就 `grep -a` 那个串，或跑 `./cc-monitor-backend --ccm-probe` 读 `build=` 那一行。

这张表的权威是 `release.yml` 里两处发布步骤的 `files:`，引用前以它为准。

发布到 https://github.com/bo0Zeng/cc-monitor/releases/tag/vX.Y.Z

---

## 3. CHANGELOG 写作规范

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。版本遵循 [SemVer](https://semver.org/)。

### 3.1 模板

```markdown
## [X.Y.Z] — YYYY-MM-DD

### 会改变已有行为的（如果有）
- **<一句话>**：以前怎样、现在怎样；要用户动手的写清怎么做。

### 新功能（如果有）
- **<功能名>**：用户看见的界面 / 命令 / 行为。

### 修复（如果有）
- <用户之前看到什么坏了>，现在怎样。

### 已知限制（如果有）
- <这一版还做不到的、没在哪种真机上跑过的>。
```

- 只写用到的段，次序照上面；「会改变已有行为的」有就放第一个（`the_changelog_top_section_is_the_version_we_ship` 查它的位置）。
- 只说用户看得到的变化，不写内部名字（文件 · 函数 · 错误码 · 字段名）与施工说法；根因与改了哪里写进提交说明。

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

### 3.4 修复那一段怎么写

写用户看得到的：之前看到什么坏了、现在怎样；知道从哪一版开始坏的就写上。不写根因、不写动了哪个文件或接口。

例（4.0.6）：

> 账号核对不再把每个号各自一份的文件（远端设置、限额记录、清理与更新记录、MCP 授权缓存、状态与反馈目录等）误报成「应当是共享链接」，修复也不再把它们链成共享。

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

---

## 5. Release Notes

Release 正文由流水线生成：`release.yml` 两处发布步骤（`build-windows` 的 `Create / update GitHub Release` · `build-linux` 的 `Append Linux artifacts to the release`）各跑一次 `node tests/scripts/release-notes.mjs RELEASE_BODY.md`，`body_path` 指向它，正文就是 **`CHANGELOG.md` 对应版本段**。发版前要做的只有一件：`CHANGELOG.md` 里有本版那一段；没有就红在渲染那一步，不会静默回落（本地门禁 `release-gate` 那一格提前一步逮同一件事）。

生成器不带下载清单（Release 页本来就逐个列出资产）。要在正文里加一段下载说明就手工加，名字与 § 2.2 那张表逐字相同：

```markdown
**下载**
- `cc-monitor_X.Y.Z_x64-setup.exe` — Windows 普通用户（NSIS）
- `cc-monitor_X.Y.Z_x64_en-US.msi` — 企业 IT 部署（MSI）
- `cc-monitor.exe` — Windows 裸 exe（需 WebView2 + 自管路径）
- `cc-monitor_X.Y.Z_amd64.deb` — Linux（Debian / Ubuntu 系）
- `cc-monitor` — Linux 裸二进制
- `SHA256SUMS.txt` / `SHA256SUMS-linux.txt` — 校验和（Windows / Linux 各一份）

完整 CHANGELOG 见 [CHANGELOG.md](https://github.com/bo0Zeng/cc-monitor/blob/main/CHANGELOG.md)
```
