#!/usr/bin/env bash
# 跑弱网台架，并断言它的运行期 PASS 数不低于地板 —— `e2e/assert-pass-floor.sh` 的同族。
#
# ## 为什么另起一份，而不是直接用 `e2e/assert-pass-floor.sh`
#
# 那一份的第一件事是 `npm run --silent "test:$SUITE"` ⇒ **要在 `package.json` 里有一条
# `test:weak-net`**。而 `package.json` **不在件 `W-F1` 的写区里**（写区只有 `e2e/`、
# `.github/workflows/ci.yml`、件文件三项）⇒ 本件加不了那条 npm 脚本。
# ⇒ 这里按同一套 fail-closed 规矩另起一份，**按路径直跑**那个量具。
#
# ⚠ **如实写明这一份换来的代价**（别读成"等价"）：
#   · 它**不在** `ci.yml` 那张「23 套真机套件都必须带断言数地板」的清单里
#     —— 那张清单只数 `run: bash e2e/assert-pass-floor.sh` 这种调用行，本文件不是。
#     本套件的地板因此**只有一处住址**（`ci.yml` 的调用行），没有反向自检替它兜底。
#   · 它也不在 `src-tauri/src/e2e_gate_registry.rs` 的人群里 —— 那张表**从 `package.json`
#     派生**，没有 npm 脚本就不进人群，于是「这套没进门禁」这件事**没有判据会喊**。
#   ⇒ 两处都指向同一个后续动作：**等 `package.json` 进了谁的写区，就把本套件收进
#     `test:weak-net` + `assert-pass-floor.sh`，并把本文件删掉。**在那之前这是一处诚实边界，
#     不是一处等价实现。
#
# ## fail-closed 三条（与 `assert-pass-floor.sh` 逐条同义）
#
# 1. 台架本身非零退出 ⇒ 直接失败（原样透传输出）；
# 2. 抓不到 `合计 PASS=<n>` ⇒ **失败**，不当 0 也不当通过；
# 3. `n < 地板` ⇒ 失败，诊断同时给实得与地板。
#
# 用法：bash e2e/weak-net/assert-floor.sh <地板>
set -uo pipefail

FLOOR="${1:?用法: assert-floor.sh <地板>}"
case "$FLOOR" in ''|*[!0-9]*) echo "地板必须是非负整数，实得：$FLOOR" >&2; exit 2 ;; esac

HERE="$(cd "$(dirname "$0")" && pwd)"
OUT_FILE="$(mktemp)"
trap 'rm -f -- "$OUT_FILE"' EXIT

# 别写成 `… | tee`：管线会把台架的退出码换成 tee 的。落文件再回显。
bash "$HERE/rig.sh" >"$OUT_FILE" 2>&1
rc=$?
cat "$OUT_FILE"

if [ "$rc" -ne 0 ]; then
  echo "::error::弱网台架失败（退出码 $rc）"
  exit "$rc"
fi

n="$(grep -oE '合计 PASS=[0-9]+' "$OUT_FILE" | grep -oE '[0-9]+' | tail -1)"
if [ -z "$n" ]; then
  echo "::error::台架输出里找不到「合计 PASS=<n>」——它被改得不打印了，或没跑到收尾。地板无从校验 ⇒ 判失败。"
  exit 1
fi

if [ "$n" -lt "$FLOOR" ]; then
  echo "::error::弱网台架断言数缩水：实得 $n < 地板 $FLOOR。真删了断言就把地板一起降，并在 commit 里说明理由。"
  exit 1
fi

echo "[weak-net] PASS=$n（地板 $FLOOR）"
