#!/usr/bin/env bash
# 从**提交状态**编一次 —— 而不是从工作树。
#
# # 为什么需要它（这不是仪式，它逮到过一次二十轮没人发现的事故）
#
# 2026-08-04 实测：`gate-core = { path = "crates/gate-core" }` 这条依赖是 F03 加进
# `src-tauri/Cargo.toml` 的，而那个文件里同时有**用户自己的** `[profile.dev]` 改动。
# 定框 §5 的红线写着「不许提交用户那段；必须带我方改动时用 blob-replay 只提交我方那几行」。
# 而实际做法只做了「排除」那半（`git restore --staged src-tauri/Cargo.toml`），
# blob-replay 那半从没做过 ⇒ **我方对该文件的改动一次都没落盘**，
# 提交状态的 `main` 在任何平台上都编不过，而这持续了约二十轮。
#
# 每一轮的读数（`cargo test` 875 passed、fmt 全绿、clippy 逐条同集）**都是真的** ——
# 它们量的是**工作树**，工作树里有那一行。**没有任何一道门量过提交状态。**
#
# ★ 教训不是「我忘了」，是**门禁的量点错了**：只要工作树里存在未提交的第三方改动
# （用户的、实验性的、`.gitignore` 之外的），「工作树绿」与「提交状态绿」就是两件事。
#
# ⚠ 为什么 CI 接不住这件：本仓的红线是**不 push**，那些 commit 只在本机 ——
# CI 从来没见过它们。所以这道门必须在本机跑。
#
# 用法：`scripts/verify-committed-state.sh [git-ref]`（默认 HEAD）
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
if grep -q '^\[profile\.dev\]' "$WT/src-tauri/Cargo.toml"; then
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

run monitor-lib   "$WT/src-tauri"           cargo check --lib
run daemon        "$WT/remote-daemon-proto" cargo check --all-targets
# 跨 target：daemon 是纯 Rust，`check` 不需要链接器（monitor 不行 —— 它有 C 依赖要 lib.exe）。
if rustup target list --installed | grep -q x86_64-pc-windows-msvc; then
  run daemon-win  "$WT/remote-daemon-proto" cargo check --all-targets --target x86_64-pc-windows-msvc
else
  echo "   skip daemon-win（没装 x86_64-pc-windows-msvc target）"
fi

[ "$fail" -eq 0 ] && echo "== 提交状态编得过 ==" || { echo "== 提交状态编不过 =="; exit 1; }
