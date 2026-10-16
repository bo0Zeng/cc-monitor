#!/usr/bin/env bash
# 按改动挑格跑门禁：`git diff --name-only <基>...HEAD` 改到的路径 → 受影响的格（映射表 tests/scripts/gate-affected/map，理由逐行写在表里）
#   ＋ 恒跑的便宜格，用 gate.sh 的 GATE_ONLY 跑、出收据，再跑私人扫描。表不认识的路径 ⇒ 整趟退回全量（等于 gate-clean.sh）。
# 用法：在干净的树里 `bash tests/scripts/gate-affected.sh [--base <ref>] [--trust-ci] [--dry-run]`
#   --base <ref>  跟谁比（取 merge-base，只算这棵树自己的改动）；不给 ⇒ base.sh 那一个：分支上游，没设就 origin/main
#   --trust-ci    合并工作树推候选分支 next 之前用（push-candidate.sh 推 next、等 CI 全绿才快进 main）：CI 会跑的格这一趟不跑，只跑
#                 恒跑的便宜格 ＋ 被改到的、本机才有的格（muslbuild 等）＋ @keep-local 那几格 ＋ 被改到被测对象的 e2e 套件（逐套现算）；
#                 先跑一遍 tests/scripts/quick-check.sh --all（重的四段不挑、全跑）兜跨路冲突
#   --dry-run     只印挑格的计划，不跑
# 规矩照 gate-clean.sh：工作树必须干净；经 heavy.sh 跑（它摘掉会话继承来的变量，表在 session-vars）；排门禁锁；拿到锁时负载 > 核数 × 0.6 就串跑。
# 门禁锁 · 槽位在 GATE_STATE_DIR（默认 ${XDG_RUNTIME_DIR:-/tmp}/cc-monitor-gate，几棵树共用一把锁）。
# 私人扫描：私人词表与扫描脚本都不进仓，只认 PATH 上的 private-scan.sh；找不到 ⇒ 开跑前就停（没判的不许说绿）。
# 收据：挑格跑的写 .build/gate-affected-receipt.json（裁决是 PARTIAL，不许拿它出货）；退回全量的写默认的 .build/gate-receipt.json。
# 计划另存 .build/gate-affected-plan.txt。末行：`AFFECTED: OK …` / `AFFECTED: 红 …`。
set -uo pipefail
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
state=${GATE_STATE_DIR:-${XDG_RUNTIME_DIR:-/tmp}/cc-monitor-gate}
base_ref=; trust=; dry=
while [ $# -gt 0 ]; do
  case "$1" in
    --base) base_ref=${2:?--base 要一个 ref}; shift 2 ;;
    --trust-ci) trust=--trust-ci; shift ;;
    --dry-run) dry=1; shift ;;
    -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
    *) echo "gate-affected: 不认识的参数 $1" >&2; exit 2 ;;
  esac
done
[ -n "$dry" ] || scan=$(command -v private-scan.sh) \
  || { echo "gate-affected: PATH 上没有 private-scan.sh —— 私人扫描判不了，门禁不能说绿；扫描脚本放上 PATH 再来"; exit 2; }
root=$(git rev-parse --show-toplevel) || exit 2
cd "$root" || exit 2

if [ -n "$(git status --porcelain --untracked-files=normal)" ]; then
  echo "gate-affected: 工作树不干净 —— 门禁判的是盘上这棵树，挑格按的是提交过的改动；先提交（或 stash）再跑"
  git status --short | head -10; exit 2
fi
if [ -n "$base_ref" ]; then
  base=$(git merge-base HEAD "$base_ref") || { echo "gate-affected: 找不到 $base_ref 与 HEAD 的 merge-base"; exit 2; }
else
  line=$(bash "$HERE/base.sh") || { echo "gate-affected: 找不到起点（没有上游 · origin/main · main）—— 用 --base 指一个"; exit 2; }
  base=${line%% *}; base_ref=${line#* }
fi
mkdir -p .build "$state"
plan_file=.build/gate-affected-plan.txt
{ echo "gate-affected 计划 —— HEAD $(git log --oneline -1 | cut -c1-70)"
  echo "  基：$base_ref @ $(git log --oneline -1 "$base" | cut -c1-60)（$(git rev-list --count "$base..HEAD") 个提交）${trust:+ · --trust-ci}"
  git -c core.quotepath=false diff --name-status -M "$base...HEAD" | python3 "$HERE/gate-affected/plan.py" . $trust
} > "$plan_file"; prc=$?
cat "$plan_file"
[ $prc -eq 0 ] || { echo "AFFECTED: 红 —— 挑格那一步停了（映射表与 gate.sh 对不上，见上）"; exit 2; }
mode=$(sed -n 's/^PLAN-MODE: //p' "$plan_file"); cells=$(sed -n 's/^PLAN-CELLS: //p' "$plan_file")
[ -n "$dry" ] && exit 0

# 每路 cargo 并行数：几十棵树同时编时，一路多给 job 只是多占内存、抢别人的时间片。
jobs=${CARGO_BUILD_JOBS:-4}
fails=()

if [ -n "$trust" ]; then
  echo; echo "── quick-check --all（两棵树整套 cargo test --lib ＋ 整套 vitest ＋ 手机那一套）"
  qc="$root/tests/scripts/quick-check.sh"   # 判的是哪棵树就用哪棵树里那份
  [ -f "$qc" ] || qc="$HERE/quick-check.sh"
  CARGO_BUILD_JOBS=$jobs GATE_STATE_DIR="$state" bash "$HERE/heavy.sh" bash "$qc" --all || fails+=("quick-check")
fi

if [ ${#fails[@]} -eq 0 ]; then
  echo; echo "── 门禁（$( [ "$mode" = full ] && echo 全量 || echo "GATE_ONLY $(echo $cells | wc -w) 格")）"
  if [ "$mode" = full ]; then only_env=(); receipt=.build/gate-receipt.json
  else only_env=(GATE_ONLY="$cells"); receipt=.build/gate-affected-receipt.json; fi
  # 门禁放进自己的 systemd scope（heavy.sh），内存吃紧时只杀这一份（10-10 14:53）。
  env CARGO_BUILD_JOBS="$jobs" "${only_env[@]}" GATE_RECEIPT="$receipt" GATE_STATE_DIR="$state" HEAVY_MEM_HIGH=18G HEAVY_MEM_MAX=22G \
    bash "$HERE/heavy.sh" flock "$state/gate.lock" bash -c '
      cores=$(nproc); load=$(cut -d" " -f1 /proc/loadavg)
      if [ -z "${GATE_SERIAL:-}" ] && awk -v l="$load" -v c="$cores" "BEGIN{exit !(l > c*0.6)}"; then
        echo "gate-affected: 负载 $load（${cores} 核）⇒ 这一趟串跑"; export GATE_SERIAL=1
      fi
      exec npm run gate' || fails+=("门禁")
fi

echo; echo "── 私人扫描"
bash "$scan" . || fails+=("私人扫描")

echo
if [ ${#fails[@]} -eq 0 ]; then
  if [ "$mode" = full ]; then echo "AFFECTED: OK —— 退回了全量，GATE: OK（可以出货的那种）"
  else echo "AFFECTED: OK —— 挑出的 $(echo $cells | wc -w) 格全绿（PARTIAL，不是出货用的 GATE: OK${trust:+；推送走 push-candidate.sh：先推 next、CI 全绿才快进 main}）"; fi
  exit 0
else echo "AFFECTED: 红 —— ${fails[*]}"; exit 1; fi
