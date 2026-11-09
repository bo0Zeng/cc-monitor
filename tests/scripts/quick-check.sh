#!/usr/bin/env bash
# 快检：交给门禁之前的那一圈。不拿门禁锁、不代替门禁。
# 用法：在仓里任一处 `bash tests/scripts/quick-check.sh [--plan | --only <段>]`
#   恒跑（便宜）：两棵 Rust 树的排版 · tsc · 门禁里只读盘上文本的那几格（名单是 ci.yml 里 `static-gate` 那个 job 的 GATE_ONLY，经 gate.sh 跑）·
#     后端那棵改到的包跑 clippy（照门禁 `clippy-backend` 的参数；系统钟 · 定时器那几类 lint 定成 deny，测试拦不住）·
#     壳那棵改到的包同样跑 clippy（clippy-shell：壳 · 文件窗口 · 通信那三个包；禁用名单住 `src/frontend/clippy.toml` · `src/comms/clippy.toml`）·
#     两棵树的库在 Windows 目标上 `cargo check`（win-check：只在本机、目标没装算红）·
#     凭据那份共享 crate 开 `harden` 那一档（creds-harden：壳那棵不开它，后端那棵不跑它的测试）· 末尾生成物有没有落在提交外（generated-drift.sh）。
#   重的四段：后端那棵 `cargo test --workspace --lib`（只编各包的测试库；bin 与 bench 由 clippy 守编译、门禁与 CI 跑）· 壳那棵 `cargo test --workspace --lib` · 整套 vitest ·
#     手机那一套（ktlint · detekt · 各模块 JVM 单测，经 heavy.sh、gradle 带 --no-daemon）。这棵树（连没提交的）相对起点改到的路径
#     交给 gate-affected/plan.py 照 gate-affected/map 判（与 gate-affected 同一张表）：挑上那一段的格（backend · cargo · npm · mobile）才跑；
#     前三段在表里恒跑（@suites：它们都扫全仓），手机那一段按改动挑。表不认识的路径、找不到起点、plan.py 停了 ⇒ 四段全跑。
#     起点是 base.sh 印的那一个（分支上游，没设就 origin/main）。
#   `--plan` 只印四段跑不跑、为什么；`--only <段>` 只跑那一段（照挑；`win-check` 那一格也认）。
#   「加了子命令要打版本号」那一条在各路树上必红（版本号合并时打：bump-build-id.sh）⇒ 单列成「待打版本号」，不算红。
# 环境：每一步经 session-scrub.sh 摘掉会话继承来的变量（表在 session-vars）；HOME 与 TMUX_TMPDIR 指到 `.build/` 下一次性沙箱目录。
#   cargo 那几步的 TMPDIR 是外面临时目录下一个一次性的空目录，收尾判它还是空的（tmp-left：测试进程退出时删自己的沙箱根，见 test-home）。
#   cargo / rustup / npm 的家照外面的 CARGO_HOME / RUSTUP_HOME / npm_config_cache，没设就是真家目录下的默认位置（换 HOME 之前定下）。
#   手机工具链：外面给了 JAVA_HOME 与 ANDROID_HOME 就用；没给就照 src/mobile/scripts/env.sh 探真家目录下的 android-dev。
# 红了：每条失败测试的 panic 原话印出来；整趟输出存 `.build/quick-check/<时刻>.log`。
# 逐条耗时：cargo 那几段存 `.build/quick-check/cargo-times.tsv`（从慢到快，收尾印最慢三条），vitest 存 `vitest-times.json`。
# 编译锁：外面给了 `CCM_BUILD_LOCK`（一份仓外的锁脚本，`<脚本> <命令…>` 排上队再起命令）⇒ 编译那几段（后端 · clippy · 壳 ·
#   creds-harden 的 cargo，手机那一段的 gradle）各经它起，排队只排这几段；cargo fmt · tsc · 静态格 · vitest 不经它。没给 ⇒ 不套锁照跑。
#   cargo 那四段起锁时另带 `CCM_BUILD_REMOTE_OK`：这一段可以派去别的机器跑（锁脚本认它才派，不认 ⇒ 照旧本机）；值是这一段
#   写在同步的树以外的几个目录（沙箱家 · 沙箱 tmux 目录 · cargo 的临时目录，空格分隔）：那边照同一路径建好，跑完把里面剩的拉回这几个目录，
#   tmp-left 照旧在本机判。手机那一段不带（gradle 的工具链只认本机）。
#   整趟外派：`bash "$CCM_BUILD_LOCK" env -u CCM_BUILD_LOCK bash tests/scripts/quick-check.sh`（摘掉锁变量：那一趟里各段不再排一次）。
#   编译调度在远端起命令时设 `BUILDSCHED_TAG`（本机没有）⇒ 手机与 win-check 两格不跑、各印一行「远端跳过，本机单跑」
#   （远端没装 Android 工具链与 Windows 目标），本机另跑 `--only mobile` · `--only win-check` 补上；其余各格照跑。
# 收尾修剪 `.build`：每棵 cargo 树每一档的 `incremental/` 下同一个 crate 的会话目录、`deps/` 下同名的测试二进制（无扩展名的可执行文件，
#   连同它的 `.d`）按名字成组，比同组最新那一份旧出 6 小时的删掉。同一个 crate 同时有几种现役变体（lib · lib 测试 · clippy · 各 bin），
#   同一趟编出来、时刻挨着，所以按窗口留、不只留最新一份；锁或特性集一变换了哈希的旧变体落在窗口外。`.fingerprint` · `build/` · rlib 不碰。
set -uo pipefail
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git rev-parse --show-toplevel) || exit 2
cd "$root" || exit 2
mode=all; only=
case "${1:-}" in
  --only) only=${2:-}; shift 2 || shift
    case "$only" in backend|shell|vitest|mobile|win-check) ;; *) echo "quick-check --only 要 backend · shell · vitest · mobile · win-check 之一" >&2; exit 2 ;; esac ;;
  --plan) mode=plan; shift ;;
  '') ;;
  *) echo "quick-check：不认识的参数 $1（只认 --plan · --only <段>）" >&2; exit 2 ;;
esac
# 后端那棵 workspace 的包 → 编进它的路径（git 路径规格，空格分隔）
#   每个后端成员一行（路径照它的目录）；主 crate 那一行排除拆出去的那几个包的目录。
# shellcheck disable=SC2034  # 两张包表都经 clippy_step 的 nameref 读
clippy_pkgs=("cc-monitor-backend:src/backend/ tests/backend/ :(exclude)src/backend/deploy/ :(exclude)src/backend/platform/ :(exclude)src/backend/common/ :(exclude)src/backend/plugin/ :(exclude)src/backend/test-meters/ :(exclude)src/backend/agents/ :(exclude)src/backend/relay/ :(exclude)src/backend/stderr_log/ :(exclude)src/backend/wire/ :(exclude)src/backend/dial/ :(exclude)src/backend/host/ :(exclude)src/backend/history/ :(exclude)src/backend/accounts/ :(exclude)src/backend/assets/ :(exclude)src/backend/control/ :(exclude)src/backend/observe/ :(exclude)src/backend/footprint/ :(exclude)src/backend/plan/ :(exclude)src/backend/faces/"
  "backend-faces:src/backend/faces/ tests/backend/faces/"
  "backend-plan:src/backend/plan/ tests/backend/plan/"
  "backend-footprint:src/backend/footprint/ tests/backend/footprint/"
  "backend-observe:src/backend/observe/ tests/backend/observe/"
  "backend-control:src/backend/control/ tests/backend/control/"
  "backend-assets:src/backend/assets/ tests/backend/assets/ tests/backend/agents/on_assets_tests.rs tests/backend/accounts/manage/library_any_os_tests.rs tests/backend/common/door_wire_tests.rs"
  "backend-accounts:src/backend/accounts/ tests/backend/accounts/ tests/backend/relay/creds_guard.rs tests/backend/relay/egress_tests.rs tests/backend/relay/member_tests.rs tests/backend/relay/upstream_selection_guard.rs tests/backend/relay/wire_golden.rs"
  "backend-history:src/backend/history/ tests/backend/history/ tests/backend/lineage_tests.rs tests/backend/agents/on_history_tests.rs"
  "backend-dial:src/backend/dial/ tests/backend/dial/ tests/backend/dial_tests.rs tests/backend/dial_probe_tests.rs tests/backend/dial_known_hosts_tests.rs tests/backend/dial_known_hosts_disk_tests.rs tests/backend/dial_forwards_tests.rs tests/backend/dial_opened_windows_tests.rs tests/backend/dial_ssh_config_tests.rs tests/backend/dial_terminal_tests.rs tests/backend/dial_terminal_processes_tests.rs tests/backend/dial_sftp_rig_host.rs tests/backend/stream/remote_ask_tests.rs tests/backend/stream/remote_ask_peers.rs"
  "backend-host:src/backend/host/ tests/backend/host/"
  "backend-wire:src/backend/wire/ tests/backend/wire/wire_tests.rs tests/backend/wire/listen_tests.rs tests/backend/wire/wire_support.rs tests/backend/wire/typed_support.rs tests/backend/ccm_line/argv_support.rs tests/backend/ccm_line/argv_tests.rs"
  "backend-agents:src/backend/agents/ tests/backend/agents/ tests/backend/agents_tests.rs"
  "backend-relay:src/backend/relay/ tests/backend/relay/"
  "backend-stderr-log:src/backend/stderr_log/ tests/backend/stderr_log_tests.rs"
  "backend-platform:src/backend/platform/ tests/backend/platform/ tests/backend/tmux_sandbox.rs"
  "backend-common:src/backend/common/ tests/backend/common/ tests/backend/heard.rs"
  "backend-plugin:src/backend/plugin/ tests/backend/plugin/ tests/backend/plugin_layer_guard.rs"
  "test-meters:src/backend/test-meters/ tests/backend/alloc_probe.rs tests/backend/alloc_probe_tests.rs tests/backend/golden.rs tests/backend/heard.rs"
  "ssh-dial:src/comms/ssh-dial/" "deploy:src/backend/deploy/" "egress:src/comms/egress/" "mobcore:src/mobile/core-rs/rust/")
# 壳那棵 workspace 里带禁用名单的包 → 编进它的路径（连它读的那份 clippy.toml）
# shellcheck disable=SC2034
shell_clippy_pkgs=("monitor:src/frontend/shell/ tests/frontend/shell/ src/frontend/clippy.toml" "cc-monitor-filewin:src/frontend/filewin/ tests/frontend/filewin/ src/frontend/clippy.toml" "resident-client:src/comms/resident-client/ tests/comms/resident-client/ src/comms/clippy.toml" "comms-inward:src/comms/inward/ tests/comms/inward/ src/comms/clippy.toml" "comms-outward:src/comms/outward/ tests/comms/outward/")
# 起点（base.sh：`<提交> <对着谁>`；找不到 ⇒ 空）
base_line=$(bash "$HERE/base.sh") || base_line=
base_ref=${base_line#* }
# 这棵树（连没提交的）相对起点改到的第一条（空 = 没改到；`?` = 找不到起点，照跑）
touched() {
  [ -n "$base_line" ] || { echo '?'; return; }
  { git -c core.quotepath=false diff --name-only "${base_line%% *}" -- "$@"
    git -c core.quotepath=false ls-files --others --exclude-standard -- "$@"; } | head -1
}
# 四段重的：段名 → 门禁里同一件事的格名（plan.py 挑的是格）
segs=(backend:backend shell:cargo vitest:npm mobile:mobile)
# 每段跑不跑：seg_why[段] = 跑的理由；没有这一项 = 不跑
declare -A seg_why
plan_all() { local s; for s in "${segs[@]}"; do seg_why[${s%%:*}]=$1; done; }
if [ -z "$base_line" ]; then plan_all "找不到起点（没有上游 · origin/main · main）"
else
  plan=$( { git -c core.quotepath=false diff --name-status -M "${base_line%% *}"
            git -c core.quotepath=false ls-files --others --exclude-standard | sed 's/^/A\t/'; } \
          | python3 "$HERE/gate-affected/plan.py" "$root" --quiet) || plan=
  case "$(sed -n 's/^PLAN-MODE: //p' <<< "$plan")" in
    '') plan_all "plan.py 停了（表与 gate.sh 对不上？），照全跑" ;;
    full) plan_all "改到 $(sed -n 's/^PLAN-WHY: FULL //p' <<< "$plan")，表说这条要全量" ;;
    *) cells=" $(sed -n 's/^PLAN-CELLS: //p' <<< "$plan") "
       for s in "${segs[@]}"; do
         why=$(sed -n "s/^PLAN-WHY: ${s#*:} //p" <<< "$plan")
         [[ $cells == *" ${s#*:} "* ]] && seg_why[${s%%:*}]=$( [[ $why == @* ]] && echo "恒跑（gate-affected/map 的 $why）" || echo "改到 $why")
       done ;;
  esac
fi
skip_why="相对 $base_ref 的改动没有一条落在它读的面上（gate-affected/map）"
# 只本机的两格（mobile · win-check）在远端那一趟（编译调度设了 BUILDSCHED_TAG）不跑：印一行，本机另跑 --only 那一格
local_only_skipped() {
  [ -n "${BUILDSCHED_TAG:-}" ] || return 1
  printf '  --   %-16s 远端跳过，本机单跑：--only %s\n' "$1" "$1"
}
if [ "$mode" = plan ]; then
  for s in "${segs[@]}"; do s=${s%%:*}
    if [ -n "${seg_why[$s]:-}" ]; then echo "${s^^}: 跑 —— ${seg_why[$s]}"; else echo "${s^^}: 不跑 —— $skip_why"; fi
  done
  exit 0
fi
mkdir -p .build/quick-check
box=$(mktemp -d "$root/.build/qc-box.XXXXXX") || exit 2
log="$root/.build/quick-check/$(date +%Y%m%d-%H%M%S).log"
: > "$log"
real_home=$HOME
cargo_home=${CARGO_HOME:-$HOME/.cargo}
rustup_home=${RUSTUP_HOME:-$HOME/.rustup}
npm_cache=${npm_config_cache:-$HOME/.npm}
ctmp=$(mktemp -d "${TMPDIR:-/tmp}/qc.XXXXXX") || exit 2
trap 'rm -rf "$box" "$ctmp"' EXIT
trap 'rm -rf "$box" "$ctmp"; exit 130' INT TERM
mkdir -p "$box/home" "$box/tmux"
# 每路测试的并行数：几路快检同时跑时，每路的测试按默认各起满核数的线程 / worker，互相挤（10-15 量过：
#   每路 8 个比默认省一到三成 CPU、墙钟持平；4 个墙钟变长）⇒ Rust 测试线程 · vitest worker · gradle worker
#   都拿「核数 ÷ heavy 槽位数」那一份（槽位数照 HEAVY_SLOTS，默认与 heavy.sh 同一个数）。外面设了 RUST_TEST_THREADS /
#   GATE_VITEST_WORKERS 就听外面的。
share=$(( $(nproc) / ${HEAVY_SLOTS:-3} )); [ "$share" -ge 1 ] || share=1
box_env=(env HOME="$box/home" TMUX_TMPDIR="$box/tmux"
  CARGO_HOME="$cargo_home" RUSTUP_HOME="$rustup_home"
  CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
  RUST_TEST_THREADS="${RUST_TEST_THREADS:-$share}" GATE_VITEST_WORKERS="${GATE_VITEST_WORKERS:-$share}")
run() { bash "$HERE/session-scrub.sh" "${box_env[@]}" "$@"; }
# 编译那几段：锁脚本在沙箱外起（它照真家找锁文件），它起的命令再进沙箱（见头注）。
build_lock=(); [ -n "${CCM_BUILD_LOCK:-}" ] && build_lock=(bash "$CCM_BUILD_LOCK")
locked() { "${build_lock[@]}" bash "$HERE/session-scrub.sh" "${box_env[@]}" "$@"; }
# 每路 cargo 并行数默认 4。要改就在外面设 CARGO_BUILD_JOBS。
# cargo 那几步不按墙钟判 ⇒ 降到 nice 10，机器忙时给别处按墙钟判的测试让路（vitest / tsc 不降）。
# runcargo：不编译的（fmt）；buildcargo：编译的，经锁。
runcargo() { run env TMPDIR="$ctmp" nice -n 10 "$@"; }
buildcargo() { CCM_BUILD_REMOTE_OK="$box/home $box/tmux $ctmp" locked env TMPDIR="$ctmp" nice -n 10 "$@"; }
fails=(); notes=()
t_all=$SECONDS
# 失败测试的 panic 原话：`panicked at` 那一行起、到空行为止（cargo 的格式）。
panics() { awk '/panicked at /{p=1} p&&/^$/{p=0; print "       ──"} p{print "       " $0}' | head -60; }
step() { local name=$1; shift; local t0=$SECONDS out
  out=$("$@" 2>&1); local rc=$?
  { echo "════ $name（rc=$rc）"; printf '%s\n' "$out"; } >> "$log"
  printf '%s\n' "$out" | cargo_times "$name"
  local n; n=$(printf '%s' "$out" | grep -oE '([0-9]+) passed' | grep -oE '[0-9]+' | paste -sd+ | bc 2>/dev/null)
  if [ $rc -eq 0 ] && [ "${n:-1}" = 0 ]; then printf '  红   %-16s 0 条跑到\n' "$name"; fails+=("$name"); return; fi
  if [ $rc -eq 0 ]; then printf '  ok   %-16s %ss%s\n' "$name" $((SECONDS-t0)) "${n:+ · $n 条}"
  else printf '  红   %-16s %ss\n' "$name" $((SECONDS-t0))
    hit=$(printf '%s\n' "$out" | grep -E "^ FAIL |^  \\| .*(✗|error|FAIL)|FAILED|panicked|Diff in|^error|error\[E|error TS|AssertionError" | head -25)
    printf '%s\n' "${hit:-$(printf '%s\n' "$out" | tail -5)}" | sed 's/^/       /'
    printf '%s\n' "$out" | panics; fails+=("$name"); fi; }

# cargo 测试的逐条耗时（找慢测试时读它）：libtest 的 `--report-time` 是不稳定选项，只在测试程序跑起来那一下开 ——
#   runner 给测试进程设 RUSTC_BOOTSTRAP=1，编译那一步看不见它（不重编，依赖的 build.rs 也不因它改判）。
#   那两段因此不带 -q：逐条一行 `test 名 ... ok <秒>`，收尾抽出来、从慢到快存 `.build/quick-check/cargo-times.tsv`（秒 · 段 · 测试程序 · 名）。
host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p'); host=${host^^}; host=${host//-/_}
timed=(env "CARGO_TARGET_${host}_RUNNER=env RUSTC_BOOTSTRAP=1")
times=.build/quick-check/cargo-times.tsv
: > "$times.part"
cargo_times() { awk -v seg="$1" '
  /^ *Running /{ b=$NF; gsub(/[()]/, "", b); sub(/.*\//, "", b); sub(/-[0-9a-f]+$/, "", b) }
  /^test .* <[0-9.]+s>$/{ t=$NF; gsub(/[<>s]/, "", t); print t "\t" seg "\t" b "\t" $2 }' >> "$times.part"; }
times_line() {
  sort -t "$(printf '\t')" -k1,1gr "$times.part" > "$times"; rm -f "$times.part"
  [ -s "$times" ] || return 0
  printf '  --   %-16s 最慢：%s · 整份 %s\n' cargo-times "$(head -3 "$times" | awk -F '\t' '{printf "%s%s %ss", (NR>1?" · ":""), $4, $1}')" "$times"
}

# 重的那四段：挑上了才跑（见头注），跑不跑与为什么各印一行。
ran=(); skipped=()
seg_go() {
  if [ -n "${seg_why[$1]:-}" ]; then printf '  --   %-16s 跑：%s\n' "$1" "${seg_why[$1]}"; ran+=("$1"); return 0; fi
  printf '  --   %-16s 不跑：%s\n' "$1" "$skip_why"; skipped+=("$1"); return 1
}
# 手机那一段：条数读这一趟 .build/mobile/*/test-results（先清掉旧的）。手机读的仓外路径由判据 flow_scripts_guard 对着手机树里字面写到的核表。
mobile_step() {
  local_only_skipped mobile && { skipped+=(mobile); return 0; }
  seg_go mobile || return 0
  step mobile locked env REAL_HOME="$real_home" bash "$HERE/heavy.sh" bash -c '
    out="$PWD/.build/mobile"; cd src/mobile || exit 1
    if [ -z "${JAVA_HOME:-}" ] || [ -z "${ANDROID_HOME:-}" ]; then
      export CCM_ANDROID_DEV="${CCM_ANDROID_DEV:-$REAL_HOME/android-dev}"
      . ./scripts/env.sh >/dev/null || { echo "mobile: 找不到 Android 工具链（JDK ＋ SDK）—— 判不了"; exit 1; }
    else export GRADLE_USER_HOME="${GRADLE_USER_HOME:-$REAL_HOME/.gradle}"; fi
    rm -rf "$out"/*/test-results
    ./gradlew --no-daemon --console=plain --max-workers='"$share"' ktlintCheck detektDebug testDebugUnitTest \
      :core-claude:test :core-claude:detektMain :core-wire:test :core-wire:detektMain :core-copy:test :core-copy:detektMain :detekt-rules:test || exit $?
    n=$(cat "$out"/*/test-results/*/*.xml 2>/dev/null | grep -oE "<testsuite [^>]*tests=\"[0-9]+\"" | grep -oE "tests=\"[0-9]+\"" | grep -oE "[0-9]+" | paste -sd+ - | bc 2>/dev/null)
    echo "mobile: ${n:-0} passed"'
}
# 一棵树的 clippy：改到的包才跑（见头注）。clippy_step <格名> <树的目录> <哪棵树（不跑时那一句）> <包表的变量名>
clippy_step() {
  local name=$1 dir=$2 tree=$3 spec paths pkgs=()
  local -n table=$4
  for spec in "${table[@]}"; do
    read -ra paths <<< "${spec#*:}"
    [ -n "$(touched "${paths[@]}")" ] && pkgs+=(-p "${spec%%:*}")
  done
  if [ ${#pkgs[@]} -eq 0 ]; then printf '  --   %-16s 不跑：相对 %s 没改到%s的包\n' "$name" "$base_ref" "$tree"; return; fi
  step "$name" buildcargo bash -c 'cd "$1" && shift && cargo clippy --all-targets "$@"' _ "$dir" "${pkgs[@]}"
}
# 共享包的文档测试（`compile_fail` 那几条钉的是「这样写编不过」）：壳那棵只跑 --lib，这几份也不在后端那棵 ⇒ 单跑 --doc。改到的包才跑。
#   名单只列有文档测试的包：一条都没有的包单独改到时 step 报「0 条跑到」。
doc_pkgs=("relay-route-core:src/common/relay-route-core/")
doc_step() {
  local spec paths pkgs=()
  for spec in "${doc_pkgs[@]}"; do
    read -ra paths <<< "${spec#*:}"
    [ -n "$(touched "${paths[@]}")" ] && pkgs+=(-p "${spec%%:*}")
  done
  if [ ${#pkgs[@]} -eq 0 ]; then printf '  --   %-16s 不跑：相对 %s 没改到带文档测试的共享包\n' doc-shared "$base_ref"; return; fi
  step doc-shared buildcargo bash -c 'cd src/frontend/shell && cargo test --doc "$@"' _ "${pkgs[@]}"
}
# Windows 目标的编译核对（只 check、不链接）：非 unix 那一臂的可见性 · 签名只在 Windows 目标上编得到，Linux 上的测试与 clippy 看不见。
#   后端那棵只主包的库（它从外面调拆出去的各包，那几包的 Windows 臂跟着编到）· 壳那棵整棵的库。
#   只在本机：经锁起但不带 CCM_BUILD_REMOTE_OK（外面带进来的也摘掉）—— 别的机器没装这个目标。目标没装 ⇒ 照实报没装、算红。
win_target=x86_64-pc-windows-gnu
win_check_step() {
  local_only_skipped win-check && return 0
  local sysroot; sysroot=$(rustc --print sysroot 2>/dev/null)
  if [ -z "$sysroot" ] || [ ! -d "$sysroot/lib/rustlib/$win_target" ]; then
    printf '  红   %-16s 本机工具链没装 %s 目标（rustup target add %s）—— 判不了\n' win-check "$win_target" "$win_target"; fails+=(win-check); return
  fi
  step win-check env -u CCM_BUILD_REMOTE_OK "${build_lock[@]}" bash "$HERE/session-scrub.sh" "${box_env[@]}" env TMPDIR="$ctmp" nice -n 10 bash -c '
    (cd src/backend && cargo check --target "$1" -p cc-monitor-backend --lib) && (cd src/frontend/shell && cargo check --target "$1" --workspace --lib)' _ "$win_target"
}
# 修剪（见头注）：同组（路径去掉末尾 `-<哈希>`）最新那一份往前 6 小时之外的删掉
prune_step() {
  local t0=$SECONDS victims=() p kb
  mapfile -t victims < <(find .build -mindepth 3 -maxdepth 4 -type d -name incremental 2>/dev/null | while IFS= read -r inc; do
      find "$inc" -mindepth 1 -maxdepth 1 -type d -printf '%T@\t%p\n'
      [ -d "${inc%/incremental}/deps" ] && find "${inc%/incremental}/deps" -maxdepth 1 -type f -perm -u+x ! -name '*.*' -printf '%T@\t%p\n'
    done | awk -F '\t' -v win=$((6 * 3600)) '
      { k = $2; sub(/-[^-\/]+$/, "", k); t[NR] = $1; p[NR] = $2; g[NR] = k; if (!(k in m) || $1 > m[k]) m[k] = $1 }
      END { for (i = 1; i <= NR; i++) if (t[i] < m[g[i]] - win) print p[i] }')
  if [ ${#victims[@]} -eq 0 ]; then printf '  ok   %-16s 没有同组旧出 6 小时的增量会话与测试二进制\n' prune; return; fi
  kb=$(du -csk "${victims[@]}" 2>/dev/null | tail -1 | cut -f1)
  for p in "${victims[@]}"; do rm -rf -- "$p"; [ -f "$p.d" ] && rm -f -- "$p.d"; done
  printf '  ok   %-16s %ss · 删了 %s 份同组旧出 6 小时的增量会话与测试二进制 · 腾出 %s\n' prune $((SECONDS-t0)) ${#victims[@]} "$(numfmt --to=iec $((${kb:-0} * 1024)))"
}
verdict() {
  prune_step
  times_line
  left=$(ls -A "$ctmp")
  if [ -z "$left" ]; then printf '  ok   %-16s cargo 那几步跑完临时目录是空的\n' tmp-left
  else printf '  红   %-16s cargo 那几步跑完临时目录里留下（建的那一处改成随作用域删）：\n' tmp-left
    printf '%s\n' "$left" | sed 's/^/       /'; fails+=(tmp-left); fi
  echo
  echo "  重的四段：跑了 ${ran[*]:-（无）}；没跑 ${skipped[*]:-（无）}"
  echo "  墙钟 $((SECONDS-t_all))s · 负载 $(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null) · 整趟输出 ${log#"$root"/}"
  if [ ${#fails[@]} -eq 0 ]; then echo "QUICK: OK ${notes[*]:+（${notes[*]}）} —— 可以排队跑门禁"; exit 0
  else echo "QUICK: 红 —— ${fails[*]}"; exit 1; fi
}

# 后端那棵整套 --lib；版本号那一条若是唯一红的 ⇒ 记「待打版本号」
backend_step() {
  seg_go backend || return 0
  t0=$SECONDS
  # 只编各包的测试库：带上 bin 与 bench 会让主库再按非测试形编一遍。bin 与 bench 编不编得过由 clippy `--all-targets` 守，
  # bench 的冒烟档（要 spawn 真二进制，进不了库的测试段）由门禁与 CI 的 `cargo test --workspace` 跑。`--workspace` 带上拆出的几个包。
  out=$(buildcargo "${timed[@]}" bash -c 'cd src/backend && cargo test --workspace --lib --no-fail-fast -- -Z unstable-options --report-time' 2>&1); rc=$?
  { echo "════ backend（rc=$rc）"; printf '%s\n' "$out"; } >> "$log"
  printf '%s\n' "$out" | cargo_times backend
  n=$(printf '%s' "$out" | grep -oE '([0-9]+) passed' | grep -oE '[0-9]+' | paste -sd+ | bc 2>/dev/null)
  failed=$(printf '%s\n' "$out" | awk '/^failures:$/{f=1; next} f && /^    [A-Za-z0-9_:]+$/{print $1} /^test result:/{f=0}' | sort -u)
  if [ $rc -eq 0 ] && [ "${n:-0}" -gt 0 ]; then printf '  ok   %-16s %ss · %s 条\n' backend $((SECONDS-t0)) "$n"
  elif [ $rc -ne 0 ] && [ "$failed" = "build_id_guard::tests::adding_a_subcommand_forces_a_build_id_bump" ]; then
    printf '  待   %-16s %ss · %s 条；子命令集变了，合并时打版本号（不算红）\n' backend $((SECONDS-t0)) "${n:-?}"; notes+=("待打版本号")
  else printf '  红   %-16s %ss\n' backend $((SECONDS-t0))
    hit=$(printf '%s\n' "$out" | grep -E "FAILED|panicked|^error|error\[E" | head -25)
    printf '%s\n' "${hit:-$(printf '%s\n' "$out" | tail -5)}" | sed 's/^/       /'
    printf '%s\n' "$out" | panics; fails+=(backend); fi
}
shell_step() {
  seg_go shell || return 0
  step shell buildcargo "${timed[@]}" bash -c 'cd src/frontend/shell && cargo test --workspace --lib --no-fail-fast -- -Z unstable-options --report-time'
}
vitest_step() {
  seg_go vitest || return 0
  # 逐条耗时另存一份（找慢测试时读它）
  step vitest run node_modules/.bin/vitest run --reporter=dot --reporter=json --outputFile.json=.build/quick-check/vitest-times.json
}

echo "快检 $(git log --oneline -1 | cut -c1-60)"
[ -n "$only" ] && { "${only//-/_}_step"; verdict; }
if [ ! -x node_modules/.bin/tsc ]; then echo "  --   node_modules 不在（新树）⇒ npm ci"; run env npm_config_cache="$npm_cache" npm ci --prefer-offline --no-audit --no-fund >/dev/null 2>&1 || { echo "QUICK: 红 —— npm ci 失败"; exit 1; }; fi
step fmt          runcargo bash -c 'cd src/frontend/shell && cargo fmt --all --check'
# 后端那棵的包名单从 `cargo metadata --no-deps` 现取（手机嵌的 `mobcore` 除外；与门禁 `fmt-backend` 那一格同一条取法）。
step fmt-backend  runcargo bash -c 'cd src/backend && pkgs=$(cargo metadata --no-deps --offline --format-version 1 | python3 -c "import json, sys; print(\" \".join(\"-p \" + p[\"name\"] for p in json.load(sys.stdin)[\"packages\"] if p[\"name\"] != \"mobcore\"))") && [ -n "$pkgs" ] || { echo "fmt-backend: cargo metadata 一个包都没取到"; exit 1; }; cargo fmt --check $pkgs'
step tsc          run node_modules/.bin/tsc --noEmit
# 便宜的静态格：只读盘上文本，合计十来秒（shellcheck 按核数并起）。名单只住 ci.yml `static-gate` 那个 job 的 GATE_ONLY。
static=$(awk '/^  static-gate:/{f=1} f&&/GATE_ONLY:/{sub(/.*GATE_ONLY:[[:space:]]*/,""); print; exit}' .github/workflows/ci.yml 2>/dev/null)
if [ -n "$static" ]; then step static run env GATE_ONLY="$static" bash tests/scripts/gate.sh
else printf '  红   %-16s ci.yml 里读不到 static-gate 那个 job 的 GATE_ONLY（job 改名了？）\n' static; fails+=(static); fi

backend_step
clippy_step clippy-backend src/backend 后端那棵 clippy_pkgs
shell_step
clippy_step clippy-shell src/frontend/shell 壳那棵 shell_clippy_pkgs
win_check_step
doc_step
# 凭据那份共享 crate 开 `harden` 那一档（壳那棵不开它，后端那棵不跑它的测试）
step creds-harden buildcargo bash -c 'cd src/frontend/shell && cargo test -p creds-core --features harden --lib -q'
vitest_step
mobile_step
# 生成物：上面几步现写过的 generated/ 与 HEAD 比，逐份印出
t0=$SECONDS; out=$(bash "$HERE/generated-drift.sh" 2>&1); rc=$?
{ echo "════ generated（rc=$rc）"; printf '%s\n' "$out"; } >> "$log"
if [ $rc -eq 0 ]; then printf '  ok   %-16s %ss · generated/ 与提交的一致\n' generated $((SECONDS-t0))
else printf '  红   %-16s %s\n' generated "$( [ $rc -eq 1 ] && echo '这几份生成物没提交（cargo test 现写的；一起提交）：' || echo '判不了：')"
  printf '%s\n' "$out" | sed 's/^/       /'; fails+=(generated); fi

verdict
