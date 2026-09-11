#!/usr/bin/env bash
# K-R48 现打对拍：同一套 argv 喂给**旧 bash `shared/ccm`** 与**新原生命令**（后端二进制），
# 逐字节比 stdout+stderr+rc。
#
# 它买的是什么：PM 09-11 的问题是「那四套 e2e（356 条）删掉之后，(b) 那类
# 『一条起会话命令长什么样』的契约还剩多少」。这份读数给的是**另一个答案**——
# 那四套多半**不必重写，只要把 `$CCM` 指向二进制**：两边输出本来就一样。
#
# 跑法（沙箱内，工作树根）：
#   cd remote-daemon-proto && cargo build --bin cc-monitor-remote && cd ..
#   bash evidence/K-R48-native-vs-bash-parity.sh
#
# 09-11 现打读数：**SAME=27 · DIFF=2**，两条 DIFF 都是**有意的**：
#   `--version`   `ccm 4` → `ccm 5`（实现换了，版本号跟着走，KCY4 那条既有纪律）
#   `--ccm-probe` 同一个 `version=` 字段（其余四行逐字相同）
# ⚠ **射程**：只比 `--print` 与报错出口这两面（那是本 CLI 的**平价预言机**）。
#   真起会话 / 真 attach / 真 tmux 那一面不在这份读数里 —— 那要真 tmux，本轮没跑。
REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$REPO/remote-daemon-proto/target}/debug/cc-monitor-remote"
W=$(mktemp -d); mkdir -p "$W/bin" "$W/z" "$W/p"
ln -sf "$BIN" "$W/bin/ccm"
cat > "$W/accounts.json" <<'J'
{"accounts":[{"name":"z","configDir":"WZ","isDefault":true},{"name":"b","configDir":"WZ"}]}
J
sed -i "s|WZ|$W/z|g" "$W/accounts.json"
# 假后端：只答 --list-accounts（bash 那侧要它；二进制那侧不问它）
cat > "$W/bin/faked" <<'D'
#!/bin/sh
case "$1" in
  --list-accounts)
    d=""; while [ $# -gt 0 ]; do [ "$1" = --accts-dir ] && d="$2"; shift; done
    printf '{"kind":"accounts-meta"}\n'
    jq -c '.accounts[] | {name:.name, configDir:.configDir, isDefault:(.isDefault // false)}' "$d/accounts.json" 2>/dev/null ;;
  *) exit 0 ;;
esac
D
chmod +x "$W/bin/faked"
E="env -u TMUX -u CLAUDE_CONFIG_DIR CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=$W/accounts.json CCM_DAEMON_BIN=$W/bin/faked HOME=$W"
PASS=0; FAIL=0
ck() { # ck <标签> <argv...>
  lbl="$1"; shift
  a="$($E bash "$REPO/shared/ccm" "$@" 2>&1; echo "rc=$?")"
  b="$($E "$W/bin/ccm" "$@" 2>&1; echo "rc=$?")"
  if [ "$a" = "$b" ]; then PASS=$((PASS+1)); printf 'SAME | %s\n' "$lbl"
  else FAIL=$((FAIL+1)); printf 'DIFF | %s\n  bash: %s\n  bin : %s\n' "$lbl" "$a" "$b"; fi
}
ck "零修饰"                 --cwd /p --print
ck "resume 位置形"          resume abc-123 --cwd /p --launcher claude --print
ck "--resume 旗标形"        --resume abc-123 --cwd /p --launcher claude --print
ck "--resume= 等号形"       --resume=abc-123 --cwd /p --launcher claude --print
ck "--base"                 --cwd /p --base --print
ck "--model"                --cwd /p --model opus --print
ck "--agent codex"          --cwd /p --agent codex --print
ck "--launcher 覆盖"        --cwd /p --launcher /x/y --print
ck "-- 透传"                --cwd /p --print -- -p "hi there"
ck "--account z"            --cwd /p --account z --print
ck "裸终端落默认号"          --cwd /p --print
ck "attach"                 attach cc-p1 --print
ck "容器路 --tmux=名"        --tmux=cc-p1 --cwd /tmp --ccm-sid p1 --base --print
ck "容器路 派生名"           --tmux --cwd "/home/pi/my proj" --print
ck "容器路 --detach+尺寸"    --tmux=n1 --cwd /p --detach --tmux-size 220x50 --print
ck "容器路 resume+model"     resume p1 --tmux=cc-p1 --cwd /tmp --model opus --ccm-sid p1 --base --print
ck "err 未知选项"            --nope
ck "err 未知 agent"          --agent gemini
ck "err account/base 互斥"   --account z --base
ck "err resume 缺 sid"       resume --tmux
ck "err attach 缺名"         attach --tmux
ck "err detach 无 tmux"      --detach
ck "err 非法尺寸"            --tmux --tmux-size x50
ck "err 多余位置参数"         foo
ck "err codex 不支持 resume" resume s --agent codex
ck "err bus-note 无登记"     --bus-note x
ck "err tmux 名互斥"         --tmux=a --tmux-base b
ck "--version"               --version
ck "--ccm-probe"             --ccm-probe
echo "===== 合计 SAME=$PASS DIFF=$FAIL ====="
