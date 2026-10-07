#!/usr/bin/env bash
# 从提交状态编一次，而不是从工作树：工作树里有未提交的改动（用户自己的 `[profile.dev]` 之类）时，
# 「工作树绿」与「提交状态绿」是两件事，其余门禁量的都是工作树。本仓不推送，CI 见不到这些提交，所以在本机跑。
# 用法：`tests/scripts/verify-committed-state.sh [git-ref]`（默认 HEAD）
set -euo pipefail

REF="${1:-HEAD}"
ROOT="$(git rev-parse --show-toplevel)"
WT="$(mktemp -d -t verify-committed-XXXXXX)"

cleanup() {
  cd "$ROOT"
  git worktree remove --force "$WT" 2>/dev/null || rm -rf "$WT"
  git worktree prune
}
trap cleanup EXIT

cd "$ROOT"
git worktree add --detach "$WT" "$REF" >/dev/null 2>&1
echo "== 从 $REF 检出到 $WT =="
echo "   $(git -C "$WT" log --oneline -1)"

# ★ 中间量自检：这份检出必须**不含**工作树里那些未提交的东西，否则本脚本在量错的东西。
if grep -q '^\[profile\.dev\]' "$WT/src/frontend/shell/Cargo.toml"; then
  echo "!! 检出里出现了 [profile.dev] —— 那是用户未提交的改动，说明 REF 不是提交状态" >&2
  exit 3
fi

fail=0
run() { # run <名字> <目录> <命令...>
  local name="$1" dir="$2"; shift 2
  local log="$WT/.verify-$name.log"
  if (cd "$dir" && "$@" >"$log" 2>&1); then
    echo "   ok   $name"
  else
    echo "   FAIL $name  —— 见 $log"
    tail -20 "$log" | sed 's/^/        /'
    fail=1
  fi
}

run monitor-lib   "$WT/src/frontend/shell"           cargo check --lib
run backend        "$WT/src/backend" cargo check --all-targets

# ── 跨 target（`backend-win`）──
# `ring` 的 build script 要编 C，cc-rs 在 Linux 上给 `*-pc-windows-msvc` 找不到 `lib.exe` ⇒ 不装 zig 时它
#   根本走不到我们的代码，那个红没有信息。出路照 CI 的 backend job：装了 `zig` 就给上那三个环境变量
#   （`zig cc` 发 COFF、`zig lib` 当归档器；`check` 不链接，只为让 build script 走完）。
# 判绿的口径是「有没有走到我们的代码」：needle 用包名 `cc-monitor-backend`（不是目录名）。
#   走到了 + exit 0 ⇒ ok · 走到了 + exit != 0 ⇒ FAIL · 没走到 ⇒ 量不到，并改变最终结论。
#   每趟都是新检出、自己的 `target/`，needle 必然会印；外面设了指向热目录的 `CARGO_TARGET_DIR` 时没印也按量不到记。
skipped=""
if rustup target list --installed | grep -q x86_64-pc-windows-msvc; then
  # 基座恒为 `env`（不用空数组）：`set -u` 下展开空数组在老 bash 上报错。
  winenv=(env)
  if command -v zig >/dev/null 2>&1; then
    winenv+=(
      CC_x86_64_pc_windows_msvc="zig cc"
      CFLAGS_x86_64_pc_windows_msvc="--target=x86_64-windows-gnu"
      AR_x86_64_pc_windows_msvc="zig lib")
  fi
  win_fail_before="$fail"
  run backend-win  "$WT/src/backend" "${winenv[@]}" cargo check --all-targets --target x86_64-pc-windows-msvc
  # 重判：`run` 只看退出码；没走到我们的代码 ⇒ 撤回 `run` 记下的那一笔，改记成「没有读数」。
  if ! grep -q 'Checking cc-monitor-backend' "$WT/.verify-backend-win.log"; then
    fail="$win_fail_before"
    if command -v zig >/dev/null 2>&1; then
      why="装了 zig 也没走到我们的代码"
    else
      why="没装 zig，构建卡在 \`ring\` 的 C 构建脚本上"
    fi
    skipped="backend-win（$why —— 日志里 \`Checking cc-monitor-backend\` 命中 0）"
    echo "   量不到 backend-win —— $why"
    echo "        ⇒ **上面那条 FAIL（如果打了）不算数**：它量的不是我们的代码。"
    echo "        见 $WT/.verify-backend-win.log；装法照抄 .github/workflows/ci.yml 的 backend job（mlugg/setup-zig，0.14.0）"
  fi
else
  skipped="backend-win（没装 x86_64-pc-windows-msvc target）"
  echo "   skip $skipped"
fi

# 跳过必须改变结论，不能只多打一行：读的人看的是最后那一行。没有当次的 Windows 读数时，结论只许写成「在 Linux 上编得过」。
# 「量不到」与「跳过」走同一个变量 `skipped`：对结论的意义相同，降级结论只有一条住址
#   （`shared_crate_registry::a_skipped_windows_check_cannot_look_like_a_full_pass` 钉着）。
if [ "$fail" -ne 0 ]; then
  echo "== 提交状态编不过 =="
  exit 1
elif [ -n "$skipped" ]; then
  echo "== 提交状态在 Linux 上编得过 —— ⚠ $skipped，Windows 那半本次没量（定框 E8）=="
else
  echo "== 提交状态编得过 =="
fi
