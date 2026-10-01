#!/usr/bin/env bash
# **令牌真的走完了「载荷 → shell → 进程环境 → 后端 → wire」这一整条**。
#
# ## 它补的是哪一格（`ccm-rbind-title.sh` 同族）
#
# `ccm-rbind-title.sh` 证的是「标题路那个 marker 真的常驻在窗口标题里」—— 用**真 tmux**
# 量那条键有没有活到它要去的地方。本套件对**令牌那条键**做同一件事，用的是**真 bash ＋ 真后端**。
#
# 步 2 的 `backend-rbind-token.sh` 头注逐字登记过它**买不到**的那一格：
#
# > 端到端那一维买不到。……本套件自己用 `env CCM_RBIND_TOKEN=…` 造出那个进程，
# > **不依赖另一路**；它证的是「后端这一侧报得出」。
#
# 本套件接的就是那一格：进程环境里那个令牌，不是本脚本自己 `env` 进去的，
# 而是**生产载荷渲染器吐出来的那串字节**在真 bash 里跑出来的。
#
# ## 那一行从哪儿来（不重抄一份）
#
# 起会话只交一行 `ccm …`：由生产渲染链现产（`resume-cmd-driver.ts direct` → 生产 `plan*` ＋ `buildCliRenderRequest`
# → 生产 Rust `render_ccm_launch`），启动器换成本趟的 sleeper（不起真 claude），令牌是本趟现铸的。
# ⇒ 本脚本在真 bash 里跑的就是生产会交给终端的那一行；令牌由**那台的 ccm** 放进 agent 进程环境。
#
# ## 四组读数（缺一组都不可信）
#
# | 组 | 买到什么 |
# |---|---|
# | [0] 量具自检 | 那一行以 `ccm ` 打头、带着本趟的令牌（`--ccm-rbind-token`），没有任何 `export` |
# | [1] 真 bash 跑那一行 | ccm 在最终 exec 那一处把令牌**真的放进了进程环境**；`--base` 真的清掉了账号目录变量 |
# | [2] 真后端 `--with-rbind-token` | wire 帧上的 `rbind_token` **等于**交给 ccm 的那个值 |
# | [3] 阴性：同一个进程形态、不带令牌 | 帧照样到（量具自检）、但**没有**令牌 —— 否则 [2] 可能是继承自本 shell 的假绿 |
#
# ## 本机安全
#
# 不碰 tmux；`claude_dir` 在 `/tmp` 下的临时目录；假会话进程是 `sleep`（不是真 claude，`C7`）。
#
# ## ⚠ 它**买不到**什么（如实登记）
#
# - 🔴 **「↗ 真的把那个窗口拉到前台」这一维一格都买不到。** 没有图形会话、没有 Windows；
#   本地那张 `token → HWND` 表（`bind.rs::lookup_hwnd_for_token`）只有平台无关那两段有单测。
# - **远端那一跳**：载荷在本机 bash 里跑，不经 ssh。ssh 只是把同一串字节交给远端 shell，
#   它改不了 env 前缀的语义 —— 但那是推论，本套件没量。
set -o pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
# 〔接进执行链〕与 `ccm-*` 那四套同一条取法：`$CARGO_TARGET_DIR` 优先，
#   缺省落仓根 `.build/backend`（`src/backend/.cargo/config.toml` 的 `target-dir`）。
#   此前只认后一半 ⇒ 门禁若在设了 `CARGO_TARGET_DIR` 的沙箱里跑，本套件会找不到它刚 build 的那一份。
D="${CCM_E2E_BACKEND_BIN:-${CARGO_TARGET_DIR:-$REPO/.build/backend}/debug/cc-monitor-backend}"
[ -x "$D" ] || { echo "需要后端二进制：$D（先 (cd src/backend && cargo build)）"; exit 1; }
# 那一行是 `ccm …` ⇒ 后端二进制以 `ccm` 之名上 PATH；家目录 / 账号库等沙箱无条件给（后端也在这份沙箱里起）。
# shellcheck source=tests/e2e/ccm-shim.sh
. "$REPO/tests/e2e/ccm-shim.sh"

W="$(mktemp -d /tmp/e2e-rbind-e2e.XXXXXX)"
cleanup() { rm -rf -- "$W"; ccm_shim_cleanup; }
trap cleanup EXIT

fail=0
pass=0
chk() { if [ "$2" = "$3" ]; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1: 期望[$3] 实得[$2]"; fail=$((fail+1)); fi; }

# 本趟的启动器：一个只会睡的脚本（ccm exec 它，pid 不变 ⇒ 后端从这个进程的环境里读令牌）。
SLEEPER="$W/sleeper"
printf '#!/bin/sh\nexec sleep 60\n' >"$SLEEPER" && chmod +x "$SLEEPER"
TOK="$(python3 -c 'import secrets;print(secrets.token_hex(16))')"

echo "[0] 生产渲染链现产那一行（不重抄）"
line_for() { (cd "$REPO" && npx tsx tests/e2e/resume-cmd-driver.ts direct "s-$1" /tmp "$SLEEPER" - "${2:--}"); }
LINE="$(line_for e2e "$TOK")"
echo "   line: ${LINE//$TOK/<令牌>}"
chk "那一行以 ccm 打头" "${LINE%% *}" "ccm"
chk "那一行带着本趟的令牌（--ccm-rbind-token）" "$(printf '%s' "$LINE" | grep -c -- "--ccm-rbind-token $TOK")" "1"
chk "那一行里没有任何 export（环境由那台的 ccm 自己设）" "$(printf '%s' "$LINE" | grep -c 'export ')" "0"
BARE="$(line_for bare -)"

# $1=要跑的那一行 → 起 bash 跑它（ccm exec 成 sleeper），留 $VPID
spawn() {
  # `env -u`：跑本套件的 shell 碰巧带着这个变量时，阴性组会继承到它、当场变假绿。
  env -u CCM_RBIND_TOKEN CLAUDE_CONFIG_DIR=/tmp/e2e-rbind-should-be-cleared bash -c "$1" & VPID=$!
  sleep 1
}

echo "[1] 真 bash 跑那一行 ⇒ ccm 把令牌放进 agent 进程环境"
spawn "$LINE"
chk "进程环境里 CCM_RBIND_TOKEN == 交给 ccm 的那个值" \
  "$(tr '\0' '\n' < "/proc/$VPID/environ" | sed -n 's/^CCM_RBIND_TOKEN=//p')" "$TOK"
chk "--base 真的清掉了账号目录变量（CLAUDE_CONFIG_DIR 不在）" \
  "$(tr '\0' '\n' < "/proc/$VPID/environ" | grep -c '^CLAUDE_CONFIG_DIR=')" "0"
kill "$VPID" 2>/dev/null; wait "$VPID" 2>/dev/null

# $1=标签  $2=要跑的那一行 → 真起后端（--with-rbind-token）收帧，帧留在 $OUT
probe() {
  local label="$1" F="$W/$1"
  OUT="$W/$label.frames"
  mkdir -p "$F/projects/p" "$F/sessions"
  spawn "$2"
  local ticks; ticks=$(awk '{print $22}' "/proc/$VPID/stat")
  CLAUDE_CONFIG_DIR="$F" timeout 20 "$D" -- --with-rbind-token < /dev/null > "$OUT" 2> "$W/$label.err" &
  local dpid=$!
  sleep 2.5
  printf '{"pid":%d,"sessionId":"sid-%s","cwd":"/tmp","kind":"interactive","procStart":"%s"}\n' \
    "$VPID" "$label" "$ticks" > "$F/sessions/$VPID.json"
  printf '{"type":"user","sessionId":"sid-%s"}\n' "$label" > "$F/projects/p/sid-$label.jsonl"
  sleep 4
  kill "$dpid" "$VPID" 2>/dev/null
  wait "$dpid" 2>/dev/null
}
added_count() { grep -c '"kind":"session_added"' "$1" 2>/dev/null || true; }
token_in_frame() {
  grep '"kind":"session_added"' "$1" 2>/dev/null \
    | sed -n 's/.*"rbind_token":"\([^"]*\)".*/\1/p' | head -1
}

echo "[2] 正题：真后端从那个进程的环境里读回令牌，送上 wire"
probe e2e "$LINE"
chk "量具自检：真的收到了 session_added" "$(added_count "$OUT")" "1"
chk "帧上的 rbind_token == 交给 ccm 的那个值" "$(token_in_frame "$OUT")" "$TOK"
chk "令牌没有漏进后端日志（§8.6 ③）" "$(grep -c "$TOK" "$W/e2e.err" 2>/dev/null || true)" "0"

echo "[3] 阴性：同一形态、不带令牌 ⇒ 帧到了，令牌没有"
probe bare "$BARE"
chk "bare 量具自检：真的收到了 session_added" "$(added_count "$OUT")" "1"
chk "bare 帧上没有 rbind_token 字段" "$(grep -c '"rbind_token"' "$OUT" 2>/dev/null || true)" "0"

echo
echo "rbind-token-endtoend: $pass passed, $fail failed"
# `tests/scripts/gate.sh` 经 `tests/e2e/assert-pass-floor.sh` 按 exact 判本套件，
#   那把尺子只认这一行的格式（`合计 PASS=<n>`）。上面那句人读的留着，这一行给尺子。
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ] || exit 1
