#!/usr/bin/env bash
# K-G8 尺子的第二半：**在沙箱里逐套真跑**，把每套的输出落成 `<套名>.log`，
# 再喂给 `K-G8-floor-slack-census.py slack --runs <目录>` 算余量。
#
# ## 为什么要真跑（而不是静态数）
#
# `K-OBSERVE-E2E-audit.md`（08-28 @ `c7a57c9`）那张逐套表是**静态展开**数出来的，
# 它自己列了四条量不到的东西，头一条逐字：「**量不了「跑得起来吗」**」——
# 而本件问的「余量」用的是 `assert-pass-floor.sh` 眼里的那个数，
# 那个数是**运行期**的 `合计 PASS=<n>`。⇒ 两把尺子不同，别互相顶替。
#
# ## 红线
#
# 🔴 只在沙箱里跑（`K31`：所有开发测试不许直接在本机跑）。本脚本**拒绝在容器外执行**：
#    判据是 `/.dockerenv` 存在。想在宿主上「就跑一下」的那条路**故意不留**。
# 🔴 串行跑，不并发 —— `e2e/README.md` 逐字：fixture 目录/cwd 固定名，并发会互删。
#
# ## 用法（在沙箱里）
#
#   bash evidence/K-G8-run-suites.sh <落日志的目录> [套名 …]
#
# 不给套名 ⇒ 跑 `ci.yml` 调用行里的**全部** 23 套（顺序照 `ci.yml`）。
# 每套单独 `timeout`（默认 300 秒，`K_G8_TIMEOUT` 可调）；超时也落日志、记退出码。
#
# 典型的整趟（宿主上敲这一条）：
#   docker run --rm --network none -v /home/zbl/文档/claudecode-frontend:/home/zbl/文档/claudecode-frontend \
#     -e HOME=/home/zbl -e CARGO_TARGET_DIR=/home/zbl/文档/claudecode-frontend/.claude/pm-targets/k-g8 \
#     -w <工作树> ccmon-devbox:latest \
#     bash evidence/K-G8-run-suites.sh <日志目录>

set -uo pipefail

if [ ! -e /.dockerenv ]; then
  echo "❌ 本脚本只许在沙箱里跑（K31）。没看见 /.dockerenv ⇒ 拒绝执行。" >&2
  exit 3
fi

OUT_DIR="${1:?用法: K-G8-run-suites.sh <落日志的目录> [套名 …]}"
shift || true
mkdir -p "$OUT_DIR"

TIMEOUT="${K_G8_TIMEOUT:-300}"

if [ "$#" -gt 0 ]; then
  SUITES=("$@")
else
  # 人群 = `ci.yml` 的调用行（**不是**覆盖面清单，也不是 package.json 的 `test:*`）。
  mapfile -t SUITES < <(
    sed -nE 's|^[[:space:]]*run:[[:space:]]*bash[[:space:]]+e2e/assert-pass-floor\.sh[[:space:]]+([^[:space:]]+)[[:space:]]+[0-9]+[[:space:]]*$|\1|p' \
      .github/workflows/ci.yml
  )
fi

echo "# 套数 ${#SUITES[@]}   每套超时 ${TIMEOUT}s   日志落在 $OUT_DIR"
echo "# 量于提交：$(git rev-parse --short HEAD 2>/dev/null || echo '<非 git>')"
echo

for s in "${SUITES[@]}"; do
  log="$OUT_DIR/$s.log"
  start=$(date +%s)
  timeout -k 10 "$TIMEOUT" npm run --silent "test:$s" >"$log" 2>&1
  rc=$?
  dur=$(( $(date +%s) - start ))
  n=$(grep -oE '合计 PASS=[0-9]+' "$log" | grep -oE '[0-9]+' | tail -1)
  printf '%-26s rc=%-4s %4ss  实得=%s\n' "$s" "$rc" "$dur" "${n:-<抓不到>}"
  echo "$rc" >"$OUT_DIR/$s.rc"
done
