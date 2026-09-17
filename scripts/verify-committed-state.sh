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
run daemon        "$WT/src/backend" cargo check --all-targets

# ── 跨 target（`daemon-win`）：**这一格的前提早就作废了，08-25 起** 〔`K-R52` 09-11 订正〕──
#
# 这里原本写着一句前提：「daemon 是纯 Rust，`check` 不需要链接器（monitor 不行 —— 它有 C
# 依赖要 lib.exe）」。**那句话自 `K-H1` 引入 TLS（`rustls` → `ring`）起就是假的** ——
# `ring` 的 build script 要**编 C**，而 cc-rs 在 Linux 上给 `*-pc-windows-msvc`
# 找不到 MSVC 的工具链。注释没跟着改，于是这一格看起来还在守着什么。
#
# 🔴 **它今天量不到我们的代码，这是现打的读数**（`K-R52` 09-11，在沙箱里对 `d231e50` 跑）：
#   `cargo check --all-targets --target x86_64-pc-windows-msvc` ⇒ **EXIT=101**、
#   `error occurred in cc-rs: failed to find tool "lib.exe"`、
#   而 `Checking cc-monitor-remote` 命中 **0** —— 它**根本没走到我们这一行**。
#   ⇒ 那个红**不是**「Windows 编不过」的证据，它什么都不是。
#   一道永远红、而红得没有信息的门，与一道「跑了就绿」的假门是同一种东西。
#
# ★ 出路照抄 CI（`.github/workflows/ci.yml` 的 daemon job）：装了 `zig` 就把那三个
#   环境变量给上 —— `zig cc` 发 COFF、自带 mingw-w64 的 libc 头、`zig lib` 就是 cc-rs
#   缺的那个归档器。`cargo check` 不链接 ⇒ 那份 `.lib` 从头到尾没人用，
#   它存在只是为了让 build script 走完、好让检查走到我们的代码。
#
# 🔴 **判绿的口径不是退出码，是「它有没有走到我们的代码」**：
#   needle 用**包名** `cc-monitor-remote`，不是目录名 `src/backend`
#   （拿目录名当 needle 会永远数出 0 —— CI 那一步的头注逐字记着这个坑）。
#   三种结局，**各自不同的结论**：
#     · 走到了 + exit 0        ⇒ ok，这一格真的量到了东西
#     · 走到了 + exit != 0     ⇒ FAIL，那是真的编不过
#     · **没走到**             ⇒ **量不到**（既不算过也不算真红），并且**改变最终结论**
#   ⚠ 冷缓存前提：本脚本每趟都 `git worktree add` 一份新检出、用它自己的 `target/`
#     ⇒ 这一趟必然真编、needle 必然会印。若外面设了 `CARGO_TARGET_DIR` 指到一个热目录，
#     needle 可能因为「没活可干」而不出现 —— 那时本格按**量不到**记（fail-closed），
#     不按绿记。
skipped=""
if rustup target list --installed | grep -q x86_64-pc-windows-msvc; then
  # ⚠ 基座恒为 `env`（而不是空数组）：`set -u` 下展开一个空数组在老 bash 上直接报错，
  #   而 `env cargo …` 与 `cargo …` 等价 ⇒ 两条路同形，少一个只在某台机器上才现形的分叉。
  winenv=(env)
  if command -v zig >/dev/null 2>&1; then
    winenv+=(
      CC_x86_64_pc_windows_msvc="zig cc"
      CFLAGS_x86_64_pc_windows_msvc="--target=x86_64-windows-gnu"
      AR_x86_64_pc_windows_msvc="zig lib")
  fi
  win_fail_before="$fail"
  run daemon-win  "$WT/src/backend" "${winenv[@]}" cargo check --all-targets --target x86_64-pc-windows-msvc
  # 🔴 **重判**：`run` 只看退出码，而退出码在这一格上说明不了事（见上面那一段）。
  #    没走到我们的代码 ⇒ 把 `run` 刚记下的那一笔**撤回**，改记成「没有读数」。
  if ! grep -q 'Checking cc-monitor-remote' "$WT/.verify-daemon-win.log"; then
    fail="$win_fail_before"
    if command -v zig >/dev/null 2>&1; then
      why="装了 zig 也没走到我们的代码"
    else
      why="没装 zig，构建卡在 \`ring\` 的 C 构建脚本上"
    fi
    skipped="daemon-win（$why —— 日志里 \`Checking cc-monitor-remote\` 命中 0）"
    echo "   量不到 daemon-win —— $why"
    echo "        ⇒ **上面那条 FAIL（如果打了）不算数**：它量的不是我们的代码。"
    echo "        见 $WT/.verify-daemon-win.log；装法照抄 .github/workflows/ci.yml 的 daemon job（mlugg/setup-zig，0.14.0）"
  fi
else
  skipped="daemon-win（没装 x86_64-pc-windows-msvc target）"
  echo "   skip $skipped"
fi

# ★ **跳过必须改变结论，不能只多打一行**〔audit-0805 08-08〕。
#
# 08-08 真路实测：把机器上的 windows target 拿掉之后，本脚本打完 `skip daemon-win`
# 仍然原样输出「== 提交状态编得过 ==」并 exit 0 —— 而读门禁的人（和 loop 里的我）
# 读的就是最后那一行。**三项还在** ≠ **三项都跑了**。
#
# 定框 E8 逐字写着：没有当次的 Windows 证据时，任何「全绿」结论只许写成
# 「Linux 上全绿」。跨 target check 在本机只有这一处真跑（`ci.yml` 那条要 push 才动，
# 而本仓红线是不 push）⇒ 它一跳过，Windows 那半**本次就是没量**，结论必须自己说出来。
#
# 🔴 〔`K-R52` 09-11〕**「量不到」与「跳过」走的是同一个变量 `skipped`，这是刻意的。**
#    两者在终端上长得不一样（一个印 `skip`、一个印 `量不到`），但它们对**结论**的意义
#    **完全相同**：Windows 那半本次没有读数。⇒ 降级结论只许有**一条**住址
#   （`shared_crate_registry::a_skipped_windows_check_cannot_look_like_a_full_pass`
#    逐字钉着「含『Linux 上编得过』的行恰好 1 行」—— 本轮先分成两条，当场被它逮住，
#    **那条判据干对了活**：两条降级结论就是两句会各自漂移的话）。
#    先前 `daemon-win` 卡在 `ring` 上那一形既没走 `skipped`、也没被识别，
#    它只是让 `fail=1`、把结论写成「提交状态编不过」——
#    于是**一个与我们的代码无关的失败，冒充了一次关于我们的代码的读数**。
if [ "$fail" -ne 0 ]; then
  echo "== 提交状态编不过 =="
  exit 1
elif [ -n "$skipped" ]; then
  echo "== 提交状态在 Linux 上编得过 —— ⚠ $skipped，Windows 那半本次没量（定框 E8）=="
else
  echo "== 提交状态编得过 =="
fi
