#!/usr/bin/env bash
# Rust 覆盖率：**判据盖了生产多少行**（TQ1 · `设计/99 §3`「度量能力」· `15 §6` 第 4 条）。
#
# 是量具，不是门禁：它**不进** `gate.sh`、不进 CI，也不给任何数定地板。
# 读数进 `调研/第四波记录/TQ1.md`（一次性），要复算就再跑一次。
#
# # 选型：rustc 自带的 `-C instrument-coverage` ＋ 系统的 `llvm-profdata` / `llvm-cov`
#
# 不用 `cargo-llvm-cov` / `grcov` / `tarpaulin`：三个都要 `cargo install`（联网、装全局），
# `llvm-tools-preview` 也要联网。本机 `/usr/bin/llvm-profdata` / `llvm-cov` 是发行版的 llvm 包，已在盘上。
# ⚠ **版本差**：rustc 自带的 LLVM 可能比系统那套新一个大版本（落地时 22 vs 21）。
#   TQ1 现打过一次小样：21 的 merge 读得了 22 编出来的 `.profraw`、`report` 的数与源码对得上。
#   哪天读不了，`merge` 那一步会报错退出 —— 本脚本对那一格 **fail-closed**，不出一张空表。
#
# # 它量什么
#
# 两侧各跑一遍**门禁里那条同样的** `cargo test`（插桩版、单独的 target 目录，不碰日常构建）：
#   bridge  —— `cd src/bridge && cargo test --workspace --exclude code-picture-core --lib`（= 门禁 `cargo` 那一格）
#   backend —— `cd src/backend && cargo test`（= 门禁 `backend` 那一格）
# 测试里 spawn 出去的子进程（后端二进制、`current_exe` 起的子测试）也是插桩的，它们的 `.profraw` 一并算进来。
# 报告的人群是**生产源码**：`src/bridge/src` · `src/bridge/crates/*/src` · `src/backend`；
# 测试树 `tests/`、vendor、依赖 crate 一律滤掉（`--ignore-filename-regex`）。
#
# # 它量不到什么
#
# - 只量「这一趟 `cargo test` 在**这台机器**上执行到了哪几行」：`#[ignore]` 的、要 `embedded_backends` cfg 的、
#   要 Xvfb / 真 sshd 的，这一趟没跑就不算 —— 读数是**门禁那两格**的覆盖，不是全部判据的覆盖。
# - 行被执行 ≠ 行被断言：源码扫描型判据读源码**文本**，一行生产代码都不执行 ⇒ 它们对这个数贡献为 0，
#   而那不代表它们没守东西（测试层分级见 `tests/bridge/crates/guard-core/test_tiers_tests.rs`）。
# - TS 那一半归 `npm run coverage`（vitest v8）＋ `tests/scripts/assert-coverage-floors.mjs`，本脚本不碰。
#
# 用法：
#   bash tests/scripts/rust-coverage.sh [bridge|backend|both]   # 默认 both
# 产物落 `<repo>/.build/coverage/<侧>/`（`.gitignore` 的 `/.build/` 已忽略）：
#   report.txt（逐文件表）· summary.json（`llvm-cov export -summary-only`）· 末尾一行 `RUST-COVERAGE <侧> 行 a/b`。
# 环境变量：`LLVM_PROFDATA` / `LLVM_COV` 指定工具（默认 PATH 上的 `llvm-profdata` / `llvm-cov`）。

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
which_side="${1:-both}"
profdata_bin="${LLVM_PROFDATA:-llvm-profdata}"
cov_bin="${LLVM_COV:-llvm-cov}"

# 🔴 **断网**：发行版的 llvm 带 debuginfod，而 Ubuntu 默认给了 `DEBUGINFOD_URLS=https://debuginfod.ubuntu.com`
#   ⇒ `llvm-cov report` 会去那个服务器取调试信息，在断网 / 沙箱里**挂着不动**（TQ1 落地那趟现打：
#   开着一个 socket、`poll` 睡着、CPU 两秒，报告一个字节都没写）。本脚本不许联网 ⇒ 清掉它。
export DEBUGINFOD_URLS=

for t in "$profdata_bin" "$cov_bin"; do
  command -v "$t" >/dev/null 2>&1 || {
    echo "rust-coverage: 找不到 \`$t\`（系统 llvm 包）。本脚本不装任何东西 —— 装好或用 LLVM_PROFDATA / LLVM_COV 指过去" >&2
    exit 2
  }
done

# 只留生产源码：测试树、vendor、依赖（registry / git checkout / 标准库）、构建目录一律滤掉。
ignore_re='(/\.cargo/|/rustc/|/tests/|/vendor/|/\.build/|/target/)'

# run_side <侧名> <cargo 所在目录> <cargo test 参数…>
run_side() {
  local side="$1" dir="$2"
  shift 2
  local out="$repo/.build/coverage/$side"
  local target="$repo/.build/cov-$side"
  rm -rf "$out"
  mkdir -p "$out/profraw"
  echo "== rust-coverage: $side（cd $dir && cargo test $*）"
  local rc=0
  (
    cd "$repo/$dir"
    RUSTFLAGS="-C instrument-coverage" \
      CARGO_TARGET_DIR="$target" \
      LLVM_PROFILE_FILE="$out/profraw/%p-%m.profraw" \
      cargo test "$@"
  ) >"$out/cargo-test.log" 2>&1 || rc=$?
  grep -E '^test result:' "$out/cargo-test.log" | sed "s/^/   [$side] /" || true
  if [ "$rc" -ne 0 ]; then
    echo "   [$side] ⚠ cargo test 退出码 $rc —— 覆盖率照出，但这一趟有红（日志 $out/cargo-test.log）"
  fi

  # 被测的目标文件：测试二进制（同一组参数 --no-run，不会重编）＋ 本侧的可执行文件（测试会 spawn 它）。
  local objs=()
  local exe
  while IFS= read -r exe; do
    [ -n "$exe" ] && objs+=("$exe")
  done < <(
    cd "$repo/$dir" &&
      RUSTFLAGS="-C instrument-coverage" CARGO_TARGET_DIR="$target" \
        cargo test "$@" --no-run --message-format=json 2>/dev/null |
      grep -o '"executable":"[^"]*"' | cut -d'"' -f4 | sort -u
  )
  for exe in "$target"/debug/cc-monitor-backend "$target"/debug/monitor; do
    [ -x "$exe" ] && objs+=("$exe")
  done
  if [ "${#objs[@]}" -eq 0 ]; then
    echo "   [$side] 🔴 一个被测目标文件都没找到 —— 读数作废" >&2
    return 1
  fi

  local raws
  raws=$(find "$out/profraw" -name '*.profraw' | wc -l)
  if [ "$raws" -eq 0 ]; then
    echo "   [$side] 🔴 一份 .profraw 都没产出 —— 插桩没生效（RUSTFLAGS 被覆盖？），读数作废" >&2
    return 1
  fi
  find "$out/profraw" -name '*.profraw' >"$out/profraw.list"
  "$profdata_bin" merge -sparse -f "$out/profraw.list" -o "$out/merged.profdata"

  local args=(-instr-profile="$out/merged.profdata" -ignore-filename-regex="$ignore_re")
  local first="${objs[0]}"
  local rest=()
  local o
  for o in "${objs[@]:1}"; do rest+=(-object "$o"); done
  "$cov_bin" report "${args[@]}" "$first" "${rest[@]}" >"$out/report.txt"
  "$cov_bin" export -summary-only "${args[@]}" "$first" "${rest[@]}" >"$out/summary.json"

  # 总行数 / 覆盖行数 取自 summary.json 的 totals.lines（不从表里抠数）。
  local line
  line=$(python3 -c '
import json, sys
d = json.load(open(sys.argv[1]))["data"][0]
t = d["totals"]["lines"]
files = len(d["files"])
print(f"{t[\"covered\"]}/{t[\"count\"]} 行（{t[\"percent\"]:.1f}%）· 生产源文件 {files} 份")
' "$out/summary.json")
  echo "RUST-COVERAGE $side $line · .profraw ${raws} 份 · 目标文件 ${#objs[@]} 个 · 表 $out/report.txt"
}

case "$which_side" in
  bridge | both) run_side bridge src/bridge --workspace --exclude code-picture-core --lib ;;
esac
case "$which_side" in
  backend | both) run_side backend src/backend ;;
esac
case "$which_side" in
  bridge | backend | both) ;;
  *)
    echo "用法：bash tests/scripts/rust-coverage.sh [bridge|backend|both]" >&2
    exit 2
    ;;
esac
