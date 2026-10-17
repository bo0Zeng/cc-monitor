#!/usr/bin/env bash
# 快检：交给门禁之前的那一圈，也是合并后 gate-affected --trust-ci 的第一步。不拿门禁锁、不代替门禁。
# 用法：在仓里任一处 `bash tests/scripts/quick-check.sh`
#   跑：两棵 Rust 树的排版 · tsc · 门禁里只读盘上文本的便宜格（static_cells，经 gate.sh 的 GATE_ONLY，收据另存不盖门禁那份）·
#       后端那棵整套 `cargo test --workspace --lib --bins --benches`（后端 ＋ 拨号核心；bench 只跑冒烟档） · 壳那棵 workspace 整套 `cargo test --workspace --lib`
#       （monitor · 文件窗口 · 共享 crate · 通信层，全部成员）· 整套 vitest · 末尾生成物有没有落在提交外（generated-drift.sh：cargo test 现写的
#       `generated/` 与 HEAD 不同或没跟踪 ⇒ 红，逐份印出；合并只看得见提交）。
#   后端那棵的 clippy（系统钟 · 定时器那几类 lint 定成 deny，测试拦不住）：照门禁 `clippy-backend` 那一格的参数，只跑相对起点改到的包
#     （clippy_pkgs；一个都没改到就跳过并说一声，找不到起点就全都跑）。
#   不按名字挑判据（10-10 起）：从前只跑名字带 guard / registry / ledger … 的那些，漏了一百多份读金样 / 冻结表的判据，
#     推上去 CI 才红。现在两棵树与 vitest 都是整套。
#   旧用法里跟着的 cargo 过滤词照收、不再用（整套本来就全跑到）。
#   「加了子命令要打版本号」那一条在各路树上必红（版本号合并时打：`tests/scripts/bump-build-id.sh`）⇒ 单列成「待打版本号」，不算红。
# 环境：每一步经 session-scrub.sh 摘掉会话继承来的变量（表在 session-vars）；HOME 与 TMUX_TMPDIR 指到 `.build/` 下一次性沙箱目录（不碰真家目录与默认 tmux）。
#   cargo / rustup / npm 的家照外面的 CARGO_HOME / RUSTUP_HOME / npm_config_cache，没设就是真家目录下的默认位置（沙箱换 HOME 之前就定下）。
# 手机那一套（ktlint · detekt · 各模块 JVM 单测；经 heavy.sh，gradle 带 --no-daemon）：这棵树（连没提交的改动）相对起点改到了
#   下面 mobile_reads 里的路径才跑，没改到就跳过并打一行说明。手机的单测与构建在 src/mobile 之外还读：tests/__fixtures__/（金样）·
#   src/backend/lib.rs（BUILD_ID 与子命令表）· src/shared/copy/（文案表，生成 CopyEntries）· src/shared/theme/（令牌正本，生成 ThemeTokens）。这张表漏没漏，由判据
#   tests/backend/flow_scripts_guard.rs 对着 src/mobile 里字面写到的仓内路径核。Android Lint 与 release 构建不在这里（门禁 mobile 格跑）。
#   起点是 base.sh 印的那一个：分支上游（合并用的分支）与 HEAD 的 merge-base，没设上游就对 origin/main；不必先合 main。
#   工具链：外面给了 JAVA_HOME 与 ANDROID_HOME 就用；没给就照 src/mobile/scripts/env.sh 探真家目录下的 android-dev（沙箱换 HOME 之前就定下）。
#   `--mobile` 只跑手机那一段；`--mobile-plan` 只印跑不跑、为什么。
# 红了：每条失败测试的 panic 原话（`panicked at` 起那几行）印出来；整趟输出存 `.build/quick-check/<时刻>.log`，偶发的那一次有原话可查。
set -uo pipefail
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git rev-parse --show-toplevel) || exit 2
cd "$root" || exit 2
mode=all
case "${1:-}" in --mobile) mode=mobile; shift ;; --mobile-plan) mode=plan; shift ;; esac
mobile_reads=(src/mobile/ tests/__fixtures__/ src/backend/lib.rs src/shared/copy/ src/shared/theme/ src/backend/ssh-dial/ src/backend/deploy/ src/common/deploy-contract/ src/frontend/shell/embedded-backends/ src/backend/Cargo.toml src/backend/Cargo.lock src/frontend/ui/generated)
# 后端那棵 workspace 的包 → 编进它的路径（git 路径规格，空格分隔）
clippy_pkgs=("cc-monitor-backend:src/backend/ tests/backend/ :(exclude)src/backend/ssh-dial/ :(exclude)src/backend/deploy/" "ssh-dial:src/backend/ssh-dial/" "deploy:src/backend/deploy/" "mobcore:src/mobile/core-rs/rust/")
# 便宜的静态格：只读盘上文本，合计十来秒（shellcheck 按核数并起）。
static_cells="hooks copy2 shellcheck e2e-smoke release-gate platform installface"
# 起点（base.sh：`<提交> <对着谁>`；找不到 ⇒ 空）
base_line=$(bash "$HERE/base.sh") || base_line=
base_ref=${base_line#* }
# 这棵树（连没提交的）相对起点改到的第一条（空 = 没改到；`?` = 找不到起点，照跑）
touched() {
  [ -n "$base_line" ] || { echo '?'; return; }
  { git -c core.quotepath=false diff --name-only "${base_line%% *}" -- "$@"
    git -c core.quotepath=false ls-files --others --exclude-standard -- "$@"; } | head -1
}
mobile_touch() { touched "${mobile_reads[@]}"; }
if [ "$mode" = plan ]; then
  hit=$(mobile_touch)
  echo "MOBILE-READS: ${mobile_reads[*]}"
  if [ -z "$hit" ]; then echo "MOBILE: 跳过 —— 相对 $base_ref 没改到"; elif [ "$hit" = '?' ]; then echo "MOBILE: 跑 —— 找不到起点（没有上游 · origin/main · main）"; else echo "MOBILE: 跑 —— 改到 $hit"; fi
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
trap 'rm -rf "$box"' EXIT
trap 'rm -rf "$box"; exit 130' INT TERM
mkdir -p "$box/home" "$box/tmux"
# 每路测试的并行数：几路快检同时跑时，每路的测试按默认各起满核数的线程 / worker，互相挤（10-15 量过：
#   每路 8 个比默认省一到三成 CPU、墙钟持平；4 个墙钟变长）⇒ Rust 测试线程 · vitest worker · gradle worker
#   都拿「核数 ÷ heavy 槽位数」那一份（槽位数照 HEAVY_SLOTS，默认与 heavy.sh 同一个数）。外面设了 RUST_TEST_THREADS /
#   GATE_VITEST_WORKERS 就听外面的。
share=$(( $(nproc) / ${HEAVY_SLOTS:-3} )); [ "$share" -ge 1 ] || share=1
run() { bash "$HERE/session-scrub.sh" env HOME="$box/home" TMUX_TMPDIR="$box/tmux" \
  CARGO_HOME="$cargo_home" RUSTUP_HOME="$rustup_home" \
  CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" \
  RUST_TEST_THREADS="${RUST_TEST_THREADS:-$share}" GATE_VITEST_WORKERS="${GATE_VITEST_WORKERS:-$share}" "$@"; }
# 每路 cargo 并行数默认 4。要改就在外面设 CARGO_BUILD_JOBS。
# cargo 那几步不按墙钟判 ⇒ 降到 nice 10，机器忙时给别处按墙钟判的测试让路（vitest / tsc 不降）。
runcargo() { run nice -n 10 "$@"; }

[ $# -gt 0 ] && echo "  --   过滤词 $* 不再需要：两棵树整套都跑"
fails=(); notes=()
t_all=$SECONDS
# 失败测试的 panic 原话：`panicked at` 那一行起、到空行为止（cargo 的格式）。
panics() { awk '/panicked at /{p=1} p&&/^$/{p=0; print "       ──"} p{print "       " $0}' | head -60; }
step() { local name=$1; shift; local t0=$SECONDS out
  out=$("$@" 2>&1); local rc=$?
  { echo "════ $name（rc=$rc）"; printf '%s\n' "$out"; } >> "$log"
  local n; n=$(printf '%s' "$out" | grep -oE '([0-9]+) passed' | grep -oE '[0-9]+' | paste -sd+ | bc 2>/dev/null)
  if [ $rc -eq 0 ] && [ "${n:-1}" = 0 ]; then printf '  红   %-16s 0 条跑到\n' "$name"; fails+=("$name"); return; fi
  if [ $rc -eq 0 ]; then printf '  ok   %-16s %ss%s\n' "$name" $((SECONDS-t0)) "${n:+ · $n 条}"
  else printf '  红   %-16s %ss\n' "$name" $((SECONDS-t0))
    hit=$(printf '%s\n' "$out" | grep -E "^ FAIL |^  \\| .*(✗|error|FAIL)|FAILED|panicked|Diff in|^error|error\[E|error TS|AssertionError" | head -25)
    printf '%s\n' "${hit:-$(printf '%s\n' "$out" | tail -5)}" | sed 's/^/       /'
    printf '%s\n' "$out" | panics; fails+=("$name"); fi; }

# 手机那一段：改到才跑（见头注）。条数读这一趟 .build/mobile/*/test-results（先清掉旧的）。
mobile_step() {
  local hit; hit=$(mobile_touch)
  if [ -z "$hit" ]; then printf '  --   %-16s 跳过：相对 %s 没改到 %s\n' mobile "$base_ref" "${mobile_reads[*]}"; return; fi
  printf '  --   %-16s %s ⇒ 跑 ktlint · detekt · JVM 单测\n' mobile "$( [ "$hit" = '?' ] && echo '找不到起点，照跑' || echo "改到 $hit")"
  step mobile run env REAL_HOME="$real_home" bash "$HERE/heavy.sh" bash -c '
    out="$PWD/.build/mobile"; cd src/mobile || exit 1
    if [ -z "${JAVA_HOME:-}" ] || [ -z "${ANDROID_HOME:-}" ]; then
      export CCM_ANDROID_DEV="${CCM_ANDROID_DEV:-$REAL_HOME/android-dev}"
      . ./scripts/env.sh >/dev/null || { echo "mobile: 找不到 Android 工具链（JDK ＋ SDK）—— 判不了"; exit 1; }
    else export GRADLE_USER_HOME="${GRADLE_USER_HOME:-$REAL_HOME/.gradle}"; fi
    rm -rf "$out"/*/test-results
    ./gradlew --no-daemon --console=plain --max-workers='"$share"' ktlintCheck detektDebug testDebugUnitTest \
      :core-claude:test :core-claude:detektMain :core-remote:test :core-remote:detektMain || exit $?
    n=$(cat "$out"/*/test-results/*/*.xml 2>/dev/null | grep -oE "<testsuite [^>]*tests=\"[0-9]+\"" | grep -oE "tests=\"[0-9]+\"" | grep -oE "[0-9]+" | paste -sd+ - | bc 2>/dev/null)
    echo "mobile: ${n:-0} passed"'
}
# 后端那棵的 clippy：改到的包才跑（见头注）
clippy_step() {
  local spec paths pkgs=()
  for spec in "${clippy_pkgs[@]}"; do
    read -ra paths <<< "${spec#*:}"
    [ -n "$(touched "${paths[@]}")" ] && pkgs+=(-p "${spec%%:*}")
  done
  if [ ${#pkgs[@]} -eq 0 ]; then printf '  --   %-16s 跳过：相对 %s 没改到后端那棵的包\n' clippy-backend "$base_ref"; return; fi
  step clippy-backend runcargo bash -c 'cd src/backend && cargo clippy --all-targets "$@"' _ "${pkgs[@]}"
}
verdict() {
  echo
  echo "  墙钟 $((SECONDS-t_all))s · 负载 $(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null) · 整趟输出 ${log#"$root"/}"
  if [ ${#fails[@]} -eq 0 ]; then echo "QUICK: OK ${notes[*]:+（${notes[*]}）} —— 可以排队跑门禁"; exit 0
  else echo "QUICK: 红 —— ${fails[*]}"; exit 1; fi
}

echo "快检 $(git log --oneline -1 | cut -c1-60)"
[ "$mode" = mobile ] && { mobile_step; verdict; }
if [ ! -x node_modules/.bin/tsc ]; then echo "  --   node_modules 不在（新树）⇒ npm ci"; run env npm_config_cache="$npm_cache" npm ci --prefer-offline --no-audit --no-fund >/dev/null 2>&1 || { echo "QUICK: 红 —— npm ci 失败"; exit 1; }; fi
step fmt          runcargo bash -c 'cd src/frontend/shell && cargo fmt --all --check'
step fmt-backend  runcargo bash -c 'cd src/backend && cargo fmt --check -p cc-monitor-backend -p ssh-dial -p deploy'
step tsc          run node_modules/.bin/tsc --noEmit
step static       run env GATE_ONLY="$static_cells" GATE_RECEIPT="$root/.build/quick-check/static-receipt.json" bash tests/scripts/gate.sh

# 后端那棵整套 lib ＋ bin ＋ bench 冒烟（bench 那几个目标 `test = true`，CI 的 `cargo test --workspace` 跑它们）；
#   版本号那一条若是唯一红的、且没有别的目标红 ⇒ 记「待打版本号」
t0=$SECONDS
out=$(runcargo bash -c 'cd src/backend && cargo test --workspace --lib --bins --benches --no-fail-fast -q' 2>&1); rc=$?
{ echo "════ backend（rc=$rc）"; printf '%s\n' "$out"; } >> "$log"
n=$(printf '%s' "$out" | grep -oE '([0-9]+) passed' | grep -oE '[0-9]+' | paste -sd+ | bc 2>/dev/null)
failed=$(printf '%s\n' "$out" | awk '/^failures:$/{f=1; next} f && /^    [A-Za-z0-9_:]+$/{print $1} /^test result:/{f=0}' | sort -u)
if [ $rc -eq 0 ] && [ "${n:-0}" -gt 0 ]; then printf '  ok   %-16s %ss · %s 条\n' backend $((SECONDS-t0)) "$n"
elif [ $rc -ne 0 ] && [ "$failed" = "build_id_guard::tests::adding_a_subcommand_forces_a_build_id_bump" ] \
  && ! printf '%s\n' "$out" | grep -q 'to rerun pass .*--b\(in\|ench\) '; then
  printf '  待   %-16s %ss · %s 条；子命令集变了，合并时打版本号（不算红）\n' backend $((SECONDS-t0)) "${n:-?}"; notes+=("待打版本号")
else printf '  红   %-16s %ss\n' backend $((SECONDS-t0))
  hit=$(printf '%s\n' "$out" | grep -E "FAILED|panicked|^error|error\[E" | head -25)
  printf '%s\n' "${hit:-$(printf '%s\n' "$out" | tail -5)}" | sed 's/^/       /'
  printf '%s\n' "$out" | panics; fails+=(backend); fi

clippy_step
# 壳那棵 workspace 整套 --lib（全部成员）
step shell        runcargo bash -c 'cd src/frontend/shell && cargo test --workspace --lib --no-fail-fast -q'
# 界面：整套 vitest
step vitest       run node_modules/.bin/vitest run --reporter=dot
# 手机：改到才跑
mobile_step
# 生成物：上面几步现写过的 generated/ 与 HEAD 比，逐份印出
t0=$SECONDS; out=$(bash "$HERE/generated-drift.sh" 2>&1); rc=$?
{ echo "════ generated（rc=$rc）"; printf '%s\n' "$out"; } >> "$log"
if [ $rc -eq 0 ]; then printf '  ok   %-16s %ss · generated/ 与提交的一致\n' generated $((SECONDS-t0))
else printf '  红   %-16s %s\n' generated "$( [ $rc -eq 1 ] && echo '这几份生成物没提交（cargo test 现写的；一起提交）：' || echo '判不了：')"
  printf '%s\n' "$out" | sed 's/^/       /'; fails+=(generated); fi

verdict
