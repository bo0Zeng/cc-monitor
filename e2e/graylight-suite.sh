#!/usr/bin/env bash
# auto-e2e F-E1(全链级):驱 gray-light 生命周期,断言 monitor 日志里的 `[e2e] tab-state` 序列。
# **前置**(同 e2e/f40-suite.sh 契约,见 e2e/README):
#   - Xvfb 上跑着 dev 实例(`npx tauri dev`,DEV 探针内建);
#   - config.json 配了一个 loopback 远端,daemonPath 指向 e2e/daemon-wrapper.sh
#     (把 daemon 的 CLAUDE_CONFIG_DIR 钉到隔离 fixture 目录,防与本地会话双 tab);
#   - 本机可读 monitor 日志(fe_perf/[e2e] 行是断言数据源)。
# 序列(跨进程整链,单测碰不到):
#   建 fixture(fake-claude 活 + @ccm_sid) → app 经 daemon SessionAdded 建 live 远端 tab
#   → kill fake-claude(留 tmux shell) → daemon SessionRemoved + TmuxSessions 仍带 @ccm_sid
#     → emitter 判 Idle → SESSION_IDLE → tabs.markTmuxIdle → `[e2e] tab-state … status=live tmuxIdle=1`(灰)
#   → tmux kill-session(另留一个无关 cc-* 防空 backend 卡灰,§24bis) → @ccm_sid 消失
#     → 收割/对账 retire → SESSION_ENDED → tabs.archiveTab → `[e2e] tab-state … status=archived`
# **status=live tmuxIdle=1 这一行同时证明**:该 tab 变灰前是 live(status 字段)+ 此刻进灰(tmuxIdle=1)。
set -euo pipefail

# ── G-C（解 BACKLOG E41）：把整套件钉在**自己的 tmux server** 上 ──────────────────
# 此前这套件裸调 tmux ⇒ 在开发者机器上会**直接操作默认 socket 上的真实会话**，
# 所以它既进不了 CI 也不敢在有活会话的机器上跑（E41）。
#
# ⚠⚠ **这里原有一整段头注，逐字写着「两件事都必须做，缺一就不隔离」（`unset TMUX` +
# `TMUX_TMPDIR`）—— 已删，因为那段话把一个会出事的形态写成了纪律。** 它自己都记着
# 「设了 `TMUX_TMPDIR` 仍在默认 socket 上建出了会话」，结论却是「所以两件都要做」；
# 而正确的结论是「**别靠环境变量做隔离**」。08-11 的事故正是漏了那两件里的一件。
# 保留这几行是为了让下一个人知道**为什么不能改回去**。
#
# ★★★ **C7i 红线改造〔08-12〕：隔离改成 `-L` shim，不再靠环境变量。**
#
# 上面那段（已删）逐字写着「两件事都必须做，缺一就不隔离」——`unset TMUX` + `TMUX_TMPDIR`。
# **那个形态本身就是病灶**：2026-08-11 实测事故 —— 一条探针写了 `TMUX_TMPDIR=… tmux kill-server`
# 却漏了 `unset TMUX`，`$TMUX` 有值时 tmux **按它给的 socket 走、`TMUX_TMPDIR` 完全不起作用**
# ⇒ 那条命令打到用户真实 server 上，**9 个真实 tmux 会话没了**。
#
# ⇒ C7i 立为红线：**tmux 命令一律带 socket 选择器（`-S <绝对路径>` 或 `-L <名>`），
#   禁止靠 `TMUX_TMPDIR`/`unset TMUX` 做隔离。**
#
# 现在的形态：把 `$BIN/tmux` 放进 PATH 最前，它 `exec` 真 tmux 并**强插 `-L e2eGray`**。
# · 漏什么环境变量都打不偏 —— 选择器写死在 shim 里，不依赖「记得清某个变量」；
# · 零调用点改动的好处**原样保留**：套件里的裸 `tmux` 一个不用改，
#   连它 shell out 出去的东西（`ccm` / `cc-spawn` 内部也裸调 tmux）也一并覆盖；
# · `unset TMUX` **仍然保留**，但它现在只是「让被测行为发生」（tmux 内会退化成就地起），
#   **不再是隔离手段** —— 隔离由 shim 独自负责。
unset TMUX TMUX_PANE
_GC_SOCK="e2eGray"
_GC_REAL_TMUX="$(command -v tmux)" || { echo "需要 tmux"; exit 1; }
_GC_BIN="$(mktemp -d /tmp/e2e-tmuxshim.XXXXXX)"
printf '#!/bin/sh\nexec %s -L %s "$@"\n' "$_GC_REAL_TMUX" "$_GC_SOCK" > "$_GC_BIN/tmux"
chmod +x "$_GC_BIN/tmux"
export PATH="$_GC_BIN:$PATH"

# ★ 前置断言**经登录 shell 问** —— 08-12 实测教训：在外层 shell 量 `command -v tmux` 会报 PASS，
#   而命令真正跑在 `bash -lic` 里（PATH 被 profile 重排过）⇒「隔离没生效」以 PASS 的形式呈现。
_gc_probe="$(bash -lic 'command -v tmux' 2>/dev/null | tail -1)"
if [ "$_gc_probe" != "$_GC_BIN/tmux" ]; then
  echo "  ABORT 隔离没生效：登录 shell 里的 tmux 是 '$_gc_probe'，不是 shim $_GC_BIN/tmux"
  echo "        绝不降级裸跑 —— 那会打到用户真实 tmux server 上（C7i 红线）。"
  rm -rf -- "$_GC_BIN"
  exit 2
fi

# 收尾：只收自己那台（`-L` 选择器在，绝不裸 `kill-server`）。
_gc_sock_cleanup() {
  set +e
  [ -n "${_GC_REAL_TMUX:-}" ] && "$_GC_REAL_TMUX" -L "$_GC_SOCK" kill-server 2>/dev/null
  [ -n "${_GC_BIN:-}" ] && rm -rf -- "$_GC_BIN"
}
# ─────────────────────────────────────────────────────────────────────────────

DISPLAY="${E2E_DISPLAY:-:80}"; export DISPLAY
REPO="$(cd "$(dirname "$0")/.." && pwd)"
LOG="${E2E_LOG:-$(ls -t "$HOME"/.claude/claudecode-frontend/logs/monitor.*.log 2>/dev/null | head -1)}"
CLAUDE_DIR="${CCM_E2E_CLAUDE_DIR:-/tmp/e2e-remote-claude}"
GRAY_WAIT="${E2E_GRAY_WAIT:-30}"    # 灰:daemon 判活轮询(2s)+ TmuxSessions 帧(≤8s)+ emitter
ARCH_WAIT="${E2E_ARCH_WAIT:-40}"    # 归档:kill-session 后 TmuxSessions 帧 + 对账去抖

[ -f "$LOG" ] || { echo "monitor 日志不存在:$LOG(dev 实例在跑吗?)"; exit 1; }

# ★★★ **开跑前自证台架**〔P0b 08-12〕。**不满足一律 `ABORT`（exit 2），不许 FAIL。**
#
# 病史：查 #60 时**连着六次**跑出「1 过 2 败」，而**每一次的成因都不是 #60** ——
#   ① 陈旧 pidfile（读到上一跑的残骸，kill 打给死 pid）
#   ② `daemonPath` 指向真 daemon 而非本 wrapper（daemon 盯 `~/.claude` 不是 fixture）
#   ③ wrapper 副本搬出仓外后 `$REPO` 推错 ⇒ 回落到陈旧 daemon
#   ④ 跑的是 `target/debug/monitor` 而非 `npx tauri dev` ⇒ **DEV 探针整支被 vite 消除**
#   ⑤ `P0d` 换 socket 隔离后 daemon 与套件**分家**（daemon 在 SSH 那头，不吃 shim）
#   ⑥ dev 实例在跑套件之前就挂了
#
# ★ 它们**失败起来长得一模一样**（都是「没变灰」）⇒ 「测不到」一路伪装成「测到了缺陷」，
#   而每修好一条只会露出下一条。**这就是为什么台架有效性必须每跑自证，不能靠一次推断**
#   （`P0` 当初那句「台架处于有效状态」正是那样的一次推断）。
#
# 三格，各挡上面一族：
_abort() { echo "  ABORT $1"; echo "        —— 台架不成立，本跑测不到任何东西。**这不是 FAIL**。"; exit 2; }

# 甲：app 活着且**正在写这份日志**（挡 ⑥）。日志尾行时间戳与现在差太远 = 它已经不写了。
_last_ts="$(tail -200 "$LOG" | grep -oE '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:]{8}' | tail -1)"
if [ -n "$_last_ts" ]; then
  _age=$(( $(date +%s) - $(date -u -d "${_last_ts}Z" +%s 2>/dev/null || echo 0) ))
  [ "$_age" -lt 600 ] || _abort "monitor 日志已 ${_age}s 没有新行（dev 实例挂了？）：$LOG"
fi

# 乙：daemon **握手过且盯的是 fixture 目录**（挡 ②③⑤）。
_hello="$(grep 'daemon hello' "$LOG" | tail -1)"
[ -n "$_hello" ] || _abort "日志里一条 daemon hello 都没有 —— 远端没连上，本跑与 #60 无关"
case "$_hello" in
  *"claude_dir=$CLAUDE_DIR"*) : ;;
  *) _abort "daemon 盯的不是 fixture 目录（要 $CLAUDE_DIR）：$_hello" ;;
esac

# 丙：跑的是 **dev 实例**（挡 ④）。DEV 探针（`import.meta.env.DEV` 门控）是本套件两条主断言的
#     唯一数据源，生产构建里整支被 vite 消除 ⇒ 断言永不可能通过。
#
# ⚠ **不能拿「日志里有没有 `[e2e]` 行」当判据** —— 08-12 实测栽过一次：
#   那些行**全是事件驱动**的（tab 状态转移时才打），刚起的 dev 实例一条都没有
#   ⇒ 那条判据会把**健康的 dev 实例**判成「探针不存在」。
#   **它想验「探针存在」，量的却是「探针已经打过」** —— 射程错了一格。
# ⇒ 改量**结构性信号**：`tauri.conf.json` 的 `devUrl` 端口上有没有 vite。
#   只有 `npx tauri dev` 会起它；生产构建走 `frontendDist`，那个端口是空的。
_devport="$(grep -oE '"devUrl"[^0-9]*([0-9]+)' "$REPO/src-tauri/tauri.conf.json" | grep -oE '[0-9]+$')"
if [ -n "$_devport" ]; then
  curl -s -o /dev/null --max-time 3 "http://localhost:$_devport" \
    || _abort "devUrl 端口 $_devport 上没有 vite —— 跑的不是 \`npx tauri dev\`？DEV 探针会被 vite 整支消除，两条主断言永不可能通过"
fi

echo "  OK   台架自证通过（app 在写日志 · daemon 盯 $CLAUDE_DIR · dev 实例在 :$_devport）"

SID="$(cat /proc/sys/kernel/random/uuid)"; SID8="${SID:0:8}"
SESSION="cc-$SID8"; KEEP="cc-e2ekeep-$$"

pass=0; fail=0
ok()  { echo "  PASS $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }

cleanup() {
  set +e
  if [ -n "${FAKE_PID:-}" ]; then kill "$FAKE_PID" 2>/dev/null; fi
  tmux kill-session -t "=$SESSION:" 2>/dev/null
  tmux kill-session -t "=$KEEP:" 2>/dev/null
}
trap 'cleanup; _gc_sock_cleanup' EXIT

# 等 monitor 日志(从 start 行之后)出现匹配 pattern 的行,回显之;超时非零。
wait_log() {  # <startline> <grep-ere> <timeout-s>
  local start="$1" pat="$2" to="$3" i hit
  for ((i=0; i<to*2; i++)); do
    hit="$(tail -n "+$((start+1))" "$LOG" | grep -E "$pat" | tail -1 || true)"
    if [ -n "$hit" ]; then echo "$hit"; return 0; fi
    sleep 0.5
  done
  return 1
}

echo "== F-E1 full-chain gray-light 套件(display $DISPLAY)=="
echo "sid=$SID session=$SESSION claude_dir=$CLAUDE_DIR"
echo "log=$LOG"

# 无关 keepalive tmux 会话(kill fixture 后 backend 仍非空)
tmux new-session -d -s "$KEEP" "exec sh"

MARK="$(wc -l <"$LOG")"

# ── 建 live fixture(隔离目录,与 daemon-wrapper 一致)──────────────────────────
CLAUDE_CONFIG_DIR="$CLAUDE_DIR" CCM_E2E_FAKE_CLAUDE="$REPO/e2e/fake-claude" \
  bash "$REPO/e2e/gen-idle-tmux.sh" "$SID" >/dev/null
echo "-- fixture 已建:$SESSION(等 fake-claude 落 pidfile → app 经 daemon 建 live tab)--"

# fake-claude 在 tmux 内**异步**起,pidfile 晚于 gen-idle-tmux 返回 → 必须**轮询等它出现**
# 再读 pid(否则 glob 竞态读空 → 杀不到 → 不变灰,首跑实测踩中)。pidfile 落地 = live 前置成立。
#
# ★★★ **P0b 实测（08-12）：这里原来是 `ls …/*.json | head -1` —— 取目录里字典序第一个，
#     既不认本跑的 sid、也不验那个进程还活不活。**
#
# 后果是整套**空真**：`/tmp/e2e-remote-claude/sessions/` 不跨跑清理，于是
#   ① 「pidfile 落地」PASS —— 但拿到的是**上一跑的残骸**（实测两跑读到同一个 pid=1667736，
#      而两个 pid 早就都死了）；
#   ② 「kill fake-claude」打给一个已死的 pid ⇒ **no-op**；
#   ③ 「30s 内未见灰灯」FAIL —— 而 claude **根本没在这一跑里死过**。
# ⇒ **一次什么都没测的跑，失败起来和真的 #60 一模一样。**
#
# 这也意味着：凡是拿这套件读数当前提的结论（含 `P0` 那两跑推出的
# 「daemon 发出 → monitor 收到 那一段有缺口」），**台架有效性都还没被证成**。
#
# 修法两条，缺一不可：
#   · **先清**本跑要用的目录（陈旧 pidfile 是这一族的根）；
#   · 认 pidfile 只认**本跑的 sid**，并**校验进程还活着**（`kill -0`）——
#     两道都要，因为清理可能被上一跑的 trap 漏掉（那正是 08-12 撞到的形态）。
rm -rf -- "$CLAUDE_DIR/sessions"
mkdir -p "$CLAUDE_DIR/sessions"
FAKE_PID=""
for _ in $(seq 1 20); do
  for PF in "$CLAUDE_DIR"/sessions/*.json; do
    [ -f "$PF" ] || continue
    grep -q "\"$SID\"" "$PF" 2>/dev/null || continue   # 只认本跑的 sid
    _pid="$(awk -F'[:,]' '{for(i=1;i<=NF;i++) if($i ~ /"pid"/){print $(i+1); exit}}' "$PF")"
    # ★ 进程必须**真的活着** —— 陈旧 pidfile 会让整套空真（见上）。
    if [ -n "$_pid" ] && kill -0 "$_pid" 2>/dev/null; then FAKE_PID="$_pid"; break; fi
  done
  [ -n "$FAKE_PID" ] && break
  sleep 0.5
done
[ -n "$FAKE_PID" ] \
  && ok "live 前置:fake-claude pidfile 落地 pid=$FAKE_PID(app 经 daemon SessionAdded 建 live tab)" \
  || bad "10s 内 fake-claude 未落 pidfile 到隔离目录(fixture 失败)"

# **必须等 app 收到一帧含 @ccm_sid=sid 的 TmuxSessions**(daemon 每 8s 才发一次)再杀 claude,
# 否则 SessionRemoved 到达时 app 的 tmux 账本还没这条 → emitter classify_removed 找不到 @ccm_sid
# → 判 Archive(直接归档)而非 Idle(灰),灰灯永不出现(首跑实测踩中)。留足 > 一个 8s 发帧周期。
TMUX_SETTLE="${E2E_TMUX_SETTLE:-14}"
echo "-- 等 ${TMUX_SETTLE}s 让 app 收到含 @ccm_sid 的 TmuxSessions 帧(daemon 8s 发一次)--"
sleep "$TMUX_SETTLE"

# ── GRAY:kill fake-claude(留 tmux)→ 灰灯 tab-state(status=live tmuxIdle=1)────
echo "-- kill fake-claude pid=${FAKE_PID:-?}(claude 退,tmux shell 留)--"
[ -n "${FAKE_PID:-}" ] && kill "$FAKE_PID" 2>/dev/null || true
FAKE_PID=""
GRAY="$(wait_log "$MARK" "\[e2e\] tab-state sid=$SID8 status=live tmuxIdle=1" "$GRAY_WAIT")" \
  && ok "灰灯(live→gray):$GRAY" \
  || bad "${GRAY_WAIT}s 内未见灰灯 tab-state(sid=$SID8 status=live tmuxIdle=1)"

# ── ARCHIVE:tmux kill-session → archived tab-state ───────────────────────────
echo "-- tmux kill-session $SESSION(@ccm_sid 消失 → 归档)--"
tmux kill-session -t "=$SESSION:" 2>/dev/null || true
ARCH="$(wait_log "$MARK" "\[e2e\] tab-state sid=$SID8 status=archived" "$ARCH_WAIT")" \
  && ok "归档(gray→archived):$ARCH" \
  || bad "${ARCH_WAIT}s 内未见归档 tab-state(sid=$SID8 status=archived)"

echo "== 结果:$pass 过 / $fail 败 =="
# G-C：与另外 8 套逐字一致的收尾格式，好让 `e2e/assert-pass-floor.sh` 用同一条正则抓。
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ]
