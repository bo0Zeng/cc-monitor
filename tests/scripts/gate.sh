#!/usr/bin/env bash
# 出货前的唯一闸门：各格跑一遍，末尾只吐一行裁决（`GATE: OK` / `GATE: PARTIAL` / `GATE: FAIL …`）。
# 用法：先跑它、看见 `GATE: OK`，再单独提交。它故意不提交任何东西 —— 读数与动作不焊在同一条命令里。
# 跑哪几格以行首的 `run_gate` / `run_gate_sum` / `run_e2e` 与 `if gate_wants <名>` 为准；格数与名单从这一趟现算，
# 收据 `.build/gate-receipt.json` 记着真跑过哪几格、每格几毫秒。CI 的 Linux job 调本脚本（`GATE_ONLY=<格>`）；
# 留在 Windows job 里的几步由 `coverage` · `audit` 两格从 `ci.yml` 按步骤名现取原样跑。
# Windows 那一维由 `winchk` · `winchk-backend` · `winlink` 交叉编译盖；真 Windows 上的行为判不了（见 `GATE_BLIND`）。
# 格分进几条「道」（`gate_lane`）：道内一格接一格，道与道同时跑；跑完按本文件里的顺序逐格判、逐格打印。
# ── 起跑之前：整趟换进一层挂载命名空间，`/tmp/tmux-<uid>` 换成这一趟自己的目录 ─────────────
# 后端与不少测试裸调 `tmux`（不带 `-L` / `-S`），会落到缺省 server —— 开发机上那是用户正在用的那台。
# 用挂载而不靠环境变量：这一趟每个进程（含各格的无网沙箱、`env-sandbox` 的内层门禁）看到的都是同一个私有目录。
# 私有目录里先放一台假的缺省 server（一个会话 ＋ 几格已知钩子，`-v` 记下每个客户端），`tmux-default` 那一格判它没人碰。
# CI（`GITHUB_ACTIONS=true`）上不换：runner 上没有用户的 tmux。
gate_tmux_q() { env -i PATH="$PATH" tmux -S "$1" "${@:2}"; }
# 假 server 的快照：pid ＋ 全局钩子 ＋ 会话 ＋ 那个会话自己的选项与钩子（一个客户端连一次）。
gate_tmux_snap() {
  gate_tmux_q "$1" display -p '#{pid}' \; show-hooks -g \; list-sessions -F '#{session_name}' \
    \; show-options -t gate-fake \; show-hooks -t gate-fake 2>&1
}
if [ -z "${GATE_TMUX_HOME:-}" ] && [ "${GITHUB_ACTIONS:-}" != true ]; then
  command -v bwrap >/dev/null 2>&1 || { echo "GATE: FAIL —— 这台机器上没有 bwrap（装 bubblewrap）：换不了 tmux 目录，不许裸跑（测试会落到用户默认的 tmux 上）"; exit 2; }
  gate_tmux_uid="$(id -u)"
  gate_tmux_home="$(mktemp -d "${TMPDIR:-/tmp}/gate-tmux.XXXXXX")" || exit 2
  mkdir -p "$gate_tmux_home.log"
  # 挂载点要先在：宿主上还没有 `/tmp/tmux-<uid>` 时照 tmux 自己的样子建（0700），别让 bwrap 建出一个 tmux 不认的。
  [ -d "/tmp/tmux-$gate_tmux_uid" ] || mkdir -m 700 "/tmp/tmux-$gate_tmux_uid"
  ( cd "$gate_tmux_home.log" && gate_tmux_q "$gate_tmux_home/default" -v -f /dev/null \
      new-session -d -s gate-fake 'sleep 2147483647' \
      \; set-hook -g 'session-created[3]' 'run-shell -b true' \
      \; set-hook -g 'session-closed[3]' 'run-shell -b true' \
      \; set-hook -g 'session-renamed[3]' 'run-shell -b true' \
      \; set-hook -g 'session-closed[60]' "run-shell -b \"'$gate_tmux_home.log/none/ccm' -- --tmux-notify 1 1\"" ) \
    || { echo "GATE: FAIL —— 起不来假的默认 tmux server"; rm -rf -- "$gate_tmux_home" "$gate_tmux_home.log"; exit 2; }
  gate_tmux_snap "$gate_tmux_home/default" > "$gate_tmux_home.log/base"
  gate_tmux_pid="$(head -1 "$gate_tmux_home.log/base")"
  # 挂载那一层由一个占位进程撑着，门禁本体用 nsenter 进去跑：直接当 bwrap 的子进程跑的话，AppArmor
  #（`unpriv_bwrap`）不许它再建命名空间，各格的无网沙箱就起不来了。
  bwrap --dev-bind / / --bind "$gate_tmux_home" "/tmp/tmux-$gate_tmux_uid" --die-with-parent -- \
    sh -c 'echo $$ > "$1"; exec sleep infinity' _ "$gate_tmux_home.log/hold" </dev/null >/dev/null 2>&1 &
  for _ in $(seq 1 50); do [ -s "$gate_tmux_home.log/hold" ] && break; sleep 0.1; done
  gate_tmux_hold="$(cat "$gate_tmux_home.log/hold" 2>/dev/null)"
  trap 'kill ${gate_tmux_hold:+"$gate_tmux_hold"} "$gate_tmux_pid" 2>/dev/null; rm -rf -- "$gate_tmux_home" "$gate_tmux_home.log"' EXIT
  [ -n "$gate_tmux_hold" ] || { echo "GATE: FAIL —— 撑挂载那一层的占位进程起不来（bwrap）"; exit 2; }
  nsenter -t "$gate_tmux_hold" -U -m --preserve-credentials -- env GATE_TMUX_HOME="$gate_tmux_home" \
    bash -c 'cd -- "$1" && shift && exec bash "$@"' _ "$PWD" "$0" "$@"
  exit $?
fi
if [ -n "${GATE_TMUX_HOME:-}" ] && [ "$(stat -c %d:%i "/tmp/tmux-$(id -u)/default" 2>/dev/null)" != "$(stat -c %d:%i "$GATE_TMUX_HOME/default" 2>/dev/null)" ]; then
  echo "GATE: FAIL —— GATE_TMUX_HOME=$GATE_TMUX_HOME 在，而 /tmp/tmux-$(id -u)/default 不是它那台假 server：tmux 目录没换成，不跑"
  exit 2
fi

# ── 换进挂载之后第一件事：无条件摘掉从开发机会话继承来的环境变量（按前缀，不按名单）─────────────────
# `CCM_*` · `CLAUDE*` · `ANTHROPIC_*` · `TMUX*` · `CC_BUS_*` 是外面那个会话的状态（指真 `~/.cc-monitor`、真端口），
# 测试一继承就会动到真东西；按前缀摘，新长的同族变量也跟着摘。要它们的测试自己设（e2e 各套另有 `tests/e2e/sandbox-env.sh`）。
# 代价：`CCM_PWSH` 这类手测开关也一起摘了（那几条本来不在门禁里）。只印名字，不印值（里面有令牌）。
gate_scrubbed=()
for gate_v in $(compgen -e); do
  case "$gate_v" in
    CCM_*|CLAUDE*|ANTHROPIC_*|TMUX*|CC_BUS_*) gate_scrubbed+=("$gate_v"); unset "$gate_v" ;;
  esac
done
unset gate_v
# 摘完之后带上「这是一次沙箱跑」的标记（`relay_route_core::SANDBOX_ENV`）：门禁里起的常驻后端若要占本账号真家目录里的门牌就拒绝起。
export CCM_SANDBOX=1
set -uo pipefail

# 仓根 = 本脚本的上两级（`tests/scripts/gate.sh` ⇒ `../..`）。
cd "$(dirname "$0")/../.." || exit 2
fails=()
printf '  ·    %-14s %s\n' "环境" "摘掉了 ${#gate_scrubbed[@]} 个从开发机会话继承来的变量：${gate_scrubbed[*]:-（一个都没有）}"

# ── 无网沙箱：会起后端 / ccm / tmux 的格，测试进程跑在新网络命名空间里（`bwrap --unshare-net`）─────
# 开发机回环上住着用户真在跑的服务（中转口、常驻后端的监听口），测试退到默认口就会连上它们。
# GitHub Actions 上不包（runner 一次性）；其余一律包，bwrap 不在或起不来 ⇒ 要包的格全红并说原因，不退回裸跑。
# 包的格：每一套 e2e · `cargo` · `backend`。`env-sandbox` 由它的内层门禁去包；`weak-net` 在 docker 里自己造网。
GATE_NONET=(bwrap --dev-bind / / --proc /proc --unshare-net --die-with-parent --)
GATE_NONET_WHY=""
if [ "${GITHUB_ACTIONS:-}" = true ]; then
  GATE_NONET=()
  printf '  ·    %-14s %s\n' "无网沙箱" "不包（GITHUB_ACTIONS=true：CI runner 上没有用户的服务）"
elif ! command -v bwrap >/dev/null 2>&1; then
  GATE_NONET_WHY="这台机器上没有 bwrap（装 bubblewrap）"
elif ! gate_v="$("${GATE_NONET[@]}" true 2>&1)"; then
  GATE_NONET_WHY="bwrap 起不来新网络命名空间：${gate_v}"
fi
unset gate_v
if [ -n "$GATE_NONET_WHY" ]; then
  printf '  ·    %-14s %s\n' "无网沙箱" "用不了 —— ${GATE_NONET_WHY}；要包的格一律判红"
elif [ "${#GATE_NONET[@]}" -gt 0 ]; then
  printf '  ·    %-14s %s\n' "无网沙箱" "e2e · cargo · backend 跑在新网络命名空间里（bwrap --unshare-net），连不到这台机器上的任何口"
fi
if [ -n "${GATE_TMUX_HOME:-}" ]; then
  printf '  ·    %-14s %s\n' "tmux 目录" "整趟看到的 /tmp/tmux-$(id -u) 是这一趟自己的（$GATE_TMUX_HOME），缺省 server 是一台假的"
else
  printf '  ·    %-14s %s\n' "tmux 目录" "不换（GITHUB_ACTIONS=true：runner 上没有用户的 tmux）"
fi
# 在无网沙箱里跑一条命令；沙箱用不了时不跑，印原因、退非零。
gate_nonet() {
  if [ -n "$GATE_NONET_WHY" ]; then
    printf '无网沙箱用不了 —— %s。这一格会起后端 / ccm / tmux，本机不许裸跑（会连上开发机上真在跑的服务）\n' "$GATE_NONET_WHY"
    return 125
  fi
  ${GATE_NONET[@]+"${GATE_NONET[@]}"} "$@"
}
# cargo 的测试格：先在沙箱外把测试编出来（缺依赖时下载要网），再进沙箱跑。`$1` 是 cargo 工程目录，其余原样给 `cargo test`。
gate_cargo_test_nonet() {
  local dir="$1" out rc
  shift
  [ -z "$GATE_NONET_WHY" ] || { gate_nonet; return; }
  out="$(cd "$dir" && gate_yield cargo test --no-run "$@" 2>&1)"; rc=$?   # 编那一半不按墙钟判 ⇒ 让路
  if [ "$rc" -ne 0 ]; then printf '%s\n' "$out"; return "$rc"; fi
  gate_nonet bash -c 'cd "$1" && shift && cargo test "$@" 2>&1' _ "$dir" "$@"
}
# e2e 那几套：无网沙箱里再换一个这一套自己的 `/tmp`（几套写死了 `/tmp/e2e-remote` 之类的目录，同时跑会互相踩），
# 这一趟的 tmux 目录绑回 `/tmp/tmux-<uid>`（私有 socket 照旧落在那里，裸调 `tmux` 的照旧连到假的缺省 server）。
# 没换 tmux 目录（CI）时退回 `gate_nonet`：那时也不并跑。
gate_nonet_e2e() {
  local t rc
  if [ -n "$GATE_NONET_WHY" ] || [ "${#GATE_NONET[@]}" -eq 0 ] || [ -z "${GATE_TMUX_HOME:-}" ]; then
    gate_nonet "$@"; return
  fi
  t="$(mktemp -d "$GATE_CELL_DIR/tmp.XXXXXX")" || return 1
  "${GATE_NONET[@]:0:${#GATE_NONET[@]}-1}" --bind "$t" /tmp --bind "$GATE_TMUX_HOME" "/tmp/tmux-$(id -u)" -- "$@"
  rc=$?
  rm -rf -- "$t"
  return "$rc"
}

# ── `GATE_ONLY` 子集 ＋ 一张跑过的收据 ─────────────────────────────────────
# 收据（`GATE_RECEIPT`，默认 `.build/gate-receipt.json`）只记真跑过这一趟才拿得到的东西：本文件的 sha256 ·
#   tree oid 与脏不脏 · 真判过的格（`GATE_RAN`）· 被挡掉的格（`GATE_SKIPPED`）· 每格与整趟的墙钟。
#   判它的是 `tests/evidence/K-G4C-gate-receipt.py`，由调用方（`tests/hooks/pre-push`）在门禁之后跑 —— 本脚本判自己的收据是同源恒真。
#   它的反空真锚：`ran ∪ skipped` 与本文件现打的格名两向相等。
# `GATE_ONLY`：空格分隔的格名（e2e 用套件短名，或组名 `e2e`），只跑那几格。
#   ① 名字不是盘上的格 ⇒ 红；② 跳过的格逐个印出；③ 跳过了一格裁决就是 `GATE: PARTIAL` —— `GATE: OK` 只表示每格都跑过；
#   ④ 自检探针不受它影响（`gate_selftest*` 里 `local GATE_PROBE=1`，动态作用域）。探针跑在 `$( )` 里，进不了那两张表。
GATE_ONLY="${GATE_ONLY:-}"
GATE_RECEIPT="${GATE_RECEIPT:-.build/gate-receipt.json}"
GATE_DECLARED=()   # 盘上声明过的格（短名，现算）
GATE_GROUPS=()     # 盘上声明过的组名（`GATE_ONLY` 里写组名 = 点名组里每一格；今天只有 `e2e`）
GATE_RAN=()        # 这一趟**命令真的执行过并被判过**的格（规范名，现算）
GATE_SKIPPED=()    # 这一趟被 GATE_ONLY 挡掉的格（短名，现算）
declare -A GATE_MS=()   # 这一趟每格的墙钟（毫秒，规范名 → 数），进收据
GATE_WIN=()        # 这一趟每格的起止（`<起 ms> <止 ms> <规范名>`）：`tmux-default` 拿它说外来连接出在哪一格
GATE_PREP_MS=0          # e2e 前置那一趟 cargo build 的墙钟（不是格，单记）
GATE_T0=""              # 整趟起点，下面定义完 `gate_now_ms` 就取

# 现在的毫秒数（`EPOCHREALTIME` 是微秒精度；去掉小数点那一位，与 locale 的小数点写法无关）。
gate_now_ms() { local t="${EPOCHREALTIME//[!0-9]/}"; printf '%s' "$(( t / 1000 ))"; }
gate_fmt_ms() { printf '%d.%d 秒' "$(( $1 / 1000 ))" "$(( $1 % 1000 / 100 ))"; }
GATE_T0="$(gate_now_ms)"
# 一格判完：记进 `GATE_RAN`，墙钟记进 `GATE_MS`。`$2` / `$3` 是这一格跑的那半的起止（`gate_now_ms`）。
gate_ran() { GATE_RAN+=("$1"); GATE_MS["$1"]=$(( $3 - $2 )); GATE_WIN+=("$2 $3 $1"); }
gate_took() { gate_fmt_ms "${GATE_MS[$1]:-0}"; }

# ── 道：并发跑的那一层 ─────────────────────────────────────────────────────────
# `gate_lane <道> [after <格>…]`：往下的格排进这条道；`after` 只许点本行之前已声明的格（道与道之间因此不会互等成环），
#   被 `GATE_ONLY` 挡掉的不等。`gate_lanes_run`：各道同时起跑、等齐，再按格在本文件里的顺序逐格判。
# 每格分「跑」与「判」两半：跑的那半把输出、退出码、起止写进这一格自己的文件（并发输出不交错），
#   判的那半在本进程里读回来做判定、记收据。不排进道时（自检探针 · `GATE_SERIAL=1` · 没有无网沙箱，CI 即如此）跑完立刻判。
# 并发靠的独立资源：每格一个网络命名空间 · e2e 每套一个 `/tmp`（`gate_nonet_e2e`）· 两棵 Rust 各一把 target 锁（同一棵的格同一条道）。
GATE_PAR=0
if [ "${GATE_SERIAL:-0}" != 1 ] && [ -z "$GATE_NONET_WHY" ] && [ "${#GATE_NONET[@]}" -gt 0 ] && [ -n "${GATE_TMUX_HOME:-}" ]; then
  GATE_PAR=1
  printf '  ·    %-14s %s\n' "并行" "格按道并发跑（道内串、道间并）；每格输出写它自己的文件，跑完按本文件顺序逐格判"
else
  printf '  ·    %-14s %s\n' "并行" "不并（GATE_SERIAL=1 或没有无网沙箱 / 私有 tmux 目录）：逐格跑完立刻判"
fi
GATE_CELL_DIR="$(mktemp -d "${TMPDIR:-/tmp}/gate-cells.XXXXXX")" || { echo "GATE: FAIL —— 建不了各格的输出目录"; exit 2; }

# ── 机器忙时谁让路（10-08）──────────────────────────────────────────────────
# 拿锁那一刻看一次负载（`gate-clean.sh`）管不住跑起来之后才涨上来的负载 ⇒ 门禁自己分两手：
#
# ① **整套 vitest 分核**：`npm` 与 `coverage` 两格在两条道里同时各起一整套 vitest，各按默认（核数 − 1）起 worker
#    ⇒ 合起来近两倍核数，再叠 Rust 与 e2e，按墙钟判的用例被挤过期限。并跑时每格只拿「核数 ÷ 格数」个 worker
#    （经 `GATE_VITEST_WORKERS` 交给 `vitest.config.ts` 的 `maxWorkers`；CI 与平时不设这个变量，照 vitest 默认）。
#    不并跑时一次只有一格在跑，拿满核数。判据：`tests/frontend/ui/gate-vitest-workers.vitest.ts`（两格合起来不超过核数）。
# >>> vitest 分核
GATE_VITEST_CELLS=(npm coverage)
if [ "$GATE_PAR" = 1 ]; then GATE_VITEST_EACH=$(( $(nproc) / ${#GATE_VITEST_CELLS[@]} )); else GATE_VITEST_EACH=$(nproc); fi
[ "$GATE_VITEST_EACH" -ge 1 ] || GATE_VITEST_EACH=1
# <<< vitest 分核
gate_vitest_share() { ( export GATE_VITEST_WORKERS="$GATE_VITEST_EACH"; "$@" ); }
#
# ② **不按墙钟判的格让路**（`nice -n 10`：负载涨上来时 CPU 先给按墙钟判的那些）：
#    · 按墙钟判（有期限、超时即红 ⇒ 普通优先级）—— `npm` · `coverage`（vitest 每条有期限）· `cargo` · `backend`
#      · `comm-boundary` · `test-tiers`（`cargo test` 里有带期限 / 等待上限的用例；这两格的 `cargo test --no-run` 那一半让路，
#      见 `gate_cargo_test_nonet`）· `ccbus-twophase`（真跑、带等待上限）· e2e 各套 · `env-sandbox` · `weak-net`；
#    · 不按墙钟判（只看编得过 / 诊断零条 / 盘上文本 / 两份对得上 ⇒ 让路）—— 编译类 `deadcode` · `deadcode-backend` · `clippy`
#      · `clippy-backend` · `appbuild` · `winchk` · `winchk-backend` · `winlink` · `muslbuild` · `tsc` · e2e 前置那趟 `cargo build`（`e2e-prep`）；
#      读文本类 `worktree-clean` · `hooks` · `copy2` · `shellcheck` · `e2e-smoke` · `release-gate` · `platform` · `installface`
#      · `fmt` · `fmt-backend` · `audit` · `generated` · `tmux-default`。
#    两张表与盘上的格两向相等（同一个判据文件钉）：新加一格就得在这里归一边。
GATE_WALL_CELLS=" npm coverage cargo backend comm-boundary test-tiers ccbus-twophase e2e env-sandbox weak-net "
GATE_YIELD_CELLS=" deadcode deadcode-backend clippy clippy-backend appbuild winchk winchk-backend winlink muslbuild tsc e2e-prep worktree-clean hooks copy2 shellcheck e2e-smoke release-gate platform installface fmt fmt-backend audit generated tmux-default "
gate_yields() { case "$GATE_YIELD_CELLS" in *" $1 "*) return 0 ;; esac; return 1; }
# 只降调它所在的那个子 shell（`$BASHPID`）及其往后起的子进程；调用方负责把它放进子 shell 里。
gate_yield() { renice -n 10 -p "$BASHPID" >/dev/null 2>&1 || true; "$@"; }
GATE_LANE=""              # 现在往哪条道里排（空 = 不排）
GATE_LANES=()             # 道名，按声明顺序
GATE_LANE_PIDS=()         # 起跑后各道的 pid（中断时按 pid 收）
declare -A GATE_LANE_WAIT=()   # 道名 → 起跑前要等的格（序号，空格分隔）
GATE_STEPS=(e2e-prep)     # 不是格、但可以被 `after` 点名的步骤
GATE_Q=()                 # 排进道里、还没判的格（序号）
GATE_Q_N=0
GATE_Q_NAME=() GATE_Q_LANE=() GATE_Q_EXEC=() GATE_Q_JUDGE=()
GATE_K=""                 # 当前这一格的文件前缀（跑与判两半都读它）

# 收掉一棵进程树（只按 pid，不按名字）。
gate_kill_tree() {
  local c
  for c in $(ps -o pid= --ppid "$1" 2>/dev/null); do gate_kill_tree "$c"; done
  kill "$1" 2>/dev/null
}
gate_par_stop() {
  local p
  for p in ${GATE_LANE_PIDS[@]+"${GATE_LANE_PIDS[@]}"}; do gate_kill_tree "$p"; done
}
trap 'gate_par_stop; rm -rf -- "$GATE_CELL_DIR"' EXIT
trap 'echo "GATE: FAIL —— 被中断"; exit 130' INT TERM

gate_lane() {
  local lane="$1" dep i hit w="" known d
  shift
  if [ "${1:-}" = after ]; then shift; fi
  for dep in "$@"; do
    hit=""
    for ((i = 0; i < GATE_Q_N; i++)); do
      if [ "${GATE_Q_NAME[i]:-}" = "$dep" ]; then hit="$i"; fi
    done
    if [ -n "$hit" ]; then w="$w $hit"; continue; fi
    known=0
    for d in ${GATE_DECLARED[@]+"${GATE_DECLARED[@]}"} "${GATE_STEPS[@]}"; do
      if [ "$d" = "$dep" ]; then known=1; break; fi
    done
    if [ "$known" -ne 1 ]; then
      fails+=("gate_lane $lane（after 点名的 \`$dep\` 在本行之前没有声明过 —— 这条道等不到它，按红记）")
    fi
  done
  GATE_LANE="$lane"
  GATE_LANES+=("$lane")
  GATE_LANE_WAIT[$lane]="$w"
}

# 一格：`$1` 道依赖里认的短名 · `$2` 跑的那半 · `$3` 判的那半（都是 `printf %q` 拼好的命令串，读 `$GATE_K`）。
gate_cell() {
  local i="$GATE_Q_N"
  GATE_Q_N=$((i + 1))
  GATE_Q_NAME[i]="$1"
  if [ "$GATE_PAR" = 1 ] && [ -n "$GATE_LANE" ] && [ "${GATE_PROBE:-0}" != 1 ]; then
    GATE_Q+=("$i"); GATE_Q_LANE[i]="$GATE_LANE"; GATE_Q_EXEC[i]="$2"; GATE_Q_JUDGE[i]="$3"
    return 0
  fi
  GATE_K="$GATE_CELL_DIR/c$i-$BASHPID"
  eval "$2"
  eval "$3"
}

# 跑的那半：起止 ＋ 退出码 ＋ stdout/stderr 合在一个文件里。命令跑在子 shell 里（与命令替换同样隔离 `exit`）。
gate_exec_cmd() {
  gate_now_ms > "$GATE_K.t0"
  ( "$@" ) > "$GATE_K.out" 2>&1
  printf '%s' "$?" > "$GATE_K.rc"
  gate_now_ms > "$GATE_K.t1"
}
# 判的那半先读回来；跑的那半没留下退出码（它那条道半路死了）⇒ 这一格不算跑过，按红记。
gate_cell_read() {
  if [ ! -s "$GATE_K.rc" ] || [ ! -s "$GATE_K.t1" ]; then
    fails+=("$1（跑的那半没留下退出码与止点 —— 它那条道半路死了，这一格判不了，按红记）")
    return 1
  fi
  GATE_C_OUT="$(cat "$GATE_K.out" 2>/dev/null)"
  GATE_C_RC="$(cat "$GATE_K.rc")"
  GATE_C_T0="$(cat "$GATE_K.t0")"
  GATE_C_T1="$(cat "$GATE_K.t1")"
}

gate_lanes_run() {
  local lane i w pid line="" n waits
  local -A lane_pid=()
  GATE_LANE=""
  [ "${#GATE_Q[@]}" -gt 0 ] || return 0
  for lane in "${GATE_LANES[@]}"; do
    n=0
    for i in "${GATE_Q[@]}"; do if [ "${GATE_Q_LANE[i]}" = "$lane" ]; then n=$((n + 1)); fi; done
    [ "$n" -gt 0 ] || continue
    line="${line:+$line · }$lane（$n）"
    # 要等的格住的那条道已先起跑；那条道死了就不再等（等的那格判时按红记）。
    waits=""
    for w in ${GATE_LANE_WAIT[$lane]}; do waits="$waits $w:${lane_pid[${GATE_Q_LANE[w]}]:-0}"; done
    (
      for w in $waits; do
        until [ -e "$GATE_CELL_DIR/c${w%%:*}.done" ]; do
          kill -0 "${w#*:}" 2>/dev/null || break
          sleep 0.2
        done
      done
      for i in "${GATE_Q[@]}"; do
        [ "${GATE_Q_LANE[i]}" = "$lane" ] || continue
        GATE_K="$GATE_CELL_DIR/c$i"
        eval "${GATE_Q_EXEC[i]}"
        : > "$GATE_K.done"
      done
    ) </dev/null &
    pid=$!
    lane_pid[$lane]="$pid"
    GATE_LANE_PIDS+=("$pid")
  done
  printf '  ·    %-14s %s\n' "并行" "${#GATE_LANE_PIDS[@]} 条道同时起跑：$line"
  wait "${GATE_LANE_PIDS[@]}"
  GATE_LANE_PIDS=()
  for i in "${GATE_Q[@]}"; do
    GATE_K="$GATE_CELL_DIR/c$i"
    eval "${GATE_Q_JUDGE[i]}"
  done
  GATE_Q=()
}

# 这一格这一趟要不要跑。返回 0 = 跑。`$2`（可选）是它所在的组名。
# ⚠ 副作用：把短名记进 `GATE_DECLARED`、组名记进 `GATE_GROUPS`。
gate_wants() {
  if [ "${GATE_PROBE:-0}" = 1 ]; then return 0; fi
  GATE_DECLARED+=("$1")
  if [ -n "${2:-}" ]; then GATE_GROUPS+=("$2"); fi
  if [ -z "$GATE_ONLY" ]; then return 0; fi
  case " $GATE_ONLY " in
    *" $1 "*) return 0 ;;
  esac
  if [ -n "${2:-}" ]; then
    case " $GATE_ONLY " in *" $2 "*) return 0 ;; esac
  fi
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
    for d in ${GATE_DECLARED[@]+"${GATE_DECLARED[@]}"} ${GATE_GROUPS[@]+"${GATE_GROUPS[@]}"}; do
      if [ "$tok" = "$d" ]; then known=1; break; fi
    done
    if [ "$known" -ne 1 ]; then
      fails+=("GATE_ONLY 里的 \`$tok\` 不是盘上任何一格的名字 —— \
拼错一个字就等于**静默少跑一格**，而少跑与跑过在终端上一模一样 ⇒ 一律按红记。\
盘上现打这几格（短名）：${GATE_DECLARED[*]}")
    fi
  done
}

# 一串字符串印成 JSON 数组。格名里没有引号与反斜杠（`found_cells()` 的正则决定），所以不转义。
gate_json_arr() {
  local i first=1
  printf '['
  for i in "$@"; do
    if [ "$first" -eq 1 ]; then first=0; else printf ', '; fi
    printf '"%s"' "$i"
  done
  printf ']'
}

# 落一张收据。绿红两支都落：只在绿那一支落，就看不见「红过一趟、然后有人把红的那一格删了」。
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
    printf '  "skipped": '; gate_json_arr ${GATE_SKIPPED[@]+"${GATE_SKIPPED[@]}"}; printf ',\n'
    printf '  "ms": {'
    local i first=1
    for i in ${GATE_RAN[@]+"${GATE_RAN[@]}"}; do
      if [ "$first" -eq 1 ]; then first=0; else printf ', '; fi
      printf '"%s": %s' "$i" "${GATE_MS[$i]:-0}"
    done
    printf '},\n'
    printf '  "e2e_prep_ms": %s,\n' "$GATE_PREP_MS"
    printf '  "total_ms": %s\n' "$(( $(gate_now_ms) - GATE_T0 ))"
    printf '}\n'
  } > "$GATE_RECEIPT" || {
    printf 'GATE: 收据落不了盘（%s 写不进去）—— 后面那条 K-G4C-gate-receipt.py 会因此红，那是对的\n' "$GATE_RECEIPT"
    return 0
  }
  printf 'GATE: 收据 —— %s（verdict=%s · 真判过 %s 格 · 跳过 %s 格 · tree %s · dirty %s）\n' \
    "$GATE_RECEIPT" "$verdict" "${#GATE_RAN[@]}" "${#GATE_SKIPPED[@]}" "${tree:0:12}" "$dirty"
}

# ── 失败诊断：红的那一格自带「为什么红」 ────────────────────────────────
# 同一个退出码装着几件事（cargo 的 101：测试红 · 编译错 · 被 OOM 杀 · 环境不满足），只有输出分得开。印三段：
#   ① 关键行：匹配 `GATE_DIAG_PAT` 的行（`-A1` 带一行下文），封顶 `GATE_DIAG_KEY` 行。模式按形状选：cargo 的 `error…`
#      （被信号杀也走它）· panic 落点 · `failures:` · `test result: FAILED` · 正在跑的测试二进制（被杀时说明死在哪个包）·
#      npm 两代前缀 · `::error::` · bash e2e 的失败行 · `cargo fmt --check` 的 `Diff in`。
#   ② 原文尾部 `GATE_DIAG_TAIL` 行，恒印：模式表天生会漏，这一段保证任何形状都不会印出零个字。
#   ③ 输出不到 `GATE_DIAG_WHOLE` 行 ⇒ 全印。
# 每行加 `  | ` 前缀：被测输出里自己打的 `GATE: OK` / `  ok   ` 混不进本脚本的裁决面。
GATE_DIAG_KEY="${GATE_DIAG_KEY:-40}"
GATE_DIAG_TAIL="${GATE_DIAG_TAIL:-30}"
GATE_DIAG_WHOLE="${GATE_DIAG_WHOLE:-60}"
GATE_DIAG_PAT='^(error|npm error|npm ERR!|thread .+ panicked|failures:|test result: FAILED|::error::|Diff in )|^[[:space:]]*(FAIL|BROKEN|Running|×|✗)'

# ①段匹配之前先去色：vitest 在非 TTY 下照样上色，行首是 `ESC[41m`，`^[[:space:]]*FAIL` 匹配不上。
# 只去 CSI（`ESC [ … 字母`），只喂给 grep；②③段仍原样印 `$out`。
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
  # 去色只加在①这一条管道上；②拿的仍是 `$out` 原样。
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

# 单包的那一格：退出码 0 ＋ 输出里 `N passed` 的最大值 > 0 才绿。
# `$2` 是这个数的分母，跟绿行一起印：几个套件串起来时最大值只是其中一套的数，一行读数要说清它数的是什么。
run_gate() {
  local name="$1"; local denom="$2"; shift 2
  gate_wants "$name" || return 0
  local ex ju
  if gate_yields "$name"; then printf -v ex '%q ' gate_exec_cmd gate_yield "$@"; else printf -v ex '%q ' gate_exec_cmd "$@"; fi
  printf -v ju '%q ' gate_judge_gate "$name" "$denom"
  gate_cell "$name" "$ex" "$ju"
}
gate_judge_gate() {
  local name="$1" denom="$2" out rc
  gate_cell_read "$name" || return 0
  out="$GATE_C_OUT"; rc="$GATE_C_RC"
  # 记在命令执行完之后：收据里 `ran` 的意思是「真跑过并被判过」，不是「被点到过」。
  gate_ran "$name" "$GATE_C_T0" "$GATE_C_T1"
  # rc=0 不等于绿（`0 passed` 也是 rc=0）：退出码与读数里的数两条都判。
  local n
  n="$(printf '%s' "$out" | grep -oE '([0-9]+) (passed|个测试)' | grep -oE '[0-9]+' | sort -rn | head -1)"
  if [ "$rc" -ne 0 ]; then
    fails+=("$name（退出码 $rc）")
    gate_diag "$name" "$out"
  elif [ -z "$n" ] || [ "$n" -eq 0 ]; then
    fails+=("$name（读数是 ${n:-<找不到>} —— 0 passed 不是绿）")
    gate_diag "$name" "$out"
  else
    printf '  ok   %-14s %s passed（%s；分母：%s）\n' "$name" "$n" "$(gate_took "$name")" "$denom"
  fi
}

# 多包求和的那一格（`--workspace`）：取最大值只得到最大那个包的数，所以逐行求和；
# 一个成员静默掉出 `--workspace` 时合计只会变小，与「有测试没跑」看不出差别 ⇒ 另判包的集合。
# 跑到的包：每个被测 lib 一行 `Running unittests src/lib.rs (…/deps/<lib 名>-<hash>)`；先剥颜色码（CI 上 cargo 带色）。
gate_ran_libs() {
  sed -E $'s/\x1b\\[[0-9;]*m//g' |
    sed -nE 's#^[[:space:]]*Running unittests .*[/\\]deps[/\\]([A-Za-z0-9_]+)-[0-9a-f]+(\.exe)?\)[[:space:]]*$#\1#p' | sort -u
}
# 一个 workspace 的成员里**该被 `cargo test --lib` 跑到**的 lib 名（`cargo metadata` 现取，不抄成员数）。
# ⚠ 不看 `[lib] test = false`：有人把一个成员的单测关掉，它就该在这里红，而不是悄悄从「该跑到」里出列。
gate_workspace_libs() {
  ( cd "$1" && cargo metadata --no-deps --offline --format-version 1 ) | python3 -c '
import json, sys
m = json.load(sys.stdin)
ids = set(m["workspace_members"])
LIB = {"lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"}
for p in m["packages"]:
    if p["id"] in ids:
        for t in p["targets"]:
            if LIB & set(t["kind"]):
                print(t["name"].replace("-", "_"))
' | sort -u
}
gate_shell_libs() { gate_workspace_libs src/frontend/shell; }

# `$2` 是一个函数名，印出该跑到的 lib 名集合（`cargo metadata` 现取，加 / 删 crate 不用改数）。
# 判：退出码 0 ＋ 该跑到的集合 == 真跑到的集合（两向）＋ 合计 > 0。
run_gate_sum() {
  local name="$1"; local want_fn="$2"; shift 2
  gate_wants "$name" || return 0
  local ex ju
  printf -v ex '%q ' gate_exec_sum "$want_fn" "$@"
  printf -v ju '%q ' gate_judge_sum "$name" "$want_fn"
  gate_cell "$name" "$ex" "$ju"
}
# 跑的那半先列该跑到的成员（`$1` 那个函数），再跑命令。
gate_exec_sum() {
  local want_fn="$1"; shift
  gate_now_ms > "$GATE_K.t0"
  ( "$want_fn" ) > "$GATE_K.want" 2>&1
  printf '%s' "$?" > "$GATE_K.wrc"
  ( "$@" ) > "$GATE_K.out" 2>&1
  printf '%s' "$?" > "$GATE_K.rc"
  gate_now_ms > "$GATE_K.t1"
}
gate_judge_sum() {
  local name="$1" want_fn="$2" out rc want wrc got miss extra lines n pkgs
  gate_cell_read "$name" || return 0
  out="$GATE_C_OUT"; rc="$GATE_C_RC"
  want="$(cat "$GATE_K.want" 2>/dev/null)"; wrc="$(cat "$GATE_K.wrc" 2>/dev/null)"; wrc="${wrc:-1}"
  gate_ran "$name" "$GATE_C_T0" "$GATE_C_T1"
  got="$(printf '%s\n' "$out" | gate_ran_libs)"
  lines="$(printf '%s' "$out" | grep -oE '^test result: ok\. [0-9]+ passed')"
  pkgs="$(printf '%s' "$lines" | grep -c . || true)"
  n="$(printf '%s' "$lines" | grep -oE '[0-9]+' | paste -sd+ - | bc 2>/dev/null || echo 0)"
  miss="$(comm -23 <(printf '%s\n' "$want") <(printf '%s\n' "$got") | grep . | tr '\n' ' ')"
  extra="$(comm -13 <(printf '%s\n' "$want") <(printf '%s\n' "$got") | grep . | tr '\n' ' ')"
  if [ "$rc" -ne 0 ]; then
    fails+=("$name（退出码 $rc）")
    gate_diag "$name" "$out"
  elif [ "$wrc" -ne 0 ] || [ -z "$want" ]; then
    fails+=("$name（列不出该跑到的成员 —— $want_fn 退出码 $wrc：${want:-<空>}）")
  elif [ -n "$miss" ] || [ -n "$extra" ]; then
    fails+=("$name（成员与真跑到的包对不上：该跑没跑 [${miss% }] · 跑了却不是成员 [${extra% }] —— \
合计变小与「有测试没跑」在终端上一模一样，只有这条认得出来）")
    gate_diag "$name" "$out"
  elif [ -z "$n" ] || [ "$n" -eq 0 ]; then
    fails+=("$name（读数是 ${n:-<找不到>} —— 0 passed 不是绿）")
    gate_diag "$name" "$out"
  else
    printf '  ok   %-14s %s passed（%s；%s 个包合计，成员集合 == 跑到的包集合）\n' "$name" "$n" "$(gate_took "$name")" "$pkgs"
  fi
}

# ── 门禁自己的自检：真跑必红的合成命令，断言那一趟真的印出了被测命令的哨兵 ──────
# 只 grep 盘上有没有 `gate_diag` 证明不了它被走到。探针跑在子 shell 里，`fails+=` 落在副本上，只漏出标准输出。
#   ① `run_gate` 的失败支印出输出 · ② `run_gate_sum` 在编译错误那一形（没有 `failures:` 段）上印出输出 ·
#   ③ 一条模式都不匹配、哨兵只在尾部 ⇒ 只能靠「原文尾部恒印」看见 ·
#   ④ 失败行行首带色码、哨兵在尾部之外 ⇒ 只能靠①段去色后匹配（垫的行数按 `GATE_DIAG_TAIL` / `GATE_DIAG_WHOLE` 现算，调大上限也裹不进来）。

# ⑤–⑩ 断「这一条判定还在判」：喂一份只有目标判定拦得住的输入（同一函数里其余判定都已满足），两侧都断 ——
#   绿行前缀 `  ok   ` 不出现（判定被掏空就会落到绿行）· 哨兵在（挡「门整个没跑」的空真）。
# 盖不到的：`generated` 那两条（行内判定，没有喂输入的入口）· `run_e2e` 的「抓不到合计 PASS=」（要真跑一整套 npm 套件）。
# 探针不改工作树、不碰 git 与 `~/.claude`；⑩ 只读 `tests/e2e/assert-pass-floor.sh`，在参数校验那步就退出，
#   靠它把参数原样回显 —— 哪天不回显了，哨兵消失 ⇒ 响的红。
gate_assert_judged() {
  local name="$1" out="$2" sentinel="$3" cut="$4"
  case "$out" in
    *"  ok   "*)
      fails+=("gate $name（**「$cut」这一条判定不再判了** —— 喂一份只有它拦得住的合成输入，\
那一格却印出了绿行 ⇒ 一条判定被掏空、门禁照旧放行，门禁照旧放行）")
      return 0 ;;
  esac
  case "$out" in
    *"$sentinel"*) ;;
    *) fails+=("gate $name（断「$cut」的那条探针**自己空转了** —— 绿行没出现，哨兵 $sentinel \
也不在输出里 ⇒ 合成输入根本没走到该走的失败支，这一格判不了，按红记，不许当成绿）") ;;
  esac
}

gate_probe_libs_a() { echo a; }
gate_probe_libs_ab() { printf 'a\nb\n'; }
gate_selftest() {
  # 探针不受 `GATE_ONLY` 影响：`gate_wants` 第一行读它，动态作用域下调出去的 `run_gate*` 里也看得见。
  local GATE_PROBE=1
  local probe
  probe="$(run_gate 自检① - bash -c 'printf "error: GATE-PROBE-A\n"; exit 3' 2>&1)"
  case "$probe" in
    *GATE-PROBE-A*) ;;
    *) fails+=("gate 自检①（run_gate 的失败支没把被测命令的输出印出来 —— 门禁红了又不说为什么红）") ;;
  esac
  probe="$(run_gate_sum 自检② gate_probe_libs_a bash -c 'printf "error[E0425]: GATE-PROBE-B\n --> src/x.rs:1:1\n"; exit 101' 2>&1)"
  case "$probe" in
    *GATE-PROBE-B*) ;;
    *) fails+=("gate 自检②（run_gate_sum 的失败支在「编译错误」那一形上印不出东西 —— \
那一形没有 failures: 段）") ;;
  esac
  probe="$(run_gate 自检③ - bash -c 'i=1; while [ $i -le 100 ]; do
      if [ $i -ge 97 ]; then printf "尾部第 %s 行 GATE-PROBE-C\n" "$i"; else printf "无关行 %s\n" "$i"; fi
      i=$((i + 1)); done; exit 4' 2>&1)"
  case "$probe" in
    *GATE-PROBE-C*) ;;
    *) fails+=("gate 自检③（模式表一条都没匹配上时，「原文尾部恒印」那半兜底没走到 —— \
fail-closed 的承重墙塌了：从此模式表漏掉的形状会退化成一个字都不印）") ;;
  esac
  # 探针④：失败行**行首带 ANSI 色码**（vitest 那一形），哨兵只在这一行上；
  # 垫的无关行条数现算，保证①之外的两条路（②尾部 / ③全印）**都够不着**它。
  probe="$(run_gate 自检④ - bash -c '
      printf "\033[41m\033[1m FAIL \033[22m\033[49m GATE-PROBE-D 行首带色的失败行\n"
      n=$(( $1 + $2 + 10 )); i=1
      while [ "$i" -le "$n" ]; do printf "无关行 %s\n" "$i"; i=$((i + 1)); done
      exit 5' _ "$GATE_DIAG_TAIL" "$GATE_DIAG_WHOLE" 2>&1)"
  case "$probe" in
    *GATE-PROBE-D*) ;;
    *) fails+=("gate 自检④（失败行行首带 ANSI 色码时，「关键行」那一段匹配不上 —— \
vitest 在非 TTY 下照样上色，ESC 不是 [[:space:]]，\
于是每一趟红都退化成「一条都没匹配上」，看不到失败用例的名字）") ;;
  esac

  # 探针⑤ · `run_gate` 的退出码那条：读数 `7 passed` 已满足「0 passed 不是绿」。
  probe="$(run_gate 自检⑤ - bash -c 'printf "GATE-PROBE-E 7 passed\n"; exit 3' 2>&1)"
  gate_assert_judged 自检⑤ "$probe" GATE-PROBE-E "run_gate 的「退出码非零 ⇒ 红」"
  # 探针⑥ · `run_gate` 的「0 passed 不是绿」：`exit 0` 已满足退出码那条。
  probe="$(run_gate 自检⑥ - bash -c 'printf "GATE-PROBE-F 0 passed\n"; exit 0' 2>&1)"
  gate_assert_judged 自检⑥ "$probe" GATE-PROBE-F "run_gate 的「0 passed 不是绿」"
  # 探针⑦ · `run_gate_sum` 的退出码那条：跑到的 {a} == 该跑的 {a}、合计 7，其余两条都已满足。
  probe="$(run_gate_sum 自检⑦ gate_probe_libs_a bash -c 'printf "  Running unittests src/lib.rs (/x/deps/a-0a1b)\ntest result: ok. 7 passed\nGATE-PROBE-G\n"; exit 3' 2>&1)"
  gate_assert_judged 自检⑦ "$probe" GATE-PROBE-G "run_gate_sum 的「退出码非零 ⇒ 红」"
  # 探针⑧ · `run_gate_sum` 的成员集合那条：`exit 0`、合计 7，只跑到 {a} 而该跑的是 {a, b}。
  probe="$(run_gate_sum 自检⑧ gate_probe_libs_ab bash -c 'printf "  Running unittests src/lib.rs (/x/deps/a-0a1b)\ntest result: ok. 7 passed\nGATE-PROBE-H\n"; exit 0' 2>&1)"
  gate_assert_judged 自检⑧ "$probe" GATE-PROBE-H "run_gate_sum 的「成员集合 == 跑到的包集合」"
  # 探针⑨ · `run_gate_sum` 的「0 passed 不是绿」：`exit 0` ＋ 集合相等，合计恰好是 0。
  probe="$(run_gate_sum 自检⑨ gate_probe_libs_a bash -c 'printf "  Running unittests src/lib.rs (/x/deps/a-0a1b)\ntest result: ok. 0 passed\nGATE-PROBE-I\n"; exit 0' 2>&1)"
  gate_assert_judged 自检⑨ "$probe" GATE-PROBE-I "run_gate_sum 的「0 passed 不是绿」"
  # 探针⑩ · 采集面认得带颜色的 cargo 输出（CI 上就是这一形）。
  [ "$(printf '\033[1m\033[92m     Running\033[0m unittests src/lib.rs (/x/deps/a-0a1b)\n' | gate_ran_libs)" = a ] ||
    fails+=("gate 自检⑩（带颜色的 cargo 输出里认不出跑到的包 ⇒ CI 上 cargo 格必红）")
}
gate_selftest

# ── `worktree-clean`：前置条件 —— 仓里不许有第二份工作副本 ──
# 排在所有格之前：一族判据的人群是「走文件系统」，仓内多一份副本（并发的 worktree、变异副本）人群就静默膨胀 ——
# 恒等断言红一片和改动无关的，地板断言则一声不吭地过去。本格红时先清副本再重跑。
run_gate worktree-clean '判过的条数（抽样 4 个扩展名 `.sh`/`.mjs`/`.rs`/`.ts`，每个一条恒等断言：`git ls-files` 认的份数 == 走文件系统走出的份数）。⚠ 抽样不是全集：挑的是走文件系统的判据在数的东西。买的是「仓里没有第二份工作副本」，不买「所有判据的人群都对」—— 被 `.gitignore` 掉的源码同样会让走文件系统的判据多看一份，本格按 gitignore 的口径算、看不见它' \
         python3 tests/evidence/K-W25-worktree-clean.py

# ── 按步骤名照 `ci.yml` 原样跑某几步 —— 只给**留在 Windows job 里**的那几步用（`coverage` · `audit`）。
# 那几步的命令住 `ci.yml` 的 Windows job（门禁跑不了真 Windows，那个 job 保留），本文件不抄第二份。
# ⚠ 只认在仓根跑的步骤：名字与 `run:` 之间出现 `working-directory:` ⇒ 按红记；名字对不上 ⇒ 同样按红记。
gate_ci_step_body() {
  awk -v want="- name: $1" '
    { s = $0; sub(/^[ \t]+/, "", s) }
    st == 0 { if (index(s, want) == 1) st = 1; next }
    st == 1 {
      if (s ~ /^#/) next
      if (s ~ /^working-directory:/) { print "\001WORKDIR"; exit }
      if (s ~ /^run:[ \t]*[|>]-?[ \t]*$/) { match($0, /^ */); key = RLENGTH; st = 2; next }
      if (s ~ /^run:/) { sub(/^run:[ \t]*/, "", s); print s; exit }
      if (s ~ /^- / || s == "") exit
      next
    }
    st == 2 {
      if (s == "") { print ""; next }
      match($0, /^ */)
      if (RLENGTH <= key) exit
      if (ind == 0) ind = RLENGTH
      print substr($0, ind + 1)
    }
  ' .github/workflows/ci.yml
}
gate_ci_steps() {
  local label="$1"; shift
  local name body ran=0 rc
  for name in "$@"; do
    body="$(gate_ci_step_body "$name")"
    case "$body" in
      '')
        printf '%s: ci.yml 里找不到名字以「%s」打头、带 run: 的那一步 —— 判不了，按红记\n' "$label" "$name"
        return 2 ;;
      $'\001WORKDIR'*)
        printf '%s: ci.yml 里「%s」那一步带 working-directory，本函数只在仓根跑 —— 判不了，按红记\n' "$label" "$name"
        return 2 ;;
    esac
    printf '%s: ── 照 ci.yml 跑「%s」\n' "$label" "$name"
    bash -eo pipefail -c "$body" 2>&1
    rc=$?
    if [ "$rc" -ne 0 ]; then
      printf '%s: 「%s」退出码 %s —— CI 上同一步会红\n' "$label" "$name" "$rc"
      return "$rc"
    fi
    ran=$((ran + 1))
  done
  printf '%s: %s passed（ci.yml 里 %s 步原样跑过）\n' "$label" "$ran" "$ran"
}

# ── 「静态」道：只读盘上文本的那几格（外加 `audit` 问一趟 registry）──
gate_lane 静态

# ── `hooks`：`tests/hooks/` 里会被 git 执行的那几份跑不跑得起来 ──
# 本仓 `core.filemode=false`，`chmod +x` 不进 git，而 644 的 hook 会被 git 忽略并照常提交。
# 所以「盘上可执行」（`test -x`）与「库里记着可执行位」（index mode）分开判；只判「文件在不在」不算数。
# 判据本体住 `tests/scripts/hooks-are-runnable.sh`（含阳性对照，能对着变异副本跑）。
run_gate hooks '每个被跟踪的 hook 文件 3 条（盘上可执行 · 库里记着可执行位 · 语法过得了它自己声明的解释器）＋ 8 条阳性对照；份数以判据本体那一行为准（它从 `git ls-files tests/hooks/` 现算）。hooks/ 之外的树本行盖不到' \
         bash tests/scripts/hooks-are-runnable.sh

# ── `copy2`：量具的还原那一跳有没有把旧 mtime 搬回被测树 ──
# 用 `shutil.copy2` 还原源码会带回旧 mtime ⇒ cargo 判「源码没变」、复用上一刀的产物，那一趟读数作废。
# 判复制的目的地落不落在被 git 跟踪的工作树内容上（造夹具、拷读数是正当用途，不红）。
# 判据本体与它看不见什么住 `tests/evidence/K-R115-ruler.py` 头注。本格是唯一盖到 `evidence/` 的门。
run_gate copy2 '`evidence/*.py` 里 `shutil` 保元数据复制族（copy2 · copytree · copystat）的调用点数，逐处判目的地；份数以 `tests/evidence/K-R115-ruler.py` 自己印的那一行为准。⚠ 只看 `evidence/` 下的 `.py`；别的目录、别的语言、shell 串里的 `cp -a` 本行盖不到' \
         bash -c 'python3 tests/evidence/K-R115-ruler.py'

# ── `shellcheck`：shell 脚本的 `--severity=error` 那一档 ───────────────────────────────────────
# 人群的唯一住址是下面这一行（CI 调本格）；`shell_lint_registry` 读它判每个 shell 脚本要么在人群里、要么登记豁免。
# 本段不让任何注释行以 `#` ＋ 空格 ＋ 那个工具名开头：那是它的指令语法。
# 反空真：展开出的份数 > 0 且本文件在里面；某组 glob 一个都不匹配由 `shell_lint_registry` 接住。本地与 CI 的二进制版本可能不同。
GATE_SHELLCHECK_GLOBS='tests/e2e/*.sh src/shared/cc-bus/scripts/* src/shared/cc-bus/examples/cc-keepalive tests/e2e/fake-claude tests/e2e/weak-net/*.sh tests/e2e/local-backend-container/*.sh tests/evidence/*.sh tests/scripts/*.sh tests/hooks/* tests/shots/perf/*.sh'
gate_shellcheck() {
  local out rc n self=0 f
  command -v shellcheck >/dev/null 2>&1 || {
    printf 'shellcheck: 这台机器上没有 shellcheck —— 判不了，按红记（不许退化成静默跳过）\n'
    return 1
  }
  local -a files=()
  shopt -s nullglob
  # shellcheck disable=SC2206  # 这里要的就是按空白拆开、再按 glob 展开
  files=($GATE_SHELLCHECK_GLOBS)
  shopt -u nullglob
  n=${#files[@]}
  for f in "${files[@]}"; do [ "$f" = tests/scripts/gate.sh ] && self=1; done
  if [ "$n" -eq 0 ] || [ "$self" -ne 1 ]; then
    printf 'shellcheck: 人群展开出 %s 份、本文件%s在里面 —— 展开坏了。「扫了 0 个」与「全都干净」在退出码上一模一样，按红记\n' \
           "$n" "$([ "$self" -eq 1 ] && echo || echo 不)"
    return 1
  fi
  out="$(LC_ALL=C.UTF-8 shellcheck --severity=error "${files[@]}" 2>&1)"
  rc=$?
  printf '%s\n' "$out" | head -80
  if [ "$rc" -ne 0 ]; then
    printf 'shellcheck: 退出码 %s\n' "$rc"
    return "$rc"
  fi
  printf 'shellcheck: %s passed（人群住本文件 GATE_SHELLCHECK_GLOBS 一处）\n' "$n"
}
run_gate shellcheck '不是「几条断言过了」：这个数是人群展开出来的 shell 文件份数（`--severity=error`）。⚠ 只判 error 这一档；`.ps1` 全仓零 lint' \
         gate_shellcheck

# ── `e2e-smoke`：两步只读盘上文本的体检 ─────────────────────────────────────────────────────
# `tests/e2e/*.py` 过 `py_compile` · `src/shared/` 下带 shebang 的文件在 git 里是 100755（`exec-bit-guard.sh`）。
gate_e2e_smoke() {
  python3 -m py_compile tests/e2e/*.py || { printf 'e2e-smoke: tests/e2e/*.py 编不过\n'; return 1; }
  bash tests/e2e/exec-bit-guard.sh || return $?
  printf 'e2e-smoke: 2 passed（py 语法 · 可执行位）\n'
}
run_gate e2e-smoke '步数：`tests/e2e/*.py` 的 `py_compile` · `tests/e2e/exec-bit-guard.sh` 两步，只读盘上文本' \
         gate_e2e_smoke

# ── `release-gate`：发版那条流水线的盘上文本 ──
# 判据本体 `tests/evidence/K-R124-ruler.py`（CI 那一步跑同一份；不依赖 PyYAML，自带 YAML 子集切块器）：
#   `release.yml` 的触发与发布闸 · 产字节那条路（承诺的平台 ↔ 产线 ↔ 登记）· `BUILD_ID` 的抠法实打得到 ·
#   本文件 `muslbuild` 点名的工具链版本 == `release.yml` 真装的 · `tests/scripts/re-embed.sh` 与发版那一步同源。
# 射程与买不到的东西住那份文件头注。它不编字节：re-embed 只真跑 `--check`（只读）。
run_gate release-gate '判过的条数（`release.yml` 上逐行印出来的 PASS：触发与发布闸 · 两处发布步骤的正文来源与生成器 · 产字节那条路（承诺的平台 ↔ 产线 ↔ 登记、target triple，两向集合相等；runner 标签逐字）· 每一处抠 `const BUILD_ID`／身份戳界标的住址实打读得到恰好一行 · 本文件 muslbuild 点名的工具链版本 == `release.yml` 真装的 · `tests/scripts/re-embed.sh` 与发版那一步同源（target 与旗标逐字、落点 ↔ `.gitignore` 两向）并真跑 `--check` · Release 资产 ↔ `src/doc/RELEASING.md` §2.2 登记表）。⚠ 不执行 GitHub 的表达式求值器、不跑流水线 ⇒ 「盘上文本满足这几条」不等于「云端那一趟会绿」，更不等于字节在目标机器上跑得起来；往 Release 上写只认 `softprops/action-gh-release` 与 `gh release`/`gh api …/releases` 两种形状；正文写得对不对不判；`--check` 只读，在没铺字节的树上只答得出「没有对不上的字节」' \
         python3 tests/evidence/K-R124-ruler.py

# ── `platform`：承诺的平台 ↔ 门禁真跑的格（判据本体 `tests/evidence/K-G4-platform-ledger.py`）──
# 各格只说「我编得过」，这一格说「该编的都编了」：少一格、或多一个没登记的 target 都红。只判登记，不编东西。
run_gate platform '判过的条数（判据本体每趟现算并印在它自己那行上：P1 承诺表 ↔ 门禁格两向集合相等 ＋ P2 每格一条逐字锚点 count()==1 ＋ P3 显式拒绝的那格全仓零脚印）。⚠ 反空真锚是 P1 那两向相等。⚠ 它不编任何东西：判的是门禁盖到了哪些平台，不判那些平台上真跑得起来' \
         python3 tests/evidence/K-G4-platform-ledger.py

# ── `installface`：安装面那 22 条命令的分组与前端落点（判据本体 `tests/evidence/K-R117-ruler.py`）──
# 每组前端落点的名单钉在 `FRONTEND_PIN` 里、逐字相等（不是 `<=`）：多一份落点或表记错都红；收干净一组就同拍把那一行降下来。
# 射程见那份文件头注；`parity_ledger.rs` 那一份判不了（命令名就从它解析），它的闸在闭集判定。
run_gate installface '判过的条数（判据本体逐条印出来的 PASS：22 条命令各归一组 ＋ 闭集并集两向 ＋ 五组交集空 ＋ 5 组前端落点棘轮 ＋ 22 条包装层入口两侧 ＋ `claims()` 10 个装/卸符号各有着落）。⚠ 只在红的时候出声的那几条不在这个数里。⚠ 落点只认调用形状 `.<命令>(`（注释里提到命令名不算），`invoke("<名>")` 直呼只出读数；度量的是几份文件不是几处引用。⚠ `parity_ledger.rs` 那一份判不了（空真），闸在闭集判定' \
         python3 tests/evidence/K-R117-ruler.py

# ── `ccbus-twophase`：cc-bus 两阶段读口 ＋ 三个适配 trait ＋ Windows 那一侧 ──
# 推进已读位置是有副作用的那一跳，错一次就是「消息被消费掉却没人看见」。判据本体与射程住
# `tests/evidence/W24C-ccbus-twophase-ruler.py`；Windows 那侧靠 `CCBUS_ADAPT_OS=windows` 在 Linux 上跑实现，不买真 Windows。
run_gate ccbus-twophase '判过的条数（判据本体每趟现算并印在它自己那几行上：静态 7 条 —— `cc-peek` 零写面（写形集合 == 登记的两处豁免，且正控要在 `cc-commit` 上扫出写）· `.pos` 写点全仓集合相等 · 锁族不增 · 通用层零脚印（带正控）· 与 `cc-recv` 的渲染逐字节对拍 · `cc-recv` 的 sha256 恒等 · 手册页那几句；真跑 14 条 —— 令牌/CAS/anchor/分段/并发/截短自愈/Stop 钩子两条路/Windows 那一侧四条；kinds 静态 2 条（敲门模板零正文 ＋ 正控）· 真跑 3 条；保活 1 条 —— 共 27）。⚠ **反空真锚不是这个数，是末尾那条「标签集合与登记两向相等」** —— 某一格悄悄没跑与它过了，在输出上一模一样。⚠ 它**不判**在真 Windows 上跑得起来（无真机）、不判性能、不判并发的公平性' \
         python3 tests/evidence/W24C-ccbus-twophase-ruler.py

# ── `fmt`：`src/frontend/shell` workspace 的排版 ──
# 云端 Windows job 的第一步就是 `cargo fmt --check`，它红了后面全 `skipped`；本机先拦住。只判排版，不判代码对。
# `src/backend` 是另一个 workspace，由下面 `fmt-backend` 盖。
run_gate fmt '不是数出来的数：`cargo fmt --all --check` 只有绿/红两态（rc=0 / rc=1），本格的「分母」是 `src/frontend/shell` 那个 workspace 的全部成员；`src/backend` 是另一个 workspace，本行盖不到（那一棵由下面 fmt-backend 那一格盖）' \
         bash -c 'cd src/frontend/shell && cargo fmt --all --check 2>&1 && echo "fmt: 1 passed"'

# ── `fmt-backend`：`src/backend` 那棵树的排版 ──
# 不并进 `src/frontend/shell` 的 workspace：那个 Linux-only 的后端会被拖进 Windows CI 的 `cargo test --all`。
# 刻意不加 `--all`：那棵树的 path 依赖指进 `src/frontend/shell`，加了会把别的树（各有自己那一格）拉进来；
#   不加时 rustfmt 只收 `src/backend` 自己的根。与 `ci.yml` 的 `backend` job 同一条命令。
run_gate fmt-backend '不是数出来的数：`cargo fmt --check` 只有绿/红两态（rc=0 / rc=1），本格的「分母」是 `src/backend` 那个 workspace 的唯一成员 `cc-monitor-backend`；`src/frontend/shell` 由上面 fmt 那一格管，本行盖不到（刻意不加 --all，理由见上方注释）' \
         bash -c 'cd src/backend && cargo fmt --check 2>&1 && echo "fmt-backend: 1 passed"'

# ── `audit`：`ci.yml` 的 `frontend` job 里那一步 `npm audit` ──
# ⚠ 要联网：它问的是 npm registry **当下**的漏洞库 —— 断网时退出码非零、本格红（不静默跳过）；
#   同一棵树也可能因为库里新登了一条 high 而隔夜变红，那与 CI 上同一步的行为一致。
run_gate audit '步数：`ci.yml` 的 `npm audit (production deps, high)` 一步原样跑（`--omit=dev --audit-level=high`），只有绿/红两态。⚠ 要联网、判的是 registry 当下的漏洞库；dev 依赖与 high 以下的档本行不看' \
         gate_ci_steps audit "npm audit (production deps, high)"

# ── 「壳 Rust」道：`src/frontend/shell` 那一棵（一把 target 锁）──
gate_lane 壳Rust

# 该跑到的包 = `src/frontend/shell` workspace 的成员（`cargo metadata` 现取），与输出里真跑到的两向相等。
run_gate_sum cargo gate_shell_libs gate_cargo_test_nonet src/frontend/shell --workspace --lib

# `src/frontend/shell/embedded-backends/` 铺没铺（gitignore 挡着，跟着铺走、不跟 git 走）决定 `build.rs` 置不置 `embedded_backends` cfg。
# 挂这个 cfg 的是「本地后端真的能起来吗」那一族：`sftp::embedded_backend_binaries_present_and_valid` ·
# `local_backend_host::the_local_backend_host_can_be_stopped_and_started_again` ·
# `local_backend::the_local_tmux_frames_really_land_in_the_ledger`〔散文墓碑〕（随 monitor 那本 tmux 原文账删了） ·
# `local_backend::the_local_backend_host_really_registers_an_inbound_client`。
# 没铺时上面那个合计少了这一族。这一行只自报、不判（铺与没铺本该是两个数），所以行首不是 `ok`。
if [ -d src/frontend/shell/embedded-backends ]; then
  printf '  分母 %-14s %s\n' "cargo" "本树铺了 src/frontend/shell/embedded-backends/ ⇒ embedded_backends cfg 会置上，「本地后端真的能起来吗」那一族在跑"
else
  printf '  分母 %-14s %s\n' "cargo" "本树未铺 src/frontend/shell/embedded-backends/ ⇒ embedded_backends cfg 不置 ⇒ 上面那个合计里少了「本地后端真的能起来吗」那一族（4 条，逐个点名见上方注释）"
fi

# ── `comm-boundary` · `test-tiers`：两族判据还在不在 ─────────────────────────────────────
# 两族各挂在一个模块上，那一行 `mod` 被摘掉时整族消失，而 `cargo` 那格只会合计小一点。
# 判：判据文件里声明的 `#[test]` 名字集合 == 这一趟 cargo 真跑过的集合（两向，两侧异源）。不判它们判得对。
gate_family() {
  local label="$1" file="$2" pkg="$3" filter="$4" out rc declared ran miss extra
  [ -r "$file" ] || { printf '%s: 判据本体 %s 盘上读不到 —— 住址改了就回来改本格\n' "$label" "$file"; return 1; }
  declared="$(awk '/^[[:space:]]*#\[(tokio::)?test\]/{t=1; next} t && /fn [A-Za-z0-9_]+/{match($0, /fn [A-Za-z0-9_]+/); print substr($0, RSTART+3, RLENGTH-3); t=0}' "$file" | sort -u)"
  out="$(cd src/frontend/shell && cargo test -p "$pkg" --lib "$filter" 2>&1)"; rc=$?
  if [ "$rc" -ne 0 ]; then printf '%s\n' "$out" | tail -40; printf '%s: cargo test 退出码 %s —— 判不了\n' "$label" "$rc"; return "$rc"; fi
  ran="$(printf '%s\n' "$out" | sed -nE "s/^test .*${filter}([A-Za-z0-9_]+) \.\.\. ok\$/\1/p" | sort -u)"
  if [ -z "$declared" ]; then printf '%s: %s 里一条 `#[test]` 都没认出来 —— 读法与源码对不上，判不了\n' "$label" "$file"; return 1; fi
  miss="$(comm -23 <(printf '%s\n' "$declared") <(printf '%s\n' "$ran") | tr '\n' ' ')"
  extra="$(comm -13 <(printf '%s\n' "$declared") <(printf '%s\n' "$ran") | tr '\n' ' ')"
  if [ -n "${miss// /}" ] || [ -n "${extra// /}" ]; then
    printf '%s\n' "$out" | tail -25
    printf '%s: 声明的判据与真跑过的对不上 —— 声明了没跑 [%s] · 跑了却没在 %s 里声明 [%s]\n' "$label" "${miss% }" "$file" "${extra% }"
    return 1
  fi
  printf '%s: %s passed（%s 里声明的 #[test] 名字集合 == cargo 真跑过的集合）\n' "$label" "$(printf '%s\n' "$ran" | grep -c .)" "$file"
}
run_gate comm-boundary '判过的条数 = 通信层那一族这一趟真跑过的条数；声明的名字集合 == 真跑过的集合' \
         gate_family comm-boundary tests/frontend/shell/comm_boundary_registry_tests.rs monitor comm_boundary_registry::tests::
run_gate test-tiers '判过的条数 = 测试层分级那一族这一趟真跑过的条数；声明的名字集合 == 真跑过的集合' \
         gate_family test-tiers tests/common/guard-core/test_tiers_tests.rs guard-core test_tiers::

# ── `deadcode` / `deadcode-backend`：非 test 构建里的死代码，零容忍 ───────────────────────────────────
# `cargo test` 看不见它（test 构建里测试就是调用方）⇒ 单开一趟 `cargo check`：`dead_code` 诊断一条都不许有，有就逐条列。
# 只在别的平台上才有调用方的项，用 `#[cfg(…)]` 和调用方放在同一个平台上；只有测试读的，删掉或收进 `#[cfg(test)]`。
# 反空真：JSON 里必须有那个包的 lib artifact。射程：monitor 那一格 = `-p monitor` 编到的包的非 test 段；
#   后端那一格 = `src/backend` 一个包的 lib ＋ bin 非 test 段（它在后端那条道里，同一把 target 锁）。`#[cfg(test)]` 两格都盖不到。
# `gate_deadcode <格名> <目录> <lib artifact 名> <cargo check 的参数…>`
gate_deadcode() {
  local label="$1" dir="$2" lib="$3" out rc
  shift 3
  out="$(cd "$dir" && cargo check "$@" --message-format=json 2>&1)"; rc=$?
  printf '%s\n' "$out" | python3 -c '
import json, sys
rc, label, lib = int(sys.argv[1]), sys.argv[2], sys.argv[3]
seen_lib, dead, errs = False, [], []
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("target", {}).get("name") == lib:
        seen_lib = True
    if m.get("reason") != "compiler-message":
        continue
    msg = m["message"]
    if msg.get("level") == "error":
        errs.append(msg.get("rendered") or msg["message"])
    if (msg.get("code") or {}).get("code") == "dead_code":
        sp = [x for x in msg["spans"] if x.get("is_primary")] or msg["spans"] or [{}]
        dead.append("%s:%s  %s" % (sp[0].get("file_name", "?"), sp[0].get("line_start", "?"), msg["message"]))
if rc != 0:
    print("".join(errs)[-4000:])
    print("%s: cargo check 退出码 %d —— 判不了" % (label, rc))
    sys.exit(rc)
if not seen_lib:
    print("%s: 输出里没有 %s 的 artifact —— 这一趟没走到那个包，「零条死代码」不算数" % (label, lib))
    sys.exit(1)
if dead:
    for d in sorted(set(dead)):
        print("  dead_code  " + d)
    print("%s: 非 test 构建里有 %d 条死代码（上面逐条）—— 删掉，或用 #[cfg(…)] 收到它真有调用方的那个平台" % (label, len(set(dead))))
    sys.exit(1)
print("%s: 1 passed（%s 非 test 构建零条 dead_code）" % (label, lib))
' "$rc" "$label" "$lib"
}
run_gate deadcode '不是数出来的数：`cargo check -p monitor`（非 test）的 `dead_code` 诊断零条才绿，有就逐条列出来' \
         gate_deadcode deadcode src/frontend/shell monitor_lib -p monitor

# ── `clippy` / `appbuild`：`ci.yml` 里 `rust` job 的 `cargo clippy --workspace --all-targets` 与 `linux-app-build` job 的 `cargo build` ──
# clippy 不带 `-D warnings`（deny 档的 lint 与编译错照样红）；appbuild 真编真链 Linux 上的两个二进制（`cargo` 那格带 `--lib`、`deadcode` 只 check）。
# 成功时只印输出尾部两行：警告片段会带出源码散文里的数字，而 `run_gate` 取最大的那个数。红的时候整份照印。
run_gate clippy '不是数出来的数：`cargo clippy --workspace --all-targets` 只有绿/红两态，分母是 `src/frontend/shell` 那个 workspace 的全部成员与全部 target（含 test 档）。⚠ 与 CI 那一步同一条命令、不加 `-D warnings` ⇒ 警告不红，只有 deny 档的 lint 与编译错红；CI 那一步跑在 windows-latest 上，本格跑在 Linux 上（Windows 那一维由 `winchk` 盖）' \
         bash -c 'cd src/frontend/shell && out=$(cargo clippy --workspace --all-targets 2>&1); rc=$?; if [ "$rc" -ne 0 ]; then printf "%s\n" "$out"; exit "$rc"; fi; printf "%s\n" "$out" | tail -2; echo "clippy: 1 passed"'
run_gate appbuild '不是数出来的数：`cargo build`（dev）只有绿/红两态，射程 = `src/frontend/shell` 根包 `monitor` 的 lib 与两个二进制（`cc-monitor` · `cc-monitor-filewin`）在 Linux 上真编真链一趟，与 `ci.yml` 的 `linux-app-build` 那一步同一条命令。⚠ 只链不跑；release 档不编；前端产物（`dist/`）由 `npm` 那格里的真 vite 构建与 `tsc` 那格盖' \
         bash -c 'cd src/frontend/shell && out=$(cargo build 2>&1); rc=$?; if [ "$rc" -ne 0 ]; then printf "%s\n" "$out"; exit "$rc"; fi; printf "%s\n" "$out" | tail -2; echo "appbuild: 1 passed"'

# ── `winchk`：Windows 那半编不编得过 ──
# 本机门禁跑在 Linux 上，`#[cfg(windows)]` 的代码根本不参与编译；这里按 `x86_64-pc-windows-gnu` 把它 check 一遍。
# 用 `-gnu` 不用 `-msvc`：几个依赖用 cc-rs 编 C，msvc target 在 Linux 上找不到 `lib.exe`；全仓不按 `target_env` 分支，两者对本仓等价。
# `--all-targets`：生产段 ＋ test 档，与 `winchk-backend` 同一个面。买的是「编得过」：不买「行为对」（要真 Windows）、
#   不买 MSVC 上链接得起来（`check` 不链接）。依赖沙箱装了 mingw-w64 与该 target，没装就红在「找不到 target」。
# `--locked`：本格同时是 `src/frontend/shell/Cargo.toml ↔ Cargo.lock` 的对账落点（`doc_claim_registry` 逐字点名）；`winchk-backend` 不带，那是另一维。
run_gate winchk '不是数出来的数：`cargo check --all-targets --target x86_64-pc-windows-gnu` 只有绿/红两态。射程 = `-p monitor` 与文件窗口包 `-p cc-monitor-filewin` 两个包的生产段 ＋ test 档（与 `winchk-backend` 同一个面）；`src/backend` 与 `creds-core` 的 `--features harden` 本行盖不到' \
         bash -c 'cd src/frontend/shell && cargo check --locked --all-targets -p monitor -p cc-monitor-filewin --target x86_64-pc-windows-gnu 2>&1 && echo "winchk: 1 passed"'

# ── `winlink`：monitor 在 Windows 上链不链得起来 ──
# `winchk` 只 check 不链接。这里 `-p monitor` 的两个二进制（`cc-monitor` · `cc-monitor-filewin`）在 `x86_64-pc-windows-gnu` 上
#   真走一趟链接器（dev）。`[lib]` 只产 `rlib`；有人把 `cdylib` 加回来 ⇒ 红在 `export ordinal too large`。
# 只链不跑；`-gnu` 不是 `-msvc`；release 档不链；内嵌后端字节不在（`native-backend/` 没铺），链的是不带后端那一形。
run_gate winlink '不是数出来的数：`cargo build --bins --target x86_64-pc-windows-gnu`（dev）只有绿/红两态。射程 = `-p monitor` 的两个二进制（`cc-monitor` · `cc-monitor-filewin`）**真链接**一趟；⚠ 只链不跑（起不起得来要真机）· `-gnu` 不是 `-msvc` · release 那一档不链 · test 档不链（那一半归 `winchk` 的 `check`）' \
         bash -c 'cd src/frontend/shell && cargo build --locked -p monitor --bins --target x86_64-pc-windows-gnu 2>&1 && echo "winlink: 1 passed"'

# ── 真机 e2e：每一套一行 `run_e2e <套件>` ─────────────────────────────────────────
# 判法复用 `tests/e2e/assert-pass-floor.sh`：退出码 0 ＋ 收尾 `合计 PASS=<n> FAIL=0` ＋ `n > 0`；跑在无网沙箱里。
# 每套几条只住套件自己的输出里，这里与 CI 都不钉数。`GATE_ONLY` 里写套件短名（`ccm-cli`），或写 `e2e` 点名全部。
# 收据与裁决行里的规范名带前缀（`ccm tests/e2e/<套件>`）。
run_e2e() {
  local suite="$1"
  gate_wants "$suite" e2e || return 0
  local ex ju
  printf -v ex '%q ' gate_exec_cmd gate_nonet_e2e bash tests/e2e/assert-pass-floor.sh "$suite"
  printf -v ju '%q ' gate_judge_e2e "$suite"
  gate_cell "$suite" "$ex" "$ju"
}
gate_judge_e2e() {
  local suite="$1" out rc n
  gate_cell_read "ccm tests/e2e/$suite" || return 0
  out="$GATE_C_OUT"; rc="$GATE_C_RC"
  gate_ran "ccm tests/e2e/$suite" "$GATE_C_T0" "$GATE_C_T1"
  n="$(printf '%s' "$out" | grep -oE '合计 PASS=[0-9]+' | grep -oE '[0-9]+' | tail -1)"
  if [ "$rc" -ne 0 ]; then
    fails+=("ccm tests/e2e/$suite（退出码 $rc；PASS=${n:-<抓不到>}。诊断原文见下）")
    gate_diag "tests/e2e/$suite" "$out"
  elif [ -z "$n" ]; then
    fails+=("ccm tests/e2e/$suite（退出码 0 但抓不到「合计 PASS=<n>」—— 门没跑与门跑了结果是空\
在终端上一模一样，判不了，不许当成绿）")
    gate_diag "tests/e2e/$suite" "$out"
  else
    printf '  ok   %-14s %-22s PASS=%s（%s）\n' "ccm e2e" "$suite" "$n" "$(gate_took "ccm tests/e2e/$suite")"
  fi
}

# 探针⑩：`run_e2e` 的「退出码非零 ⇒ 红」还在判。给判法脚本两个参数 ⇒ 它在参数校验那一步 `exit 2`
#   并把参数原样回显（不跑 npm）；参数里带着 `合计 PASS=7` ⇒ 读数抓得到，掏掉退出码那条就会落到绿行。
gate_selftest_e2e() {
  local GATE_PROBE=1   # 同 `gate_selftest`：探针不受 `GATE_ONLY` 影响
  local probe
  probe="$(run_e2e '自检⑩ 合计 PASS=7 GATE-PROBE-J' 2>&1)"
  gate_assert_judged 自检⑩ "$probe" GATE-PROBE-J "run_e2e 的「退出码非零 ⇒ 红」"
}
gate_selftest_e2e

# e2e 的被测对象是后端二进制 ⇒ 先编出来（`backend` 那格的 `cargo test` 只编 test 版，不产 `debug/cc-monitor-backend`）。
# 它不进判定面：编不出来时每一套各自 fail-closed 红并说清原因。
gate_e2e_wanted() {
  if [ -z "$GATE_ONLY" ]; then return 0; fi
  local tok
  for tok in $GATE_ONLY; do
    case "$tok" in e2e) return 0 ;; esac
    if grep -qE "^run_e2e $tok\$" "$0"; then return 0; fi
  done
  return 1
}
gate_exec_prep() {
  gate_now_ms > "$GATE_K.t0"
  ( cd src/backend && gate_yield cargo build --bin cc-monitor-backend >/dev/null 2>&1 ) || true
  printf 0 > "$GATE_K.rc"
  gate_now_ms > "$GATE_K.t1"
}
gate_judge_prep() {
  local t0 t1
  t0="$(cat "$GATE_K.t0" 2>/dev/null)"; t1="$(cat "$GATE_K.t1" 2>/dev/null)"
  GATE_PREP_MS="$(( ${t1:-0} - ${t0:-0} ))"
  printf '  ·    %-14s %s\n' "e2e 前置" "build 后端二进制（e2e 那几套与 env-sandbox 的被测对象；$(gate_fmt_ms "$GATE_PREP_MS")）"
}

# ── 「后端 Rust」道：`src/backend` 那一棵（一把 target 锁）；先编 e2e 的被测对象，e2e 那几条道等它 ──
gate_lane 后端Rust

if gate_e2e_wanted; then
  gate_cell e2e-prep gate_exec_prep gate_judge_prep
else
  printf '  ·    %-14s %s\n' "e2e 前置" "跳过（GATE_ONLY 一套 e2e 都没点 ⇒ 不白编那一趟 cargo build）"
fi

run_gate backend '单包 src/backend，只有一行 test result ⇒ 最大值 = 合计' \
         gate_cargo_test_nonet src/backend
run_gate deadcode-backend '不是数出来的数：`src/backend` 的 `cargo check`（非 test，lib ＋ bin）的 `dead_code` 诊断零条才绿，有就逐条列出来' \
         gate_deadcode deadcode-backend src/backend cc_monitor_backend
run_gate clippy-backend '不是数出来的数：`cargo clippy --all-targets` 只有绿/红两态，射程 = `src/backend` 那一个 crate 的全部 target，与 `ci.yml` 的 `backend` job 那一步同一条命令（不带 `-D warnings` ⇒ 只有 deny 档的 lint 与编译错红）' \
         bash -c 'cd src/backend && out=$(cargo clippy --all-targets 2>&1); rc=$?; if [ "$rc" -ne 0 ]; then printf "%s\n" "$out"; exit "$rc"; fi; printf "%s\n" "$out" | tail -2; echo "clippy-backend: 1 passed"'

# ── `winchk-backend`：`src/backend` 在 Windows 上编不编得过 ──
# CI 用 `-msvc`、本格用 `-gnu`（沙箱里没有 zig，`ring` 的 build script 缺 `lib.exe`）。这一族错是 `cfg(unix)` 那一维
#   （`std::os::unix` · `libc::utimensat` 之类），与 ABI 无关，两者等价；MSVC ABI 专属的那一类只有 CI 有。
# `--all-targets` 是承重的：这类错会全落在 test 档、生产段照样编得过。不带 `--locked`，与 CI 那一步一致。
run_gate winchk-backend '不是数出来的数：`cargo check --all-targets --target x86_64-pc-windows-gnu` 只有绿/红两态。射程 = `src/backend` 这一个 crate 的**生产段 ＋ test 档**（云端那 10 个错全在 test 档，所以 `--all-targets` 是承重的）。⚠ 本格用的是 `-gnu`，云端用的是 `-msvc`（沙箱里没有 zig，`ring` 的 build script 缺 `lib.exe`）⇒ **MSVC ABI 专属的那一类本行盖不到**；`src/frontend/shell` 那棵树由上面 winchk 那一格盖' \
         bash -c 'cd src/backend && cargo check --all-targets --target x86_64-pc-windows-gnu 2>&1 && echo "winchk-backend: 1 passed"'

# ── `muslbuild`：远端 Linux 那一格 —— 两个 musl target 编得出静态字节 ──
# 用 `cargo zigbuild`，版本与 `release.yml` 对齐（`release-gate` 两向对拍）：版本一漂，本格的绿就不代表发版那趟会绿。
# 买不到「在真远端上跑得起来」，也不编 test 档（只编 bin）。
run_gate muslbuild '不是数出来的数：两个 musl target（`x86_64` ＋ `aarch64`）各一趟 `cargo zigbuild`，只有绿/红两态。⚠ 买的是「编得出静态字节」，不买「在真远端上跑得起来」、不买 test 档（只编 bin）。每个 arch 的字节里要认得出 mimalloc（远端版换过分配器：源码那一半由 `allocator_guard` 判，字节这一半本格判，认法是它自带的报错前缀 `mimalloc: `）。⚠ 工具链版本与 `release.yml` 对齐（zig 0.14.0 / cargo-zigbuild 0.23.0）—— 版本一漂，本格的绿就不再代表发版那趟会绿' \
         bash -c 'cd src/backend && n=0; for t in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do cargo zigbuild --target "$t" >/dev/null || { echo "musl: $t 编不过"; exit 1; }; grep -qa "mimalloc: " "../../.build/backend/$t/debug/cc-monitor-backend" || { echo "musl: $t 的字节里认不出 mimalloc（分配器没换上）"; exit 1; }; n=$((n+1)); done; printf "muslbuild: %s passed（两个 arch 各一趟 cargo zigbuild，字节里都认得出 mimalloc，zig $(zig version)）\n" "$n"'

# ── 「前端」道（与下面「覆盖率」道）：读 `src/frontend/ui/generated/` 的那几格，等 `cargo` 与 `backend` 两格写完它 ──
gate_lane 前端 after cargo backend

# ── `generated`：改了 Rust 不跑生成，这里红 ──
# 与 `ci.yml` 那条「生成物必须最新」同形：`git diff` 已提交的 `src/frontend/ui/generated/`。
# 位置是承重的：必须在 `cargo` 与 `backend` 两格都跑完之后（「前端」道 after 它俩）—— 两格的 `cargo test --lib` 里都有 ts-rs 导出测试，跑过它们生成物才按当前 Rust 源重写过；
#   排在前面判的是没重写的树，恒绿；与它们并发则读到截了一半的文件（ts-rs 截断再写）。
# 看不见的：全新的生成物文件（untracked，由 `generated-boundary-guard.vitest.ts` 的目录清单接）· 内容对不对 · 有没有提交。
# 本格不走 `run_gate`，`gate_wants` / `gate_cell` 手接；漏接会让收据少一格，`K-G4C-gate-receipt.py` 的两向相等当场分叉。
gate_exec_generated() {
  gate_now_ms > "$GATE_K.t0"
  ( gate_yield git diff --quiet --exit-code -- src/frontend/ui/generated/ )
  printf '%s' "$?" > "$GATE_K.rc"
  gate_now_ms > "$GATE_K.t1"
  git diff --stat -- src/frontend/ui/generated/ > "$GATE_K.out" 2>&1
}
gate_judge_generated() {
  local gen_rc
  gate_cell_read generated || return 0
  gen_rc="$GATE_C_RC"
  gate_ran generated "$GATE_C_T0" "$GATE_C_T1"
  case "$gen_rc" in
    0) printf '  ok   %-14s %s\n' "generated" "与 Rust 源一致（cargo 与 backend 两格跑完之后再判的；$(gate_took generated)）" ;;
    1)
      printf '  FAIL %-14s %s\n' "generated" "src/frontend/ui/generated/ 与 Rust 源不一致："
      printf '%s\n' "$GATE_C_OUT"
      fails+=("generated（改了带 ts_rs::TS 的类型 ⇒ 跑 npm run gen:types 并把 src/frontend/ui/generated/ 一起提交）")
      ;;
    *)
      # 退出码既不是 0 也不是 1（如 128：不在 git 仓里）⇒ **判不了**。不许当成绿。
      fails+=("generated（git diff 退出码 $gen_rc —— 判不了，不许当成绿）")
      ;;
  esac
}
if gate_wants generated; then
  gate_cell generated gate_exec_generated gate_judge_generated
fi

# ── `tsc`：类型检查（`npm run build` 的前一半）──
# `npm` 那格的 tsx 与 vitest 只转译、不做类型检查，纯类型错误在那里一条都不红。`vite build` 与 `cargo tauri build` 本格盖不到。
# 第二条判定：程序面没被掏空 —— 空程序上 tsc 也退 0。`--listFiles` 真读进的仓内 `.ts`/`.tsx`/`.mts` 份数
#   与盘上份数同一趟现打、恒等对账，一个都不写死。
run_gate tsc '不是「几条断言过了」：这个数是这一趟真读进 tsc 程序的仓内 `.ts`/`.tsx`/`.mts` 份数，并与盘上 `src` ＋ `tests` 下现打的份数恒等对账（与 `tsconfig.json` 的 include 同一个面）—— include 被收窄时两边不再相等。⚠ 只判类型（`npm run build` 的前一半）；`vite build` 与 `cargo tauri build` 那两段、以及仓根那几份不在 include 里的 `.ts`（`vite.config.ts` / `vitest.config.ts`），本行一概盖不到' \
         bash -c 'out=$(node_modules/.bin/tsc --noEmit --listFiles 2>&1); rc=$?; \
want=$(find src tests -type f \( -name "*.ts" -o -name "*.tsx" -o -name "*.mts" \) | wc -l | tr -d " "); \
got=$(printf "%s\n" "$out" | grep -v "/node_modules/" | grep -cE "/(src|tests)/.*\.(ts|tsx|mts)$"); \
printf "tsc: 盘上现打 %s 份仓内 .ts，这一趟真读进程序的 %s 份\n" "$want" "$got"; \
printf "%s\n" "$out" | grep -E "error TS" | head -60; \
if [ "$rc" -ne 0 ]; then printf "tsc: 退出码 %s —— 类型没编过。它就是 npm run build 的第一步，红着这棵树发不出产物\n" "$rc"; exit "$rc"; fi; \
if [ "$got" -ne "$want" ]; then printf "tsc: 真读进程序的 %s 份 != 盘上现打的 %s 份 —— tsconfig 的 include 被掏空或收窄了。空程序上 tsc 退出码也是 0，「一个文件都没检」与「全检过了」在退出码上一模一样，所以一律按红记\n" "$got" "$want"; exit 1; fi; \
printf "tsc: %s passed（仓内 %s 份 .ts 全部过 tsc --noEmit；两个数同一趟现打）\n" "$got" "$want"'

run_gate npm '`npm test` 串起来的各套件里，只有 vitest（`test:dom`）那一套的数大，取最大值 ⇒ 这个数是 `test:dom` 的；只打「all X tests passed」的 tsx 套件「跑了 0 个」这一格守不住（失败仍由 && 链的退出码守）' \
         gate_vitest_share npm test

# ── `coverage`：`ci.yml` 的 `frontend` job 里那两步覆盖率 ─────────────────────
# 覆盖率逐文件地板（`tests/scripts/assert-coverage-floors.mjs`）点名具体文件：文件被删或改名而清单还点着它，本格红。
# 按步骤名从 `ci.yml` 现取原样跑（`audit` 同法，在「静态」道），地板与清单只住它们自己的文件。
# 自己一条道，与 `npm` 那格同时跑：两趟 vitest 都读写 `node_modules/.vite` 下的结果缓存，而 vitest 读它不设防
#   （读到另一趟写了一半的 JSON 当场抛）⇒ 并跑时本格在一层挂载命名空间里换一个自己的 `node_modules/.vite`。
gate_private_vite() {
  local t rc
  if [ "$GATE_PAR" != 1 ]; then "$@"; return; fi
  t="$(mktemp -d "$GATE_CELL_DIR/vite.XXXXXX")" || return 1
  mkdir -p node_modules/.vite || return 1
  export -f gate_ci_steps gate_ci_step_body
  bwrap --dev-bind / / --bind "$t" "$PWD/node_modules/.vite" --die-with-parent -- bash -c '"$@"' _ "$@"
  rc=$?
  rm -rf -- "$t"
  return "$rc"
}
gate_lane 覆盖率 after cargo backend
run_gate coverage '这一趟 vitest（带 v8 覆盖率）真跑过的条数：`ci.yml` 的 `coverage floor (vitest jsdom)`（`npm run coverage`，`vitest.config.ts` 里的全局阈值）＋ `coverage per-file floors + zero-coverage ratchet`（逐文件地板与零覆盖棘轮）两步原样跑。⚠ 与 `npm` 那格是同一批 vitest 文件再跑一遍（带插桩，慢一截）；覆盖率只量 `src/**/*.ts`，tsx 套件与 Rust 一概不进分母' \
         gate_vitest_share gate_private_vite gate_ci_steps coverage "coverage floor (vitest jsdom)" "coverage per-file floors + zero-coverage ratchet"

# 18 套摊成几条道（按热缓存时长大致拉平）；各套各在自己的网络命名空间与 `/tmp` 里，互不相见。
# `local-backend` 在壳里跑 `cargo test --lib`，等 `cargo` 那格编完再起，免得两条道抢壳的 target 锁。
gate_lane e2e-1 after e2e-prep
run_e2e cc-spawn-uplift
run_e2e backend-tmux-late-server
run_e2e tmux-target
run_e2e restart
gate_lane e2e-2 after e2e-prep
run_e2e backend-sessions-rewatch
run_e2e backend-cc-bus
run_e2e resume
run_e2e cc-bus-queue-drain
run_e2e graylight-frames
run_e2e resume-frames
gate_lane e2e-3 after e2e-prep
# `backend-gate2` · `local-backend` 按环境显式分支：版本门 / 没铺 `embedded-backends/` 的格记 SKIP 并说原因；
#   未登记的 SKIP 由套件自己判红（收尾 `[ "$skip" -eq 0 ] || exit 1`）。
run_e2e backend-gate2
run_e2e ccm-cli
run_e2e inbound-frames
run_e2e ccm-contract-parity
# `p3t-local-tmux` 跑在 `bash -lic` 里、依赖本机登录 shell：套件自己的 fail-closed 前置罩住（解析到的不是本树那个二进制就 ABORT）。
run_e2e p3t-local-tmux
run_e2e ccm-print-parity
run_e2e backend-fork
gate_lane e2e-4 after e2e-prep cargo
run_e2e local-backend

gate_lane env-sandbox after e2e-prep

# ── `env-sandbox`：在一台假开发机上，带着指向「真目录」「真口」的会话环境起一趟门禁 ─────────────
# 假开发机 = bwrap 撑着的一个新网络命名空间（命令用 nsenter 进去），默认中转口（`relay-route-core` 的 `PORT`）在里面听着；
# 「真目录」是一棵临时假家（日志目录 ＋ 哨兵 stderr.log ＋ 常驻后端的门牌目录），另有现找的三个空闲口。
# 带着指向它们的 `CCM_*` / `CLAUDE_CONFIG_DIR` / `ANTHROPIC_BASE_URL` / `TMUX` 起内层门禁，跑 `backend-tmux-late-server` · `restart` 两套。
# 判：① 内层门禁绿；② 假家前后逐份（路径 · 大小 · mtime）相同；③ 那三个口全程没有监听（`ss` 每 0.1 秒看一次）；
#   ④ 假开发机上 TCP 主动建连计数（`/proc/<pid>/net/snmp` 的 `ActiveOpens`）前后不变、假真口零连接。
# CI 上 e2e 不包 ⇒ 不造假开发机、④ 不判。
gate_env_sandbox() {
  local d p1 p2 p3 before after out rc w lis hold="" real="" rport="" o0="" o1="" hits="" ns=()
  command -v ss >/dev/null 2>&1 || { printf 'env-sandbox: 这台机器上没有 ss —— 看不了监听，判不了\n'; return 1; }
  if [ -n "$GATE_NONET_WHY" ]; then printf 'env-sandbox: 无网沙箱用不了（%s）—— 判不了\n' "$GATE_NONET_WHY"; return 1; fi
  d="$(mktemp -d "${TMPDIR:-/tmp}/gate-env-sandbox.XXXXXX")" || return 1
  mkdir -p "$d/cc/logs/backend" "$d/claude"
  printf 'sentinel\n' > "$d/cc/logs/backend/stderr.log"
  mkdir -p "$d/cc/run"
  if [ "${#GATE_NONET[@]}" -gt 0 ]; then
    rport="$(sed -nE 's/^pub const PORT: u16 = ([0-9]+);$/\1/p' src/common/relay-route-core/src/lib.rs)"
    case "$rport" in ''|*[!0-9]*) printf 'env-sandbox: relay-route-core 里抠不出默认中转口 —— 判不了\n'; rm -rf -- "$d"; return 1 ;; esac
    # 占位进程不接本格的输出管道（否则命令替换等不到 EOF）；bwrap 死了它不跟着死，收尾按它自己的 pid 杀。
    "${GATE_NONET[@]}" sh -c 'echo $$ > "$1"; exec sleep infinity' _ "$d.ns" </dev/null >/dev/null 2>&1 &
    for _ in $(seq 1 50); do [ -s "$d.ns" ] && break; sleep 0.1; done
    hold="$(cat "$d.ns" 2>/dev/null)"
    ns=(nsenter -t "${hold:-0}" -U -n --preserve-credentials)
    "${ns[@]}" python3 -c 'import socket, sys
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", int(sys.argv[2]))); s.listen(64)
log = open(sys.argv[1], "a", buffering=1); log.write("ready\n")
while True:
    c, peer = s.accept(); log.write("accept %d\n" % peer[1]); c.close()' "$d.real" "$rport" </dev/null >/dev/null 2>&1 &
    real=$!
    for _ in $(seq 1 50); do grep -q ready "$d.real" 2>/dev/null && break; sleep 0.1; done
    o0="$(gate_tcp_opens "$hold")"
  fi
  read -r p1 p2 p3 < <(${ns[@]+"${ns[@]}"} python3 -c 'import socket
s = [socket.socket() for _ in range(3)]
for x in s: x.bind(("127.0.0.1", 0))
print(*[x.getsockname()[1] for x in s])')
  before="$(find "$d" -printf '%P %s %T@\n' | sort)"
  ( while :; do ${ns[@]+"${ns[@]}"} ss -ltnH 2>/dev/null | awk '{print $4}' | grep -E ":($p1|$p2|$p3)\$"; sleep 0.1; done ) > "$d.listen" 2>/dev/null &
  w=$!
  out="$(${ns[@]+"${ns[@]}"} env CCM_RESIDENT=1 CCM_DATA_DIR="$d/cc" CCM_RELAY_PORT="$p2" \
             CCM_BACKEND_STDERR_LOG="$d/cc/logs/backend/stderr.log" CCM_HISTORY_METADATA="$d/cc/history.json" \
             CCM_APIKEY_CREDENTIALS="$d/cc/apikey.json" CLAUDE_CONFIG_DIR="$d/claude" \
             ANTHROPIC_BASE_URL="http://127.0.0.1:$p3" TMUX="$d/tmux.sock,1,0" \
             GATE_ONLY="backend-tmux-late-server restart" GATE_RECEIPT="$d.receipt.json" \
             bash tests/scripts/gate.sh 2>&1)"; rc=$?
  kill "$w" 2>/dev/null; wait "$w" 2>/dev/null
  if [ -n "$rport" ]; then
    o1="$(gate_tcp_opens "$hold")"
    hits="$(grep -c '^accept' "$d.real")"
    kill "$real" ${hold:+"$hold"} 2>/dev/null; wait "$real" 2>/dev/null
  fi
  after="$(find "$d" -printf '%P %s %T@\n' | sort)"
  lis="$(sort -u "$d.listen" | tr '\n' ' ')"
  rm -rf -- "$d" "$d.listen" "$d.receipt.json" "$d.ns" "$d.real"
  if [ "$before" != "$after" ] || [ -n "${lis// /}" ]; then
    diff <(printf '%s\n' "$before") <(printf '%s\n' "$after") | head -20
    printf 'env-sandbox: 会话环境漏进去了 —— 假家有写入（上面的 diff）、或这几个口上出现过监听 [%s]\n' "${lis% }"
    return 1
  fi
  if [ -n "$rport" ] && { [ -z "$o0" ] || [ -z "$o1" ] || [ "$o0" != "$o1" ] || [ "$hits" != 0 ]; }; then
    printf 'env-sandbox: 测试进程连到了假开发机上 —— TCP 主动建连 %s → %s，假真口 %s 收到 %s 个连接（本机的 e2e 没被关进无网沙箱）\n' \
      "${o0:-?}" "${o1:-?}" "$rport" "${hits:-?}"
    return 1
  fi
  if [ "$rc" -ne 0 ] || ! printf '%s\n' "$out" | grep -q '^GATE: PARTIAL'; then
    printf '%s\n' "$out" | tail -30
    printf 'env-sandbox: 内层门禁退出码 %s、没有 PARTIAL 裁决 —— 判不了\n' "$rc"
    return 1
  fi
  if [ -n "$rport" ]; then
    printf 'env-sandbox: 1 passed（假家零写入、三个口零监听、假开发机上零连接（假真口 %s 收到 0 个）；内层门禁绿）\n' "$rport"
  else
    printf 'env-sandbox: 1 passed（假家零写入、三个口零监听；内层门禁绿。CI 上 e2e 不包，零连接不判）\n'
  fi
}
# 一个网络命名空间里内核记下的 TCP 主动建连数（`$1` 是住在那个命名空间里的一个进程）。
gate_tcp_opens() {
  awk '/^Tcp:/ { if (!h) { for (i = 1; i <= NF; i++) if ($i == "ActiveOpens") c = i; h = 1 } else if (c) print $c }' "/proc/$1/net/snmp" 2>/dev/null
}
run_gate env-sandbox '不是数出来的数：在一台假开发机（新网络命名空间）上带着指向假「真目录」「真口」的会话环境起一趟内层门禁（两套起后端 / ccm 的 e2e），零写入 ＋ 零监听 ＋ 零连接才绿' \
         gate_env_sandbox

gate_lane weak-net

# ── `weak-net`：弱网台架（建镜像 · 跑台架）──────────────────────────────────────────────────
# 台架全程在 docker 里自建网络造网况（宿主网卡不动）；要本机有 docker 与 `NET_ADMIN`。镜像已在就跳过建镜像。
# 判法同 `run_e2e`：退出码 0 ＋ `合计 PASS=<n> FAIL=0` ＋ `n > 0`（`tests/e2e/weak-net/assert-floor.sh`），不钉条数。
gate_weaknet() {
  local out rc
  bash tests/e2e/weak-net/build-image.sh || { printf 'weak-net: 建台架镜像失败\n'; return 1; }
  out="$(bash tests/e2e/weak-net/assert-floor.sh 2>&1)"; rc=$?
  printf '%s\n' "$out"
  [ "$rc" -eq 0 ] || return "$rc"
  printf 'weak-net: %s passed（台架跑完、FAIL=0）\n' "$(printf '%s' "$out" | grep -oE '合计 PASS=[0-9]+' | grep -oE '[0-9]+' | tail -1)"
}
run_gate weak-net '台架跑完且 FAIL=0（四维网况 ＋ SSH，改前/改后两个读数）。⚠ 要 docker；只量容器里自建网络上的网况，真远端、真 Windows 一概不在' \
         gate_weaknet

# 各道同时起跑、等齐，再按上面的顺序逐格判。
gate_lanes_run

# ── `tmux-default`：缺省 tmux server 全程没人碰 ─────────────────────────────────────────────
# 文件头那层挂载把 `/tmp/tmux-<uid>` 换成了这一趟的私有目录，里面的 `default` 是起跑前放好的一台假 server
#（会话 `gate-fake` ＋ 段外三格、后端段里一格「死后端」的已知钩子）。各格里该用的都是各自的私有 server，
# 谁裸调 `tmux`（不带 `-L` / `-S`）就连到这台上。判：① 默认位置上的 socket 仍是它（同一个 inode，隔离还在）；
# ② 它的 `-v` 日志里，起跑那两个客户端之外一个新客户端都没有；③ 快照（pid · 全局钩子 · 会话 · 会话选项与钩子）
# 与起跑时逐字相同 —— 后端装钩子、清死槽、给会话打标都会改它。外来连接按「落在哪一格 · 发的什么命令」归并印出来。
# 只读假的那台；用户真那台在挂载外面，本格碰不到也不去读。放在最后一格：前面各格都跑完了再判。
gate_tmux_default() {
  local s n now base t at cwd w g a b nm
  [ -n "${GATE_TMUX_HOME:-}" ] || { printf 'tmux-default: 没换 tmux 目录（CI 上不换）—— 没有假的缺省 server，判不了\n'; return 1; }
  s="$GATE_TMUX_HOME/default"
  if [ "$(stat -c %d:%i "/tmp/tmux-$(id -u)/default" 2>/dev/null)" != "$(stat -c %d:%i "$s" 2>/dev/null)" ]; then
    printf 'tmux-default: /tmp/tmux-%s/default 已经不是起跑时那台假 server（被删 / 换掉了）\n' "$(id -u)"; return 1
  fi
  base="$(cat "$GATE_TMUX_HOME.log/base")"
  n="$(cat "$GATE_TMUX_HOME.log"/tmux-server-*.log 2>/dev/null | grep -cE '^[0-9.]+ new client ')"
  if [ "$n" != 2 ]; then
    printf 'tmux-default: 缺省 server 上来过 %s 个外来客户端（起跑那 2 个之外）：\n' "$(( n - 2 ))"
    cat "$GATE_TMUX_HOME.log"/tmux-server-*.log |
      awk '$2 == "new" && $3 == "client" { if (++c > 2) tm[$4] = $1 }
           $4 == "IDENTIFY_CLIENTPID" && ($3 in tm) { who["client-" $5] = tm[$3]; delete tm[$3] }
           $2 == "message:" && ($3 in who) { t = who[$3]; $1 = $2 = $3 = $4 = ""; print t, substr($0, 5) }' |
      while read -r t cwd; do
        at="${t%%.*}${t#*.}"; at="$(( ${at:0:13} ))"; w=""
        # 并发跑时一个时刻可能落在几格的起止里：全列。
        for g in "${GATE_WIN[@]}"; do
          read -r a b nm <<<"$g"; if [ "$at" -ge "$a" ] && [ "$at" -le "$b" ]; then w="${w:+$w | }$nm"; fi
        done
        w="${w:-（不在任何一格里）}"
        printf '  · %s · %s\n' "$w" "${cwd:0:48}"
      done | sort | uniq -c | head -80
    return 1
  fi
  now="$(gate_tmux_snap "$s")"
  if [ "$now" != "$base" ]; then
    diff <(printf '%s\n' "$base") <(printf '%s\n' "$now") | head -30
    printf 'tmux-default: 缺省 server 的钩子 / 会话 / 会话选项与起跑时不同（上面的 diff）\n'; return 1
  fi
  printf 'tmux-default: 1 passed（缺省 server 上零外来连接，钩子 %s 格、会话、会话选项与起跑时逐字相同）\n' \
    "$(printf '%s\n' "$base" | grep -c '^session-')"
}
run_gate tmux-default '不是数出来的数：整趟门禁跑完，挂载换进来的那台假的缺省 tmux server 零外来连接、钩子与会话原样不动才绿。⚠ 只盖门禁里跑的东西；单跑的 cargo test / e2e 不经门禁就没有这层挂载' \
         gate_tmux_default

# ── 射程：`GATE: OK` 那一行不对什么负责 ──────
# `键|说明`，条数现算。这是一张列不全的黑名单：保证列出来的这几条不会悄悄变成没人守的散文。只在 OK / PARTIAL 两支印。
GATE_BLIND=(
  "windows-runner|Windows runner 上才犯的那一族 —— 本门禁的 npm / tsc / e2e 全跑在 Linux 上，路径分隔符恒是 /，这一形本机在构造上红不了"
  "ci-job-shape|.github/workflows/*.yml 里那些 job 自己的形状 —— 装了哪条工具链、runner 是谁、缓存与 needs 怎么连、每一步的 if 条件。Linux 那几个 job 的命令就是调本脚本（GATE_ONLY），命令只住这里；但 job 的环境（apt 装了什么、runner 是谁）本门禁不判。release-gate 割走了 release.yml 的一部分（触发器、发布闸、产字节那条路、BUILD_ID 的抠法、工具链版本），上传清单与校验和算哪几份也对上了资产登记表；其余每一步（打包 · artifact 传递）仍然没人看；那些切片买的也只是「盘上这份文本满足这几条」"
  "msvc-abi|MSVC ABI 专属的那一类跨平台编译问题 —— 两格 Windows 交叉检查用的都是 -gnu（沙箱里没有 zig，ring 的 build script 缺 lib.exe）。只在 msvc 上才犯的毛病本门禁盖不到"
  "did-ci-actually-run|云端那条流水线到底跑没跑、绿没绿 —— 本门禁一次 gh run view 都不做。GATE: OK 说的是这棵树在本机这几格上的样子，不是它在云端的样子。ci.yml 的触发器只有 push(main/v*) 与 pull_request，而本仓不推送 ⇒ 那几个 job 在 GitHub runner 上从未起过；tests/hooks/pre-push 同理"
)
gate_print_blind() {
  printf 'GATE: 射程 —— 上面那行只对它自己那几格负责；下面这 %s 件事**本门禁不看**：\n' "${#GATE_BLIND[@]}"
  local item
  for item in "${GATE_BLIND[@]}"; do
    printf '  不看  %-20s %s\n' "${item%%|*}" "${item#*|}"
  done
}

# 裁决行里的格名用短名（e2e 那批去掉 `ccm tests/e2e/` 前缀）；格数与名单都从 `GATE_RAN` 现算，不抄。
gate_short_names() {
  local n out=""
  for n in ${GATE_RAN[@]+"${GATE_RAN[@]}"}; do out="${out:+$out · }${n#ccm tests/e2e/}"; done
  printf '%s' "$out"
}
# 墙钟：这一趟合计 ＋ 最慢的几格（逐格的数在每一格的绿行上、也在收据的 `ms` 里）。
gate_print_timing() {
  local total=$(( $(gate_now_ms) - GATE_T0 )) n ms line=""
  while IFS=$'\t' read -r ms n; do
    line="${line:+$line · }$n $(gate_fmt_ms "$ms")"
  done < <(for n in ${GATE_RAN[@]+"${GATE_RAN[@]}"}; do printf '%s\t%s\n' "${GATE_MS[$n]:-0}" "${n#ccm tests/e2e/}"; done | sort -rn | head -8)
  printf 'GATE: 墙钟 —— 合计 %s（其中 e2e 前置 %s）；最慢的几格：%s\n' "$(gate_fmt_ms "$total")" "$(gate_fmt_ms "$GATE_PREP_MS")" "${line:-（一格都没跑）}"
}

echo
gate_print_timing
# 拼错的 `GATE_ONLY` 是红，不是「少跑一格」。
gate_check_only
# 落收据。裁词先算再落：`OK` 只在一格没红且一格没跳时给。
if [ "${#fails[@]}" -ne 0 ]; then
  gate_verdict=FAIL
elif [ "${#GATE_SKIPPED[@]}" -ne 0 ]; then
  gate_verdict=PARTIAL
else
  gate_verdict=OK
fi
gate_write_receipt "$gate_verdict"

if [ "${#fails[@]}" -eq 0 ] && [ "${#GATE_SKIPPED[@]}" -ne 0 ]; then
  # 刻意不印 `GATE: OK`：「跑过的全绿」与「全都跑过」是两句话。
  printf 'GATE: PARTIAL —— 跑过的那 %s 格全绿，但 GATE_ONLY 挡掉了 %s 格：%s\n' \
    "${#GATE_RAN[@]}" "${#GATE_SKIPPED[@]}" "${GATE_SKIPPED[*]}"
  echo "**这不是 GATE: OK，不许拿它出货。** 出货要的是不带 GATE_ONLY 的那一趟。"
  gate_print_blind
  exit 0
fi
if [ "${#fails[@]}" -eq 0 ]; then
  # 格数与点名从这一趟真判过的格现算。
  printf 'GATE: OK —— %s 格全绿（%s），可以出货\n' "${#GATE_RAN[@]}" "$(gate_short_names)"
  gate_print_blind
  exit 0
fi
# 分隔符不走 `IFS='；'`：IFS 按字节认，`；` 是 3 个字节，两格以上同红时会拼出坏字节。
joined=""
for f in "${fails[@]}"; do joined="${joined:+$joined；}$f"; done
printf 'GATE: FAIL —— %s\n' "$joined"
echo "**别提交**。先修，再重跑本脚本。"
exit 1
