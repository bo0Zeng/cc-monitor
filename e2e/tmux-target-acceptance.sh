#!/bin/bash
# tmux 目标精确匹配（INVARIANTS §31a）的**真机行为验收**。
#
# 与黄金串的分工：黄金串断言「命令串长什么样」，本脚本断言「这条命令在真 tmux 上干了什么」。
# 二者缺一不可——上一轮修复三门禁全绿却让 send-keys 完全失效，正是因为只有前者。
#
# 输入 = 真 builder 产出的生产命令串（e2e/tmux-target-emit.mts）。
# 隔离 -L socket + tmux shim，不碰用户任何真实会话。
# 跑法：bash e2e/tmux-target-acceptance.sh   （需要 tmux；npm run test:tmux-target）
set -o pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
SP="$(mktemp -d)"
trap 'rm -rf "$SP"' EXIT
# 生产命令串来自**真 builder**（不手搓等价命令）——见 tmux-target-emit.mts 头注
(cd "$REPO" && npx tsx e2e/tmux-target-emit.mts) > "$SP/f01-cmds.tsv"
SOCK=ccmF01
# 缺 tmux 必须硬失败（Phase G 审阅）：原先这行**完全没有守卫**，`TMUX_BIN` 会是空串，
# shim 变成 `exec  -L ccmF01 "$@"`，套件在一堆看不懂的报错里跑，而不是明确说"需要 tmux"。
TMUX_BIN="$(command -v tmux)" || { echo "需要 tmux"; exit 1; }
SHIM="$SP/shim"; rm -rf "$SHIM"; mkdir -p "$SHIM"
printf '#!/bin/sh\nexec %s -L %s "$@"\n' "$TMUX_BIN" "$SOCK" > "$SHIM/tmux"
chmod +x "$SHIM/tmux"
export PATH="$SHIM:$PATH"

CMD() { grep -P "^$1\t" "$SP/f01-cmds.tsv" | cut -f2-; }
T() { "$TMUX_BIN" -L "$SOCK" "$@"; }
reset() { T kill-server 2>/dev/null; sleep 0.3; }
sessions() { T ls -F '#{session_name}' 2>/dev/null | sort | tr '\n' ' '; }
pane() { T capture-pane -p -t "=$1:" 2>/dev/null; }

PASS=0; FAIL=0
ck() { # ck <描述> <期望> <实得>
  if [ "$2" = "$3" ]; then printf 'PASS | %-56s | %s\n' "$1" "$3"; PASS=$((PASS+1));
  else printf 'FAIL | %-56s | 期望=%s 实得=%s\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi
}

echo "===== 场景 A：只有兄弟会话 cc-p1-2（cc-p1 不存在）====="
reset; T new-session -d -s cc-p1-2; sleep 0.5
bash -c "$(CMD resumeIntoExisting)" >/dev/null 2>&1; rc=$?
ck "生产串 resumeIntoExisting → rc≠0" "nonzero" "$([ $rc -ne 0 ] && echo nonzero || echo zero)"
ck "兄弟 cc-p1-2 未收到载荷" "clean" "$(pane cc-p1-2 | grep -q CCMPROBE && echo POLLUTED || echo clean)"
bash -c "$(CMD attach)" >/dev/null 2>&1; rc=$?
ck "生产串 attach → rc≠0（注：无 tty 下 attach 必失败，本行只作冒烟；正例见场景 E）" "nonzero" "$([ $rc -ne 0 ] && echo nonzero || echo zero)"
T send-keys -t '=cc-p1:' '/exit' Enter >/dev/null 2>&1; rc=$?
ck "Rust 形态 send-keys '=cc-p1:' → rc≠0" "nonzero" "$([ $rc -ne 0 ] && echo nonzero || echo zero)"
T capture-pane -p -t '=cc-p1:' >/dev/null 2>&1; rc=$?
ck "Rust 形态 capture-pane '=cc-p1:' → rc≠0" "nonzero" "$([ $rc -ne 0 ] && echo nonzero || echo zero)"
T set-option -t '=cc-p1:' @ccm_sid X >/dev/null 2>&1; rc=$?
ck "set-option '=cc-p1:' → rc≠0" "nonzero" "$([ $rc -ne 0 ] && echo nonzero || echo zero)"
ck "兄弟未被写入 @ccm_sid" "unset" "$(T show-options -v -t '=cc-p1-2:' @ccm_sid 2>/dev/null || echo unset)"
T kill-session -t '=cc-p1:' >/dev/null 2>&1; rc=$?
ck "Rust 形态 kill-session '=cc-p1:' → rc≠0" "nonzero" "$([ $rc -ne 0 ] && echo nonzero || echo zero)"
ck "兄弟 cc-p1-2 仍存活" "cc-p1-2 " "$(sessions)"

echo
echo "===== 场景 B：cc-p1 与 cc-p1-2 并存 ====="
reset; T new-session -d -s cc-p1-2; T new-session -d -s cc-p1; sleep 0.5
bash -c "$(CMD resumeIntoExisting)" >/dev/null 2>&1
sleep 1
ck "载荷落进 cc-p1" "HIT" "$(pane cc-p1 | grep -q CCMPROBE && echo HIT || echo MISS)"
ck "兄弟 cc-p1-2 未被污染" "clean" "$(pane cc-p1-2 | grep -q CCMPROBE && echo POLLUTED || echo clean)"
T set-option -t '=cc-p1:' @ccm_sid PROBE-SID >/dev/null 2>&1
ck "set-option '=cc-p1:' → 读回自己写的值" "PROBE-SID" "$(T show-options -v -t '=cc-p1:' @ccm_sid 2>/dev/null)"
ck "兄弟未被写入 @ccm_sid" "unset" "$(T show-options -v -t '=cc-p1-2:' @ccm_sid 2>/dev/null || echo unset)"
T kill-session -t '=cc-p1:' >/dev/null 2>&1; rc=$?
ck "kill-session '=cc-p1:' → rc=0" "zero" "$([ $rc -eq 0 ] && echo zero || echo nonzero)"
ck "只杀掉 cc-p1，cc-p1-2 存活" "cc-p1-2 " "$(sessions)"

echo
echo "===== 场景 C：glob 不得生效 ====="
reset; T new-session -d -s alpha; sleep 0.4
T kill-session -t '=a*a:' >/dev/null 2>&1; rc=$?
ck "kill-session '=a*a:' → rc≠0" "nonzero" "$([ $rc -ne 0 ] && echo nonzero || echo zero)"
ck "alpha 存活" "alpha " "$(sessions)"

echo
echo "===== 场景 D：新建路径（生产串）建自己的会话，不碰兄弟 ====="
reset; T new-session -d -s cc-p1-2; sleep 0.4
bash -c "$(CMD resumeTmux)" >/dev/null 2>&1
sleep 1
ck "cc-p1 被建出来" "yes" "$(T has-session -t '=cc-p1:' 2>/dev/null && echo yes || echo no)"
ck "载荷落进新建的 cc-p1" "HIT" "$(pane cc-p1 | grep -q CCMPROBE && echo HIT || echo MISS)"
ck "兄弟 cc-p1-2 未被污染" "clean" "$(pane cc-p1-2 | grep -q CCMPROBE && echo POLLUTED || echo clean)"
ck "@ccm_sid 写在 cc-p1 上" "p1" "$(T show-options -v -t '=cc-p1:' @ccm_sid 2>/dev/null)"

echo
echo "===== 场景 E：posixQuote 名路径（buildLauncherCmd 生产串）+ pty 下 attach 正例 ====="
reset; T new-session -d -s cc-p1-2; sleep 0.4
bash -c "$(CMD launcher)" >/dev/null 2>&1
sleep 1
ck "cc-p1 被建出（posixQuote 名路径）" "yes" "$(T has-session -t '=cc-p1:' 2>/dev/null && echo yes || echo no)"
ck "载荷落进 cc-p1" "HIT" "$(pane cc-p1 | grep -q CCMPROBE && echo HIT || echo MISS)"
ck "兄弟 cc-p1-2 未被污染" "clean" "$(pane cc-p1-2 | grep -q CCMPROBE && echo POLLUTED || echo clean)"
# pty 下 attach 正例：真名存在 → 能接上（不再是"无 tty 必失败"的伪证据）
if command -v script >/dev/null 2>&1; then
  out=$(timeout 4 script -qec "$TMUX_BIN -L $SOCK attach -t '=cc-p1:'" /dev/null 2>&1 | head -c 200)
  ck "pty 下 attach '=cc-p1:' 能接上真名" "ok" "$(echo "$out" | grep -q "can.t find" && echo notfound || echo ok)"
  out2=$(timeout 4 script -qec "$TMUX_BIN -L $SOCK attach -t '=cc-nosuch:'" /dev/null 2>&1 | head -c 200)
  ck "pty 下 attach 不存在名 → can't find" "notfound" "$(echo "$out2" | grep -q "can.t find" && echo notfound || echo ok)"
else
  echo "SKIP | 无 script(1)，跳过 pty attach 正例"
fi

reset; rm -rf "$SHIM"
echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
