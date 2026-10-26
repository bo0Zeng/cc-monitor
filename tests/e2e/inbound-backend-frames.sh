#!/usr/bin/env bash
# U8a-2a：**入方向（控制通道）的真进程端到端**。
#
# U6b-1/2/3 在后端侧建了整条入方向，但**从来没有任何东西真的往它 stdin 写过一个字节**
# —— U8a-2 摸底实测确认过。所以那条通道此前只被单元测试喂过夹具。本套件是它第一次
# 在**真二进制 + 真管道**上跑：把后端起起来，往它 stdin 写命令行，读 stdout 的应答帧。
#
# 覆盖的判据（每条都是后端侧代码里写着、但此前没有真进程验证的）：
#   hello.commands 声明 · ping 往返 · id 逐字回显 · 未知命令 · 坏 JSON 不杀进程 ·
#   超长单行 · cancel 幂等 · 命令间不串台 · 关写端不杀进程 · stdin 开着时 SIGTERM 仍能收掉
#
# ★ 跨轨对拍：喂进去的那条 ping 行是 **monitor 的 `inbound_client::encode_request` 的产物**
#   （由 monitor 侧 `the_e2e_ping_line_is_exactly_what_the_encoder_produces` 逐字节钉住）。
#   否则这套件只证明「backend 认得我手写的 JSON」，证明不了「monitor 发的那种 JSON」。
#
# 红线：backend 零改动（只跑它）/ 不碰真 ~/.claude / **不碰用户真实的 tmux server**。
. "$(cd "$(dirname "$0")" && pwd)/sandbox-env.sh"  # 无条件清掉继承来的 CCM_* / CLAUDE* / ANTHROPIC_* / TMUX* / CC_BUS_*
set -euo pipefail

# ── 隔离（照抄 graylight-backend-frames.sh 的 G-C 做法，两件事缺一不可）────────────
# backend 一起来就会去装 tmux hook + 挂 pidfd 看守。不隔离的话它会往**用户真实的
# tmux server** 上装三条 hook（实测踩过：hook 里编着一个马上就死的 pid，
# 之后每次建/关/改名会话都白起一个进程）。
#   ① unset TMUX —— 否则 $TMUX 会让客户端连外层那台 server 并**完全忽略** TMUX_TMPDIR。
#   ② TMUX_TMPDIR 必须是短路径 —— unix socket 路径上限 108 字节。
# `C7i` 隔离：走**共享原语**（`P0e` 08-12 抽出来的，原本这段在各套件里各抄一份）。
# 它把 `$BIN/tmux` shim 放进 PATH 最前、强插 `-L <本趟私有名>` —— 漏什么环境变量都打不偏。
# ⚠ 本套件此前靠 `TMUX_TMPDIR` 隔离，那是 `C7i` 逐字禁止的形态
#   （08-11 一条同形态的探针把用户 **9 个真实会话**打没了）。
# shellcheck source=tests/e2e/tmux-shim.sh
. "$(cd "$(dirname "$0")" && pwd)/tmux-shim.sh" e2eInbound
_sock_cleanup() { tmux_shim_cleanup; }

E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$E2E_DIR/../.." && pwd)"
BACKEND="${CCM_E2E_BACKEND:-$REPO/.build/backend/debug/cc-monitor-backend}"
WORK="$(mktemp -d /tmp/e2e-inbound.XXXXXX)"
# 家目录换成本趟沙箱：后端会读 / 写的 `~/.cc-monitor/` 那一族（账号库 · 中转钥匙 · 凭据表 …）都跟着家走，
#   不许落到开发机的真家目录里。工具链先钉住（只读，与被测数据无关）。
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export HOME="$WORK/home"
mkdir -p "$HOME"
CLAUDE_DIR="$WORK/claude"
IN="$WORK/in.fifo"
OUT="$WORK/out.jsonl"
ERR="$WORK/backend.stderr"

[ -x "$BACKEND" ] || { echo "backend 二进制不存在/不可执行：$BACKEND"; exit 1; }

pass=0; fail=0
ok()  { echo "  PASS $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }

cleanup() {
  set +e
  exec 3>&- 2>/dev/null
  [ -n "${BACKEND_PID:-}" ] && kill "$BACKEND_PID" 2>/dev/null
  rm -rf -- "$WORK"
}
trap 'cleanup; _sock_cleanup' EXIT

mkdir -p "$CLAUDE_DIR/projects"
mkfifo "$IN"

# ── ★ 跨轨对拍的那条行（monitor `encode_request` 的逐字节产物）───────────────────
# 改这里 ⇒ monitor 侧 `the_e2e_ping_line_is_exactly_what_the_encoder_produces` 变红。
INBOUND_PING_LINE='{"id":"e2e-ping-1","cmd":"ping","args":null,"within_ms":10000}'

echo "== U8a-2a 入方向真进程端到端 =="
echo "backend: $BACKEND"

CLAUDE_CONFIG_DIR="$CLAUDE_DIR" "$BACKEND" -- --stream <"$IN" >"$OUT" 2>"$ERR" &
BACKEND_PID=$!
# 把 fifo 的写端**持有住**：不这样的话第一个写者退出即 EOF，backend 的入方向 reader 当场寿终，
# 后面每一条命令都发不进去（进程还活着，但再也不理你）。
exec 3>"$IN"

# 等某个 grep 模式出现在 stdout 里（上限是兜底的那一个数）。返回 0 = 出现了。
wait_for() {
  _wf_pat="$1"
  wall_until 'grep -qF -- "$_wf_pat" "$OUT"'
}
send() { printf '%s\n' "$1" >&3; }

# ── 1. hello 先到，且声明了入方向命令集（U6b-2）─────────────────────────────────
if wait_for '"kind":"hello"'; then ok "backend 发出 hello"; else
  bad "10s 内没等到 hello"; echo "--- stderr ---"; tail -20 "$ERR"
fi
HELLO="$(head -1 "$OUT")"
# F04a：新增 kill（第一条破坏性入方向命令）—— 这一行与后端命令表里的单词命令（`hello.commands` 从命令表派生）
# 由 monitor 侧 `the_e2e_command_list_matches_the_backend_command_table` 逐项钉住，两处要一起动。
# 新增 resync（手动对齐）。
for c in ping cancel kill resync; do
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
  # id 是**不透明串、原样回显** —— backend 不许解析/改写它。
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
if kill -0 "$BACKEND_PID" 2>/dev/null; then ok "坏输入之后后端仍存活"; else bad "backend 被坏输入弄死了"; fi
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

# ── 9. 关掉写端**不许**把后端弄死 ───────────────────────────────────────────
#
# ⚠ 这条判据的第一版写反了：写的是「关写端 ⇒ backend 自退」，理由抄自 `stream_source/`
#    `probe_backend` 里那句「关掉写半边（backend 看到 EOF 自行退出）」。**那句话是错的**，
#    本套件第一次跑就把它打红了。真实机制读 `main.rs` 的 select 就清楚：
#    进程只在 ① writer_task 结束（stdout 关了）或 ② 收到停机信号 时退出；
#    stdin EOF 只让**入方向 reader task**寿终（`stream/inbound/` 那句「正常寿终」说的是 task）。
#    monitor 的 probe 之所以能收尾，靠的是**整条 SSH channel 被 drop**，不是 stdin EOF。
#
# 于是这条反过来钉：关写端之后后端必须**还活着**。
# 长连接（`stream_loop`）上如果哪天有人让 stdin EOF 顺手杀进程，读面会当场一起死。
# 等后端说它读到 EOF、决定照开（stderr 里 `stdin-eof-kept` 那一行）之后再看活不活。
exec 3>&-
wall_until 'grep -qF stdin-eof-kept "$ERR"' || { bad "关写端之后没等到后端说「读到 EOF、照开」"; tail -20 "$ERR"; }
if kill -0 "$BACKEND_PID" 2>/dev/null; then ok "关写端后后端仍存活（stdin EOF ≠ 进程退出）"; else
  bad "关写端把后端弄死了 —— 长连接上这会把读面一起弄死"
fi
kill "$BACKEND_PID" 2>/dev/null || true
BACKEND_PID=""

# ── 10. SIGTERM 回归钉：**stdin 一直开着**时也必须立刻退 ────────────────────────
#
# 这是 U6b-1 的 D 审计抓到的那条阻塞的永久回归钉，形状照抄它：
# stdin 接一条开着但没数据的 FIFO，发 SIGTERM。回归时 5s 后仍不退（`tokio::io::stdin()`
# 走阻塞线程池，runtime drop 会等它），修法是 `std::process::exit(0)`。
# **爆炸半径正是生产形状** —— monitor 经 SSH exec 连着时 stdin 一直开着。
IN2="$WORK/in2.fifo"; mkfifo "$IN2"
CLAUDE_CONFIG_DIR="$CLAUDE_DIR" "$BACKEND" -- --stream <"$IN2" >"$WORK/out2.jsonl" 2>/dev/null &
P2=$!
exec 4>"$IN2"   # 持有写端 ⇒ backend 的 stdin 一直开着、且没有数据
wall_until "grep -q '\"kind\":\"hello\"' \"\$WORK/out2.jsonl\" 2>/dev/null" || true
kill -TERM "$P2" 2>/dev/null
gone=0
# 回归时是 stdin 不关就一直不退 ⇒ 等满兜底上限还在就是红；不按墙钟数几秒。
wall_until '! kill -0 "$P2" 2>/dev/null' && gone=1
exec 4>&-
if [ "$gone" = 1 ]; then ok "stdin 开着时 SIGTERM 仍能立刻收掉 backend"; else
  bad "SIGTERM 回归复现：stdin 开着时到兜底上限还没退出"
  kill -9 "$P2" 2>/dev/null || true
fi

echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ]
