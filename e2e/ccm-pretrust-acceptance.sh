#!/bin/bash
# ccm 预信任能力的**真机行为验收**（unify-launch F11：预信任逻辑从 cc-bus 的 cc-spawn 原样
# 搬进 ccm 的 --tmux 建会话路径，免 claude/codex 起来卡在信任确认框）。
#
# 与 e2e/ccm-acceptance.sh 同一套隔离手法（-L socket + tmux shim + 假 launcher），
# 本脚本只聚焦预信任写入本身：~/.claude.json 的 hasTrustDialogAccepted / ~/.codex/config.toml
# 的 trust_level stanza，用 CCM_CLAUDEJSON/CCM_CODEXTOML 测试钩子指向隔离副本，不碰真实文件。
#
# 跑法：bash e2e/ccm-pretrust-acceptance.sh   （npm run test:ccm-pretrust）
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
CCM="$REPO/shared/ccm"
SOCK=ccmF11
TMUX_BIN="$(command -v tmux)" || { echo "需要 tmux"; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"; "$TMUX_BIN" -L "$SOCK" kill-server 2>/dev/null' EXIT

BIN="$TMP/bin"; mkdir -p "$BIN"
printf '#!/bin/sh\nexec %s -L %s "$@"\n' "$TMUX_BIN" "$SOCK" > "$BIN/tmux"; chmod +x "$BIN/tmux"
printf '#!/bin/sh\nsleep 5\n' > "$BIN/PROBE"; chmod +x "$BIN/PROBE"
# 假 launcher：模拟 claude 真的弹出信任确认框、等一行输入——用来验证轮询兜底真的会检测到
# 这段文本并自动发 Enter（不只是语法层面看起来对）。
printf '#!/bin/sh\necho "Yes, I trust this folder"\nread -r _line\necho GOT_ENTER > %s/poll-result\n' "$TMP" > "$BIN/TRUSTPROMPT"
chmod +x "$BIN/TRUSTPROMPT"
export PATH="$BIN:$PATH"
unset CLAUDE_CONFIG_DIR
unset TMUX TMUX_PANE

CFG="$TMP/ccm-config"
mkdir -p "$TMP/ws"
printf 'CCM_WORKSPACE=%s\n' "$TMP/ws" > "$CFG"
export CCM_CONFIG="$CFG" CCM_SELF="$CCM"

T() { "$TMUX_BIN" -L "$SOCK" "$@"; }
reset() { T kill-server 2>/dev/null; sleep 0.3; }

PASS=0; FAIL=0
ck() { if [ "$2" = "$3" ]; then printf 'PASS | %-52s | %s\n' "$1" "$3"; PASS=$((PASS+1))
       else printf 'FAIL | %-52s | 期望=%s 实得=%s\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }

mkdir -p "$TMP/proj1" "$TMP/proj2"

echo "===== 场景 1：claude 未信任的目录 —— 建会话后 hasTrustDialogAccepted 变 true ====="
reset
echo '{"projects":{}}' > "$TMP/claude.json"
( cd "$TMP/proj1" && CCM_CLAUDEJSON="$TMP/claude.json" bash "$CCM" --tmux --agent claude --launcher PROBE >/dev/null 2>&1 & )
sleep 2
ck "该目录 hasTrustDialogAccepted=true" "true" \
  "$(jq -r --arg d "$TMP/proj1" '.projects[$d].hasTrustDialogAccepted // "missing"' "$TMP/claude.json" 2>/dev/null)"
ck "claude.json 仍是合法 JSON" "ok" "$(jq -e . "$TMP/claude.json" >/dev/null 2>&1 && echo ok || echo bad)"

echo
echo "===== 场景 1b：--cwd <相对路径> —— 必须用规范化绝对路径当 key，不能写出字面量 '.'/相对串 ====="
# Phase D 审计发现的阻塞项：resolve_cwd() 对非 auto 值原样透传，不保证绝对路径；
# 早期实现直接拿 $cwd 当 key，导致这个场景写出的是字面量 "proj1"（对真实信任表毫无意义、
# 还污染真实配置文件）。已修：预信任专用 $cwd_abs="$(cd "$cwd" && pwd)"。
reset
echo '{"projects":{}}' > "$TMP/claude.json"
( cd "$TMP" && CCM_CLAUDEJSON="$TMP/claude.json" bash "$CCM" --tmux=cc-relcwd --agent claude --cwd proj1 --launcher PROBE >/dev/null 2>&1 & )
sleep 2
ck "写入的 key 是规范化绝对路径（非字面量相对串）" "true" \
  "$(jq -r --arg d "$TMP/proj1" '.projects[$d].hasTrustDialogAccepted // "missing"' "$TMP/claude.json" 2>/dev/null)"
ck "没有写出字面量 'proj1' 这个错误 key" "missing" \
  "$(jq -r '.projects["proj1"].hasTrustDialogAccepted // "missing"' "$TMP/claude.json" 2>/dev/null)"

echo
echo "===== 场景 2：claude 已信任的目录 —— 幂等，不重复改写 ====="
reset
echo '{"projects":{"'"$TMP"'/proj1":{"hasTrustDialogAccepted":true,"otherField":"keep-me"}}}' > "$TMP/claude.json"
( cd "$TMP/proj1" && CCM_CLAUDEJSON="$TMP/claude.json" bash "$CCM" --tmux=cc-proj1b --agent claude --launcher PROBE >/dev/null 2>&1 & )
sleep 2
ck "已信任目录的其它字段不受影响（未被整段覆盖）" "keep-me" \
  "$(jq -r --arg d "$TMP/proj1" '.projects[$d].otherField // "missing"' "$TMP/claude.json" 2>/dev/null)"

echo
echo "===== 场景 3：codex 未信任的目录 —— config.toml 新增 trust_level stanza ====="
reset
: > "$TMP/config.toml"
( cd "$TMP/proj2" && CCM_CODEXTOML="$TMP/config.toml" bash "$CCM" --tmux --agent codex --launcher PROBE >/dev/null 2>&1 & )
sleep 2
ck "config.toml 含该目录的 trust_level stanza" "yes" \
  "$(grep -qF "[projects.\"$TMP/proj2\"]" "$TMP/config.toml" 2>/dev/null && grep -qF 'trust_level = "trusted"' "$TMP/config.toml" && echo yes || echo no)"

echo
echo "===== 场景 4：codex 已有 stanza 的目录 —— 幂等，不重复追加 ====="
reset
printf '[projects."%s"]\ntrust_level = "trusted"\n' "$TMP/proj2" > "$TMP/config.toml"
before="$(wc -l < "$TMP/config.toml")"
( cd "$TMP/proj2" && CCM_CODEXTOML="$TMP/config.toml" bash "$CCM" --tmux=cc-proj2b --agent codex --launcher PROBE >/dev/null 2>&1 & )
sleep 2
after="$(wc -l < "$TMP/config.toml")"
ck "重复目录不追加第二份 stanza（行数不变）" "$before" "$after"

echo
echo "===== 场景 5：codex 目录路径含双引号/反斜杠 —— 正确转义成合法 TOML ====="
reset
weird="$TMP/proj-with-\"quote"
mkdir -p "$weird"
: > "$TMP/config.toml"
( cd "$weird" && CCM_CODEXTOML="$TMP/config.toml" bash "$CCM" --tmux --agent codex --launcher PROBE >/dev/null 2>&1 & )
sleep 2
# 转义规则：先 \ 再 "（同脚本内 cesc=${cwd//\\/\\\\}; cesc=${cesc//\"/\\\"}）
esc_quote='\"'
ck "含引号的路径被正确转义写入" "yes" \
  "$(grep -qF "[projects.\"$TMP/proj-with-${esc_quote}quote\"]" "$TMP/config.toml" 2>/dev/null && echo yes || echo no)"

echo
echo "===== 场景 6：目标文件 chmod 400（常见误判，非真失败）—— 不该意外阻断建会话 ====="
# 标题订正（Phase D 后端审计独立复现并指出）：chmod 400 只移除文件自身权限位，rename(2)/mv
# 只看**目录**写权限，并不检查被覆盖目标的权限——这个场景**不是**真失败路径（写入其实会成功），
# 只是验证"这种常见但其实无效的误判操作不会意外把建会话搞挂"。真失败场景见场景 7（目录不可写）。
reset
: > "$TMP/claude-readonly.json"
echo '{"projects":{}}' > "$TMP/claude-readonly.json"
chmod 400 "$TMP/claude-readonly.json"
( cd "$TMP/proj1" && CCM_CLAUDEJSON="$TMP/claude-readonly.json" bash "$CCM" --tmux=cc-ro --agent claude --launcher PROBE >/dev/null 2>&1 & )
sleep 2
ck "只读预信任文件不阻断会话创建" "yes" "$(T has-session -t '=cc-ro:' 2>/dev/null && echo yes || echo no)"
chmod 600 "$TMP/claude-readonly.json"

echo
echo "===== 场景 7：预信任写入真失败（目录不可写，chmod 400 单文件并不会真的挡住 mv/rename）====="
# Phase D 审计发现：场景 6 的 chmod 400 并不能真正阻止写入——rename(2) 只看**目录**写权限，
# 不看目标文件自身权限位（已实测复现）。真正的失败需要让 jq 写临时文件那一步本身失败——
# 把目标文件放进一个不可写目录，`cp -p`/临时文件创建/mv 全部失败，才是真失败路径。
reset
mkdir -p "$TMP/ro-dir"
echo '{"projects":{}}' > "$TMP/ro-dir/claude.json"
chmod 500 "$TMP/ro-dir"   # 目录本身不可写：无法在其中新建 .tmp/.bak 文件
( cd "$TMP/proj1" && CCM_CLAUDEJSON="$TMP/ro-dir/claude.json" bash "$CCM" --tmux=cc-ro2 --agent claude --launcher PROBE 2>"$TMP/ro-stderr.log" & )
sleep 2
ck "目录不可写时 stderr 报预信任写入失败" "yes" \
  "$(grep -qF '预信任写入失败' "$TMP/ro-stderr.log" 2>/dev/null && echo yes || echo no)"
ck "写入真失败仍不阻断会话创建" "yes" "$(T has-session -t '=cc-ro2:' 2>/dev/null && echo yes || echo no)"
chmod 700 "$TMP/ro-dir"

echo
echo "===== 场景 8：CCM_NO_PRETRUST=1 —— 完全关闭预信任写入 ====="
reset
echo '{"projects":{}}' > "$TMP/claude.json"
( cd "$TMP/proj1" && CCM_NO_PRETRUST=1 CCM_CLAUDEJSON="$TMP/claude.json" bash "$CCM" --tmux=cc-noopt --agent claude --launcher PROBE >/dev/null 2>&1 & )
sleep 2
ck "opt-out 时该目录未被标记为已信任" "missing" \
  "$(jq -r --arg d "$TMP/proj1" '.projects[$d].hasTrustDialogAccepted // "missing"' "$TMP/claude.json" 2>/dev/null)"

echo
echo "===== 场景 9：预信任没生效（无 claude.json）—— 轮询兜底真的检测到信任框并自动按 Enter ====="
# Phase D 审计（阻塞项修复）核心断言：不只是"语法上加了轮询代码"，是真的能从 pane 里
# 抓到信任框文本、真的会发 Enter、真的能让等在 read 那一行的进程收到并继续。
reset
rm -f "$TMP/poll-result"
( cd "$TMP/proj1" && CCM_CLAUDEJSON="$TMP/nonexistent.json" bash "$CCM" --tmux=cc-polltest --agent claude --launcher TRUSTPROMPT >/dev/null 2>&1 & )
sleep 4
ck "轮询兜底检测到信任框文本并自动按 Enter" "GOT_ENTER" "$(cat "$TMP/poll-result" 2>/dev/null || echo missing)"

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
