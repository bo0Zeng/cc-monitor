#!/bin/bash
# ccm CLI 的**真机行为验收**（unify-launch F02）。
#
# 与 e2e/ccm-cli.test.sh 的分工：那个断言「命令串长什么样」（--print），
# 本脚本断言「这条命令在真 tmux 上干了什么」。二者缺一不可
# （F01 教训：三门禁全绿仍放行了一个让 send-keys 完全失效的改动）。
#
# 核心要证的一件事：**账号 env 穿得过 tmux 进程边界**。
# 旧 cct 的做法是外层 export + 自建 tmux + send-keys，而 tmux 的 update-environment
# 默认列表不含 CLAUDE_CONFIG_DIR → export 被整个吃掉。本脚本同时跑**对照组**证明这一点。
#
# 隔离 -L socket + tmux shim + 假 launcher（不起真 claude），不碰任何真实会话。
# 跑法：bash e2e/ccm-acceptance.sh   （npm run test:ccm-acceptance）
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
CCM="$REPO/shared/ccm"
SOCK=ccmF02
TMUX_BIN="$(command -v tmux)" || { echo "需要 tmux"; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"; "$TMUX_BIN" -L "$SOCK" kill-server 2>/dev/null' EXIT

# tmux shim：把 CLI 内部裸调的 `tmux` 全部导到隔离 socket。
BIN="$TMP/bin"; mkdir -p "$BIN"
printf '#!/bin/sh\nexec %s -L %s "$@"\n' "$TMUX_BIN" "$SOCK" > "$BIN/tmux"; chmod +x "$BIN/tmux"
# 假 launcher：不起真 agent（真 claude 会清屏 → 探针假 PASS，F01 踩过），
# 只把「我看到的环境」落盘，供断言。
printf '#!/bin/sh\nprintf "CFG=%%s\\nPWD=%%s\\nNESTED=%%s\\n" "${CLAUDE_CONFIG_DIR:-<unset>}" "$PWD" "${CLAUDECODE:-<unset>}" >> %s/probe.log\nsleep 5\n' \
  "$TMP" > "$BIN/CCMPROBE"; chmod +x "$BIN/CCMPROBE"
export PATH="$BIN:$PATH"
# **测试自身必须干净**：本脚本可能跑在一个已经带 CLAUDE_CONFIG_DIR 的进程里
# （例如 cc-monitor 的开发者本人正用某个隔离账号跑 Claude Code）。不清掉的话，
# tmux server 会从这里继承，对照组就测不出东西（实测踩过：拿到的是开发者自己的账号目录）。
unset CLAUDE_CONFIG_DIR
# **同时必须脱离 $TMUX**：CLI 在 tmux 内会退化成"就地起"（不建嵌套会话，见 shared/ccm），
# 那时本脚本要验的容器行为根本不发生。生产路径是 `ssh -t … bash -lic`，$TMUX 本就不存在。
unset TMUX TMUX_PANE

# 隔离的账号库 + 配置（不碰用户真实的 ~/.claude-accts）
ACCTS="$TMP/accts"; mkdir -p "$ACCTS/z" "$ACCTS/b"
cat > "$ACCTS/accounts.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$ACCTS/z", "isDefault": true, "mode": "isolated" },
  { "name": "b", "configDir": "$ACCTS/b", "isDefault": false, "mode": "isolated" } ] }
JSON
CFG="$TMP/ccm-config"
printf 'CCM_ACCTS_MANIFEST=%s\nCCM_WORKSPACE=%s\n' "$ACCTS/accounts.json" "$TMP/ws" > "$CFG"
mkdir -p "$TMP/ws" "$TMP/proj"
export CCM_CONFIG="$CFG" CCM_SELF="$CCM"

T() { "$TMUX_BIN" -L "$SOCK" "$@"; }
reset() { T kill-server 2>/dev/null; : > "$TMP/probe.log"; sleep 0.3; }
opt() { T show-options -v -t "=$1:" "$2" 2>/dev/null; }

PASS=0; FAIL=0
ck() { if [ "$2" = "$3" ]; then printf 'PASS | %-52s | %s\n' "$1" "$3"; PASS=$((PASS+1))
       else printf 'FAIL | %-52s | 期望=%s 实得=%s\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }

echo "===== 场景 1：ccm --tmux --account z —— 账号必须穿过 tmux 边界 ====="
reset
( cd "$TMP/proj" && bash "$CCM" --tmux --account z --launcher CCMPROBE >/dev/null 2>&1 & )
sleep 3
ck "会话 cc-proj 被建出" "yes" "$(T has-session -t '=cc-proj:' 2>/dev/null && echo yes || echo no)"
ck "@ccm_agent 打上" "claude" "$(opt cc-proj @ccm_agent)"
ck "**CLAUDE_CONFIG_DIR 穿过了 tmux 边界**" "$ACCTS/z" \
   "$(grep -m1 '^CFG=' "$TMP/probe.log" 2>/dev/null | cut -d= -f2-)"
ck "cwd 正确落到 --cwd 解析值" "$TMP/proj" \
   "$(grep -m1 '^PWD=' "$TMP/probe.log" 2>/dev/null | cut -d= -f2-)"

echo
echo "===== 场景 2（对照组）：旧 cct 的做法 —— 证明它会丢账号 ====="
# **关键前提**：tmux server 必须先由一个**不带该 env 的 shell** 拉起。
# 若 server 恰好由带 env 的 shell 首次拉起，它会继承环境 → 对照组假阴性。
# 这也解释了为什么旧 cct 的账号丢失是**非确定性**的（用户原话「经常出现无法换号」而非「总是」）：
# 取决于 tmux server 是谁起的。update-environment 默认列表不含 CLAUDE_CONFIG_DIR，
# 故 server 一旦在跑，后续 new-session 一律拿不到。
reset
env -u CLAUDE_CONFIG_DIR "$TMUX_BIN" -L "$SOCK" new-session -d -s bootstrap   # 由**确定无该 env** 的 shell 拉起 server
sleep 0.5
(
  cd "$TMP/proj"
  export CLAUDE_CONFIG_DIR="$ACCTS/z"      # 外层 export（旧 cct 就是这样）
  T new-session -d -s old_cc -c "$TMP/proj"
  T send-keys -t '=old_cc:' 'CCMPROBE' Enter
) >/dev/null 2>&1
sleep 3
ck "对照组：server 已在跑时，外层 export 被 tmux 边界吃掉" "<unset>" \
   "$(grep -m1 '^CFG=' "$TMP/probe.log" 2>/dev/null | cut -d= -f2-)"
ck "update-environment 默认列表确实不含 CLAUDE_CONFIG_DIR" "" \
   "$(T show-options -g update-environment 2>/dev/null | grep -c CLAUDE_CONFIG_DIR | sed 's/^0$//')"

echo
echo "===== 场景 3：身份 @ccm_sid ====="
reset
( cd "$TMP/proj" && bash "$CCM" --tmux --ccm-sid deadbeef-1234 --launcher CCMPROBE >/dev/null 2>&1 & )
sleep 3
ck "通道A：已知 sid 建时即打标" "deadbeef-1234" "$(opt cc-proj @ccm_sid)"

echo
echo "===== 场景 4：agent 轴 ====="
# 外层先带毒（模拟 issue #24 的 tmux server env 污染），验两个 agent 的清理差异。
reset
( cd "$TMP/proj" && CLAUDECODE=1 bash "$CCM" --tmux --agent codex --launcher CCMPROBE >/dev/null 2>&1 & )
sleep 3
ck "@ccm_agent = codex" "codex" "$(opt cc-proj @ccm_agent)"
ck "codex 无嵌套 env 概念 → CLAUDECODE 保留" "1" \
   "$(grep -m1 '^NESTED=' "$TMP/probe.log" 2>/dev/null | cut -d= -f2-)"
reset
( cd "$TMP/proj" && CLAUDECODE=1 bash "$CCM" --tmux --launcher CCMPROBE >/dev/null 2>&1 & )
sleep 3
ck "claude 起前清嵌套 env（治 issue #24 带毒）" "<unset>" \
   "$(grep -m1 '^NESTED=' "$TMP/probe.log" 2>/dev/null | cut -d= -f2-)"

echo
echo "===== 场景 5：幂等 —— 同目录连开两次接回同一会话，不产孤儿 ====="
reset
( cd "$TMP/proj" && bash "$CCM" --tmux --launcher CCMPROBE >/dev/null 2>&1 & )
sleep 2.5
( cd "$TMP/proj" && bash "$CCM" --tmux --launcher CCMPROBE >/dev/null 2>&1 & )
sleep 2.5
ck "只有一个 cc-proj，无 cc-proj-2 孤儿" "cc-proj" "$(T ls -F '#{session_name}' 2>/dev/null | sort | tr '\n' ' ' | sed 's/ $//')"

echo
echo "===== 场景 6：会话名过 cc-monitor 的 is_ccm_tmux_name（F04 之前也能被控制面接受）====="
name="$(T ls -F '#{session_name}' 2>/dev/null | head -1)"
case "$name" in cc-*) r=yes ;; *) r=no ;; esac
ck "会话名以 cc- 开头" "yes" "$r"

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
