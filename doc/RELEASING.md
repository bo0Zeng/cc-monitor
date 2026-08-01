# 发版 SOP + CHANGELOG 写作规范

## 1. 发版前 checklist

详 [CONTRIBUTING.md § 1.5](CONTRIBUTING.md#15-发版前)。摘要：

- [ ] 改 **版本号三处对齐**（package.json + src-tauri/Cargo.toml + src-tauri/tauri.conf.json）
- [ ] `Cargo.lock` 提交
- [ ] [CHANGELOG.md](../CHANGELOG.md) 加新版本段（写法见 § 3）
- [ ] **改 README 的版本号两处**：`README.md` 抬头那行 + 「项目当前状态」块的「版本」
      （`README.en.md` 抬头同理）。
      > **这一条是补出来的，别删。** v3.1→v3.4 **连续四次**发版漏改 README，
      > 于是 README 的「当前版本」长期落后一个大版本；BACKLOG 早把「checklist 里没有
      > README 这一条」点名为**机制性根因**，而根因没修 ⇒ 第四次照样复发。
      > 版本号是最便宜的一条，**测试数 / CI job 数 / 代码行数那几处不要求每版跟**
      >（它们在 README 里有 4-5 份副本、注定会漂；真要治是给它们找单一落点，见 BACKLOG E65）。
- [ ] **README 的「平台」与功能列表**：本版若**新增了平台或用户可感知的大功能**，
      抬头那行的「平台」和状态段的一句话概述要跟上（例：v3.4.0 首发 `.deb`，
      而 README 到 v3.5.0 才补上「Linux」——用户读完会以为不支持）
- [ ] `cargo fmt --check + cargo check + cargo test --all + cargo test -p code-picture-core + cargo test -p branch-core + npm test + npm run coverage + npm run build` 全绿（fmt 不过 CI 会红；`npm test` 含 16 组 node 纯函数 + vitest DOM 1047 测 = 前端 job；后端 `cargo test --all`〔+ `-p code-picture-core` + `-p branch-core`，两者都是 path 依赖非 workspace 成员，`--all` 测不到〕 + 远端 daemon `cargo test`（`remote-daemon-proto/`，含 `cargo fmt --check`）+ Linux app 构建 + 三个 e2e job 都是**独立 CI job**，别漏跑；CI 共 **7** job）
- [ ] **若本版动过滚动/渲染管线**（stream/tabs/session-viewer/branch-fold/render-*）：跑一遍 `e2e/f40-suite.sh`（Linux Xvfb，见 e2e/README）+ Windows 真机把 e2e/README「人工场景」的 WebView2 复核过一遍（WebKitGTK 无 overflow-anchor，两端补批语义不同）
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
- [ ] [CONTRIBUTING.md § 1.5](CONTRIBUTING.md#15-发版前) 列出的关键 UI 入口手测

---

## 2. Git 操作 + CI

```powershell
git commit -m "release: vX.Y.Z"            # 不加 Claude coauthor
git tag vX.Y.Z
git push origin main
git push origin vX.Y.Z                     # tag push 触发 release.yml
```

CI 跑 ~6-8 分钟，产物：

- `cc-monitor_X.Y.Z_x64-setup.exe`（NSIS）
- `cc-monitor_X.Y.Z_x64_zh-CN.msi`（MSI）
- `cc-monitor.exe`（裸 exe）
- `SHA256SUMS`（校验和）

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
- `cc-monitor_X.Y.Z_x64_zh-CN.msi` — 企业 IT 部署（MSI）
- `cc-monitor.exe` — 裸 exe（需 WebView2 + 自管路径）

完整 CHANGELOG 见 [CHANGELOG.md](https://github.com/bo0Zeng/cc-monitor/blob/main/CHANGELOG.md)
```
