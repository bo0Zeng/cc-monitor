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
# 现在的形态：把 `$BIN/tmux` 放进 PATH 最前，它 `exec` 真 tmux 并**强插 `-L e2eGrayFrames`**。
# · 漏什么环境变量都打不偏 —— 选择器写死在 shim 里，不依赖「记得清某个变量」；
# · 零调用点改动的好处**原样保留**：套件里的裸 `tmux` 一个不用改，
#   连它 shell out 出去的东西（`ccm` / `cc-spawn` 内部也裸调 tmux）也一并覆盖；
# · `unset TMUX` **仍然保留**，但它现在只是「让被测行为发生」（tmux 内会退化成就地起），
#   **不再是隔离手段** —— 隔离由 shim 独自负责。
unset TMUX TMUX_PANE
_GC_SOCK="e2eGrayFrames"
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
# S0 **之前**那个 sid。留着它只有一个用途：给下面那格「标签漂了没」当**反向判据**
#（证「老的已经不在快照里了」）。**它不是后面各节的比对目标** —— 拿它当比对目标正是
# 本夹具 08-12～09-09 之间错在的地方，见 §1bis 末尾那块碑。
OLD_SID="$SID"
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
  # ★★★ 〔09-09〕**这里原来立着一条今天两头都不成立的假设，留碑，别改回去。**
  #
  # 原文逐字是：「真机上 `shared/ccm` 有个 1 秒 poller 会把标签改成新 sid
  #（`shared/ccm:612` 注释自陈就是为了「随 /branch 漂移」），**本 fixture 没有那个 poller**」
  # ⇒ 据此把 `TAG_SID` 钉死在最初那个 sid 上，第 2、3 两节都拿它去 grep。**两头都塌了**：
  #
  #   · 那条 poller **08-14 被整条删掉** —— 提交 `0085d0d`（2026-08-14）标题逐字
  #     「U-NP④：ccm 的 1s 身份轮询整条删掉，`@ccm_sid` 改由 daemon 打」。今天 `shared/ccm`
  #     只写 `@ccm_sid_expect`（**意图**通道），`@ccm_sid`（**事实**通道）它一个字都不写；
  #     `shared/ccm` 里还留着一条 `the_identity_poller_is_gone_for_good` 守卫钉这件事。
  #   · 打标搬进了 **daemon 自己**，而**本夹具跑的就是那个 daemon**（上面 `$DAEMON`，
  #     PATH 上挂着 `-L e2eGrayFrames` 的 tmux shim ⇒ 它的 `tmux` 打的正是本套件这台 server）：
  #     `observe/watcher.rs::process_session_added` 在冒名检查之后调
  #     `crate::control::identity_tag::tag(pid, &sid)`；`tag()` 经 `/proc/<pid>/environ` 的
  #     `TMUX_PANE` 定位到会话（fake-claude 是 pane 里 `sh -c` 的子进程，`TMUX_PANE` 是继承来的、
  #     `exec sleep` 也不丢），比对现值不同就 `set-option -t <session_id> @ccm_sid <新 sid>`。
  #
  # ⇒ **S0 之后 `@ccm_sid` 真的会漂到新 sid。** 云端 run `34441405591` 的红帧逐字印证：
  #   `{"kind":"tmux_sessions","raw":"cc-<老 sid8>\t…\t11111111-2222-3333-4444-555555555555\n…"}`
  #   —— 会话名还是老 sid8（tmux 不会改名），最后一列（`#{@ccm_sid}`）已经是新 sid。
  #
  # **产品行为是对的**：`@ccm_sid` 是破坏性动作（kill）唯一认的事实，标签停在旧 sid 才会杀错
  # 会话。错的是夹具拿了旧 sid 那个变量去比。⇒ **后面两节断言的对象是「当前挂在 tmux 上的
  # 那个 sid」= `$SID`（下面这行已经换成新的）**，`OLD_SID` 只用来做反向判据。
  SID="$NEWSID"    # 当前活着的会话 sid ＝ tmux `@ccm_sid` 上现在挂着的那个

  # ── S0-tag：把「标签确实漂到了新 sid」这件事**正面钉住**〔09-09 新增〕──────────────
  #
  # 为什么必须单独立一格：第 2、3 两节都拿「tmux 上现在挂的是新 sid」当**前提**，而在此之前
  # **全套件没有任何一条断言碰过这个前提**。前提塌掉时两格的表现完全不同 ——
  #   · 第 2 格以**红**的形式露出来（09-09 云端就是这么红的，看得见）；
  #   · 第 3 格**以「恒真」的形式哑掉**（它拿完整老 UUID 去 grep，而会话名里只有前 8 位
  #     ⇒ 永远不命中 ⇒ 永远走 PASS 分支）——**看不见，且地板数挡不住**（条数没变，掉的是牙口）。
  # 把前提本身立成一格，两种形态就都堵住了；它同时是下面两格「能失败」的凭据来源。
  #
  # **这一格可以等新帧，第 2 格不可以 —— 两者不是一回事，别照抄**：
  # 「pidfile 绑的 sid 变了」是 daemon **承诺**要重探 tmux 的事件（`watcher.rs`：
  # `if state.sessions.get(&key).map(|e| e.sid.clone()) != sid_before { sid_drifted = true }`
  # → `run_tmux_probe()`），且 `identity_tag::tag()` 是 `process_session_added` 里**同步**跑完的
  # ⇒ 那次重探必然看见新标签。而第 2 格等不到新帧，是因为「杀 pane 里的 claude 进程」
  # 不生不死不改名 ⇒ 不触发任何 tmux hook ⇒ P5 删掉 ticker 之后本来就不该有新帧。
  #
  # 三岔的第二支（新 sid 在、老 sid 也在）挡的是 `identity_tag` 目标解析打偏那一族
  #（08-14 真事故：`display-message -t ''` 被静默解析成「某个会话」⇒ 标打错 ⇒ kill 杀错）。
  TS_TAG="$(wait_line "$MARK_BRANCH" "\"kind\":\"tmux_sessions\".*$NEWSID" 12 'tmux frame carrying the drifted tag' || true)"
  if [ -z "$TS_TAG" ]; then
    bad "S0 前提：换 sid 后 12s 内没有任何一帧 tmux_sessions 带新 sid ⇒ 「@ccm_sid」没漂到新 sid（打标没跑，或 sid 漂移没触发重探）。下面两节的前提不成立，它们的结论不可信"
  elif printf '%s' "$TS_TAG" | grep -q "$OLD_SID"; then
    bad "S0 前提：新 sid 出现了，但**老 sid 也还在同一份快照里** ⇒ 标签打到了别的会话上（identity_tag 目标解析打偏）⇒ kill 会杀错会话:$TS_TAG"
  else
    ok "S0 前提：「@ccm_sid」已从老 sid 漂到新 sid，老 sid 不在快照里了:$(printf '%.160s' "$TS_TAG")"
  fi
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
# ★ 找的针是 **`$SID`** —— S0 之后它已经是新 sid，而 tmux 上的 `@ccm_sid` 也已经跟着漂到新 sid
#（上一格「S0 前提」刚刚正面证过）。这正是灰灯要问的那个问题：**刚刚死掉的那个会话的 sid，
# 在 monitor 手上这份快照里还找不找得到**。找得到 ⇒ Idle（灰）；找不到 ⇒ Archive。
#
#〔09-09 修〕原来拿的是 `TAG_SID` ＝ S0 **之前**那个 sid，依据是 §1bis 里那条
# 「本 fixture 没有 ccm 的 1s poller 所以标签不动」的假设 —— 那条假设 08-14 起两头都不成立
#（poller 已删；打标搬进了本夹具正在跑的这个 daemon）。云端 run `34441405591` 就是被它拦红的，
# 而**红的是夹具、不是产品**：标签跟着 `/branch` 漂到新 sid 恰恰是对的，停在旧 sid 才会杀错会话。
TS_GRAY="$(grep '"kind":"tmux_sessions"' "$FRAMES" | tail -1 || true)"
if [ -n "$TS_GRAY" ]; then
  if printf '%s' "$TS_GRAY" | grep -q "$SID"; then
    ok "claude 死后最新 tmux 快照仍含当前 sid 的 @ccm_sid ⇒ 灰(Idle 非 Archive):$(printf '%.160s' "$TS_GRAY")"
  else
    bad "claude 死后最新 tmux 快照丢了当前 sid 的 @ccm_sid(不该):$TS_GRAY"
  fi
else
  bad "至今一条 tmux_sessions 帧都没有（连起飞初探那拍都没到？）"
fi

# ── 3. ARCHIVE:tmux kill-session → 新 tmux 帧不再含 sid ───────────────────────
echo "-- tmux kill-session $SESSION(@ccm_sid 消失 → 归档触发)--"
tmux kill-session -t "=$SESSION:" 2>/dev/null || true
MARK_KS="$(wc -l <"$FRAMES")"
TS_ARCH="$(wait_line "$MARK_KS" "\"kind\":\"tmux_sessions\"" 14 'tmux frame post-kill-session')"
# ★ 与上一格**同一根针**（`$SID`），这是刻意的：上一格证「它在」，这一格证「它没了」。
# 一根针被两格反向咬住 ⇒ 任何一格退化成恒真，另一格必红。这就是它们各自「能失败」的凭据。
#
# 🔴〔09-09 救活〕这一格此前和上一格一样拿的是 `TAG_SID` ＝ S0 **之前**那个完整 UUID。
# 它**没红过，但已经没牙了 —— 是恒真的**：S0 之后那串完整老 UUID 在任何一帧里都不会再出现
#（会话名是 `cc-<老 sid8>`，只含前 8 位；`@ccm_sid` 列已经是新 sid）⇒ `grep -q` 恒不命中
# ⇒ **恒走下面的 PASS 分支**。也就是说它本来要挡的形状（kill-session 之后快照里还赖着那一行）
# 今天一点都挡不住。**地板 12 看不见这件事**：条数没变，掉的是牙口 —— 恒真比红贵得多。
#
# 举证（产品若退化成「kill-session 之后那一行连同标签还留在快照里」，两种写法各判什么）：
#   拿云端 34441405591 那份真 raw 当退化后的帧：
#     raw = "cc-87503085\t…\t11111111-2222-3333-4444-555555555555\ncc-e2ekeep-22967\t…\t"
#   · 旧写法 `grep -q "$TAG_SID"`（TAG_SID ＝ 老完整 UUID，例如 87503085-….…）：
#     raw 里只有 `cc-87503085` 这个**前 8 位**，完整老 UUID 一次都不出现 ⇒ **不命中**
#     ⇒ 走 else ⇒ `ok "kill-session 后 tmux 帧不再含 @ccm_sid"` ⇒ **假 PASS，退化溜过去**。
#   · 新写法 `grep -q "$SID"`（SID ＝ 11111111-2222-3333-4444-555555555555）：
#     raw 第一行末列逐字就是它 ⇒ **命中** ⇒ 走 then ⇒ `bad` ⇒ **FAIL，退化被逮住**。
if [ -n "$TS_ARCH" ]; then
  if printf '%s' "$TS_ARCH" | grep -q "$SID"; then
    bad "kill-session 后 tmux 帧仍含当前 sid(不该):$TS_ARCH"
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
