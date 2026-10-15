#!/usr/bin/env bash
# 生成物有没有落在提交外：仓里每个 `generated/` 目录下，与 HEAD 不同（改了 · 删了）或还没跟踪的文件逐份印出。
# 用法：bash tests/scripts/generated-drift.sh   退 0 = 一份都没有；1 = 有（印出来的就是）；2 = 判不了（不在 git 仓里）。
#   生成物（ts-rs 导出等）由 cargo test 现写；只在工作树里对、没提交的，合并树上才红 ⇒ quick-check 末尾与门禁 `generated` 格都调它。
#   生成物目录一律叫 `generated`（判据 ts_export_guard 钉着登记的落点都叫这个名字），这里按名字找，不另列目录。
set -u
root=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "generated-drift: 不在 git 仓里 —— 判不了"; exit 2; }
out=$(git -C "$root" -c core.quotepath=false status --porcelain --untracked-files=all -- ':(glob)**/generated/**') \
  || { echo "generated-drift: git status 失败 —— 判不了"; exit 2; }
[ -z "$out" ] && exit 0
printf '%s\n' "$out"
exit 1
