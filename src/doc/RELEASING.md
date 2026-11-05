# 发版 SOP + CHANGELOG 写作规范

## 1. 发版前 checklist

- [ ] 改**版本号**，手改的只有这四处，`release.yml` 的 `Verify version consistency with tag` 卡的也正是这四处：`package.json` · `src/frontend/shell/Cargo.toml` · `src/frontend/shell/tauri.conf.json` · `src/frontend/shell/Cargo.lock` 里 `name = "monitor"` 紧跟的那一行 version（改完 `Cargo.toml` 跑一次 `cargo check` 就跟上，记得提交）。漏一处，那一步就失败，而那时 tag 已经推出去了。
      > 别处都从这几处读：后端编译时读 `package.json`（`src/backend/common/build.rs`），手机的 versionName 也读它（`src/mobile/app/build.gradle.kts`）。
- [ ] `package-lock.json` 不手改：改完 `package.json` 跑一次 `npm install`，顶上那两处 `"version"` 由它跟上，一起提交。
- [ ] [CHANGELOG.md](../../CHANGELOG.md) 加本版那一段（写法见 § 3）。流水线从这一段生成 Release 正文，没有就红在渲染那一步；最上面那一节与 `Cargo.toml` 的版本号对不上由 `the_changelog_top_section_is_the_version_we_ship` 拦。
- [ ] README 不写版本号：两份 README 的抬头与「项目当前状态」只写「见 Releases / CHANGELOG」，不用跟（`the_entry_readmes_write_no_version_number` 守着两份 README 里没有 `X.Y.Z` 形的号）。
- [ ] **README 的「平台」与功能列表**：本版新增了平台或用户看得见的大功能，抬头那行的「平台」与功能段要跟上。
- [ ] 本地全量门禁：`bash tests/scripts/gate-affected.sh --full`，`GATE: OK` 才往下走。
- [ ] **若本版动过滚动 / 渲染管线**（stream · tabs · 会话查看器 · 分支折叠 · render-*）：在 Windows 真机按 `tests/e2e/README.md` 的「人工场景」复核 WebView2（WebKitGTK 没有 `overflow-anchor`，两端补批语义不同）。
- [ ] **若本版加删过依赖**（任一份 `Cargo.lock` 或 `package-lock.json` 变了）：`python3 tests/scripts/third-party-notices.py` 重新生成第三方许可声明并提交（门禁 `release-gate` ⑰ 不一致就红，发版那一趟 `--check` 也拦）。
- [ ] **若本版改过后端源码**：用 `bash tests/scripts/bump-build-id.sh <新 id>` 打新的 `BUILD_ID`（`src/backend/common/id.rs`；子命令集变了它同时换指纹历史表）。走 tag 发版时 `release.yml` 的 `build-backends` job 从源码重编内嵌字节，官方渠道恒一致；**本地手工打包分发**则必须先重编并换掉 `src/frontend/shell/embedded-backends/` 里的字节（`bash tests/scripts/re-embed.sh`），否则装出去的是旧后端，连上后无限重装。
      > 身份住在字节自己里（后端 `lib.rs::CC_MONITOR_BUILD_STAMP`，一段 `#[used] static`），`src/frontend/shell/build.rs` 编译时直接扫，几份内嵌字节自报的那个共同 id 就是 monitor 的「我这一版」（`byte_table.rs::my_backend_id`）。两种情况直接让编译失败：内嵌字节里问不出身份戳、几份内嵌字节不是同一版。「字节自报 == 源码 `BUILD_ID`」那条核对住 `release.yml` 的内嵌校验与 `bash tests/scripts/re-embed.sh --check`，本地手工打包前跑一次后者。出路二选一：① `bash tests/scripts/re-embed.sh` 重编重铺；② `bash tests/scripts/re-embed.sh --clean` 删掉落点，自动部署诚实关闭（目录本就在 `.gitignore` 里）。干净 clone 与 CI 里没有那个目录 ⇒ 「我这一版」是 `None`：不判远端旧、不发起换装，健康信息里说版本不可比。
- [ ] **需要人手跑的 e2e 套件**：权威清单是 `shared_crate_registry_tests.rs::every_test_script_is_either_run_by_ci_or_registered_as_manual` 的 `MANUAL` 表，这里不抄第二份。表里的 `test:f40` 例外：它会往真的 `~/.claude/` 写 fixture，发版前不跑。
- [ ] **关键入口手测**：
  - [ ] 启动 monitor，本机正在跑的会话自动出现 tab
  - [ ] 点 tab 上的 ↗、按 `` ` ``，对应终端到前台
  - [ ] `H` 打开历史浏览器，resume 一个历史会话
  - [ ] `,` 打开设置，悬停各个 `?`，提示框在视口内
  - [ ] 机器页给一个有自定义内容的 profile / rc 装别名块：块外原内容保留，旁边生成 `.ccm-backup-…` 备份
  - [ ] Windows：Windows Terminal 开两个窗口，从标签栏 resume 一个远端会话、各新开一个 PowerShell 标签页 `ssh` 到远端起会话、本机起一个会话、再开一个经典控制台（conhost）里的会话；每个 tab 的 ↗ 各切到自己那个窗口（不装接入块也一样）

---

## 2. Git 操作 + CI

在合并工作树里（§ 1 那几样都已提交、`gate-affected.sh --full` 绿了）：

```bash
git commit -m "release: vX.Y.Z"                       # 不加 AI 署名行
bash tests/scripts/push-candidate.sh <合并工作树名>   # 先推 next，CI 全绿才快进 main，再等 main 上那一趟
git tag vX.Y.Z main
git push origin vX.Y.Z                                # tag push 触发 release.yml
```

`main` 不直接推：它只经 `push-candidate.sh` 收 CI 绿过的提交（头注写着它每一步做什么）。tag 打在 `main` 上那个提交上。

### 2.1 发版的第一道门：CI 必须绿

`release.yml` 的第一个 job 是 **`ci-gate`**：它按 `head_sha` 查同一个 commit 上 `ci.yml` 的 run，没有一条绿的就不放行，后面四个 job（后端交叉编译 / Windows 打包 / Linux 打包 / 手机打包）全部不起。

对发版流程的影响，三条：

1. **tag 打在 CI 已经绿过的提交上**：`push-candidate.sh` 把 main 快进过去之前，同一个提交已在 `next` 上跑绿过。这道门接受这个 commit 上任意一条绿的 CI run（`next`、main 与 tag 那几次跑的是同一棵树、同一份 `ci.yml`），所以它立刻放行。
2. **CI 红 ⇒ 发不出去，这是设计意图，不是故障。** 修 CI，然后在 GitHub 的 Release run 页面只重跑 `ci-gate` 这一个 job（Re-run failed jobs），不必重打 tag。
3. **超时也拦**：查不到 CI run 等 10 分钟、CI 还在跑等 45 分钟，到点判红。

### 2.2 产物

下面这张表是 Release 资产的**登记**：资产名、由哪个 job 上传、进哪份校验和。`release.yml` 三处发布步骤（Windows · Linux · 手机）的 `files:` 与它两向相等，三份校验和由各自 job 里的生成步骤逐个算、算的那几份也与它两向相等（门禁 `release-gate` 那一格核）。改资产名先改这张表。

手机端不从 Release 下后端：APK 里带着同一趟 `build-backends` 编的那两份 Linux 后端，连上一台、那台后端与自己不是同一版就提示更新，按了就从包里装上去。后端两行（两个 musl 目标 x86_64 / aarch64）与 `SHA256SUMS-linux.txt` 照旧是对外的资产名，不随手改。

<!-- 发版资产登记：起 -->
| 资产 | job | 校验和 | 是什么 |
|---|---|---|---|
| `cc-monitor_X.Y.Z_x64-setup.exe` | build-windows | `SHA256SUMS.txt` | NSIS 安装器 |
| `cc-monitor_X.Y.Z_x64_en-US.msi` | build-windows | `SHA256SUMS.txt` | MSI（后缀是 `en-US`：`tauri.conf.json` 没配 WiX 语言 ⇒ 走默认） |
| `cc-monitor.exe` | build-windows | `SHA256SUMS.txt` | 裸 exe（名字是 `src/frontend/shell/Cargo.toml` 里主 `[[bin]]` 的名字；包名仍是 `monitor`） |
| `THIRD-PARTY-NOTICES.txt` | build-windows | `SHA256SUMS.txt` | 第三方许可声明（依赖的版权与许可全文；安装包里也带一份） |
| `SHA256SUMS.txt` | build-windows | — | Windows 那四样的校验和 |
| `cc-monitor_X.Y.Z_amd64.deb` | build-linux | `SHA256SUMS-linux.txt` | Debian / Ubuntu 安装包 |
| `cc-monitor` | build-linux | `SHA256SUMS-linux.txt` | Linux 裸二进制 |
| `cc-monitor-backend-x86_64` | build-linux | `SHA256SUMS-linux.txt` | 远端后端，musl 静态（x86_64） |
| `cc-monitor-backend-aarch64` | build-linux | `SHA256SUMS-linux.txt` | 远端后端，musl 静态（aarch64） |
| `SHA256SUMS-linux.txt` | build-linux | — | Linux 那四样的校验和 |
| `cc-monitor-X.Y.Z.apk` | build-mobile | `SHA256SUMS-mobile.txt` | 手机端安装包（Android，正式签名；versionName 与桌面同一个版本号） |
| `SHA256SUMS-mobile.txt` | build-mobile | — | 手机端那一样的校验和 |
<!-- 发版资产登记：止 -->

### 2.3 手机端（Android）签名包

`release.yml` 的 `build-mobile` 打正式签名的 APK（`./gradlew :app:assembleRelease --no-daemon`），改名成 `cc-monitor-X.Y.Z.apk` 挂到同一个 Release，排在 `build-windows` 之后（Release 由它建，版本号与 tag 一致那道检查由它守）。

- **版本号**：手机的 `versionName` 就是 `package.json` 的 `version`，`versionCode` = 主 × 1_000_000 ＋ 次 × 1_000 ＋ 修订（次、修订各在 0..999），都由 `src/mobile/app/build.gradle.kts` 构建时现读，bump 桌面版本号时手机这份跟着变，不用另改。发版那一趟对着真 APK 再核一次 versionName。
- **签名材料只从 GitHub secrets 来**，四个：`ANDROID_KEYSTORE_B64`（keystore 的 base64）· `ANDROID_KEYSTORE_PASSWORD` · `ANDROID_KEY_ALIAS` · `ANDROID_KEY_PASSWORD`。job 把它们还原成 runner 临时目录里的 keystore 与 keystore.properties（不落工作树、不进日志），打包一结束就删。`ci.yml` 的 mobile 格只打未签名包，不碰这几个 secrets。门禁 `release-gate` ⑱ 逐条核。
- keystore 与 `keystore.properties` 不进仓（仓根与 `src/mobile/` 的 `.gitignore` 都挡着）；本机签名时把 `keystore.properties` 放在 `src/mobile/`，或用 `-Pccm.keystoreProperties=<路径>` 指到别处。
- 那一步会印出 APK 签名证书的指纹（`apksigner verify --print-certs`）：换了钥匙，装着旧版的手机就升级不上，要先卸载。

远端后端的身份在字节自己里（`<<ccm-build-id:…:ccm-build-id>>`），要问它是谁就 `grep -a` 那个串，或跑 `./cc-monitor-backend --ccm-probe` 读 `build=` 那一行。

发布到 https://github.com/bo0Zeng/cc-monitor/releases/tag/vX.Y.Z

### 2.4 发后核对

发版清单只住这一份。Release 页出齐之后，把资产全下到一个空目录里核三件：

- [ ] **校验和**：`sha256sum -c SHA256SUMS.txt` · `sha256sum -c SHA256SUMS-linux.txt` · `sha256sum -c SHA256SUMS-mobile.txt`，每行 `OK`；资产名与 § 2.2 那张表逐个对得上。
- [ ] **各处 `ccm-build-id` 同值**：两份远端后端、`cc-monitor` 与 `cc-monitor.exe`（内嵌的那几份）、APK 里带的那两份（`unzip -p` 出来再 `grep -a`），扫出来的 `<<ccm-build-id:…:ccm-build-id>>` 全是同一个，且等于 `src/backend/common/id.rs` 的 `BUILD_ID`。
- [ ] **安装包覆盖装**：装着上一版的机器上直接装新的 NSIS / MSI / `.deb` / APK（不先卸载），装完起得来、版本是新号、原来的设置与会话还在。

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

Release 正文由流水线生成：`release.yml` 三处发布步骤（`build-windows` 的 `Create / update GitHub Release` · `build-linux` 的 `Append Linux artifacts to the release` · `build-mobile` 的 `Append Android app to the release`）各跑一次 `node tests/scripts/release-notes.mjs RELEASE_BODY.md`，`body_path` 指向它，正文就是 **`CHANGELOG.md` 对应版本段**。发版前要做的只有一件：`CHANGELOG.md` 里有本版那一段；没有就红在渲染那一步，不会静默回落（本地门禁 `release-gate` 那一格提前一步逮同一件事）。

生成器不带下载清单（Release 页本来就逐个列出资产）。要在正文里加一段下载说明就手工加，名字与 § 2.2 那张表逐字相同：

```markdown
**下载**
- `cc-monitor_X.Y.Z_x64-setup.exe` — Windows 普通用户（NSIS）
- `cc-monitor_X.Y.Z_x64_en-US.msi` — 企业 IT 部署（MSI）
- `cc-monitor.exe` — Windows 裸 exe（需 WebView2 + 自管路径）
- `cc-monitor_X.Y.Z_amd64.deb` — Linux（Debian / Ubuntu 系）
- `cc-monitor` — Linux 裸二进制
- `cc-monitor-X.Y.Z.apk` — Android 手机端
- `SHA256SUMS.txt` / `SHA256SUMS-linux.txt` / `SHA256SUMS-mobile.txt` — 校验和（Windows / Linux / 手机各一份）

完整 CHANGELOG 见 [CHANGELOG.md](https://github.com/bo0Zeng/cc-monitor/blob/main/CHANGELOG.md)
```
