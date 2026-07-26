#!/usr/bin/env bash
# auto-e2e F-E2(命令级整合):resume idle 就地复用（#75/#76）——真源命令构造 + 真 tmux + fake-claude,
# 断言 argv.log(CLAUDE_CONFIG_DIR + resume 命令) 与 `tmux ls` 孤儿数。**GUI 触发在 Linux 结构性不可达**:
# `launch.rs::launch_powershell_window` 仅 Windows(`Err("拉起终端窗口仅支持 Windows")`)——headless Linux
# 的 GUI resume 必回退剪贴板、绝不执行,故 argv 断言的诚实天花板 = 命令级(直接驱真源 builder)。见 e2e/README。
#
# 每条边界:①用 `resume-cmd-driver.ts` 取 app **真正会跑**的命令串(import 真实 remote-launch.ts,
# 不重写);②断言命令形状(复用 cc-<sid8> 名/无 new-session/无 -N/CLAUDE_CONFIG_DIR 前缀);③真把该串
# 跑到真 tmux(send-keys 进 idle pane 的 sh);④断言 argv.log(sid 命中行的 CLAUDE_CONFIG_DIR + `--resume`)
# 与 `tmux list-sessions` 孤儿计数。红线:daemon 零改(不跑它) / 隔离 CLAUDE_CONFIG_DIR 绝不碰真 ~/.claude。
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
FAKE="$REPO/e2e/fake-claude"
DRIVER="$REPO/e2e/resume-cmd-driver.ts"
GEN="$REPO/e2e/gen-idle-tmux.sh"

# 三个隔离 CLAUDE_CONFIG_DIR:remote/base(初始 live+idle 落这)、账号 A、账号 B(模拟两账号,#75/pin)。
REMOTE_DIR="${CCM_E2E_REMOTE_DIR:-/tmp/e2e-resume-remote}"
ACCT_A="${CCM_E2E_ACCT_A:-/tmp/e2e-resume-acctA}"
ACCT_B="${CCM_E2E_ACCT_B:-/tmp/e2e-resume-acctB}"

pass=0; fail=0
ok()  { echo "  PASS $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }
SESSIONS=()  # 建过的 tmux 会话名,cleanup 兜底

cleanup() {
  set +e
  for s in "${SESSIONS[@]:-}"; do [ -n "$s" ] && tmux kill-session -t "$s" 2>/dev/null; done
  # 兜底:kill 掉可能残留的 fake-claude(隔离目录里的 pidfile)
  for d in "$REMOTE_DIR" "$ACCT_A" "$ACCT_B" "/tmp/e2e-remote-claude"; do
    for pf in "$d"/sessions/*.json; do
      [ -f "$pf" ] || continue
      p="$(awk -F'[:,]' '{for(i=1;i<=NF;i++) if($i ~ /"pid"/){print $(i+1); exit}}' "$pf" 2>/dev/null)"
      [ -n "$p" ] && kill "$p" 2>/dev/null
    done
  done
}
trap cleanup EXIT

drv() { npx tsx "$DRIVER" "$@"; }

# fire_resume <cmd>:真把 app 会跑的 resume 串执行到 tmux(send-keys 进 idle pane)。命令尾部的
# `tmux attach` 无 tty 即刻失败(headless,无害)——send-keys 已把 resume 打进 pane 的 sh。timeout 兜底。
fire_resume() { timeout 8 bash -c "$1" >/dev/null 2>&1 || true; }

# 造一个 idle-tmux(灰):跑 fake-claude→拿 pid→kill→pane 落回 sh(会话+@ccm_sid 仍在)。回显 session 名。
make_idle() {  # <sid> <claude_dir>
  local sid="$1" dir="$2" sess="cc-${1:0:8}" pf p
  rm -rf "$dir"; mkdir -p "$dir/sessions" "$dir/projects"
  CLAUDE_CONFIG_DIR="$dir" CCM_E2E_FAKE_CLAUDE="$FAKE" bash "$GEN" "$sid" >/dev/null
  SESSIONS+=("$sess")
  for _ in $(seq 1 20); do
    pf="$(ls "$dir"/sessions/*.json 2>/dev/null | head -1 || true)"
    [ -n "$pf" ] && { p="$(awk -F'[:,]' '{for(i=1;i<=NF;i++) if($i ~ /"pid"/){print $(i+1); exit}}' "$pf")"; break; }
    sleep 0.3
  done
  [ -n "${p:-}" ] || { echo "make_idle: fake-claude 未落 pidfile($dir)"; return 1; }
  kill "$p" 2>/dev/null || true       # claude 退 → idle(灰)
  sleep 0.6
  echo "$sess"
}

# 等 argv.log 里出现「sid 命中且含 --resume」的行,回显之(取最后一条)。超时非零。
wait_argv_resume() {  # <claude_dir> <sid> <timeout-s>
  local dir="$1" sid="$2" to="$3" i hit
  for ((i=0; i<to*2; i++)); do
    hit="$(grep -E "sid=$sid .*argv=--resume $sid" "$dir/argv.log" 2>/dev/null | tail -1 || true)"
    [ -n "$hit" ] && { echo "$hit"; return 0; }
    sleep 0.5
  done
  return 1
}

# tmux 里以 <base> 为前缀的 `-N` 孤儿计数(cc-<sid8>-2/-3…);base 自身不算孤儿。
# pipefail 安全:grep 无命中(=0 孤儿,正是好结果)会 exit 1 → `|| true` 吞掉,否则 set -e 误杀脚本。
orphan_count() {  # <base>
  local n
  n="$(tmux list-sessions -F '#{session_name}' 2>/dev/null | { grep -cE "^$1-[0-9]+$" || true; })"
  echo "${n:-0}"
}
session_exists() { tmux has-session -t "$1" 2>/dev/null && echo 1 || echo 0; }

# 基座(不带 pin / unset CLAUDE_CONFIG_DIR)resume 时 fake-claude 落回它自身默认目录
# (模拟真 claude 落 ~/.claude);pin 账号则落 export 指定目录。cwd 目录须真实存在(tmux -c / cd &&)。
BASE_DEFAULT="/tmp/e2e-remote-claude"
CWD_DIR="/tmp/e2e-remote"

echo "== F-E2 resume 命令级整合套件（真源 builder + 真 tmux + fake-claude）=="
echo "repo=$REPO  remote=$REMOTE_DIR  acctA=$ACCT_A  acctB=$ACCT_B  base=$BASE_DEFAULT"
rm -rf "$ACCT_A" "$ACCT_B" "$BASE_DEFAULT"
mkdir -p "$ACCT_A/sessions" "$ACCT_A/projects" "$ACCT_B/sessions" "$ACCT_B/projects" "$BASE_DEFAULT/sessions" "$CWD_DIR"

# ── B1:远端 archived + idle-tmux(灰) → Resume(tmux) 就地复用 cc-<sid8>,无 -N 孤儿 ──────────
echo "-- B1 idle 就地复用(治 #76:复用名不产孤儿)--"
SID1="$(cat /proc/sys/kernel/random/uuid)"; S1="cc-${SID1:0:8}"
make_idle "$SID1" "$REMOTE_DIR" >/dev/null
CMD1="$(drv into-existing "$SID1" "$S1" "$FAKE" -)"
echo "   cmd: $CMD1"
if echo "$CMD1" | grep -q "send-keys -t $S1 " && ! echo "$CMD1" | grep -q "new-session"; then
  ok "B1 命令复用原名 $S1、无 new-session(就地 resume,治 #76 根因)"
else bad "B1 命令未就地复用(含 new-session 或名不符)"; fi
BEFORE1="$(session_exists "$S1")"
fire_resume "$CMD1"
# 基座 resume(unset CLAUDE_CONFIG_DIR)→ fake-claude 落回自身默认目录(模拟真 claude 落 ~/.claude)。
if AL="$(wait_argv_resume "$BASE_DEFAULT" "$SID1" 12)"; then ok "B1 argv.log 见 resume: $AL"; else bad "B1 12s 内 argv.log 无 resume 行"; fi
ORPH1="$(orphan_count "$S1")"; AFTER1="$(session_exists "$S1")"
echo "   before=$BEFORE1 after=$AFTER1 orphan($S1-N)=$ORPH1 | ls: $(tmux list-sessions -F '#{session_name}' 2>/dev/null | grep "^${S1}" | paste -sd, -)"
if [ "$AFTER1" = 1 ] && [ "$ORPH1" = 0 ]; then ok "B1 复用后 $S1 仍在且孤儿数=0(无 cc-<sid8>-N,#76 治愈)"; else bad "B1 孤儿数=$ORPH1(#76 回归)或会话丢失"; fi

# ── B2:远端 archived(无 tmux) → Resume 新建 session,argv CLAUDE_CONFIG_DIR = 目标账号 A ──────
echo "-- B2 无 tmux → 新建 resume,注入账号 A 目录(直连 + tmux-new 两形态)--"
SID2="$(cat /proc/sys/kernel/random/uuid)"; S2="cc-${SID2:0:8}"
# 直连形态(resumeTab 路径):断言命令构造含账号 A 前缀(直连在前台跑 fake-claude 会阻塞,
# 执行验证交给下面非阻塞的 tmux-new 形态,两者共用 buildEnvPrefix,注入语义一致)。
CMD2D="$(drv direct "$SID2" "$CWD_DIR" "$FAKE" "$ACCT_A")"
echo "   direct: $CMD2D"
echo "$CMD2D" | grep -q "export CLAUDE_CONFIG_DIR='$ACCT_A'" && ok "B2 直连命令含 CLAUDE_CONFIG_DIR=$ACCT_A" || bad "B2 直连命令缺账号 A 前缀"
# tmux-new 形态(resumeTabTmux 归档分支):新建 cc-<sid8> 会话 + @ccm_sid,真执行 → argv 落 A。
CMD2T="$(drv tmux-new "$SID2" "$CWD_DIR" "$FAKE" "$S2" "$ACCT_A")"
echo "   tmux-new: $CMD2T"
SESSIONS+=("$S2")
echo "$CMD2T" | grep -q "new-session -d -s $S2" && echo "$CMD2T" | grep -q "@ccm_sid $SID2" && ok "B2 tmux-new 建 $S2 并设 @ccm_sid" || bad "B2 tmux-new 命令形状不符"
fire_resume "$CMD2T"
if AL="$(wait_argv_resume "$ACCT_A" "$SID2" 12)"; then
  echo "   argv(A): $AL"
  echo "$AL" | grep -q "CLAUDE_CONFIG_DIR=$ACCT_A " && ok "B2 新建 resume argv 落账号 A 目录($ACCT_A)" || bad "B2 argv CLAUDE_CONFIG_DIR≠A"
else bad "B2 12s 内 A 目录 argv.log 无 resume(tmux-new)"; fi
[ "$(session_exists "$S2")" = 1 ] && [ "$(orphan_count "$S2")" = 0 ] && ok "B2 tmux-new 建出 $S2、无孤儿" || bad "B2 tmux-new 未建会话或有孤儿"

# ── B3:带账号 pin「用账号 B resume」→ argv CLAUDE_CONFIG_DIR = B(非 remote/base,非 A)──────────
echo "-- B3 pin 账号 B:idle 复用 + 注入 B 目录(两隔离账号验证)--"
SID3="$(cat /proc/sys/kernel/random/uuid)"; S3="cc-${SID3:0:8}"
make_idle "$SID3" "$REMOTE_DIR" >/dev/null
CMD3="$(drv into-existing "$SID3" "$S3" "$FAKE" "$ACCT_B")"
echo "   cmd: $CMD3"
# idle 复用是 send-keys 形态:整个载荷再被 posixQuote 包一层,内层 ' → '\''(故不按裸引号 grep);
# 断言 export 前缀 + B 目录路径同时出现即可,真正的路由证据是下面 argv 落 B 目录。
if echo "$CMD3" | grep -q "export CLAUDE_CONFIG_DIR=" && echo "$CMD3" | grep -qF "$ACCT_B"; then
  ok "B3 pin 命令含 export CLAUDE_CONFIG_DIR + $ACCT_B"
else bad "B3 pin 命令缺 B 前缀"; fi
fire_resume "$CMD3"
if AL="$(wait_argv_resume "$ACCT_B" "$SID3" 12)"; then
  echo "   argv(B): $AL"
  # 关键:argv 落 B 目录,且 remote/base 目录里**不该**有本 sid 的 resume(证明 pin 路由到 B 不是 base)
  echo "$AL" | grep -q "CLAUDE_CONFIG_DIR=$ACCT_B " && ok "B3 argv 落账号 B 目录($ACCT_B)" || bad "B3 argv CLAUDE_CONFIG_DIR≠B"
  if grep -qE "sid=$SID3 .*argv=--resume" "$REMOTE_DIR/argv.log" 2>/dev/null; then bad "B3 pin 泄漏:remote/base 目录也记到该 resume"; else ok "B3 pin 隔离:remote/base 目录无该 sid resume(不串号)"; fi
else bad "B3 12s 内 B 目录 argv.log 无 resume"; fi

# ── B4:不带 pin → 基座 + follow 解析当前工作账号（#75 主因）──────────────────────────────
echo "-- B4 不带 pin:命令走基座(unset CLAUDE_CONFIG_DIR)+ resolveFollowAccount(真源)解析 --"
SID4="$(cat /proc/sys/kernel/random/uuid)"; S4="cc-${SID4:0:8}"
make_idle "$SID4" "$REMOTE_DIR" >/dev/null
CMD4="$(drv into-existing "$SID4" "$S4" "$FAKE" -)"
echo "   cmd: $CMD4"
echo "$CMD4" | grep -q "unset CLAUDE_CONFIG_DIR;" && ok "B4 基座命令前置 unset CLAUDE_CONFIG_DIR(清空 shell 残留旧号,#75 复用变体逃生口)" || bad "B4 基座命令缺 unset CLAUDE_CONFIG_DIR"
# #75 主因:不带 pin 时的跟随解析——lastAccount 无 → 当前工作账号 current(真源 resolveFollowAccount)。
STATE_B4='{"accounts":[{"name":"work","email":"","configDir":"'"$ACCT_A"'","isDefault":true,"mode":"isolated","exists":true,"loggedIn":true}]}'
FOL="$(drv follow - work "$STATE_B4")"
[ "$FOL" = "work" ] && ok "B4 无 pin → resolveFollowAccount 落当前工作账号 work(#75:不再散落基座错目录)" || bad "B4 follow 解析=$FOL(期望 work)"
fire_resume "$CMD4"
if AL="$(wait_argv_resume "$BASE_DEFAULT" "$SID4" 12)"; then ok "B4 基座 resume 执行(argv 落默认隔离目录): $AL"; else bad "B4 12s 内基座 argv 无 resume"; fi

# ── B6a:重复 resume 幂等(tmux-new create-gate:第二次 new-session 失败短路,不双 resume)──────
echo "-- B6a 重复 resume 幂等(create-gate 短路)--"
SID6="$(cat /proc/sys/kernel/random/uuid)"; S6="cc-${SID6:0:8}"
rm -f "$ACCT_A/argv.log"
CMD6="$(drv tmux-new "$SID6" "/tmp/e2e-remote" "$FAKE" "$S6" "$ACCT_A")"
SESSIONS+=("$S6")
fire_resume "$CMD6"; sleep 1
fire_resume "$CMD6"; sleep 1     # 第二次:会话已存在 → new-session 2>/dev/null 失败 → && 短路跳过 send-keys
N6="$(grep -cE "sid=$SID6 .*argv=--resume" "$ACCT_A/argv.log" 2>/dev/null || true)"
echo "   argv resume 行数=$N6  orphan($S6-N)=$(orphan_count "$S6")  session=$(session_exists "$S6")"
[ "$N6" = 1 ] && [ "$(orphan_count "$S6")" = 0 ] && ok "B6a 幂等:仅 1 次 resume(第二次被 create-gate 短路)、0 孤儿" || bad "B6a 非幂等(resume 行数=$N6 或有孤儿)"

# ── B6b:tmux 已消失 → 回退新建(pickFreshTmuxName base 空 → 复用 base 名新建)────────────────
echo "-- B6b tmux 已消失 → 回退新建 --"
SID7="$(cat /proc/sys/kernel/random/uuid)"; S7="cc-${SID7:0:8}"
FRESH="$(drv pick-fresh "$SID7" "cc-unrelated,cc-other")"
[ "$FRESH" = "$S7" ] && ok "B6b 无撞名 → pickFreshTmuxName 复用 base 名 $S7" || bad "B6b pick-fresh=$FRESH(期望 $S7)"
CMD7="$(drv tmux-new "$SID7" "/tmp/e2e-remote" "$FAKE" "$FRESH" -)"
SESSIONS+=("$S7")
fire_resume "$CMD7"; sleep 1
[ "$(session_exists "$S7")" = 1 ] && [ "$(orphan_count "$S7")" = 0 ] && ok "B6b 回退新建出 $S7、无孤儿" || bad "B6b 回退未建会话或有孤儿"

# ── B6c:会话仍 live → 守卫不误动(create-gate 短路,不双 resume/不产孤儿)──────────────────────
echo "-- B6c 会话仍 live → create-gate 守卫不误动 --"
SID8="$(cat /proc/sys/kernel/random/uuid)"; S8="cc-${SID8:0:8}"
rm -f "$ACCT_A/argv.log"
# 先真起一个 live(fake-claude 常驻,不 kill)——模拟目标会话仍活
CMD8A="$(drv tmux-new "$SID8" "/tmp/e2e-remote" "$FAKE" "$S8" "$ACCT_A")"
SESSIONS+=("$S8")
fire_resume "$CMD8A"; sleep 1
N8_1="$(grep -cE "sid=$SID8 .*argv=--resume" "$ACCT_A/argv.log" 2>/dev/null || true)"
# 会话 live 时再发一次 tmux-new resume:new-session 失败短路 → 不再 resume(守卫)
fire_resume "$CMD8A"; sleep 1
N8_2="$(grep -cE "sid=$SID8 .*argv=--resume" "$ACCT_A/argv.log" 2>/dev/null || true)"
echo "   resume 行数 起始=$N8_1 再触发后=$N8_2  orphan=$(orphan_count "$S8")"
[ "$N8_1" = 1 ] && [ "$N8_2" = 1 ] && [ "$(orphan_count "$S8")" = 0 ] && ok "B6c live 时再 resume 被守卫短路(行数仍=1)、0 孤儿" || bad "B6c 守卫误动(行数 $N8_1→$N8_2 或有孤儿)"

echo "== 结果:$pass 过 / $fail 败 =="
[ "$fail" -eq 0 ]
