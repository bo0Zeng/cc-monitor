#!/usr/bin/env bash
# 全链灰灯套件（`graylight-suite.sh`）判台架判得对不对：台架不成立 ⇒ ABORT（exit 2），台架成立、主断言不满足才 FAIL（exit 1）。
#
# 为什么要它：#60 查了十几拍，「测不到」一路伪装成「测到了缺陷」——台架上任何一样没起来，套件都报「没变灰」FAIL。
#   真套件要 Xvfb 上跑着的 dev app，门禁里起不来 ⇒ 它的判台架那几格从来没人在无头环境里验过。
#
# 做法：搭一个照真东西形状的假世界，在里面跑**真套件**（拷一份进一棵假仓，套件按自己的位置找仓根）：
#   · monitor 日志：一份文件，尾行带新鲜时间戳、有一行 `backend hello`（套件甲 · 乙两格读的就是这两样）；
#   · 后端进程：`sleep` 拷成 `.build/backend/<x>/cc-monitor-backend`，环境里带本趟的 `CLAUDE_CONFIG_DIR`
#     （套件按 exe 名与进程环境认「盯着 fixture 的后端」）；
#   · devUrl 端口：假仓的 `tauri.conf.json` 写一个空闲端口，上面起一个静态 http 服务（套件丙那一格只问端口上有没有东西）；
#   · 后端 stderr：一份文件（台架里是 wrapper 抄出来的 `frames.tap.err`），经 `E2E_BACKEND_ERR` 交给套件；
#   · 假 app：轮询本趟私有 tmux 与 fixture 目录，照真 app 的样子往日志里写 tab 出现 · 可重连 · 已结束那几行。
# 四种情况：
#   ① 后端 stderr 有「文件监视挂不上」⇒ ABORT，不印 FAIL；
#   ② 假 app 不起（日志里一直没有这个会话的 tab）⇒ ABORT，不印 FAIL；
#   ③ 假 app 照序写全 ⇒ 套件全过（exit 0）——证明上面两条 ABORT 不是假世界本身不成立；
#   ④ 假 app 写了 tab、不写可重连 ⇒ 真 FAIL（exit 1）——台架成立之后的缺陷照旧报 FAIL。
# 沙箱：tmux 一律经私有 `-L`（套件自己的垫片 ＋ 本文件里每一条都带选择器）；HOME 换成本趟目录；不起后端、不起 claude。
. "$(cd "$(dirname "$0")" && pwd)/sandbox-env.sh"  # 无条件清掉继承来的 CCM_* / CLAUDE* / ANTHROPIC_* / TMUX* / CC_BUS_*
set -euo pipefail

E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
# sandbox-env 已在 PATH 最前装了一份垫片（钉它自己的名字）；本文件里的 tmux 一律用真 tmux ＋ 显式 `-L`，
#   套件那一趟的 PATH 摘掉那份垫片（套件自己装一份钉 `CCM_E2E_TMUX_SOCK` 的，两层叠着就是两个 `-L`）。
REAL_TMUX="${TMUX_SHIM_REAL:-}"
[ -x "$REAL_TMUX" ] || { echo "需要 tmux"; exit 1; }
SUITE_PATH="${PATH#"$TMUX_SHIM_BIN":}"
command -v python3 >/dev/null || { echo "需要 python3"; exit 1; }
command -v curl >/dev/null || { echo "需要 curl"; exit 1; }

T="$(mktemp -d /tmp/e2e-glverdict.XXXXXX)"
export HOME="$T/home"
mkdir -p "$HOME"
FAKE_REPO="$T/repo"
mkdir -p "$FAKE_REPO/tests/e2e" "$FAKE_REPO/tests/scripts" "$FAKE_REPO/src/frontend/shell"
for f in graylight-suite.sh assert.sh tmux-shim.sh gen-idle-tmux.sh fake-claude; do
  cp "$E2E_DIR/$f" "$FAKE_REPO/tests/e2e/$f"
done

. "$E2E_DIR/assert.sh"

HTTP_PID=""; BACKEND_PID=""; APP_PID=""
cleanup() {
  set +e
  for p in $APP_PID $BACKEND_PID $HTTP_PID; do kill "$p" 2>/dev/null; done
  wait 2>/dev/null
  rm -rf -- "$T" /tmp/e2e-glverdict-claude-$$-*
}
trap 'cleanup; tmux_shim_cleanup' EXIT

# devUrl 端口上的静态服务（套件丙那一格）。
PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')"
printf '{ "build": { "devUrl": "http://localhost:%s" } }\n' "$PORT" > "$FAKE_REPO/src/frontend/shell/tauri.conf.json"
mkdir -p "$T/www"
( cd "$T/www" && exec python3 -m http.server --bind 127.0.0.1 "$PORT" ) > "$T/http.log" 2>&1 &
HTTP_PID=$!
wall_until "curl -s -o /dev/null --max-time 1 http://127.0.0.1:$PORT"

# 跑一种情况。$1 = 名字 · $2 = 远端那份后端 stderr 里写什么 · $3 = 假 app 怎么演（none | full | no-gray）。
# 后端 stderr 照台架交两份（本机常驻 · 远端常驻，冒号隔开）：本机那份恒是干净的几行，$2 写进远端那份。
# 套件的退出码落进 $RC，输出落进 $OUT。
run_case() {
  local name="$1" err_text="$2" app="$3" claude_dir log errf errl sock
  claude_dir="/tmp/e2e-glverdict-claude-$$-$name"
  log="$T/$name.monitor.log"
  errl="$T/$name.local-backend.err"
  errf="$T/$name.remote-backend.err"
  sock="$(e2e_run_name "e2eGlv$name")"
  rm -rf -- "$claude_dir"; mkdir -p "$claude_dir"
  printf '%s INFO monitor: backend hello build_id=fake host_arch=x86_64\n' "$(date -u +%Y-%m-%dT%H:%M:%S.000Z)" > "$log"
  printf '2026-10-26T00:00:00.000000Z  INFO cc_monitor_backend: resident listening\n' > "$errl"
  printf '%s' "$err_text" > "$errf"

  # 按身份认得出的后端：exe 名对、环境里带本趟的 fixture 目录。
  mkdir -p "$T/.build/backend/fake"
  # 一份 bash 拷成那个名字、在一个 fifo 上一直读（不 fork 子进程，收的时候按 pid 收干净）。
  cp "$(command -v bash)" "$T/.build/backend/fake/cc-monitor-backend"
  [ -p "$T/hold" ] || mkfifo "$T/hold"
  env CLAUDE_CONFIG_DIR="$claude_dir" "$T/.build/backend/fake/cc-monitor-backend" -c 'read -r _ <> "$0"' "$T/hold" &
  BACKEND_PID=$!

  if [ "$app" != none ]; then
    fake_app "$sock" "$claude_dir" "$log" "$app" &
    APP_PID=$!
  fi

  set +e
  OUT="$(env -u TMUX -u TMUX_PANE HOME="$HOME" PATH="$SUITE_PATH" \
    E2E_LOG="$log" CCM_E2E_CLAUDE_DIR="$claude_dir" CCM_E2E_TMUX_SOCK="$sock" E2E_BACKEND_ERR="$errl:$errf" \
    E2E_TMUX_SETTLE=0 E2E_TAB_WAIT=4 E2E_GRAY_WAIT=4 E2E_ARCH_WAIT=4 \
    timeout "$WALL_CEILING_S" bash "$FAKE_REPO/tests/e2e/graylight-suite.sh" 2>&1)"
  RC=$?
  set -e
  if [ -n "$APP_PID" ]; then kill "$APP_PID" 2>/dev/null || true; wait "$APP_PID" 2>/dev/null || true; APP_PID=""; fi
  kill "$BACKEND_PID" 2>/dev/null || true; wait "$BACKEND_PID" 2>/dev/null || true; BACKEND_PID=""
  e2e_tmux_reap "$REAL_TMUX" "$sock" || true   # 套件收尾时已收过 ⇒ kill-server 回非 0
  rm -rf -- "$claude_dir"
  printf '%s\n' "$OUT" | sed 's/^/    | /'
}

# 假 app：照真 app 往日志里写。会话出现（tmux 上挂着 @ccm_sid 的那一个）⇒ tab 那一行；
#   fake-claude 进程没了 ⇒ 可重连（no-gray 那一演不写）；会话名没了 ⇒ 已结束。
fake_app() {
  local sock="$1" claude_dir="$2" log="$3" how="$4" sid="" sid8 pid="" stage=tab line pf
  while :; do
    case "$stage" in
      tab)
        for pf in "$claude_dir"/sessions/*.json; do
          [ -f "$pf" ] || continue
          sid="$(sed -n 's/.*"sessionId":"\([^"]*\)".*/\1/p' "$pf")"
          pid="$(sed -n 's/.*"pid":\([0-9]*\).*/\1/p' "$pf")"
        done
        if [ -n "$sid" ] && "$REAL_TMUX" -L "$sock" has-session -t "=cc-${sid:0:8}:" 2>/dev/null; then
          sid8="${sid:0:8}"
          printf '%s INFO monitor: [e2e] tab-state sid=%s dot=idle origin=e2e-loopback\n' "$(date -u +%Y-%m-%dT%H:%M:%S.000Z)" "$sid8" >> "$log"
          stage=gray
        fi ;;
      gray)
        if ! kill -0 "$pid" 2>/dev/null; then
          [ "$how" = no-gray ] && return 0
          printf '%s INFO monitor: [e2e] tab-state sid=%s dot=exited origin=e2e-loopback\n' "$(date -u +%Y-%m-%dT%H:%M:%S.000Z)" "$sid8" >> "$log"
          stage=ended
        fi ;;
      ended)
        if ! "$REAL_TMUX" -L "$sock" has-session -t "=cc-$sid8:" 2>/dev/null; then
          printf '%s INFO monitor: [e2e] tab-state sid=%s dot=ended origin=e2e-loopback\n' "$(date -u +%Y-%m-%dT%H:%M:%S.000Z)" "$sid8" >> "$log"
          return 0
        fi ;;
    esac
    sleep 0.1
  done
}

no_fail_lines() { ! printf '%s\n' "$OUT" | grep -q '^  FAIL '; }

echo "== 全链灰灯套件判台架 =="

echo "-- ① 后端 stderr：文件监视挂不上 --"
run_case watch "2026-10-26T00:00:00.000000Z ERROR cc_monitor_backend::observe::watcher: watch failed for /tmp/x/sessions: OS file watch limit reached. about [\"/tmp/x/sessions\"]
" full
{ [ "$RC" = 2 ] && printf '%s\n' "$OUT" | grep -q 'ABORT 后端的文件监视挂不上' && no_fail_lines; } \
  && ok "watch 挂不上 ⇒ ABORT（exit 2），不报 FAIL" \
  || bad "watch 挂不上：退出码 $RC，要 2 ＋ ABORT ＋ 没有 FAIL 行"

echo "-- ② app 一直没建出这个会话的 tab --"
run_case notab "" none
{ [ "$RC" = 2 ] && printf '%s\n' "$OUT" | grep -q 'ABORT .*没出现这个会话的 tab' && no_fail_lines; } \
  && ok "tab 等不到 ⇒ ABORT（exit 2），不报 FAIL" \
  || bad "tab 等不到：退出码 $RC，要 2 ＋ ABORT ＋ 没有 FAIL 行"

echo "-- ③ 假 app 照序写全 --"
run_case full "" full
{ [ "$RC" = 0 ] && printf '%s\n' "$OUT" | grep -q '合计 PASS=[1-9][0-9]* FAIL=0'; } \
  && ok "台架成立 ＋ 灰 · 归档都到 ⇒ 全过（exit 0）" \
  || bad "照序写全：退出码 $RC，要 0 ＋「合计 PASS=n FAIL=0」"

echo "-- ④ 假 app 不写可重连 --"
run_case nogray "" no-gray
{ [ "$RC" = 1 ] && printf '%s\n' "$OUT" | grep -q '^  FAIL ' && ! printf '%s\n' "$OUT" | grep -q 'ABORT'; } \
  && ok "台架成立、灰灯没到 ⇒ FAIL（exit 1），不是 ABORT" \
  || bad "不写可重连：退出码 $RC，要 1 ＋ FAIL 行 ＋ 没有 ABORT"

echo "-- ⑤ 台架的远端后端包装器：叫一次只起一趟后端 --"
# 照 tier2-rig setup 的布法：wrapper 拷进落点、旁边 backend-path · tmux-sock；后端换成一个记次数、印一行回答的替身。
W="$T/landing"; mkdir -p "$W"
cp "$E2E_DIR/backend-wrapper.sh" "$W/ccm"; chmod +x "$W/ccm"
cat > "$W/fake-backend" <<F
#!/bin/sh
echo "\$*" >> "$W/calls"
case "\$*" in *--stream*) echo '{"kind":"hello"}' ;; *) echo '{"pid":1,"replaced":false}' ;; esac
F
chmod +x "$W/fake-backend"
printf '%s' "$W/fake-backend" > "$W/backend-path"
e2e_run_name e2eGlvWrap > "$W/tmux-sock"
WOUT="$(env -i PATH=/usr/bin:/bin HOME="$T/home" CCM_E2E_CLAUDE_DIR="/tmp/e2e-glverdict-claude-$$-wrap" \
  CCM_E2E_FRAME_TAP="$T/wrap.tap" sh "$W/ccm" -- --resident-ensure)"
WCALLS="$(grep -c -- '--resident-ensure' "$W/calls" || true)"
{ [ "$WCALLS" = 1 ] && [ "$(printf '%s\n' "$WOUT" | grep -c .)" = 1 ]; } \
  && ok "wrapper 叫一次 ⇒ 后端只跑一趟、stdout 一行" \
  || bad "wrapper 叫一次：后端跑了 ${WCALLS} 趟，stdout $(printf '%s\n' "$WOUT" | grep -c .) 行（要 1 · 1）"

finish
