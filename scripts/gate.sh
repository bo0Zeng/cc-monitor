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
# ⚠ 覆盖面如实写：它跑的是**工作树**的三道门 + 一道生成物漂移检查。
# · 跨平台 / 提交状态那一维归 `npm run verify:committed`（`C16`，动 daemon 时跑）；
# · 真机 e2e 归各自的套件（本脚本不跑它们 —— 它们要 tmux/Xvfb，几分钟起步）。
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2
fails=()

run_gate() {
  local name="$1"; shift
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
    printf '  ok   %-14s %s passed\n' "$name" "$n"
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

run_gate daemon  bash -c 'cd remote-daemon-proto && cargo test 2>&1'
run_gate npm     npm test

# pb check 不打「passed」，单独判：它自己会打 `FAIL=<n> BROKEN=<n>`。
pb_out="$(python3 "$HOME/.claude-accts/z/skills/planned-build/bin/pb.py" check \
          ../.claude/planned-build/control-parity 2>&1 | tail -1)"
case "$pb_out" in
  *"FAIL=0 BROKEN=0"*) printf '  ok   %-14s %s\n' "pb check" "$pb_out" ;;
  *) fails+=("pb check（$pb_out）") ;;
esac

echo
if [ "${#fails[@]}" -eq 0 ]; then
  echo "GATE: OK —— 三道门 + 生成物漂移 + pb check 全绿，可以出货"
  exit 0
fi
printf 'GATE: FAIL —— %s\n' "$(IFS='；'; echo "${fails[*]}")"
echo "**别提交**。先修，再重跑本脚本。"
exit 1
