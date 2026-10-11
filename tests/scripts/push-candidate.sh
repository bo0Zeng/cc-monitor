#!/usr/bin/env bash
# 用法（用户手敲）：在本仓任一棵树里 `bash tests/scripts/push-candidate.sh <合并树名>`
#   ① w8/<名> 强推到 origin/next（候选分支；ci.yml 在它上面跑与 main 同一套 job，不打标签、不发版）
#   ② 用 gh 等这一趟 CI 跑完
#   ③ 全部 job 绿 ⇒ 主仓 main `merge --ff-only w8/<名>` 并推 origin main；
#      有红 ⇒ 停下，打印红的 job、链接与失败日志落在哪；main 不动。
# 主仓 = 这个仓的主工作树（`git worktree list` 的第一棵），它一直在 main 上，脚本不切分支。
# 私人扫描（要推的那一段提交的说明与改动）：私人词表不进仓 —— 取 PRIVATE_SCAN 指的那份 private-scan.sh，没设就找 PATH 上的；
#   都没有 ⇒ 跳过并说明（这台机器上没有词表，扫不了）。
# 环境变量（一般不用设）：PUSH_FIND_MIN 等 run 出现的分钟数（默认 10）· PUSH_WAIT_MIN 等跑完的分钟数（默认 60）·
#   GATE_STATE_DIR 失败日志落在它下面的 logs/（默认 ${XDG_RUNTIME_DIR:-/tmp}/cc-monitor-gate）。
set -euo pipefail
case "${1:-}" in -h|--help) sed -n '2,11p' "$0"; exit 0 ;; esac
name=${1:?用法：bash tests/scripts/push-candidate.sh <合并树名>（推的是 w8/<名>）}
br="w8/$name"
find_min=${PUSH_FIND_MIN:-10}; wait_min=${PUSH_WAIT_MIN:-60}
logdir=${GATE_STATE_DIR:-${XDG_RUNTIME_DIR:-/tmp}/cc-monitor-gate}/logs
main_tree=$(git worktree list --porcelain 2>/dev/null | sed -n '1s/^worktree //p')
[ -n "$main_tree" ] || { echo "push: 不在本仓的任一棵树里（git worktree list 没出主工作树）" >&2; exit 1; }
cd "$main_tree"

die() { echo "push: $*" >&2; exit 1; }
[ "$(git symbolic-ref --short HEAD 2>/dev/null)" = main ] || die "主仓不在 main 上（脚本不切分支），先回 main 再来"
sha=$(git rev-parse --verify --quiet "$br^{commit}") || die "没有分支 $br"
git merge-base --is-ancestor main "$sha" || die "$br 不是 main 的快进（先在合并树里 merge main）"
git fetch -q origin main
git merge-base --is-ancestor origin/main "$sha" || die "$br 不是 origin/main 的快进（远端 main 动过，先在合并树里 merge 它）"
git show "$sha:.github/workflows/ci.yml" | grep -qE '^    branches: \[[^]]*\bnext\b' \
  || die "$br 的 ci.yml 的 push 触发里没有 next ⇒ 推候选分支不会起 CI；这棵树要先带上「CI 认候选分支」那一改"
gh auth status >/dev/null 2>&1 || die "gh 没登录（gh auth status）"
# 要推出去的这一段提交：说明与改动里不许有私人标识（树里干净不够——旧提交里的会跟着历史出去）
scan=${PRIVATE_SCAN:-$(command -v private-scan.sh || true)}
if [ -n "$scan" ] && [ -f "$scan" ]; then
  bash "$scan" --history "origin/main..$sha" || die "要推的提交历史里有私人标识（见上）；main 没动"
else
  echo "push: 跳过私人扫描 —— 没有扫描脚本（PRIVATE_SCAN 没设、PATH 上也没有 private-scan.sh）；私人词表不进仓，只在有它的机器上扫"
fi

short=${sha:0:9}
echo "push: $br @ $short → origin/next（强推）"
git push --force origin "$sha:refs/heads/next"

echo "push: 等 CI（ci.yml · next · $short）出现…"
run=; t_end=$(( SECONDS + find_min * 60 ))
while :; do
  run=$(gh run list --workflow ci.yml --branch next --commit "$sha" --event push -L 1 --json databaseId -q '.[0].databaseId // empty' 2>/dev/null || true)
  [ -n "$run" ] && break
  [ "$SECONDS" -lt "$t_end" ] || die "推了 $find_min 分钟还没看到 CI run（next @ $short）；main 没动。看 GitHub Actions 页面"
  sleep 15
done
url=$(gh run view "$run" --json url -q .url)
echo "push: CI run $run  $url"

t_end=$(( SECONDS + wait_min * 60 )); last=
while :; do
  st=$(gh run view "$run" --json status,conclusion,jobs \
        -q '"\(.status) \(.conclusion) \([.jobs[] | select(.status == "completed")] | length)/\(.jobs | length)"' 2>/dev/null || echo "? ? ?")
  [ "$st" != "$last" ] && { echo "push:   $(date +%H:%M:%S)  $st（状态 结论 跑完的 job/全部）"; last=$st; }
  case "$st" in completed\ *) break ;; esac
  [ "$SECONDS" -lt "$t_end" ] || die "等 CI 等了 $wait_min 分钟还没跑完；main 没动。$url"
  sleep 30
done

jobs=$(gh run view "$run" --json jobs -q '.jobs[] | "\(.conclusion)\t\(.name)\t\(.url)"')
concl=$(gh run view "$run" --json conclusion -q .conclusion)
red=$(printf '%s\n' "$jobs" | awk -F'\t' 'NF && $1 != "success"')
if [ "$concl" != success ] || [ -n "$red" ] || [ -z "$jobs" ]; then
  mkdir -p "$logdir"; log="$logdir/push-$name-$run.log"
  gh run view "$run" --log-failed > "$log" 2>&1 || true
  echo
  echo "PUSH: 红 —— CI 在 next @ $short 上没全绿（run 结论 $concl），main 没动"
  printf '%s\n' "${red:-（job 清单是空的）}" | awk -F'\t' '{ printf "  %-10s %s\n             %s\n", $1, $2, $3 }'
  echo "  失败日志：$log（gh run view $run --log-failed）"
  echo "  整趟：$url"
  exit 1
fi
echo "push: CI 全绿（$(printf '%s\n' "$jobs" | wc -l) 个 job）⇒ main 快进到 $br"
git merge --ff-only "$br"
git push origin main
echo "PUSH: OK —— main = $short（CI run $run 在同一个提交上全绿）"
