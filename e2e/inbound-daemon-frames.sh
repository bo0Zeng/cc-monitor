#!/usr/bin/env bash
# U8a-2a：**入方向（控制通道）的真进程端到端**。
#
# U6b-1/2/3 在 daemon 侧建了整条入方向，但**从来没有任何东西真的往它 stdin 写过一个字节**
# —— U8a-2 摸底实测确认过。所以那条通道此前只被单元测试喂过夹具。本套件是它第一次
# 在**真二进制 + 真管道**上跑：把 daemon 起起来，往它 stdin 写命令行，读 stdout 的应答帧。
#
# 覆盖的判据（每条都是 daemon 侧代码里写着、但此前没有真进程验证的）：
#   hello.commands 声明 · ping 往返 · id 逐字回显 · 未知命令 · 坏 JSON 不杀进程 ·
#   超长单行 · cancel 幂等 · 命令间不串台 · 关写端不杀进程 · stdin 开着时 SIGTERM 仍能收掉
#
# ★ 跨轨对拍：喂进去的那条 ping 行是 **monitor 的 `inbound_client::encode_request` 的产物**
#   （由 monitor 侧 `the_e2e_ping_line_is_exactly_what_the_encoder_produces` 逐字节钉住）。
#   否则这套件只证明「daemon 认得我手写的 JSON」，证明不了「monitor 发的那种 JSON」。
#
# 红线：daemon 零改动（只跑它）/ 不碰真 ~/.claude / **不碰用户真实的 tmux server**。
set -euo pipefail

# ── 隔离（照抄 graylight-daemon-frames.sh 的 G-C 做法，两件事缺一不可）────────────
# daemon 一起来就会去装 tmux hook + 挂 pidfd 看守。不隔离的话它会往**用户真实的
# tmux server** 上装三条 hook（实测踩过：hook 里编着一个马上就死的 pid，
# 之后每次建/关/改名会话都白起一个进程）。
#   ① unset TMUX —— 否则 $TMUX 会让客户端连外层那台 server 并**完全忽略** TMUX_TMPDIR。
#   ② TMUX_TMPDIR 必须是短路径 —— unix socket 路径上限 108 字节。
unset TMUX TMUX_PANE
TMUX_TMPDIR="$(mktemp -d /tmp/e2e-sock.XXXXXX)"; export TMUX_TMPDIR
_sock_cleanup() {
  set +e
  [ -n "${TMUX_TMPDIR:-}" ] && /usr/bin/tmux -S "$TMUX_TMPDIR/tmux-$(id -u)/default" kill-server 2>/dev/null
  [ -n "${TMUX_TMPDIR:-}" ] && rm -rf -- "$TMUX_TMPDIR"
}

E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$E2E_DIR/.." && pwd)"
DAEMON="${CCM_E2E_DAEMON:-$REPO/remote-daemon-proto/target/debug/cc-monitor-remote}"
WORK="$(mktemp -d /tmp/e2e-inbound.XXXXXX)"
CLAUDE_DIR="$WORK/claude"
IN="$WORK/in.fifo"
OUT="$WORK/out.jsonl"
ERR="$WORK/daemon.stderr"

[ -x "$DAEMON" ] || { echo "daemon 二进制不存在/不可执行：$DAEMON"; exit 1; }

pass=0; fail=0
ok()  { echo "  PASS $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }

cleanup() {
  set +e
  exec 3>&- 2>/dev/null
  [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
  rm -rf -- "$WORK"
}
trap 'cleanup; _sock_cleanup' EXIT

mkdir -p "$CLAUDE_DIR/projects"
mkfifo "$IN"

# ── ★ 跨轨对拍的那条行（monitor `encode_request` 的逐字节产物）───────────────────
# 改这里 ⇒ monitor 侧 `the_e2e_ping_line_is_exactly_what_the_encoder_produces` 变红。
INBOUND_PING_LINE='{"id":"e2e-ping-1","cmd":"ping","args":null}'

# ★ U8a-2c-1 跨轨对拍：**这一行是 monitor 的 `launch_args` + `encode_request` 的产物**
# （由 monitor 侧 `the_e2e_send_into_line_is_exactly_what_the_encoder_produces` 逐字节钉住）。
# 上面那条 ping 只证明「daemon 认得 monitor 编的信封」；这一条证明的是
# **monitor 真正会发的那条业务命令**（`daemon_send_into` 唯一会说的 `send-into`）。
# 值全是写死的（不插值），否则钉不住。
INBOUND_SEND_INTO_LINE='{"id":"e2e-si-1","cmd":"launch","args":{"mode":"send-into","name":"e2e-si-fixed","payload":"true"}}'

echo "== U8a-2a 入方向真进程端到端 =="
echo "daemon: $DAEMON"

CLAUDE_CONFIG_DIR="$CLAUDE_DIR" "$DAEMON" --tail-only <"$IN" >"$OUT" 2>"$ERR" &
DAEMON_PID=$!
# 把 fifo 的写端**持有住**：不这样的话第一个写者退出即 EOF，daemon 的入方向 reader 当场寿终，
# 后面每一条命令都发不进去（进程还活着，但再也不理你）。
exec 3>"$IN"

# 等某个 grep 模式出现在 stdout 里（最多 ~10s）。返回 0 = 出现了。
wait_for() {
  local pattern="$1" i
  for i in $(seq 1 200); do
    grep -qF -- "$pattern" "$OUT" && return 0
    sleep 0.05
  done
  return 1
}
send() { printf '%s\n' "$1" >&3; }

# ── 1. hello 先到，且声明了入方向命令集（U6b-2）─────────────────────────────────
if wait_for '"kind":"hello"'; then ok "daemon 发出 hello"; else
  bad "10s 内没等到 hello"; echo "--- stderr ---"; tail -20 "$ERR"
fi
HELLO="$(head -1 "$OUT")"
for c in ping cancel resolve launch; do
  if printf '%s' "$HELLO" | grep -qF "\"$c\""; then ok "hello.commands 声明了 $c"; else
    bad "hello 里没有 $c：$HELLO"
  fi
done

# ── 2. ping 往返 —— 本套件的核心判据 ────────────────────────────────────────────
send "$INBOUND_PING_LINE"
if wait_for '"kind":"reply"'; then
  # ⚠ **选择谓词与断言谓词必须不同**。第一版两边都用 `grep '"id":"e2e-ping-1"'` ——
  #   先用它选出 REPLY 再断言 REPLY 含它，那条断言在结构上不可能失败（D 审计点名，
  #   地板 15 里有 1 分是白送的）。现在按 kind 选、按 id 断言。
  REPLY="$(grep -F '"kind":"reply"' "$OUT" | head -1)"
  if printf '%s' "$REPLY" | grep -qF '"ok":true'; then
    ok "ping 收到 {\"kind\":\"reply\",\"ok\":true}"
  else
    bad "应答形状不对：$REPLY"
  fi
  # id 是**不透明串、原样回显** —— daemon 不许解析/改写它。
  if printf '%s' "$REPLY" | grep -qF '"id":"e2e-ping-1"'; then
    ok "id 逐字回显"
  else
    bad "id 被改写了（应为 e2e-ping-1）：$REPLY"
  fi
else
  bad "5s 内没等到 ping 的应答"; echo "--- stdout ---"; cat "$OUT"; echo "--- stderr ---"; tail -20 "$ERR"
fi

# ── 3. 未知命令 → 结构化错误，**不是**沉默也不是崩 ──────────────────────────────
send '{"id":"e2e-unknown-1","cmd":"definitely-not-a-command","args":{}}'
if wait_for '"id":"e2e-unknown-1"'; then
  R="$(grep -F '"id":"e2e-unknown-1"' "$OUT" | head -1)"
  if printf '%s' "$R" | grep -qF '"ok":false' && printf '%s' "$R" | grep -qF 'unknown_command'; then
    ok "未知命令回 unknown_command"
  else
    bad "未知命令的应答不对：$R"
  fi
else
  bad "未知命令没有应答"
fi

# ── 4. 坏 JSON：回 bad_request，且**绝不结束读循环** ────────────────────────────
send '{this is not json'
if wait_for 'bad_request'; then ok "坏 JSON 回 bad_request"; else bad "坏 JSON 没有应答"; fi

# ── 5. 超长单行：回 line_too_long，进程存活（U6b-1 D 审计那条阻塞的回归钉）───────
#    1 MiB 上限 → 喂 2 MiB。
python3 -c "import sys; sys.stdout.write('{\"id\":\"x\",\"cmd\":\"ping\",\"args\":{\"pad\":\"' + 'A'*2097152 + '\"}}')" >&3
printf '\n' >&3
if wait_for 'line_too_long'; then ok "超长单行回 line_too_long"; else bad "超长单行没有应答"; fi

# ── 6. 上面几发之后进程必须还活着，且**还在正常应答**（不是只剩个僵尸）──────────
if kill -0 "$DAEMON_PID" 2>/dev/null; then ok "坏输入之后 daemon 仍存活"; else bad "daemon 被坏输入弄死了"; fi
send '{"id":"e2e-after-garbage","cmd":"ping","args":null}'
if wait_for '"id":"e2e-after-garbage"'; then ok "坏输入之后读循环仍在工作"; else bad "读循环停摆了"; fi

# ── 7. cancel 一个不存在的 id 是**幂等的**、不是错误 ────────────────────────────
send '{"id":"e2e-cancel-1","cmd":"cancel","args":{"target":"no-such-id"}}'
if wait_for '"id":"e2e-cancel-1"'; then
  R="$(grep -F '"id":"e2e-cancel-1"' "$OUT" | head -1)"
  if printf '%s' "$R" | grep -qF '"ok":true'; then ok "cancel 不存在的 id 幂等回 ok"; else
    bad "cancel 幂等性破了：$R"
  fi
else
  bad "cancel 没有应答"
fi

# ── 8. 命令不串台：连发三条，三条 id 各自拿到自己的应答 ─────────────────────────
send '{"id":"e2e-multi-a","cmd":"ping","args":null}'
send '{"id":"e2e-multi-b","cmd":"ping","args":null}'
send '{"id":"e2e-multi-c","cmd":"ping","args":null}'
missing=""
for k in a b c; do
  wait_for "\"id\":\"e2e-multi-$k\"" || missing="$missing $k"
done
if [ -z "$missing" ]; then ok "三条并发命令各自拿到应答"; else bad "这几条没应答：$missing"; fi

# ── 8b. U8a-2b：`launch` 打在**真 tmux** 上 ─────────────────────────────────────
#
# 这一段是 2b 的关键证据：不是喂夹具，是让真 daemon 用真 tmux 建出一个真会话、
# 把载荷真敲进去。tmux server 是本套件开头隔离出来的私有 socket（`TMUX_TMPDIR`），
# **绝不碰用户真实的 server**。
if ! command -v tmux >/dev/null 2>&1; then
  bad "没有 tmux —— launch 段无法验证（本套件依赖真 tmux，不接受跳过）"
else
  SESS="e2e-launch-$$"
  MARK="$WORK/launched.marker"
  # 载荷：写一个 marker 文件。它比「看进程名」可靠得多 —— 能证明**这一行真的被执行了**。
  send "{\"id\":\"e2e-launch-1\",\"cmd\":\"launch\",\"args\":{\"mode\":\"create-or-attach\",\"name\":\"$SESS\",\"payload\":\"touch '$MARK'\",\"ccm_sid\":\"e2e-sid-1\"}}"
  if wait_for '"id":"e2e-launch-1"'; then
    R="$(grep -F '"id":"e2e-launch-1"' "$OUT" | head -1)"
    if printf '%s' "$R" | grep -qF '"ok":true' &&
       printf '%s' "$R" | grep -qF '"created":true' &&
       printf '%s' "$R" | grep -qF '"typed":true'; then
      ok "launch create-or-attach 回 created=true typed=true"
    else
      bad "launch 应答不对：$R"
    fi
  else
    bad "launch 没有应答"
  fi
  if tmux has-session -t "=$SESS:" 2>/dev/null; then ok "真 tmux 会话建出来了"; else
    bad "tmux 里找不到会话 $SESS"
  fi
  GOT_SID="$(tmux show-options -v -t "=$SESS:" @ccm_sid 2>/dev/null || true)"
  if [ "$GOT_SID" = "e2e-sid-1" ]; then ok "@ccm_sid 已设（$GOT_SID）"; else
    bad "@ccm_sid 不对：${GOT_SID:-<空>}"
  fi
  got_marker=0
  for _ in $(seq 1 60); do [ -f "$MARK" ] && { got_marker=1; break; }; sleep 0.05; done
  if [ "$got_marker" = 1 ]; then ok "载荷真的在会话里执行了（marker 落盘）"; else
    bad "3s 内 marker 没出现 —— send-keys 没真落进交互 shell"
  fi

  # 幂等：同名再来一次 ⇒ created=false typed=false（**不重复 resume**）
  rm -f "$MARK"
  send "{\"id\":\"e2e-launch-2\",\"cmd\":\"launch\",\"args\":{\"mode\":\"create-or-attach\",\"name\":\"$SESS\",\"payload\":\"touch '$MARK'\"}}"
  if wait_for '"id":"e2e-launch-2"'; then
    R="$(grep -F '"id":"e2e-launch-2"' "$OUT" | head -1)"
    if printf '%s' "$R" | grep -qF '"created":false' && printf '%s' "$R" | grep -qF '"typed":false'; then
      ok "会话已存在 ⇒ 幂等短路（created=false typed=false）"
    else
      bad "幂等短路破了：$R"
    fi
  else
    bad "第二次 launch 没有应答"
  fi
  sleep 0.4
  if [ ! -f "$MARK" ]; then ok "幂等短路真的没重复键入载荷"; else
    bad "幂等短路声称没键入，marker 却又出现了"
  fi

  # send-into：往**已存在**的会话键入 ⇒ typed=true
  send "{\"id\":\"e2e-launch-3\",\"cmd\":\"launch\",\"args\":{\"mode\":\"send-into\",\"name\":\"$SESS\",\"payload\":\"touch '$MARK'\"}}"
  if wait_for '"id":"e2e-launch-3"'; then
    R="$(grep -F '"id":"e2e-launch-3"' "$OUT" | head -1)"
    if printf '%s' "$R" | grep -qF '"ok":true' && printf '%s' "$R" | grep -qF '"typed":true'; then
      ok "send-into 往已存在会话键入成功"
    else
      bad "send-into 应答不对：$R"
    fi
  else
    bad "send-into 没有应答"
  fi
  got_marker=0
  for _ in $(seq 1 60); do [ -f "$MARK" ] && { got_marker=1; break; }; sleep 0.05; done
  if [ "$got_marker" = 1 ]; then ok "send-into 的载荷真的执行了"; else bad "send-into 没真敲进去"; fi

  # ★ #76 防线（形态迁移后）：send-into 一个**不存在**的会话 ⇒ 报错，**绝不新建**
  send '{"id":"e2e-launch-4","cmd":"launch","args":{"mode":"send-into","name":"e2e-nope-never","payload":"true"}}'
  if wait_for '"id":"e2e-launch-4"'; then
    R="$(grep -F '"id":"e2e-launch-4"' "$OUT" | head -1)"
    if printf '%s' "$R" | grep -qF '"ok":false' && printf '%s' "$R" | grep -qF 'no_such_session'; then
      ok "send-into 会话不存在 ⇒ no_such_session"
    else
      bad "应当回 no_such_session：$R"
    fi
  else
    bad "send-into(不存在) 没有应答"
  fi
  if tmux has-session -t "=e2e-nope-never:" 2>/dev/null; then
    bad "**send-into 顺手把会话建出来了** —— 那是 #76 的反向"
  else
    ok "send-into 没有顺手新建会话（#76 反向防线）"
  fi

  # ★ U8a-2c-1：**monitor 真正会发的那条 send-into**（逐字节由 monitor 侧钉住）打在真 tmux 上。
  # 与上面 e2e-launch-3 的区别：那条是本脚本手写+插值的，这条是**编码器的产物**——
  # 手写那条只证明 daemon 认得我写的形状，证明不了 monitor 发的形状。
  tmux new-session -d -s e2e-si-fixed
  send "$INBOUND_SEND_INTO_LINE"
  if wait_for '"id":"e2e-si-1"'; then
    R="$(grep -F '"id":"e2e-si-1"' "$OUT" | head -1)"
    if printf '%s' "$R" | grep -qF '"ok":true' && printf '%s' "$R" | grep -qF '"typed":true'; then
      ok "monitor 编码器产的 send-into 行被真 daemon 接受并键入"
    else
      bad "编码器产的 send-into 行被拒了：$R"
    fi
  else
    bad "编码器产的 send-into 行没有应答"
  fi

  # attach 不归 daemon（平面 ③）
  send '{"id":"e2e-launch-5","cmd":"launch","args":{"mode":"attach-only","name":"x","payload":"y"}}'
  if wait_for '"id":"e2e-launch-5"'; then
    R="$(grep -F '"id":"e2e-launch-5"' "$OUT" | head -1)"
    if printf '%s' "$R" | grep -qF 'invalid_args'; then ok "attach-only 被拒（平面 ③ 不归 daemon）"; else
      bad "attach-only 居然被接受了：$R"
    fi
  else
    bad "attach-only 没有应答"
  fi
  tmux kill-session -t "=$SESS:" 2>/dev/null || true
fi

# ── 9. 关掉写端**不许**把 daemon 弄死 ───────────────────────────────────────────
#
# ⚠ 这条判据的第一版写反了：写的是「关写端 ⇒ daemon 自退」，理由抄自 `ssh_source.rs`
#    `probe_daemon` 里那句「关掉写半边（daemon 看到 EOF 自行退出）」。**那句话是错的**，
#    本套件第一次跑就把它打红了。真实机制读 `main.rs` 的 select 就清楚：
#    进程只在 ① writer_task 结束（stdout 关了）或 ② 收到停机信号 时退出；
#    stdin EOF 只让**入方向 reader task**寿终（`inbound.rs` 那句「正常寿终」说的是 task）。
#    monitor 的 probe 之所以能收尾，靠的是**整条 SSH channel 被 drop**，不是 stdin EOF。
#
# 于是这条反过来钉：关写端之后 daemon 必须**还活着**。
# 长连接（`stream_loop`）上如果哪天有人让 stdin EOF 顺手杀进程，读面会当场一起死。
exec 3>&-
sleep 0.4
if kill -0 "$DAEMON_PID" 2>/dev/null; then ok "关写端后 daemon 仍存活（stdin EOF ≠ 进程退出）"; else
  bad "关写端把 daemon 弄死了 —— 长连接上这会把读面一起弄死"
fi
kill "$DAEMON_PID" 2>/dev/null || true
DAEMON_PID=""

# ── 10. SIGTERM 回归钉：**stdin 一直开着**时也必须立刻退 ────────────────────────
#
# 这是 U6b-1 的 D 审计抓到的那条阻塞的永久回归钉，形状照抄它：
# stdin 接一条开着但没数据的 FIFO，发 SIGTERM。回归时 5s 后仍不退（`tokio::io::stdin()`
# 走阻塞线程池，runtime drop 会等它），修法是 `std::process::exit(0)`。
# **爆炸半径正是生产形状** —— monitor 经 SSH exec 连着时 stdin 一直开着。
IN2="$WORK/in2.fifo"; mkfifo "$IN2"
CLAUDE_CONFIG_DIR="$CLAUDE_DIR" "$DAEMON" --tail-only <"$IN2" >"$WORK/out2.jsonl" 2>/dev/null &
P2=$!
exec 4>"$IN2"   # 持有写端 ⇒ daemon 的 stdin 一直开着、且没有数据
for _ in $(seq 1 100); do grep -q '"kind":"hello"' "$WORK/out2.jsonl" 2>/dev/null && break; sleep 0.05; done
kill -TERM "$P2" 2>/dev/null
gone=0
for _ in $(seq 1 40); do   # 2s 预算（回归时是 >5s 仍不退）
  kill -0 "$P2" 2>/dev/null || { gone=1; break; }
  sleep 0.05
done
exec 4>&-
if [ "$gone" = 1 ]; then ok "stdin 开着时 SIGTERM 仍能立刻收掉 daemon"; else
  bad "SIGTERM 回归复现：stdin 开着时 2s 内没退出"
  kill -9 "$P2" 2>/dev/null || true
fi

echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ]
