#!/usr/bin/env bash
# `设计/80 §8.7` 步 3：**令牌真的走完了「载荷 → shell → 进程环境 → 后端 → wire」这一整条**。
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
# ## 字节从哪儿来（不重抄一份）
#
# 取自入库的逐字节金标准 `src/bridge/src/backend/control/fixtures/payload-golden.json`
# 里「五种 EnvOp 同时出现」那一条 —— 那份金标准由 `launch_payload_parity.rs` 钉着
# **与生产 Rust 渲染器 `payload::render_payload` 逐字节相等**。
# ⇒ 本脚本跑的就是生产会送去远端的那一串（只截到 env 前缀为止，见下）。
#
# ## 四组读数（缺一组都不可信）
#
# | 组 | 买到什么 |
# |---|---|
# | [0] 量具自检 | 抽到的是「五种 EnvOp 全在」那一条（令牌之前真有 `unset`）—— 最复杂的那一串 |
# | [1] 真 bash 跑载荷前缀 | 生产字节在**真 shell** 里执行之后，令牌**真的落进了进程环境** |
#
# ⚠ **[1] 关于「顺序」只买到一半，如实写**：今天没有任何 `unset` 变体碰得到
#   `CCM_RBIND_TOKEN`（`RBIND_TOKEN_DIMENSION` 头注 ① 逐字），所以**今天**把令牌那句
#   挪到 unset 前面，[1] 照样绿 —— 它量不出一个今天不存在的冲突。
#   它买到的是另一半：**哪天金标准里出现一条排在令牌之后、又清掉它的 `unset`，[1] 当场红**
#   （死值验刀 E5 现打过：往前缀尾巴上补一句 `unset CCM_RBIND_TOKEN; ` ⇒ [1][2] 红）。
# | [2] 真后端 `--with-rbind-token` | wire 帧上的 `rbind_token` **等于**金标准里渲出的那个值 |
# | [3] 阴性：同一个进程形态、不跑载荷前缀 | 帧照样到（量具自检）、但**没有**令牌 —— 否则 [2] 可能是继承自本 shell 的假绿 |
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
# - **截断**：只跑到 env 前缀（`cd '/w' && claude` 那一截换成 `exec sleep`）—— 目录与
#   launcher 不在本题射程里，它们有自己的金标准。
set -o pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
# 〔第二波 T4 接进执行链〕与 `ccm-*` 那四套同一条取法：`$CARGO_TARGET_DIR` 优先，
#   缺省落仓根 `.build/backend`（`src/backend/.cargo/config.toml` 的 `target-dir`）。
#   此前只认后一半 ⇒ 门禁若在设了 `CARGO_TARGET_DIR` 的沙箱里跑，本套件会找不到它刚 build 的那一份。
D="${CCM_E2E_BACKEND_BIN:-${CARGO_TARGET_DIR:-$REPO/.build/backend}/debug/cc-monitor-backend}"
[ -x "$D" ] || { echo "需要后端二进制：$D（先 (cd src/backend && cargo build)）"; exit 1; }
GOLD="$REPO/src/bridge/src/backend/control/fixtures/payload-golden.json"
[ -f "$GOLD" ] || { echo "找不到载荷金标准：$GOLD"; exit 1; }

W="$(mktemp -d /tmp/e2e-rbind-e2e.XXXXXX)"
cleanup() { rm -rf -- "$W"; }
trap cleanup EXIT

fail=0
pass=0
chk() { if [ "$2" = "$3" ]; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1: 期望[$3] 实得[$2]"; fail=$((fail+1)); fi; }

echo "[0] 从金标准里取那一串生产载荷（不重抄）"
PAYLOAD="$(python3 - "$GOLD" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
hits = [c["payload"] for c in d["cases"]
        if "unset " in c.get("payload", "") and "export CCM_RBIND_TOKEN=" in c.get("payload", "")]
print(hits[0] if len(hits) == 1 else "")
PY
)"
[ -n "$PAYLOAD" ] || { echo "  FAIL 金标准里「既有 unset 又有令牌」的那一条不是恰好 1 条 —— 量具瞄偏了"; exit 1; }
# 截到令牌那一句为止（含）：`export CCM_RBIND_TOKEN='<32hex>'; `
PREFIX="$(printf '%s' "$PAYLOAD" | sed -n "s/^\(.*export CCM_RBIND_TOKEN='[0-9a-f]*'; \).*/\1/p")"
TOK="$(printf '%s' "$PREFIX" | sed -n "s/.*export CCM_RBIND_TOKEN='\([0-9a-f]*\)'; $/\1/p")"
chk "取到的令牌形状是 32 个小写十六进制字符" "$(printf '%s' "$TOK" | grep -cE '^[0-9a-f]{32}$')" "1"
# 🔴 取不到就**当场停**：下面好几处是 `grep -c "$TOK"`，空串会匹配每一行 ——
#   「令牌没漏进日志」那一格会被读成一个与病因无关的数（死值验 09-24 现打出来的）。
printf '%s' "$TOK" | grep -qE '^[0-9a-f]{32}$' || { echo "  量具瞄偏了：没取到令牌，停"; exit 1; }
# ★ 量具自检：抽到的是「令牌之前真有 unset」那一条（金标准里最复杂的一串），不是只有令牌的那条。
chk "抽到的是令牌之前真有 unset 的那一条（最复杂的那一串）" \
  "$(printf '%s' "${PREFIX%%export CCM_RBIND_TOKEN=*}" | grep -c 'unset ')" "1"

# $1=标签  $2=要跑的载荷前缀（空串 = 不跑）  → 起 bash 跑它、exec 成 sleep，留 $VPID
spawn() {
  # `env -u`：跑本套件的 shell 碰巧带着这个变量时（步 3 落地后开发机上完全可能），
  #  阴性组会继承到它、当场变假绿。
  env -u CCM_RBIND_TOKEN bash -c "$2 exec sleep 60" & VPID=$!
  sleep 0.5
}

echo "[1] 真 bash 跑生产载荷前缀 ⇒ 令牌落进进程环境"
spawn live "$PREFIX"
chk "进程环境里 CCM_RBIND_TOKEN == 金标准渲出的那个值" \
  "$(tr '\0' '\n' < "/proc/$VPID/environ" | sed -n 's/^CCM_RBIND_TOKEN=//p')" "$TOK"
chk "那一排 unset 真的生效了（CLAUDE_CONFIG_DIR 不在）" \
  "$(tr '\0' '\n' < "/proc/$VPID/environ" | grep -c '^CLAUDE_CONFIG_DIR=')" "0"
kill "$VPID" 2>/dev/null; wait "$VPID" 2>/dev/null

# $1=标签  $2=载荷前缀 → 真起后端（--with-rbind-token）收帧，帧留在 $OUT
probe() {
  local label="$1" F="$W/$1"
  OUT="$W/$label.frames"
  mkdir -p "$F/projects/p" "$F/sessions"
  spawn "$label" "$2"
  local ticks; ticks=$(awk '{print $22}' "/proc/$VPID/stat")
  CLAUDE_CONFIG_DIR="$F" timeout 20 "$D" --with-rbind-token < /dev/null > "$OUT" 2> "$W/$label.err" &
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
probe e2e "$PREFIX"
chk "量具自检：真的收到了 session_added" "$(added_count "$OUT")" "1"
chk "帧上的 rbind_token == 金标准里渲出的那个值" "$(token_in_frame "$OUT")" "$TOK"
chk "令牌没有漏进后端日志（§8.6 ③）" "$(grep -c "$TOK" "$W/e2e.err" 2>/dev/null || true)" "0"

echo "[3] 阴性：同一形态、不跑载荷前缀 ⇒ 帧到了，令牌没有"
probe bare ""
chk "bare 量具自检：真的收到了 session_added" "$(added_count "$OUT")" "1"
chk "bare 帧上没有 rbind_token 字段" "$(grep -c '"rbind_token"' "$OUT" 2>/dev/null || true)" "0"

echo
echo "rbind-token-endtoend: $pass passed, $fail failed"
# 〔第二波 T4〕`tests/scripts/gate.sh` 经 `tests/e2e/assert-pass-floor.sh` 按 exact 判本套件，
#   那把尺子只认这一行的格式（`合计 PASS=<n>`）。上面那句人读的留着，这一行给尺子。
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ] || exit 1
