#!/bin/bash
# 结构性守卫：`shared/**` 里带 shebang 的文件，在 **git 里**必须记成 100755。
#
# 为什么需要这条：本仓 `core.fileMode=false`（`git config core.fileMode`），
# git **忽略文件系统的可执行位**——所以文件在盘上是 755、`git add` 之后仍被记成 100644，
# 而且本地怎么跑都正常（本地用的是盘上那份），**只有干净 checkout 才会暴露**。
# 已经踩过两次：
#   ① R00：`shared/ccm` 记成 644 → CI 上 `-bash: …/shared/ccm: Permission denied`，
#      e2e 连红三轮才靠 pane dump 定位到；
#   ② B01：13 个 cc-bus 脚本 `git add` 后全部是 100644。
# 靠"记得 git update-index --chmod=+x"是纪律，这条脚本把它变成门禁。
set -o pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
cd "$REPO" || exit 1

fail=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  # 只看有 shebang 的（examples/*.service、SKILL.md、*.md 这些不该可执行）
  head -c 2 "$f" 2>/dev/null | grep -q '#!' || continue
  mode="$(git ls-files -s -- "$f" | awk '{print $1}')"
  if [ "$mode" != "100755" ]; then
    printf 'FAIL | %s 带 shebang 但 git 里是 %s（应为 100755）\n' "$f" "$mode"
    printf '       修法: git update-index --chmod=+x %s\n' "$f"
    fail=$((fail+1))
  else
    printf 'PASS | %s = 100755\n' "$f"
  fi
done < <(git ls-files shared/)

echo
if [ "$fail" -gt 0 ]; then
  echo "===== 合计 FAIL=$fail —— 干净 checkout 上这些文件将不可执行 ====="
  exit 1
fi
echo "===== 合计 PASS，shared/ 下所有带 shebang 的文件在 git 里均为 100755 ====="
