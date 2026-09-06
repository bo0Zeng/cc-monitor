#!/usr/bin/env bash
# 出货前的**唯一闸门**：三道门跑一遍，末尾只吐一行裁决。
#
# # 它解决的是一个**过程**问题，不是技术问题
#
# `C7` 的门禁是「全量 npm test + 全量 cargo test，`0 passed` 不是绿」。三条命令分散着，
# 于是很容易写成 `cargo test … && npm test … && git commit -F -` 这种一条龙 ——
# **08-13 我就这么干了一次，`1140 passed; 1 failed` 的那行滚过去没人看，红着就出货了**
#（下一拍单独 commit 订正）。
#
# 病根不是「忘了看」，是**读数与动作被塞进同一条命令**：一条龙的输出很长，
# 而 `git commit` 的成功回显在最后一行，看起来像「都好了」。
#
# ⇒ 本脚本把三道门（+ 下面那道 `generated` 生成物漂移检查，K-A1 第四轮补的）收成一条命令，
# 并且**只在最后打一行裁决**（`GATE: OK` / `GATE: FAIL …`）。
# 用法就一句纪律：**先跑它、看见 `GATE: OK`，再单独敲 `git commit`。**
# ⚠ 它**故意不提交任何东西**、也不接 `--commit` 之类的开关 —— 那会把刚拆开的两件事又焊回去。
#
# ⚠ 覆盖面如实写：它跑的是**工作树**的三道门 + 一道生成物漂移检查 + `pb check` + 四套 `ccm` e2e。
#   （`K-G7` 09-03 从两套改成四套 —— 拦路石是一个 `jq`，见下面那一节。）
# · 跨平台 / 提交状态那一维归 `npm run verify:committed`（`C16`，动 daemon 时跑）；
#
# ★★ `K-G3`（09-01）：**「真机 e2e 本脚本不跑它们」这句话已经作废，但只作废了 2/6。**
#    ★ **`K-G7`（09-03）：作废到 4/6。** 剩下没作废的那 2/6 是 `ccm-acceptance` /
#      `ccm-pretrust` —— 拦它们的**不是** `jq`（那个本拍解开了），是沙箱里各红 1 条，
#      另一笔账，见下面那一节。
#
# 上一版这一行逐字写着「真机 e2e 归各自的套件（本脚本不跑它们 —— 它们要 tmux/Xvfb，
# 几分钟起步）」。`丙1-f1` 逮到的正是这句话与那句「出货前的**唯一闸门**」对不上：
# `grep -c ccm scripts/gate.sh` = **0**（PM 08-24 独立复核，K-G3 09-01 在 `b28464e` 上复打，仍是 0）。
#
# **「几分钟起步」这个理由现打是假的**（09-01，沙箱 `ccmon-devbox:latest` 里逐套计时）：
#   · `ccm-print-parity` **1.28 秒** · `ccm-rbind-title` **0.28 秒**（两套合计 ≈ 1.6 秒）
#   · `ccm-cli` 7.16 秒 · `ccm-contract-parity` 5.60 秒
#   · `ccm-acceptance` 37.7 秒 · `ccm-pretrust` 35.6 秒
# 分母：门禁基线墙钟 **148 秒**（09-01，同一沙箱，同一棵树，`k-g3-c1` target）。
#
# ⚠⚠ **那个 148 秒今天不能拿来跟本文件里任何新读数相减**〔`K-G7` 09-03〕：
#   它量于 **09-01 的旧镜像**、`k-g3-c1` 那个 target 目录、那一天的主干。
#   本拍在 **`644ea0ce5c3d` 新镜像** + `k-p2` target（6.0 GB，热）上现打的同一趟基线是
#   **45 秒**。⇒ 两个数分母不同，**差的 103 秒里有多少是镜像、有多少是 target 冷热、
#   有多少是主干长胖，本拍没有拆开量，别当成「换镜像快了 103 秒」。**
#   墙钟**从来不是**这道门的判据，各格 passed 数才是 —— 那些数逐格可比，理由见下。
#
# ⚠ **为什么当初只挂两套，另外四套的确切拦路石**（如实写，别读成「它们太慢」）：
#   · `ccm-cli` / `ccm-acceptance` / `ccm-contract-parity` / `ccm-pretrust` 都硬依赖 `jq`，
#     而**当时的沙箱镜像 `ccmon-devbox:latest` 里没有 `jq`**（`K-G3` 09-01 现打：
#     `command -v jq` ⇒ MISSING）。四套都是 fail-closed 的（自己打「需要 jq」再 exit 1），
#     所以它们**不会假绿**，但当天挂上去就是四条恒红 ⇒ 不挂。
#   · 在一份**只多装了 `jq`** 的探针镜像上现打过：`ccm-cli` 126 PASS/0 FAIL、
#     `ccm-contract-parity` 68 PASS/0 FAIL（这两套加 `jq` 就能挂，合计 +12.8 秒）；
#     而 `ccm-acceptance` 28/1、`ccm-pretrust` 14/1 —— **沙箱里各红 1 条**，
#     那是另一笔账（`.claude/devbox/gate` 头注自己写着「容器里 `HOME` 几乎是空的」）。
#   · `.claude/devbox/Dockerfile` 不在 `K-G3` 的写区 ⇒ 加 `jq` 这一步交回 PM 裁。
#
# ★★ **`K-G7`（09-03）：那块拦路石没了 —— 两套变四套。**
#
# PM 09-03 裁「加」，重建了 `ccmon-devbox:latest`（现物 **`644ea0ce5c3d`**，容器里现打
# `jq-1.7` 在 `/usr/bin/jq`）⇒ 下面多挂了 `ccm-cli` 与 `ccm-contract-parity` 两行。
#
# ⚠⚠ **`K-G8`（09-03 下午）订正：上面那个「现物」今天已经不是盘上那个了。**
#   现打 `docker image inspect ccmon-devbox:latest` ⇒ **`sha256:553f5932…`**（建于 09-03T11:17）。
#   ⇒ **`644ea0ce5c3d` 与下面那个「沙箱 `644ea0ce5c3d`」都要按「那一拍量于哪个镜像」读，
#     不是「今天的镜像」。** 旧数**刻意不删** —— 它记着「哪一次重建买到了 `jq`」。
#   🔴 这一族（散文里一个**现在时**的住址，盘上已经没有了）本仓 09-03 一天逮到两次，
#     另一次是 `K-G8` 摸底件 `§7.0` 那个 `.sh` 死住址。`K-R20` 那道闸**两次都够不着**
#     （① 住在注释里；② 不是 `snake_case` 标识符）⇒ **镜像 ID 写进散文时要带日期。**
#
#   · 🔴 **地板本拍重打，不沿用 09-01 那两个数**（主干此后动了很多次，那两个数已经馊了）。
#     量于 **`b8a6ecd`**（= 本件基点 `4558394` + 一次纯 `evidence/` 提交，`scripts/` 与
#     `e2e/` 一个字节没动），沙箱 `644ea0ce5c3d`，连打**两趟**、两趟同值：
#       `ccm-cli` **126 PASS / 0 FAIL**（6.78 秒） ·
#       `ccm-contract-parity` **68 PASS / 0 FAIL**（5.58 秒） —— 两套 **0 条 SKIP**。
#     ⇒ 重打的结果与 09-01 那两个数**相等**，但**那是重打出来的相等，不是沿用**。
#     合计 **+12.36 秒**（同一趟现打，不是从 09-01 的 12.8 抄的）。
#
#   · **这两个地板与 `.github/workflows/ci.yml` 的调用行同值**，刻意的：
#     `ci.yml` 里 `assert-pass-floor.sh ccm-cli <地板>` · `… ccm-contract-parity <地板>` 那两行。
#     ⚠ **09-03 `K-P2 D2` 把这里的行号拆掉了，那不是洁癖**：原文写的是 `ci.yml:559` / `:574`，
#       而本拍在 `ci.yml` 那两行**上方**加了一段棘轮注释 ⇒ 两个行号当场双双失真
#       （**559 → 575** · **574 → 590**）。**按名字指，不写行号**（`e2e/ccm-cli.test.sh` 头注同一条纪律）。
#       ★ **这两个数在本拍之内就漂了两次**：写这段话时量到的是 571 / 586，
#         而后来又往那段棘轮注释里补了 4 行 ⇒ 再漂成 575 / 590。
#         **「行号会失真」这句话在同一拍里自己被验了一遍。**
#     ⚠ **现值 09-03 `D3` 起是 `ccm-cli 242` · `ccm-contract-parity 68`**（下面那两行 `run_e2e` 同值）。
#     一个性质两把尺子是本区最贵那族病（`K13`）⇒ 这里不另起一个数。
#     ⚠ **代价如实写：同值靠的是纪律，不是判据。** 今天没有任何东西会在「本文件的地板」
#       与「`ci.yml` 的地板」分叉时报红（`ci.yml` 那张 `pair` 反向清单只看它自己那个文件）。
#       ⇒ **棘地板要三处一起改**：`ci.yml` 调用行 · `ci.yml` 的 `pair` 清单 · 本文件这两行。
#       ⚠⚠ **`K-G8`（09-03）订正上面这段的两处，别照旧读**：
#         ① 「本文件这两行」今天是**四行**（`K-G7` 从两套挂到四套）。
#         ② 「没有任何东西会在分叉时报红」**只剩半句是真的**：本文件的 `run_e2e` 改成
#            **恒等**（`exact`）之后，「**本文件的地板陈了**」这一侧**当场红**
#            （实得 > 本文件地板 ⇒ 红）。仍然没人管的是**另一侧** ——
#            `ci.yml` 的地板陈了而本文件跟上了：那种分叉今天照旧没有判据，
#            因为 `ci.yml` 那 19 条挂在一条 29 天没通电的流水线上（归 `K-G3`）。
#            ⇒ **「三处一起改」这条纪律一个字不变**，变的只是**忘了改本文件那一处会响**。
#
#   · **仍然不挂 `ccm-acceptance` / `ccm-pretrust`** —— `jq` 只解开了四套里的两套，
#     那两套在沙箱里各红 1 条（上一段那笔账），挂了就是两条恒红。
#
#   ⚠⚠ **`K-P2 F` 拍（09-04）订正上面那句的一半，并报一条它买不到的东西。**
#     现打（沙箱 `ccmon-devbox:latest`，工作树 `k-p2f` @ `d305ffa` 的 `shared/ccm`，
#     量具 `evidence/K-P2-F-suites.py`）：
#       · `ccm-acceptance`  **29 PASS / 0 FAIL（rc=0）** —— **它今天是绿的**，
#         上面那句「各红 1 条」对它**已经馊了**（那是 09-01 在一份探针镜像上量的 28/1）；
#       · `ccm-pretrust`    **14 PASS / 1 FAIL（rc=1）** —— 这一半仍然属实。
#     ⇒ **不挂 `ccm-acceptance` 的理由今天只剩「没人回来重量过」**。
#     🔴 **而这一格是有代价的，本拍现打逮到了**：同一趟量具在**改后**那一侧读到
#       `ccm-acceptance` **5/24**、`cc-spawn-uplift` **31/41**、`ccm-pretrust` **12/3**，
#       而**门禁四格只看见 `ccm-cli` 那 9 条** ⇒ 一次真行为变更的 71 条红里，
#       **这道门看得见 9 条、看不见 62 条**。
#     ⇒ 该不该把 `ccm-acceptance` 挂上来是 `K-G` 那一族的活（挂它要连 `cc-spawn-uplift`
#       一起想，两套都真起 tmux），**`K-P2` 不自批**；这里只把**读数**订正到今天，
#       并把「62 条看不见」这个分母写下来 —— 不写下来，下一个人会照着上面那句旧话
#       继续以为「不挂是因为它红」。
#
#   ★★ **`F` 拍后半的收尾读数（同一趟量具，改完之后重打）**：那 71 条**全部回绿**，
#     而且六套里四套还涨了：`ccm-cli` 242→**264** · `ccm-contract-parity` 68→**72** ·
#     `ccm-acceptance` 29→**31** · `ccm-pretrust` 14/1→**15/0** ·
#     `ccm-print-parity` 12（=）· `ccm-rbind-title` 8（=）。
#     `cc-spawn-uplift` 67/5→**71/1**：那 1 条是**沙箱的既有红**（非 ASCII 目录名在容器里
#     被搞成 `__ ____ ______`，locale 的事），**与 `shared/ccm` 无关** ——
#     现打对照：同一份新夹具喂**旧** `shared/ccm` 也是 **71/1**，逐字相同。
#     买到这些的是一份**可复用的假后端** `e2e/fake-daemon.sh`（六套共用一份，不是六份各写一遍）。
#
#   🔴 **给 `K-G` 的建议（只写建议，本件不挂）**：把 `ccm-acceptance` 挂进本门。
#     理由三条，都是本拍现打出来的：
#       ① 它今天在沙箱里 **31/0 绿**（挂上去不是「两条恒红」）；
#       ② 本拍那次行为变更里它一个人就吃了 **24/71** —— 门禁看不见的 62 条里它占最大一块；
#       ③ 它是四套里**唯一真起 tmux** 的那一套 ⇒ 它买的是「命令在真 tmux 上干了什么」，
#          与现挂四套（都只看串与字节）**不同源**。
#     ⚠ 代价如实写：它 **37.7 秒**（本文件上面那段计时），比现挂四套合计还长一倍多；
#       而 `cc-spawn-uplift` **今天仍挂不了**（沙箱那 1 条既有红，那是另一笔账 ——
#       修它要动容器 locale，不在本件写区）。
#
# ⚠ **`jq` 哪天从镜像里没了，这两行是 fail-closed 的**（`K-G7 §4` 死值验现打）：
#   把 `/usr/bin/jq` 挡掉再跑**整趟**门禁 ⇒ 那两格双双红、诊断原文里带着套件自己打的
#   「需要 jq」、末行 `GATE: FAIL` ⇒ **不会退化成静默跳过**。
#
# ⚠ **`K-G3` 那条「诚实边界」的前提没了，连它一起订正。** 原文逐字：
#   「这两套买不到 `K-C1` 那 54 条 …… 只对 20 条断言成立（12 + 8）」——
#   而那 54 条按原文自己的说法就住在 `ccm-cli` / `ccm-contract-parity` 里，本拍挂上了
#   ⇒ **那个「只对 20 条成立」的读数到此作废**，`K-G7` 当天四套合计 **214 条**（12 + 8 + 126 + 68）。
#   ⚠ **09-03 `K-P2` `D` 阶段之后这个合计是 291**（12 + 8 + **203** + 68）—— 量于那趟
#     沙箱门禁的四格读数，**不是从 214 加出来的**（`ccm-cli` 那一格是重打的）。
#   ⚠ **`D3`（生产接线）之后是 330**（12 + 8 + **242** + 68）—— 同样量于本拍那趟沙箱门禁的
#     四格读数，**不是从 291 加出来的**。
#   ⚠ 别把这一格读大成另一个方向，两条：
#     ① **214 是「跑了多少条断言」，不是「盖住了多少行为」**；
#     ② 「那 54 条住在这两套里」是**引原文**，`K-G7` **没有独立复核**过这个归属
#        （已在件文件 `§7` 列为未核项）—— 别拿它当已证的数。
#
# ★★ 本脚本**今天仍然盖不到的两维**（`K-G3` `己1-f13` / `己1-f14`，09-01 现打；
#    写在这里是因为「自称的射程 > 实际盖住的面」正是这个文件被立案的原因）：
#
#   ① **Windows 那半编不编得过**：`grep -c -- --target` 本文件 = **0**。
#      `creds-core` 的 `harden` feature 带 `#[cfg(windows)]` 的平台原语，
#      本脚本跑在 Linux 上 ⇒ 那一段**根本不参与编译**。
#      刀已切过（把 `perm.rs` 的 `FILE_ATTRIBUTE_NORMAL` 改坏）：
#      `cargo check --target x86_64-pc-windows-msvc` **rc=101**，而**本脚本六格全绿、印 `GATE: OK`**。
#      买法是一行 `cargo check -p creds-core --features harden --target x86_64-pc-windows-msvc`
#      （冷 5.67s / 热 0.15s），**卡在沙箱镜像没装那个 target**（`rustup target list --installed`
#      只有 `x86_64-unknown-linux-gnu`）⇒ 归 PM。
#      ⚠ 诚实边界：那一行买的是「**编得过**」，**买不到「行为对」** —— 行为要真 Windows 机器，
#      那一格今天是**判不了**，不是「通过」。
#
#   ② **格式漂移**：`grep -c fmt` 本文件 = **0**。
#      `cargo fmt --all --check` 在 `b28464e` 上是 **rc=1 / 78 处 / 18 个文件 / 1.09 秒**
#      （存量最重的是 `local_daemon.rs` 28 处）。**卡在沙箱镜像没装 `rustfmt` 组件**
#      （`cargo fmt --version` 报 `'cargo-fmt' is not installed`）⇒ 归 PM。
#
#   ⚠ 这两条**不是「以后再说」**，是「买法在写区外」：两条都要改
#     `.claude/devbox/Dockerfile`（55+ 棵树共用、且不在任何 git 仓里）。
#     在它改之前，把这两维写成一道门 = 把 55 棵树的门禁一起打红。
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2
fails=()

# ── 失败诊断（`K-R22` 09-04）：**红的那一格必须自带「为什么红」** ────────────────
#
# 病灶逐字：本文件把子命令的输出吃进变量，而**失败支只记退出码、把输出整个丢掉**
# ⇒ 终端上只剩一行 `GATE: FAIL —— cargo（退出码 101）`，哪条测试红的一个字都没有。
#
# ★★ **它比「少印一段日志」严重，理由是那个退出码装着好几件互不相关的事。**
#   `cargo` 的 `101` 在本仓至少是四件：① 真有测试红 ② **编译错误（测试一条都没跑）**
#   ③ **被内核杀掉（OOM）** ④ 环境不满足（`--network none` 下 blackhole 判据秒失败）。
#   四件事的处置完全不同，而**唯一能把它们分开的就是被丢掉的那段字**。
#   09-04 一天里 PM 与三路 agent **各自独立**撞上，四次都得把那一格重跑一遍才知道是哪一件。
#   ⇒ 这正是本区最贵那族病（**一个值装了几件事**）长在门禁自己身上。
#
# ★ **人群不是一个 `if`，是 6 处**（`K-R22 D1` 量于 `5e1d07b`，主尺 `grep -n 'fails+=('`
#   得 11 处判红支，真跑了命令的 10 处里**全丢 6 处**：`run_gate` 两支 · `run_gate_sum`
#   三支 · `run_e2e` 的「抓不到数」支）⇒ **修必须落在共用的这一段上**，落在某一个 `if`
#   里面只治得了六分之一。
#
# ★ 取法：**两段 + 一条兜底**，两侧都要避（印太少还得重跑；印太多把裁决那行埋掉）。
#   ① **关键行**：整段输出里匹配下面那张模式表的行（`-A1` 带一行下文），封顶 `GATE_DIAG_KEY` 行。
#      模式表按**形状**选，不按工具选 —— 每一条都点名它治的是哪一形：
#        `^error`              cargo 的 `error[E0xxx]:` · `error: could not compile` ·
#                              `error: test failed`；**被信号杀掉那一形也走它**
#                              （`error: could not compile … (signal: 9, SIGKILL: kill)`）
#        `^thread .+ panicked` panic 落点（自带 `文件:行:列`）；`-A1` 把断言原文带出来
#        `^failures:`          cargo 失败清单的头
#        `^test result: FAILED` 那一格的合计（几过几败）
#        `^ *Running`          正在跑哪个测试二进制 —— **被杀那一形唯一能说明「死在哪个包」的行**
#        `^npm error` `^npm ERR!` npm 两代前缀
#        `^::error::`          `e2e/assert-pass-floor.sh` 自己的诊断
#        `^ *(FAIL|BROKEN|×|✗)` 本仓 bash e2e 与 `pb check` 自己的失败行
#   ② **原文尾部** `GATE_DIAG_TAIL` 行：**恒印**。
#   ③ 输出短于 `GATE_DIAG_WHOLE` 行 ⇒ 不摘要，**全印**（那一形上摘要的收益是负的）。
#
# ★★ **② 是 fail-closed 的那一半，别当冗余删掉。**
#   ① 是一张**模式表**，而模式表天生会漏（换了工具 / 换了措辞 / 本地化）。
#   ⚠ 直答 `K-R22 D2` 那问：**编译错误那一形没有 `failures:` 段** ——
#     它落在 `^error` 与 `-A1` 带出来的 `--> 文件:行:列` 上（死值验现打 5 行，件文件 `§5`）；
#     而**假如哪天它连 `^error` 都不匹配**，② 仍把最后几十行原样端上来
#     ⇒ **「印出零个字」在任何形状上都不可能**。这条比模式表准不准重要得多。
#
# ⚠ 每行加 `  | ` 前缀是**承重的，不是排版**：被测命令的输出里要是自己打了一行
#   `GATE: OK` 或 `  ok   xxx`，不带前缀就会**混进本脚本自己的裁决面**。
#
# ⚠ **它一个字都没碰任何一条判定**（`K-G3 §143` 硬边界：那五道门的口径不许动）——
#   `fails+=` 的条件、包数自检、`0 passed 不是绿`，逐字原样。本段只加「印什么」。
GATE_DIAG_KEY="${GATE_DIAG_KEY:-40}"
GATE_DIAG_TAIL="${GATE_DIAG_TAIL:-30}"
GATE_DIAG_WHOLE="${GATE_DIAG_WHOLE:-60}"
GATE_DIAG_PAT='^(error|npm error|npm ERR!|thread .+ panicked|failures:|test result: FAILED|::error::)|^[[:space:]]*(FAIL|BROKEN|Running|×|✗)'

# ── `N-G1`（09-05）：**①段匹配之前先去一次色。同样一个字都没碰任何一条判定。** ────
#
# ★★ **病因是现打的，不是猜的。** 上一版这里的解释停在「模式表漏了这一形」，而
#   `N-F1b` / `N-F2` 两件的实现方连着两次在交回里写「关键行**一条都没匹配上**」
#   ⇒ 每一件的死值验都只剩「红了几条」，**是哪几条只能靠推断**。
#
#   `NG1D1` 的量法（量具住 `evidence/N-G1-diag-match.py`，模式表从本文件现读，不复述）：
#   在沙箱里故意让一条前端判据红，把 `out="$(npm test 2>&1)"` 那一份**原始** stdout+stderr
#   拿去 `od -c` —— ⚠ **不是**门禁已经加过 `  | ` 前缀的日志（PM 就在那上面栽过一次）。
#
#   现打的读数：`vitest 4.1.10` 在**非 TTY**（命令替换 · `TERM=dumb` · `FORCE_COLOR`/
#   `NO_COLOR`/`CI` 三个都没设）下**照样上色**，2413 行输出里 **1584 行含 `ESC`**。
#   那行 ` FAIL ` 的头 15 个字节 `od -c` 逐字：
#     033  [  4  1  m  033  [  1  m  空格  F  A  I  L  空格
#   ⇒ **行首不是空白，是 `ESC[41m``ESC[1m`**（徽章的红底＋粗体）。
#   而模式表那一支是 `^[[:space:]]*(FAIL|…)`，`ESC`(0x1b) **不属于** `[[:space:]]`
#   ⇒ 锚点 `^` 之后第一个字节就不匹配。**那句「一条都没匹配上」是被这两个 ESC 挡出来的。**
#   同一份输出里 `   × <用例名>` 那行同样被 `ESC[31m` 挡着。
#
# ★ **每一格各量了一次**（分母：**7 份原始输出**，盖住走 `gate_diag` 的 **5 个门禁格**
#   —— `cargo` · `daemon` · `npm` · `e2e` · `pb check`；全是现打，不是抽样。
#   ⚠ 第九格 `generated` 不进这个分母：它红了走的是 `git diff --stat`，根本不调 `gate_diag`）：
#     npm·vitest      2413 行 · 含 ESC **1584** 行 · 改前命中 **0** · 去色后 **2**  ← 只有它中招
#     npm·tsx（`✗`）   186 行 · 含 ESC 0 · 改前 **1** · 去色后 1
#     cargo·workspace 1740 行 · 含 ESC 0 · 改前 **11** · 去色后 11
#     cargo·daemon     642 行 · 含 ESC 0 · 改前 **6**  · 去色后 6
#     cargo·编译错误    15 行 · 含 ESC 0 · 改前 **2**  · 去色后 2
#     e2e·地板不符      28 行 · 含 ESC 0 · 改前 **1**  · 去色后 1
#     pb check         40 行 · 含 ESC 0 · 改前 **3**  · 去色后 3
#   ⇒ **cargo / e2e / pb check 三形本来就是好的**（cargo 与本仓 bash e2e 在非 TTY 下不上色）；
#     病**只在 npm 的 vitest 那一形**上。
#   ⚠ **没量的，写出来**：Windows 上的任何一形 · CI runner 上的任何一形 ·
#     显式 `FORCE_COLOR=1` 的跑法 · npm 那 16 个 tsx 套件里除 `format` 外的 15 个。
#
# ★ 取法：**只在①段之前去一次色**，`sed` 只吃 CSI（`ESC [ … 一个字母`）。
#   ⚠ **② 原文尾部与 ③ 短输出全印，一个字节都没动。** 那两段的性质就是「**原样**端上来」，
#     给它们去色等于把「原样」降级成「差不多」；`NG1D4` 反过来钉住②。
#   ⚠ **不顺手扩模式表去治别的工具**（件计划 `§2` 逐字）：量出来的病是**色码**，不是表少了词。
#     扩表治不了下一个上色的工具，去色治得了；而表**天生会漏** —— 那正是②存在的理由。
#   ⚠ 去色的射程只到 CSI，**OSC（`ESC ] … BEL`）不在里面** —— 现打：7 份里 `ESC ]` **0 处**，
#     而且去色之后 7 份**残留 `ESC` 行全 0** ⇒ 这一形上这条 `sed` 是够的（换个工具就未必）。
#   ⚠ 去色只喂给 `grep`，也**只影响①段印出来的那几行**（印出来的是去色版，终端上更好读）；
#     `$out` 本身一个字节没改，②仍拿它原样端。
#
# ⚠ **它一个字都没碰任何一条判定**（`K-G3 §143` 硬边界，与上面那段头注同一条）——
#   `fails+=` 的条件、包数自检、`0 passed 不是绿`，逐字原样。本段仍然只改「印什么」。
#   ★ **这句话有读数，不是自称**：`run_gate` / `run_gate_sum` / `run_e2e` 三块**整块 md5**
#     在 `333fcde` 与本拍之间**逐字节相同**（`8d0ac685b949` / `a2da94965516` / `48f1c09cf1b0`），
#     量具住 `evidence/N-G1-gate-fn-md5.py`（可复跑）。
#   ⚠ **一处别读窄**：本文件全文 `fails+=(` 从 **15** 涨到 **16**，涨的那一处**在 `gate_selftest` 里**
#     （下面新加的自检④）—— 与 `K-R22` 立探针①②③ 时同一个说法：**新加的一格自检，
#     不是改了哪一道旧门**。「逐字原样」说的是那五道门的判红条件，不是「全文一处没加」。
gate_decolor() { sed $'s/\033\\[[0-9;:?]*[a-zA-Z]//g'; }

gate_diag() {
  local name="$1"; local out="$2"
  local total key
  # 「输出为空」本身就是一条读数，不许静默 —— **被信号杀掉那一形长这样**。
  if [ -z "$out" ]; then
    printf '  ---- %s 诊断：被测命令 stdout+stderr **一个字都没有**（退出码非零而输出为空——多半是被信号杀掉，如 OOM）\n' "$name"
    return 0
  fi
  total="$(printf '%s\n' "$out" | wc -l)"
  if [ "$total" -le "$GATE_DIAG_WHOLE" ]; then
    printf '  ---- %s 失败原文（全 %s 行）----\n' "$name" "$total"
    printf '%s\n' "$out" | sed 's/^/  | /'
    printf '  ---- %s 诊断完 ----\n' "$name"
    return 0
  fi
  # N-G1：`gate_decolor` 只加在**①这一条管道**上（②在下面，拿的仍是 `$out` 原样）。
  key="$(printf '%s\n' "$out" | gate_decolor | grep -E -A1 "$GATE_DIAG_PAT" | grep -v '^--$' | head -n "$GATE_DIAG_KEY")"
  if [ -n "$key" ]; then
    printf '  ---- %s 关键行（%s 行输出里匹配到的，封顶 %s 行）----\n' "$name" "$total" "$GATE_DIAG_KEY"
    printf '%s\n' "$key" | sed 's/^/  | /'
  else
    printf '  ---- %s 关键行：一条都没匹配上（共 %s 行）—— 模式表漏了这一形，只看下面的尾部\n' "$name" "$total"
  fi
  printf '  ---- %s 原文尾部 %s 行（共 %s 行）----\n' "$name" "$GATE_DIAG_TAIL" "$total"
  printf '%s\n' "$out" | tail -n "$GATE_DIAG_TAIL" | sed 's/^/  | /'
  printf '  ---- %s 诊断完 ----\n' "$name"
}

# ★★ `K-G3`（09-01）第二个参数 `denom` 是**这个数的分母**，跟着绿行一起印出来。
#
# 它治的是题面里的**第 5 个洞**：`sort -rn | head -1` 取的是**所有 `N passed` 里的最大值**，
# 而 `npm test` 是 **17** 个套件（1 个 vitest `test:dom` + **16** 个 `tsx`）用 `&&` 串起来的。
#
# ★ **分母现打（`K-G3` 09-01，跑了一趟真 `npm test` 数命中行，不是抽样）**：
#   整趟输出里命中 `([0-9]+) (passed|个测试)` 的**只有 3 行** ——
#   `test:diff` 的 `17 passed, 0 failed`（`src/cards/diff.test.ts:234`）·
#   vitest 的 `117 passed`（Test Files）与 `1480 passed`（Tests）。
#   ⇒ **16 个 tsx 套件里有 15 个不带数字**（`all X tests passed` 那一形），**第 16 个（`diff`）带**，
#   但它的 17 被 `sort -rn` 吃掉 ⇒ **`n` 仍恒等于 `test:dom` 那一个数**。
#   ⚠ 件文件 `§0b-1` 逐字写的是「**16** 个 tsx 套件……**没有数字**」—— 那句是 **15/16**，
#   已在 `§4a` 订正；**结论不受影响**（多出来的那个数比它小，`max` 照样吃掉）。
#
# ⚠ 洞的准确形状（别读大）：那 16 套的**失败**逮得到 —— `&&` 链里任一非零退出码
#   都会走上面 `rc != 0` 那一支。逮不到的是「某套**跑了 0 个测试**却照样 exit 0」
#   （文件改名 / `describe` 被注释 / glob 没匹配上）：它照打那句 `all X tests passed`、
#   照退 0 ⇒ `n` 仍是 `test:dom` 的数 ⇒ 全绿。**`C7` 那条「0 passed 不是绿」，
#   在 16/17 的面上是空的。**
#
# ⚠ **本参数不是判据，是分母** —— 它一个字都没改上面那两条自检（`K-G3 §2` 逐字禁止）。
#   买的只有一件事：**那行绿不再自称它不是的东西**。PM 08-29 逐字承认过被它骗：
#   「我这一整窗汇报里写的每一个 `npm 1512 passed`，读法都错了 —— 那不是
#   『npm 门跑了 1512 个测试』，是『`test:dom` 这一个套件 1512 个』。」
#   ⇒ 与 `K-R10` 给 `pb check` 那行加 `[$PB_WS]` 是同一条道理：
#   **一行不带分母的读数，不论数字是几都不算数。**
#
# ⚠ `fails` 那两支**刻意没动**：`K-G3 §2` 写死「`0 passed 不是绿` 这条自检一个字不许改」。
#   代价如实记：**红的那一行今天仍不带分母。** 要补得连着改那条自检的字面，归 PM 裁。
run_gate() {
  local name="$1"; local denom="$2"; shift 2
  local out
  out="$("$@" 2>&1)"
  local rc=$?
  # ⚠ **`rc=0` 不等于绿**：`0 passed` 也会 rc=0（`C7` 逐字：「0 passed 不是绿」）。
  #   ⇒ 两条都判：退出码 + 那行读数里的数字。
  local n
  n="$(printf '%s' "$out" | grep -oE '([0-9]+) (passed|个测试)' | grep -oE '[0-9]+' | sort -rn | head -1)"
  if [ "$rc" -ne 0 ]; then
    fails+=("$name（退出码 $rc）")
    gate_diag "$name" "$out"          # K-R22：判定一个字没动，只是把 $out 端出来
  elif [ -z "$n" ] || [ "$n" -eq 0 ]; then
    fails+=("$name（读数是 ${n:-<找不到>} —— 0 passed 不是绿）")
    gate_diag "$name" "$out"          # K-R22：这一支同样得说清「那它到底打了什么」
  else
    printf '  ok   %-14s %s passed（分母：%s）\n' "$name" "$n" "$denom"
  fi
}

# ★★ `K-H2a`（08-27）：**`--workspace` 是补上来的 —— 在此之前，7 个共享 crate 的判据
#    一条都不在这道门里。**
#
# 起因：`K-H2a` 把 key 那件事落在新开的 `crates/creds-core`，写完 18 条判据、`GATE: OK`，
# 而 `cargo` 那个数**一条没涨**（1195 → 1195）。它正是 `KP3` 那个形状：
# 「有生成物 / 有判据」**不等于**「本地门禁拦得住」。
# 现打的分母（08-27，`cargo test -p <名> --lib` 逐个数）：
#   `guard-core 24 · creds-core 18 · usage-core 11 · acct-core 9 · branch-core 8 ·
#    gate-core 8 · shell-quote-core 1` ⇒ **79 条**，其中 **61 条是本件之前就有的存量**。
#
# ⚠⚠ **`--exclude code-picture-core` 是承重的，不许删成裸 `--workspace`。**
# PM 08-27 现打三个数：`--lib` **1195** · 裸 `--workspace --lib` **1299** ·
# 带 exclude **1274** —— 差恰好 **25**，就是 vendor 那 25 条。
# 而 `C7` 逐字：「vendor `code-picture-core` **不动**」⇒ 裸 `--workspace` 会把 25 条
# **我们无权修**的判据拉进出货门禁：它们哪天红了我们修不了也不许修，
# 那是一道**我们满足不了的闸**，比没有闸更坏。
#
# ⚠ 另记一条**反直觉**的读数（`己1-f9` 独立跟进，本处不修）：
# `src-tauri/Cargo.toml:22` **明明写着** `exclude = ["vendor/code-picture-core"]`，
# **而 cargo 不认** —— `cargo metadata --no-deps` 的权威 member 名单 9 个里就有它。
# ⇒「配置里写了 exclude」**≠**「cargo 认它被排除了」，所以这里必须再显式排一次。
#
# ⚠ **CI 那一侧没跟着改**（`ci.yml` 不在 `K-H2a` 的写区）⇒ 从此**本地门禁比 CI 严**。
#   别把「本地绿」读成「CI 也会绿」。
# ★★ **它必须求和，不能沿用 `run_gate`** —— 08-27 实测：只把命令换成 `--workspace`、
#    读法照旧，那道门印的仍是 **1195**。
#
# 原因在 `run_gate` 的读法本身：`sort -rn | head -1` 取的是**所有 `N passed` 里的最大值**。
# 单包时只有一行，最大值 = 合计；**多包时它恒等于最大那个包**（`monitor` 的 1195），
# 于是往任何一个共享 crate 加判据，这个数**永远不动**。
# ⇒ 那正是本轮要治的病换了个位置又长出来一次：**命令的射程扩了，读数的射程没扩。**
#
# 本函数改成**逐行求和**，并且**钉死包数**（不是松地板）：
# 一个 crate 静默掉出 `--workspace`（改名 / members 漏登记）时，合计只会**变小**，
# 而「变小」和「有测试没跑」在终端上一模一样 —— 只有包数相等断言认得出来。
# 〔同一条道理 `platform/fallback_guard.rs` 逐字论证过：「第一版是 `checked >= 3`，
#  而实测 `checked = 7` —— 余量 2.3 倍，4 个块可以静默掉出采集面而地板照绿」。〕
run_gate_sum() {
  local name="$1"; local want_pkgs="$2"; shift 2
  local out
  out="$("$@" 2>&1)"
  local rc=$?
  local lines n pkgs
  lines="$(printf '%s' "$out" | grep -oE '^test result: ok\. [0-9]+ passed')"
  pkgs="$(printf '%s' "$lines" | grep -c . || true)"
  n="$(printf '%s' "$lines" | grep -oE '[0-9]+' | paste -sd+ - | bc 2>/dev/null || echo 0)"
  if [ "$rc" -ne 0 ]; then
    fails+=("$name（退出码 $rc）")
    gate_diag "$name" "$out"          # K-R22：本件的病灶就是这一支，判定一个字没动
  elif [ "$pkgs" -ne "$want_pkgs" ]; then
    # ⚠ 这一支是**采集面自检**，不是测试失败：包数对不上 ⇒ 下面那个合计不算数。
    fails+=("$name（只跑到 $pkgs 个包，应当 $want_pkgs —— 有包静默掉出了 --workspace；\
合计变小与「有测试没跑」在终端上一模一样，只有这条认得出来。真加/删了 crate 就来改这个数）")
    # K-R22：这一形最需要的恰是那几行 `Running …`（到底跑到了哪几个包），模式表里有它。
    gate_diag "$name" "$out"
  elif [ -z "$n" ] || [ "$n" -eq 0 ]; then
    fails+=("$name（读数是 ${n:-<找不到>} —— 0 passed 不是绿）")
    gate_diag "$name" "$out"
  else
    printf '  ok   %-14s %s passed（%s 个包合计）\n' "$name" "$n" "$pkgs"
  fi
}

# ── 门禁自己的自检（`K-R22 D3`）：**「盘上有 echo」不等于「失败时真的印了」** ──────
#
# ★ 这一格**最容易假绿的写法**是钉一条 `grep -q gate_diag scripts/gate.sh` ——
#   那只证明**盘上有**，不证明**被走到**（`K-R18` 语料八：盘上有 ≠ 被走到）。
#   ⇒ 这里**真跑必红的合成命令**，断言那一趟的**标准输出里出现被测命令自己印的哨兵串**。
#   删掉任何一个 `gate_diag` 调用、或把取法改回「只记退出码」，这几条里至少一条当场红。
#
# ⚠ **成本与副作用 —— `N-G2`（09-05）现打，把上一版那句话改掉了两处。**
#   上一版逐字：「四条探针合计 ≈ 10 毫秒：**不碰 cargo / npm / 网络 / 文件系统**」。
#   量具 `evidence/N-G2-selftest-cost.py`（把自检段真正会走到的那几块**原样切下来**跑，
#   不复算），沙箱 `ccmon-devbox:latest` 里各 9 趟交替跑、取中位数：
#     · **本拍十条（①–⑩）≈ 131 ms**（135.1 ms 减去 4.5 ms 的空跑基线 = **130.6 ms**）
#     · 同一把尺子量基点 `5924b91` **那四条 ≈ 54 ms**（58.5 − 4.2 = 54.3 ms）
#   ⇒ 🔴 **「四条 ≈ 10 毫秒」这句话本身就不成立**（同一把尺子今天量得 54 ms，差 5 倍）；
#     `N-G2` 只答「新加六条要多少」= **≈ +76 ms**。每条探针要 fork 一个 `bash`，那才是大头。
#     ⚠ 这两个数量于**这台机器、这个镜像、这一拍**，是快照不是常量 —— 引用前重打。
#     ⚠ 它们量的是**自检段**，不是整趟门禁（同一拍那一趟墙钟 61 秒）；**两个分母不同，别相减**。
#   ⇒ 「不碰 cargo / npm / 网络」**仍然成立**；**「不碰文件系统」对探针⑩ 不成立**（见下）。
# ⚠ 上面 `N-G1` 那段头注里「全文 `fails+=(` 从 **15** 涨到 **16**」是**那一拍的快照**，
#   今天已经不是全文的数了。本拍现打 **17 处真的 `fails+=`** = 门那侧 **11** ＋ 探针那侧 **6**
#   （口径与量具住 `evidence/N-G2-verdict-md5.py` 的【覆盖自证】，它按「整行 strip 后以 `#` 打头」
#    剔注释）。⚠ 裸 `grep -c` 会比它多几处 —— 多出来的是**头注里引它当主尺的那几行**，
#   **本行自己就是其中一行** ⇒ 那个数每写一句话就变一次，别拿它当读数。
# ⚠ 它们跑在**子 shell**（`$( )`）里 ⇒ 里面那几个 `fails+=` 落在数组副本上，
#   污染不到真裁决；能漏出来的只有标准输出，而那正是要断言的东西。
# ⚠ 探针② 走的是**编译错误那一形**（**没有 `failures:` 段**）—— `K-R22 D2` 特意问的那一格，
#   从此每趟出货都验一遍，不是只在死值验那天验过一次。
# ⚠ 探针③ 造的输出**长过 `GATE_DIAG_WHOLE`（不走「全印」那条捷径）且一条模式都不匹配**，
#   哨兵只出现在最后几行 ⇒ **它只能靠「原文尾部恒印」那半兜底才看得见**。
#   那半是 fail-closed 的承重墙，得有一条判据专门盯着它。
#   ★★ **`NG1D4` 就钉在这一条上**：把 `gate_diag` 里「原文尾部恒印」那两行删掉 ⇒ 探针③ 当场红。
#      09-05 现打验过（死值验 `G1M2`），不是读注释读出来的。
# ⚠ 探针④（`N-G1` 09-05）是探针③的**镜像**，钉的是**另一半**（①关键行真的匹配上了）：
#   它造的失败行**行首带 ANSI 色码**（`ESC[41m``ESC[1m` —— 就是 vitest 那一形逐字节量出来的样子），
#   哨兵**只在这一行上**，而后面垫的无关行条数是 `GATE_DIAG_TAIL + GATE_DIAG_WHOLE + 10`
#   ⇒ ★ 两条捷径**结构上都走不到**：总行数恒大于 `GATE_DIAG_WHOLE`（不走③「全印」）、
#     哨兵那行恒在**尾部 `GATE_DIAG_TAIL` 行之外**（②兜不住它）。
#   ⇒ **它只有在①真的匹配上时才看得见哨兵。** 这正是 `NG1D2` 的 acceptor 点名要挡的那条捷径：
#     「把上限调大让尾部把名字裹进来**不算**」—— 垫的条数跟着上限走，调多大都裹不进来。
#   ⚠ 垫的条数**现算、不写死**：写死一个常数，哪天有人把 `GATE_DIAG_TAIL` 调到比它大，
#     这条判据就静默退化成「②兜底了」，而输出长得一模一样。
#
# ★ 这是**新加的一格自检**，不是改了哪一道旧门 —— `K-G3 §143` 那五道门的判定口径逐字未动。

# ── `N-G2`（09-05）：断「**这一条判定还在判**」的六条（探针⑤–⑩） ──────────────
#
# ★★ **病灶是现打的，不是推出来的**：把 `run_gate` 的 `if [ "$rc" -ne 0 ]` 改成 `-gt 1000`
#   （那一格从此**不按退出码判红**）⇒ **整趟门禁 `GATE: OK`，一格没红**。
#   `N-G1` 实现方自报（死值验 `G1M3`），**PM 独立复现过**（`audits/N-G1-PM.md` `§四 刀乙`）。
#
# ★ **上面那四条为什么接不住**：它们断的是「失败那条路**印没印出**被测命令的输出」。
#   单刀掏掉一条判定时，同一份合成输出会**落到隔壁那条判定上**：那一格照旧红、照旧调
#   `gate_diag`、照旧印出哨兵 ⇒ 四条探针全都满意。**替身接住了，牙一口没咬到。**
#
#   ⚠ **这不是漂移，是当初就没人立过这一格**（`NG2D1` 查了五处，头注之外的独立证据三条）：
#   `K-R22` 的题面逐字是「门禁红了**不说为什么红**」，`K-G3 §143` 又禁它碰任何一条判定
#   （逐字：「不动那五道已有的门的判定口径」）；件文件 `KR22D3` 那张探针表把每条探针
#   「专门盯的失效」写成 `gate_diag` 调用被删 / `D2` 那格退化 / 兜底那半塌掉，并把射程逐字
#   写成「**`gate_diag` 本体 ＋ `run_gate` / `run_gate_sum` 两个失败支**，8 个调用点里的 2 个」；
#   `audits/K-R22-PM.md §五1` PM 逐条认了这个射程。⇒ **「哪一条判定在判」从来不在谁的射程里。**
#
# ★ 取法：喂一份**只有目标那一条判定拦得住**的合成输入，**两侧都断**：
#   ① **那一格的绿行不出现**（`  ok   ` 那个前缀）—— 这是本族探针唯一的牙：
#      目标判定被掏空 ⇒ 输入一路落到 `else` ⇒ 那道门**印出绿行** ⇒ 当场红。
#   ② **哨兵在**（失败支真走到、`gate_diag` 真把输出端出来）—— 挡**空真**：
#      门整个没跑起来时输出为空，光断①**会恒真**（`brief` 第 9 条那一族）。
#   ⚠ ①用的是**裸前缀 `  ok   `**，不是「`  ok   ` ＋ 探针自己的名字」：探针名是夹具的名字，
#     拿它当断言子串正是 `brief` 第 12 条点名的那条路。三个函数的绿行逐字都以 `  ok   ` 开头，
#     而每条探针的 `$out` 只装**它自己那一次调用**的标准输出（子 shell）⇒ 里面的绿行只可能是它的。
#
# ⚠ **「只有它拦得住」是每条探针的承重前提**，不是修辞：合成输入必须同时**满足**同一个函数里
#   其余每一条判定。逐条写在每条探针自己的注释里。写错了那一条就退化成「隔壁接住了」，
#   而**死值验会当场逮到**（掏掉目标判定却不红）—— 这一族每条都验过会红、也验过会绿。
#
# ⚠ **射程如实写：11 条判定盖住 6 条，盖不住 5 条。**
#   盖住：`run_gate` 两条（`:352` `:355`）· `run_gate_sum` 三条（`:409` `:413` `:418`）·
#         `run_e2e` 的退出码那条（`:626`）。
#   盖不住 5 条，逐条给理由（`§4` 登记）：
#     · `generated` 两条（`:560` `:564`）与 `pb check` 两条（`:714` `:736`）—— **行内，不是函数**，
#       **没有可以喂合成输入的入口**。三条出路各有代价：抽成函数 = 改判定的形状（件计划 `§2` 明禁）·
#       另写一份独立复算 = 「盘上有 ≠ 被走到」的假绿 · 造一次 `git diff` 非空要**改工作树**
#       （`NG2D5` 硬边界明禁）。⇒ **做不到**，不硬凑。
#     · `run_e2e` 的「抓不到「合计 PASS=」」那条（`:634`）—— 立它要 `rc=0` **且**抓不到那行，
#       而 `e2e/assert-pass-floor.sh` **只有跑完一整套 npm 套件才退 0** ⇒ 立它就得每趟真跑一套。
#       ⇒ **没立**（代价与自检段「不碰 npm」那条承诺直接冲突）。
#
# 🔴 **探针不许改工作树**（`NG2D5` 硬边界）：这十条一个 git 写操作都没有、不往工作树落文件、
#   不碰 `~/.claude`。⚠ 一条例外如实写：探针⑩ **读**了 `e2e/assert-pass-floor.sh`（起一个 `bash`）
#   ⇒ 上面那句「不碰文件系统」**对它不成立**，头注已改。它在**地板参数校验**那一步就 `exit 2`，
#   而那一步排在 `npm run` 与 `mktemp` **之前** ⇒ 走不到 npm、也不落任何文件。
#   ⚠ 它因此**依赖写区外一份文件的一行措辞**（那句 `地板必须是非负整数，实得：$FLOOR` 会把
#     参数原样回显，探针⑩ 的哨兵与那个 `合计 PASS=7` 就藏在参数里）。那份文件哪天不再回显参数，
#     哨兵当场消失 ⇒ **②那一侧红**，是**响的**退化，不是静默的。这正是两侧都断的理由。
gate_assert_judged() {
  local name="$1" out="$2" sentinel="$3" cut="$4"
  case "$out" in
    *"  ok   "*)
      fails+=("gate $name（**「$cut」这一条判定不再判了** —— 喂一份只有它拦得住的合成输入，\
那一格却印出了绿行 ⇒ 一条判定被掏空、门禁照旧放行，正是 N-G2 立这一族探针要治的那一形）")
      return 0 ;;
  esac
  case "$out" in
    *"$sentinel"*) ;;
    *) fails+=("gate $name（断「$cut」的那条探针**自己空转了** —— 绿行没出现，哨兵 $sentinel \
也不在输出里 ⇒ 合成输入根本没走到该走的失败支，这一格判不了，按红记，不许当成绿）") ;;
  esac
}

gate_selftest() {
  local probe
  probe="$(run_gate 自检① - bash -c 'printf "error: KR22-PROBE-A\n"; exit 3' 2>&1)"
  case "$probe" in
    *KR22-PROBE-A*) ;;
    *) fails+=("gate 自检①（run_gate 的失败支没把被测命令的输出印出来 —— K-R22 那一格被改回去了：\
门禁红了又不说为什么红）") ;;
  esac
  probe="$(run_gate_sum 自检② 8 bash -c 'printf "error[E0425]: KR22-PROBE-B\n --> src/x.rs:1:1\n"; exit 101' 2>&1)"
  case "$probe" in
    *KR22-PROBE-B*) ;;
    *) fails+=("gate 自检②（run_gate_sum 的失败支在「编译错误」那一形上印不出东西 —— \
那一形没有 failures: 段，正是 K-R22 D2 点名要盖住的那格）") ;;
  esac
  probe="$(run_gate 自检③ - bash -c 'i=1; while [ $i -le 100 ]; do
      if [ $i -ge 97 ]; then printf "尾部第 %s 行 KR22-PROBE-C\n" "$i"; else printf "无关行 %s\n" "$i"; fi
      i=$((i + 1)); done; exit 4' 2>&1)"
  case "$probe" in
    *KR22-PROBE-C*) ;;
    *) fails+=("gate 自检③（模式表一条都没匹配上时，「原文尾部恒印」那半兜底没走到 —— \
fail-closed 的承重墙塌了：从此模式表漏掉的形状会退化成一个字都不印）") ;;
  esac
  # 探针④：失败行**行首带 ANSI 色码**（vitest 那一形），哨兵只在这一行上；
  # 垫的无关行条数现算，保证①之外的两条路（②尾部 / ③全印）**都够不着**它。
  probe="$(run_gate 自检④ - bash -c '
      printf "\033[41m\033[1m FAIL \033[22m\033[49m NG1-PROBE-D 行首带色的失败行\n"
      n=$(( $1 + $2 + 10 )); i=1
      while [ "$i" -le "$n" ]; do printf "无关行 %s\n" "$i"; i=$((i + 1)); done
      exit 5' _ "$GATE_DIAG_TAIL" "$GATE_DIAG_WHOLE" 2>&1)"
  case "$probe" in
    *NG1-PROBE-D*) ;;
    *) fails+=("gate 自检④（失败行行首带 ANSI 色码时，「关键行」那一段匹配不上 —— \
N-G1 治的正是这一形：vitest 在非 TTY 下照样上色，ESC 不是 [[:space:]]，\
于是每一趟红都退化成「一条都没匹配上」，死值验拿不到失败用例的名字）") ;;
  esac

  # ── 探针⑤–⑨（`N-G2`）：断「这一条判定还在判」。取法与承重前提见上面那段头注。────
  #
  # 探针⑤ · `run_gate` 的**退出码**那条。
  #   只有它拦得住：读数 `7 passed` ⇒ 「0 passed 不是绿」那条**已被满足**，掏掉退出码那条没人接。
  probe="$(run_gate 自检⑤ - bash -c 'printf "NG2-PROBE-E 7 passed\n"; exit 3' 2>&1)"
  gate_assert_judged 自检⑤ "$probe" NG2-PROBE-E "run_gate 的「退出码非零 ⇒ 红」"
  #
  # 探针⑥ · `run_gate` 的**「0 passed 不是绿」**那条。
  #   只有它拦得住：`exit 0` ⇒ 退出码那条**已被满足**，掏掉这条就一路落到绿行。
  probe="$(run_gate 自检⑥ - bash -c 'printf "NG2-PROBE-F 0 passed\n"; exit 0' 2>&1)"
  gate_assert_judged 自检⑥ "$probe" NG2-PROBE-F "run_gate 的「0 passed 不是绿」"
  #
  # 探针⑦ · `run_gate_sum` 的**退出码**那条。
  #   只有它拦得住：喂 1 行 `test result: ok.` 且 want_pkgs 就给 1 ⇒ 包数自检**已被满足**；
  #   合计 7 ≠ 0 ⇒ 「0 passed 不是绿」也**已被满足**。
  probe="$(run_gate_sum 自检⑦ 1 bash -c 'printf "test result: ok. 7 passed\nNG2-PROBE-G\n"; exit 3' 2>&1)"
  gate_assert_judged 自检⑦ "$probe" NG2-PROBE-G "run_gate_sum 的「退出码非零 ⇒ 红」"
  #
  # 探针⑧ · `run_gate_sum` 的**包数自检**（采集面：跑到的包数 ≠ 应有）。
  #   只有它拦得住：`exit 0` ⇒ 退出码那条**已被满足**；合计 7 ≠ 0 ⇒ 「0 passed」那条也**已被满足**；
  #   只喂 1 行 `test result:` 而 want_pkgs 给 2 ⇒ 差的正是包数这一条。
  probe="$(run_gate_sum 自检⑧ 2 bash -c 'printf "test result: ok. 7 passed\nNG2-PROBE-H\n"; exit 0' 2>&1)"
  gate_assert_judged 自检⑧ "$probe" NG2-PROBE-H "run_gate_sum 的「包数自检：跑到的包数 ≠ 应有」"
  #
  # 探针⑨ · `run_gate_sum` 的**「0 passed 不是绿」**那条。
  #   只有它拦得住：`exit 0` ＋ 包数 1 = want_pkgs 1 ⇒ 前两条**都已被满足**，合计恰好是 0。
  probe="$(run_gate_sum 自检⑨ 1 bash -c 'printf "test result: ok. 0 passed\nNG2-PROBE-I\n"; exit 0' 2>&1)"
  gate_assert_judged 自检⑨ "$probe" NG2-PROBE-I "run_gate_sum 的「0 passed 不是绿」"
}
gate_selftest

# 8 个包 = `monitor` + 7 个共享 crate（`vendor/code-picture-core` 已被上面那条 `--exclude` 排掉）。
run_gate_sum cargo 8 bash -c 'cd src-tauri && cargo test --workspace --exclude code-picture-core --lib 2>&1'

# ★★ `K-G3`（09-01）：上面那个合计**还缺一个分母** —— `src-tauri/embedded-daemons/` 铺没铺。
#
# `build.rs:376` 只有在 `src-tauri/embedded-daemons/` 里两个 arch 的二进制**都在且 build_id 对得上**
# 时才 `println!("cargo:rustc-cfg=embedded_daemons")`；那个目录被 `.gitignore` 挡着
# ⇒ **它跟着「铺没铺」走，不跟着 git 走**。挂 `#[cfg(embedded_daemons)]` 的那一族全是
# 「本地后端真的能起来吗」：`sftp::embedded_daemon_binaries_present_and_valid` ·
# `local_daemon::the_local_daemon_can_be_stopped_and_started_again` ·
# `local_backend::the_local_tmux_frames_really_land_in_the_ledger` ·
# `local_backend::the_local_daemon_really_registers_an_inbound_client`。
#
# 病灶逐字（`ROADMAP.md` 风险行 `5t`，PM 08-25 实测撞上、08-29 复打）：
# **「没有任何东西报出『这一跑少编了几条』」** —— 少编与「都跑了」在终端上一模一样，
# 因为那个合计只会**变小**，而变小没有任何东西认得出来。
#
# ⚠ **这一行只自报家门，不是判据**，理由是判不了：地板得是个常数，而同一份代码
#   铺了与没铺**本来就该是两个数**，钉死任何一个都会把另一种铺法误判成红。
#   真要买成判据得先有一张「铺法 ⇒ 应有条数」的映射，那张表今天盘上没有 ⇒ 交回 PM。
# ⚠ 行首刻意**不是** `ok` —— 它不判任何东西，写成 `ok` 就是把一条诊断伪装成一格绿。
if [ -d src-tauri/embedded-daemons ]; then
  printf '  分母 %-14s %s\n' "cargo" "本树铺了 src-tauri/embedded-daemons/ ⇒ embedded_daemons cfg 会置上，「本地后端真的能起来吗」那一族在跑"
else
  printf '  分母 %-14s %s\n' "cargo" "本树未铺 src-tauri/embedded-daemons/ ⇒ embedded_daemons cfg 不置 ⇒ 上面那个合计里少了「本地后端真的能起来吗」那一族（4 条，逐个点名见上方注释）"
fi

# ★ 生成物漂移（K-A1 第四轮 `R1`）：**改了 Rust 不跑生成，这里红。**
#
# 形状照 `.github/workflows/ci.yml` 那条「生成物必须最新（C05）」来 —— 它逐字是
# `git diff --exit-code -- ../src/generated/`（那一步在 `src-tauri` 目录下跑，所以带 `../`；
# 本脚本开头已经 `cd` 到仓根，所以不带），失败时印一句 `::error::` 提示「请跑
# npm run gen:types 并把 src/generated/ 一起提交」再 `git diff --stat`。
# ⚠ 那条 CI 步骤的头注还写明了它**排除了什么**：它只买「已提交的生成物 == 从 Rust 源生成的」
# 这一半，另一半「TS 消费方 == 已提交的生成物」由 frontend job 的 `tsc` 买 —— 拆成两半的理由是
# **没有任何 job 同时有 Rust 和 node**（给 Rust job 加 `npm ci` 是分钟级，给 frontend job 加
# 整套 Tauri 编译是 CI 里最贵的东西）。本脚本两样都有，所以这一半在这里只值一条 git 命令。
#
# ⚠⚠ **位置是承重的：它必须排在上面那道 `cargo` 门之后。**
# `ts-rs` 的导出测试就住 `cargo test --lib` 里（`package.json` 的 `gen:types` 逐字就是
# `cd src-tauri && cargo test --lib export_bindings`）⇒ 跑过那道门，`src/generated/**` 已经被
# 按当前 Rust 源重写了一遍，这里的 `git diff` 才是「Rust 源 与 已提交版本」的差。
# 排在它**之前** ⇒ 检查的是一棵还没被重写的树，**恒绿 = 假绿**。
#
# 立项理由（K-A1 D 阶段审计实测：往 `RemoteAccount` 加一个字段而**不**跑生成，四条读数）：
#   · vitest 全量（含 `generated-boundary-guard` 那一族）**1467 全绿**
#   · `npx tsc --noEmit` **0 错**
#   · `cargo test --lib` **自己把 `src/generated/RemoteAccount.ts` 重写了、然后报
#     `1181 passed; 0 failed`（绿）** ⇒ 本脚本原来那四道门**结构上一条都抓不到**
#   · 只有 CI 那条抓得到。
# 而本脚本头注自称「出货前的**唯一闸门**」—— 补上这一句才对得起那句话。
# ⚠ 顺带订正一句写在别处的假话：件计划 `KAY1③` 曾写「生成物一致性由
# `generated-boundary-guard` 那族 + `npm run gate` 守，改 Rust 不跑生成就红」——
# **在本行落地之前，后半个主语是假的**（订正记在件计划 `§1 KAY1③`）。
#
# ⚠ 射程如实写（它**抓不到**什么，三条）：
#   1. 它判**已跟踪文件的 diff** ⇒ 一个**全新**的生成物文件是 untracked，`git diff` 看不见。
#      那一格由 `src/generated-boundary-guard.vitest.ts` 的目录清单**逐项等号对拍**钉住
#      （它对 `src/generated/` 做 `readdirSync` + 等号比对，新增文件必然让它红一次）。
#   2. 它不判生成物**内容对不对**（该不该 `ts(optional)` 之类）—— 那也是上面那一族的活。
#   3. 它判的是**工作树**，不判「你有没有真把它 commit 上去」（那一维归 `npm run verify:committed`，
#      与本脚本头注里那条分工一致）。
git diff --quiet --exit-code -- src/generated/
gen_rc=$?
case "$gen_rc" in
  0) printf '  ok   %-14s %s\n' "generated" "与 Rust 源一致（跑过上面那道 cargo 门之后再判的）" ;;
  1)
    printf '  FAIL %-14s %s\n' "generated" "src/generated/ 与 Rust 源不一致："
    git diff --stat -- src/generated/
    fails+=("generated（改了带 ts_rs::TS 的类型 ⇒ 跑 npm run gen:types 并把 src/generated/ 一起提交）")
    ;;
  *)
    # 退出码既不是 0 也不是 1（如 128：不在 git 仓里）⇒ **判不了**。不许当成绿。
    fails+=("generated（git diff 退出码 $gen_rc —— 判不了，不许当成绿）")
    ;;
esac

run_gate daemon '单包 remote-daemon-proto，只有一行 test result ⇒ 最大值 = 合计' \
         bash -c 'cd remote-daemon-proto && cargo test 2>&1'
run_gate npm '17 个套件（16 tsx + 1 vitest）里只有 2 个打得出数字（test:dom 1480 · test:diff 17），而取最大值 ⇒ 这个数恒是 test:dom 的；另 15 个 tsx 套件只打「all X tests passed」，它们「跑了 0 个」这一格守不住（失败仍由 && 链的退出码守）' \
         npm test

# ── 门⑥ `ccm` e2e（`K-G3` 09-01，治 `丙1-f1`）────────────────────────────────
#
# ★★ 它买的是什么：`shared/ccm` 是 1258 行的 bash 启动器，`K-C1` 为它写了 54 条断言，
#    而在本行落地之前 `grep -c ccm scripts/gate.sh` = **0** ⇒ 出货那一刀**一条都不看**。
#    头注那句「出货前的**唯一闸门**」与这个 0 对不上，`丙1-f1` 就是这笔账。
#
# ★ **判法不自造，复用 `e2e/assert-pass-floor.sh`** —— CI 的 26 条 e2e 步骤用的就是它，
#   地板值也照抄 `ci.yml` 那两行（`ccm-print-parity 12` · `ccm-rbind-title 8`）。
#   一个性质两个量法就是本区最贵那族病（`K13`）；这里刻意只留一份。
#   它自己 fail-closed 的三条（头注逐字）：非零退出 ⇒ 红 · 抓不到「合计 PASS=」⇒ 红
#   （不当 0 也不当过）· 实得 < 地板 ⇒ 红。
#
# ⚠ **本函数在它之外再判一次「抓不抓得到那个数」**，不是重复：`assert-pass-floor.sh`
#   自己红时会 `exit 1`，而**它整个没跑起来**（脚本被删 / bash 起不来）时 `rc` 也是非零，
#   两者在 `fails` 里长得一样。多抓一次 `n` 是为了让绿行**带上实得数**——
#   门禁类判据的专属陷阱是「门没跑」与「门跑了结果是空」在终端上一模一样。
#
# ★★★ **`K-G8`（09-03）：本函数跑的这几套一律按 `exact` 判 —— 实得 ≠ 地板就红。**
#
#   在此之前判法是 `n < 地板`（`assert-pass-floor.sh:56` 逐字 `-lt`）⇒ **只挡缩水，
#   不挡「涨了而地板没跟」**。那一侧是**静默**的，09-03 当天就发生了一次活体：
#   `ccm-cli` 实得 **173** / 地板 **126** ⇒ 门禁印 `PASS=173（地板 126）`，**绿的**，
#   那 **47** 条断言在地板眼里等于不存在（当天可以被整族删掉，没有任何东西会说一句话）。
#   最后是下一拍的人**顺手**棘上去的 —— **不是任何判据逮到的。**
#   ⇒ ★ 余量的宽度**不是这套机制的属性**，是「上一次有人手动棘距今多久」的属性。
#
#   🔴 **为什么恒等能装在这里、不能装在 `ci.yml` 那 19 条上**（分母写死，别读宽）：
#     地板今天有 **23** 条调用行，本文件只跑其中 **4** 条；另外 **19** 条只住在 `ci.yml`，
#     而那条流水线 **751 个提交 / 29 天没通电**（`origin/main` = `1eeb4bf` @2026-08-05）。
#     给一条没人在跑的判据换判法 ⇒ **没有任何读数能验它**。那 19 条归 `K-G3`，本拍不动。
#     ⇒ **4/23。别把「这 4 套装上了恒等」读成「地板这件事解决了」。**
#
#   🔴 **摩擦落在谁头上，写清楚**：落在**加断言的那一拍**身上，而且落点是**三处** ——
#     `ci.yml` 调用行 + `ci.yml` 的 `pair` 自检清单 + 本文件这几行 `run_e2e`。
#     那三处**本来就该一起改**（`ci.yml` 那条纪律逐字写着），恒等买到的不是「少改一处」，
#     是把「忘了改」从**静默**变成**当场红**。⚠ 摩擦的量没变，变的是忘了会不会响。
#
#   ⚠ **前置买过了才装的**：恒等把「涨了也红」加进来之后，一套 PASS 数**随环境浮动**的
#     套件会**双向都红**。`K-G8` 落地拍现打：23 套 × **4 趟**（同镜像连打两趟 ·
#     换镜像 `kg3-devbox-full:probe` 一趟 · 换网络 `--network host` 且带并发负载一趟）
#     ⇒ **23/23 逐套 `(PASS, FAIL, rc)` 三元组全同值**，这 4 套在内。
#     ⚠ 那是「4 趟里没抓到不稳」，**不是「恒稳」**；CI runner / Windows 上**未知，不是稳**。
#     尺子留在 `evidence/K-G8-stability-diff.py`（可复算）。
#
#   ⚠ **`exact` 是第三个参数、fail-closed**：拼错 ⇒ `exit 2`，**不回落 `at-least`**。
#     回落等于把「拼错了」静默降级成旧行为 —— 那正是这道闸要治的那一族。
run_e2e() {
  local suite="$1"; local floor="$2"
  local out rc n
  out="$(bash e2e/assert-pass-floor.sh "$suite" "$floor" exact 2>&1)"
  rc=$?
  n="$(printf '%s' "$out" | grep -oE '合计 PASS=[0-9]+' | grep -oE '[0-9]+' | tail -1)"
  if [ "$rc" -ne 0 ]; then
    fails+=("ccm e2e/$suite（退出码 $rc；实得 PASS=${n:-<抓不到>}，地板 $floor，判法 exact。\
诊断原文见上方本套件自己的输出）")
    # K-R22：原来这里是裸 `tail -20`。换成共用的 `gate_diag` 有两处不是排版：
    #   ① 套件自己的 `::error::` 诊断行**可能落在尾部 20 行之外**（`assert-pass-floor.sh`
    #      先 `cat` 整份输出再打诊断，一套失败几十条时那行就被顶出去了）—— 模式表把它捞回来；
    #   ② 加了 `  | ` 前缀，套件输出里的 `PASS=`/`FAIL=` 行不再混进本脚本的裁决面。
    gate_diag "e2e/$suite" "$out"
  elif [ -z "$n" ]; then
    fails+=("ccm e2e/$suite（退出码 0 但抓不到「合计 PASS=<n>」—— 门没跑与门跑了结果是空\
在终端上一模一样，判不了，不许当成绿）")
    # K-R22 D1 #9：这一支原来一个字都不印，而「门没跑」与「门跑了结果是空」
    # **恰恰只能靠那段输出分开** —— 不印等于把这条判据自己那句话作废。
    gate_diag "e2e/$suite" "$out"
  else
    printf '  ok   %-14s %-22s PASS=%s（地板 %s，恒等）\n' "ccm e2e" "$suite" "$n" "$floor"
  fi
}

# ── 探针⑩（`N-G2`）：`run_e2e` 的**退出码**那条判定还在不在判 ──────────────────
#
# 🔴 **为什么它不住在上面那个 `gate_selftest` 里**：bash 要函数**先定义后调用**，而
#   `gate_selftest` 的调用点在本文件靠前处、`run_e2e` 到这里才定义。两条别的路都更贵 ——
#   把 `run_e2e` 的定义整块前移、或把 `gate_selftest` 的调用点后移，都是为了一条探针去动
#   本文件的结构。⇒ **让这一条住在它盯的那道门旁边**，`gate_assert_judged` 与取法仍是同一份。
#
# 只有它拦得住：给一个**非数字的地板** ⇒ `e2e/assert-pass-floor.sh` 在**参数校验**那一步
#   `exit 2`，并把那个参数**原样回显**；参数里带着 `合计 PASS=7` ⇒ `run_e2e` 抓得到读数
#   ⇒ 「抓不到「合计 PASS=」」那条**已被满足**，掏掉退出码那条就一路落到绿行。
# ⚠ 那一步排在 `npm run` 与 `mktemp` **之前** ⇒ 不跑 npm、不落文件（见上面自检段头注）。
gate_selftest_e2e() {
  local probe
  probe="$(run_e2e 自检⑩ '合计 PASS=7 NG2-PROBE-J' 2>&1)"
  gate_assert_judged 自检⑩ "$probe" NG2-PROBE-J "run_e2e 的「退出码非零 ⇒ 红」"
}
gate_selftest_e2e

run_e2e ccm-print-parity 12
run_e2e ccm-rbind-title  8
# ── `K-G7`（09-03）新挂的两套 ─────────────────────────────────────────────────
# 地板量于 `b8a6ecd`、镜像 `644ea0ce5c3d`，连打两趟同值（126 / 68，各 0 FAIL 0 SKIP）；
# 与 `ci.yml` 的调用行同值 —— 详见本文件头注那一节，改地板要三处一起改。
# ⚠ 这两套硬依赖 `jq` 且 fail-closed：镜像里没有 `jq` ⇒ 这两格红，不会静默跳过。
# ★★ **`K-P2` `D` 阶段（09-03）：`ccm-cli` 棘 `126` → `203`**（`ccm-contract-parity` 本拍没动）。
#   那 77 条分两拍买的：`D1` 的 `JSONENC` 47 条（真 JSON 编码器，与 `jq -Rs .` 逐字节对拍）
#   ＋ `D2` 的 `WIRE` 30 条（真发请求那一半：落盘式假 daemon，「发了」与「发对了」分两族判）。
#   量于 `65b2792`、同一份 devbox 镜像，`PASS=203 FAIL=0`。
#   🔴 **顺带如实登记这一格是怎么被逮到的**：`D1` 交回时实得 **173**、地板还停在 **126**，
#   而 `e2e/assert-pass-floor.sh:56` 是 `n -lt FLOOR` ⇒ **只挡缩水、不挡「涨了不跟」**
#   ⇒ 那 47 条在地板眼里等于不存在，**没有任何东西会说一句话**（「静默不涨」）。
#   那个**机制**（地板该不该恒等 / 自动棘轮）归 `K-G8`；这里只是把这一次的数棘上去。
# ★★ **`K-P2` `D` 阶段第三拍（09-03，生产接线）：`ccm-cli` 再棘 `203` → `242`**。
#   那 39 条是 `WIRE/launch` 一节：**`D2` 那 30 条盖的是 `--resolve` 那一跳，一条都没盖住
#   `launch`**。棘之前那一趟是**切完 8 刀、补完判据之后**的那一趟（`§19 裁八`），
#   沙箱 `ccmon-devbox:latest` 现打 `PASS=242 FAIL=0`。
#   ⚠ 那 8 刀按 242 这个基线**重打过一遍**（先前按 240 打的那一轮作废）。
# ★★ **`K-P2` `F` 拍（09-04，真搬拍）：`ccm-cli` 棘 `242` → `261` · `ccm-contract-parity` 棘 `68` → `72`。**
#   用@09-04 逐字「**ccm不要管找不到, 统一走后端**」⇒ `shared/ccm` 的**建会话**与**账号解析**
#   两条本地退路都删了。那不是「删几条判据」——盘上是**整族翻面 ＋ 净增**：
#     · `ccm-cli` +19：`WIRE/launch` 的「降级」族翻成「不可达」族（＋`腿分开` 3 条：
#       用一份「账号答得上、只有 `--launch` 答不出」的后端把两条腿分开）· `KCY2` 整族翻面
#       （`rc=4` / 「`CCM_NO_DAEMON=1` 不是逃生口」/「空表是合法答案」各成一格）·
#       身份前置检查那族补了「不给 `--base` ⇒ 账号那条腿先报」的反向对照 ·
#       `WIRE/夹具自检④` 拆成「没后端 ⇒ 没有兜底」＋「后端在但答不出 `--resolve` ⇒ 静默退路仍在」。
#     · `ccm-contract-parity` +4：`A″`/`A′e`/`A′h` 三处的反向对照从「关掉后端」换成
#       「换一份后端」（同一条代码路径、只有输入不同 —— provenance 正是后者），
#       并各补一格「`CCM_NO_DAEMON=1` ⇒ rc=4」。
#     · `ccm-cli` 再 +3：`控制字符` 那一族拆成两半 —— 「换行照发」（修回来的那个能力：
#       载荷里的 `\n`/`\t` 是**键**，`create-or-attach` 放行；`ESC`/`CR`/`NUL` 照旧挡）
#       ＋「线上那份请求里那个换行是**转义**过去的」。
#   量于 `ccmon-devbox:latest`，`PASS=264 FAIL=0` / `PASS=72 FAIL=0`。
#   ⚠ **`ci.yml` 那两行不在本件写区** ⇒ 逐字 diff 交回 PM 落（头注那条「三处一起改」的纪律照旧）。
run_e2e ccm-cli               264
run_e2e ccm-contract-parity   72

# pb check 不打「passed」，单独判：它自己会打 `FAIL=<n> BROKEN=<n>`。
#
# ★★ `K-R10`（09-01）：**查哪个计划工作区，由调用方用环境变量 `PB_WS` 给** ——
#    在此之前这里硬写着 `../.claude/planned-build/control-parity`。
#
# 它骗过的人是现打的：`K-R7` 收官前后，**实现方与 PM 各被它误导过一次** ——
# 门禁打绿的那行说的是 `control-parity`，而当时在做的是 `backend-consolidation`。
# ⇒ 这是本仓最高频的那族病（**量具的作用域对不上事实**）长在门禁自己身上。
#
# ⚠ **为什么是「调用方给」而不是「从工作树推导」** —— 推导那条路 09-01 摸底否掉了，
#   理由不是它难写，是**它的失效面形状和今天这个 bug 一模一样但更隐蔽**：
#   推错时它会印出一个**看起来完全合理的工作区名**，而硬写的常量至少肉眼可查。
#   拿一个更难发现的同族 bug 去换一个已经被发现的，不划算。
#  （盘上也没有权威映射：`.dispatch.json` 只有 2/54 条能点出工作树，
#    而且那两条用了两个不同的 key 名、两种路径写法。）
#
# ⚠ **为什么是环境变量而不是加一个位置参**：`.claude/devbox/gate` 的 arg2 已经是 TAG，
#   再加 arg3 会让「两参写法」被静默吃成 TAG —— 又一个静默失效面。
#
# ★★ **fail-closed：不给 `PB_WS` 就红，不回落任何默认值。**
#   这条是承重的，别改成 `${PB_WS:-control-parity}` 之类「友好」的写法：
#   回落默认 = 把今天这个 bug 原样搬进 `:-` 右边，而且从此**连硬写的常量都看不见了**。
#   ⚠ 直接跑 `npm run gate`（不经沙箱）会因此红 —— **那是设计**：红线本来就写着
#   门禁一律走沙箱，那条路本就不该是绿的，让它红是把纪律变成闸。
#
# ★★ **绿的那一行必须自报家门（逐字带上 `PB_WS`）**，`fails` 那一支同样带。
#   理由逐字：**这个 bug 骗过两个人靠的不是数字错，是那行字里没有任何能让人发现
#   它在说别人的信息。** 一行不带名字的 `ok pb check FAIL=0`，不论 FAIL 是几都不算数。
#
# ⚠ 名字**给错**那一侧不用在这里再判：`pb.py` 今天就已经 fail-closed
#   （不存在的目录 rc=3 · 没 `features/` rc=2 · 空目录 rc=2，三种都试过）⇒
#   在这里补一层「目录存不存在」是仪式。本处只治**同一性**（查的是不是你那个），不治存在性。
if [ -z "${PB_WS:-}" ]; then
  fails+=("pb check（没给 PB_WS —— 这道门查哪个计划工作区必须由调用方指定；\
不许回落默认值：硬写一个名字正是 K-R10 治的那个 bug）")
else
  # ★ `K-R22`（09-04）：**判定那一行一个字没动，改的只是「红了印什么」。**
  #   原来是 `pb_out="$(python3 … | tail -1)"` ⇒ 其余几十行在**进变量之前**就没了，
  #   红的时候终端上只剩一句 `FAIL=1 BROKEN=0`：**几条红一目了然，是哪一条一个字都没有。**
  #   ⚠ 本件收官前自己被它咬了一口：那个 `FAIL=1` 查出来是 `[J3 陈账] INDEX.md 比源文件旧`，
  #     而那一趟门禁**没有任何一个字**指得到它 —— 又赔进去一趟重跑。
  #
  # ⚠ **为什么是落文件，而不是 `printf '%s\n' "$(…)" | tail -1`** —— 这一步承重，别「简化」：
  #   `$(cmd | tail -1)` 与 `$(tail -1 文件)` 对命令替换而言**逐字等价**（两者都是对 `tail`
  #   的输出做替换）；而 `printf '%s\n' "$(cmd)" | tail -1` **不等价** ——
  #   命令替换会先把结尾的空行吃掉。合成命令现打（输出 `a\nb\n\n\n`）：
  #   `$(cmd|tail -1)` 得 **``**（空），`printf '%s\n' "$(cmd)"|tail -1` 得 **`b`**。
  #   ⇒ 那一格差别能**把一条本该红的判据变绿**（空串匹配不上 `FAIL=0 BROKEN=0` ⇒ 红；
  #     换成 `b` 就可能匹配上 ⇒ 绿）。所以这里走文件，不走那个写法。
  pb_raw="$(mktemp)"
  python3 "$HOME/.claude-accts/z/skills/planned-build/bin/pb.py" check \
          "../.claude/planned-build/$PB_WS" >"$pb_raw" 2>&1
  pb_out="$(tail -1 "$pb_raw")"
  case "$pb_out" in
    *"FAIL=0 BROKEN=0"*) printf '  ok   %-14s [%s] %s\n' "pb check" "$PB_WS" "$pb_out" ;;
    *) fails+=("pb check[$PB_WS]（$pb_out）")
       gate_diag "pb check[$PB_WS]" "$(cat "$pb_raw")" ;;
  esac
  rm -f -- "$pb_raw"
fi

echo
if [ "${#fails[@]}" -eq 0 ]; then
  echo "GATE: OK —— 三道门 + 生成物漂移 + pb check + 四套 ccm e2e 全绿，可以出货"
  exit 0
fi
# ★ `K-G3`（09-01）：分隔符**不能**走 `IFS='；'` —— `IFS` 是按**字节**认的，
#   而 `；`（U+FF1B）是 3 个字节，`${fails[*]}` 只会拿它的**第一个字节**去拼
#   ⇒ 两格以上一起红时，裁决行里印出来的是一个坏字节（`�`），后面那几格的名字被它糊住。
#   现打：本拍的死值验 `C1` / `C2` 两刀各撞到一次（两格同红）。一格红时看不出来 ——
#   这正是「只在多失败那一支才发作」的形状，而没人会为了看分隔符去造两格同红。
#   ⇒ 自己拼，不借 `IFS`。
joined=""
for f in "${fails[@]}"; do joined="${joined:+$joined；}$f"; done
printf 'GATE: FAIL —— %s\n' "$joined"
echo "**别提交**。先修，再重跑本脚本。"
exit 1
