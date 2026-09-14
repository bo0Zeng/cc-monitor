# K-R114 · 死值验与实测读数（`KR114D1` / `KR114D2`）

> 一切在沙箱里跑（`K31`）。宿主上没跑过 `cargo` / `npm`。
> 本文里每个数都带「量于哪一刻 · 用什么量的 · 分母怎么数的」。

## 〇 · 量具住址（三份，各自唯一）

| 量具 | 住址 | 被测对象指向哪棵树 |
|---|---|---|
| `release.yml` 手工触发守卫（**判据本体**） | `.github/workflows/ci.yml` 的 `e2e-smoke` job，步骤名 `release.yml 手工触发守卫（KR114D1）`（跟仓走，可重跑） | 由环境变量 `RELEASE_WORKFLOW` 给；不给则 `.github/workflows/release.yml` |
| 判据的**跑法**（从 `ci.yml` 里现抽那一步的 `run:` 再执行） | `scratchpad/kr114/kr114-guard-runner.py`（本轮会话专属） | 命令行第 1 参给的仓根 = `.claude/worktrees/k-r114` |
| TLS 产物探针（`KR114D2` 的判据） | 本文件 `§三·附` 的整块（同一份也落在 `scratchpad/kr114/kr114-tls-probe.sh`） | 命令行第 1 参给的二进制路径 |

⚠ **诚实边界**：`scratchpad/kr114/` 里另有一份 `M0-gate.txt`（mtime 03:02），**不是本轮这一路落的**
（本路 08:09 才落 `M0.txt`）—— 临时目录是共用的，两份归一化后不同形，我没拿它当读数。

## 一 · `M0` 基线（现打，量于 `979aeab` ＋ 工作树把本轮 workflow 改动 `git stash` 掉之后）

命令：`PB_WS=backend-consolidation .claude/devbox/gate <k-r114 工作树> k-r114`

```
ok   hooks          11 passed
ok   fmt            1 passed        （绿/红两态）
ok   fmt-daemon     1 passed        （绿/红两态）
ok   winchk         1 passed        （绿/红两态；cargo check --target x86_64-pc-windows-gnu）
ok   cargo          1610 passed     （9 个包合计）
分母 cargo          本树未铺 src-tauri/embedded-daemons/ ⇒ embedded_daemons cfg 不置
                    ⇒ 合计里少了「本地后端真的能起来吗」那一族（4 条）
ok   generated      与 Rust 源一致
ok   daemon         761 passed      （单包 remote-daemon-proto）
ok   npm            1726 passed     （17 个套件里只有 2 个打得出数字，取最大值 ⇒ 恒是 test:dom 的）
ok   ccm e2e        ccm-print-parity     PASS=12（地板 12）
ok   ccm e2e        ccm-rbind-title      PASS=8 （地板 8）
ok   ccm e2e        ccm-cli              PASS=46（地板 46）
ok   ccm e2e        ccm-contract-parity  PASS=45（地板 45）
ok   pb check       [backend-consolidation] FAIL=0 BROKEN=0
GATE: OK —— 13 格全绿   ·   GATE_EXIT=0
```

## 二 · `KR114D1` —— `workflow_dispatch` ＋「不推 artifact」的开关

### 2.1 判据是什么（12 行，逐行一格）

判据住 `ci.yml`，读 `release.yml` 的 **YAML 字段**（不在散文里找结论），逐格比**整格**、不比子串：
2 条地板（触发器解析得出来 · job 数 ≥ 4）＋ ① 触发得了 ＋ ② 三条（有 `publish` · 是 boolean ·
默认 false）＋ ③ `env.PUBLISH` 与钉住的字面逐字相同 ＋ ④ 一条地板 ＋ 两处上传步骤各一格 ＋
⑤ 一条地板 ＋ CI 门那一格。

**绿趟（盘上那一份，量于 `d1a0552`，沙箱 `ccmon-zigbox:k-r114`）：红 0 条 / 12 行全 PASS。**
关键几行的真实输出：

```
PASS  ①触发得了（有 workflow_dispatch） :: on.workflow_dispatch = {'inputs': {'publish': {..., 'type': 'boolean', 'default': False}}}
PASS  ③env.PUBLISH 与钉住的字面逐字相同 :: 盘上="${{ github.event_name == 'push' || inputs.publish == true }}"
PASS  ④地板·找得到「往 Release 上写」的步骤 :: 2 处（分母 = 全部 job 的全部 step 共 35 个）
PASS  ④闸·build-windows / Create / update GitHub Release :: if="env.PUBLISH == 'true'"
PASS  ④闸·build-linux / Append Linux artifacts to the release :: if="env.PUBLISH == 'true'"
PASS  ⑤CI 门挂在同一个闸上 :: if="env.PUBLISH == 'true'"
---- 红 0 条（分母 = 上面逐行 PASS/FAIL 打出来的那几条）----   GUARD_EXIT=0
```

### 2.2 变异表（每刀一份**新副本**，夹具名一律中性 `wf-a…wf-d`）

| 刀 | 切在哪 / 锚点命中 | 射程（只打这一面） | 红几条 | 红的是不是正是那几格 |
|---|---|---|---|---|
| **①** 摘掉 `workflow_dispatch` 整块 | `release.yml` 的 `on:`；锚点 `  workflow_dispatch:\n` 命中 **1** 次 | 只动 `on:`，不碰 `env` / 闸 / CI 门 | **2** | ✅ `①触发得了` ＋ `②有 publish 这个输入`（后者是前者的必然连带：块没了，`inputs` 也没了）。其余 8 行仍绿 |
| **②a** 开关默认反过来 `default: false → true` | `        default: false\n`，命中 **1** 次 | **一个 token**，最小面 | **1** | ✅ 恰好是 `②publish 默认 false`，输出 `default=True` |
| **②b** 开关表达式反过来 `== true → != true` | `  PUBLISH: ${{ … inputs.publish == true }}\n`，命中 **1** 次 | 只动文件头那一行 | **1** | ✅ 恰好是 `③env.PUBLISH 与钉住的字面逐字相同` |
| **②c** 只反两处上传步骤的闸 `== 'true' → != 'true'` | `        if: env.PUBLISH == 'true'\n        uses: softprops/action-gh-release@v2\n`，命中 **2** 次 | 只动那两步，不碰 `on` / `env` / CI 门 | **2** | ✅ 恰好是 `④闸·build-windows` ＋ `④闸·build-linux` |
| **退实现** 本轮 `release.yml` 改动整个退掉（= `git show HEAD~1:…`） | 整份 | —— | **6** | 见下 |

**「把实现整个退掉，还有多少条新断言仍绿」**：退掉后判据只印 **10** 行
（`②publish 是 boolean` 与 `②publish 默认 false` 两行因为 `inputs.publish` 根本不存在而不再打印），
其中 **6 红 4 绿**。仍绿的 4 条**逐条给理由**：

| 仍绿的 | 理由 |
|---|---|
| `地板·触发器解析得出来` | 它是**地板**，本来就该在两侧都绿（证「够得到」，不证「有没有违例」） |
| `地板·job 数` | 同上 |
| `④地板·找得到「往 Release 上写」的步骤` | 同上；上传步骤两侧都在（本轮加的是闸，不是步骤） |
| `⑤地板·找得到 CI 门那一步` | 同上；CI 门那一步 09-09 就有了，本轮只给它挂了闸 |

⇒ **4 条仍绿全部是地板，一条牙都没有**。这正是想要的形状。

### 2.3 🔴 真触发一次（`KR114D1` 逐字要的那一条）

| 项 | 值 |
|---|---|
| 触发方式 | `gh workflow run release.yml --ref track/k-r114 -f publish=false`（退出码 0） |
| **run id** | **`34861383050`** |
| 住址 | `https://github.com/bo0Zeng/cc-monitor/actions/runs/34861383050` |
| event / branch / head sha | `workflow_dispatch` / `track/k-r114` / `d1a0552` |
| 起于 | 2026-09-14T15:19:18Z |

⚠ **顺带否掉一条我开工时以为成立的事**：GitHub 文档那句「`workflow_dispatch` 的 workflow 必须在默认分支上」
**在本仓今天不挡人** —— `origin/main`（`14e0f05`）上那份 `release.yml` **没有** `workflow_dispatch`，
而从 `track/k-r114` 触发**直接成功**。⇒ 「要先合 main 才能手工触发」这条**不是**阻塞项。
（这一条是实打出来的，不是读文档推的。）

### 2.4 那一趟真跑的**结论**（逐 job / 逐步，全部取自云端 API，不是我转述）

| job | 结论 |
|---|---|
| `Gate on CI (same commit)` | **success**（其唯一那一步 `Require a green CI run on this exact commit` → **skipped** —— 🔴 这是闸真的生效了的**无歧义**证据：job 绿而步骤跳，只有 `if: env.PUBLISH == 'true'` 能造成这一形） |
| `Cross-compile remote daemons (musl)` | **success**（`Cross-compile daemon for both musl targets` → success；`Stage binaries` → success；artifact 上传 → success） |
| `Build Windows artifacts` | 🔴 **failure**，死在第 13 步 `tauri build` |
| `Build Linux artifacts (deb)` | **skipped**（`needs: build-windows`） |
| 整体 | `completed / failure`，2026-09-14T15:27:53Z |

**这一轮 `release.yml` 那几步「一次都没在 CI 上跑过」的改动，今天全跑了一遍，逐条真实输出**：

```
embedded x86_64 (7573832 bytes)
embedded aarch64 (6729680 bytes)
expecting stamp: <<ccm-build-id:p2j-bus-state:ccm-build-id>>
stamp x86_64 = p2j-bus-state (从字节里扫出来的，不是从旁边抄的)
stamp aarch64 = p2j-bus-state (从字节里扫出来的，不是从旁边抄的)
Version self-consistent: 3.7.0 (package.json / tauri.conf / Cargo.toml / Cargo.lock all agree)
演练（publish=false）：跳过「与 tag 一致」那一半；ref_name=track/k-r114 不是版本号
Backend identity OK: remote-daemon-proto version=0.0.0 (deliberate), BUILD_ID=p2j-bus-state
native daemon staged: src-tauri/native-daemon/cc-monitor-native (7921664 bytes, target=x86_64-pc-windows-msvc)
```

⇒ `.build_id` 清单换成「从字节里扫身份戳」（`K-R70`）· 后端身份检查 ·
`Stage native daemon for self-extract`（`K-R42`）· 本轮版本号那一步拆两半 —— **四处全部实跑通过**。

### 2.5 🔴 这一趟演练**逮到一条真的发版阻塞项**（这就是 `KU27` 要的那个东西）

`tauri build` 的 `beforeBuildCommand`（`npm run build` = `tsc && vite build`）**编不过**，
云端逐字 6 条，退出码 2：

```
src/views/history.ts(1601,9): error TS2322: Type 'Counted<number>' is not assignable to type 'number | null'.
src/views/history.ts(1603,9): …（同形）
src/views/history.ts(1640,9): …
src/views/history.ts(1642,9): …
src/views/history.ts(1838,20): …
src/views/history.ts(1839,19): …
```

**沙箱里重打一遍，同形**（`docker run … ccmon-zigbox:k-r114 npx tsc --noEmit`）：**同样 6 条，`TSC_EXIT=2`**。
⇒ 不是云端环境的事，是这棵树今天真的编不过。

**根因（现打，三处住址）**：
- `src/views/counted.ts:26` — `export type Counted<T> = T | null | undefined;`
- `src/generated/HistoryProject.ts:26` — `starredCount: number | null,`（少一态 `undefined`）
- `src/views/history.ts:1601` 等 6 处 — `proj.starredCount = bumpCounted(proj.starredCount, ±1)`

引入于 `e707acd`（`K-R92`，2026-09-12 20:59，合并于 `d9086e4`）。

🔴 **门禁为什么全绿**：13 格里的 `npm` 那一格跑的是 `npm test`（16 tsx + 1 vitest），
**不含 `tsc`**（`package.json` 里有 `check:types`，但门禁不调它）。
云端 `ci.yml` 的 `frontend` job 有 `npx tsc --noEmit` —— 而 `ci.yml` 只在
`push:branches:[main] / tags / PR` 上触发，**这 210 个提交一次都没进过 `origin/main`** ⇒ 它一次都没跑。
⇒ **「本机门禁不是云端的超集」这一课今天又付了一次费**（`.claude/devbox/Dockerfile` 头注 09-10 记的那一条同族）。

🔴 **本件不修它**：`src/**` 明令不在写区（件文件 `§2`）。**端给 PM，发版前必修。**

## 三 · `KR114D2` —— zigbuild aarch64 **带 TLS** 实编

### 3.1 沙箱（新造，用完即弃；**宿主上没装 zig / 交叉工具链**）

`ccmon-zigbox:k-r114` = `ccmon-devbox:latest` ＋ zig `0.14.0` ＋ cargo-zigbuild `0.23.0`
＋ `rustup target add aarch64-unknown-linux-musl x86_64-unknown-linux-musl` ＋ `binutils` / `file` / `python3-yaml`。
🔴 **zig 与 cargo-zigbuild 的版本逐字照抄 `release.yml` 的 `build-daemons`**，不是随手挑的。
Dockerfile 住 `scratchpad/zigbox/Dockerfile`。

### 3.2 编出来了 —— 风险 `5v` 的第一半**买到了**

```
$ cargo zigbuild --release --locked --target aarch64-unknown-linux-musl
    Finished `release` profile [optimized] target(s) in 47.71s     （ZIGBUILD_EXIT=0）
```

产物住址（在沙箱里生成，落在**工作树之外**，不脏树）：
`/home/zbl/文档/claudecode-frontend/.claude/pm-targets/k-r114-zig/aarch64-unknown-linux-musl/release/cc-monitor-remote`

```
file:      ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV), statically linked, stripped
readelf -h: Class=ELF64  Type=EXEC  Machine=AArch64
readelf -d: There is no dynamic section in this file.      （= 静态）
size:      6722608 字节
sha256:    6d91f326bea85c7b61f7eb15c88ae5a7f45d75581e060d3b186b69b184abc5ce
```

### 3.3 「TLS 真进去了」的读数 —— **读产物字节，不读 Cargo.toml**

| 判据 | 读数 | 怎么量的 |
|---|---|---|
| `T1` 是 aarch64 | `Machine = AArch64` | `readelf -h` |
| `T2` **信任锚在产物里** | **6/6** 个 CA subject 命中：`[ISRG Root X1=1] [Amazon Root CA 1=1] [GlobalSign Root CA=2] [Go Daddy Root Certificate Authority - G2=1] [Certum Trusted Network CA=2] [USERTrust RSA Certification Authority=1]` | `strings -a \| grep -c -F`，分母 = 探针里那 6 个名字（现算 `${#CAS[@]}`） |
| `T3` rustls 代码在产物里 | distinct rustls 源路径串 = **33**（地板 20）例：`rustls-0.23.43/src/client/hs.rs` | `strings -a \| grep -oE 'rustls-0\.23\.[0-9]+/src/…\.rs' \| sort -u \| wc -l` |
| `T4` 静态链接 | 没有动态段 | `readelf -d` |

⚠ **为什么 `T2` 才是「TLS 真进去了」的那一格**：`rustls` 有代码但信任锚是空的，那个二进制
**连不上任何真实 https**。而 `remote-daemon-proto/src/relay/upstream.rs:230-236` 的 docstring
逐字记着这条病曾经真的发生过：「把这里的根证书集整个换成空 `Vec::new()`，**384 条判据全绿**（审计 `CS` 实测）」
—— 本仓自己的单测**逮不住**它，`T2` 逮得住。
（这一句我**没有重打**那 384 那个数，住址就在那一行。）

### 3.4 死值验（一刀）—— 证明判据看的是产物不是配置

**做法**：把源码整棵拷到沙箱里的副本（`101 个 .rs / 9 个 Cargo.toml`，副本里没有 `.git`），
先原样编一趟作**控制组**，再切一刀重编。

**锚点**：`roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),`
—— `src/relay/upstream.rs` 命中 **1** 次、`src/sidecars/codepicture/acquire.rs` 命中 **1** 次
（切之前逐个断言过，命中数不是 1 就停）。换成 `roots: Vec::new(),`。**变异已落地：2 处，各 1 次。**

🔴 **`Cargo.toml` 一个字节没动** —— `md5` 变异前后同为 `3259259b7ba537aeefd03ad8a4822704`（现打两次）。

| 组 | 产物 sha256 / 大小 | T1 | **T2** | T3 | T4 | 退出码 |
|---|---|---|---|---|---|---|
| 控制（副本原样） | `54b5367158306df3…` / 6722448 | PASS | **PASS 6/6** | PASS 33 | PASS | 0 |
| 变异（root store 换空集） | `81f451c8219ef57d…` / 6662568 | PASS | 🔴 **FAIL 0/6** | PASS 33 | PASS | 1 |

⇒ **判据红了，而且红的正是 `T2` 那一格**；产物小了 **59880** 字节（信任锚那一块没了）。
配置面一个字节没变 ⇒ **它看的是产物。**

⚠ **射程，写死别读宽**：这一刀**没有**让 `T3` 红 —— `rustls` 的代码仍在树里，只是没有根证书可信。
`T3` 的牙在另一把刀上（把 `rustls` 从依赖树里摘掉），那一刀本轮**没切**。
⇒ 现在能说的是「`T2` 有牙」，**不能**说「四条都有牙」。

### 3.5 🔴 它**跑起来了** —— 在 `qemu-aarch64` 用户态模拟下（沙箱内，`--network none`）

这台机器没有 aarch64 硬件。**但产物不必因此「一次都没执行过」**：
在沙箱里装 `qemu-user-static`（**只装在容器里，宿主一个包都没装；也没动宿主的 binfmt**），
把那份 aarch64 二进制**真的跑起来**。

**① 以本名跑（进常驻流模式），第一帧 `hello` 逐字**：

```json
{"kind":"hello","v":1,"build_id":"p2j-bus-state","host_arch":"aarch64","claude_dir":"/tmp/fakehome/.claude",
 "capabilities":["bg","tail-only"],
 "emits":["line","session_added","session_status","session_removed","overflow","turn_end","tmux_sessions","tmux_session_closed"],
 "commands":["bus-kill","bus-list","bus-send","bus-state","cancel","capture-pane","kill","launch","oneshot-session","ping","resolve"]}
```

三条现打读数：`host_arch` 是**进程自己报的** `aarch64`（不是我从 `file` 抄的）·
`build_id` = `p2j-bus-state`（与源码那一处逐字相同）·
`commands` 里**有 `bus-state`**（`K-R113` 那条新原语在 aarch64 产物里真的在）。
收到 SIGTERM 时逐字 `shutdown signal received; exiting`，退出码 0。

**② 以 `argv[0] = ccm` 跑（一次性 CLI 模式）**，`--ccm-probe` 六行逐字：

```
name=ccm
version=5
self=/tmp/bin/ccm
capabilities=new,resume,attach,tmux,account,model,cwd,agent,launcher,ccm-sid,print,detach,tmux-size,tmux-base,bus-register,daemon-discover,account-via-daemon,base-url-across-tmux
agents=claude,codex
build=p2j-bus-state
```

⇒ `K33`「后端只有一个：常驻模式与 `ccm` 是同一个二进制的两种模式」这件事，
**在 aarch64 产物上也成立**（同一份字节，两种 `argv[0]`，两种模式都答得出来）。

### 3.6 判不了的那一半（诚实边界，收窄之后重写）

🔴 **`qemu-user` 买到的比真机少，逐条写死**：
- 它模拟的是 **aarch64 用户态指令**，**系统调用仍然由这台机器的 x86_64 内核服务**
  ⇒ 「真 aarch64 内核上对不对」（页大小 64K、内存序、真实 `/proc`、真 tmux）**一格都没买到**。
- 那一趟是 `--network none` ⇒ **TLS 一次都没真握手过**。`T2` 证的是「信任锚在产物字节里」，
  **不是**「它连得上 https」。
- 没有真 tmux、没有真 `~/.claude` ⇒ 观测面那一半跑的是「零会话」那一支。

⇒ 今天准确的说法：**编得出 · 静态 · 信任锚在产物里 · 在 qemu-aarch64 下跑得起来并答得出身份；
真 aarch64 机器上没跑过、TLS 没握过手。**
`remote-daemon-proto/Cargo.toml` 那条登记（「这一格是「没验」，不是「验过没事」」）
今天该按这句话改写 —— ⚠ 那份文件明令不在写区，**我没改**，落点交 PM。
运行期剩下的那一半已经写进 `evidence/K-R114-真机清单.md` 的 `R6`。

## 附 · TLS 产物探针全文（`§〇` 那张表的第三行；同一份落在 `scratchpad/kr114/kr114-tls-probe.sh`）

```bash
#!/usr/bin/env bash
# 只读产物字节，一个字节都不读 Cargo.toml / CI 配置。用法：<脚本> <二进制路径>
set -uo pipefail
BIN="${1:?用法: kr114-tls-probe.sh <二进制>}"
fails=0
say(){ if [ "$1" -eq 0 ]; then echo "PASS  $2 :: $3"; else echo "FAIL  $2 :: $3"; fails=$((fails+1)); fi; }
if [ ! -s "$BIN" ]; then echo "::error::产物不在或是 0 字节 —— 判不了，按红记"; exit 2; fi
echo "地板  产物在且非空 :: $(stat -c %s "$BIN") 字节 · sha256=$(sha256sum "$BIN" | cut -d' ' -f1)"
mach=$(readelf -h "$BIN" | sed -n 's/^ *Machine: *//p')
[ "$mach" = "AArch64" ]; say $? "T1 产物是 aarch64" "readelf -h 的 Machine = $mach"
CAS=("ISRG Root X1" "Amazon Root CA 1" "GlobalSign Root CA" \
     "Go Daddy Root Certificate Authority - G2" "Certum Trusted Network CA" \
     "USERTrust RSA Certification Authority")
hit=0; det=""
for ca in "${CAS[@]}"; do
  n=$(strings -a "$BIN" | grep -c -F "$ca"); det="$det [${ca}=${n}]"; [ "$n" -gt 0 ] && hit=$((hit+1))
done
[ "$hit" -eq "${#CAS[@]}" ]; say $? "T2 信任锚在产物字节里" "${hit}/${#CAS[@]} 个 CA subject 命中 ·$det"
r=$(strings -a "$BIN" | grep -oE 'rustls-0\.23\.[0-9]+/src/[a-zA-Z0-9_/]*\.rs' | sort -u | wc -l)
[ "$r" -ge 20 ]; say $? "T3 rustls 代码在产物里" "distinct rustls 源路径串 = $r（地板 20）"
if readelf -d "$BIN" 2>&1 | grep -q 'There is no dynamic section'; then
  say 0 "T4 静态链接" "readelf -d：没有动态段"
else
  say 1 "T4 静态链接" "readelf -d：有动态段 ⇒ 不是静态 musl"
fi
echo "---- 红 $fails 条（分母 = 上面 T1–T4 共 4 条）----"
exit $([ "$fails" -eq 0 ] && echo 0 || echo 1)
```
