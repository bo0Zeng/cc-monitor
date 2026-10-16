#!/usr/bin/env bash
# 重打金样的唯一入口。金样判据只比不写：现打的与仓里那份不一样时，现打的那份落在本树 `.build/golden/<仓相对路径>`、判据红
#   （`tests/backend/golden.rs`）。对过那几份之后跑本脚本，逐份拷回仓里原处、删掉现打的那份；下一趟测试照常比。
# 用法：bash tests/scripts/bless-golden.sh   退 0 = 拷回了（逐份印出）；1 = 没有现打的金样。
set -euo pipefail
root=$(git rev-parse --show-toplevel)
fresh="$root/.build/golden"
n=0
if [ -d "$fresh" ]; then
  while IFS= read -r -d '' f; do
    rel=${f#"$fresh"/}
    mkdir -p -- "$(dirname -- "$root/$rel")"
    cp -- "$f" "$root/$rel"
    rm -- "$f"
    echo "重打 $rel"
    n=$((n + 1))
  done < <(find "$fresh" -type f -print0)
  find "$fresh" -depth -type d -empty -delete
fi
[ "$n" -gt 0 ] || { echo "bless-golden: $fresh 下没有现打的金样（先跑红了的那几条判据）"; exit 1; }
