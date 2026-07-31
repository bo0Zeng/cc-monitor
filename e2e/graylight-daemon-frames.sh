#!/usr/bin/env bash
# auto-e2e F-E1(fallback 级):**daemon 线上帧**断言 gray-light 的后端半场——单测碰不到的
# 跨进程/tmux 判活边界。不需 GUI/SSH:直接把 daemon 二进制指向隔离 fixture 跑,读它 stdout 的
# 线协议帧(JSON-per-line,`{"kind":...}`)。序列:
#   SessionAdded(sid)               —— fake-claude 活 + @ccm_sid 已置 → live
#   (kill fake-claude 进程,tmux 会话留活)
#   SessionRemoved(sid)             —— daemon 2s 判活轮询发现 claude 死
#   TmuxSessions.raw 仍含 @ccm_sid   —— **关键**:claude 死但 tmux 还在 → monitor emitter 据此走
#                                       Idle(灰)而非 Archive(前端半场:markTmuxIdle→tmuxIdle=1)
#   (tmux kill-session)
#   TmuxSessions.raw 不再含 sid      —— @ccm_sid 没了 → monitor 归档触发边沿(archived)
# 前端 emitter→灰灯半场由单测(tabs.vitest.ts)+ 全链 GUI 跑覆盖;本脚本钉住后端边沿。
# 红线:daemon 零改动(只跑它) / 不碰真 ~/.claude(CLAUDE_CONFIG_DIR 隔离) / 不改 TMUX_LS_FMT。
set -euo pipefail

# ── G-C（解 BACKLOG E41）：把整套件钉在**自己的 tmux server** 上 ──────────────────
# 此前这套件裸调 tmux ⇒ 在开发者机器上会**直接操作默认 socket 上的真实会话**，
# 所以它既进不了 CI 也不敢在有活会话的机器上跑（E41）。
#
# **两件事都必须做，缺一就不隔离**（2026-07-30 本机实测）：
#   ① `unset TMUX` —— 从 tmux 会话里跑这套件时，`$TMUX` 会让客户端连**外层那台 server**
#      并**完全忽略 `TMUX_TMPDIR`**（实测：设了 TMUX_TMPDIR 仍在默认 socket 上建出了会话）。
#      **这才是 E41 的实质**：不只是「缺 `-L`」，是「继承了 `$TMUX`」。
#   ② `TMUX_TMPDIR` 必须是**短路径** —— unix socket 路径上限 108 字节，指向长目录时
#      tmux 报 `File name too long`（实测在 scratchpad 那种长路径上必踩）。
#
# 这样做的好处是**零调用点改动**：套件里 84 处裸 `tmux` 一个都不用改，
# 也自动覆盖它 shell out 出去的东西（`ccm` / `cc-spawn` 内部也是裸调 tmux）。
unset TMUX TMUX_PANE
TMUX_TMPDIR="$(mktemp -d /tmp/e2e-sock.XXXXXX)"; export TMUX_TMPDIR
# 收尾：只用 **`-S <私有 socket>`** 收自己那台（**绝不裸 `kill-server`** —— 万一上面的
# 隔离没生效，裸的那个会打到用户的 server 上）。server 无会话时本就会自己退，这条是兜底。
_gc_sock_cleanup() {
  set +e
  [ -n "${TMUX_TMPDIR:-}" ] && /usr/bin/tmux -S "$TMUX_TMPDIR/tmux-$(id -u)/default" kill-server 2>/dev/null
  [ -n "${TMUX_TMPDIR:-}" ] && rm -rf -- "$TMUX_TMPDIR"
}
# ─────────────────────────────────────────────────────────────────────────────

E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$E2E_DIR/.." && pwd)"
DAEMON="${CCM_E2E_DAEMON:-$REPO/remote-daemon-proto/target/debug/cc-monitor-remote}"
CLAUDE_DIR="${CCM_E2E_CLAUDE_DIR:-/tmp/e2e-remote-claude}"
WORK="$(mktemp -d /tmp/e2e-graylight.XXXXXX)"
FRAMES="$WORK/frames.jsonl"
DAEMON_ERR="$WORK/daemon.stderr"

[ -x "$DAEMON" ] || { echo "daemon 二进制不存在/不可执行:$DAEMON"; exit 1; }
command -v tmux >/dev/null || { echo "无 tmux"; exit 1; }

SID="$(cat /proc/sys/kernel/random/uuid)"
SID8="${SID:0:8}"
SESSION="cc-$SID8"
KEEP="cc-e2ekeep-$$"   # 无关 cc-* 会话:kill 掉 fixture 后 backend 仍非空(§24bis 空 backend 守卫)

pass=0; fail=0
ok()  { echo "  PASS $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }

cleanup() {
  set +e
  [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
  # fixture pane 的 fake-claude(若还活)+ 两个 tmux 会话
  if [ -n "${FAKE_PID:-}" ]; then kill "$FAKE_PID" 2>/dev/null; fi
  tmux kill-session -t "=$SESSION:" 2>/dev/null
  tmux kill-session -t "=$KEEP:" 2>/dev/null
  rm -rf "$CLAUDE_DIR" "$WORK"
}
trap 'cleanup; _gc_sock_cleanup' EXIT

echo "== F-E1 daemon-frame gray-light 套件 =="
echo "sid=$SID  session=$SESSION  claude_dir=$CLAUDE_DIR"
echo "daemon=$DAEMON"

# 干净 fixture 目录
rm -rf "$CLAUDE_DIR"
mkdir -p "$CLAUDE_DIR/sessions" "$CLAUDE_DIR/projects"

# 无关 keepalive tmux 会话(空 shell)
tmux new-session -d -s "$KEEP" "exec sh"

# 造 fixture:cc-<sid8> 跑 fake-claude(该 sid)+ 置 @ccm_sid。**导出 CLAUDE_CONFIG_DIR**——
# gen-idle-tmux 会把它内联进 tmux 命令串,让 fake-claude 落 pidfile 到隔离 fixture 而非真 ~/.claude。
CCM_E2E_FAKE_CLAUDE="$E2E_DIR/fake-claude" CLAUDE_CONFIG_DIR="$CLAUDE_DIR" \
  bash "$E2E_DIR/gen-idle-tmux.sh" "$SID" >/dev/null

# 启动 daemon(隔离 CLAUDE_CONFIG_DIR),stdout=帧,stderr 分离
CLAUDE_CONFIG_DIR="$CLAUDE_DIR" "$DAEMON" >"$FRAMES" 2>"$DAEMON_ERR" &
DAEMON_PID=$!

# 轮询帧日志直到出现 pattern(在给定起始行之后),或超时。回显命中行。
# 用法:wait_line <startline> <grep-ere> <timeout-s> <desc>
wait_line() {
  local start="$1" pat="$2" to="$3" desc="$4" i hit
  for ((i=0; i<to*2; i++)); do
    hit="$(tail -n "+$((start+1))" "$FRAMES" | grep -E "$pat" | head -1 || true)"
    if [ -n "$hit" ]; then echo "$hit"; return 0; fi
    sleep 0.5
  done
  return 1
}

# ── 1. LIVE:SessionAdded(sid)────────────────────────────────────────────────
SA="$(wait_line 0 "\"kind\":\"session_added\".*$SID" 15 'session_added')" \
  && { ok "SessionAdded(live):$SA"; } \
  || { bad "15s 内未见 SessionAdded($SID)"; }

# tmux 帧带 @ccm_sid(claude 活时)
TS_LIVE="$(wait_line 0 "\"kind\":\"tmux_sessions\".*$SID" 12 'tmux_sessions live')" \
  && ok "TmuxSessions 带 @ccm_sid(live):$(printf '%.160s' "$TS_LIVE")" \
  || bad "12s 内未见含 @ccm_sid 的 tmux_sessions 帧"

# ── 1bis. S0：**同 pidfile 原地换 sid**（= 用户的 `/branch`）───────────────────
#
# 用户 2026-07-30 实测：「执行 branch 后原本的 tab 不会变灰，而是变成灰点也杀不掉」。
# 机制：`/branch` **不重启 claude 进程**（`shared/ccm` 末尾是 exec，pid 不变），只让 CC
# 把 `sessions/<pid>.json` 里的 sessionId 换掉。这里就照这个形态复现：**进程不动，
# 只原地改 sessionId**。
#
# 为什么必须有这条 e2e、单测不够：单测直接调 `process_session_added`，绕过了 inotify。
# 而这个场景的整条链路是「CC 改文件 → inotify → 原地换 sid 分支 → 帧」，
# 中间任何一环断了单测都看不出来。
#
# 断言的核心是 **`cause`**：旧 sid 的 session_removed 必须带 `"cause":"superseded"`。
# monitor 靠它区分「死了（tmux 还在 ⇒ 灰点）」和「被顶替了（⇒ 直接归档）」。
# 缺了它，monitor 只能去查自己缓存的 tmux 快照——而那份快照对本场景**恒错**
#（旧 sid 的 tmux 格子还在，只是已改挂新 sid），且 P5 删掉 8s ticker 之后
# `/branch` 不触发任何事件路径去刷新它 ⇒ 永久灰点。
NEWSID="11111111-2222-3333-4444-555555555555"
TAG_SID="$SID"
PIDFILE="$(ls "$CLAUDE_DIR"/sessions/*.json 2>/dev/null | head -1 || true)"
if [ -z "$PIDFILE" ]; then
  bad "S0：找不到 fake-claude 写的 pidfile，无法复现 /branch"
else
  MARK_BRANCH="$(wc -l <"$FRAMES")"
  echo "-- S0：原地把 $PIDFILE 的 sessionId 从 $SID 改成 $NEWSID（进程不动）--"
  # 只换 sessionId，**其余字段（pid / procStart / kind）原样保留** —— 动了它们会撞上
  # daemon 的 add-time 冒名检查（F20），那时红的原因就不是本条要测的东西了。
  TMPJ="$(mktemp)"; sed "s/$SID/$NEWSID/g" "$PIDFILE" >"$TMPJ" && mv "$TMPJ" "$PIDFILE"

  SR_SUP="$(wait_line "$MARK_BRANCH" "\"kind\":\"session_removed\".*$SID" 12 'session_removed(old sid)' || true)"
  if [ -z "$SR_SUP" ]; then
    bad "S0：原地换 sid 后 12s 内没有旧 sid 的 session_removed 帧"
  else
    ok "S0：旧 sid 收到 session_removed:$(printf '%.140s' "$SR_SUP")"
    if printf '%s' "$SR_SUP" | grep -q '"cause":"superseded"'; then
      ok "S0：该帧带 cause=superseded ⇒ monitor 会直接归档，不会判成永久灰点"
    else
      bad "S0：该帧**缺** cause=superseded ⇒ monitor 会去查陈旧 tmux 快照、把它判成灰点（正是用户报的 bug）:$SR_SUP"
    fi
  fi
  # 新 sid 必须作为一个新会话宣告出来（否则 /branch 之后用户就没有 tab 了）。
  SA_NEW="$(wait_line "$MARK_BRANCH" "\"kind\":\"session_added\".*$NEWSID" 12 'session_added(new sid)' || true)"
  [ -n "$SA_NEW" ] \
    && ok "S0：新 sid 已宣告为新会话:$(printf '%.140s' "$SA_NEW")" \
    || bad "S0：原地换 sid 后 12s 内未见新 sid 的 session_added"
  # 对照组：**真死**那条路不能被误标成 superseded（下面第 2 节杀进程时验，见那里）。
  #
  # 后续各节针对当前活着的那个 sid；但 **tmux 上那个 `@ccm_sid` 标签仍是老的**，
  # 所以分成两个变量。**这与真机的差异要如实记**：真机上 `shared/ccm` 有个 1 秒 poller
  # 会把标签改成新 sid（`shared/ccm:612` 注释自陈就是为了「随 /branch 漂移」），
  # 本 fixture 没有那个 poller。这个差异**不影响本节要测的东西**——`cause` 完全由
  # daemon 从 pidfile 视角判定，与标签无关；恰恰是「不依赖标签」才是 S0 的修法要点。
  TAG_SID="$SID"   # tmux `@ccm_sid` 上挂着的（本 fixture 里恒为最初那个）
  SID="$NEWSID"    # 当前活着的会话 sid
fi

# ── 2. GRAY:杀 fake-claude(留 tmux 会话)→ SessionRemoved + tmux 帧仍含 @ccm_sid ──
FAKE_PID="$(awk -F'[:,]' '{for(i=1;i<=NF;i++) if($i ~ /"pid"/){print $(i+1); exit}}' "$CLAUDE_DIR"/sessions/*.json)"
echo "-- kill fake-claude pid=$FAKE_PID(claude 退出,tmux 会话保留)--"
kill "$FAKE_PID" 2>/dev/null || true
FAKE_PID=""  # 已杀,cleanup 不再重复

MARK_KILL="$(wc -l <"$FRAMES")"
# 起始行用 0 而非 MARK_KILL：MARK_KILL 是在 kill **之后**取的，帧可能已经先落盘了。
# 此刻 $SID 已是 1bis 换上的新 sid，全文只会有它这一条 session_removed，不会串。
SR="$(wait_line 0 "\"kind\":\"session_removed\".*$SID" 12 'session_removed' || true)"
if [ -z "$SR" ]; then
  bad "12s 内未见 SessionRemoved($SID)"
else
  ok "SessionRemoved(claude 死):$SR"
  # ★ S0 对照组：**真死绝不能带 superseded** —— 带了就等于把「claude 死了但 tmux 还在」
  # 这个灰点功能整个砸掉（用户会看到会话凭空归档、回不去）。
  if printf '%s' "$SR" | grep -q '"cause"'; then
    bad "S0 对照：真死的帧不该带任何 cause（Gone 按 additive 约定不上线）:$SR"
  else
    ok "S0 对照：真死的帧不带 cause ⇒ monitor 按 Gone 处理 ⇒ 灰点功能不受影响"
  fi
fi

# claude 死后，**monitor 手上最新的那份 tmux 快照**必须仍含 @ccm_sid
#（claude 死但 tmux 未亡 = 灰灯的后端条件）。
#
# ★ P5 之后这条断言的形态必须变，否则它测的就不是灰灯条件了：
# 原来写的是「等一个 **新** tmux 帧」。那能过，是因为当时有 8s ticker 每隔一阵就重发一份
# 快照。**P5 把 ticker 删了**（判活改成纯事件驱动），而「杀掉会话里的 claude 进程」**不动
# tmux 会话本身** ⇒ 不触发任何 hook ⇒ **本来就不该有新帧**。等新帧会一直等到超时。
#
# 灰灯真正依赖的是「**最新已知**快照里还有这个 sid」—— monitor 侧本来就是拿缓存的那份判的
#（`tmux_raw_registry` 只存最新一份）。所以改成读**最后一条** tmux 帧，语义与原意一致、
# 且不再依赖一个已经被有意删掉的节拍。
#
# **这条是 P5 留下的真回归，被 P6 的 e2e 工作撞出来的**：P5 那轮只跑了 cargo/npm 门禁，
# 而这 6 套是 CI-only、不在其中 ⇒ 没接住。教训已记进 P6 文档。
TS_GRAY="$(grep '"kind":"tmux_sessions"' "$FRAMES" | tail -1 || true)"
if [ -n "$TS_GRAY" ]; then
  if printf '%s' "$TS_GRAY" | grep -q "$TAG_SID"; then
    ok "claude 死后最新 tmux 快照仍含 @ccm_sid ⇒ 灰(Idle 非 Archive):$(printf '%.160s' "$TS_GRAY")"
  else
    bad "claude 死后最新 tmux 快照丢了 @ccm_sid(不该):$TS_GRAY"
  fi
else
  bad "至今一条 tmux_sessions 帧都没有（连起飞初探那拍都没到？）"
fi

# ── 3. ARCHIVE:tmux kill-session → 新 tmux 帧不再含 sid ───────────────────────
echo "-- tmux kill-session $SESSION(@ccm_sid 消失 → 归档触发)--"
tmux kill-session -t "=$SESSION:" 2>/dev/null || true
MARK_KS="$(wc -l <"$FRAMES")"
TS_ARCH="$(wait_line "$MARK_KS" "\"kind\":\"tmux_sessions\"" 14 'tmux frame post-kill-session')"
if [ -n "$TS_ARCH" ]; then
  if printf '%s' "$TS_ARCH" | grep -q "$TAG_SID"; then
    bad "kill-session 后 tmux 帧仍含 sid(不该):$TS_ARCH"
  else
    ok "kill-session 后 tmux 帧不再含 @ccm_sid ⇒ 归档触发边沿:$(printf '%.160s' "$TS_ARCH")"
  fi
else
  bad "kill-session 后 14s 内无新 tmux_sessions 帧"
fi

# ── 4. P6：端到端延迟 —— 「多个会话里杀掉其中一个」必须是**事件驱动**的 ────────────
#
# 这是**唯一**没有内核事件源的场景：server 还活着、socket 还在，pidfd 与 inotify 都不响。
# P4 用 tmux hook → `--tmux-notify` → SIGUSR1 补上了它，P5 据此删掉了 8s 轮询。
# **删了轮询之后，这条路一旦坏掉，该场景就从「16s」直接变成「永不」** —— 那正是本断言要挡的。
#
# **阈值是数量级判据，不是性能指标。** 本机手工实测 126ms；这里给 5s 的宽松上限：
# CI runner 比开发机慢得多，把阈值卡在实测值上只会换来随机红。它要区分的是
# 「事件驱动（亚秒）」与「退回轮询（≥8s）／永不」，5s 足够把这三者分开。
LAT_CEIL_S=5

echo "-- P6：再起一个会话，杀掉其中一个，量到死亡帧的墙上时间 --"
OTHER="p6-other-$$"
tmux new-session -d -s "$OTHER" 2>/dev/null || true
if tmux has-session -t "=$OTHER:" 2>/dev/null; then
  # 等它进过一次快照再杀 —— daemon 的差分要有「上一份」才能算出消失
  #（第一次观测不报死亡，那是刻意的：否则 daemon 一启动就诬告一批）。
  MARK_SEEN="$(wc -l <"$FRAMES")"
  # **`|| true` 不能省**：本套件开了 `set -e`，`V="$(cmd)"` 里 cmd 失败会**直接中止脚本**
  # ⇒ 下面那句 `bad` 里精心写的诊断永远打不出来（本轮变异验收实测：脚本在这儿静默停住，
  # 只剩一个 rc=1）。让它返回空串，交给 `[ -z ]` 分支去报。
  SEEN="$(wait_line "$MARK_SEEN" "\"kind\":\"tmux_sessions\".*$OTHER" 10 'snapshot containing the new session' || true)"
  if [ -z "$SEEN" ]; then
    bad "P6：新会话 $OTHER 10s 内没进过任何 tmux_sessions 快照（差分无基线可比）"
  else
    ok "P6 前置：新会话已进快照（差分有基线）"
    MARK_P6="$(wc -l <"$FRAMES")"
    # **别用 `date +%s%3N`**：本机的 date 不认 `%3N`，会原样吐 9 位纳秒 ⇒ 算出来的
    # 「ms」是个天文数字。断言照样过，但 CI 日志里那个数会误导人（本轮实测踩到）。
    T0_NS="$(date +%s%N)"
    tmux kill-session -t "=$OTHER:" 2>/dev/null || true
    CLOSED="$(wait_line "$MARK_P6" "\"kind\":\"tmux_session_closed\"" "$LAT_CEIL_S" 'tmux_session_closed frame' || true)"
    T1_NS="$(date +%s%N)"
    ELAPSED=$(( (T1_NS - T0_NS) / 1000000 ))
    if [ -z "$CLOSED" ]; then
      bad "P6：杀掉多个会话中的一个后，${LAT_CEIL_S}s 内**没有**死亡帧 —— 事件通路坏了（hook 没装上？SIGUSR1 没接上？），而轮询已在 P5 删除 ⇒ 该场景现在是「永不」"
    else
      ok "P6：死亡帧 ${ELAPSED}ms（上限 ${LAT_CEIL_S}s；本机手工实测约 126ms）:$(printf '%.120s' "$CLOSED")"
      # 报的必须是**被杀的那个**，不是随便一个 —— 差分方向搞反 / 报全量都会在这里露馅。
      if printf '%s' "$CLOSED" | grep -q "$OTHER"; then
        ok "P6：死亡帧点名的是被杀的那个会话（$OTHER）"
      else
        bad "P6：死亡帧报的不是 $OTHER:$CLOSED"
      fi
    fi
  fi
else
  bad "P6：起不来第二个会话，无法验「多个中杀一个」这个场景"
fi

echo "== 结果:$pass 过 / $fail 败 =="
# G-C：与另外 8 套逐字一致的收尾格式，好让 `e2e/assert-pass-floor.sh` 用同一条正则抓。
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ]
