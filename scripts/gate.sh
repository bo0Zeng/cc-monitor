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
# ⚠ 覆盖面如实写：它跑的是**工作树**的三道门 + 一道生成物漂移检查 + `pb check` + 两套 `ccm` e2e。
# · 跨平台 / 提交状态那一维归 `npm run verify:committed`（`C16`，动 daemon 时跑）；
#
# ★★ `K-G3`（09-01）：**「真机 e2e 本脚本不跑它们」这句话已经作废，但只作废了 2/6。**
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
# ⚠ **为什么只挂两套，另外四套的确切拦路石**（如实写，别读成「它们太慢」）：
#   · `ccm-cli` / `ccm-acceptance` / `ccm-contract-parity` / `ccm-pretrust` 都硬依赖 `jq`，
#     而**沙箱镜像 `ccmon-devbox:latest` 里没有 `jq`**（现打：`command -v jq` ⇒ MISSING）。
#     四套都是 fail-closed 的（自己打「需要 jq」再 exit 1），所以它们**不会假绿**，
#     但今天挂上去就是四条恒红 ⇒ 不挂。
#   · 在一份**只多装了 `jq`** 的探针镜像上现打过：`ccm-cli` 126 PASS/0 FAIL、
#     `ccm-contract-parity` 68 PASS/0 FAIL（这两套加 `jq` 就能挂，合计 +12.8 秒）；
#     而 `ccm-acceptance` 28/1、`ccm-pretrust` 14/1 —— **沙箱里各红 1 条**，
#     那是另一笔账（`.claude/devbox/gate` 头注自己写着「容器里 `HOME` 几乎是空的」）。
#   · `.claude/devbox/Dockerfile` 不在 `K-G3` 的写区 ⇒ 加 `jq` 这一步交回 PM 裁。
#
# ⚠ **诚实边界：这两套买不到 `K-C1` 那 54 条。** `K-C1` 的账号解析判据住在
#   `ccm-cli` / `ccm-contract-parity`（要 `jq` 的那两套）里。本行落地之后，
#   「`shared/ccm` 的行为面进了出货门禁」这句话**只对 20 条断言成立**（12 + 8），
#   不对那 54 条成立。别把这一格读大。
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2
fails=()

# ★★ `K-G3`（09-01）第二个参数 `denom` 是**这个数的分母**，跟着绿行一起印出来。
#
# 它治的是题面里的**第 5 个洞**：`sort -rn | head -1` 取的是**所有 `N passed` 里的最大值**，
# 而 `npm test` 是 **17** 个套件（1 个 vitest `test:dom` + **16** 个 `tsx`）用 `&&` 串起来的，
# 那 16 个 tsx 套件打的是 `all branching tests passed` 这种**不带数字**的形状
# ⇒ 这个 `grep -oE` 在它们的输出里**零命中** ⇒ **`n` 恒等于 `test:dom` 那一个数**。
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
  elif [ -z "$n" ] || [ "$n" -eq 0 ]; then
    fails+=("$name（读数是 ${n:-<找不到>} —— 0 passed 不是绿）")
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
  elif [ "$pkgs" -ne "$want_pkgs" ]; then
    # ⚠ 这一支是**采集面自检**，不是测试失败：包数对不上 ⇒ 下面那个合计不算数。
    fails+=("$name（只跑到 $pkgs 个包，应当 $want_pkgs —— 有包静默掉出了 --workspace；\
合计变小与「有测试没跑」在终端上一模一样，只有这条认得出来。真加/删了 crate 就来改这个数）")
  elif [ -z "$n" ] || [ "$n" -eq 0 ]; then
    fails+=("$name（读数是 ${n:-<找不到>} —— 0 passed 不是绿）")
  else
    printf '  ok   %-14s %s passed（%s 个包合计）\n' "$name" "$n" "$pkgs"
  fi
}

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
run_gate npm '17 个套件里只有 test:dom（vitest）打得出数字，这个数只是它一个；另 16 个 tsx 套件的失败由 && 链的退出码守，「跑了 0 个」则守不住' \
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
run_e2e() {
  local suite="$1"; local floor="$2"
  local out rc n
  out="$(bash e2e/assert-pass-floor.sh "$suite" "$floor" 2>&1)"
  rc=$?
  n="$(printf '%s' "$out" | grep -oE '合计 PASS=[0-9]+' | grep -oE '[0-9]+' | tail -1)"
  if [ "$rc" -ne 0 ]; then
    fails+=("ccm e2e/$suite（退出码 $rc；实得 PASS=${n:-<抓不到>}，地板 $floor。\
诊断原文见上方本套件自己的输出）")
    printf '%s\n' "$out" | tail -20
  elif [ -z "$n" ]; then
    fails+=("ccm e2e/$suite（退出码 0 但抓不到「合计 PASS=<n>」—— 门没跑与门跑了结果是空\
在终端上一模一样，判不了，不许当成绿）")
  else
    printf '  ok   %-14s %-22s PASS=%s（地板 %s）\n' "ccm e2e" "$suite" "$n" "$floor"
  fi
}
run_e2e ccm-print-parity 12
run_e2e ccm-rbind-title  8

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
  pb_out="$(python3 "$HOME/.claude-accts/z/skills/planned-build/bin/pb.py" check \
            "../.claude/planned-build/$PB_WS" 2>&1 | tail -1)"
  case "$pb_out" in
    *"FAIL=0 BROKEN=0"*) printf '  ok   %-14s [%s] %s\n' "pb check" "$PB_WS" "$pb_out" ;;
    *) fails+=("pb check[$PB_WS]（$pb_out）") ;;
  esac
fi

echo
if [ "${#fails[@]}" -eq 0 ]; then
  echo "GATE: OK —— 三道门 + 生成物漂移 + pb check + 两套 ccm e2e 全绿，可以出货"
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
