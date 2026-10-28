#!/usr/bin/env bash
# 用法（用户手敲）：在本仓任一棵树里 `bash tests/scripts/push-candidate.sh <合并工作树名>`
#   ① w8/<名> 强推到 origin/next（候选分支；ci.yml 在它上面跑与 main 同一套 job，不打标签、不发版）
#   ② 用 gh 等这一趟 CI 跑完
#   ③ 全部 job 绿 ⇒ 主仓 main `merge --ff-only w8/<名>` 并推 origin main；
#      有红 ⇒ 停下，打印红的 job、链接与失败日志落在哪；main 不动。
#   ④ 同一个提交在 main 上还会再跑一遍 CI：也等它跑完，红了照样印红的 job 与日志、退非 0（main 已经推出去了）。
# 主仓 = 这个仓的主工作树（`git worktree list` 的第一棵），它一直在 main 上，脚本不切分支。
# 私人扫描（要推的那一段提交的说明与改动）：私人词表与扫描脚本都不进仓，只认 PATH 上的 private-scan.sh；找不到 ⇒ 拒推。
# 依赖漏洞判据（`dep-advisories.mjs`，问 OSV）：判的是 w8/<名> 那个提交里的锁；红或判不了 ⇒ 拒推。
# 环境变量（一般不用设）：PUSH_FIND_MIN 每一趟等 run 出现的分钟数（默认 10）· PUSH_WAIT_MIN 每一趟等跑完的分钟数（默认 60）·
#   GATE_STATE_DIR 失败日志落在它下面的 logs/（默认 ${XDG_RUNTIME_DIR:-/tmp}/cc-monitor-gate）。
set -euo pipefail
case "${1:-}" in -h|--help) sed -n '2,12p' "$0"; exit 0 ;; esac
name=${1:?用法：bash tests/scripts/push-candidate.sh <合并工作树名>（推的是 w8/<名>）}
br="w8/$name"
scan=$(command -v private-scan.sh) \
  || { echo "push: PATH 上没有 private-scan.sh —— 要推的提交扫不了私人标识，不推；扫描脚本放上 PATH 再来" >&2; exit 1; }
find_min=${PUSH_FIND_MIN:-10}; wait_min=${PUSH_WAIT_MIN:-60}
logdir=${GATE_STATE_DIR:-${XDG_RUNTIME_DIR:-/tmp}/cc-monitor-gate}/logs
here=$(cd "$(dirname "$0")" && pwd)
main_tree=$(git worktree list --porcelain 2>/dev/null | sed -n '1s/^worktree //p')
[ -n "$main_tree" ] || { echo "push: 不在本仓的任一棵树里（git worktree list 没出主工作树）" >&2; exit 1; }
cd "$main_tree"

die() { echo "push: $*" >&2; exit 1; }
[ "$(git symbolic-ref --short HEAD 2>/dev/null)" = main ] || die "主仓不在 main 上（脚本不切分支），先回 main 再来"
sha=$(git rev-parse --verify --quiet "$br^{commit}") || die "没有分支 $br"
git merge-base --is-ancestor main "$sha" || die "$br 不是 main 的快进（先在合并工作树里 merge main）"
git fetch -q origin main
git merge-base --is-ancestor origin/main "$sha" || die "$br 不是 origin/main 的快进（远端 main 动过，先在合并工作树里 merge 它）"
gh auth status >/dev/null 2>&1 || die "gh 没登录（gh auth status）"
# 要推出去的这一段提交：说明与改动里不许有私人标识（树里干净不够——旧提交里的会跟着历史出去）
bash "$scan" --history "origin/main..$sha" || die "要推的提交历史里有私人标识（见上）；main 没动"
node "$here/dep-advisories.mjs" --rev "$sha" || die "依赖漏洞判据没过（见上：红 ⇒ 升锁或登记豁免，判不了 ⇒ OSV 问不到）；main 没动"
short=${sha:0:9}

# 等 <分支> 上 ci.yml 对 $sha 的那一趟 push 跑完；全部 job 绿 ⇒ 返回 0，否则印红的 job · 链接 · 失败日志并返回 1。
# 找不到 run / 等超时 ⇒ 带着 <没动的话> 直接退。
wait_ci() {
  local branch=$1 untouched=$2 run= url st last= t_end jobs concl red log
  echo "push: 等 CI（ci.yml · $branch · $short）出现…"
  t_end=$(( SECONDS + find_min * 60 ))
  while :; do
    run=$(gh run list --workflow ci.yml --branch "$branch" --commit "$sha" --event push -L 1 --json databaseId -q '.[0].databaseId // empty' 2>/dev/null || true)
    [ -n "$run" ] && break
    [ "$SECONDS" -lt "$t_end" ] || die "等了 $find_min 分钟还没看到 CI run（$branch @ $short）；$untouched。看 GitHub Actions 页面"
    sleep 15
  done
  url=$(gh run view "$run" --json url -q .url)
  echo "push: CI run $run（$branch）  $url"
  t_end=$(( SECONDS + wait_min * 60 ))
  while :; do
    st=$(gh run view "$run" --json status,conclusion,jobs \
          -q '"\(.status) \(.conclusion) \([.jobs[] | select(.status == "completed")] | length)/\(.jobs | length)"' 2>/dev/null || echo "? ? ?")
    [ "$st" != "$last" ] && { echo "push:   $(date +%H:%M:%S)  $st（状态 结论 跑完的 job/全部）"; last=$st; }
    case "$st" in completed\ *) break ;; esac
    [ "$SECONDS" -lt "$t_end" ] || die "等 $branch 上的 CI 等了 $wait_min 分钟还没跑完；$untouched。$url"
    sleep 30
  done
  jobs=$(gh run view "$run" --json jobs -q '.jobs[] | "\(.conclusion)\t\(.name)\t\(.url)"')
  concl=$(gh run view "$run" --json conclusion -q .conclusion)
  red=$(printf '%s\n' "$jobs" | awk -F'\t' 'NF && $1 != "success"')
  if [ "$concl" = success ] && [ -z "$red" ] && [ -n "$jobs" ]; then
    echo "push: $branch 上的 CI 全绿（$(printf '%s\n' "$jobs" | wc -l) 个 job）"
    return 0
  fi
  mkdir -p "$logdir"; log="$logdir/push-$name-$branch-$run.log"
  gh run view "$run" --log-failed > "$log" 2>&1 || true
  echo
  printf '%s\n' "${red:-（job 清单是空的）}" | awk -F'\t' '{ printf "  %-10s %s\n             %s\n", $1, $2, $3 }'
  echo "  失败日志：$log（gh run view $run --log-failed）"
  echo "  整趟：$url（run 结论 $concl）"
  return 1
}

echo "push: $br @ $short → origin/next（强推）"
git push --force origin "$sha:refs/heads/next"
wait_ci next "main 没动" || { echo "PUSH: 红 —— CI 在 next @ $short 上没全绿，main 没动"; exit 1; }

echo "push: main 快进到 $br"
git merge --ff-only "$br"
git push origin main
# 快进以后同一个提交在 main 上还会再跑一遍（10-16 那次 main 上那一遍红了没人看见）：也等它。
wait_ci main "main 已经是 $short" \
  || { echo "PUSH: 红 —— main 已快进到 $short，但 main 上那一遍 CI 没全绿（见上）"; exit 1; }
echo "PUSH: OK —— main = $short（CI 在 next 与 main 上那两遍都全绿）"
