#!/usr/bin/env bash
# 冷开一条会话的台架：照壳与界面的问法问后端（cold.mjs），跑三趟，印每趟的合计与中位。读数不进判据。
#
#   cold.sh <后端二进制> <样本.jsonl> <工作目录>                 直连（stdio）
#   cold.sh <后端二进制> <样本.jsonl> <工作目录> <往返ms> [kbps]  走真 SSH：沙箱 sshd ＋ 用户态延迟限速代理（delay.mjs）
#
# 工作目录放沙箱家目录、sshd 的钥匙与配置（现生）；样本只读、拷进沙箱。起的 sshd 与代理按 PID 停。
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
bin=$(realpath "$1")
sample=$(realpath "$2")
mkdir -p "$3"
work=$(realpath "$3")
rtt=${4:-}
kbps=${5:-0}
rounds=${ROUNDS:-3}

pids=()
stop() {
  for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done
  [ -f "$work/sshd/backend.pid" ] && kill "$(cat "$work/sshd/backend.pid")" 2>/dev/null || true
}
trap stop EXIT
free_port() { python3 -I -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }

if [ -n "$rtt" ]; then
  k="$work/sshd"
  mkdir -p "$k"
  for f in host client; do [ -f "$k/$f" ] || ssh-keygen -q -t ed25519 -N '' -C "cold-$f" -f "$k/$f"; done
  cp "$k/client.pub" "$k/authorized_keys"
  : >"$k/known_hosts"
  # 登录壳：清空环境，起 cold.mjs 备好的那个沙箱里的后端；记下它的 PID（ssh 断了它不一定跟着退，每趟收尾按它停）。
  sb="$work/run"
  cat >"$k/force.sh" <<EOF
#!/bin/sh
echo \$\$ >"$k/backend.pid"
exec env -i PATH=/usr/bin:/bin HOME=$sb/home LANG=C.UTF-8 TZ=Asia/Shanghai TMUX_TMPDIR=$sb/tmux XDG_RUNTIME_DIR=$sb/run RUST_LOG=warn $bin -- --stream
EOF
  chmod +x "$k/force.sh"
  sport=$(free_port)
  cat >"$k/sshd_config" <<EOF
Port $sport
ListenAddress 127.0.0.1
HostKey $k/host
AuthorizedKeysFile $k/authorized_keys
PidFile $k/sshd.pid
UsePAM no
StrictModes no
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
ForceCommand $k/force.sh
Compression no
PermitTTY no
AllowTcpForwarding no
X11Forwarding no
LogLevel ERROR
EOF
  /usr/sbin/sshd -D -f "$k/sshd_config" -E "$k/sshd.log" &
  pids+=($!)
  pport=$(free_port)
  node "$here/delay.mjs" "$pport" "$sport" $((rtt / 2)) "$kbps" >"$k/delay.log" &
  pids+=($!)
  until grep -q READY "$k/delay.log" 2>/dev/null; do sleep 0.1; done
  export SSH_TO="$pport:$k/client:$k/known_hosts"
fi

totals=()
for i in $(seq 1 "$rounds"); do
  out=$(node "$here/cold.mjs" "$bin" "$sample" "$work/run")
  if [ -n "$rtt" ] && [ -f "$k/backend.pid" ]; then
    kill "$(cat "$k/backend.pid")" 2>/dev/null || true
    rm -f "$k/backend.pid"
  fi
  echo "$out" | sed "s/^/[$i] /"
  totals+=("$(echo "$out" | tail -1)")
done
printf '%s\n' "${totals[@]}" | node -e '
const rs = require("fs").readFileSync(0, "utf8").trim().split("\n").map(JSON.parse);
const med = (k) => { const v = rs.map((r) => r[k]).sort((a, b) => a - b); return v[v.length >> 1]; };
const o = { median_of: rs.length };
for (const k of ["tail_pages", "tail_kb", "open_trips", "open_ms", "hello_ms"]) o[k] = med(k);
o.rows_sha = [...new Set(rs.map((r) => r.rows_sha))].join(",");
console.log(JSON.stringify(o));'
