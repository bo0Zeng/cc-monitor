#!/usr/bin/env bash
# 快检：交给门禁之前的那一圈，也是合并树上 gate-affected --trust-ci 的第一步。不拿门禁锁、不代替门禁。
# 用法：在仓里任一处 `bash tests/scripts/quick-check.sh`
#   跑：两棵 Rust 树的排版 · tsc · 后端那棵整套 `cargo test --lib` · 壳那棵 workspace 整套 `cargo test --workspace --lib`
#       （monitor · 文件窗口 · 共享 crate · 通信层，全部成员）· 整套 vitest。
#   不按名字挑判据（10-10 起）：从前只跑名字带 guard / registry / ledger … 的那些，漏了一百多份读金样 / 冻结表的判据，
#     合并树推上去 CI 才红（mg37a · mg37）。现在两棵树与 vitest 都是整套。
#   旧用法里跟着的 cargo 过滤词照收、不再用（整套本来就全跑到）。
#   「加了子命令要打版本号」那一条在各路树上必红（版本号合并时打：`tests/scripts/bump-build-id.sh`）⇒ 单列成「待打版本号」，不算红。
# 环境：清掉会话继承来的 CCM_* / CLAUDE_* / ANTHROPIC_* / TMUX*；HOME 与 TMUX_TMPDIR 指到 `.build/` 下一次性沙箱目录（不碰真家目录与默认 tmux）。
#   cargo / rustup / npm 的家照外面的 CARGO_HOME / RUSTUP_HOME / npm_config_cache，没设就是真家目录下的默认位置（沙箱换 HOME 之前就定下）。
# 红了：每条失败测试的 panic 原话（`panicked at` 起那几行）印出来；整趟输出存 `.build/quick-check/<时刻>.log`，偶发的那一次有原话可查。
set -uo pipefail
root=$(git rev-parse --show-toplevel) || exit 2
cd "$root" || exit 2
mkdir -p .build/quick-check
box=$(mktemp -d "$root/.build/qc-box.XXXXXX") || exit 2
log="$root/.build/quick-check/$(date +%Y%m%d-%H%M%S).log"
: > "$log"
cargo_home=${CARGO_HOME:-$HOME/.cargo}
rustup_home=${RUSTUP_HOME:-$HOME/.rustup}
npm_cache=${npm_config_cache:-$HOME/.npm}
trap 'rm -rf "$box"' EXIT
trap 'rm -rf "$box"; exit 130' INT TERM
mkdir -p "$box/home" "$box/tmux"
scrub=()
while IFS='=' read -r k _; do
  case "$k" in CCM_*|CLAUDE_CONFIG_DIR|CLAUDE_CODE_*|ANTHROPIC_*|TMUX|TMUX_PANE|TMUX_TMPDIR) scrub+=(-u "$k") ;; esac
done < <(env)
run() { env "${scrub[@]}" HOME="$box/home" TMUX_TMPDIR="$box/tmux" \
  CARGO_HOME="$cargo_home" RUSTUP_HOME="$rustup_home" \
  CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" "$@"; }
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
    hit=$(printf '%s\n' "$out" | grep -E "^ FAIL |FAILED|panicked|Diff in|^error|error\[E|error TS|AssertionError" | head -25)
    printf '%s\n' "${hit:-$(printf '%s\n' "$out" | tail -5)}" | sed 's/^/       /'
    printf '%s\n' "$out" | panics; fails+=("$name"); fi; }

echo "快检 $(git log --oneline -1 | cut -c1-60)"
if [ ! -x node_modules/.bin/tsc ]; then echo "  --   node_modules 不在（新树）⇒ npm ci"; run env npm_config_cache="$npm_cache" npm ci --prefer-offline --no-audit --no-fund >/dev/null 2>&1 || { echo "QUICK: 红 —— npm ci 失败"; exit 1; }; fi
step fmt          runcargo bash -c 'cd src/frontend/shell && cargo fmt --all --check'
step fmt-backend  runcargo bash -c 'cd src/backend && cargo fmt --check'
step tsc          run node_modules/.bin/tsc --noEmit

# 后端那棵整套 --lib；版本号那一条若是唯一红的 ⇒ 记「待打版本号」
t0=$SECONDS
out=$(runcargo bash -c 'cd src/backend && cargo test --lib --no-fail-fast -q' 2>&1); rc=$?
{ echo "════ backend（rc=$rc）"; printf '%s\n' "$out"; } >> "$log"
n=$(printf '%s' "$out" | grep -oE '([0-9]+) passed' | grep -oE '[0-9]+' | paste -sd+ | bc 2>/dev/null)
failed=$(printf '%s\n' "$out" | awk '/^failures:$/{f=1; next} f && /^    [A-Za-z0-9_:]+$/{print $1} /^test result:/{f=0}' | sort -u)
if [ $rc -eq 0 ] && [ "${n:-0}" -gt 0 ]; then printf '  ok   %-16s %ss · %s 条\n' backend $((SECONDS-t0)) "$n"
elif [ $rc -ne 0 ] && [ "$failed" = "build_id_guard::tests::adding_a_subcommand_forces_a_build_id_bump" ]; then
  printf '  待   %-16s %ss · %s 条；子命令集变了，合并时主会话打版本号（不算红）\n' backend $((SECONDS-t0)) "${n:-?}"; notes+=("待打版本号")
else printf '  红   %-16s %ss\n' backend $((SECONDS-t0))
  hit=$(printf '%s\n' "$out" | grep -E "FAILED|panicked|^error|error\[E" | head -25)
  printf '%s\n' "${hit:-$(printf '%s\n' "$out" | tail -5)}" | sed 's/^/       /'
  printf '%s\n' "$out" | panics; fails+=(backend); fi

# 壳那棵 workspace 整套 --lib（全部成员）
step shell        runcargo bash -c 'cd src/frontend/shell && cargo test --workspace --lib --no-fail-fast -q'
# 界面：整套 vitest
step vitest       run node_modules/.bin/vitest run --reporter=dot

echo
echo "  墙钟 $((SECONDS-t_all))s · 负载 $(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null) · 整趟输出 ${log#"$root"/}"
if [ ${#fails[@]} -eq 0 ]; then echo "QUICK: OK ${notes[*]:+（${notes[*]}）} —— 可以排队跑门禁"; exit 0
else echo "QUICK: 红 —— ${fails[*]}"; exit 1; fi
