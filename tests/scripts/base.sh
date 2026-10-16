#!/usr/bin/env bash
# 这棵树「自己改了哪些」从哪儿算起：HEAD 与分支上游起点的 merge-base，印 `<提交> <对着谁>` 一行。
# 用法：bash tests/scripts/base.sh   （quick-check 挑段、gate-affected 挑格都用它）
#   对着谁：分支设了上游（`git branch --set-upstream-to=<合并用的分支>`，或建分支时带 `--track`）⇒ 上游；
#           没设 ⇒ origin/main；没有 origin/main ⇒ main。都找不到 ⇒ 退 1、什么都不印。
#   取 merge-base，所以上游往前走了（合并用的分支又收了别的分支）也只算这棵树自己的改动，不必先合 main。
set -u
for ref in '@{upstream}' origin/main main; do
  b=$(git merge-base HEAD "$ref" 2>/dev/null) || continue
  [ "$ref" = '@{upstream}' ] && ref=$(git rev-parse --abbrev-ref '@{upstream}')
  echo "$b $ref"; exit 0
done
exit 1
