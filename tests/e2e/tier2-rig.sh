#!/usr/bin/env bash
# tier-2 台架搭建器 —— 把 `tests/e2e/README.md` 的「全链套件怎么跑」那份**散文配方**
# 变成一条能跑的命令。
#
# 🔴 为什么要有它：那份配方逐字记着几个「踩过才知道」的前提，其中一条的代价是
# **后端会用真 `~/.claude` 起来**。让人照散文手搓，就是让下一个人再踩一次。
# 本仓自己的定框 E12 说得最清楚：**散文纪律等于没有纪律。**
#
# 两头的家都在沙箱里，真家目录下的文件一个都不碰：
#   · app（dev 实例）的家 = `$SBX/h`；
#   · 「远端」= 台架自己起的一台回环 sshd（本用户身份、随机端口、临时 host key 与客户端钥匙），
#     每个会话先过 `ForceCommand` 那一小段：`HOME` 换成 `$SBX/r`、清掉 tmux 环境、`cd` 进去再执行原命令
#     （sftp 子系统也经它，起始目录同是 `$SBX/r`）⇒ 远端后端的落点 `~/.cc-monitor/bin/ccm` 是 `$SBX/r` 底下那一份，
#     台架把 `backend-wrapper.sh` 放在那里（旁边带 `backend-path` 与 `tmux-sock`）。
#   · tmux 一律带 `-L`：远端后端那一台由 wrapper 钉（名字在 `tmux-sock`），app 那一台由台架放进 PATH 最前的垫片钉。
#
# 用法：
#   bash tests/e2e/tier2-rig.sh setup     # 建沙箱 ＋ 起回环 sshd ＋ 写 config.json ＋ 起 Xvfb（自证：钉住的后端是这一版 · 远端的家在沙箱里）
#   bash tests/e2e/tier2-rig.sh dev       # 起 dev 实例（前台；或自己加 &）
#   bash tests/e2e/tier2-rig.sh run       # 跑 graylight-suite ＋ f40-suite
#   bash tests/e2e/tier2-rig.sh teardown  # 收 dev（整个进程组，vite 在里面）· 两头的常驻后端 · sshd · Xvfb · 两台私有 tmux（都按记下的 pid / 名字）
#
# 环境变量：SBX（沙箱根，默认本树 `.build/tier2`）· DISP（默认 :80）
set -euo pipefail

REPO="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
SBX="${SBX:-$REPO/.build/tier2}"
DISP="${DISP:-:80}"
RIG="$SBX/rig"
LHOME="$SBX/h"                       # app 的家
RHOME="$SBX/r"                       # 回环 sshd 那一头的家
LANDING="$RHOME/.cc-monitor/bin"     # 远端后端的落点目录（`relay_route_core::BACKEND_LANDING_SHELL` 那一份）
BACKEND="$REPO/.build/backend/debug/cc-monitor-backend"
# 真 HOME 只用来指回 rustup / cargo 的家（沙箱 HOME 会把它们一起换掉；那两个不是账号数据）。
REAL_HOME="$(getent passwd "$(id -un)" | cut -d: -f6)"
# 台架那份私有 tmux socket 名取自共享原语（setup 写进 `$RIG/tmux-sock`，wrapper 与套件都读它）。
# shellcheck source=tests/e2e/tmux-shim.sh
. "$REPO/tests/e2e/tmux-shim.sh" --names-only

die() { echo "❌ $*" >&2; exit 1; }

_build_id() {
  # 单一事实源 = 后端库面那个 const（`src/backend/lib.rs`；抠法同 `tests/scripts/bump-build-id.sh`），不抄字面量。
  local id
  id="$(sed -n 's/^pub const BUILD_ID: &str = "\([^"]*\)";$/\1/p' "$REPO/src/backend/lib.rs")"
  [ -n "$id" ] || die "抠不到后端的 BUILD_ID（src/backend/lib.rs）—— 改名了？"
  printf '%s' "$id"
}

setup() {
  [ -x "$BACKEND" ] || die "后端 debug 二进制不在：$BACKEND
   先跑：cd src/backend && cargo build"
  [ ! -e "$RIG/sshd.pid" ] || die "上一次的台架没收（$RIG/sshd.pid 还在）—— 先跑：bash $0 teardown"
  local bid said
  bid="$(_build_id)"
  # 台架自证 ①：钉住的那份后端就是这一版（旧的那份会让套件测的不是这棵树）。它自己答（`--backend-probe` 那一行的 `buildId`）。
  mkdir -p "$RIG/probe-home"
  said="$(env -i PATH=/usr/bin:/bin HOME="$RIG/probe-home" "$BACKEND" -- --backend-probe < /dev/null 2>/dev/null | head -1 \
    | sed -n 's/.*"buildId":"\([^"]*\)".*/\1/p')"
  rm -rf "$RIG/probe-home"
  [ "$said" = "$bid" ] || die "钉住的后端自报 build_id「$said」，源码是「$bid」—— 先重编：cd src/backend && cargo build"
  local sshd sftp_server p
  sshd="$(command -v sshd || echo /usr/sbin/sshd)"
  [ -x "$sshd" ] || die "没有 sshd"
  sftp_server=""
  for p in /usr/lib/openssh/sftp-server /usr/libexec/openssh/sftp-server /usr/libexec/sftp-server; do
    [ -x "$p" ] && { sftp_server="$p"; break; }
  done
  [ -n "$sftp_server" ] || die "没有 sftp-server"

  mkdir -p "$LHOME/.cc-monitor" "$LHOME/.claude/sessions" "$LANDING" "$RIG/xdg" "$RIG/bin"   # f40 把宿主 pidfile 落进 app 那一头的 sessions/
  chmod 700 "$LHOME/.cc-monitor" "$RHOME/.cc-monitor" "$RIG/xdg"   # 数据目录与后端的家同一个，只给本人
  cp "$REPO/tests/e2e/backend-wrapper.sh" "$LANDING/ccm"
  chmod +x "$LANDING/ccm"
  # wrapper 离开仓之后 `$REPO` 解析到别处 ⇒ 它会走「自愈」静默换二进制。旁边这个 `backend-path` 钉死用哪个（SSH exec 不带 env）。
  printf '%s' "$BACKEND" > "$LANDING/backend-path"
  # 后端经 SSH 起、读不到套件的 env ⇒ 私有 tmux socket 名也落成旁边一个文件；台架目录留一份给 `run` 交给套件。
  e2e_run_name e2eGray > "$LANDING/tmux-sock"
  cp "$LANDING/tmux-sock" "$RIG/tmux-sock"
  # fixture 的账号目录：fake-claude 只许写 /tmp 下（名字跟着本台架那台 tmux 走，几个台架不撞）。
  printf '/tmp/%s-claude' "$(cat "$RIG/tmux-sock")" > "$RIG/claude-dir"
  # app 那一头的 tmux 也钉一台私有的（垫片放进 dev 实例的 PATH 最前，强插 `-L`）。
  e2e_run_name e2eGrayApp > "$RIG/app-tmux-sock"
  printf '#!/bin/sh\nexec %s -L %s "$@"\n' "$(command -v tmux)" "$(cat "$RIG/app-tmux-sock")" > "$RIG/bin/tmux"
  chmod +x "$RIG/bin/tmux"

  local k
  for k in host_key client_key; do rm -f "$RIG/$k" "$RIG/$k.pub"; ssh-keygen -q -t ed25519 -N "" -f "$RIG/$k"; done
  cp "$RIG/client_key.pub" "$RIG/authorized_keys"; chmod 600 "$RIG/authorized_keys"
  rm -f "$RIG/known_hosts"
  local port
  port="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')"
  printf '%s' "$port" > "$RIG/port"
  # 每个会话的入口：家换进沙箱、tmux 环境清掉，再照原样执行（exec 的命令 · sftp 子系统都经这里）。
  cat > "$RIG/session.sh" <<F
#!/bin/sh
export HOME='$RHOME' CCM_SANDBOX=1 CCM_E2E_CLAUDE_DIR='$(cat "$RIG/claude-dir")' CCM_E2E_FRAME_TAP='$RIG/frames.tap' PATH=/usr/local/bin:/usr/bin:/bin LANG=C.UTF-8
unset TMUX TMUX_PANE TMUX_TMPDIR CLAUDE_CONFIG_DIR
cd "\$HOME" || exit 1
exec /bin/sh -c "\$SSH_ORIGINAL_COMMAND"
F
  chmod +x "$RIG/session.sh"
  cat > "$RIG/sshd_config" <<F
Port $port
ListenAddress 127.0.0.1
HostKey $RIG/host_key
AuthorizedKeysFile $RIG/authorized_keys
PidFile $RIG/sshd.own.pid
UsePAM no
StrictModes no
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
AllowTcpForwarding yes
LogLevel VERBOSE
MaxSessions 64
Subsystem sftp $sftp_server
ForceCommand $RIG/session.sh
F
  setsid "$sshd" -D -E "$RIG/sshd.log" -f "$RIG/sshd_config" < /dev/null > /dev/null 2>&1 &
  echo $! > "$RIG/sshd.pid"
  local _i
  for ((_i=0; _i<200; _i++)); do (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null && break; sleep 0.05; done
  # 台架自证 ②：那一头的家真在沙箱里（不是的话远端后端就落到真家目录那一份上）。
  local rh
  rh="$(ssh -F /dev/null -i "$RIG/client_key" -p "$port" -o BatchMode=yes -o IdentitiesOnly=yes \
        -o UserKnownHostsFile="$RIG/known_hosts" -o StrictHostKeyChecking=accept-new "$(id -un)@127.0.0.1" 'printf %s "$HOME"')" \
    || die "台架的回环 sshd 连接失败（看 $RIG/sshd.log）"
  [ "$rh" = "$RHOME" ] || die "回环 sshd 那一头的家是「$rh」，不是沙箱 $RHOME"

  local fp
  fp="$(ssh-keygen -lf "$RIG/host_key.pub" | awk '{print $2}')"
  python3 - "$LHOME" "$RIG" "$port" "$fp" <<'PY'
import json, os, sys
home, rig, port, fp = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
cfg = {
    "remote": {
        "enabled": True,
        "legacyNoBackend": [],
        "hosts": [{
            "label": "e2e-loopback",
            "host": "127.0.0.1",
            "port": port,
            "user": os.environ.get("USER") or os.getlogin(),
            "keyPath": f"{rig}/client_key",
            "hostKeyFingerprint": fp,
            "addresses": [],
            "jump": "",
            "resumeCommand": "",
        }],
    }
}
p = f"{home}/.cc-monitor/config.json"
with open(p, "w", encoding="utf-8") as f:
    json.dump(cfg, f, ensure_ascii=False, indent=2)
print(f"  写了 {p}")
PY

  # Xvfb —— 用 setsid 起、记 pid，收尾按 pid 收（不用 `pkill -f`：那个模式会匹配到执行它的那条 shell 自己的命令行）。
  if [ -S "/tmp/.X11-unix/X${DISP#:}" ]; then
    echo "  Xvfb $DISP 已在跑（不是本台架起的，收尾不收它）"
  else
    setsid /usr/bin/Xvfb "$DISP" -screen 0 1920x1080x24 >"$RIG/xvfb.log" 2>&1 < /dev/null &
    echo $! > "$RIG/xvfb.pid"
    for ((_i=0; _i<1200; _i++)); do [ -S "/tmp/.X11-unix/X${DISP#:}" ] && break; sleep 0.05; done
    [ -S "/tmp/.X11-unix/X${DISP#:}" ] || die "Xvfb $DISP 起不来，看 $RIG/xvfb.log"
    echo "  起了 Xvfb $DISP"
  fi

  echo "✅ 台架就绪：SBX=$SBX  DISPLAY=$DISP  build_id=$bid  sshd 127.0.0.1:$port"
  echo "   下一步：bash $0 dev    （另开一个终端/后台）"
}

dev() {
  [ -f "$LHOME/.cc-monitor/config.json" ] || die "先跑 setup"
  cd "$REPO"
  # 环境从空起（不带开发机会话的 TMUX · CLAUDE_CONFIG_DIR · 真端口那些）；RUSTUP_HOME / CARGO_HOME 显式指回真路径。
  # 自己一个进程组（setsid）、pid 记下：收尾整组收（vite 在组里）。
  setsid env -i PATH="$RIG/bin:/usr/local/bin:/usr/bin:/bin:$REAL_HOME/.cargo/bin" LANG=C.UTF-8 USER="$(id -un)" \
    DISPLAY="$DISP" HOME="$LHOME" CLAUDE_CONFIG_DIR="$LHOME/.claude" XDG_RUNTIME_DIR="$RIG/xdg" \
    CCM_SANDBOX=1 RUSTUP_HOME="$REAL_HOME/.rustup" CARGO_HOME="$REAL_HOME/.cargo" TMPDIR="${TMPDIR:-/tmp}" \
    CCM_NO_DEVTOOLS=1 npx tauri dev &
  echo $! > "$RIG/dev.pid"
  wait $!
}

run() {
  local log
  log="$(ls -t "$LHOME"/.cc-monitor/logs/monitor/monitor.*.log 2>/dev/null | head -1 || true)"
  [ -n "$log" ] || die "沙箱里没有 monitor 日志 —— dev 实例在跑吗？（bash $0 dev）"
  [ -s "$RIG/tmux-sock" ] || die "台架目录里没有 tmux-sock —— 先跑 setup"
  echo "  日志：$log"
  local rc=0 s
  for s in graylight-suite f40-suite; do
    printf '▼ %s\n' "$s"
    E2E_DISPLAY="$DISP" E2E_LOG="$log" HOME="$LHOME" CLAUDE_CONFIG_DIR="$LHOME/.claude" \
      CCM_E2E_CLAUDE_DIR="$(cat "$RIG/claude-dir")" CCM_E2E_TMUX_SOCK="$(cat "$RIG/tmux-sock")" \
      bash "$REPO/tests/e2e/$s.sh" || { rc=$?; echo "  rc=$rc"; }
  done
  return "$rc"
}

teardown() {
  set +e
  local f
  if [ -s "$RIG/dev.pid" ]; then
    kill -TERM -- "-$(cat "$RIG/dev.pid")" 2>/dev/null && echo "  收了 dev 进程组 $(cat "$RIG/dev.pid")"
    rm -f "$RIG/dev.pid"
  fi
  # 两头的常驻后端（不随 app 走）：各用自己那份 ccm、在自己的家里让它停。
  if [ -x "$LANDING/ccm" ]; then
    env -i PATH=/usr/bin:/bin HOME="$RHOME" CCM_SANDBOX=1 CCM_E2E_CLAUDE_DIR="$(cat "$RIG/claude-dir" 2>/dev/null)" \
      CCM_E2E_FRAME_TAP="$RIG/frames.tap" "$LANDING/ccm" -- --resident-stop > /dev/null 2>&1 && echo "  停了远端常驻"
  fi
  if [ -x "$LHOME/.cc-monitor/bin/ccm" ]; then
    env -i PATH="$RIG/bin:/usr/bin:/bin" HOME="$LHOME" XDG_RUNTIME_DIR="$RIG/xdg" CCM_SANDBOX=1 \
      "$LHOME/.cc-monitor/bin/ccm" -- --resident-stop > /dev/null 2>&1 && echo "  停了本机常驻"
  fi
  for f in sshd.pid xvfb.pid; do
    [ -s "$RIG/$f" ] && kill "$(cat "$RIG/$f")" 2>/dev/null && echo "  收了 ${f%.pid} $(cat "$RIG/$f")"
    rm -f "$RIG/$f"
  done
  local real
  real="$(command -v tmux)"
  for f in tmux-sock app-tmux-sock; do
    [ -s "$RIG/$f" ] && e2e_tmux_reap "$real" "$(cat "$RIG/$f")" && echo "  收了私有 tmux $(cat "$RIG/$f")"
  done
  # fixture 的账号目录（/tmp 下，名字跟着本台架那台 tmux）。
  [ -s "$RIG/claude-dir" ] && rm -rf -- "$(cat "$RIG/claude-dir")"
  echo "✅ 收尾完"
}

case "${1:-}" in
  setup) setup ;; dev) dev ;; run) run ;; teardown) teardown ;;
  *) echo "用法: $0 {setup|dev|run|teardown}"; exit 64 ;;
esac
