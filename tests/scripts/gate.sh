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
# ⇒ 本脚本把三道门〔量于 08-13·当时就那三道门；今天跑几格看下面那段自述节，别拿这一句当读数〕
#   （+ 下面那道 `generated` 生成物漂移检查，K-A1 第四轮补的）收成一条命令，
# 并且**只在最后打一行裁决**（`GATE: OK` / `GATE: FAIL …`）。
# 用法就一句纪律：**先跑它、看见 `GATE: OK`，再单独敲 `git commit`。**
# ⚠ 它**故意不提交任何东西**、也不接 `--commit` 之类的开关 —— 那会把刚拆开的两件事又焊回去。
#
# ┌─ 〔自述·射程〕本脚本此刻跑哪几格 ─────────────────────────────────────────────
# │
# │ 🔴 **这一段由 `tests/evidence/K-R80-gate-cell-coverage.py` 的 `C5b` 逐字对拍**（`K-R91` 09-12）：
# │   格数三方对拍（这一段的数 · 下面裁决行的数 · 现打的判定格数）＋ 逐格点名**集合相等**
# │   ＋ **这一段里点名的每一条住址现打得存在**。
# │
# │ 🔴 **上一版这里逐字写着「三道门 + 一道生成物漂移检查 + `pb check` + 四套 `ccm` e2e」**
# │   ＝ 3+1+1+4 = **9**，而那天盘上已经是 **13** 格。**那个算式与 `R42` 裁定零里被 PM
# │   传播了 46 次的错数是同一个算式、同一份文件。** `C5` 只对拍下面那行裁决，**钉不到这里**
# │   —— ⚠ **腐的时长如实写**：这句话在 09-10 加 `fmt`/`winchk` 之前是**对的**（当时就 9 格），
# │   09-10 起变成 9 vs 11，09-12 `K-R80`/`K-R82` 之后是 9 vs 13。裁决行那一句 09-12 有人订正，
# │   **头注这一句没有**（没有判据钉着它）—— 这才是本件的题面，不是「错了很久」。
# │
# │ ⚠ **自述句只许住在这一段里。** `C5b` 会把这一段之外、头注里任何一句「本脚本跑 N 格 /
# │   N 道门」判红；历史读数的唯一豁免是**在那一行**逐字带上 `〔量于 …〕`。
# │
# │ 〔自述·格数〕36 格
# │ 〔自述·点名〕worktree-clean · hooks · copy2 · shellcheck · ci-e2e-prereq · release-gate · gate-selfdesc ·
# │   ccbus-twophase ·
# │   platform · muslbuild ·
# │   installface ·
# │   fmt · fmt-backend ·
# │   winchk · winchk-backend · winlink · cargo · comm-boundary · test-tiers · deadcode · generated · backend · panorama-engine · tsc · npm ·
# │   ccm tests/e2e/ccm-print-parity · ccm tests/e2e/ccm-rbind-title · ccm tests/e2e/ccm-cli ·
# │   ccm tests/e2e/ccm-contract-parity ·
# │   ccm tests/e2e/backend-rbind-token · ccm tests/e2e/rbind-token-endtoend · ccm tests/e2e/backend-cc-bus ·
# │   ccm tests/e2e/backend-gate2 · ccm tests/e2e/local-backend · ccm tests/e2e/restart-frames · ccm tests/e2e/restart
# │ 〔自述·现物〕十一套 e2e 的被测文件：`tests/e2e/ccm-print-parity.sh` · `tests/e2e/ccm-rbind-title.sh` ·
# │   `tests/e2e/ccm-cli.test.sh` · `tests/e2e/ccm-contract-parity.sh` ·
# │   `tests/e2e/backend-rbind-token.sh` · `tests/e2e/rbind-token-endtoend.sh` · `tests/e2e/backend-cc-bus.sh` ·
# │   `tests/e2e/backend-gate2-acceptance.sh` · `tests/e2e/local-backend-supervise.sh` ·
# │   `tests/e2e/restart-backend-frames.sh` · `tests/e2e/restart-suite.sh`；判法一律走 `tests/e2e/assert-pass-floor.sh`。
# │ 〔自述·现物〕`copy2` 那一格的判据本体：`tests/evidence/K-R115-ruler.py`（`K-R115` 09-14 第 14 格）。
# │ 〔自述·现物〕`shellcheck` 那一格没有独立的判据文件 —— 它的**人群与地板都从
# │   `.github/workflows/ci.yml` 现读**（那一段 `FILES=` ＋ 它下面那条覆盖面地板行），
# │   判定逐字写在下面那个 `gate_shellcheck` 函数里（`K-R122` 09-14 第 17 格）。
# │ 〔自述·现物〕`ci-e2e-prereq` 那一格的判据本体：`tests/evidence/K-R122-ruler.py`（`K-R122` 09-14 第 18 格）。
# │ 〔自述·现物〕`release-gate` 那一格的判据本体：`tests/evidence/K-R124-ruler.py`（`K-R124` 09-15 第 20 格）
# │   —— 它与 `.github/workflows/ci.yml` 里那一步跑的是**同一份文件**，不是两份抄件；
# │   被测对象是 `.github/workflows/release.yml`，它顺带调 `scripts/release-notes.mjs --check`。
# │ 〔自述·现物〕`worktree-clean` 那一格的判据本体：`tests/evidence/K-W25-worktree-clean.py`
# │   （09-19 第 25 格）—— **前置条件**，排在所有格之前。
# │ 〔自述·现物〕`platform` 那一格的判据本体：`tests/evidence/K-G4-platform-ledger.py`
# │   （`G4` 09-19 第 24 格）—— 承诺的平台（条 63：三格）↔ 门禁真跑的格，两向对拍。
# │ 〔自述·现物〕`muslbuild` 那一格没有独立的判据文件 —— 它就是两趟 `cargo zigbuild`
# │   （`x86_64-unknown-linux-musl` ＋ `aarch64-unknown-linux-musl`），判定逐字写在
# │   下面那一行 `run_gate muslbuild` 的内联脚本里（`G4` 09-19 第 23 格）。
# │ 〔自述·现物〕`ccbus-twophase` 那一格的判据本体：`tests/evidence/W24C-ccbus-twophase-ruler.py`
# │   （`w24c` 09-19 第 24 格）—— 它既读盘上那几份 shell，也在一次性 `CC_BUS_HOME` 里真跑；
# │   死值验住 `tests/evidence/W24C-deathvalue.md`。
# │ 〔自述·现物〕`comm-boundary` 那一格没有独立的判据文件 —— 被测对象就是
# │   `tests/bridge/comm_boundary_registry_tests.rs`（通信层那一族，`13b` 步 1 09-20 第 27 格；C1–C5 ＋ X1–X6
# │   ＋ 锚 ＋ 余下五份 ＋ 传输面四份 ＋ 元判据，17 条），判定（三方对拍 ＋ 元判据/「说得出今天是空的」两条逐字锚点）
# │   逐字写在下面那一行 `run_gate comm-boundary` 的内联脚本里。
# │   🔴 本格买的是「这一族还在不在」。⚠ **这里刻意不写人群有几份** —— 那个数
# │   有唯一住址（该族自己那条三方相等 ＋ 模块头注那句，两处都是被判的），
# │   散文里再抄一份必腐：它 09-20 立表时是 0 份，09-21 步 3 之后是非空，而这 6 处
# │   自述**一条都不会红**（`gate-selfdesc` 的锚只钉裁词前缀）。
# │ 〔自述·现物〕`test-tiers` 那一格没有独立的判据文件 —— 被测对象就是
# │   `tests/bridge/crates/guard-core/test_tiers_tests.rs`（测试层分级：分区 ＋ 五层各一条反空真自检 ＋
# │   `tests/benches/` 登记 ＋ 合成夹具自检，TQ1 09-24 第 29 格），判定（三方对拍 ＋ 两条逐字锚点）逐字写在
# │   下面那一行 `run_gate test-tiers` 的内联脚本里。条数的唯一住址是那一行的 `pin`。
# │ 〔自述·退役〕〔第四波 S4〕第 26 格 `f3-copy`（秤 F3 两向）连同它量的那条零流量复制一起退役：
# │   窗口的复制早已走后端 `files-copy`（F7a），池子里只剩传输 ⇒ 29 格 → 28 格。
# │ 〔自述·现物〕`gate-selfdesc` 那一格的判据本体：`tests/evidence/K-R80-gate-cell-coverage.py`
# │   （09-19 第 22 格）—— **被测对象就是本文件**。它默认读 `tests/scripts/gate.sh`，
# │   也接一个路径参数（对着变异过的副本跑死值验时用）。
# │ 〔自述·现物〕`installface` 那一格的判据本体：`tests/evidence/K-R117-ruler.py`（`K-R128` 09-15
# │   第 21 格）—— 它同时是 `K-R117` 第一拍摸底的那把尺子，本件只往它上面加了 `R8`/`R9`/`R10`
# │   三条判定（`R1`–`R7` 一个字节没动）。死值验 16 刀住 `tests/evidence/K-R128-deathvalue.md`。
# │ 〔自述·现物〕`winchk-backend` 那一格没有独立的判据文件 —— 它就是一趟
# │   `cargo check --all-targets`，target 是 `x86_64-pc-windows-gnu`，跑在
# │   `src/backend` 那个 workspace 上（`K-R122` 09-14 第 18 格）。
# │ 〔自述·现物〕`winlink` 那一格没有独立的判据文件 —— 它就是一趟 `cargo build --bins`，
# │   target 是 `x86_64-pc-windows-gnu`，跑在 `src/bridge` 那个 workspace 的 `-p monitor` 上（WIN1 第四波 4D 第 30 格）。
# │ 〔自述·现物〕`deadcode` 那一格没有独立的判据文件 —— 它就是一趟 `cargo check -p monitor`
# │   加一个递减棘轮，判定逐字写在下面那一行 `run_gate deadcode` 的内联脚本里（第 15 格）。
# │ 〔自述·现物〕`tsc` 那一格没有独立的判据文件 —— 它就是一趟 `tsc --noEmit`
# │   （发版那条 `npm run build` 的**第一步**）加一条「程序面没被掏空」的对账，
# │   判定逐字写在下面那一行 `run_gate tsc` 的内联脚本里（`K-R118` 09-14 第 16 格）；
# │   程序面由 `tsconfig.json` 的 include 决定。
# │ 〔自述·不在射程〕跨平台 / 提交状态那一维归 `npm run verify:committed`（`C16`，动后端时跑）。
# │
# └─ 〔自述·射程〕完 ────────────────────────────────────────────────────────────
#
# 🔴 **〔量于 09-12·`K-R91`〕下面凡是点名 `ccm-acceptance` / `ccm-pretrust` 的段落，一律是
#   量于 09-01 / 09-03 / 09-04 的历史账 —— 那两套 09-11 `K-R48` 第二拍随 `shared/ccm`
#   一起删了，`tests/e2e/` 下现打双 `No such file`。**
#   旧读数**刻意不删**（它们记着「当初为什么只挂两套」「`jq` 那块拦路石怎么解开的」），
#   但每一段的**首行**都补上了 `〔量于 …〕` —— **自述与历史读数在散文里长得一模一样，
#   标一个，`C5b` 才分得开。** ⚠ 那个标记买不到「标着历史、内容却是今天的自述」那一形
#   （要读语义）；它买的是**没标的新句子当场红**。
#
# ★★ `K-G3`（09-01）〔量于 09-03〕：**「真机 e2e 本脚本不跑它们」这句话已经作废，
#    但只作废了 2/6。** ★ **`K-G7`（09-03）：作废到 4/6。**
#    剩下没作废的那 2/6 是 `ccm-acceptance` / `ccm-pretrust` —— 拦它们的**不是** `jq`
#    （那个本拍解开了），是沙箱里各红 1 条，另一笔账，见下面那一节。
#
# 上一版这一行逐字写着「真机 e2e 归各自的套件（本脚本不跑它们 —— 它们要 tmux/Xvfb，
# 几分钟起步）」。`丙1-f1` 逮到的正是这句话与那句「出货前的**唯一闸门**」对不上：
# `grep -c ccm scripts/gate.sh` = **0**（PM 08-24 独立复核，K-G3 09-01 在 `b28464e` 上复打，仍是 0）。
#
# **「几分钟起步」这个理由现打是假的**〔量于 09-01〕（沙箱 `ccmon-devbox:latest` 里逐套计时）：
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
# ⚠ **为什么当初只挂两套，另外四套的确切拦路石**〔量于 09-01〕（如实写，别读成「它们太慢」）：
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
#     `tests/e2e/` 一个字节没动），沙箱 `644ea0ce5c3d`，连打**两趟**、两趟同值：
#       `ccm-cli` **126 PASS / 0 FAIL**（6.78 秒） ·
#       `ccm-contract-parity` **68 PASS / 0 FAIL**（5.58 秒） —— 两套 **0 条 SKIP**。
#     ⇒ 重打的结果与 09-01 那两个数**相等**，但**那是重打出来的相等，不是沿用**。
#     合计 **+12.36 秒**（同一趟现打，不是从 09-01 的 12.8 抄的）。
#
#   · **这两个地板与 `.github/workflows/ci.yml` 的调用行同值**，刻意的：
#     `ci.yml` 里 `assert-pass-floor.sh ccm-cli <地板>` · `… ccm-contract-parity <地板>` 那两行。
#     ⚠ **09-03 `K-P2 D2` 把这里的行号拆掉了，那不是洁癖**：原文写的是 `ci.yml:559` / `:574`，
#       而本拍在 `ci.yml` 那两行**上方**加了一段棘轮注释 ⇒ 两个行号当场双双失真
#       （**559 → 575** · **574 → 590**）。**按名字指，不写行号**（`tests/e2e/ccm-cli.test.sh` 头注同一条纪律）。
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
#   · **仍然不挂 `ccm-acceptance` / `ccm-pretrust`**〔量于 09-03；那两套 09-11 `K-R48` 已删，
#     这一句连同它的处置一起作废，见头注 `K-R91` 那一段〕—— `jq` 只解开了四套里的两套，
#     那两套在沙箱里各红 1 条（上一段那笔账），挂了就是两条恒红。
#
#   ⚠⚠ **`K-P2 F` 拍（09-04）订正上面那句的一半，并报一条它买不到的东西。**〔量于 09-04〕
#     现打（沙箱 `ccmon-devbox:latest`，工作树 `k-p2f` @ `d305ffa` 的 `shared/ccm`，
#     量具 `tests/evidence/K-P2-F-suites.py`）：
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
#   ★★ **`F` 拍后半的收尾读数（同一趟量具，改完之后重打）**〔量于 09-04〕：那 71 条**全部回绿**，
#     而且六套里四套还涨了：`ccm-cli` 242→**264** · `ccm-contract-parity` 68→**72** ·
#     `ccm-acceptance` 29→**31** · `ccm-pretrust` 14/1→**15/0** ·
#     `ccm-print-parity` 12（=）· `ccm-rbind-title` 8（=）。
#     `cc-spawn-uplift` 67/5→**71/1**：那 1 条是**沙箱的既有红**（非 ASCII 目录名在容器里
#     被搞成 `__ ____ ______`，locale 的事），**与 `shared/ccm` 无关** ——
#     现打对照：同一份新夹具喂**旧** `shared/ccm` 也是 **71/1**，逐字相同。
#     买到这些的是一份**可复用的假后端** `tests/e2e/fake-backend.sh`（六套共用一份，不是六份各写一遍）。
#
#   🔴 **给 `K-G` 的建议（只写建议，本件不挂）**〔量于 09-04；`ccm-acceptance` 09-11 已删，
#      这条建议随之作废 —— 留着是因为它记着「不挂的理由当时只剩没人重量过」〕：
#      把 `ccm-acceptance` 挂进本门。
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
#      `cargo check --target x86_64-pc-windows-msvc` **rc=101**，而**本脚本六格全绿〔量于 09-01·
#      当时六格〕、印 `GATE: OK`**。
#      买法是一行 `cargo check -p creds-core --features harden --target x86_64-pc-windows-msvc`
#      （冷 5.67s / 热 0.15s），**卡在沙箱镜像没装那个 target**（`rustup target list --installed`
#      只有 `x86_64-unknown-linux-gnu`）⇒ 归 PM。
#      ⚠ 诚实边界：那一行买的是「**编得过**」，**买不到「行为对」** —— 行为要真 Windows 机器，
#      那一格今天是**判不了**，不是「通过」。
#
#   ② **格式漂移** —— ✅ **09-10 已买**，那一道门就在下面 `run_gate fmt` 那一行。
#      〔本条留着不删，因为它记着为什么当初没有：**卡在沙箱镜像没装 `rustfmt` 组件**
#      （`cargo fmt --version` 报 `'cargo-fmt' is not installed`）。
#      09-10 给 `.claude/devbox/Dockerfile` 加了一层 `rustup component add rustfmt`，
#      前提消失，门当天补上。历史读数：`b28464e` 上是 rc=1 / 78 处 / 18 个文件 / 1.09 秒。〕
#
#   ① **Windows 那半编不编得过** —— ✅ **09-10 下午也买到了**，门在下面 `run_gate winchk` 那一行。
#      〔本条同样留着不删，它记着两件事：**当初为什么没有**（沙箱镜像没装那个 target），
#      以及**原本提议的买法射程错了** —— 那条 `-p creds-core …` 只盖 86 处 `cfg(windows)` 里的
#      **2 处（2.3%）**，而 09-09 那 17 个编译错**全在 monitor 本体那 67 处里**。
#      换成 `-p monitor --target x86_64-pc-windows-gnu`（`-msvc` 扩不到本体：C 依赖要 `lib.exe`）。〕
#
#   ★ 原文那句「在它改之前，把这两维写成一道门 = 把 55 棵树的门禁一起打红」**仍然对**：
#     ①②两条能加，都是因为**前提被先改掉了**（`.claude/devbox/Dockerfile` 先装了
#     `rustfmt` / `mingw-w64` + 那个 target），不是因为那句话过时了。
set -uo pipefail

# 🔴 **仓根 = 本脚本的上两级**（`tests/scripts/gate.sh` ⇒ `../..`）。
# 〔2026-09-18 修〕原文是 `/..`，那是脚本还住顶层时的写法；09-17 重组把它搬进
# `tests/scripts/` 之后少了一级 ⇒ 它 `cd` 到的是 `<repo>/tests`，于是 `src/backend`、
# `tests/e2e/*.sh`、`node_modules/.bin` 全部解错 ⇒ **十几格一起红，而且红的形状是
# 「找不到文件 / 退出码 127」，看起来像代码坏了**。同族的 45 个脚本在 `78bcb195`
# 那一笔里修过了，**唯独漏了门禁自己** —— 一个「检查别人的东西」自己没被检查。
cd "$(dirname "$0")/../.." || exit 2
fails=()

# ── 〔被谁调用〕`GATE_ONLY` 子集 ＋ 一张**跑过的收据**（`G4` 空洞③，09-20）──────────
#
# ## 题面：这道门此前**只有人手动跑**
#
# 现打（本拍复打，读数与 `真相源/92 §2.1.2` ①那条一致）：
#   `grep -c 'bash tests/scripts/gate.sh' .github/workflows/ci.yml` ⇒ 落地前是 **0**；
#   `.git/hooks/` 下零个非 sample 钩子；`tests/hooks/` 下当时只有一份 `pre-commit`，
#   而那一份**默认是死的**（要人手 `git config core.hooksPath` 才活）。
# ⇒ **此前没有任何东西强制它在出货前跑过。** 而「跑了」与「没跑」在终端上**一模一样**
#   —— 两边都是**什么都没有**。这正是本仓反复治的那一形，长在门禁自己身上。
#
# ## 🔴 反空真：「装了钩子」**不等于**「它跑过」
#
# 一个 hook 文件躺在盘上、一个 job 写在 `ci.yml` 里，这两件事**一个字都没说那一趟真跑了**。
# 同一族的另一半本仓已经现打过读数（`tests/scripts/hooks-are-runnable.sh` 头注，git 2.43.0）：
# 没有可执行位时 git **忽略这个 hook 并照常提交**，`rc=0`，只留一句**可以关掉**的 advice hint。
# ⇒ 光看退出码、光看盘上有没有文件，**分不开「跑了」与「跳过了」**。
#
# ★ 取法：**这一趟自己开一张收据**（`GATE_RECEIPT`，默认 `.build/gate-receipt.json`，
#   那个目录 `.gitignore` 已经整棵忽略 ⇒ 不脏工作树、也不用动写区外的 `.gitignore`）。
#   收据里记的都是**只有真跑过这一趟才拿得到**的东西：
#     ① 这份 `gate.sh` 的 sha256 —— 换了版本开的收据不算这一份门禁开的；
#     ② 这棵树的 tree oid ＋ 工作树脏不脏 —— 别的树上那一趟不算这一趟；
#     ③ 这一趟**真的判过**哪几格（`GATE_RAN`，**现算**，不是抄的清单）；
#     ④ 被 `GATE_ONLY` 挡掉哪几格（`GATE_SKIPPED`，**现算**）。
#   判它的判据本体是 `tests/evidence/K-G4C-gate-receipt.py` —— 它**刻意不在本脚本里跑**：
#   本脚本跑得了它就说明本脚本跑了，那是同源恒真。它是给**调用方**（`ci.yml` 那个 job /
#   `tests/hooks/pre-push`）在门禁**之后**跑的那一条：门禁跳过了 ⇒ 没有收据 / 收据陈了 ⇒ 当场红。
#   🔴 **那条判据的反空真锚是「`ran ∪ skipped` 与本文件现打的格名两向集合相等」** ——
#     收据写空了、或本文件被读成空串，两个集合当场分叉。单向包含在表被清空时恒真。
#
# ## `GATE_ONLY`：**子集是明着少跑，不是静默少跑**
#
# 给 `GATE_ONLY` 一串空格分隔的格名 ⇒ 只跑那几格（e2e 那四格用套件短名）。
# 四条纪律焊在下面的代码里，一条都不是装饰：
#   ① `GATE_ONLY` 里有一个名字不是盘上真有的格 ⇒ **红**。拼错**不许**静默降级成「少跑一格」；
#   ② 跳过的格**逐字印出来**，不许只印跑了的那几格；
#   ③ 只要跳过了一格，裁决行就**不许**是 `GATE: OK`，换成 `GATE: PARTIAL` ——
#      🔴 **`GATE: OK` 这四个字的意思只有一个：盘上每一格都跑过了。** 不许有第二种意思；
#   ④ 自检探针（`自检①`–`自检⑩`）**不受 `GATE_ONLY` 影响**：它们是量具的量具，
#      被过滤掉就等于把门禁自己的自检关了，而那一形在输出上看不出来。
#      ⇒ `gate_selftest` / `gate_selftest_e2e` 里各有一句 `local GATE_PROBE=1`（动态作用域）。
#   ⚠ 探针一律跑在 `$( )` 里（子 shell）⇒ 它们**进不了** `GATE_RAN`/`GATE_SKIPPED` 那两张表，
#     那不是巧合，是 `found_cells()` 只认行首调用、刻意不把探针数成格的同一条边界。
GATE_ONLY="${GATE_ONLY:-}"
GATE_RECEIPT="${GATE_RECEIPT:-.build/gate-receipt.json}"
GATE_DECLARED=()   # 盘上声明过的格（短名，现算）
GATE_RAN=()        # 这一趟**命令真的执行过并被判过**的格（规范名，现算）
GATE_SKIPPED=()    # 这一趟被 GATE_ONLY 挡掉的格（短名，现算）

# 这一格这一趟要不要跑。返回 0 = 跑。⚠ 副作用：把短名记进 `GATE_DECLARED`。
gate_wants() {
  if [ "${GATE_PROBE:-0}" = 1 ]; then return 0; fi
  GATE_DECLARED+=("$1")
  if [ -z "$GATE_ONLY" ]; then return 0; fi
  case " $GATE_ONLY " in
    *" $1 "*) return 0 ;;
  esac
  GATE_SKIPPED+=("$1")
  printf '  skip %-14s %s\n' "$1" "GATE_ONLY 没点它 ⇒ 这一格这一趟**没判**（收据里如实记着，裁决行因此不许是 GATE: OK）"
  return 1
}

# `GATE_ONLY` 里每一个名字都得是盘上真有的格 —— 拼错是**红**，不是「少跑一格」。
gate_check_only() {
  if [ -z "$GATE_ONLY" ]; then return 0; fi
  local tok d known
  for tok in $GATE_ONLY; do
    known=0
    for d in ${GATE_DECLARED[@]+"${GATE_DECLARED[@]}"}; do
      if [ "$tok" = "$d" ]; then known=1; break; fi
    done
    if [ "$known" -ne 1 ]; then
      fails+=("GATE_ONLY 里的 \`$tok\` 不是盘上任何一格的名字 —— \
拼错一个字就等于**静默少跑一格**，而少跑与跑过在终端上一模一样 ⇒ 一律按红记。\
盘上现打这几格（短名）：${GATE_DECLARED[*]}")
    fi
  done
}

# 一串字符串印成 JSON 数组。⚠ 格名里没有引号与反斜杠（`found_cells()` 那三条正则决定了这件事），
#   所以这里不做转义 —— 哪天格名里真出现引号，是那一边该改，不是这里该补一层猜。
gate_json_arr() {
  local i first=1
  printf '['
  for i in "$@"; do
    if [ "$first" -eq 1 ]; then first=0; else printf ', '; fi
    printf '"%s"' "$i"
  done
  printf ']'
}

# 落一张收据。**两条路都要落**（`OK`/`PARTIAL` 与 `FAIL`）——
# 只在绿那一支落，就买不到「红过一趟、然后有人把红的那一格删了」那一形。
gate_write_receipt() {
  local verdict="$1" sha tree head dirty
  sha="$(sha256sum "$0" 2>/dev/null | cut -d' ' -f1)"
  sha="${sha:-<数不出 sha256>}"
  tree="$(git rev-parse 'HEAD^{tree}' 2>/dev/null)"
  tree="${tree:-<不在 git 仓里>}"
  head="$(git rev-parse HEAD 2>/dev/null)"
  head="${head:-<不在 git 仓里>}"
  if git diff --quiet HEAD 2>/dev/null; then dirty=false; else dirty=true; fi
  mkdir -p "$(dirname "$GATE_RECEIPT")" 2>/dev/null
  {
    printf '{\n'
    printf '  "verdict": "%s",\n' "$verdict"
    printf '  "gate_sha256": "%s",\n' "$sha"
    printf '  "tree": "%s",\n' "$tree"
    printf '  "head": "%s",\n' "$head"
    printf '  "dirty": %s,\n' "$dirty"
    printf '  "when": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '  "gate_only": "%s",\n' "$GATE_ONLY"
    printf '  "ran": '    ; gate_json_arr ${GATE_RAN[@]+"${GATE_RAN[@]}"}        ; printf ',\n'
    printf '  "skipped": '; gate_json_arr ${GATE_SKIPPED[@]+"${GATE_SKIPPED[@]}"}; printf '\n'
    printf '}\n'
  } > "$GATE_RECEIPT" || {
    printf 'GATE: 收据落不了盘（%s 写不进去）—— 后面那条 K-G4C-gate-receipt.py 会因此红，那是对的\n' "$GATE_RECEIPT"
    return 0
  }
  printf 'GATE: 收据 —— %s（verdict=%s · 真判过 %s 格 · 跳过 %s 格 · tree %s · dirty %s）\n' \
    "$GATE_RECEIPT" "$verdict" "${#GATE_RAN[@]}" "${#GATE_SKIPPED[@]}" "${tree:0:12}" "$dirty"
}

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
#        `^::error::`          `tests/e2e/assert-pass-floor.sh` 自己的诊断
#        `^ *(FAIL|BROKEN|×|✗)` 本仓 bash e2e 与 `pb check` 自己的失败行
#        `^Diff in `           `cargo fmt --check` 指哪个文件哪一行不合排版
#                              〔09-10 补：fmt 那道门第一次被刀切红时，模式表**一条都没匹配上**
#                              （终端逐字「fmt 关键行：一条都没匹配上（共 72 行）」）——
#                              它自己那条兜底（原文尾部恒印）救了场，但「红的那一格必须
#                              自带为什么红」这句话当时只兑现了一半。⇒ 补这一形。
#                              ★ 它同时是 ① 这张表「天生会漏」那句话的**又一个实例**：
#                              加一道新门就可能带来一种新形状，**加门那一拍要顺手切一刀看诊断印不印得出来**〕
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
GATE_DIAG_PAT='^(error|npm error|npm ERR!|thread .+ panicked|failures:|test result: FAILED|::error::|Diff in )|^[[:space:]]*(FAIL|BROKEN|Running|×|✗)'

# ── `N-G1`（09-05）：**①段匹配之前先去一次色。同样一个字都没碰任何一条判定。** ────
#
# ★★ **病因是现打的，不是猜的。** 上一版这里的解释停在「模式表漏了这一形」，而
#   `N-F1b` / `N-F2` 两件的实现方连着两次在交回里写「关键行**一条都没匹配上**」
#   ⇒ 每一件的死值验都只剩「红了几条」，**是哪几条只能靠推断**。
#
#   `NG1D1` 的量法（量具住 `tests/evidence/N-G1-diag-match.py`，模式表从本文件现读，不复述）：
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
#   —— `cargo` · `backend` · `npm` · `e2e` · `pb check`；全是现打，不是抽样。
#   ⚠ 第九格 `generated` 不进这个分母：它红了走的是 `git diff --stat`，根本不调 `gate_diag`）：
#     npm·vitest      2413 行 · 含 ESC **1584** 行 · 改前命中 **0** · 去色后 **2**  ← 只有它中招
#     npm·tsx（`✗`）   186 行 · 含 ESC 0 · 改前 **1** · 去色后 1
#     cargo·workspace 1740 行 · 含 ESC 0 · 改前 **11** · 去色后 11
#     cargo·backend     642 行 · 含 ESC 0 · 改前 **6**  · 去色后 6
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
#     量具住 `tests/evidence/N-G1-gate-fn-md5.py`（可复跑）。
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
#   `test:diff` 的 `17 passed, 0 failed`（`tests/cards/diff.test.ts:234`）·
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
  gate_wants "$name" || return 0
  local out
  out="$("$@" 2>&1)"
  local rc=$?
  # 🔴 记在**命令执行完之后**，不是在派发之前 —— 收据里 `ran` 那一栏的意思必须是
  #   「这一格的命令真的跑过并被判过」，不是「这一格被点到过」。两者在收据上长得一样。
  GATE_RAN+=("$name")
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
#   `guard-core 24 · creds-core 18 · codex-token-core 3 · acct-core 9 · branch-core 8 ·
#    gate-core 8 · shell-quote-core 1` ⇒ **71 条**，其中 **53 条是本件之前就有的存量**。
#   ⚠〔09-18〕原文第三项是 `usage-core 11`，合计 **79**、存量 **61** —— 用量下线后那个 crate
#     改建成 `codex-token-core`，现打 `cargo test -p codex-token-core --lib` = **3 条**
#     ⇒ 合计 71、存量 53（存量 = 合计 − `K-H2a` 新开的 `creds-core` 18，两组数各自自洽）。
#     （`--exclude` 与**包数都不变**：不是减 crate，是同一格换了被测对象 ⇒ 下面那条
#      cargo 合计行里的包数 **9** 不动。⚠ 刻意不把那条调用的**逐字前缀**抄进本注释：
#      `shared_crate_registry::the_gate_package_count_tracks_the_number_of_shared_crates`
#      用 `find_pinned` 钉的就是那个前缀、**要求全文唯一**，抄一次它当场判红。现打栽过。）
#
# 〔TL1 · 4C 拍板 ③〕这里从前写着「`--exclude code-picture-core` 是承重的，不许删成裸 `--workspace`」——
#   08-27 现打那时 vendor 是 monitor 的 path 依赖，cargo 把它算成成员（`[workspace] exclude` 对 path 依赖不生效），
#   裸 `--workspace` 会多拉进 vendor 那 25 条我们无权修的判据（`C7`「vendor 不动」）。
#   RM1f 起 monitor 不再依赖它（链它的只剩 `src/panorama-engine`）⇒ 它不再是成员，那条 `--exclude` 只剩一条
#   cargo warning（`excluded package(s) not found`）⇒ 删了；裸 `--workspace` 现打就是 9 个成员（`monitor` ＋ 8 个共享 crate）。
#   🔴 **谁在守「vendor 别再被拉回来」**（死值验现打：往 monitor 清单加回那条 path 依赖 ⇒ `cargo metadata` 成员 9 → 10）：
#   一是 `shared_crate_registry::the_windows_cross_target_signal_covers_only_the_backend` ③（monitor 清单零 vendor 依赖，那一刀当场红）；
#   二是下面这一格的**包数相等**（那个 9：成员 10 ⇒ 合计行包数对不上 ⇒ 红；⚠ 这里刻意不抄那一行的逐字前缀 —— `find_pinned` 要它全文唯一）。
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
  gate_wants "$name" || return 0
  local out
  out="$("$@" 2>&1)"
  local rc=$?
  GATE_RAN+=("$name")
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
#   量具 `tests/evidence/N-G2-selftest-cost.py`（把自检段真正会走到的那几块**原样切下来**跑，
#   不复算），沙箱 `ccmon-devbox:latest` 里各 9 趟交替跑、取中位数：
#     · **本拍十条（①–⑩）≈ 131 ms**（135.1 ms 减去 4.5 ms 的空跑基线 = **130.6 ms**）
#       ⚠ 同一把尺子隔一会儿复打一次得 **128.6 ms** ⇒ 噪声带大致 **129–131 ms**，
#         **小数位不是读数**；要精确到几毫秒就自己重打，别引这里的小数。
#     · 同一把尺子量基点 `5924b91` **那四条 ≈ 54 ms**（58.5 − 4.2 = 54.3 ms）
#   ⇒ 🔴 **「四条 ≈ 10 毫秒」这句话本身就不成立**（同一把尺子今天量得 54 ms，差 5 倍）；
#     `N-G2` 只答「新加六条要多少」= **≈ +76 ms**。每条探针要 fork 一个 `bash`，那才是大头。
#     ⚠ 这两个数量于**这台机器、这个镜像、这一拍**，是快照不是常量 —— 引用前重打。
#     ⚠ 它们量的是**自检段**，不是整趟门禁（同一拍那一趟墙钟 61 秒）；**两个分母不同，别相减**。
#   ⇒ 「不碰 cargo / npm / 网络」**仍然成立**；**「不碰文件系统」对探针⑩ 不成立**（见下）。
# ⚠ 上面 `N-G1` 那段头注里「全文 `fails+=(` 从 **15** 涨到 **16**」是**那一拍的快照**，
#   今天已经不是全文的数了。本拍现打 **17 处真的 `fails+=`** = 门那侧 **11** ＋ 探针那侧 **6**
#   （口径与量具住 `tests/evidence/N-G2-verdict-md5.py` 的【覆盖自证】，它按「整行 strip 后以 `#` 打头」
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
#   🔴 下面这几个行号**钉在 `5924b91` 上**（本件基点），**不是本文件此刻的行号** ——
#     本段一落地它们就往下推了。本文件此刻的行号**现算**：
#     `python3 tests/evidence/N-G2-verdict-md5.py <基点>` 的【覆盖自证】每趟都把五块的行范围
#     与那 11 条各自的行号印出来，**那份才是当下的读数**。
#   盖住：`run_gate` 两条（`5924b91:352` `:355`）· `run_gate_sum` 三条（`:409` `:413` `:418`）·
#         `run_e2e` 的退出码那条（`:626`）。
#   盖不住 5 条，逐条给理由（`§4` 登记）：
#     · `generated` 两条（`5924b91:560` `:564`）与 `pb check` 两条（`:714` `:736`）—— **行内，不是函数**，
#       **没有可以喂合成输入的入口**。三条出路各有代价：抽成函数 = 改判定的形状（件计划 `§2` 明禁）·
#       另写一份独立复算 = 「盘上有 ≠ 被走到」的假绿 · 造一次 `git diff` 非空要**改工作树**
#       （`NG2D5` 硬边界明禁）。⇒ **做不到**，不硬凑。
#     · `run_e2e` 的「抓不到「合计 PASS=」」那条（`5924b91:634`）—— 立它要 `rc=0` **且**抓不到那行，
#       而 `tests/e2e/assert-pass-floor.sh` **只有跑完一整套 npm 套件才退 0** ⇒ 立它就得每趟真跑一套。
#       ⇒ **没立**（代价与自检段「不碰 npm」那条承诺直接冲突）。
#
# 🔴 **探针不许改工作树**（`NG2D5` 硬边界）：这十条一个 git 写操作都没有、不往工作树落文件、
#   不碰 `~/.claude`。⚠ 一条例外如实写：探针⑩ **读**了 `tests/e2e/assert-pass-floor.sh`（起一个 `bash`）
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
  # 🔴 探针**不受 `GATE_ONLY` 影响**（`gate_wants` 第一行读它）。bash 是动态作用域 ⇒
  #   这一句在本函数调出去的 `run_gate`/`run_gate_sum` 里也看得见。
  #   把量具的量具过滤掉，在输出面上**一个字都不会说**。
  local GATE_PROBE=1
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

# ── `worktree-clean`：**前置条件** —— 仓里不许有第二份工作副本（第 25 格，09-19）──
#
# 🔴 **它排在所有格之前，这是刻意的。** 本仓有一族判据的人群是「走文件系统」
#   （`walk` / `read_dir` / `readdirSync` / `find` / `eslint .`），现打至少 8 份文件里
#   有这种取法。仓内一出现第二份工作副本（并发 agent 的 worktree、死值验的变异副本），
#   它们的人群就**静默膨胀**，然后一片**和本拍改动毫无关系**的红。
#   〔2026-09-19 一天绊了三次：`shell_lint_registry` · `bus_identity_registry` ·
#     `cc_bus_deploy` · `eslint 基线 7 → 1383` 同时红，每次都要花时间才认出来。〕
#
# 🔴 **更坏的那一半今天没发作但它在**：上面那几条是**恒等**断言所以红得响。
#   同族里凡是用**地板**（`>= N`）的，人群膨胀时**一声不吭地过去** ——
#   「多扫了 1138 个文件」在地板下和「扫对了」长得一模一样。**那才是本格的真正理由。**
#
# ⚠ 它**不修**那一族（八处以上的改动，另案），只让这个条件先出声、并说清
#   「这不是你的改动坏了」。⇒ 本格红时**先清副本再重跑**，别去追下面那些红。
run_gate worktree-clean '判过的条数（抽样的 4 个扩展名 `.sh`/`.mjs`/`.rs`/`.ts`，每个一条**恒等**断言：`git ls-files` 认的份数 == 走文件系统走出的份数）。⚠ **抽样不是全集** —— 挑的是那几条真出过事的判据在数的东西（`.sh`→shellcheck 那一族 · `.mjs`→eslint 基线 · `.rs`→`readonly_guard` 的分区恒等 · `.ts`→`tsc` 的 `want`）。⚠ 它买的是「仓里没有第二份工作副本」，**买不到**「所有判据的人群都对」——一份被 `.gitignore` 掉的源码同样会让走文件系统的判据多看一份，而本格按 gitignore 的口径算、看不见它' \
         python3 tests/evidence/K-W25-worktree-clean.py


# ── `hooks/` 里那份**会被执行**的东西，跑不跑得起来（`K-R82` 09-12，第 13 格）──────
#
# ★★ 题面是 `K-R80` 的转置读数（`DECISIONS.md#R42` 裁定四）：**12 格里 0 格看着 `hooks/`**，
#   而 `tests/hooks/pre-commit` 与另两棵 0 覆盖的树（`evidence/` · 仓根文件）**性质不同** ——
#   它**会被 git 执行**、跑在**每一次提交**上、**能改仓**（它挡的是 `C7` 那条
#   「`[profile.dev]` 不许进提交」）。
#
# 🔴 **本格落地那一趟就逮到一条真的**（现打，不是合成的）：
#   `tests/hooks/pre-commit` 在 index 里是 **`100644`** ⇒ 本仓 `core.filemode=false`，
#   `chmod +x` 从来没进过 git ⇒ **每一棵新 checkout 出来的工作树里它都是 644**，
#   而 git 对 644 的 hook 的处置是**忽略它并照常提交**（`rc=0` ＋ 一句可关掉的 advice hint，
#   现打读数在 `tests/evidence/K-R82-hooks-gate.md` `§1`）⇒ **那道挡在那些树里等于不在。**
#   本拍用 `git update-index --chmod=+x` 把它记进库里，本格从此盯着它不再掉。
#
# ⚠ **两句话分开判**（记忆条 `filemode-false-chmod-invisible`）：
#   「**盘上跑不跑得起来**」（`test -x`，跟 checkout 走）与「**库里记没记**」
#   （index mode，跟提交走）是两个互不相干的事实，本格各判一条、红了也分开说。
# ⚠ **失效方向**（件计划 `KR82D1` 逐字）：**只判「文件在不在」** —— 那和「它跑得起来」
#   是两件事。⇒ 判据里一条 `test -e` 都没有。
# ⚠ 判据本体住 `scripts/hooks-are-runnable.sh`（含 8 条阳性对照：三把尺子**正反各一条**，
#   挡「尺子瞎了」也挡「尺子恒红」）—— 放在那儿是为了能**对着变异过的副本**跑死值验，
#   不必去动真工作树。
# ⚠ 本格的数是**数出来的**（每个 hook 文件 3 条 ＋ 8 条阳性对照），跟 `fmt` 那几格的
#   「只有绿/红两态」不同 —— 往 `hooks/` 里加一份 hook，这个数会涨，那是对的。
run_gate hooks '每个被跟踪的 hook 文件 3 条（盘上可执行 · 库里记着可执行位 · 语法过得了它自己声明的解释器）＋ 8 条阳性对照。⚠〔09-20 订正〕本行原先写着「现打 hooks/ 下 1 个文件 ⇒ 11」——那是个**手抄的份数**，而本拍加了 `tests/hooks/pre-push` 之后盘上是 2 份 ⇒ 14。同 `copy2` 那一拍的订正：**摘掉抄来的数**，份数以判据本体自己印的那一行为准（它每趟从 `git ls-files tests/hooks/` 现算）。hooks/ 之外的任何一棵树本行都盖不到' \
         bash tests/scripts/hooks-are-runnable.sh

# ── 量具的**还原那一跳**有没有把旧 mtime 搬回被测树（`K-R115` `KR115D1`，09-14，第 14 格）──
#
# ## 题面：一条纪律立了一天，第二天在另一把量具里又长出来
#
# `K-R75`（09-12）：变异台 `restore` 用 `shutil.copy2` 把**旧 mtime** 一起搬回
#   ⇒ `cargo` 判「源码没变」⇒ 复用上一刀的产物 ⇒ **那一趟读到的是上一刀的回声**，整趟作废。
# `K-R102`（09-13）：**同形复发**在另一把量具上 —— `M6-final` 印 `GATE: OK · backend 755`，
#   而那条判据还在盘上、一趟都没跑。它自己逮住并重跑。
# ⇒ 当时的处置逐字是「每趟变异都要有一个『它真的重编过吗』的活体信号」——
#   **一句纪律，没有任何东西在执行它**。第二次发生就是证据。本格是那句纪律的机器面。
#
# ⚠ **它判的不是「源码里有没有 `copy2` 这个词」**：`shutil.copy2` 有正当用途
#   （造夹具 · 拷读数文件 · 把二进制搬进临时目录），那些一个都不该红。
#   判的是**这一次复制的目的地落不落在「被 git 跟踪的工作树内容」上** —— 落在那儿，
#   你在还原被测源码；落在临时目录 / 一个 git 里一份文件都没有的暂存目录，你在造夹具或备份。
# ⚠ 判据本体住 `tests/evidence/K-R115-ruler.py`（`--census` 只印人群表不判，供死值验对照）。
#   它的**诚实边界**（看不见 shell 串里的 `cp -a`、看不见 `tarfile`、判落点不判意图）
#   逐条写在那份文件的头注里，**这里不复述一份**（复述就会漂）。
# ⚠ 本格是**唯一一格盖到 `evidence/`** 的门。那棵树在 `K-R80` 的登记里此前是
#   「0 格覆盖，而这正是它的用途」（`[J3 陈账]` 死锁的泄压口）—— 本格落地之后那条登记要跟着改，
#   随动逐处交回 PM，`tests/evidence/K-R115-deathvalue.md` 里点名。
run_gate copy2 '`evidence/*.py` 里，`shutil` 保元数据复制族（copy2 · copytree · copystat）的**调用点**数，逐处判目的地；绿行那个数就是判过的调用点数。⚠ 〔`K-R122` 09-14 订正〕本行原先写着「现打 176 份」——那是从判据本体那句现算的分母**手抄**过来的第二份，而本件落地前盘上已经是 183、落地后 185 ⇒ **摘掉那个抄来的数**，份数以 `tests/evidence/K-R115-ruler.py` 自己印的那一行为准。⚠ 只看 `evidence/` 下的 `.py`，别的目录、别的语言、shell 串里的 `cp -a` 本行一概盖不到' \
         bash -c 'python3 tests/evidence/K-R115-ruler.py'

# ── `shellcheck`：**CI 独有的那一格收进门禁**（`K-R122` `KR122D2` 甲，09-14，第 17 格）──
#
# ## 题面：这一格在本门禁里**一格都没有**，而它在 CI 里是独立一个 job
#
# 〔散文墓碑 · 2026-09-18〕**下面这段立项理由今天已不成立，原话照留。**
# 原话：「`K-R119`（09-14）推 `v3.8.0` 那一趟被 `release.yml` 自己的 `ci-gate` 拦下，
# CI 五条红里有一条就是它：`e2e-smoke` job 的 `shellcheck --severity=error`
# 报 `SC1081` 六处，全在 `tests/e2e/usage-probe-acceptance.sh`（一个叫 `FOR` 的函数，
# 它按「大小写写错的关键字」判 error）。」
# 🔴 **今天不成立的是「全在」那个住址**：用量整条产品面下线，
# `tests/e2e/usage-probe-acceptance.sh` **整份删了** ⇒ 那六处 `SC1081` 连同宿主一起没了，
# 这条立项理由指着一个不存在的文件。**留着原话是因为它是本格当初为什么存在的唯一记录**
# （「那一趟被拦下」这件事发生过，不因文件删除而变假）。
# ⚠ **立项理由作废 ≠ 本格作废**：本格钉的是「这一维在本门禁里有没有格」，
#   而那个答案今天仍是「有了才对」——人群从 ci.yml 现读（见下），一格都不许再回到零。
# 而同一棵树上本门禁 **16 格全绿** —— 〔量于 09-14 本件落地之前〕
# `grep -c -i shellcheck scripts/gate.sh` = **0**。
# ⇒ 这不是「射程印出来了没人读」，是**这一维根本没有格**。
#
# ⚠ **本段刻意不让任何一行以 `#` ＋ 空格 ＋ 那个工具名开头** —— 那是它的**指令**语法
#   （`# shellcheck disable=…` 那一形）。现打栽过一次：本段第一版有一行那么开头，
#   它当场报 `SC1072`/`SC1073`（`Expected '=' after directive key`）把本文件自己判红。
#
# ## 🔴 人群**从 `.github/workflows/ci.yml` 现读**，本文件不写第二份清单
#
# 那份清单（`FILES=$(printf …)` 那一段 ＋ 它下面那条覆盖面地板）今天已经有唯一住址，
# 而且 `src/bridge/src/shell_lint_registry.rs` 那条恒等判据就是靠**解析它**来钉
# 「每个 shell 脚本要么进 shellcheck 要么登记豁免」。
# ⇒ 在本文件里抄一份 = 同一个闭集第三个住址，三处必漂（本仓那笔账写在
#   `tests/e2e/assert-pass-floor.sh` 的地板纪律里，已经栽过两次）。
# ⇒ 本格**解析那一段**取人群、**解析那条地板行**取地板，一个数都不写死。
#
# ⚠ **fail-closed 的三条**（缺一条它就会在「解析坏了」的时候静默地绿）：
#   ① `shellcheck` 不在 PATH ⇒ 红（不许退化成「跳过」）；
#   ② 解析不到地板数 ⇒ 红；
#   ③ 展开出来的份数 < `ci.yml` 自己那条地板 ⇒ 红 —— 解析坏了最可能的样子就是展开出 0 份，
#      而「扫了 0 个文件」与「全都干净」在 `shellcheck` 的退出码上一模一样。
#
# ⚠ **诚实边界**：
#   · 本格与 CI 那一格用的是**两份 shellcheck 二进制**（沙箱镜像 0.9.0 · `ubuntu-latest` 自带）。
#     版本不同 ⇒ 规则集可能不同；本格买的是「**本地先看见**」，不是「与 CI 逐字等价」。
#   · 它只判 `--severity=error`（与 CI 同一档）；warning / info / style 一概不看。
#   · 它读 `ci.yml` 的**那一段文本**。那一段的写法一变（换成别的取人群方式）⇒ 本格红在
#     「展开份数不够」上，**那是对的**：人群换了家，就该有人回来看一眼。
gate_shellcheck() {
  local yml=".github/workflows/ci.yml"
  local block pats floor n out rc
  command -v shellcheck >/dev/null 2>&1 || {
    printf 'shellcheck: 这台机器上没有 shellcheck —— 判不了，按红记（不许退化成静默跳过）\n'
    return 1
  }
  [ -f "$yml" ] || {
    printf 'shellcheck: 读不到 %s —— 人群的唯一住址不在了，判不了\n' "$yml"
    return 1
  }
  # 取 `FILES=$(printf …)` 那一段：从它那一行起，到第一行以 `)` 收尾的行为止。
  block="$(awk '/FILES=[$][(]printf/{f=1} f{print} f && /[)][[:space:]]*$/{exit}' "$yml")"
  # 首行剥到最后一个单引号（那是 `printf` 的格式串尾），每行剥行尾续行符，末行剥收尾括号。
  pats="$(printf '%s\n' "$block" | sed -e "1s/^.*'//" -e 's/\\[[:space:]]*$//' -e '$s/)[[:space:]]*$//')"
  floor="$(grep -oE '"\$N" -ge [0-9]+' "$yml" | grep -oE '[0-9]+' | head -1)"
  case "${floor:-}" in
    ''|*[!0-9]*)
      printf 'shellcheck: 从 %s 里解析不到那条覆盖面地板（`[ "$N" -ge <数> ]`）—— 判不了\n' "$yml"
      return 1 ;;
  esac
  local -a files=()
  shopt -s nullglob
  # shellcheck disable=SC2206
  files=($pats)
  shopt -u nullglob
  n=${#files[@]}
  if [ "$n" -lt "$floor" ]; then
    printf 'shellcheck: 人群只展开出 %s 份，而 %s 自己那条地板是 %s —— 要么那一段的写法变了、要么真的少了文件。「扫了 0 个」与「全都干净」在退出码上一模一样，所以一律按红记\n' \
           "$n" "$yml" "$floor"
    return 1
  fi
  # 🔴 `LC_ALL` 是承重的，别删：本仓的 shell 脚本里有大量中文注释，而 shellcheck 报告时会把
  #   出错那一行**原样打出来**。沙箱镜像里 `LANG` 未设（现打 `locale -a` 只有 `C` / `C.utf8` /
  #   `POSIX`）⇒ 非 UTF-8 下它死在 `commitBuffer: invalid argument (invalid character)`、rc=2，
  #   而那个 rc 与「真有 error」在退出码上一模一样。CI 的 `ubuntu-latest` 自带 `LANG=C.UTF-8`
  #   ⇒ 不设这一条，本格与 CI 那一格在**同一份输入**上会给出不同的答案。
  out="$(LC_ALL=C.UTF-8 shellcheck --severity=error "${files[@]}" 2>&1)"
  rc=$?
  printf '%s\n' "$out" | head -80
  if [ "$rc" -ne 0 ]; then
    printf 'shellcheck: 退出码 %s —— 云端 `E2E scripts health` 那个 job 跑的是同一条命令、同一份人群，它会红\n' "$rc"
    return "$rc"
  fi
  printf 'shellcheck: %s passed（人群与地板都是从 %s 现读的，本文件一个数都没写死；地板 %s）\n' \
         "$n" "$yml" "$floor"
}
run_gate shellcheck '不是「几条断言过了」：这个数是**从 `.github/workflows/ci.yml` 现读的那张人群**展开出来的 shell 文件份数（与云端 `E2E scripts health` 那个 job 同一份人群、同一档 `--severity=error`）。⚠ 只判 error 这一档；warning/info/style 本行一概不看。⚠ 沙箱与 CI 是两份 shellcheck 二进制，版本可能不同 ⇒ 本格买的是「本地先看见」，不是「与 CI 逐字等价」。⚠ 人群之外的 shell（`.ps1` 全仓零 lint · 没进那张人群的任何脚本）本行盖不到' \
         gate_shellcheck

# ── `ci-e2e-prereq`：**CI 里那些 e2e 的前置跟没跟上**（`K-R122` `KR122D1` ③④，09-14，第 18 格）──
#
# ## 题面：`K-R119` 那趟五条红里有**两条**是这一形，而它在本地一个字都看不见
#
# `K-R48` 第二拍（09-11）把 `ccm` 收成后端的原生命令、`K-R104`（09-13）把用量探针整条
# 重写成「往真后端的帧面写帧」—— **两次都换了被测对象，而 `ci.yml` 里那两处 job 的
# 前置一次都没跟上** ⇒ 云端双双红在「找不到原生入口 / 需要先 build backend」。
#
# 🔴 **为什么本地看不见**：本脚本自己在跑四套 ccm e2e 之前**有一步 build 后端二进制**
#（下面那行 `e2e 前置`），CI 那两个 job 没有 ⇒ 同一份被测对象，两边的「绿」长得一模一样。
# ⇒ 这一格把「**前置齐不齐**」本身变成判据：它不跑任何 e2e，只读 `ci.yml` ＋ `package.json`
#   ＋ 那些 `.sh`，对账「每一套硬门后端二进制的 e2e，同 job 里都有一条 build 排在它前面」。
#
# ⚠ 判据本体住 `tests/evidence/K-R122-ruler.py`，**它的射程与买不到的东西逐条写在那份文件的头注里**，
#   这里不复述一份（复述就会漂）。放在那儿也是为了能对着变异过的副本跑死值验
#   （`K_R122_ROOT=<副本>`），不必去动真工作树 —— 与 `hooks` 那一格同一条取法。
run_gate ci-e2e-prereq '判过的 e2e 调用行数（`ci.yml` 的 `steps:` 里形如 `assert-pass-floor.sh <套件> <地板>` 的 `run:`，现打 20 条），其中「被测对象是后端二进制」的那几条逐条要求同 job 里有一条 `cargo build` 排在它前面。⚠ 它**不跑任何 e2e**，只读盘上三份文本 ⇒ 「前置齐了」不等于「那一套会绿」；⚠ 认「要不要二进制」靠一个字面量、认「有 build」靠 `cargo build` 四个字，两处的失效形状逐条写在判据本体的头注里' \
         python3 tests/evidence/K-R122-ruler.py

# ── `release-gate`：**发版那条流水线的两件事**（`K-R124` `KR124D1`/`KR124D2`，09-15，第 20 格）──
#
# ## 题面：一条**从加进去那天起就不可能过**的守卫，在本地一个字都看不见
#
# `K-R114`（09-14，`d1a0552`）在 `ci.yml` 里加了「`release.yml` 手工触发守卫」，判据本体
# 整段写在 `run: |` 块里。而 **runner 会把 `run:` 里的 `${{ … }}` 先求值再交给 shell**
# ⇒ 它要比的那个字面渲染后变成 `"false"`，与盘上那串模板**在三个触发器上都必不相等**。
# 云端实打读数住 `tests/evidence/K-R123-发版读数.md § 1.3`。
#
# 🔴 **为什么坏了一个月没人看见**（这一半才是本格存在的理由）：那段判据用 `yaml.safe_load`
#   写，而沙箱镜像里 `python3 -c 'import yaml'` 是 `ModuleNotFoundError`
#   ⇒ **它在本地一次都跑不起来**；而 `ci.yml` 里它前面那一步（shellcheck）先红，
#   `-e` 带着它一起没执行 ⇒ 云端也**从来没露过面**。两头都看不见。
# ⇒ 本格把它收进本地门禁：判据本体搬到 `tests/evidence/K-R124-ruler.py`（不依赖 PyYAML，
#   自带 YAML 子集切块器），**CI 那一步与本格跑的是同一份文件**，不是两份抄件。
#
# ⚠ 射程与买不到的东西逐条写在那份文件的头注里，这里不复述一份（复述就会漂 ——
#   与上面 `ci-e2e-prereq` 那一格同一条取法）。放在那儿也是为了能对着变异过的副本跑死值验
#   （`K_R124_ROOT=<副本>` / `RELEASE_WORKFLOW=<某份 release.yml>`），不必去动真工作树。
#
# ── 🔴 `19b`（09-19）：本格**多买了第三件事 —— 产字节那条路** ────────────────────
#
# 条 63 承诺三格平台，`G4`（上面 `platform` 那一格）已经把「**门禁盖到了哪几格**」对上了。
# 但那一格读的是**本文件**，它答不了另一半：「**发版那趟真的为那几格产字节吗**」。
# 两半必须分开，理由是硬的（`设计/96 §7.1.2` 现打）：三个落点全部 gitignore ⇒ **字节不进仓**，
# 三条产线**只由 `release.yml` 一个文件驱动** ⇒ 这张表的门禁**只能建在 `release.yml` 上**。
# ⇒ 本格从 `19b` 起同时判：⑨ 承诺的平台 ↔ 产线两向相等、本文件里「编后端」的步骤 ↔ 登记
#   两向相等、target triple 两向相等、runner 标签逐字；⑩ 每一处抠 `const BUILD_ID` /
#   身份戳界标的住址**实打读那份文件**、抠不出恰好一行就红；⑪ `build.rs` 那一侧
#   「抠不到」是一条**所有构建形态都响**的失败（`"unknown"` 兜底从类型上消失）；
#   ⑫ 本格 `muslbuild` 裁词里点名的工具链版本 == `release.yml` 真装的那两个。
# 🔴 ⑩ 有现物：步 9 把 `BUILD_ID` 搬进 `lib.rs` 时，`release.yml` 里**两处**抽取只改了一处，
#   另一处留在 `main.rs`（那里今天没有那个 const）⇒ 真发版会死在抽取上。本拍两件事一起做：
#   住址收进 `env.CCM_BACKEND_IDENTITY_SRC`（一处），并让判据每趟实打核它指得到真东西。
#
# ── 〔`19c` 09-19〕⑬：产字节那条路的**本机那一端** ─────────────────────────────
# 同一条路，`19b` 收了云端那一端（产线 ＋ 吃字节的 `build.rs`），`19c` 收本机那一端：
# 「bump 了 `BUILD_ID` 之后，谁把那两份内嵌字节重编回来」。在它之前那条配方**手抄在
# `build.rs` 的 panic 文案里**，而且与发版那趟**不是同一条路**（`rust-lld` vs `zigbuild`）。
# ⇒ 收成一条命令 `tests/scripts/re-embed.sh`，两侧配方由本格两向对拍。
# 🔴 ⑬d 也有现物：步 8 全仓改名之后 `src/bridge/.gitignore` 还写着 `/embedded-daemons/`
#   与 `/native-daemon/` ⇒ **两个内嵌落点从那天起就没被挡住**（09-19 现打 `git check-ignore`
#   两条都不命中），而 `设计/96 §7.1.2` 与 `release.yml` 文件头都还把「三个落点全部
#   gitignore」当硬事实在用 —— 那句话在本拍之前是假的。
# ⚠ **本格不因此变成「编译格」**：⑬ 一条字节都不编，⑬g 真跑的只是那条命令的 `--check`（只读）。
run_gate release-gate '判过的条数（`release.yml` 上逐行印出来的 PASS：三条地板 ＋ ①触发得了 ②手工默认不发布 ③`env.PUBLISH` 字面 ④两处发布步骤的闸 ⑤CI 门的闸 ⑥两处发布步骤各自的正文来源 ⑦生成器排在发布步骤前面 ⑧生成器吐得出本版正文 ＋〔19b〕⑨产字节那条路（承诺的平台 ↔ 产线 · 编后端的步骤 ↔ 登记 · target triple ↔ 登记，三条都是**两向集合相等**；每条产线步骤在那个 job 里 count()==1；runner 标签逐字）⑩每一处抠 `const BUILD_ID`／身份戳界标的住址，逐处计数相等 ＋ **实打去读那份源码**、抠不出恰好一行就红 ⑪`build.rs` 那一侧「抠不到」是所有构建形态都响的失败（`unknown` 兜底从类型上消失）⑫本文件 `muslbuild` 裁词点名的工具链版本 == `release.yml` 真装的那两个 ＋〔19c〕⑬`BUILD_ID` bump 的同拍步骤 re-embed（`tests/scripts/re-embed.sh` 是出路的**唯一住址**，`build.rs::REEMBED_CMD` 逐字指着它；本机那条配方与 `release.yml` 产字节那一步**同源** —— target **两向集合相等** ＋ 旗标逐字相同；它铺的 arch ↔ `build.rs` 吃的 arch **两向集合相等**；三个内嵌落点 ↔ `src/bridge/.gitignore` 里带机检锚的那几行**两向集合相等**；`build.rs` 那两个内嵌函数的出路各点名那条命令 ≥2 处、代码行里不许再手抄第二条产字节配方；mtime 那张安全网仍看**两份**源码；末一条**真跑** `re-embed.sh --check`，要有数）。⚠ 它**不执行 GitHub 的表达式求值器**，也**不跑那条流水线** ⇒ 「盘上这几份文本满足这几条」不等于「云端那一趟会绿」——⑨ 尤其如此：「登记的那一步在文件里」≠「那一步在 runner 上编得出字节」，更不等于「那份字节在目标机器上跑得起来」，真机那一维仍是**判不了**；⚠ 「往 Release 上写」只认两种形状（`softprops/action-gh-release` 的 `uses:` · `run:` 里的 `gh release`/`gh api …/releases`），换第三种路子上传它看不见；⚠ 正文**写得对不对**它一个字都不判；⚠ ⑬ 那一组同一条边界 —— ⑬a–⑬f 全是**盘上文本**的对拍，「配方写得一样」≠「那条命令今天在这台机器上跑得出字节」（它要 zig ＋ cargo-zigbuild，本格一个都不装、不跑）；⑬g 真跑的只是 `--check`（**只读**），在一棵没铺字节的树上它只答得出「这里没有一份对不上的字节」，**不是**「字节是对的」，更不是「发版那一拍办完了」' \
         python3 tests/evidence/K-R124-ruler.py

# ── `gate-selfdesc`：**门禁自述 ↔ 门禁现状的对拍**（09-19，第 22 格）───────────
#
# 🔴 **立这一格的直接起因是本门禁自己说了一整天假话。** `pb check` 那一格 09-18 已按用户
#   拍板**整格删除**，而裁决行照旧点着它的名、自称「21 格全绿」——实跑 20 格。
#   ⚠ **本该抓住它的东西一直都在**：`tests/evidence/K-R80-gate-cell-coverage.py` 的
#   `C5`/`C5b` 逐字就是「裁决行的数 == 现打格数」「逐格点名集合相等」。它没瞎，是**两头坏**：
#     ① **没人跑它** —— 它只被本文件头注引为「可复跑」，**不在执行链上**；
#     ② 它默认找的是**重构前**的 `scripts/gate.sh` ⇒ 手跑当场 `FileNotFoundError`。
#   两头叠在一起，效果就是本仓自己的那句话：**坏尺子会把真缺陷一起藏起来。**
#   ⇒ 本拍把 ② 修了，并把它接成真的一格，治 ①。**从此「自述腐了」这件事不靠人记得手跑。**
#
# ⚠ **它判的是登记，不是行为** —— 它只买「本文件的自述与盘上现状对得上、且每条登记指得到
#   真东西」。裁词对不对（某格到底盖没盖到某棵树）它一个字都不判，那要读语义。
# ⚠ **它是本门禁里唯一一格「被测对象就是本文件自己」** —— 改本文件的头注、加删一格、
#   动一句自述，都会在这一格上出声。这是刻意的：自述与现状分叉，正是它要治的病。
run_gate gate-selfdesc '判过的条数（逐项分母由判据本体每趟现算并印在它自己那行上：C1 两向集合对拍 ＋ 每格一条逐字锚点 ＋ 逐格逐树的裁词与理由 ＋ 树的分区恒等 ＋ 每棵树非空与在盘 ＋ 裁决行格数对拍 ＋「不需要门」的说明与钉子 ＋ 分档表逐份成员）。⚠ **反空真锚不是这个数，是 `C1` 那两向集合相等** —— 登记空了或 `gate.sh` 读成空串，两个集合当场分叉（死值验：改一格的名 ⇒ C1 红）。⚠ 它只判「登记完整且指得到真东西」，**不判裁词对不对、不判归得对不对**（那要读语义）' \
         python3 tests/evidence/K-R80-gate-cell-coverage.py

# ── `muslbuild`：**远端 Linux 那一格**（G4 · 09-19，第 23 格）─────────────────
#
# 🔴 **它补的是 `真相源/92 §2` 登记的 G4 空洞①**，逐字：「**musl 那两个 target 在 CI 与
#    本地门禁里都是零命中**（只在 tag 那天编一次）」。远端 Linux 是**条 63 点名的三格
#    承诺平台之一**，而它在每次提交上一个字节都没人验 —— 坏了要等推 tag 那天才知道。
#
# 🔴 **用 `cargo zigbuild`、版本跟 `release.yml` 对齐，这不是洁癖**：
#    zig **0.14.0** ＋ cargo-zigbuild **0.23.0**。
#    版本不同 ⇒ 本格的绿**不代表发版那趟会绿**，而那正是这一格要买的东西。
#    🔴 〔`19b` 09-19〕**这句话从此有人核了**：上面 `release-gate` 那一格的 ⑫ 把
#      「本格裁词里点名的版本」与「`release.yml` 里真装的那两个」**两向对拍** ——
#      在那之前这是两处手抄的数，漂了没有任何东西会说话。
#    ⚠ 原文这里写着 `release.yml:168` / `:173` 两个**行号**，`19b` 删掉了：
#      行号会随那份文件的每一次改动而漂（本拍就漂了 20 多行），而它指的东西已经由 ⑫ 核着。
#    ⚠ 现打一条差异如实记：**宿主上装的是 zig 0.16.0**；沙箱镜像刻意钉 0.14.0 ——
#      门禁要代理的是**发版那条路**，不是这台开发机。
#
# ⚠⚠ **诚实边界，写死别读宽**：它买的是「**编得出静态字节**」。
#    **买不到**「那份字节在真的远端 Linux 上跑得起来」（没有真机、没有运行）；
#    **买不到** `--all-targets`（这里只编 bin：测试档在 musl 上要跑不要编，另一回事）。
run_gate muslbuild '不是数出来的数：两个 musl target 各一趟 `cargo zigbuild`，只有绿/红两态。分母 = **条 63 承诺的「远端 Linux」那一格的两个 arch**（`x86_64` ＋ `aarch64`），逐个编。⚠ 买的是「编得出静态字节」，**不买**「在真远端上跑得起来」（无真机、不运行）、**不买** test 档（只编 bin）。⚠ 工具链版本与 `release.yml` 对齐（zig 0.14.0 / cargo-zigbuild 0.23.0）—— 版本一漂，本格的绿就不再代表发版那趟会绿' \
         bash -c 'cd src/backend && n=0; for t in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do cargo zigbuild --target "$t" >/dev/null || { echo "musl: $t 编不过"; exit 1; }; n=$((n+1)); done; printf "muslbuild: %s passed（两个 arch 各一趟 cargo zigbuild，zig $(zig version)）\n" "$n"'

# ── `platform`：**承诺的平台 ↔ 门禁真跑的格**（G4 · 09-19，第 24 格）──────────
#
# 🔴 **上面那几格各自只说「我编得过」，没有任何东西说「该编的都编了」。**
#    条 63 点名三格承诺平台（本机 Windows x86_64 · 远端 Linux · 本机 Linux）＋
#    一格显式拒绝（Windows aarch64）。少一格门禁、或多一格没人登记的 target，
#    在今天的输出面上**一个字都不会说**。这一格买的就是那句话。
# ⚠ 它是**登记的机检**，不自己编任何东西 —— 判的是「门禁盖到了哪些平台」，
#   **不判那些平台上真的跑得起来**（那一维仍然判不了，逐字写在判据本体的头注里）。
run_gate platform '判过的条数（判据本体每趟现算并印在它自己那行上：P1 承诺表↔门禁格**两向集合相等** ＋ P2 每格一条逐字锚点 count()==1 ＋ P3 显式拒绝的那格全仓零脚印；〔S5〕原来的 P4「壳-折」随那一档放弃（V105）删了）。⚠ **反空真锚是 P1 那两向相等**，不是「承诺表里每条都找得到」——后者在表被清空时恒真。⚠ 它不编任何东西：判的是**门禁盖到了哪些平台**，不判那些平台上真跑得起来' \
         python3 tests/evidence/K-G4-platform-ledger.py

# ── `installface`：**安装面切件方案与量具的对账**（`K-R128`，09-15，第 21 格）──────
#
# ## 题面：第三块（`S1`–`S5`）要开工了，而撑着那个切法的两句话**一句都没有闸**
#
# 甲 `K-R117` `§3-3` 的**收工判据**逐字要「该件那一组的前端落点数恒等于 1」——
#   而 `ruler.py` 的 `§S5b` 今天**只出读数、不判红** ⇒ 那句话没有闸，
#   「这一组收干净了没有」只能靠人看那张表。
# 乙 **纪律 A**（「第二拍全程不改命令名 ⇒ 三份共用文件一字不动 ⇒ 五件写区才真不相交」）
#   **完全建立在纪律上，没有任何东西在盘它**，而 `S1` / `S5` 是方案里唯一许并跑的一对
#   ⇒ 一旦有人顺手改了个命令名，两件当场撞车**而且没人会响**。
#
# 🔴 **为什么非要进门禁**（`KR128D4` 的裁词）：这两条判定的失效方向**就是「没人在看」**。
#   不进门禁，唯一的跑者是「`S1`–`S5` 的实现方记得跑」—— 而件文件逐字写着那不是答案；
#   并跑窗口恰恰就是它全程静默的那一段。CI 那侧也够不着（本格断网跑，它是本地 evidence 量具）。
#
# 🔴 **它为什么第一天就是绿的**（`K-R128` `§0c`，这一条是承重的）：前端落点现打 8 份、目标 3，
#   把「恒等于 1」直接打开会**当场全红、而且要红到第三块做完** —— 那不是闸，是把门禁钉死。
#   ⇒ 装的是**棘轮**：每组当前落点**名单**钉在 `FRONTEND_PIN` 里，判**逐字相等**
#   （不是 `<=` —— `<=` 只防涨、不防「悄悄记错」）。份数一律 `len()` 现算，表里没有基数字面量。
#   ⇒ 任何人往上加一份落点、或把表改馊，**当场红**；某一件真收干净了，
#   **同一拍**把它那一行降下来 —— 降不动就是没做完。**不许改成 `<=`、不许把名单改大让今天好过。**
#
# ⚠ 射程与买不到的东西逐条写在 `tests/evidence/K-R117-ruler.py` 的头注里（`B1`–`B8`），
#   这里不复述一份（复述就会漂 —— 与上面两格同一条取法）。其中要在这儿点一句的只有一条：
#   **`src/bridge/src/parity_ledger.rs` 那一份 `§S5e` 判不了**（那 22 条命令名就是从它解析出来的
#   ⇒ 空真），它的闸在 `§S5c` 的闭集判定 —— 别把这一格读成「三份共用文件都判了」。
run_gate installface '判过的条数（`§S5c`/`§S5d`/`§S5e` 三节逐条印出来的 PASS：22 条命令各归一组 ＋ 闭集并集两向 ＋ 五组交集空 ＋ 5 组前端落点棘轮 ＋ 22 条包装层入口两侧 ＋ `claims()` 10 个装/卸符号各有着落）。⚠ `ruler.py` 原有的 `R1`–`R7` **不在这个数里**（它们只在红的时候出声，没有逐条的「过了」事件）⇒ 这个数**不是**「那把尺子判过的全部条数」。⚠ 落点只认**调用形状** `.<命令>(`，只在注释/散文里提到命令名的**不算落点**（否则这把尺子可以靠删一条注释变绿）；别的调用形状（`invoke("<名>")` 直呼）它看不见，那一档逐处印在 `§S5d` 第二档里只出读数。⚠ 度量的是「几**份**文件」不是「几处引用」⇒ 往一份已经在名单里的文件里再加一处引用**不红**。⚠ `parity_ledger.rs` 那一份 `§S5e` **判不了**（空真），闸在 `§S5c`' \
         python3 tests/evidence/K-R117-ruler.py


# ── `ccbus-twophase`：**cc-bus 两阶段读口 ＋ 三个适配 trait ＋ Windows 那一侧**（`w24c`，09-19，第 24 格）──
#
# ## 题面：cc-bus 这一族此前**没有任何一格在判它的行为**
#
# 落地前现打：`grep -c cc-peek tests/scripts/gate.sh` = **0**。
# 这棵树以前只被两样东西碰过 —— `shellcheck`（语法那一档）与 `plugin_class_registry`
# （脚本**条数**）⇒ 「这条命令做的事对不对」这一维一格都没有。
# 而本轮加的正是**有副作用的那一跳**（推进已读位置），它错一次的形状是
# 「消息被消费掉却没人看见」（实测：160KB 积压那件事故，40 条一次性读不到了）。
#
# ⚠ 判据本体住 `tests/evidence/W24C-ccbus-twophase-ruler.py`，**射程与买不到的东西
#   逐条写在那份文件的头注里**，这里不复述一份（复述就会漂 —— 与上面几格同一条取法）。
#   要在这儿点一句的只有一条：**没有真 Windows 机器** —— 那一侧靠 `CCBUS_ADAPT_OS=windows`
#   在 Linux 上把实现整段跑一遍，买的是「实现跑得通、能力自陈与降级是真的」，
#   **不买**「在 Windows 上跑得起来」。
run_gate ccbus-twophase '判过的条数（判据本体每趟现算并印在它自己那几行上：静态 7 条 —— `cc-peek` 零写面（写形集合 == 登记的两处豁免，且正控要在 `cc-commit` 上扫出写）· `.pos` 写点全仓集合相等 · 锁族不增 · 通用层零脚印（带正控）· 与 `cc-recv` 的渲染逐字节对拍 · `cc-recv` 的 sha256 恒等 · 手册页那几句；真跑 14 条 —— 令牌/CAS/anchor/分段/并发/截短自愈/Stop 钩子两条路/Windows 那一侧四条；kinds 静态 2 条（敲门模板零正文 ＋ 正控）· 真跑 3 条；保活 1 条 —— 共 27）。⚠ **反空真锚不是这个数，是末尾那条「标签集合与登记两向相等」** —— 某一格悄悄没跑与它过了，在输出上一模一样。⚠ 它**不判**在真 Windows 上跑得起来（无真机）、不判性能、不判并发的公平性' \
         python3 tests/evidence/W24C-ccbus-twophase-ruler.py

# ── 格式漂移 ────────────────────────────────────────────────────────────────
#
# 🔴 **这一格补的是本文件头注里那条「归 PM」的第 ②**（09-10 落，PM）。
#   那条注释当时写着**不能加的理由**，逐字：「卡在沙箱镜像没装 `rustfmt` 组件」
#   ＋「在它改之前，把这两维写成一道门 = 把 55 棵树的门禁一起打红」。
#   **那个前提 09-10 被改掉了**：`.claude/devbox/Dockerfile` 加了一层
#   `rustup component add rustfmt`（仓外文件，不进版本控制）。
#
# ## 为什么它值一道门 —— 同一天付了两次学费
#
# 09-10 云端 Windows 那一格**连红两趟，两趟都红在 `cargo fmt --check`**
# （run `34460879900` @ `payload.rs:2308` · run `34467490069` @ `cc_bus.rs:2328`），
# 而两趟之前**本机沙箱门禁都是 `GATE: OK`**。
# 🔴 更贵的是它的位置：**`fmt` 是那一格的第一步** ⇒ 它红了之后
# `clippy` / `cargo test` / 生成物检查**全部 `skipped`**
# ⇒ 那两趟真正想验的东西（Windows 上 cc-bus 那条修复）**一次都没跑到**。
#
# ★ 定性：**本机门禁不是云端的超集，而「绿」这个字在两边长得一模一样。**
#   这一行就是把那句话变成假的。
#
# ⚠ **诚实边界，别读宽**：
#   · 它买的是「**排版与 rustfmt 一致**」，**买不到**「代码对」。
#   · 头注那条 ① （Windows 那半编不编得过，要 `--target x86_64-pc-windows-msvc`）
#     **今天仍然没买到** —— 沙箱镜像仍没装那个 target。**别把这一格读成两条都补上了。**
#   · 它跑在 `src/bridge` 上（`--all` = 那个 workspace 的全部成员）；
#     `src/backend` 是**另一个 workspace**，本行盖不到它。
#     🔴 **`K-R80`（09-12）：那句话一个字没改，改的是它后面缺的那一格** ——
#     那棵树今天由下面 `fmt-backend` 那一行盖。**别再把这一句读成处置。**
run_gate fmt '不是数出来的数：`cargo fmt --all --check` 只有绿/红两态（rc=0 / rc=1），本格的「分母」是 `src/bridge` 那个 workspace 的全部成员；`src/backend` 是另一个 workspace，本行盖不到（那一棵由下面 fmt-backend 那一格盖）' \
         bash -c 'cd src/bridge && cargo fmt --all --check 2>&1 && echo "fmt: 1 passed"'

# ── backend 那棵树的格式漂移（`K-R80` 09-12）──────────────────────────────────
#
# 🔴 **本格买的是上面那句诚实注释的处置。**
#
# ## 病不是「没人知道」，是「知道了而没人补」
#
# 上面那一格的分母里逐字写着「`src/backend` 是另一个 workspace，本行盖不到」，
# 而那句话**每趟门禁都印在终端上** —— 它不是静默失效，是**一格「我盖不到那儿」的注释
# 被当成了处置**。`K-R79` 交回时报出：backend 那棵树 `cargo fmt --check` **在基点上就是红的**，
# PM 现打复核 **6 处 / 3 文件**（`agents/mod.rs` 1 · `control/ccm/argv.rs` 4 · `protocol_doc_guard.rs` 1）。
# ⇒ **说清了射程 ≠ 射程够。** 本仓反复抓这一形，这一次长在门禁自己身上。
#
# ## 为什么是**多一格**，不是**并成一棵**
#
# 把 `src/backend` 塞进 `src/bridge` 那个 workspace 就能「顺便盖到」——
# **不许**。`K25` 裁的是「一份代码、每平台一份原生二进制」，而那棵树的 standalone
# 是**真架构约束**（它自己的 `Cargo.toml` 头注逐字：一个 workspace 会把这个 Linux-only 的
# backend 拖进 Windows CI 的 `cargo test --all`）。为一格排版去动两棵树的依赖关系，
# **代价远大于本格**。⇒ 多一行，各跑各的。
#
# ## 🔴 为什么是 `cargo fmt --check` 而**不是** `cargo fmt --all --check`
#
# **这一条是现打出来的，别顺手加 `--all` 去「对齐上面那一格」**（`K-R80` 09-12，
# 沙箱 `ccmon-devbox:latest`，`cargo fmt --all --check -v` 读它真喂给 rustfmt 的那串文件）：
# 在 `src/backend` 下加 `--all`，rustfmt 实收 **12 个 crate 根**，其中 **11 个不在这棵树里** ——
# `src/bridge/build.rs` · `src/bridge/src/lib.rs` · `src/bridge/src/main.rs` ·
# `crates/{acct,branch,codex-token,creds,gate,guard,shell-quote}-core/src/lib.rs`，
# 以及 🔴 **`src/bridge/vendor/code-picture-core/src/lib.rs`**。
#（成因：那棵树的 path 依赖指进 `../../src/bridge`，`cargo fmt --all` 顺着它们走出去；
#  `cargo metadata --no-deps` 的 `workspace_members` 现打**只有 1 个**，两者不是一回事。）
# ⇒ 加 `--all` 会把 vendor 那棵**我们无权修**的树拉进出货门禁 —— 与下面 `cargo` 那一格
#   从前 `--exclude code-picture-core` 要避开的是同一件事（`C7` 逐字「vendor `code-picture-core` **不动**」；〔TL1〕那条 exclude 随 vendor 退出 workspace 删了）：
#   **一道我们满足不了的闸，比没有闸更坏。**
# ⚠ 不加 `--all` 时 `cargo metadata` 那 11 个一个都不进来（同一趟 `-v` 现打：rustfmt 只收
#   `src/backend/main.rs` 一个根），读数 6 处不变 ⇒ **少的只有别人家那棵树。**
#
# ## ⚠ 诚实边界，别读宽
#   · 它买的是「**排版与 rustfmt 一致**」，**买不到**「代码对」——与上面那一格同一句话。
#   · 分母是**一个包** `cc-monitor-backend`，射程 = 从 `src/main.rs` 顺 `mod` 走得到的那些文件；
#     那棵树里**走不到的 `.rs` 文件本格看不见**（今天没有这样的文件，但那是事实不是判据）。
#   · `.github/workflows/ci.yml` 的 `backend` job **早就有这一步**（逐字同一条命令
#     `cargo fmt --check`，`working-directory: src/backend`）⇒ 本行**不是新买一条判据**，
#     是把「本机门禁不是云端的超集」这个已知缺口在这一维上补平。⚠ 因此 `ci.yml` **不用改**，
#     上面那条「三处一起改」的纪律与本行无关。
run_gate fmt-backend '不是数出来的数：`cargo fmt --check` 只有绿/红两态（rc=0 / rc=1），本格的「分母」是 `src/backend` 那个 workspace 的唯一成员 `cc-monitor-backend`；`src/bridge` 与 `vendor/code-picture-core` 由上面 fmt 那一格与它自己的 exclude 管，本行盖不到（刻意不加 --all，理由见上方注释）' \
         bash -c 'cd src/backend && cargo fmt --check 2>&1 && echo "fmt-backend: 1 passed"'

# ── Windows 那半编不编得过 ──────────────────────────────────────────────────
#
# 🔴 **本文件头注那条「归 PM」的第 ① —— 09-10 买到了。**
#
# ## 它买的是这个项目今年最贵的那一课
#
# cc-monitor v1 是 **Windows 专供**，而 **Windows 上编不过这件事在 08-13 到 09-09 之间
# 没有任何人发现**（云端 CI 自 08-05 起红在第一步，后面全部 `skipped`）——
# 09-09 那一趟修出来 **17 个互不相同的编译错地址**。
# 本机门禁跑在 Linux 上，那 `#[cfg(windows)]` 的 **67 处**（`src/bridge/src`，现打 09-10）
# **根本不参与编译** ⇒ 它一次都没看见。
#
# ## 🔴 铁律 12 的刀（**这一格不是推的，是切出来的**）
#
# 在 `config.rs::atomic_replace`（`#[cfg(windows)]`）里放一行 `let _: u32 = "…";`：
#   · `cargo check -p monitor`（Linux 原生，= 门禁其余各格看得见的那一面）⇒ **退出码 0，全绿**
#   · `cargo check -p monitor --target x86_64-pc-windows-gnu`         ⇒ **退出码 101**，
#     并逐字点名 `error[E0308]: mismatched types` 在哪一行
# **两侧读数相反 —— 那正是这一格存在的全部理由。**
#
# ## 为什么是 `-gnu` 而不是 `-msvc`
#
# `-msvc` 扩不到 monitor 本体：`ring` · `libsqlite3-sys` · 四个 `tree-sitter-*` 都用 `cc-rs`
# 编 C，而它在 msvc target 上找 `lib.exe` ⇒ Linux 上没有（实测
# `error occurred in cc-rs: failed to find tool "lib.exe"`）。`-gnu` 走 mingw-w64，编得过。
# ⚠ **`-gnu` 是不是忠实代理，是量过的**：两者唯一的分歧点是 `target_env`，
# 而**全仓 `target_env` 命中 0 处** ⇒ 那 86 处一处都分辨不出这两者。
#
# ⚠ **本文件头注原本提议的窄买法（`-p creds-core --features harden --target …msvc`）
#   只盖 86 处里的 2 处（2.3%）** —— 它跑得通（实测 5.65s，与那条注释预测的 5.67s 对得上），
#   但 09-09 那 17 个编译错**全在 monitor 本体那 67 处里，它一个都逮不住**。⇒ 换成本行。
#
# ⚠⚠ **诚实边界，写死别读宽**：`cargo check` 买的是「**编得过**」——
#   **买不到「行为对」**（要真 Windows 机），**也买不到「MSVC 上链接得起来」**（`check` 不链接）。
#   真机行为那一格今天仍然是**判不了**，不是「通过」。
#
# ⚠ 依赖沙箱镜像装了 `mingw-w64` 与 `x86_64-pc-windows-gnu`（`.claude/devbox/Dockerfile`，
#   仓外、不进版本控制）。没装的机器上这一格会红在「找不到 target」——**那是对的**：
#   fail-closed 比静默跳过好。
# ⚠⚠ **`--all-targets` 是 `15 §5.1 A5` 补的，它把本格的射程从「生产段」扩到「生产段 ＋ test 档」。**
#
# 题面逐字（`15 §2.6` 漏洞 2）：「**`winchk` 少 `--all-targets`，而兄弟格 `winchk-backend` 有**，
# 并注明『云端那 10 个错**全在 test 档**，所以 `--all-targets` 是**承重的**』
# ⇒ **同一个性质两把不同长度的尺子**。修它只要一个词。」
#
# ⇒ 本行加上之后，两格量的是**同一件事的同一个面**，只是包不同：
#   · `winchk`        = `src/bridge` 的 `-p monitor` 一个包，生产段 ＋ test 档
#   · `winchk-backend` = `src/backend` 一个 crate，生产段 ＋ test 档
#
# ⚠ **它买不到的仍然一个字没变**（别因为射程变长就把这句读松）：
#   · 买的是「**编得过**」，**不是「行为对」**——那要一台真 Windows（`99 §4.5.8` 的 `G2a`）。
#   · 本格是 `-gnu`，**MSVC ABI 专属的那一类照旧盖不到**（`check` 不链接，且沙箱里没有 zig）。
#   · **包**这一维没变：8 个共享 crate 仍然只有 `-p monitor` 依赖图里的那几个被顺带 check 到，
#     `creds-core` 的 `--features harden` 那 2 处**本行还是盖不到**（`15 §2.6` 漏洞 4 还欠着）。
# ⚠ `--locked` 照旧带着：本格同时是 `src/bridge/Cargo.toml ↔ Cargo.lock` 那条对账的落点
#   （`doc_claim_registry` 两处逐字点名「门禁 `winchk` 那一格的 `cargo check --locked`」）。
#   与 `winchk-backend` 刻意不带 `--locked` 的差别是**另一维**，别顺手抹平。
run_gate winchk '不是数出来的数：`cargo check --all-targets --target x86_64-pc-windows-gnu` 只有绿/红两态。射程 = `-p monitor` 一个包的**生产段 ＋ test 档**（`src/bridge/src` 的 67 处 `cfg(windows)`；`--all-targets` 是 `A5` 补的，与兄弟格 `winchk-backend` 对齐 —— 那一格的读数逐字「云端那 10 个错全在 test 档」）；`src/backend` 那 17 处与 `creds-core` 那 2 处本行盖不到' \
         bash -c 'cd src/bridge && cargo check --locked --all-targets -p monitor --target x86_64-pc-windows-gnu 2>&1 && echo "winchk: 1 passed"'

# ── `winchk-backend`：**backend 那棵树在 Windows 上编不编得过**（`K-R122` `KR122D2` 甲，09-14，第 18 格）──
#
# ## 题面：上面那一行自己写着「盖不到」，而那句话 09-14 兑现成了发版被拦
#
# 上面 `winchk` 那一行的分母逐字写着「`src/backend` 那 17 处与 `creds-core` 那 2 处
# 本行盖不到」。`K-R119` 推 `v3.8.0` 那一趟，云端 `Remote backend (Linux) lint + test`
# 那个 job 正是红在它的第 7 步（`cargo check --all-targets --target x86_64-pc-windows-msvc`）：
# **10 个编译错，全在 test 档**。⇒ **射程印在那一行上，而没有任何东西替它出声。**
#
# ## 🔴 与 CI 那一格的差别，逐条写清（别把本格读成「和 CI 一样」）
#
#   · **target 不同**：CI 用 `x86_64-pc-windows-msvc`，本格用 `x86_64-pc-windows-gnu`。
#     现打（09-14，本沙箱镜像）：msvc 那条在这里跑不了 —— `ring` 的 build script 要编 C，
#     cc-rs 找不到 `lib.exe` ⇒ rc=101，而它**根本走不到我们自己的代码**
#     （CI 那边是靠额外装一个 `zig` 把 `lib.exe` 这一环补上的，沙箱镜像里没有 zig，
#     而 `.claude/devbox/Dockerfile` 不在本件写区）。
#   · **两个 target 对本族缺陷等价**：那 10 个错全是「这个名字在 Windows 上不存在」
#     （`std::os::unix` / `libc::utimensat` / `libc::AT_FDCWD` / `Permissions::from_mode`）——
#     那是 `cfg(unix)` 这一维，与 ABI 无关。**现打验过**：同一份未修的源码在本格这条
#     `-gnu` 命令下逐字报 `due to 10 previous errors`、`(bin "cc-monitor-backend" test)`，
#     与 CI 那趟 msvc 的读数**同数同档**。
#   · ⚠ **它买不到 MSVC ABI 专属的那一类** —— 只在 msvc 上才犯的毛病（C 依赖的链接面、
#     MSVC 特有的 `#[link]`）本格盖不到。**那一格仍然只有 CI 有。**
#
# ⚠ **`--all-targets` 是承重的，别「简化」掉**：那 10 个错**一个都不在生产段**
#   （同一份源码在 `build-windows` 里原生编出过 `cc-monitor-backend.exe`）。
#   不加这个 flag，本格会在这一族缺陷上**全绿**。
# ⚠ 依赖沙箱镜像装了 `x86_64-pc-windows-gnu` 这个 target（现打在；`.claude/devbox/Dockerfile`
#   仓外、不进版本控制）。没装的机器上本格红在「找不到 target」—— fail-closed，那是对的。
# ⚠ 刻意**不带** `--locked`：CI 那一步也没带（`src/backend` 的锁文件由它自己的
#   `cargo test` 那一步管）。一个性质两把尺子是本区最贵那族病。
run_gate winchk-backend '不是数出来的数：`cargo check --all-targets --target x86_64-pc-windows-gnu` 只有绿/红两态。射程 = `src/backend` 这一个 crate 的**生产段 ＋ test 档**（云端那 10 个错全在 test 档，所以 `--all-targets` 是承重的）。⚠ 本格用的是 `-gnu`，云端用的是 `-msvc`（沙箱里没有 zig，`ring` 的 build script 缺 `lib.exe`）⇒ **MSVC ABI 专属的那一类本行盖不到**；`src/bridge` 那棵树由上面 winchk 那一格盖' \
         bash -c 'cd src/backend && cargo check --all-targets --target x86_64-pc-windows-gnu 2>&1 && echo "winchk-backend: 1 passed"'

# ── `winlink`：**monitor 在 Windows 上链不链得起来**（WIN1 · 第四波 4D，第 30 格）──
#
# 守的要求：用户裁决 **V115**（「Win11 虚拟机可以当真机测试资源」）那一趟 RT1 现打出来的 F1 ——
# `RT1.md §8` 逐字「`-gnu` 交叉编 monitor **两个 profile 都链不过**：`monitor_lib.dll`（`[lib] crate-type`
# 里的 `cdylib`）导出序号超 65535（release 125 946 / dev 241 784）。门禁 `winchk` 只 `cargo check`，看不见」。
# ⇒ 上面 `winchk` 那一格的分母逐字写着「买不到『链接得起来』（`check` 不链接）」—— 这一格补的就是那半。
# 设计住址：`设计/01 §6.7a` 表 B 逐字「**本机 Windows**（x86_64） | ✅ **承诺** | 独立进程那个壳要有字节、要进门禁」。
#   以及 `设计/01 §7.3` 逐字「门禁补一格真链接、桌面不需要的 `cdylib` 收掉（4D WIN1）」。
#
# ## 它买什么 / 不买什么
#   · 买：`-p monitor` 的**两个二进制**（`monitor` 主窗 ＋ `cc-monitor-filewin` 文件窗口）在
#     `x86_64-pc-windows-gnu` 上**真走一趟链接器**、链得出 `.exe`。`[lib]` 那一格收成 `rlib` 之后
#     不再产 dll（WIN1：全仓没有移动端，`cdylib` / `staticlib` 两格零消费者）；有人把 `cdylib` 加回来
#     ⇒ 这里当场红在 `export ordinal too large`（死值验住 `第四波记录/WIN1.md`）。
#   · ⚠ 只链、不跑：「链出来的 exe 在 Windows 上起得来」要真机（`RT1.md` 那台虚拟机），本格判不了。
#   · ⚠ `-gnu` 不是 `-msvc`：发版那一格是 `windows-latest` 原生构建，MSVC 链接器那一类本格盖不到。
#   · ⚠ dev profile；release 那一档的链接本格不跑（`RT1-build-win.py` 走 release，它不在门禁上）。
#   · ⚠ 内嵌的本机后端字节此时**不在**（`native-backend/` 没铺）⇒ 链进去的是「没带后端」那一形，
#     与 `winchk` 同一形；带字节的那一形要 `RT1-build-win.py`。
run_gate winlink '不是数出来的数：`cargo build --bins --target x86_64-pc-windows-gnu`（dev）只有绿/红两态。射程 = `-p monitor` 的两个二进制（`monitor` · `cc-monitor-filewin`）**真链接**一趟；⚠ 只链不跑（起不起得来要真机）· `-gnu` 不是 `-msvc` · release 那一档不链 · test 档不链（那一半归 `winchk` 的 `check`）' \
         bash -c 'cd src/bridge && cargo build --locked -p monitor --bins --target x86_64-pc-windows-gnu 2>&1 && echo "winlink: 1 passed"'

# 13 个包 = `monitor` + 12 个共享 crate（〔TL1〕`vendor/code-picture-core` 早已不是成员 —— monitor 不再依赖它 —— 不用再 `--exclude`）。
# 〔CP2c〕9 → 10：加了 `copy-core`（对外文案表的 Rust 取文口）。
# 〔US1 · 4D〕〔合并 US1 × 主线〕10 → 11：新共享 crate `relay-route-core`（中转门牌：端口 · 钥匙路径 · 路由语法，`设计/20 §5` 目标）。
# 〔DUP2 · 4D〕11 → 12：新共享 crate `agent-tools-core`（agent 工具词表：哪些工具名算「展开 = 子会话」，monitor 渲染与后端会话事实共用，J19）。
# 〔DUP3 · 4D〕12 → 13（本路增量 ＋1）：新共享 crate `upstream-url-core`（上游 base URL 能不能用，J9）。
run_gate_sum cargo 13 bash -c 'cd src/bridge && cargo test --workspace --lib 2>&1'

# ★★ `K-G3`（09-01）：上面那个合计**还缺一个分母** —— `src/bridge/embedded-backends/` 铺没铺。
#
# `build.rs:376` 只有在 `src/bridge/embedded-backends/` 里两个 arch 的二进制**都在且 build_id 对得上**
# 时才 `println!("cargo:rustc-cfg=embedded_backends")`；那个目录被 `.gitignore` 挡着
# ⇒ **它跟着「铺没铺」走，不跟着 git 走**。挂 `#[cfg(embedded_backends)]` 的那一族全是
# 「本地后端真的能起来吗」：`sftp::embedded_backend_binaries_present_and_valid` ·
# `local_backend_host::the_local_backend_host_can_be_stopped_and_started_again` ·
# `local_backend::the_local_tmux_frames_really_land_in_the_ledger` ·
# `local_backend::the_local_backend_host_really_registers_an_inbound_client`。
#
# 病灶逐字（`ROADMAP.md` 风险行 `5t`，PM 08-25 实测撞上、08-29 复打）：
# **「没有任何东西报出『这一跑少编了几条』」** —— 少编与「都跑了」在终端上一模一样，
# 因为那个合计只会**变小**，而变小没有任何东西认得出来。
#
# ⚠ **这一行只自报家门，不是判据**，理由是判不了：地板得是个常数，而同一份代码
#   铺了与没铺**本来就该是两个数**，钉死任何一个都会把另一种铺法误判成红。
#   真要买成判据得先有一张「铺法 ⇒ 应有条数」的映射，那张表今天盘上没有 ⇒ 交回 PM。
# ⚠ 行首刻意**不是** `ok` —— 它不判任何东西，写成 `ok` 就是把一条诊断伪装成一格绿。
if [ -d src/bridge/embedded-backends ]; then
  printf '  分母 %-14s %s\n' "cargo" "本树铺了 src/bridge/embedded-backends/ ⇒ embedded_backends cfg 会置上，「本地后端真的能起来吗」那一族在跑"
else
  printf '  分母 %-14s %s\n' "cargo" "本树未铺 src/bridge/embedded-backends/ ⇒ embedded_backends cfg 不置 ⇒ 上面那个合计里少了「本地后端真的能起来吗」那一族（4 条，逐个点名见上方注释）"
fi

# ★ 生成物漂移（K-A1 第四轮 `R1`）：**改了 Rust 不跑生成，这里红。**
#
# 形状照 `.github/workflows/ci.yml` 那条「生成物必须最新（C05）」来 —— 它逐字是
# `git diff --exit-code -- ../../src/generated`（那一步在 `src/bridge` 目录下跑，所以带 `../`；
# 本脚本开头已经 `cd` 到仓根，所以不带），失败时印一句 `::error::` 提示「请跑
# npm run gen:types 并把 src/generated/ 一起提交」再 `git diff --stat`。
# ⚠ 那条 CI 步骤的头注还写明了它**排除了什么**：它只买「已提交的生成物 == 从 Rust 源生成的」
# 这一半，另一半「TS 消费方 == 已提交的生成物」由 frontend job 的 `tsc` 买 —— 拆成两半的理由是
# **没有任何 job 同时有 Rust 和 node**（给 Rust job 加 `npm ci` 是分钟级，给 frontend job 加
# 整套 Tauri 编译是 CI 里最贵的东西）。本脚本两样都有，所以这一半在这里只值一条 git 命令。
#
# ⚠⚠ **位置是承重的：它必须排在上面那道 `cargo` 门之后。**
# `ts-rs` 的导出测试就住 `cargo test --lib` 里（`package.json` 的 `gen:types` 逐字就是
# `cd src/bridge && cargo test --lib export_bindings`）⇒ 跑过那道门，`src/generated/**` 已经被
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
#      那一格由 `tests/generated-boundary-guard.vitest.ts` 的目录清单**逐项等号对拍**钉住
#      （它对 `src/generated/` 做 `readdirSync` + 等号比对，新增文件必然让它红一次）。
#   2. 它不判生成物**内容对不对**（该不该 `ts(optional)` 之类）—— 那也是上面那一族的活。
#   3. 它判的是**工作树**，不判「你有没有真把它 commit 上去」（那一维归 `npm run verify:committed`，
#      与本脚本头注里那条分工一致）。
# ⚠ 本格**不走** `run_gate`（判定手写在下面那个 `case` 里）⇒ `gate_wants`/`GATE_RAN`
#   也得手接一次。漏接的形状是：`GATE_ONLY` 点不到它、而它照样跑，
#   于是收据里 `ran ∪ skipped` 少一格 ⇒ `K-G4C` 的两向相等当场分叉（那是**响的**）。
if gate_wants generated; then
git diff --quiet --exit-code -- src/generated/
gen_rc=$?
GATE_RAN+=("generated")
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
fi

# ── `dead_code`：门禁此前**没有这一格**（`K-R115` `KR115D2` 甲，09-14，第 15 格）────────
#
# ## 题面：要量它只能自己开一条路
#
# 门禁跑的是 `cargo test`，而 test 构建里那些函数**有调用方**（测试自己）⇒ 那条 `dead_code`
# 它一辈子看不见。`K-R109`（09-13）要量这一维，只能自己拼一条 `docker run … cargo check`
# —— **因此破了派工单「唯一许可命令」的字面**（PM 已裁「破字面未破实质 · 照登记不抹」）。
# ⇒ `KR115D2` 二选一：收进门禁，或明写「本门禁不看它」。本格选的是**收进来**。
#
# ⚠ **射程如实写**：`-p monitor` 一个包的**非 test** 构建。
#   `src/backend` 那棵树、`src/bridge` 的其余成员、`#[cfg(test)]` 里的死代码，
#   本行**一概盖不到**。
# ⚠ **判法是恒等，不是「不超过某个上限」** —— 这一条承重，理由是死值验逼出来的：
#   本格第一趟落地时写的是「≤ 54」（照抄 `K-R109` 09-13 的读数），而**本趟现打是 41** ⇒
#   造一处 `dead_code` 只会让它变成 42，**离 54 还很远，那一刀不红**。
#   一个宽了 13 的上限，长得和一道门一模一样，而它拦不住本格要拦的那一形。
#   ⇒ 恒等：多了红（有人写了新的死代码），少了也红（好事 —— 回来把这个数改小，
#   并写清降的是哪几条；不写就没人分得开「清理了」与「这一趟根本没编」）。
# ⚠ **「这一趟根本没编」那一形单独有话说**：`cargo` 对**新鲜**单元会重放缓存里的警告，
#   万一哪天它不重放了，「一条都没有」与「没编」在终端上一模一样 —— 恒等把它一起接住了。
# ⚠ 它**不修**任何一条 `dead_code`，只是从此有人在数（`K-R80 §0d` 同一条边界：数出来归数出来）。
#
# 〔量于 09-14，本工作树 `track/k-r115`，沙箱 `ccmon-devbox:latest`〕**41 条**，本格墙钟 **71 秒**
#   （冷 target 的第一趟；这个数就是 `KR115D2` 甲的实测代价，同一趟的门禁基线是 4 分 45 秒）。
# ⚠ `K-R109` 09-13 现打的是 **54** 条 —— **两个数分母不同，别相减**：那一趟的命令是
#   `touch src/history.rs src/lib.rs && cargo check -p monitor`（默认 message-format），
#   本格是 `--message-format=short`、不 touch，而且量于另一个主干尖。
deadcode_t0=$(date +%s)
# 🔴 **2026-09-18：41 → 33，降的 8 条逐条记在这里**（本格自己要求「回来把数改小，**并写清降的是哪几条**」）。
# 起因：用量 ②③ 两轴整轴退役（`设计/50`）⇒ `usage.rs` · `account_usage.rs` ·
# `observe/usage_query.rs` · `agents/codex/usage.rs` 等整删，它们里面那 8 条死代码**随文件一起消失**，
# 不是有人去修的。⇒ 这是「真清掉了」那一支，不是「cargo 没重编」那一支 —— 证据：现打 33 条里
# **一条都不含用量相关符号**（逐条核过）。
# ⚠ `codex_record.rs` 的 `token_usage_last` / `turn_context_model` **仍在这 36 条里**，那是
# 「codex 后面单独做」（用户 2026-09-18 拍板）的**已知代价**，不是删漏 —— 别顺手清掉。
#
# 🔴 **2026-09-20：33 → 36，涨的 3 条逐条记在这里**（本格自己要求「说清为什么留着」）。
# 三条**全部出自 `src/bridge/src/origin.rs`**，是步 12（`origin` 归一）第一刀的**预期状态**：
#   · `src/origin.rs` 的 `LOCAL` 常量 · `Origin` 这个 enum ·
#     `Origin` 的五个方法（`local` / `is_local` / `is_remote` / `host_name` / `as_wire_str`）
# 这一刀刻意**只定类型、不换调用点**（线上形状与今天逐字节相同、零协议变更），
# 所以「类型在、还没人用」是它落地那一拍的**正确**样子，不是漏。
# ⚠ **本格是恒等不是上限，这里有一条刻意的耦合**：等迁移真的开始吃 `Origin`，
#   这三条会自己消失 ⇒ 本格当场红，逼人回来把这个数改小。**那正是要的。**
# 🔴 **2026-09-20（同日第二次）：36 → 34，降的 2 条逐条记在这里。**
# **这不是有人去清的，是那条刻意的耦合按设计开火了。** 上面那段逐字预告过：
# 「等迁移真的开始吃 `Origin`，这三条会自己消失 ⇒ 本格当场红，逼人回来把这个数改小。**那正是要的。**」
# 步 12·C（`origin` 归一第二刀）合掉 5 对双份命令、`Origin::route()` 上线
# ⇒ `LOCAL` 常量与 `Origin` 这个 enum **真的有了生产段调用方** ⇒ 两条出列。
# ⚠ 剩下的第 3 条**仍在**：`Origin` 的五个方法（`local` / `is_local` / `is_remote` /
#   `host_name` / `as_wire_str`）今天只在测试段用 ⇒ 仍算死代码，**不是漏清**。
#   等调用点真的吃这几个方法，本格会再红一次 —— 那一次同样是对的。
# ⚠ 这个数是**现打**的（`cargo check -p monitor --message-format=short | grep -c "never used"`），
#   不是 36−2 算的。
# 🔴 **2026-09-21（步 `24f` 第四刀）：34 → 33，降的 1 条逐条记在这里。**
# **又是那条刻意的耦合按设计开火了，而且这一次上面那段逐字预告过它**：
#   「剩下的第 3 条**仍在**：`Origin` 的五个方法……等调用点真的吃这几个方法，
#    本格会再红一次 —— **那一次同样是对的**。」
# 吃它的是 `src/bridge/src/filewin/find.rs`（原生文件窗口那一侧的搜索）＋
#   `filewin/source.rs::Source::origin`：前者调 `Origin::as_wire_str()`，后者调 `Origin::local()`
#   ⇒ 那一条「associated items 从来没用过」的合并警告整条出列。
# ⚠ **`is_local` / `is_remote` / `host_name` 今天仍然没有生产调用方** —— 现打核过
#   （全树 `.is_local()` / `.host_name()` 只命中 `origin.rs` 自己头注里那一句散文）。
#   那三个方法没有单独再报一条，是 rustc 把同一个 impl 块的未用项**合并成一条**警告的结果
#   ⇒ **别把「警告没了」读成「五个方法都有人用了」。**
# ⚠ 这个数同样是**现打**的（同一条命令，两趟：`touch src/lib.rs` 与
#   `touch src/origin.rs` 各一趟，都是 33），不是 34−1 算的。
# ⚠ 🔴 顺带修一条本格自己的腑坏：这个数原先在下面的内联脚本里**手抄了五遍**
#   （3 处写 33、**2 处还写着更早的 41**）⇒ 终端上印出来的「恒等钉在 41」是假话，
#   而没有任何东西会因此变红。现在它只住 `pin=` 一处。
# 🔴 **2026-09-24（第一波合并 T3 令牌步 3）：33 → 35，涨的 2 条逐条记在这里。**
# 两条**全部出自 `src/bridge/src/bind.rs`**，都是「接得住、还没人收」那一拍的**预期状态**：
#   · `lookup_hwnd_for_token` —— 令牌 → HWND 的查询口。消费点（↗ 按令牌分派）是**步 4**，
#     排在第二波 ⇒ 今天零生产调用方。**步 4 落地那一拍它会自己出列 ⇒ 本格当场红，逼人改回 34。**
#   · `entry_from_marker_hit` —— 从 `#[cfg(windows)]` 函数体里抽出来、为的是 Linux 上也验得了
#     （测试段有调用方）。生产调用方只在 Windows 那支 ⇒ 本格（Linux 非 test 构建）看它是死的。
#     ⚠ 这一条**不会**自己出列；它的代价就是「平台分支抽出来可测」本身，不是漏。
# ⚠ 这个数是**现打**的（合并后主线 `cargo check -p monitor --message-format=short`），不是 33+2 算的；
#   两条的新旧由 `95132442:src/bridge/src/bind.rs` 里这两个函数**零命中**核过。
# 🔴 **2026-09-24（第二波 T4 令牌步 4）：35 → 34，降的 1 条逐条记在这里。**
# **又是那条刻意的耦合按设计开火了，而且上面那段逐字预告过它**：
#   「`lookup_hwnd_for_token` …… **步 4 落地那一拍它会自己出列 ⇒ 本格当场红，逼人改回 34。**」
# 吃它的是 `src/bridge/src/bind.rs::resolve_remote_front`（↗ 远端那一格的唯一分派点，
#   先令牌 `sid → token → HWND`、后标题退路），生产调用链是
#   `lib.rs::bring_remote_terminal_to_front` → `bind::bring_remote_front` → 它 ⇒ 那一条出列。
# ⚠ `entry_from_marker_hit` **仍在这 34 条里**，上面那段也逐字预告过「这一条**不会**自己出列」——
#   它的生产调用方只在 `#[cfg(windows)]` 那支，本格量的是 Linux 非 test 构建。
# ⚠ 这个数是**现打**的（本工作树 `w2/t4`，`cargo check -p monitor --message-format=short | grep -c "never used"`
#   = 34，同一趟 `grep bind.rs` 只剩 `entry_from_marker_hit` / `find_window_by_marker_substr` /
#   `process_creation_filetime` 三条），不是 35−1 算的。
# 🔴 **2026-09-24（第二波大合并那一拍）：34 → 33，现打，逐条记**：
#   R2 让生产改走 `payload.rs` 的 `relay_route_path_in` / `relay_base_url_in`，旧的两口 `relay_route_path` /
#   `relay_base_url` 与两个跨半边样例常量 `RELAY_ROUTE_SAMPLE` / `RELAY_PASSTHROUGH_SAMPLE` 只剩判据在用
#   ⇒ 四样都挂 `#[cfg(test)]`（样例常量是后端 `include_str!` 按源码文本读的，挂属性不影响那一读）。
#   其中 `relay_route_path` 那一条在 33 之前的读数里本来就在（旧的那条 `payload.rs` 警告）⇒ 净 −1。
#   ⚠ 同一拍 T4 让 `bind.rs::lookup_hwnd_for_token` 有了生产调用方（35→34 那一拍已记）。
# 🔴 **2026-09-24（第三波 B2 合并）：33 → 34，现打，逐条记**：B2 加了第四句退出文案 `backend_policy.rs::EXIT_UNREADABLE`
#   （「那台机器上的退出策略读不出来，按默认办」）。它与同文件已在册的 `EXIT_KILLS` / `EXIT_UNATTENDED` / `EXIT_SELF_DIES` /
#   `EXIT_COPY` 同一族：Rust 这一份**只为与 TS 那份逐字对拍而存在**（家在 `src/backend-policy.ts`），非 test 构建里本来就没读者。
#   ⚠ 这一族要不要整族挂 `#[cfg(test)]` 是另一件事（会一次降 7 条），不在合并这一拍做。
# 🔴 **2026-09-24（F7c 收尾，主会话授权动这一个数）：34 → 36，现打，逐条记**：
#   池子那十二条 Tauri 命令删了之后，`sftp_pool.rs` 里**浏览那一半**的通道闸在生产上没人用了 ——
#   `ChannelSet::lease`（只过通道闸、不过车道闸的那一口）与 `Leased::discard`（原先只有 `with_sftp` 的死连重试调它）。
#   **刻意没删**：秤 F4（`sftp_pool_f4_tests`）拿它俩量「传输占满车道时浏览还进得来」那条 `6 − 4 = 2` 的设计；
#   而浏览今天整个走后端 `files-*`，SFTP 上已经没有浏览 ⇒ 车道闸「给浏览留格子」那条前提不在了。
#   它俩连同 `TRANSFER_LANE_CAP` 的去留是一道设计题（要主会话裁），不是本拍顺手删的活。
# 🔴 **2026-09-24（第四波 S4 · SFTP 收尾，主会话授权动这个数）：36 → 34，现打，逐条记**：
#   上一段那两条（`ChannelSet::lease` · `Leased::discard`）**本拍删了** —— 浏览离开 SFTP 之后车道闸没有要保护的东西，
#   闸（`TRANSFER_LANE_CAP`）、浏览用的借法与它俩一起退役（`设计/99` 第三波留给第四波那一条）。
#   同拍删的零流量复制一段（命令 · 核心 · 裸通道借据 · 老 Tauri 进度通道 · 取消登记表）**不带走也不带来**死代码：
#   它们删之前在生产上都有调用方（那条命令），删之后整块不在了。
# ⚠ 这个数是**现打**的（本工作树 `w4/s4`，`cargo check -p monitor --message-format=short | grep -c "never used"` = 34），不是 36−2 算的。
# ⚠ 〔RM1e 09-24〕下面两段原先夹在 `run_gate deadcode … \` 的续行与 `bash -c` 之间 —— 续行接上一行注释 ⇒ 命令在那里就断了、
#    `bash -c` 那一段成了一条游离命令（本格跑的不是它）。挪到 `run_gate` 之上，一字未改。
# 🔴 **2026-09-24（第四波 SR1b 合并）：34 → 35，现打，逐条记**：多的一条是 `dial_host.rs::RemoteFs::home`（`method home is never used`）。
#    生产侧不读它（`open` 里只核「后端答出了起始目录」）；唯一的读者是真 sshd 那条 `#[ignore]` 读数用例
#    （`sftp_tests::sr1b_loopback_deploy_and_transfer_through_the_resident_backend` 断言「起始目录就是 sshd 给的那个」）。
#    ⚠ 没改成 `#[cfg(test)]`：那会把 `src/bridge/src` 的「测试专用支撑项」顶到 16（`structural_scan` 的只许降棘轮，上限 15）——
#    两条纪律冲突时，动**允许说清理由再改的**这一个数，不动只许降的那一个。
# 🔴 **2026-09-24（第四波 RM1e 子步 1）：35 → 34，现打，逐条记**：少的那一条正是上一段那条 `RemoteFs::home` ——
#    推全景小程序字节（`panorama_bytes·rs::push_to`）拿它拼远端落点 `<home>/.cc-monitor/bin/cc-monitor-panorama`，
#    它有了生产读者 ⇒ 出列。同拍 `panorama_bytes` 那两个 `cfg_attr(not(test), allow(dead_code))` 摘了（有了生产调用方），不进这个数。
#    ⚠ 现打：本工作树 `cargo check -p monitor --message-format=short | grep -c "never used"` = 34。
# 🔴 **2026-09-25（第四波 CP2b · 文案全量抽表）：34 → 23，现打，逐条记**：降的 11 条正是 B2 合并那段点名的那一族 ——
#    `backend_policy·rs` 的 `EXIT_KILLS` / `EXIT_UNATTENDED` / `EXIT_SELF_DIES` / `EXIT_UNREADABLE` / `EXIT_COPY` /
#    `HEALTH_UNKNOWN` / `HEALTH_CLEAN` / `HEALTH_CRASHED` / `HEALTH_LAST_MISSING` / `HEALTH_COPY` / `CROSS_LANGUAGE_COPY`。
#    它们「只为与 TS 那份逐字对拍而存在」；文案表立起来之后两侧读同一条表项（`backendPolicy.*`），Rust 副本与逐字对拍一起删了。
#    ⚠ 现打：本工作树 `w4/cp2b` 的 `cargo check -p monitor --message-format=short | grep -c "never used"` = 23，
#    与删之前那份 34 条逐行 diff 只差这 11 行（其余 23 条原样在）。本路不跑 gate，这个数是写区外动的，已报备。
# 🔴 **2026-09-26（PB1 · 90 阶段 B）：23 → 21，现打**：`backend_policy·rs` 的 `describe_health` 与只经它活着的 `Health::seen` 出列 ——
#    前者并进有生产读者的 `health_face`（`backend_status` 调它），后者随之有了生产读者。与基线逐行 diff 只差这 2 行。
# 🔴 **2026-09-26（第四波 4D SH1）：23 → 22，现打，逐条记**：少的那一条是 cc-bus 驾驶舱远端读 `exec_read` 里那句
#    「value assigned to `overflowed` is never read」—— 那个函数随读面改问后端整个删了。同拍新长的三条零生产调用项
#    （monitor `tmux.rs::TMUX_LS_FMT` 双写点 · `spawn_managed` 的 async 出口两项）各带 `cfg_attr(not(test), allow(dead_code))` 与理由，不进这个数。
# 🔴 **2026-09-26（合并 PB1 × SH1）：23 − 2（PB1）− 1（SH1）= 20**。
# 🔴 **2026-09-28（MIG-2 · 起会话进后端）：本路 −0**：起会话搬进后端之后 `apikey_remote.rs` 的发送口（`BUDGET` · `call` · `said`）零调用方，
#    按 V41 整个模块删了（没有抬这个数）。现打本工作树 = 21，多的那一条是 `user_files.rs::delete_empty_dir`，归 MIG-3a（它合后回 20）。
run_gate deadcode '`cargo check -p monitor` 的非 test 构建里 `never used` 的条数（**恒等**钉在 20，理由见上方注释）。射程只有 monitor 一个包的生产段；backend 那棵树与 cfg(test) 里的死代码本行盖不到' \
         bash -c 'pin=20; cd src/bridge && out=$(cargo check -p monitor --message-format=short 2>&1); rc=$?; \
n=$(printf "%s\n" "$out" | grep -c "never used"); \
printf "%s\n" "$out" | tail -5; \
if [ "$rc" -ne 0 ]; then printf "deadcode: cargo check 退出码 %s —— 判不了\n" "$rc"; exit "$rc"; fi; \
if [ "$n" -gt "$pin" ]; then printf "deadcode: never used %s 条，钉的是 %s —— 有人写了新的死代码；修掉它，或者说清为什么留着、再来改这个数\n" "$n" "$pin"; exit 1; fi; \
if [ "$n" -lt "$pin" ]; then printf "deadcode: never used 只数到 %s 条，钉的是 %s —— 要么真清掉了几条（好事：回来把这个数改小，并写清降的是哪几条），要么这一趟 cargo 根本没重编 / 没重放警告。两者在终端上一模一样，所以一律按红记\n" "$n" "$pin"; exit 1; fi; \
printf "deadcode: %s passed（never used %s 条，恒等钉在 %s）\n" "$n" "$n" "$pin"'
# ⚠ 这一行是**读数**不是判定 —— 那一格被 `GATE_ONLY` 挡掉时它会印出「墙钟 0 秒」，
#   而 0 秒与「真的快」在这一行上长得一模一样 ⇒ 挡掉了就别印。
case " ${GATE_SKIPPED[*]-} " in
  *" deadcode "*) : ;;
  *) printf '  分母 %-14s %s\n' "deadcode" "本格墙钟 $(( $(date +%s) - deadcode_t0 )) 秒（现打，与门禁基线相减就是加这一格的代价）" ;;
esac

run_gate backend '单包 src/backend，只有一行 test result ⇒ 最大值 = 合计' \
         bash -c 'cd src/backend && cargo test 2>&1'
# 〔TAIL · 09-26〕全景小程序是独立 crate（自己一份 Cargo.lock，不进任何 workspace）⇒ 上面两格都编不到它；
#   它自己的 `tests/panorama-engine/cli_tests.rs`（含「引擎零写用户文件」）此前不在任何执行链上。
run_gate panorama-engine '单包 src/panorama-engine（独立 crate），只有一行 test result ⇒ 最大值 = 合计' \
         bash -c 'cd src/panorama-engine && cargo test 2>&1'
# ── `tsc`：**发版产物编不编得出来**，此前门禁一格都没有（`K-R118` `KR118D1` ②，09-14，第 16 格）──
#
# ## 题面：一条缺陷 09-12 进来、09-14 才被发现，而发现它的不是任何判据
#
# `tauri build` 的第一步是 `npm run build` ＝ `tsc && vite build`。09-14 `K-R114` 去**真编一次
# 发版产物**，那一步在 `src/views/history.ts` 上红了 6 条 `TS2322` —— 而同一棵树的门禁
# **15 格全绿**（现打，`tests/evidence/K-R118-deathvalue.md#§A` 的 `M0`）。
#
# 🔴 **两条路同时断，这一格补的是第一条**：
#   ① 门禁 `npm` 那一格跑的是 `npm test`（16 个 tsx 套件 + `vitest run`）—— **不含 `tsc`**。
#      `tsx` 与 `vitest` 都是**转译**执行，`esbuild` 只剥类型不做类型检查 ⇒
#      一条纯类型错误在那一格下**一条都不会红**。
#   ② 云端 `.github/workflows/ci.yml` 里那条 `npx tsc --noEmit` **只在 `main` / tag / PR 上跑**，
#      而本分支这一族提交一次都没进过 `origin/main`。
#   ⇒ 这与 `audit-0805` 的 `3w`/`3x`/`3y` 是同一族病：**判据在，执行面没有**（`R73` 第五节）。
#
# ⚠ **射程如实写**：本格只跑 `tsc --noEmit`，也就是 `npm run build` 的**前一半**。
#   `vite build` 那一半（打包 / 产物体积 / 资源解析）、`cargo tauri build` 那一整段
#   （签名 · 打包 · installer），本格**一概盖不到**。
# ⚠ **它不是 `npm` 那一格的超集，也不是子集**：`npm` 买行为（跑起来对不对），
#   本格买类型（编不编得过）。两格都要。
#
# ## 第二条判定：**程序面没被掏空**（这一条是承重的，别删）
#
# `tsc --noEmit` 在一个**空程序**上退出码是 **0** —— 把 `tsconfig.json` 的 `include` 改小 /
# 改错，「一个文件都没检」与「全检过了」在退出码上**一模一样**。
# ⇒ 本格把 `--listFiles` 真读进程序的那批文件数出来，与**盘上现打**的 `src/` ＋ `tests/e2e/` 下
#   `.ts`/`.tsx`/`.mts` 份数对账，**两个数在同一趟里现打**，一个都不写死
#   （写死一个数，加一份文件就红，那种格三天就会被人调宽）。
run_gate tsc '不是「几条断言过了」：这个数是**这一趟真读进 tsc 程序**的仓内 `.ts`/`.tsx`/`.mts` 份数（`tsconfig.json` 的 include 现打是 `[\"src\", \"tests\"]`），并与盘上现打的份数**恒等对账**。🔴 〔订正 09-19〕**本行原先两侧都只数 `src` ＋ `tests/e2e`（210 份），而 tsc 真读进去的是 372 份** —— 两侧同时把 `tests/` 的其余 **162** 份剔掉，于是等式照样成立、本格照样绿。⚠ **那不是少印一个数，是一个静默洞**：有人把 `include` 收窄成 `[\"src\", \"tests/e2e\"]`，那 162 份当场不再被检，而 `want` 与 `got` 会一起掉到 210 ⇒ **仍然相等、仍然全绿**。本拍把两侧都改成按 `include` 的真值数（372 == 372），这条路才堵上。⚠ 只判类型（`npm run build` 的前一半）；`vite build` 与 `cargo tauri build` 那两段、以及仓根那几份不在 include 里的 `.ts`（`vite.config.ts` / `vitest.config.ts`），本行一概盖不到' \
         bash -c 'out=$(node_modules/.bin/tsc --noEmit --listFiles 2>&1); rc=$?; \
want=$(find src tests -type f \( -name "*.ts" -o -name "*.tsx" -o -name "*.mts" \) | wc -l | tr -d " "); \
got=$(printf "%s\n" "$out" | grep -v "/node_modules/" | grep -cE "/(src|tests)/.*\.(ts|tsx|mts)$"); \
printf "tsc: 盘上现打 %s 份仓内 .ts，这一趟真读进程序的 %s 份\n" "$want" "$got"; \
printf "%s\n" "$out" | grep -E "error TS" | head -60; \
if [ "$rc" -ne 0 ]; then printf "tsc: 退出码 %s —— 类型没编过。它就是 npm run build 的第一步，红着这棵树发不出产物\n" "$rc"; exit "$rc"; fi; \
if [ "$got" -ne "$want" ]; then printf "tsc: 真读进程序的 %s 份 != 盘上现打的 %s 份 —— tsconfig 的 include 被掏空或收窄了。空程序上 tsc 退出码也是 0，「一个文件都没检」与「全检过了」在退出码上一模一样，所以一律按红记\n" "$got" "$want"; exit 1; fi; \
printf "tsc: %s passed（仓内 %s 份 .ts 全部过 tsc --noEmit；两个数同一趟现打）\n" "$got" "$want"'

run_gate npm '17 个套件（16 tsx + 1 vitest）里只有 2 个打得出数字（test:dom 1480 · test:diff 17），而取最大值 ⇒ 这个数恒是 test:dom 的；另 15 个 tsx 套件只打「all X tests passed」，它们「跑了 0 个」这一格守不住（失败仍由 && 链的退出码守）' \
         npm test

# ── 门⑥ `ccm` e2e（`K-G3` 09-01，治 `丙1-f1`）────────────────────────────────
#
# ★★ 它买的是什么：`shared/ccm` 是 1258 行的 bash 启动器，`K-C1` 为它写了 54 条断言，
#    而在本行落地之前 `grep -c ccm scripts/gate.sh` = **0** ⇒ 出货那一刀**一条都不看**。
#    头注那句「出货前的**唯一闸门**」与这个 0 对不上，`丙1-f1` 就是这笔账。
#
# ★ **判法不自造，复用 `tests/e2e/assert-pass-floor.sh`** —— CI 的 26 条 e2e 步骤用的就是它，
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
#     尺子留在 `tests/evidence/K-G8-stability-diff.py`（可复算）。
#
#   ⚠ **`exact` 是第三个参数、fail-closed**：拼错 ⇒ `exit 2`，**不回落 `at-least`**。
#     回落等于把「拼错了」静默降级成旧行为 —— 那正是这道闸要治的那一族。
run_e2e() {
  # 第三个参数可选：`exact`（缺省）或 `exact-with-skip`（〔E2 尾 09-27〕按 PASS+SKIP 恒等判，给按环境显式分支的套件）。
  local suite="$1"; local floor="$2"; local mode="${3:-exact}"
  # ⚠ 两个名字**刻意不同**：`GATE_ONLY` 里写套件短名（`ccm-cli`），而收据与
  #   `found_cells()` 认的**规范名**带前缀（`ccm tests/e2e/ccm-cli`）。
  #   规范名里有空格 ⇒ 它当不了空格分隔的 `GATE_ONLY` 记号，所以两侧各用一个，
  #   归一那一跳写在 `K-G4C-gate-receipt.py` 与 `K-R80` 的 `C5d`/`C8` 里（同一份取法）。
  gate_wants "$suite" || return 0
  local out rc n
  out="$(bash tests/e2e/assert-pass-floor.sh "$suite" "$floor" "$mode" 2>&1)"
  rc=$?
  GATE_RAN+=("ccm tests/e2e/$suite")
  n="$(printf '%s' "$out" | grep -oE '合计 PASS=[0-9]+' | grep -oE '[0-9]+' | tail -1)"
  if [ "$rc" -ne 0 ]; then
    fails+=("ccm tests/e2e/$suite（退出码 $rc；实得 PASS=${n:-<抓不到>}，地板 $floor，判法 $mode。\
诊断原文见上方本套件自己的输出）")
    # K-R22：原来这里是裸 `tail -20`。换成共用的 `gate_diag` 有两处不是排版：
    #   ① 套件自己的 `::error::` 诊断行**可能落在尾部 20 行之外**（`assert-pass-floor.sh`
    #      先 `cat` 整份输出再打诊断，一套失败几十条时那行就被顶出去了）—— 模式表把它捞回来；
    #   ② 加了 `  | ` 前缀，套件输出里的 `PASS=`/`FAIL=` 行不再混进本脚本的裁决面。
    gate_diag "tests/e2e/$suite" "$out"
  elif [ -z "$n" ]; then
    fails+=("ccm tests/e2e/$suite（退出码 0 但抓不到「合计 PASS=<n>」—— 门没跑与门跑了结果是空\
在终端上一模一样，判不了，不许当成绿）")
    # K-R22 D1 #9：这一支原来一个字都不印，而「门没跑」与「门跑了结果是空」
    # **恰恰只能靠那段输出分开** —— 不印等于把这条判据自己那句话作废。
    gate_diag "tests/e2e/$suite" "$out"
  else
    printf '  ok   %-14s %-22s PASS=%s（地板 %s，判法 %s）\n' "ccm e2e" "$suite" "$n" "$floor" "$mode"
  fi
}

# ── 探针⑩（`N-G2`）：`run_e2e` 的**退出码**那条判定还在不在判 ──────────────────
#
# 🔴 **为什么它不住在上面那个 `gate_selftest` 里**：bash 要函数**先定义后调用**，而
#   `gate_selftest` 的调用点在本文件靠前处、`run_e2e` 到这里才定义。两条别的路都更贵 ——
#   把 `run_e2e` 的定义整块前移、或把 `gate_selftest` 的调用点后移，都是为了一条探针去动
#   本文件的结构。⇒ **让这一条住在它盯的那道门旁边**，`gate_assert_judged` 与取法仍是同一份。
#
# 只有它拦得住：给一个**非数字的地板** ⇒ `tests/e2e/assert-pass-floor.sh` 在**参数校验**那一步
#   `exit 2`，并把那个参数**原样回显**；参数里带着 `合计 PASS=7` ⇒ `run_e2e` 抓得到读数
#   ⇒ 「抓不到「合计 PASS=」」那条**已被满足**，掏掉退出码那条就一路落到绿行。
# ⚠ 那一步排在 `npm run` 与 `mktemp` **之前** ⇒ 不跑 npm、不落文件（见上面自检段头注）。
gate_selftest_e2e() {
  local GATE_PROBE=1   # 同 `gate_selftest`：探针不受 `GATE_ONLY` 影响
  local probe
  probe="$(run_e2e 自检⑩ '合计 PASS=7 NG2-PROBE-J' 2>&1)"
  gate_assert_judged 自检⑩ "$probe" NG2-PROBE-J "run_e2e 的「退出码非零 ⇒ 红」"
}
gate_selftest_e2e

# ★★ 🔴 `K-R48` 第二拍（09-11）：**这四套的被测对象换成了后端二进制，所以先把它 build 出来。**
#
# 〔用@09-11 `K33`〕逐字「后端**只有一个**…**不要有什么 bash 脚本**，**不要有什么单独的 ccm**」
# ⇒ `shared/ccm` 删了，四套 e2e 的 `$CCM` 指向 `$CARGO_TARGET_DIR/debug/cc-monitor-backend`。
#
# 🔴 **为什么要单独 build，不能指望上面 `backend` 那格顺手带出来**：现打实测过 ——
#   `cargo test`（那一格跑的就是它）**只编 `src/main.rs` 的 test 版**
#   （`deps/cc_monitor_backend-<hash>`），**不产 `debug/cc-monitor-backend`**。
#   不加这一步的话，四套 e2e 会在 fail-closed 那道 `[ -x ]` 上一起红，
#   而诊断说的是「先 cargo build」——对，但那件事该由门禁自己做。
# ⚠ 它**不进判定面**：build 失败时下面四格会各自红并说清原因（fail-closed），
#   这里再加一层判定只会让同一件事报两遍。
gate_e2e_wanted() {
  if [ -z "$GATE_ONLY" ]; then return 0; fi
  local suite
  for suite in ccm-print-parity ccm-rbind-title ccm-cli ccm-contract-parity backend-rbind-token rbind-token-endtoend backend-cc-bus backend-gate2 local-backend restart-frames restart; do
    case " $GATE_ONLY " in *" $suite "*) return 0 ;; esac
  done
  return 1
}
if gate_e2e_wanted; then
printf '  ·    %-14s %s\n' "e2e 前置" "build 后端二进制（下面那几套 e2e 的被测对象）"
( cd src/backend && cargo build --bin cc-monitor-backend >/dev/null 2>&1 ) || true
else
printf '  ·    %-14s %s\n' "e2e 前置" "跳过（GATE_ONLY 一套 e2e 都没点 ⇒ 不白编那一趟 cargo build）"
fi

run_e2e ccm-print-parity 12
run_e2e ccm-rbind-title  8
# ── `K-G7`（09-03）新挂的两套 ─────────────────────────────────────────────────
# 地板量于 `b8a6ecd`、镜像 `644ea0ce5c3d`，连打两趟同值（126 / 68，各 0 FAIL 0 SKIP）；
# 与 `ci.yml` 的调用行同值 —— 详见本文件头注那一节，改地板要三处一起改。
# ⚠ 这两套硬依赖 `jq` 且 fail-closed：镜像里没有 `jq` ⇒ 这两格红，不会静默跳过。
# ★★ **`K-P2` `D` 阶段（09-03）：`ccm-cli` 棘 `126` → `203`**（`ccm-contract-parity` 本拍没动）。
#   那 77 条分两拍买的：`D1` 的 `JSONENC` 47 条（真 JSON 编码器，与 `jq -Rs .` 逐字节对拍）
#   ＋ `D2` 的 `WIRE` 30 条（真发请求那一半：落盘式假后端，「发了」与「发对了」分两族判）。
#   量于 `65b2792`、同一份 devbox 镜像，`PASS=203 FAIL=0`。
#   🔴 **顺带如实登记这一格是怎么被逮到的**：`D1` 交回时实得 **173**、地板还停在 **126**，
#   而 `tests/e2e/assert-pass-floor.sh:56` 是 `n -lt FLOOR` ⇒ **只挡缩水、不挡「涨了不跟」**
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
#       （`rc=4` / 「`CCM_NO_BACKEND=1` 不是逃生口」/「空表是合法答案」各成一格）·
#       身份前置检查那族补了「不给 `--base` ⇒ 账号那条腿先报」的反向对照 ·
#       `WIRE/夹具自检④` 拆成「没后端 ⇒ 没有兜底」＋「后端在但答不出 `--resolve` ⇒ 静默退路仍在」。
#     · `ccm-contract-parity` +4：`A″`/`A′e`/`A′h` 三处的反向对照从「关掉后端」换成
#       「换一份后端」（同一条代码路径、只有输入不同 —— provenance 正是后者），
#       并各补一格「`CCM_NO_BACKEND=1` ⇒ rc=4」。
#     · `ccm-cli` 再 +3：`控制字符` 那一族拆成两半 —— 「换行照发」（修回来的那个能力：
#       载荷里的 `\n`/`\t` 是**键**，`create-or-attach` 放行；`ESC`/`CR`/`NUL` 照旧挡）
#       ＋「线上那份请求里那个换行是**转义**过去的」。
#   量于 `ccmon-devbox:latest`，`PASS=264 FAIL=0` / `PASS=72 FAIL=0`。
#   ⚠ **`ci.yml` 那两行不在本件写区** ⇒ 逐字 diff 交回 PM 落（头注那条「三处一起改」的纪律照旧）。
# ★★ 🔴 **`K-R48` 第二拍（09-11）：`ccm-cli` 264 → 46 · `ccm-contract-parity` 72 → 39。**
#   **这是本门第一次往下拧地板，所以理由要比往上棘时写得更细。**
#
# 用户 09-11 逐字：「那就把这个门禁删了，**bash 脚本直接删**」。PM 第一拍的对拍读数
#（`tests/evidence/K-R48-native-vs-bash-parity.py`，SAME=27 / DIFF=2）买到的结论是
#「那四套多半不必重写，只要把 `$CCM` 指向二进制」⇒ 本拍**没有删套件**，是
#**逐条判了那 356 条断言**（`tests/evidence/K-R48-356-verdicts.tsv`）再把没有指称对象的删掉。
#
# 四格实测（本拍现打，沙箱 `ccmon-devbox:latest`）：
#   · `ccm-print-parity`   12 → **12**（0 删；断言一个字没改，只把 PATH 上那个 `ccm` 换成软链）
#   · `ccm-rbind-title`     8 → **8**（0 删；格式串取值点从 `sed shared/ccm` 换成 `sed` 那个 Rust `const`）
#   · `ccm-cli`           264 → **46**（删 218）
#   · `ccm-contract-parity` 72 → **39**（删 33）
#
# **删掉的那 251 条按族**（逐条判词在 TSV 第 6 列）：
#   · `JSONENC` 47 —— 手写的 bash JSON 编码器与 `jq -Rs .` 对拍。Rust 侧是 `serde_json`。
#   · `WIRE` 85 —— 「发了 / 发对了 / 不可达 / 撞名」。同一个进程之下没有「上线字节」这回事；
#     其中「那几件事一件都不许丢」已落成 Rust 判据
#     `the_container_launch_goes_through_the_one_door_with_every_field_intact`。
#   · 账号解析走后端一节 69 + 身份后端前置检查 15 —— 「找不到后端」这个概念没了。
#   · `A″`/`A′d`/`A′e`/`A′f`/`A′h` 31 —— 全是「ccm 去问另一个进程」这件事的形状。
#   · `A′g` 2 —— **搬进了 Rust**（`a_command_from_the_backend_is_never_rewritten_by_the_shell`）。
#
# 🔴 **两族是本拍实测推翻第一拍判词的**（第一拍判「搬得过去」，指过去之后发现没有指称对象）：
#   ccm-cli 第 82–83（`--print` 不受身份前置检查影响 ＋ **非空对照断的正是已删的 rc=2**）·
#   第 281–284（`--print` 纯性，第 281 数「假后端被调几次」⇒ 原生实现恒 0，**空真**）。
#
# ⚠ **新增判据落在别处，不在这四格里**：backend 那格 676 → 677（容器路三条转发那条）。
# 〔2026-09-24 第二波 MC1〕46 → 48：新增两格「设了 CCM_SELF 也不被读」＋ 正控（后端只认 self_invocation）。
# 〔09-26 AL3 · V138 / V142–V145〕48 → 53：`--cwd ../x` 补绝对 · `new` 保留 · 位置词 `attach` 交 claude · `--ccm-tmux` / `--ccm-agent` 让名 · 启动器拆词各一格（现打 53）。
run_e2e ccm-cli               58
# 🔴 〔`K-R61` 09-11〕39 → **42**：C 组加了三格（`capabilities=` 声明
#    `base-url-across-tmux` · 容器载荷真带 `export ANTHROPIC_BASE_URL=` · 反空真）。
#    判法是 `exact` ⇒ 这个数不改，涨了照样红。**同一拍要改三处**（本行 + `ci.yml` 的
#    调用行 + 那个 job 里的清单副本），三处都不在 `K-R61` 写区，已点名交回 PM。
# 🔴 〔`K-R70` 09-12〕42 → **45**：C 组再加三格，问的是**一份真编出来的二进制**
#    「你是哪一次构建」——① 抽取器自检（从后端源码抠得到 `BUILD_ID`）·
#    ② `--ccm-probe` 的 `build=` 行 == 那个 `BUILD_ID` · ③ `build=` 与 `version=` 不同值
#    （后者是 CLI 契约版本，答不出身份）。**它是本件唯一跑真二进制的判据**，
#    Rust 侧那几条跑的是测试壳。⚠ `ci.yml` 那两处（调用行 + 清单副本）**不在本件写区**，
#    逐字 diff 已交回 PM（头注那条「三处一起改」的纪律照旧）。
run_e2e ccm-contract-parity   45

# ── 令牌那两套（第二波 T4，2026-09-24）：**只被 shellcheck、不被执行**的那一格接进执行链 ──
#
# 🔴 题面：`设计/80 §8.7` 步 2 立了 `tests/e2e/backend-rbind-token.sh`（真后端报不报得出令牌），
#   步 3 立了 `tests/e2e/rbind-token-endtoend.sh`（生产载荷字节 → 真 bash → 进程环境 → 真后端 → wire）。
#   两套落地那天都**只进了 `ci.yml` 的 shellcheck 人群**，没有任何一条执行链跑它们 ——
#   `ci.yml` 两段注释逐字「也没有加 `assert-pass-floor` 那一行 …… **待拍板**」。
#   ⇒ 步 4（↗ 改走令牌 join）一落地，「令牌真的活到 wire 上」这一格就是它的前提，
#   而那个前提**没人在验**。判据不在执行链上就等于不存在。
#
# ★ 形状与上面四格**逐字同一条**：`run_e2e`（`assert-pass-floor.sh … exact`）⇒
#   PASS 数**恒等**（多了少了都红）＋ 抓不到「合计 PASS=」红 ＋ 退出码非零红。
#   ⚠ 格名前缀沿用 `ccm`：`K-R48` 之后 `ccm` 就是 `cc-monitor-backend` 这个二进制
#   （`argv[0]` 叫 `ccm` 就进一次性模式），而这两套的被测对象**正是这个二进制** ⇒ 名实相符，
#   不为它另开一个前缀（另开 ⇒ `found_cells()` / `K-G4C` 收据那几条正则都要跟着分叉）。
#
# ★ **反空真锚**不是这个数，是每套自带的「量具自检」格：`session_added` 真的到了
#   （`added_count == 1`）才判帧上有没有令牌；阴性组（不跑载荷前缀 / 没索要 / 形状不对）
#   与正题组**同一形态**，只差被测的那一个变量 ⇒ 「帧根本没到」读不成「令牌不在」。
#   端到端那套还有一道取值自检：从金标准里抽不到**恰好一条**带令牌的载荷就当场 `exit 1`
#   （先于「合计」行 ⇒ 这里红在「退出码」那一支，不是「抓不到数」那一支）。
#
# 〔量于 2026-09-24，本工作树 `w2/t4`，本机非沙箱〕`backend-rbind-token` **11 PASS / 0 FAIL** ·
#   `rbind-token-endtoend` **9 PASS / 0 FAIL**，两套连打两趟同值。
#   ⚠ **不在 `ci.yml` 的计数地板里** —— 那一行是 T3/步 2 报备「待拍板」的另一件事，本拍不替它拍；
#   `tests/bridge/e2e_gate_registry_tests.rs` 的 `EXEMPT` 为此各登记了一条（理由写在那里）。
# ⚠ 它**买不到**什么：两套都不经 ssh、不经 Windows、不开窗 ⇒ 「↗ 真的把那个窗口拉到前台」
#   这一维仍是零格（`设计/80 §10.4` / §11 同一句）。
run_e2e backend-rbind-token   11
run_e2e rbind-token-endtoend   9
# 〔TAIL · 09-26〕`backend-cc-bus`：DUP2 把拒码改成 `bad_id` 之后它红了 3 条、一整天没人看见 ——
#   它只挂在 `ci.yml` 那条不通电的流水线上。接进本机执行链（isolated tmux socket ＋ jq，fail-closed）。
#   〔量于 2026-09-26，本工作树 `w4/tail`〕**96 PASS / 0 FAIL**。
run_e2e backend-cc-bus        96
# 〔E2 尾 · 09-27〕同一族的另外四套：只挂在 `ci.yml` 那条不通电的流水线上（本仓不推送），于是各红了几天没人看见 ——
#   `backend-gate2` 的 `meta_equals` 判据过时（DUP3 §5 ⑦ 按设计放行 `=`，它仍期望 `invalid_args`）·
#   `local-backend` 两趟过滤串重叠、常驻那族跑两遍（且第一个宿主起的后端没人收，靠收尾 `pkill -f` 兜着）·
#   `restart-frames` / `restart` 的驱动器载不了 `.css`、shim 还在说已经退役的 Tauri 命令（真源早改走通道）。
#   修完接进本机执行链。〔量于 2026-09-27，本工作树 `w4/e2`，本机 tmux 3.6〕35 · 15 · 5 · 24 PASS / 0 FAIL，约 100 秒。
#   ★ `backend-gate2` 按 **PASS+SKIP** 恒等判（`exact-with-skip`）：`meta_dollar` 那一格要 tmux ≥3.5，版本不够的机器上
#     它记 SKIP 并说原因（套件里的版本门 `min_tmux_for`；版本不够却建得出来 ⇒ FAIL「版本门过时」）⇒ 3.6 上 35+0、
#     3.4 上 34+1，这一格两边都是 35。只钉 PASS 的话就是把开发机的 tmux 烤进了判据。
run_e2e backend-gate2         34 exact-with-skip
#   ★ 〔DEL 续〕`local-backend` 同形按 **PASS+SKIP** 恒等判：三条起真后端的判据由 `cfg(embedded_backends)` 门着，
#     没铺 `src/bridge/embedded-backends/` 的树上它们记 SKIP 并说原因（套件里那一段；落点齐了却不跑 ⇒ FAIL）⇒ 铺了 26+0、没铺 15+11。
run_e2e local-backend         26 exact-with-skip
run_e2e restart-frames         5
run_e2e restart               24

# ── 〔第四波 S4〕这里原先是第 26 格 `f3-copy`（秤 F3 两向：零流量复制的包计数对拍，三方对拍 ＋ 两向锚点）。
#   它量的那条池子命令与核心随浏览 / 复制离开 SFTP 一起退役（窗口的复制走后端 `files-copy`），
#   判据本体那份台架文件一起删了 ⇒ 本格退役，29 格 → 28 格。它的形状（三方对拍 ＋ 写死的 pin）
#   仍是下面 `comm-boundary` 那一格的取法，说明留在那一格里。

# ── `comm-boundary`：通信层那十七条独立成格（`13b` 步 1 落地，本拍第 27 格）──────
#
# 🔴 **题面与当年那一格 `f3-copy`（〔第四波 S4〕已退役）同形，但这一格更要紧 —— 因为那一族的人群可能是空集。**
#   `tests/bridge/comm_boundary_registry_tests.rs`（1400 行）是 `设计/05 §2`/`§3.3.6`
#   那 C1–C5 ＋ X1–X6 的本体。人群为空时，绿的理由是 `0 == 0` 而不是「扫不到」。
#   ⚠ **本文件不写人群有几份** —— 见 `〔自述·现物〕` 那一段的理由。
#   ⇒ 整个模块被从 `lib.rs` 摘掉时，**十七条一条都不跑，连那条元判据也不跑**，
#     而 `cargo` 那一格只会合计小一点 —— **「摘掉了」与「全绿」在终端上一模一样**。
#   🔴 一个人群为空的判据族，如果连「它自己还在不在」都没人看，那它买到的是零。
#   立本格的那一路自己停下报备了这件事（写区不含本文件），这一拍补上。
#
# ★ 取法照当年那一格 `f3-copy`（〔第四波 S4〕已退役）：**三方对拍**（本行 `pin=17` · 那份文件现打的 `#[test]` 条数 ·
#   `cargo test` 真跑出的 passed），三个数必须相等。
# ★ 两个逐字锚点选的是**最承重的两条**，不是随便挑两个名字：
#   ① `every_criterion_is_on_the_execution_chain` —— 元判据本身（判据清单 ↔ 真实 `#[test]`
#      两向集合相等）。它没跑，等于这一族的自述没人核。
#   ② `the_boundary_registry_says_out_loud_how_big_it_is_today` —— 「说得出今天是空的」那条。
#      它没跑，「人群为空」就退回成「扫不到」。
#   ⚠ 两侧不同源：名字写死在本文件里，命中数来自 `cargo` 的运行时输出。
# ⚠ 本格**不买**「没盖标记的文件不是通信层」—— 那是那张表自己最大的诚实边界
#   （一份真在做传输的代码不盖标记，它一个字都看不见）。本格只买「这十七条没有静默消失」。
#
# ★ 〔2026-09-21〕**这一格的三方对拍逐腿砍过**（`pin` 从 15 抬到 16 那一拍，
#   还原一律 `cp` 覆盖 ＋ `sha256` 对账，**不搬 mtime**）：
#   · **`pin` 那条腿** —— 把本行的 `pin=16` 改回 `15` ⇒ 当场红，逐字
#     「三方对拍分叉 —— 本行钉 15 · … 声明 16 条 · cargo 真跑 16 条」；
#   · **「真跑」那条腿** —— 给那份文件里任一条判据加 `#[ignore]` ⇒ 当场红，逐字
#     「本行钉 16 · … 声明 16 条 · cargo 真跑 15 条」。
#   ⇒ 三个数里**改动任何一个**都会分叉，不存在「两边一起改掉」的安静路径
#     （`pin` 在本文件、`declared` 在那份判据文件、`ran` 来自 cargo 的运行时输出，**三侧异源**）。
#   ⚠ 「声明」那条腿与「真跑」那条腿会被**同一次编辑**一起改掉（真删一条判据 ⇒ 两个都变 15），
#     而那时 `pin` 还是 16 ⇒ 照样红。**本格的反空真锚是那个三方相等，不是任何单独一条腿。**
#
# ★ 〔2026-09-22〕`pin` **从 16 抬到 17**（`设计/99 §4 P16`「步 4 的剩余」立第十七条：
#   面 A 的传输面候选逐份两向集合相等）。同一拍逐腿再砍过一遍，读数与上面那次同形：
#   · 只加判据**不**抬 `pin` ⇒ 当场红，逐字「本行钉 16 · … 声明 17 条 · cargo 真跑 17 条」；
#   · 只抬 `pin` 而判据没加 ⇒ 当场红（反方向）。
#   🔴 **为什么抬它不是放宽**：这个 `pin` 是一条**恒等**腿（`pin == declared == ran`），
#     **不是一个上限**。加一条真判据时把它同拍抬上去，买到的是**让那条恒等继续成立**；
#     而「把上限调上去让今天好过」是方向相反的另一件事 —— 那一种会让分母悄悄变小，
#     这一种让分母变大而三侧仍异源。⇒ 判据与 `pin` **必须同拍改，别分两次**；
#     单独抬 `pin` 或单独加判据，两个方向都当场红。
run_gate comm-boundary '判过的条数 = 通信层那一族（C1–C5 ＋ X1–X6 ＋ 锚 ＋ 余下五份 ＋ 传输面四份 ＋ 元判据）这一趟真跑过的条数。**三方对拍**：本行钉的 17 · 那份文件里现打的 `#[test]` 条数 · `cargo test` 真跑出来的 passed，三个数必须**相等** ＋ 两条逐字锚点（元判据 · 「说得出今天是空的」那条）各命中**恰好 1 次**。⚠ **本格存在的唯一理由是那一族的人群可能是空集** —— 模块被摘掉时十七条与元判据一起消失，而 `cargo` 那一格只会合计小一点，「摘掉了」与「全绿」在终端上分不开。⚠ 本格买的是「这十七条没有静默消失」，**不买**它们判得对（那由它们各自的头注与死值验负责），更**不买**「没盖标记的文件不是通信层」（那张表自己写死的最大边界）；⚠ 这 17 条**同时**算在 `cargo` 那一格的合计里 —— 两格都在，档位不叠加' \
         bash -c 'pin=17; f=tests/bridge/comm_boundary_registry_tests.rs; \
meta=every_criterion_is_on_the_execution_chain; \
empty=the_boundary_registry_says_out_loud_how_big_it_is_today; \
[ -r "$f" ] || { printf "comm-boundary: 那一族的判据本体 %s 盘上读不到 —— 住址改了就回来改本格，不许静默跳过\n" "$f"; exit 1; }; \
declared=$(grep -cE "^[[:space:]]*#\[(tokio::)?test\]" "$f"); \
out=$(cd src/bridge && cargo test -p monitor --lib comm_boundary_registry::tests:: 2>&1); rc=$?; \
if [ "$rc" -ne 0 ]; then printf "%s\n" "$out" | tail -25; printf "comm-boundary: cargo test 退出码 %s —— 判不了\n" "$rc"; exit "$rc"; fi; \
ran=$(printf "%s\n" "$out" | grep -oE "^test result: ok\. [0-9]+ passed" | grep -oE "[0-9]+" | head -1); \
if [ -z "$ran" ]; then printf "%s\n" "$out" | tail -25; printf "comm-boundary: 那趟输出里抠不出「test result: ok. N passed」—— 读法与 cargo 的输出面对不上，本格判不了（不许当成绿）\n"; exit 1; fi; \
if [ "$declared" -ne "$pin" ] || [ "$ran" -ne "$pin" ]; then printf "comm-boundary: 三方对拍分叉 —— 本行钉 %s · %s 里现打声明 %s 条 · cargo 真跑 %s 条；三个数必须相等。真加/删了一条判据，就回来改本行那个 pin（别去动另外两边）\n" "$pin" "$f" "$declared" "$ran"; exit 1; fi; \
for t in "$meta" "$empty"; do n=$(printf "%s\n" "$out" | grep -c "comm_boundary_registry::tests::$t \.\.\. ok"); if [ "$n" -ne 1 ]; then printf "comm-boundary: 锚点 %s 在这趟跑过的名单里命中 %s 次（应当恰好 1 次）—— 它没跑，或者它改了名\n" "$t" "$n"; exit 1; fi; done; \
printf "comm-boundary: %s passed（C1–C5 ＋ X1–X6 ＋ 锚 ＋ 余下五份 ＋ 传输面四份 ＋ 元判据；三方对拍 pin %s == 声明 %s == 真跑 %s）\n" "$ran" "$pin" "$declared" "$ran"'

# ── `test-tiers`（TQ1 09-24，第 29 格）：测试层分级那一族**还在不在** ─────────────────────
#
# 同 `comm-boundary` 那一形，理由也同：那一族挂在 `guard-core` 的 lib 上
# （`src/bridge/crates/guard-core/src/lib.rs` 末尾那一行 `mod test_tiers;`），
# 那一行被摘掉时十二条一起消失，而 `cargo` 那一格只会合计小十二 ——「摘掉了」与「全绿」在终端上分不开。
# 而这一族守的正是「别的判据有没有静默变空」⇒ 它自己静默变空是最贵的那一种。
# 三方对拍：`pin`（写死在本行）· 那份文件里现打的 `#[test]` 条数 · `cargo` 真跑出来的 passed，三侧异源；
# 两条锚点：分区那条（五层登记表的人群闸）· 真机层那条（`#[ignore]` 触发链）各在跑过的名单里恰好 1 次。
# ⚠ 本格**不买**它们判得对（各条头注与 `调研/第四波记录/TQ1.md` 的死值验负责）。
# ⚠ `benches/` 那条判据只判「秤有家、`cargo test` 会跑它的冒烟档」；**墙钟一个都不进本门禁**。
# ★ 逐腿死值验（TQ1 09-24，还原一律 `shutil.copyfile` ＋ `touch` ＋ sha256 对账）：
#   · `pin` 那条腿：`pin=12` → `11` ⇒ 当场红（三方对拍分叉）；
#   · 「挂载」那条腿：`guard-core` 的 `lib.rs` 里 `mod test_tiers;` 那一行改成别的模块名 ⇒
#     `cargo` 编得过、这一族零条跑 ⇒ 抠不出 passed ／ 真跑 0 ≠ pin ⇒ 红。
run_gate test-tiers '判过的条数 = 测试层分级那一族（分区 ＋ 单元 / 扫描 / 集成 / e2e / 真机五层各一条反空真自检 ＋ `benches/` 登记 ＋ 五条合成夹具自检）这一趟真跑过的条数。**三方对拍**：本行钉的 12 · 那份文件里现打的 `#[test]` 条数 · `cargo test -p guard-core --lib test_tiers::` 真跑出来的 passed，三个数必须**相等** ＋ 两条逐字锚点（分区 · 真机层）各命中**恰好 1 次**。⚠ **本格存在的唯一理由**：那一族挂在 `guard-core` 的 lib 上，那一行 `mod` 被摘掉时十二条一起消失，而 `cargo` 那一格只会合计小一点。⚠ 本格买的是「测试层分级没有静默消失」，**不买**任何一层的判据判得对；⚠ 这 12 条**同时**算在 `cargo` 那一格的合计里 —— 两格都在，档位不叠加' \
         bash -c 'pin=12; f=tests/bridge/crates/guard-core/test_tiers_tests.rs; \
part=the_tiers_partition_the_test_files_on_disk; \
real=real_machine_tier_every_ignored_test_is_registered_and_its_trigger_still_reaches_it; \
[ -r "$f" ] || { printf "test-tiers: 那一族的判据本体 %s 盘上读不到 —— 住址改了就回来改本格，不许静默跳过\n" "$f"; exit 1; }; \
declared=$(grep -cE "^[[:space:]]*#\[(tokio::)?test\]" "$f"); \
out=$(cd src/bridge && cargo test -p guard-core --lib test_tiers:: 2>&1); rc=$?; \
if [ "$rc" -ne 0 ]; then printf "%s\n" "$out" | tail -40; printf "test-tiers: cargo test 退出码 %s —— 判不了\n" "$rc"; exit "$rc"; fi; \
ran=$(printf "%s\n" "$out" | grep -oE "^test result: ok\. [0-9]+ passed" | grep -oE "[0-9]+" | head -1); \
if [ -z "$ran" ]; then printf "%s\n" "$out" | tail -25; printf "test-tiers: 那趟输出里抠不出「test result: ok. N passed」—— 读法与 cargo 的输出面对不上，本格判不了（不许当成绿）\n"; exit 1; fi; \
if [ "$declared" -ne "$pin" ] || [ "$ran" -ne "$pin" ]; then printf "test-tiers: 三方对拍分叉 —— 本行钉 %s · %s 里现打声明 %s 条 · cargo 真跑 %s 条；三个数必须相等。真加/删了一条判据，就回来改本行那个 pin（别去动另外两边）\n" "$pin" "$f" "$declared" "$ran"; exit 1; fi; \
for t in "$part" "$real"; do n=$(printf "%s\n" "$out" | grep -c "test_tiers::$t \.\.\. ok"); if [ "$n" -ne 1 ]; then printf "test-tiers: 锚点 %s 在这趟跑过的名单里命中 %s 次（应当恰好 1 次）—— 它没跑，或者它改了名\n" "$t" "$n"; exit 1; fi; done; \
printf "test-tiers: %s passed（分区 ＋ 五层自检 ＋ benches/ ＋ 合成夹具；三方对拍 pin %s == 声明 %s == 真跑 %s）\n" "$ran" "$pin" "$declared" "$ran"'

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
# 〔墓碑 2026-09-18 —— `pb check` 那一格整格退役，用户拍板「不管他，把他删了」〕
#
# 原来这里是一道「查 planned-build 工作区」的门：没给 `PB_WS` 就红，理由逐字是
# 「这道门查哪个计划工作区必须由调用方指定；不许回落默认值：硬写一个名字正是 `K-R10`
# 治的那个 bug」。那条**拒绝猜**的纪律本身没错，今天不成立的是它的**对象**：
#
#   ① 本仓**没有** `.claude/planned-build/` —— 今天的设计与排期走的是仓外的 `调研/` 那一族文档；
#   ② 它调的 `~/.claude-accts/z/skills/planned-build/bin/pb.py` **今天不在盘上**
#      ⇒ 就算给了 `PB_WS`，这道门也跑不起来。
#
# ⇒ 它不是「红」，是**没有可判的对象**。而让一道门在没有对象时自己闭嘴（跳过/回落）
#   正是本仓反复记账的那种病 ⇒ 不加「没目录就跳过」的口子，**整格删掉**。
# ⚠ 若哪天本仓真用起 planned-build，复活它要连同 `tests/evidence/N-G2-verdict-md5.py`
#   那份登记一起回来（那份尺子把本格登记成一个「行内格」）。

# ── 〔裁决·射程〕`GATE: OK` 那一行**不对什么负责**（`K-R122` `KR122D2` 乙，09-14）──────
#
# ## 题面：那一行今天**不带射程**，而它不等于「CI 会绿」
#
# `K-R119`（09-14）：同一棵树上本门禁 **16 格全绿**，推 tag 那一趟云端 **8 个 job 里 5 个红**
#（读数住 `tests/evidence/K-R119-发版读数.md § 六`）。四条差异里三条落在门禁自己**逐格印出来**的
# 射程之外 —— 读数在那儿，而**没有人把它读成「所以这三件事没人管」**；
# 第四条更直接：`shellcheck` 当时 16 格里**一格都没有**。
# ⇒ 本件甲那一半已经把其中两条收成了格（`shellcheck` · `winchk-backend`）。
#   **剩下的这几条今天仍然买不到，所以要在裁决那一刻逐字说出来。**
#
# ## 形状：`键|说明`，而**键是有牙的那一半**
#
# 说明是给人读的散文；**键**（`|` 左边那个小写标识）进机检：
# `tests/evidence/K-R80-gate-cell-coverage.py` 的 `C5c` 对拍三件事 ——
#   ① 这张表非空、每一项形状对、键不重复；
#   ② **键集合与现打的判定格名互不相交** —— 哪天有人把某一维收成了格而这里还自称「不看」，
#      当场红（那正是「买到了却还在说不看」那一形，和 `C5b` 治的腐同源）；
#   ③ 印出来的条数是**现算**的（`${#GATE_BLIND[@]}`），不许写死一个数。
#
# ⚠ **它买不到什么**：这张表是**黑名单**，列不全 —— 它保证的是「**列出来的这几条不会悄悄
#   变成一句没人守的散文**」，不是「射程之外只有这几条」。
# ⚠ 只在 `GATE: OK` 那一支印。`GATE: FAIL` 那一支本来就没有在声称什么，那里再印一遍只会
#   把真正要看的诊断顶下去。
GATE_BLIND=(
  "windows-runner|Windows runner 上才犯的那一族 —— 本门禁的 npm / tsc / e2e 全跑在 Linux 上，路径分隔符恒是 /。K-R119 那趟云端 vitest 的唯一一条红（1 failed / 1725 passed）就是这一形，本机在构造上红不了"
  "ci-job-shape|.github/workflows/*.yml 里那些 job 自己的形状 —— 装了哪条工具链、runner 是谁、缓存与 needs 怎么连、每一步的 if 条件。⚠ 这一条已经被收窄过三次，每次只割走一个切片：ci-e2e-prereq 判 ci.yml 里 e2e 步骤的 build 前置齐不齐（K-R122）；release-gate 判 release.yml 的触发器、env.PUBLISH 字面、两处发布步骤与 CI 门那一步的 if、以及两处发布步骤的正文来源（K-R124）；〔19b 09-19〕release-gate 又割走**产字节那条路**——release.yml 里「跑 cargo build/zigbuild」的步骤 ↔ 条 63 承诺的三格（两向）、出现的 target triple（两向）、三个 job 的 runs-on 逐字、每一处抠 const BUILD_ID 的住址实打指得到真东西、setup-zig 与 install-action 那两个版本。**其余全部仍然没人看** —— 包括 ci.yml 那 8 个 job 的 runner/工具链/needs/缓存，和 release.yml 里除上面点名那几处以外的每一步（打包 · 校验和 · 上传清单 · artifact 传递）。⚠ 而且那几个切片买的都只是「盘上这份文本满足这几条」——**云端那一趟会不会绿，见下面 did-ci-actually-run 那一条**"
  "msvc-abi|MSVC ABI 专属的那一类跨平台编译问题 —— 两格 Windows 交叉检查用的都是 -gnu（沙箱里没有 zig，ring 的 build script 缺 lib.exe）。只在 msvc 上才犯的毛病本门禁盖不到"
  "did-ci-actually-run|云端那条流水线到底跑没跑、绿没绿 —— 本门禁断网跑（--network none），它一次 gh run view 都做不到。GATE: OK 说的是这棵树在本机这几格上的样子，不是它在云端的样子。⚠〔09-20 收窄一刀，不摘〕本文件此刻已经**被调用方接住了**：.github/workflows/ci.yml 的 local-gate 那个 job 与 tests/hooks/pre-push 都在门禁之后跑 tests/evidence/K-G4C-gate-receipt.py（「这一趟到底跑没跑门禁」从此有判据）。**但那两条路今天一趟都没在云端真跑过** —— 本仓红线是不推送，而 ci.yml 的触发器只有 push(main/v*) 与 pull_request ⇒ 那个 job 在 GitHub runner 上从未起过；pre-push 同理（没有 push 就没有 pre-push）。⇒ 盘上那几份文本满足那几条判据，**不等于**云端那一趟会绿，这一条因此不摘。⚠ 本条刻意不写第二个格数：那个数是**写死在裁决行那一句里的字面量**，由 K-R80 的 C5（数）· C5b（自述节三方对拍）· C5d（裁决行那串点名与盘上两向集合相等）三处钉着。上一版这里逐字声称那个数是门禁自己算出来的 —— **那是假话**（裁决行是字面量，C5 那条正则正是靠它是字面量才钉得住），而说明栏 C5c 逐字声明「一个字都不判」⇒ 那句假话没人守，腐着。C5d 第二半从此把它焊住"
)
gate_print_blind() {
  printf 'GATE: 射程 —— 上面那行只对它自己那几格负责；下面这 %s 件事**本门禁不看**：\n' "${#GATE_BLIND[@]}"
  local item
  for item in "${GATE_BLIND[@]}"; do
    printf '  不看  %-20s %s\n' "${item%%|*}" "${item#*|}"
  done
}

echo
# ── 〔被谁调用〕拼错的 `GATE_ONLY` 是**红**，不是「少跑一格」 ───────────────────
gate_check_only
# ── 〔被谁调用〕落收据。**三种裁词都落** —— 判它的是 `tests/evidence/K-G4C-gate-receipt.py` ──
# ⚠ 裁词**先算再落**：`OK` 只在「一格没红 **且** 一格没跳」时给。
#   少了后半个条件，`GATE_ONLY` 一设就能拿到一行 `GATE: OK` —— 那正是本段要焊死的那一形。
if [ "${#fails[@]}" -ne 0 ]; then
  gate_verdict=FAIL
elif [ "${#GATE_SKIPPED[@]}" -ne 0 ]; then
  gate_verdict=PARTIAL
else
  gate_verdict=OK
fi
gate_write_receipt "$gate_verdict"

if [ "${#fails[@]}" -eq 0 ] && [ "${#GATE_SKIPPED[@]}" -ne 0 ]; then
  # 🔴 **这一支刻意不印 `GATE: OK`。** 跑过的那几格全绿是真的，而「全绿」与「全都跑过」
  #   是两句话 —— 把它们印成同一行，就是本仓反复治的那一形（少跑与跑过在终端上一模一样）。
  printf 'GATE: PARTIAL —— 跑过的那 %s 格全绿，但 GATE_ONLY 挡掉了 %s 格：%s\n' \
    "${#GATE_RAN[@]}" "${#GATE_SKIPPED[@]}" "${GATE_SKIPPED[*]}"
  echo "**这不是 GATE: OK，不许拿它出货。** 出货要的是不带 GATE_ONLY 的那一趟。"
  gate_print_blind
  exit 0
fi
if [ "${#fails[@]}" -eq 0 ]; then
  # 🔴 `K-R80`（09-12）：**这一行原来逐字是「三道门 + 生成物漂移 + pb check + 四套 ccm e2e」
  #   —— 那是 09-10 加 `fmt`/`winchk` 之前的点名，盘上现打 11 格时它只点得出 9 格。**
  #   本拍加了第 12 格（`fmt-backend`），顺手把它订正到今天，并且**不让它再自己烂下去**：
  #   下面这个 `12` 与「本文件里到底有几格判定」由 `tests/evidence/K-R80-gate-cell-coverage.py`
  #   三方对拍（本行的数 · 本文件真有的判定格 · 那份覆盖登记的条数），对不上就红。
  #   ⚠ 那把尺子**不在本脚本里跑** —— 它是登记的机检，不是出货闸的一格。
  # 🔴 `K-R82`（09-12）：**12 → 13**，加的是 `hooks` 那一格（上面 `gate_selftest` 之后那一段）。
  #   这一行的数与点名跟着改了 —— 而**不是靠人记得改**：`C5` 那条三方对拍会当场逮到。
  # 🔴 `K-R122`（09-14）：**16 → 19**，加了三格 —— `shellcheck`（第 17 格）·
  #   `ci-e2e-prereq`（第 18 格）· `winchk-backend`（第 19 格）。三格都是
  #   「CI 那边有人看、本门禁一个字都看不见」的那一维（`KR122D2` 甲）。
  # 🔴 `K-R128`（09-15）：**20 → 21**，加的是 `installface`（第 21 格）——
  #   第三块（`S1`–`S5`）开工前，把 `K-R117` 自陈的那个洞（收工判据没有闸）
  #   与**纪律 A**（改了命令名没人响）两条都接上闸。棘轮，所以第一天就是绿的。
  # 🔴 `K-R124`（09-15）：**19 → 20**，加的是 `release-gate`（第 20 格）。它治的不是
  #   「CI 有、本地没有」，是**更坏的一档**：CI 里那一步从加进去那天起就不可能过，
  #   而它在本地跑不起来（判据本体用了沙箱里没有的 PyYAML）⇒ **两头都看不见**。
  # 🔴 〔订正 09-19〕**21 → 20，摘掉 `pb check`。** 那一格 09-18 已按用户拍板**整格删除**
  #   （`PB_WS` / `planned-build` 在本文件非注释处现打零命中），而这一行与上面自述段
  #   **两处都还在点它的名**，于是门禁整整一天印着一个比实跑多一格的数。
  #   ⚠ **这正是 `C5`/`C5b` 该抓而没抓到的那条** —— 它没瞎，是**没人跑它**：
  #   `K-R80` 不在本脚本的执行链上，且它默认找的是重构前的 `scripts/gate.sh`（现打直接
  #   `FileNotFoundError`）。**两头坏叠在一起 ⇒ 假账在裁决行上挂了一天。**
  #   ⇒ 本拍把它接成真的一格（见下面 `run_gate gate-selfdesc`），不再靠人记得手跑。
    # 🔴 第二波 T4（09-24）：**27 → 29**，加的是令牌那两套 e2e（`backend-rbind-token` ·
  #   `rbind-token-endtoend`，见上面 `run_e2e` 那一段）—— 它们此前只被 shellcheck、不被执行。
  # 〔TAIL · 09-26〕**30 → 31**，加的是 `backend-cc-bus`（见上面 `run_e2e` 那一段）；
  #   **31 → 32**，加的是 `panorama-engine`（全景小程序自己的测试，见 `backend` 那一格下面）。
  echo "GATE: OK —— 36 格全绿（worktree-clean · hooks · copy2 · shellcheck · ci-e2e-prereq · release-gate · gate-selfdesc · ccbus-twophase · platform · installface · fmt · fmt-backend · winchk · winchk-backend · winlink · muslbuild · cargo · comm-boundary · test-tiers · deadcode · generated · backend · panorama-engine · tsc · npm · ccm-print-parity · ccm-rbind-title · ccm-cli · ccm-contract-parity · backend-rbind-token · rbind-token-endtoend · backend-cc-bus · backend-gate2 · local-backend · restart-frames · restart），可以出货"
  gate_print_blind
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
