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
# ⇒ 本脚本把三道门收成一条命令，并且**只在最后打一行裁决**（`GATE: OK` / `GATE: FAIL …`）。
# 用法就一句纪律：**先跑它、看见 `GATE: OK`，再单独敲 `git commit`。**
# ⚠ 它**故意不提交任何东西**、也不接 `--commit` 之类的开关 —— 那会把刚拆开的两件事又焊回去。
#
# ⚠ 覆盖面如实写：它跑的是**工作树**的三道门。
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

run_gate cargo   bash -c 'cd src-tauri && cargo test --lib 2>&1'
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
  echo "GATE: OK —— 三道门 + pb check 全绿，可以出货"
  exit 0
fi
printf 'GATE: FAIL —— %s\n' "$(IFS='；'; echo "${fails[*]}")"
echo "**别提交**。先修，再重跑本脚本。"
exit 1
