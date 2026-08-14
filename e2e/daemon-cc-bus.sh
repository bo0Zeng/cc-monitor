#!/usr/bin/env bash
# `P4f`：daemon 的 cc-bus 基础命令 —— `--bus-list` / `--bus-send` **真跑**。
#
# ## 为什么必须有真跑这一层
#
# 单测把 `parse_list` / `classify_send` 那些纯函数钉住了，但它们**证明不了命令调得动**：
# 08-13 实测撞到过一次 —— 两条命令进了 `inbound::REGISTRY`、`hello.commands` 也报了它们、
# 分派臂也认（`cli_control::handles` 是派生的），而 `main::is_query_mode` 这道**闸门**
# 读的是手写的 `SUBCOMMANDS`。漏加两行的后果是**静默的**：daemon 打一行
# 「未知 flag，已忽略」的 warn 之后**照常进流模式**，调用方拿到一堆 jsonl 行。
# 单测全绿。⇒ 这套件跑的是**真二进制的真 argv**。
#
# ## 本机安全
#
# · `CLAUDE_CONFIG_DIR` 指向 mktemp 沙箱 —— **不指的话 daemon 会去流式读你真实的
#   `~/.claude`**（只读，但会把你的转录刷一屏，08-13 我就这么干了一次）；
# · `CC_BUS_HOME` 同样在沙箱里，绝不碰真实 `~/.cc-bus/`；
# · `CC_BUS_BIN_DIR` 指向**仓内**的 cc-bus 脚本（不是已装的那份）——
#   验的是仓里这一版，与 `exec-bit-guard` 的口径一致；
# · **不用 tmux** ⇒ `C7i` 那条红线在这里天然不成立；
# · 起的进程只有 daemon 自己（一次性 exec，`</dev/null` + `timeout`）。
set -o pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
D="$REPO/remote-daemon-proto/target/debug/cc-monitor-remote"
[ -x "$D" ] || { echo "需要先 build daemon：cd remote-daemon-proto && cargo build"; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "需要 jq"; exit 1; }
TIMEOUT="$(command -v timeout)" || { echo "需要 timeout"; exit 1; }

SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"' EXIT
BUS="$SANDBOX/bus"; CLA="$SANDBOX/claude"; EMPTY="$SANDBOX/empty"; NOHOME="$SANDBOX/nohome"
mkdir -p "$BUS"/{inbox,state,log,queue} "$CLA/projects" "$EMPTY" "$NOHOME"

SCRIPTS="$REPO/shared/cc-bus/scripts"
[ -x "$SCRIPTS/cc-list" ] || { echo "仓内没有 cc-list：$SCRIPTS"; exit 1; }

pass=0; fail=0
chk() { if [ "$2" = "$3" ]; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1: 期望[$3] 实得[$2]"; fail=$((fail+1)); fi; }

# 一次调用 = 一个干净的 env（**逐次显式给全**，免得上一格的变量漏进来）
d() {
  env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$SCRIPTS" \
      CCBUS_POLICY_MODE="${POLICY:-off}" CC_BUS_ID=probe_cc \
      "$TIMEOUT" 20 "$D" "$@" 2>"$SANDBOX/err.txt"
}

echo "===== P4f：daemon 的 cc-bus 基础命令 ====="

echo "[1] --bus-list：空总线"
out="$(d --bus-list </dev/null)"
chk "空总线回空数组（不是报错）" "$(printf '%s' "$out" | jq -c '.agents')" "[]"

echo "[2] --bus-list：读得出 id / 地址 / 待读数"
# 台架**直接写** agents.tsv：它扮演的是"cc-bus 的状态"。
# ⚠ 被测的 daemon **不许**读这个格式（`no_cc_bus_data_layout_leaks_into_the_daemon` 钉着），
#   台架知道它是可以的 —— 格式变了这套件会红，那正是我们要的信号。
printf 'alpha_cc\talpha_cc:0.0\t2026-08-13T00:00:00-07:00\t111\n' > "$BUS/agents.tsv"
printf 'a\nb\n' > "$BUS/inbox/alpha_cc.jsonl"
echo 0 > "$BUS/state/alpha_cc.pos"
out="$(d --bus-list </dev/null)"
chk "id" "$(printf '%s' "$out" | jq -r '.agents[0].id')" "alpha_cc"
chk "tmux 地址" "$(printf '%s' "$out" | jq -r '.agents[0].target')" "alpha_cc:0.0"
chk "待读数（真数出来的，不是猜的）" "$(printf '%s' "$out" | jq -r '.agents[0].unread')" "2"

echo "[3] --bus-send：真的送到了收件人的收件箱"
before="$(wc -l < "$BUS/inbox/alpha_cc.jsonl")"
out="$(printf '{"to":"alpha_cc","text":"来自 daemon 的一条"}' | d --bus-send)"
chk "回 sent=true" "$(printf '%s' "$out" | jq -r '.sent')" "true"
chk "回显收件人" "$(printf '%s' "$out" | jq -r '.to')" "alpha_cc"
chk "★ 收件箱真的多了一行" "$(( $(wc -l < "$BUS/inbox/alpha_cc.jsonl") - before ))" "1"
chk "  正文一字不差" \
  "$(tail -1 "$BUS/inbox/alpha_cc.jsonl" | jq -r .text)" "来自 daemon 的一条"

echo "[4] ★ 收件人非法：daemon **不预判**，由 cc-bus 自己拒（白名单只有一份）"
printf '{"to":"a/b","text":"x"}' | d --bus-send >"$SANDBOX/o4.txt"; rc4=$?
chk "退出码非 0" "$([ "$rc4" -ne 0 ] && echo yes || echo no)" "yes"
chk "码是 invalid_args" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "invalid_args"
chk "  消息里带着 cc-send 自己那句话（证明是它拒的，不是我们）" \
  "$(jq -r .message < "$SANDBOX/err.txt" | grep -c '非法收件人')" "1"

echo "[5] ★ 被路由层拦下：与「名字写错了」必须分得开"
printf 'probe_cc\tnobody\n' > "$BUS/policy.tsv"
POLICY=on
printf '{"to":"alpha_cc","text":"x"}' | d --bus-send >/dev/null; rc5=$?
POLICY=off
chk "退出码非 0" "$([ "$rc5" -ne 0 ] && echo yes || echo no)" "yes"
chk "★ 码是 rejected（不是 invalid_args）" "$(jq -r .code < "$SANDBOX/err.txt" 2>/dev/null)" "rejected"
rm -f "$BUS/policy.tsv"

echo "[6] ★ 没装 cc-bus：说得出查过哪儿"
env CLAUDE_CONFIG_DIR="$CLA" CC_BUS_HOME="$BUS" CC_BUS_BIN_DIR="$EMPTY" HOME="$NOHOME" PATH="$EMPTY" \
    "$TIMEOUT" 20 "$D" --bus-list </dev/null >/dev/null 2>"$SANDBOX/err6.txt"
chk "码是 not_installed（不是笼统的 failed）" "$(jq -r .code < "$SANDBOX/err6.txt" 2>/dev/null)" "not_installed"
msg="$(jq -r .message < "$SANDBOX/err6.txt" 2>/dev/null)"
chk "  列出了 ~/.local/bin 那一处" "$(printf '%s' "$msg" | grep -c '.local/bin/cc-list')" "1"
chk "  列出了 skills 那一处" "$(printf '%s' "$msg" | grep -c 'skills/cc-bus/scripts/cc-list')" "1"
chk "  告诉人怎么指过去" "$(printf '%s' "$msg" | grep -c 'CC_BUS_BIN_DIR')" "1"

echo "[7] ★ 两个入口都有：能力探测口报得出这两条"
probe="$(d --daemon-probe </dev/null)"
# ⚠ 探测口报的是 **CLI flag 形式**（`--bus-list`），不是裸命令名 —— 我第一版按裸名断言，
#   当场红。写判据前先看一眼真实输出，别照着脑子里的形状写。
chk "探测口的 commands 里有 --bus-list" \
  "$(printf '%s' "$probe" | jq -r '.commands | index("--bus-list") != null')" "true"
chk "探测口的 commands 里有 --bus-send" \
  "$(printf '%s' "$probe" | jq -r '.commands | index("--bus-send") != null')" "true"
# ★ 这一格钉的是那条**静默失效**：命令"被报出来"不等于"调得动"。
#   闸门（`main::SUBCOMMANDS`）漏加时，上面两格照样绿，而下面这格会红。
out="$(d --bus-list </dev/null)"
chk "★ 报得出**且**调得动（漏了闸门这格会红）" \
  "$(printf '%s' "$out" | jq -e 'has("agents")' >/dev/null 2>&1 && echo yes || echo no)" "yes"

echo
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ] || exit 1
echo "===== P4f 验收全部通过 ====="
