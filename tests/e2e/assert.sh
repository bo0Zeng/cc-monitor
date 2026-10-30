# shellcheck shell=bash
# e2e 各套共用的计数与收尾行。各套 source 它；只定义函数和计数，source 它不跑任何东西。
#
#   ok   <描述>                 过一条
#   bad  <描述> [细节]          败一条（细节另起一行印）；设了 `E2E_ON_BAD=<函数>` ⇒ 记败之前先调它（台架不成立就当场停的那类）
#   ck   <描述> <期望> <实得>   两串相等 ⇒ ok，否则 bad 并印出期望与实得
#   skip <描述> [条数]          跳过（缺省 1 条）
#   finish                      印唯一那一行收尾 `===== 合计 PASS=n FAIL=m SKIP=k =====`；有败 ⇒ 退 1，否则退 0
#
# `assert-pass-floor.sh` 与门禁 `tests/scripts/gate.sh` 的 `run_e2e` 抓的就是 `finish` 印的那一行。
# 计数只经这几个函数改；套件要读当前数就读 `E2E_PASS` / `E2E_FAIL` / `E2E_SKIP`。

E2E_PASS=0
E2E_FAIL=0
E2E_SKIP=0

ok() {
  E2E_PASS=$((E2E_PASS + 1))
  printf '  PASS %s\n' "$1"
}

bad() {
  [ -z "${E2E_ON_BAD:-}" ] || "$E2E_ON_BAD"
  E2E_FAIL=$((E2E_FAIL + 1))
  printf '  FAIL %s\n' "$1"
  [ -z "${2:-}" ] || printf '       %s\n' "$2"
  return 0
}

ck() {
  if [ "$2" = "$3" ]; then
    ok "$1"
  else
    bad "$1" "期望 [$2] · 实得 [$3]"
  fi
}

skip() {
  E2E_SKIP=$((E2E_SKIP + ${2:-1}))
  printf '  SKIP %s\n' "$1"
}

finish() {
  printf '===== 合计 PASS=%d FAIL=%d SKIP=%d =====\n' "$E2E_PASS" "$E2E_FAIL" "$E2E_SKIP"
  [ "$E2E_FAIL" -eq 0 ] || exit 1
  exit 0
}
