#!/usr/bin/env bash
# 手机冒烟台架：沙箱 sshd ＋ 沙箱后端 ＋ 替身 agent（不进门禁默认格，按需跑；怎么起见同目录 README.md）。
#
# 用法：bash tests/mobile-smoke/rig.sh up|down|env|tmux <tmux 参数…>
#   up    建台架目录（默认 <仓根>/.build/mobile-smoke/box，CCM_SMOKE_BOX 可改）、私有 host key 与登录钥匙、
#         把本树编好的后端放成 <台架家>/.cc-monitor/bin/ccm、起私有端口上的 sshd（默认 22422，CCM_SMOKE_PORT 可改）。
#   down  停 sshd、停台架里的常驻后端与 tmux 服务端（只认台架目录里的门牌与套接字）。
#   env   印出台架那几格（家目录 · tmux 套接字 · 端口 · 登录钥匙），给驱动脚本 source。
#   ssh   以台架的环境在台架里跑一条（和手机进来走同一个登录壳）—— 种历史会话、看台架里的状态用它。
#   tmux  在台架的 tmux 服务端上敲一条（显式 -S，清掉继承来的 TMUX 那几格）—— 桌面那一侧接着打字就用它。
#   land  none|<文件>：停台架里的常驻，落点删掉 / 换成那份（手机装后端 · 升级后端那两步用）。
#
# 隔离：ssh 进来的每条命令都由台架的登录壳（up 时写进台架目录的 sshd/login.sh）换上台架的 HOME · TMUX_TMPDIR · PATH（替身 claude / codex 在前）·
#   CCM_SANDBOX=1（后端要占真家目录的门牌就拒绝起）。sshd 只听 127.0.0.1（模拟器经它的网关、即宿主机够到它），
#   只认台架自己的登录钥匙。不碰本机正在跑的 sshd · 后端 · tmux。
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
box=${CCM_SMOKE_BOX:-$root/.build/mobile-smoke/box}
port=${CCM_SMOKE_PORT:-22422}
backend=${CCM_SMOKE_BACKEND:-$root/.build/backend/debug/cc-monitor-backend}
home=$box/home
sock=$box/tmux/tmux-$(id -u)/default

say() { printf 'rig: %s\n' "$*" >&2; }

up() {
  [ -x "$backend" ] || { say "没有编好的后端 $backend（先 cd src/backend && cargo build）"; exit 2; }
  mkdir -p "$box"/{sshd,bin,tmux,logs} "$home"/.cc-monitor/bin "$home"/.claude "$home"/.codex "$home"/work
  chmod 700 "$box/tmux"
  [ -f "$box/sshd/host_ed25519" ] || ssh-keygen -q -t ed25519 -N '' -f "$box/sshd/host_ed25519" -C smoke-host
  [ -f "$box/sshd/client_ed25519" ] || ssh-keygen -q -t ed25519 -N '' -f "$box/sshd/client_ed25519" -C smoke-client
  cp "$box/sshd/client_ed25519.pub" "$box/sshd/authorized_keys"
  # 换后端那份文件之前先停台架里的常驻（它跑的是旧文件；文件换掉之后它再起子进程会找不到自己）。
  if ! cmp -s "$backend" "$home/.cc-monitor/bin/ccm"; then stop_resident; install -m 755 "$backend" "$home/.cc-monitor/bin/ccm"; fi
  install -m 644 "$root/tests/mobile-smoke/fake-agent.py" "$box/bin/fake-agent.py"
  for k in claude codex; do
    printf '#!/bin/bash\nCCM_SMOKE_KIND=%s exec -a %s /usr/bin/python3 %q "$@"\n' "$k" "$k" "$box/bin/fake-agent.py" > "$box/bin/$k"
    chmod 755 "$box/bin/$k"
  done
  # 台架家里的 shell 起步文件：只放 PATH（替身在前）；登录 shell 与交互 shell 都认它。
  # 台架家里的 ~/.cc-monitor/bin（ccm 与别名的链接）也在 PATH 里：同装好别名的那台，终端里敲 `ccm @名` 能找到。
  printf 'export PATH=%q:%q:/usr/local/bin:/usr/bin:/bin\n' "$box/bin" "$home/.cc-monitor/bin" > "$home/.profile"
  cp "$home/.profile" "$home/.bashrc"
  printf '[ -f ~/.bashrc ] && . ~/.bashrc\n' > "$home/.bash_profile"
  cat > "$box/sshd/login.sh" <<EOF
#!/bin/bash
# sshd 的 ForceCommand：换上台架的环境再跑那条命令（没有命令 ⇒ 登录 shell）。
for k in \$(compgen -e); do case "\$k" in CCM_*|CLAUDE*|ANTHROPIC_*|TMUX*|CC_BUS_*|CODEX_*) unset "\$k";; esac; done
export HOME=$(printf %q "$home") TMUX_TMPDIR=$(printf %q "$box/tmux") CCM_SANDBOX=1 CCM_SMOKE_BOX=$(printf %q "$box")
export PATH=$(printf %q "$box/bin"):$(printf %q "$home/.cc-monitor/bin"):/usr/local/bin:/usr/bin:/bin SHELL=/bin/bash
# 台架后端的中转口用私有的（默认那个口本机正在跑的后端占着）。替身起到能收第一句要多久：外面给了就带进来。
export CCM_RELAY_PORT=${CCM_SMOKE_RELAY_PORT:-28788} CCM_SMOKE_BOOT=${CCM_SMOKE_BOOT:-2}
cd "\$HOME"
# 每条进来的命令记一行（冒烟看手机到底跑了什么）。
printf '%s %s\n' "\$(date +%T.%N | cut -c1-12)" "\${SSH_ORIGINAL_COMMAND:-<登录 shell>}" >> $(printf %q "$box/logs/commands.log")
case "\${SSH_ORIGINAL_COMMAND:-}" in
  '') exec -a -bash /bin/bash ;;
  internal-sftp|*sftp-server*) exec /usr/lib/openssh/sftp-server ;;
  *) exec /bin/bash -c "\$SSH_ORIGINAL_COMMAND" ;;
esac
EOF
  chmod 755 "$box/sshd/login.sh"
  cat > "$box/sshd/sshd_config" <<EOF
Port $port
ListenAddress 127.0.0.1
HostKey $box/sshd/host_ed25519
PidFile $box/sshd/sshd.pid
AuthorizedKeysFile $box/sshd/authorized_keys
AllowUsers $(id -un)
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
UsePAM no
StrictModes no
PermitUserEnvironment no
ForceCommand $box/sshd/login.sh
Subsystem sftp internal-sftp
LogLevel VERBOSE
EOF
  if [ -f "$box/sshd/sshd.pid" ] && kill -0 "$(cat "$box/sshd/sshd.pid")" 2>/dev/null; then say "sshd 已在跑"; else
    /usr/sbin/sshd -f "$box/sshd/sshd_config" -E "$box/logs/sshd.log"
    sleep 0.5; say "sshd 起在 127.0.0.1:$port（pid $(cat "$box/sshd/sshd.pid")）"; fi
  ssh -F /dev/null -i "$box/sshd/client_ed25519" -p "$port" -o StrictHostKeyChecking=no \
    -o UserKnownHostsFile="$box/sshd/known_hosts" "$(id -un)@127.0.0.1" \
    '"$HOME"/.cc-monitor/bin/ccm -- --backend-probe' > "$box/logs/probe.json"
  say "probe：$(head -c 200 "$box/logs/probe.json")"
}

# 台架家里的后端进程：只认 exe 是台架那份 ccm 的（文件换过的那种 exe 末尾带「 (deleted)」，也认）。
stop_resident() {
  local p e
  for p in $(pgrep -f "$home/.cc-monitor/bin/ccm" || true); do
    e=$(readlink /proc/"$p"/exe 2>/dev/null) || continue
    case "$e" in "$home/.cc-monitor/bin/ccm" | "$home/.cc-monitor/bin/ccm (deleted)") kill "$p" 2>/dev/null || true ;; esac
  done
}

down() {
  if [ -f "$box/sshd/sshd.pid" ]; then kill "$(cat "$box/sshd/sshd.pid")" 2>/dev/null || true; rm -f "$box/sshd/sshd.pid"; fi
  stop_resident
  [ -S "$sock" ] && env -u TMUX -u TMUX_PANE tmux -S "$sock" kill-server 2>/dev/null || true
  say "台架停了（目录留着：$box）"
}

# 落点换一份（装后端那两步用）：先停台架里的常驻，再把落点删掉（`none`）或换成给的那份文件。
land() {
  stop_resident
  case "${1:-}" in
    none) rm -f "$home/.cc-monitor/bin/ccm"; say "落点删了（台架那台没有后端）" ;;
    *) [ -f "$1" ] || { say "没有这份文件：$1"; exit 2; }; install -m 755 "$1" "$home/.cc-monitor/bin/ccm"; say "落点换成 $1" ;;
  esac
}

case "${1:-}" in
  up) up ;;
  down) down ;;
  land) shift; land "$@" ;;
  env) printf 'SMOKE_BOX=%q\nSMOKE_HOME=%q\nSMOKE_SOCK=%q\nSMOKE_PORT=%q\nSMOKE_KEY=%q\n' "$box" "$home" "$sock" "$port" "$box/sshd/client_ed25519" ;;
  ssh) shift; exec ssh -F /dev/null -i "$box/sshd/client_ed25519" -p "$port" -o StrictHostKeyChecking=no \
    -o UserKnownHostsFile="$box/sshd/known_hosts" -o LogLevel=ERROR "$(id -un)@127.0.0.1" "$@" ;;
  tmux) shift; exec env -u TMUX -u TMUX_PANE tmux -S "$sock" "$@" ;;
  *) echo "用法：rig.sh up|down|env|ssh|tmux|land <参数…>" >&2; exit 2 ;;
esac
