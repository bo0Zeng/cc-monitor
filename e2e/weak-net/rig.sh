#!/usr/bin/env bash
# **弱网台架** —— 在两个容器之间把四种网况各造一遍，每一维都给「改前 / 改后」两个读数，
# 并在同一趟里把一条真 SSH 连接放进这个网况里量（件 `W-F1`）。
#
# ## 形状（定框 `W3` 逐字钉住的那一个）
#
#   A（本侧，`--cap-add=NET_ADMIN`）── 一条自建 docker 网络 ──> B（远端，跑 sshd + nc 靶子）
#
# `tc` 只打在 **A 自己的 `eth0`** 上，全程**不碰宿主网络**：
#   · 打在回环口 `lo` 上效果会**加倍**（一来一回各过一次 qdisc；实测：设 300ms 得 RTT 600ms、
#     设 30% 丢包得 50%）⇒ 读数不好解释，而且那根本不是「远端」的形状。
#   · 用宿主网络跑 = `tc` 打在用户这台机的真网卡上 ⇒ 红线，判据见 `guard-run-netns.sh`。
#
# ## 「改前」那一半不是装饰
#
# 读数是本脚本自己印的 ⇒ 只印「改后」那一个数时，「没跑成」和「造出来了」长得一模一样。
# 所以每一维都是**成对**的：先量没规则时的值，再量加了规则的值，最后拿两者的**差**跟门槛比。
# 差不到门槛 ⇒ 判红，并把两个数一起印出来（`WF1D3②`）。
#
# ## 坏了会喊的三处（`WF1D3`）
#
# ① `tc` 装不上（多半是运行那一步漏了 `--cap-add=NET_ADMIN`）⇒ 点名它、非零退出，
#    **不当成「网况就是这样」**；
# ② 改前/改后差不到门槛 ⇒ 判红并印出两个数；
# ③ 运行面出现宿主网络 / 全权限 ⇒ 判红（`guard-run-netns.sh`，本脚本开跑第一件事就是它）。
#
# ## 跑完不留东西（`WF1D4`）
#
# 容器、自建网络、临时目录跑完都删；宿主的 `docker network ls` 与 `ip link` 跑前跑后**逐字比**。
# 开跑前还会先查一遍「上一趟有没有留垃圾」——**刻意不做「先强删再建」**：那样清理这一步
# 被拆掉也照样绿（第二趟会替第一趟擦屁股），而这里要的正是「没清干净时第二趟当场红」。
#
# ## 本机安全
#
# 全程不起真 claude / 真 daemon / 真 tmux；不碰用户真实的 `~/.ssh`——主机密钥与
# `authorized_keys` 都是在容器里现造的，私钥从头到尾没离开过容器的文件系统。
#
# 用法：
#   bash e2e/weak-net/rig.sh            # 跑一趟，收尾印「合计 PASS=… FAIL=…」
#   bash e2e/weak-net/rig.sh --clean    # 只清掉上一趟崩掉时留下的容器/网络，然后退出
# 环境变量：WEAKNET_IMAGE（默认 ccmon-weaknet:latest，由 build-image.sh 建）
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
IMG="${WEAKNET_IMAGE:-ccmon-weaknet:latest}"
NET="ccmon-weaknet-net"
CA="ccmon-weaknet-a"
CB="ccmon-weaknet-b"

# 🔴 规则打在容器的 eth0，不打 lo（打 lo 效果加倍，见头注）。
DEV="eth0"

# ── 设的是多少（每个量只有这一个家）
SET_DELAY_MS=200
SET_JITTER_MS=50
SET_LOSS_PCT=20
SET_RATE_MBIT=2
SET_SSH_DELAY_MS=300
PAYLOAD_MB=2

# ── 差多少算过（门槛一律取「设定值的一半」这一档：netem 是统计量，取满值会假红）
GATE_DELAY_MS=$(( SET_DELAY_MS / 2 ))
GATE_LOSS_PCT=$(( SET_LOSS_PCT / 2 ))
# 理论传输毫秒 = 载荷比特 / 速率；载荷 2MiB @ 2mbit ⇒ 约 8.4 秒。
THEORY_BW_MS=$(( PAYLOAD_MB * 1048576 * 8 * 1000 / (SET_RATE_MBIT * 1000000) ))
GATE_BW_MS=$(( THEORY_BW_MS / 2 ))
GATE_SSH_MS=1000
GATE_CUT_PCT=90

# ── ssh 的超时参数**显式给**（`WF1D2` 的 acceptor 失效点：ssh 自己的退避会伪装成「网况变差」）
SSH_OPTS="-i /root/.ssh/id_ed25519 -o BatchMode=yes -o StrictHostKeyChecking=no"
SSH_OPTS="$SSH_OPTS -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR"
SSH_OPTS="$SSH_OPTS -o ConnectTimeout=5 -o ServerAliveInterval=5 -o ServerAliveCountMax=2"

pass=0
fail=0
SNAP=""

line() { printf '%s\n' "------------------------------------------------------------"; }
ok()   { echo "  PASS $1"; pass=$((pass + 1)); }
no()   { echo "  FAIL $1"; fail=$((fail + 1)); }
chk()  { if [ "$2" = "$3" ]; then ok "$1"; else no "$1: 期望[$3] 实得[$2]"; fi; }

# 清理：容器 + 自建网络。**WM4 那一刀切的就是这个函数体。**
cleanup_rig() {
  docker rm -f "$CA" "$CB" >/dev/null 2>&1
  docker network rm "$NET" >/dev/null 2>&1
}

on_exit() {
  cleanup_rig
  [ -n "$SNAP" ] && rm -rf -- "$SNAP"
  return 0
}

finish() { # $1 = 退出码
  echo "===== 合计 PASS=$pass FAIL=$fail ====="
  exit "$1"
}

if [ "${1:-}" = "--clean" ]; then
  cleanup_rig
  echo "[rig] 已清掉 $CA / $CB / $NET（若在）"
  exit 0
fi

command -v docker >/dev/null 2>&1 || { echo "需要 docker" >&2; exit 2; }

# ══════════════════════════════════════════════════════════════════
echo "== P0 判据：运行面不许出现宿主网络 / 全权限（WF1D3③）"
if bash "$HERE/guard-run-netns.sh"; then
  ok "运行面零命中宿主网络 / 全权限"
else
  no "运行面出现宿主网络或全权限 —— 那会把网况打到用户这台机上（定框 W3）"
  finish 9
fi
line

echo "== P1 前置：镜像 · 残留 · 宿主快照"
if ! docker image inspect "$IMG" >/dev/null 2>&1; then
  echo "::error::没有镜像 $IMG ⇒ 先跑：bash e2e/weak-net/build-image.sh" >&2
  no "镜像 $IMG 不在"
  finish 2
fi
ok "镜像在：$IMG"

leftover=""
for c in "$CA" "$CB"; do
  [ -n "$(docker ps -aq -f "name=^${c}$")" ] && leftover="$leftover $c"
done
[ -n "$(docker network ls -q -f "name=^${NET}$")" ] && leftover="$leftover $NET"
if [ -n "$leftover" ]; then
  no "上一趟没清干净，盘上还留着：$leftover"
  echo "::error::台架不替上一趟擦屁股（那会让「清理」这一步被拆掉也照样绿）。" >&2
  echo "::error::⇒ 先跑 bash e2e/weak-net/rig.sh --clean，再重跑。" >&2
  finish 7
fi
ok "盘上没有上一趟的残留"

SNAP="$(mktemp -d /tmp/ccmon-weaknet-snap.XXXXXX)"
trap on_exit EXIT
docker network ls > "$SNAP/net.before" 2>&1
ip link            > "$SNAP/link.before" 2>&1
ok "宿主快照已取（docker network ls · ip link）"
line

# ══════════════════════════════════════════════════════════════════
echo "== P2 起台架：一条自建 docker 网络 + 两个容器"
docker network create "$NET" >/dev/null 2>&1 || { no "建自建网络失败"; finish 3; }
# B = 远端。只 sleep，服务由下面 docker exec 起。
docker run -d --name "$CB" --network "$NET" "$IMG" sleep infinity >/dev/null 2>&1 \
  || { no "起容器 B 失败"; finish 3; }
# A = 本侧。要改自己 netns 里的 qdisc ⇒ 只给 NET_ADMIN 这一项能力。
docker run -d --name "$CA" --network "$NET" --cap-add=NET_ADMIN "$IMG" sleep infinity >/dev/null 2>&1 \
  || { no "起容器 A 失败"; finish 3; }

# ★ 正向读数：这一格是下面「跑完都没了」那几条的**分母** ——
#   第一次就没建出来的话，那几条会恒绿。
chk "自建网络确实建出来了（$NET）" "$(docker network ls -q -f "name=^${NET}$" | wc -l)" "1"
chk "容器 A 确实在跑（$CA）" "$(docker inspect -f '{{.State.Running}}' "$CA" 2>/dev/null)" "true"
chk "容器 B 确实在跑（$CB）" "$(docker inspect -f '{{.State.Running}}' "$CB" 2>/dev/null)" "true"

BIP="$(docker inspect -f "{{(index .NetworkSettings.Networks \"$NET\").IPAddress}}" "$CB" 2>/dev/null)"
case "$BIP" in
  ''|*[!0-9.]*) no "取不到 B 的地址（实得[$BIP]）"; finish 3 ;;
esac
echo "  B 的地址 = $BIP（用 IP 不用容器名：把 docker 内嵌 DNS 从读数里摘出去）"
line

# ══════════════════════════════════════════════════════════════════
# tc 的两个入口。**加不上一律点名 NET_ADMIN 并非零退出**（WF1D3①）——
# 不许把「规则没加上」静默地读成「网况就是这样」。
tc_add() { # $1=设备，其余=qdisc 规格
  local dev="$1"; shift
  local out rc
  out="$(docker exec "$CA" tc qdisc add dev "$dev" root "$@" 2>&1)"; rc=$?
  if [ "$rc" -ne 0 ]; then
    no "tc 规则加不上（dev=$dev root $*）：$out"
    echo "::error::台架起不来：容器 $CA 装不上 qdisc。" >&2
    echo "::error::⇒ 最常见的成因是**运行那一步漏了 --cap-add=NET_ADMIN**。" >&2
    echo "::error::   容器当前能力集：$(docker exec "$CA" sh -c 'grep ^CapEff /proc/self/status' 2>&1)" >&2
    echo "::error::⚠ 这一格不许当成「网况就是这样」——没有规则的读数是**基线**，不是弱网。" >&2
    finish 8
  fi
}
tc_del() { docker exec "$CA" tc qdisc del dev "$1" root >/dev/null 2>&1; }

# 判一维：印「设的是多少 / 改前 / 改后 / 门槛 / 实差」，差不到门槛就红（WF1D3②）。
judge() { # $1=维名 $2=设的是多少 $3=改前 $4=改后 $5=门槛 $6=单位
  local d=$(( $4 - $3 ))
  echo "  [$1] 设的是 $2 · 改前 $3$6 · 改后 $4$6 · 实差 $d$6 · 门槛 ≥$5$6"
  if [ "$d" -ge "$5" ]; then
    ok "$1：改前 $3$6 → 改后 $4$6，实差 $d$6 ≥ 门槛 $5$6"
  else
    no "$1 改前/改后差不到门槛：改前 $3$6 · 改后 $4$6 · 实差 $d$6 < 门槛 $5$6（规则多半没生效）"
  fi
}

# 往返延迟（微秒整数；ping 印的是毫秒小数，这里乘 1000 收成整数，全程不用 bc）
rtt_us() { # $1=包数
  docker exec "$CA" sh -c "ping -c $1 -q -i 0.2 -W 3 $BIP 2>/dev/null" \
    | awk -F'= ' '/min\/avg\/max/ {split($2,a,"/"); printf "%d", a[2]*1000}'
}
loss_pct() { # $1=包数 $2=间隔 $3=等待
  docker exec "$CA" sh -c "ping -c $1 -q -i $2 -W $3 $BIP 2>/dev/null" \
    | sed -n 's/.*, \([0-9]*\)% packet loss.*/\1/p'
}
xfer_ms() { # $1=端口
  docker exec -d "$CB" sh -c "nc -l -p $1 > /dev/null" >/dev/null 2>&1
  sleep 1
  docker exec "$CA" sh -c \
    "s=\$(date +%s%N); nc -q0 $BIP $1 < /tmp/payload >/dev/null 2>&1; e=\$(date +%s%N); echo \$(( (e - s) / 1000000 ))"
}

echo "== P3 前置：A 装得上 tc 规则吗（漏了 NET_ADMIN 就在这里喊，WF1D3①）"
tc_add "$DEV" netem delay 1ms
tc_del "$DEV"
ok "A 的 $DEV 上装得上 qdisc（说明 --cap-add=NET_ADMIN 到位）"
line

# ══════════════════════════════════════════════════════════════════
echo "== 维一 · 延迟（netem delay ${SET_DELAY_MS}ms ${SET_JITTER_MS}ms distribution normal）"
D_BEFORE="$(rtt_us 5)"
tc_add "$DEV" netem delay "${SET_DELAY_MS}ms" "${SET_JITTER_MS}ms" distribution normal
D_AFTER="$(rtt_us 8)"
tc_del "$DEV"
: "${D_BEFORE:=0}" "${D_AFTER:=0}"
judge "延迟（往返，微秒）" "${SET_DELAY_MS}ms±${SET_JITTER_MS}ms 单向" \
      "$D_BEFORE" "$D_AFTER" "$(( GATE_DELAY_MS * 1000 ))" "us"
line

echo "== 维二 · 丢包（netem loss ${SET_LOSS_PCT}%，100 个包）"
L_BEFORE="$(loss_pct 100 0.02 1)"
tc_add "$DEV" netem loss "${SET_LOSS_PCT}%"
L_AFTER="$(loss_pct 100 0.02 1)"
tc_del "$DEV"
: "${L_BEFORE:=0}" "${L_AFTER:=0}"
judge "丢包（百分点）" "${SET_LOSS_PCT}% 单向出向" "$L_BEFORE" "$L_AFTER" "$GATE_LOSS_PCT" "%"
line

echo "== 维三 · 带宽（tbf rate ${SET_RATE_MBIT}mbit，载荷 ${PAYLOAD_MB}MiB，理论 ${THEORY_BW_MS}ms）"
docker exec "$CA" sh -c "dd if=/dev/urandom of=/tmp/payload bs=1M count=$PAYLOAD_MB status=none"
B_BEFORE="$(xfer_ms 9101)"
tc_add "$DEV" tbf rate "${SET_RATE_MBIT}mbit" burst 32kbit latency 400ms
B_AFTER="$(xfer_ms 9102)"
tc_del "$DEV"
: "${B_BEFORE:=0}" "${B_AFTER:=0}"
judge "带宽（传 ${PAYLOAD_MB}MiB 的耗时）" "${SET_RATE_MBIT}mbit ⇒ 理论 ${THEORY_BW_MS}ms" \
      "$B_BEFORE" "$B_AFTER" "$GATE_BW_MS" "ms"
line

echo "== 维四 · 断链（netem loss 100%，删规则后要回得来）"
C_BEFORE="$(loss_pct 3 0.2 2)"
tc_add "$DEV" netem loss 100%
C_AFTER="$(loss_pct 3 0.2 1)"
tc_del "$DEV"
sleep 1
C_BACK="$(loss_pct 3 0.2 2)"
: "${C_BEFORE:=0}" "${C_AFTER:=0}" "${C_BACK:=100}"
judge "断链（丢包百分点）" "100% 全断" "$C_BEFORE" "$C_AFTER" "$GATE_CUT_PCT" "%"
# ⚠ 「回得来」这一格的**分母是「先真断了」** —— 规则压根没加上时它照样绿（空真）。
#   09-05 现打的活体：把 tc_add 整个退成空操作，这一格仍 PASS。⇒ 先核分母再判。
if [ "$C_AFTER" -lt "$GATE_CUT_PCT" ]; then
  no "断链恢复：分母没了（改后只丢 $C_AFTER% < $GATE_CUT_PCT%，根本没断过）—— 这一格不许算绿"
elif [ "$C_BACK" -le 5 ]; then
  ok "断链：删掉规则后立刻回得来（丢包 $C_BACK%）"
else
  no "断链：删掉规则后没回来（丢包仍 $C_BACK%，断前是 $C_BEFORE%）"
fi
line

# ══════════════════════════════════════════════════════════════════
# WF1D2：B 起一台 sshd，A 用密钥登进去，然后把这条连接放进弱网里量。
# 🔴 主机密钥与 authorized_keys 全在容器里现造 —— 用户真实的 ~/.ssh 一个字节都不碰。
echo "== 维五 · SSH（B 起 sshd，A 用密钥登进去；仓里此前没有任何一处自己起过 sshd）"
docker exec "$CB" sh -c 'mkdir -p /run/sshd /root/.ssh && chmod 700 /root/.ssh && ssh-keygen -A' >/dev/null 2>&1
docker exec "$CB" sh -c 'printf "%s\n" "PermitRootLogin prohibit-password" "PubkeyAuthentication yes" "UseDNS no" > /etc/ssh/sshd_config.d/weaknet.conf' >/dev/null 2>&1
docker exec "$CA" sh -c 'mkdir -p /root/.ssh && chmod 700 /root/.ssh && ssh-keygen -q -t ed25519 -N "" -f /root/.ssh/id_ed25519' >/dev/null 2>&1
PUB="$(docker exec "$CA" cat /root/.ssh/id_ed25519.pub 2>/dev/null)"
case "$PUB" in
  ssh-ed25519*) ok "A 里现造了一把 ed25519（私钥没离开过容器）" ;;
  *) no "A 里没造出密钥（实得[$PUB]）" ;;
esac
printf '%s\n' "$PUB" | docker exec -i "$CB" sh -c 'cat > /root/.ssh/authorized_keys && chmod 600 /root/.ssh/authorized_keys'
docker exec -d "$CB" /usr/sbin/sshd -D -e >/dev/null 2>&1
sleep 2
SSHD_LISTEN="$(docker exec "$CB" sh -c 'ss -ltn 2>/dev/null | grep -c ":22 "' 2>/dev/null)"
: "${SSHD_LISTEN:=0}"
if [ "$SSHD_LISTEN" -ge 1 ]; then
  ok "B 里 sshd 真的在听 22（$SSHD_LISTEN 个监听口）"
else
  no "B 里 sshd 没起来（22 上零个监听口）"
fi

ssh_probe() { # 印「耗时毫秒 退出码」
  docker exec "$CA" sh -c \
    "s=\$(date +%s%N); ssh $SSH_OPTS root@$BIP 'echo weaknet-probe' >/dev/null 2>&1; rc=\$?; e=\$(date +%s%N); echo \"\$(( (e - s) / 1000000 )) \$rc\""
}

read -r S_BEFORE S_RC0 <<<"$(ssh_probe)"
: "${S_BEFORE:=0}" "${S_RC0:=1}"
chk "改前：A 用密钥登得进 B（一条真 ssh 命令）" "$S_RC0" "0"

tc_add "$DEV" netem delay "${SET_SSH_DELAY_MS}ms"
read -r S_AFTER S_RC1 <<<"$(ssh_probe)"
tc_del "$DEV"
: "${S_AFTER:=0}" "${S_RC1:=1}"
chk "改后：加了 ${SET_SSH_DELAY_MS}ms 延迟，那条 ssh 仍连得上（只是慢）" "$S_RC1" "0"
echo "  ssh 超时参数（显式给，不用默认值）：$SSH_OPTS"
judge "SSH 单条命令耗时" "netem delay ${SET_SSH_DELAY_MS}ms 单向" \
      "$S_BEFORE" "$S_AFTER" "$GATE_SSH_MS" "ms"

tc_add "$DEV" netem loss 100%
read -r S_CUT S_RC2 <<<"$(ssh_probe)"
tc_del "$DEV"
: "${S_CUT:=0}" "${S_RC2:=0}"
# ⚠ 同一族空真：**没有远端可连时这条 ssh 本来就会失败**，于是「断链要报错」恒绿。
#   09-05 现打的活体：把 B 的 sshd 拿掉，D2 那格 5 条红，而这一条照样 PASS。
#   ⇒ 它的分母是「改前那条 ssh 真的连得上」，先核分母。
if [ "$S_RC0" -ne 0 ]; then
  no "断链：分母没了（改前那条 ssh 就没连上，退出码 $S_RC0）—— 「断链会报错」这一格不许算绿"
elif [ "$S_RC2" -ne 0 ]; then
  ok "断链：那条 ssh 报错退出（退出码 $S_RC2，耗时 ${S_CUT}ms，ConnectTimeout=5）"
else
  no "断链：loss 100% 之下那条 ssh 居然还成功了（耗时 ${S_CUT}ms）—— 规则没打到这条连接上"
fi
sleep 1
read -r S_BACK S_RC3 <<<"$(ssh_probe)"
: "${S_BACK:=0}" "${S_RC3:=1}"
chk "删掉规则后重连成功（耗时 ${S_BACK}ms）" "$S_RC3" "0"
line

# ══════════════════════════════════════════════════════════════════
echo "== P9 收尾：自己清理，宿主逐字复原（WF1D4③④）"
cleanup_rig
sleep 1
chk "容器 A 跑完没了" "$(docker ps -aq -f "name=^${CA}$" | wc -l)" "0"
chk "容器 B 跑完没了" "$(docker ps -aq -f "name=^${CB}$" | wc -l)" "0"
chk "自建网络跑完没了" "$(docker network ls -q -f "name=^${NET}$" | wc -l)" "0"

docker network ls > "$SNAP/net.after" 2>&1
ip link            > "$SNAP/link.after" 2>&1
if diff -q "$SNAP/net.before" "$SNAP/net.after" >/dev/null 2>&1; then
  ok "宿主 docker network ls 跑前跑后逐字相同"
else
  no "宿主 docker network ls 变了：$(diff "$SNAP/net.before" "$SNAP/net.after" | tr '\n' ' ')"
fi
if diff -q "$SNAP/link.before" "$SNAP/link.after" >/dev/null 2>&1; then
  ok "宿主 ip link 跑前跑后逐字相同"
else
  no "宿主 ip link 变了：$(diff "$SNAP/link.before" "$SNAP/link.after" | tr '\n' ' ')"
fi

TMPDIR_KEEP="$SNAP"
rm -rf -- "$SNAP"
if [ -d "$TMPDIR_KEEP" ]; then
  no "临时目录没删掉：$TMPDIR_KEEP"
else
  ok "临时目录跑完没了"
fi
line

[ "$fail" -eq 0 ] && finish 0
finish 1
