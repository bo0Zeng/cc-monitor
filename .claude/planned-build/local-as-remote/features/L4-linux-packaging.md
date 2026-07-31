# L4 — Linux 打包 + 进 CI/release（**已交付；发版 job 从未真跑过，如实标注**）

> 主计划：`../MASTERPLAN.md` §1 L4（P2）· §3 账本第 5 行 · §4 顺序表第 6 位
> 前置：L0 构建半（`4ecd93c`）+ L1（`d8a9df6`/`04d33ca`）—— 都已交付

## 1. 开工复测

| 复测项 | 结果 |
|---|---|
| `tauri.conf.json` 的 `bundle.targets` | `["msi", "nsis"]` —— **纯 Windows**，无 `linux` 段 |
| `release.yml` 触发条件 | `push: tags: ['v*']` —— 加 job **不会**改变触发时机 |
| release 由谁创建 | **`build-windows`**（`softprops/action-gh-release@v2`），`build-daemons` 只产 artifact |
| CI 里能不能装系统包 | **有先例**：`ci.yml` 两处 `apt-get install`（tmux/jq）⇒「不装软件包」那条红线说的是**开发者本机**，不是 runner |
| Linux 二进制叫什么 | **`monitor`**（cargo 包名），不是 productName `cc-monitor` —— 本机 `target/debug/monitor` 实测 |

## 2. 两个 job，分工是刻意的

### 2.1 `ci.yml::linux-app-build` —— 持续信号（**会真跑**）

L0 证明了「这台机器上能构建」，但那是**一台机器一次性的读数**。没有持续信号，Linux 侧的构建
会在没人看的时候烂掉 —— 而打包 job 只在 tag 时跑，**等发版才发现就晚了**。

⇒ 这个 job 是那个发版 job 的**前置信号**。**刻意只 build 不 bundle**：回归信号在「能不能编过」
就拿到了，bundle 多花的几分钟留给 release（那里本来就要跑一次）。

### 2.2 `release.yml::build-linux` —— 产物（**从未真跑过**）

⚠ **它只在 tag 时触发，而本轮红线是不发版 ⇒ 一次都没执行过。**
所以每一处设计都是为了「万一它坏了，坏得可见且不伤及发版」：

| 设计 | 理由 |
|---|---|
| `needs: [build-daemons, build-windows]` | ① Release 由 `build-windows` 创建；两个 job 同时调 `action-gh-release` 会**竞争同一个 release**，串起来就没这个竞态 ② `build-windows` 里那道**四处版本号与 tag 一致**的检查因此**被继承** —— 版本漂了它先失败，本 job 根本不会起 |
| **不重复实现版本检查** | 承上：重复 = 又一个会漂的副本。**同理不重做 daemon 清单校验** —— `build-windows` 已对**同一批 artifact** verify 过（清单存在 + 内容 == 源码 `BUILD_ID`） |
| 失败**不**让 release 消失 | Windows 那半已建好并传完；表现是「workflow 红 + release 里少了 `.deb`」——**可见的缺失，不是静默降级** |
| **不动 `tauri.conf.json`** | `bundle.targets` 是 Windows 主路径的共享面（最高风险）。格式在命令行给：`npx tauri build --bundles deb` |

### 2.3 格式选 **deb** 而不是 AppImage —— 与 STATUS 的建议不同，说清为什么

STATUS 里 `[待定]` 建议 AppImage（单文件、不管发行版依赖）。**那个好处是真的**，但：

- AppImage 打包要**下载 `linuxdeploy`**（构建期网络依赖）+ 依赖 **`libfuse2`**
  （Ubuntu 24.04 起不默认提供）。
- 而这是一个**没法在本地验证**的 job。在它上面叠两个额外失败面，等于把「第一次跑就绿」的
  概率往下压。
- `deb` 用 Tauri 自带的**纯 Rust 打包器**，零外部工具、零网络。

⇒ **先把 deb 跑通，AppImage 留作后续**（§6 登记）。这是「拿不准就别硬做」，不是否掉那个建议。

## 3. ★ 自查抓到两个真 bug（都在这份没法真跑的 YAML 里）

写完回读，抓到两处，**都当场实证**而不是眼看：

| bug | 实证 | 修法 |
|---|---|---|
| checksum 里 `sed 's#.*/##'` 想把路径收成 basename，**却把 hash 一起吃掉了**（`.*/` 是贪婪的，`hash␠␠path/to/` 整段被匹配） | `printf 'deadbeef  a/b/x.deb' \| sed 's#.*/##'` → 输出 **`x.deb`**（hash 没了） | 换 `awk '{n=split($2,p,"/"); print $1 "  " p[n]}'`，实测输出 `deadbeef  x.deb` |
| 产物路径写了 `target/release/cc-monitor`（照 productName 猜的） | `ls target/debug/` → 实际是 **`monitor`**（cargo 包名，与 Windows 那半的 `monitor.exe` 同源） | 改 `monitor`，并把理由写进注释 |

**这正是「没法真跑的东西更要逐行实证」** —— 两个都不会让 YAML 解析失败，只会在真发版那天
悄悄产出一个没有 hash 的校验文件、和一个永远匹配不到的产物路径。

## 4. 可证伪性（本轮没有可变异的对象，不编造变异记录）

改动是两段 workflow YAML，**没有可变异的运行时行为**。代之以四条可复跑的核实：

| # | 核实 | 结果 |
|---|---|---|
| 1 | 两个 workflow 仍是合法 YAML，job 列表符合预期 | ✅ `ci.yml` 7 个 job（+`linux-app-build`）· `release.yml` 3 个（+`build-linux`） |
| 2 | **触发条件未改** | ✅ `ci.yml` 仍 `push:[main, v*] + pull_request:[main]`；`release.yml` 仍 `push: tags:['v*']` |
| 3 | **纯追加** | ✅ `git diff` **零删除行**（151 行全是 `+`） |
| 4 | 新增的 `run:` 块 bash 语法 | ✅ 4 块全过 `bash -n` |

**如实标注**：铁律 8 要求并行多 agent 审计；本会话常驻指令「除非用户要求不开 agent」
⇒ 主线程自查 + 全门禁代替。**这是欠账，不是强度裁剪。**

## 5. 门禁

未碰任何源码 ⇒ 全部沿用 L3a 的读数并复跑确认：monitor `cargo test --all` **638** ·
clippy lib **36** · npm **872/58** · tsc **0** · daemon **173** · shellcheck **37 rc=0** ·
`tauri.conf.json` **未动**。

## 6. 签收与登记

- [x] 复测五条（bundle targets 纯 Windows · 触发条件 · release 由谁创建 · CI 装包有先例 · 二进制真名）
- [x] `ci.yml::linux-app-build`：Linux 构建的**持续信号**，是发版 job 的前置
- [x] `release.yml::build-linux`：deb 产物，串在 Windows 之后 ⇒ **继承版本检查、避开 release 竞态、失败不伤发版**
- [x] **不动 `tauri.conf.json`**；格式走命令行
- [x] 自查抓到并实证修复两个真 bug（吃掉 hash 的 `sed` · 猜错的二进制名）
- [ ] **`build-linux` 从未真跑过** —— 只在 tag 时触发，本轮红线不发版。**没验就是没验**
- [ ] **AppImage** —— §2.3，等 deb 在真发版上跑通后再加
- [ ] **deb 装上去能不能跑**（依赖是否齐全、桌面项是否正确）—— 要真机装包，本轮没做
