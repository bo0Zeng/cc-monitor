#!/usr/bin/env bash
# `cc-send`：**消息没到的时候，必须有人说话**〔08-13 实测〕。
#
# 两件事同一族，都在这套里：
#  ① 队列里滞留的消息没人管（`cc-busd` 死在「入队后、取走前」）——`[1]`~`[6]`；
#  ② 收件人根本不存在（名字打错/对方还没登记）——`[7]`；
#  ③ Stop 钩子把消息**消费掉了却没喂回去**（`cc-recv` 有副作用，喂回在它之后）——`[8]`；
#  ④ `cc-recv` 跳过读不懂的行却**一个字都不说**——`[14]`。
# 共同形状：**投递没发生，而两侧都被告知一切正常**。
#
# ## 它守的那件事
#
# daemon 在跑时 `cc-send` 只入队就返回（打印「已入队」——当时是真话）。若 `cc-busd` 在
# 取走它之前死掉/关机（`POLL` 默认 0.5s，这个窗口天天存在），那条消息就**永远躺在队列里**：
# 此后 `cc-send` 判定 daemon 不在、走兜底，而兜底**只处理自己刚入队的那一条**，从不回头看队列。
#
# 实测（A 正常投递 → daemon 卡住时发 B → daemon 被杀 → 再发 C、D）：
#   收件箱 3 条 `[A C D]`，队列里永远躺着 1 条 `[B]`
# 发信方看到的是「已入队」、收信方 `cc-recv` 读到 3 条 —— **两侧都不知道少了一条**。
#
# ⚠ 人群不是零，但也不是所有人：装了 systemd 单元的用户 daemon 会被拉起来、队列随即补投
#（那是**迟到**不是丢）。真正永久丢的是 `cc-bus-install.sh` 教的另一条路 ——
# 手动 `cc-busd start` 起过、之后它死了或关机。
#
# ## 判据钉的是「那条消息最终到没到」
#
# 不是「补投函数被调用了」，也不是「stderr 里有那句话」。所以每一格都读**收件箱**：
# 条数 · 内容 · 顺序 · 去重后的条数。
#
# ## 本机安全
#
# `CC_BUS_HOME` **与 `HOME`** 双双指向 `mktemp -d`，**绝不碰真实 `~/.cc-bus/`**
#（下面有一道硬门：脚本眼里的 bus 不在沙箱内就直接 exit 9，且每建一个 bus 重验一次）。**本套件不用 tmux** —— 收发都走文件，
# 于是 `C7i` 那条红线在这里天然不成立（没有任何 tmux 命令可写错）。
# 起的进程只有 `cc-busd` 自己（一个 bash 循环），退出时按 pid 精确 kill，不用 pkill。
set -o pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
S="$REPO/shared/cc-bus/scripts"
command -v jq >/dev/null 2>&1 || { echo "需要 jq"; exit 1; }
command -v flock >/dev/null 2>&1 || { echo "需要 flock"; exit 1; }

SANDBOX="$(mktemp -d)"
ORIG_HOME="$HOME"          # ← 必须在下面改 HOME **之前**取，否则真实 bus 的地址就丢了
# ★★ **腰带 + 背带**〔08-13 我自己踩的坑，见下方 preflight〕：
#    `cc-send` 读的是 `${CC_BUS_HOME:-$HOME/.cc-bus}` —— 只钉 `CC_BUS_HOME` 是不够的，
#    它一旦没传到（我第一版就是），fallback 会直接落到**真实 `~/.cc-bus`**。
#    ⇒ `HOME` 也搬进沙箱，让那条 fallback 结构上够不着真实 bus。
export HOME="$SANDBOX/home"
mkdir -p "$HOME"
BUSD_PIDS=()
cleanup() {
  local p
  for p in ${BUSD_PIDS+"${BUSD_PIDS[@]}"}; do kill -CONT "$p" 2>/dev/null; kill -9 "$p" 2>/dev/null; done
  rm -rf "$SANDBOX"
}
trap cleanup EXIT

# ===== 起飞前自检：落点绝不能是真实 bus =====
#
# ⚠⚠ **这道门是我自己撞出来的**〔08-13〕：本套件第一版把 `export CC_BUS_HOME=…` 写在
# `B="$(new_bus 1)"` 的**命令替换**里 —— 那是子 shell，export **出不到父 shell**。
# 于是 `cc-busd`/`cc-send` 全都落到了**真实 `~/.cc-bus`** 上（实得损伤：多出
# `cc-busd.lock` 与 `log/busd.log` 两个文件，消息数据没动，已逐个删回原状）。
# ★ 而第一版的「肯定式 canary」**没拦住**：它断言的是沙箱目录可写、真实 bus 里没有记号，
#   瞄的是**目录**，不是**被测脚本真正读的那个变量**。⇒ 判据要钉在因上，不是钉在旁证上。
#
# ⇒ 现在两向都钉住：
#  ① `HOME` 已搬进沙箱（上面），fallback 路径够不着真实 bus；
#  ② 下面按 `cc-send` 逐字相同的取法算出「脚本眼里的 bus」，断言它落在沙箱内，
#     且**每建一个新 bus 都重验一次**（见 `new_bus`）——一次性自检拦不住中途丢变量。
REAL_BUS="$ORIG_HOME/.cc-bus"   # ⚠ 此刻 `~`/`$HOME` 已指向沙箱，只能用开跑前存下的那个
assert_sandboxed() {
  local seen="${CC_BUS_HOME:-$HOME/.cc-bus}"     # ← 与 cc-send 第 4 行逐字同一个取法
  case "$seen" in
    "$SANDBOX"/*) : ;;
    *) echo "起飞前自检失败：脚本眼里的 bus 是 $seen（不在沙箱 $SANDBOX 内）——拒绝在真实 bus 上跑" >&2; exit 9 ;;
  esac
  [ "$seen" = "$REAL_BUS" ] && { echo "起飞前自检失败：落点正是真实 bus" >&2; exit 9; }
  return 0
}

pass=0; fail=0
chk() { if [ "$2" = "$3" ]; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1: 期望[$3] 实得[$2]"; fail=$((fail+1)); fi; }

# 每个场景一套干净的 bus（上一场景的残留会让"队列是空的"这种断言恒真）。
# ⚠ **不许写成 `B="$(new_bus 1)"`** —— 命令替换是子 shell，里面的 `export` 出不来
#   （就是上面那场事故）。这里改成**直接赋全局 `B`**，并当场重验落点。
new_bus() {
  B="$SANDBOX/bus$1"
  rm -rf "$B"; mkdir -p "$B/inbox" "$B/state" "$B/log" "$B/queue"
  export CC_BUS_HOME="$B"
  export CCBUS_POLICY_MODE=off
  assert_sandboxed
}

# 起 cc-busd，pid 落全局 `PID`。
# ⚠ 同样**不许放进命令替换**：后台进程会继承那根管子 ⇒ `$( )` 等到它死才返回
#   （第一版就这么挂了 10 分钟，直到 timeout 把整组杀掉）。三个 fd 全部改道，
#   再显式 `</dev/null`，让它与本套件的输出完全脱钩。
start_busd() {
  local i
  "$S/cc-busd" run </dev/null >"$B/busd.out" 2>&1 &
  local launched=$!
  disown 2>/dev/null || true      # 免得后面 kill -9 时 bash 往 stderr 刷"已杀死"
  for i in $(seq 60); do [ -f "$B/cc-busd.pid" ] && break; sleep 0.1; done
  PID="$(cat "$B/cc-busd.pid" 2>/dev/null)"
  [ -n "$PID" ] || PID="$launched"
  BUSD_PIDS+=("$PID")
}

# 等收件箱涨到 n 条（不用固定 sleep：慢机上会假红，快机上白等）
wait_inbox() {
  local f="$1" n="$2" i
  # ⚠ `wc -l < "$f"` 里的重定向失败是**shell 自己**报的，`2>/dev/null` 挂在 wc 上拦不住
  #   ⇒ 文件还不存在时先短路，别刷一屏"没有那个文件"。
  for i in $(seq 60); do
    [ -f "$f" ] && [ "$(wc -l < "$f")" -ge "$n" ] && return 0
    sleep 0.1
  done
  return 1
}

inbox_texts() { jq -r .text < "$1" 2>/dev/null | tr '\n' '|'; }
qcount() { ls "$1"/queue/*.json 2>/dev/null | wc -l | tr -d ' '; }

echo "===== cc-send 兜底：滞留队列补投 ====="

echo "[1] 事故重演 → 修后应当补投"
new_bus 1; start_busd
chk "cc-busd 起来了（pidfile 有 pid）" "$([ -n "$PID" ] && echo yes || echo no)" "yes"
"$S/cc-send" bob "A 正常" >/dev/null 2>&1
wait_inbox "$B/inbox/bob.jsonl" 1 || true
chk "对照组：daemon 在跑时正常投递" "$(wc -l < "$B/inbox/bob.jsonl" 2>/dev/null || echo 0)" "1"
# SIGSTOP：daemon 还活着（kill -0 通过、argv 没变、pidfile 还在）⇒ cc-send 判它在跑、只入队
kill -STOP "$PID"
"$S/cc-send" bob "B 卡住时发的" >/dev/null 2>&1
sleep 0.6
chk "daemon 卡住 ⇒ 那条留在队列里" "$(qcount "$B")" "1"
chk "  且**没有**进收件箱（这就是事故现场）" "$(wc -l < "$B/inbox/bob.jsonl")" "1"
kill -9 "$PID" 2>/dev/null; sleep 0.3
out1="$("$S/cc-send" bob "C 之后发的" 2>&1)"
chk "★ 再发一条时**顺带补投**了滞留那条" "$(wc -l < "$B/inbox/bob.jsonl")" "3"
chk "★ 顺序是入队序（滞留的先于自己那条）" "$(inbox_texts "$B/inbox/bob.jsonl")" "A 正常|B 卡住时发的|C 之后发的|"
chk "  队列清空" "$(qcount "$B")" "0"
chk "  且明说了（stderr 有补投计数）" "$(printf '%s' "$out1" | grep -c '补投 1 条')" "1"

echo "[2] daemon 在跑时**不走**这条路（零影响）"
new_bus 2; start_busd
out2="$("$S/cc-send" bob "D daemon 活着" 2>&1)"
wait_inbox "$B/inbox/bob.jsonl" 1 || true
chk "走的是入队路径" "$(printf '%s' "$out2" | grep -c '已入队')" "1"
chk "没有补投那句话" "$(printf '%s' "$out2" | grep -c '滞留消息')" "0"
chk "消息照常投达" "$(wc -l < "$B/inbox/bob.jsonl" 2>/dev/null || echo 0)" "1"
kill -9 "$PID" 2>/dev/null

echo "[3] ★ 并发兜底不许双投（认领靠原子改名，不是靠先看后动）"
new_bus 3
for i in 1 2 3; do
  jq -cn --arg id "stale-$i" --arg t "滞留$i" \
    '{id:$id,from:"alice",to:"bob",ts:"2026-08-13T00:00:00-07:00",text:$t,class:"direct",in_reply_to:null,trace:"alice",hops:0,prio:0}' \
    > "$B/queue/178666000000000000$i-x.json"
done
for i in 1 2 3 4 5; do "$S/cc-send" bob "并发$i" >/dev/null 2>&1 & done
wait
chk "5 个 cc-send + 3 条滞留 = 8 条" "$(wc -l < "$B/inbox/bob.jsonl" 2>/dev/null || echo 0)" "8"
chk "★ 去重后仍是 8（一条都没被投两次）" "$(jq -r .id < "$B/inbox/bob.jsonl" | sort -u | wc -l | tr -d ' ')" "8"
chk "三条滞留的都到了" "$(jq -r .id < "$B/inbox/bob.jsonl" | grep -c '^stale-')" "3"
chk "队列清空" "$(qcount "$B")" "0"
_nproc=0; for f in "$B"/queue/.proc.*; do [ -e "$f" ] && _nproc=$((_nproc+1)); done
chk "  .proc 认领文件零残留" "$_nproc" "0"

echo "[4] 上界：积压再多也不许把一次 cc-send 拖死"
new_bus 4
for i in $(seq -w 1 30); do
  jq -cn --arg id "s$i" '{id:$id,from:"alice",to:"bob",ts:"x",text:"m",class:"direct",in_reply_to:null,trace:"alice",hops:0,prio:0}' \
    > "$B/queue/17866600000000000$i-x.json"
done
out4="$(CCBUS_DRAIN_MAX=5 "$S/cc-send" bob "自己那条" 2>&1)"
chk "带走 5 条 + 自己那条 = 6" "$(wc -l < "$B/inbox/bob.jsonl" 2>/dev/null || echo 0)" "6"
chk "剩下 25 条**原样留在队列**（不是被丢掉）" "$(qcount "$B")" "25"
chk "★ 报数诚实（超预算那 25 条明写出来）" "$(printf '%s' "$out4" | grep -c '补投 5 条.*超预算 25 条')" "1"

echo "[5] 只碰顶层 *.json —— 点前缀是别人的地盘（与 daemon 口径一致）"
new_bus 5
printf '{}\n' > "$B/queue/.proc.999.stale.json"
printf '{}\n' > "$B/queue/.dead.badmsg.json"
printf '{}\n' > "$B/queue/.tmp.halfwritten"
"$S/cc-send" bob "只发我自己这条" >/dev/null 2>&1
chk ".proc（别人正在处理）没被碰" "$([ -f "$B/queue/.proc.999.stale.json" ] && echo yes || echo no)" "yes"
chk ".dead（死信）没被碰" "$([ -f "$B/queue/.dead.badmsg.json" ] && echo yes || echo no)" "yes"
chk ".tmp（半截文件）没被碰" "$([ -f "$B/queue/.tmp.halfwritten" ] && echo yes || echo no)" "yes"
chk "自己那条正常投达" "$(wc -l < "$B/inbox/bob.jsonl" 2>/dev/null || echo 0)" "1"

echo "[6] 坏信封不许卡住整条队列（毒丸）"
new_bus 6
printf '这不是 JSON\n' > "$B/queue/1786660000000000001-x.json"
jq -cn '{id:"good-1",from:"alice",to:"bob",ts:"x",text:"坏信封后面那条",class:"direct",in_reply_to:null,trace:"alice",hops:0,prio:0}' \
  > "$B/queue/1786660000000000002-x.json"
out6="$("$S/cc-send" bob "自己那条" 2>&1)"
chk "★ 坏信封之后那条照样补投" "$(jq -r .id < "$B/inbox/bob.jsonl" | grep -c '^good-1')" "1"
chk "  坏信封被消费掉、不再占队列" "$(qcount "$B")" "0"
chk "  拦下计数如实（坏信封算被路由层拦下）" "$(printf '%s' "$out6" | grep -c '拦下 1 条')" "1"

echo "[7] 收件人根本不存在时要说一句（把 planner 打成 planer）"
new_bus 7
printf 'alice\t%%1\t2026-08-13T00:00:00-07:00\n' > "$B/agents.tsv"
out7="$("$S/cc-send" planer "把结论发给 planner" 2>&1)"
chk "★ 未登记的收件人 ⇒ 明说" "$(printf '%s' "$out7" | grep -c "'planer' 不在已登记名单")" "1"
chk "  但消息照发（不拦截：先发后到是正当用法）" "$(wc -l < "$B/inbox/planer.jsonl" 2>/dev/null || echo 0)" "1"
out7b="$("$S/cc-send" alice "正常一条" 2>&1)"
chk "  已登记的收件人**零噪音**" "$(printf '%s' "$out7b" | grep -c '不在已登记名单')" "0"
# 名字匹配必须是**整行整字段**：planner 登记着，plan / planner2 都不算登记过
printf 'planner\t%%2\tx\n' > "$B/agents.tsv"
chk "  精确匹配：planner 已登记 ⇒ 不告警" "$("$S/cc-send" planner m 2>&1 | grep -c '不在已登记名单')" "0"
chk "  精确匹配：plan（前缀）⇒ 照样告警" "$("$S/cc-send" plan m 2>&1 | grep -c '不在已登记名单')" "1"
rm -f "$B/agents.tsv"
chk "  agents.tsv 还不存在（第一次用）⇒ 也告警，不当成「都登记过」" "$("$S/cc-send" bob m 2>&1 | grep -c '不在已登记名单')" "1"

echo "[8] Stop 钩子：消息不许「读过了」却没人看见"
# ★ 真事故：`cc-recv` **会推进已读位置**（有副作用），而"喂回给 Claude"在它之后。
#   40 条普通消息（每条约 4KB，单条都不超限）累计 160KB ⇒ 原来的 `jq --arg r "$reason"`
#   走 argv 撞 MAX_ARG_STRLEN(128 KiB) ⇒ 钩子 rc=126、stdout 空，**而位置已经推到 40**
#   ⇒ 那 40 条再也读不到。一个 agent 离开一阵子回来，积压到这个量是正常的。
# ⇒ 两道：① `--rawfile` 不走 argv；② 喂不回去就**把位置退回去**（管所有失败模式）。
HOOK="$S/cc-bus-stop-hook"
new_bus 8
export CC_BUS_ID=hookme_cc
python3 - "$B/inbox/hookme_cc.jsonl" <<'PYEOF'
import json,sys
with open(sys.argv[1],'w') as f:
    for i in range(40):
        f.write(json.dumps({"id":f"m{i}","from":"alice","to":"hookme_cc","ts":"x","text":"内容"*1000,
                            "class":"direct","in_reply_to":None,"trace":"alice","hops":0,"prio":0},
                           ensure_ascii=False)+"\n")
PYEOF
echo 0 > "$B/state/hookme_cc.pos"
_hout="$(printf '{}' | bash "$HOOK" 2>/dev/null)"
chk "★ 160KB 积压：钩子仍吐出 block 决定（不再撞 argv 上限）" \
  "$(printf '%s' "$_hout" | jq -r '.decision // "<空>"' 2>/dev/null)" "block"
chk "  40 条都在喂回去的正文里" \
  "$(printf '%s' "$_hout" | jq -r .reason 2>/dev/null | grep -c '【cc-bus 来自')" "40"
chk "  这一次推进已读位置是**对的**（消息确实送到了）" "$(cat "$B/state/hookme_cc.pos")" "40"

# 第二道：喂回失败时**必须退回位置**。用一个只让带 --rawfile 那次失败的 jq 桩，
# cc-recv 内部用的 jq 照常放行（否则连消息都读不出来，判据就测不到这条路）。
new_bus 9
export CC_BUS_ID=roll_cc
python3 - "$B/inbox/roll_cc.jsonl" <<'PYEOF'
import json,sys
with open(sys.argv[1],'w') as f:
    for i in range(3):
        f.write(json.dumps({"id":f"m{i}","from":"alice","to":"roll_cc","ts":"x","text":f"第{i}条",
                            "class":"direct","in_reply_to":None,"trace":"alice","hops":0,"prio":0},
                           ensure_ascii=False)+"\n")
PYEOF
echo 0 > "$B/state/roll_cc.pos"
mkdir -p "$SANDBOX/fakebin"
{ echo '#!/bin/bash'
  echo 'for a in "$@"; do [ "$a" = "--rawfile" ] && exit 7; done'
  echo "exec $(command -v jq) \"\$@\""
} > "$SANDBOX/fakebin/jq"
chmod +x "$SANDBOX/fakebin/jq"
_rout="$(printf '{}' | PATH="$SANDBOX/fakebin:$PATH" bash "$HOOK" 2>"$SANDBOX/hookerr.txt")"
chk "★ 喂回失败 ⇒ 已读位置**退回原处**" "$(cat "$B/state/roll_cc.pos")" "0"
chk "  三条都还读得到（没被吞）" \
  "$(bash "$S/cc-recv" roll_cc | grep -c '【cc-bus 来自')" "3"
chk "  且明说了（不是静默吞掉）" \
  "$(grep -c '喂回消息失败' "$SANDBOX/hookerr.txt")" "1"
unset CC_BUS_ID

echo "[14] ★ 读不懂的行照跳，但必须说一句"
# `fromjson? // empty` 静默丢掉解析不了的行（防毒丸卡死，那是对的），
# 而已读位置**照样推过去** ⇒ 那条消息永远读不到了，且没有任何地方提过。
# ⚠ 丢弃不改（毒丸不能卡住整条收件箱）——改的是**沉默**。
new_bus 14
python3 - "$B/inbox/br_cc.jsonl" <<'PYEOF'
import json,sys
with open(sys.argv[1],'w') as f:
    f.write(json.dumps({"id":"a","from":"alice","to":"br_cc","ts":"x","text":"第一条",
                        "class":"direct","in_reply_to":None,"trace":"alice","hops":0,"prio":0},
                       ensure_ascii=False)+"\n")
    f.write('{"id":"b","from":"alice","to":"br_cc","text":"这行被截断了…\n')   # 半截 JSON
    f.write(json.dumps({"id":"c","from":"alice","to":"br_cc","ts":"x","text":"第三条",
                        "class":"direct","in_reply_to":None,"trace":"alice","hops":0,"prio":0},
                       ensure_ascii=False)+"\n")
PYEOF
echo 0 > "$B/state/br_cc.pos"
_rv="$(bash "$S/cc-recv" br_cc)"
chk "两条好的照常显示" "$(printf '%s' "$_rv" | grep -c '【cc-bus 来自')" "2"
chk "★ 那一条读不懂的**被说出来了**（不再是沉默）" \
  "$(printf '%s' "$_rv" | grep -c '跳过 1 条读不懂')" "1"
chk "  且说了去哪儿查（收件箱路径 + 行号范围）" \
  "$(printf '%s' "$_rv" | grep -c 'br_cc.jsonl 的第 1..3 行')" "1"
chk "  已读位置照样推到底（毒丸不许卡住整条收件箱）" "$(cat "$B/state/br_cc.pos")" "3"
# 对照：全是好行时不许多嘴
new_bus 15
python3 - "$B/inbox/ok_cc.jsonl" <<'PYEOF'
import json,sys
with open(sys.argv[1],'w') as f:
    f.write(json.dumps({"id":"a","from":"alice","to":"ok_cc","ts":"x","text":"好的",
                        "class":"direct","in_reply_to":None,"trace":"alice","hops":0,"prio":0},
                       ensure_ascii=False)+"\n")
PYEOF
echo 0 > "$B/state/ok_cc.pos"
chk "  对照：全是好行时**零噪音**" \
  "$(bash "$S/cc-recv" ok_cc | grep -c '跳过')" "0"

echo
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ] || exit 1
echo "===== cc-send 滞留队列补投验收全部通过 ====="
