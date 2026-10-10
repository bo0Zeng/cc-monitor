#!/usr/bin/env bash
# 门禁条数的唯一口径：find 去重仓根 .build/mobile/<模块>/test-results 各目录，逐目录打印条数，再求和。
# 注意：两个 glob 相加（test*/ 与 test/）会把 test/ 数两次，所以这里用 find。
#
# 用法：先跑全量门禁，再跑这个（它只读 .build/mobile/ 里已有的 results，自己不跑测试）：
#   . ./scripts/env.sh && ./gradlew ktlintCheck testDebugUnitTest :core-claude:test :core-remote:test
#   ./scripts/gate-count.sh
#
# 注意：test-results/ 反映的是最后一次跑了什么。用 --tests 过滤跑过之后，
# 那个目录只剩被过滤出的几条，加起来的数就偏小。所以下面打印每个目录的最后写入时间。
set -euo pipefail
cd "$(dirname "$0")/../../../.build/mobile" 2>/dev/null || { echo "没有 .build/mobile —— 先跑门禁。" >&2; exit 1; }

mapfile -t dirs < <(
  find . -path '*/test-results/*' -name '*.xml' -not -path '*/tmp/*' 2>/dev/null |
    sed 's#/[^/]*\.xml$##' | sort -u
)
if [ ${#dirs[@]} -eq 0 ]; then
  echo "没有任何 test-results —— 先跑门禁。" >&2
  exit 1
fi

sum_of() { cat "$1"/*.xml 2>/dev/null | grep -o "$2=\"[0-9]*\"" | tr -dc '0-9\n' | awk '{s+=$1} END{print s+0}'; }

total=0 fails=0
declare -a stamps=()
printf '%8s %8s %-17s %s\n' 条数 失败 最后写入 目录
for d in "${dirs[@]}"; do
  n=$(sum_of "$d" tests)
  f=$(( $(sum_of "$d" failures) + $(sum_of "$d" errors) ))
  t=$(date -r "$d" '+%m-%d %H:%M:%S' 2>/dev/null || echo '?')
  printf '%8s %8s %-17s %s\n' "$n" "$f" "$t" "${d#./}"
  stamps+=("$t")
  total=$((total + n)); fails=$((fails + f))
done
printf '%8s %8s  ← 合计（门禁条数的口径：各 results 目录去重后求和）\n' "$total" "$fails"
# 时间戳只陈述、不下结论：没改动的模块 gradle 判 UP-TO-DATE 不重跑，时间自然停在上一次。
oldest=$(printf '%s\n' "${stamps[@]}" | sort | head -1)
newest=$(printf '%s\n' "${stamps[@]}" | sort | tail -1)
if [ "$oldest" != "$newest" ]; then
  echo "注意：各目录最后写入时间跨度 $oldest … $newest。两种解释，自己对一下："
  echo "    ①（常见、无害）没改动的模块被 gradle 判 UP-TO-DATE、没重跑，时间戳就停在上一次。"
  echo "    ②（会让这个数变小）上一轮用了 \`--tests '*Foo*'\` 过滤，那个模块只剩被过滤出的几条。"
  echo "    拿不准就跑一次完整门禁（见本脚本头注）再引这个数。"
fi
[ "$fails" -eq 0 ] || { echo "有 $fails 条失败" >&2; exit 1; }
