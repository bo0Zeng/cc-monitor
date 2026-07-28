#!/bin/bash
# F10（剩余账号 UX）：每账号 plan 用量窗口% 探针的**编排机制**真机行为验收。
#
# 本脚本验证的是"探针会话的建/等/送键/抓屏/清理/自毁看门狗"这套编排本身是否按设计工作
# ——**不**验证解析器（`account-usage-parse.ts`）对真实 `/usage` 输出格式的正确性，那部分
# 没有真机 spike 前无法验证（见 features/F10-remaining-account-ux.md §0/§7：不应该在这个
# 开发环境里启动一个真实已认证的 claude 子进程去测试）。用一个假的"claude" stand-in
# （FAKECLAUDE）模拟 REPL 行为——收到 `/usage` 后打印固定文本，不是真 claude。
#
# 输入 = 真 builder 产出的生产命令串（`cargo test --lib -- --ignored --nocapture
# emit_usage_probe_cmd_for_e2e`，见 src-tauri/src/account_usage.rs 对应测试头注）——
# 不手搓等价命令。隔离 -L socket，不碰用户任何真实会话/真实 tmux。
#
# 跑法：bash e2e/usage-probe-acceptance.sh   （npm run test:usage-probe）
set -o pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
SOCK=ccmF10usage
TMUX_BIN="$(command -v tmux)" || { echo "需要 tmux"; exit 1; }
SP="$(mktemp -d)"
trap 'rm -rf "$SP"; "$TMUX_BIN" -L "$SOCK" kill-server 2>/dev/null' EXIT

(cd "$REPO/src-tauri" && cargo test --lib -- --ignored --nocapture emit_usage_probe_cmd_for_e2e 2>/dev/null) \
  | grep -P '^\w+\t' > "$SP/f10-cmds.tsv"
[ -s "$SP/f10-cmds.tsv" ] || { echo "cargo test 未产出任何命令串——检查上游 emit_usage_probe_cmd_for_e2e 是否编译/运行成功"; exit 1; }
CMD() { grep -P "^$1\t" "$SP/f10-cmds.tsv" | cut -f2-; }

T() { "$TMUX_BIN" -L "$SOCK" "$@"; }
reset() { T kill-server 2>/dev/null; sleep 0.3; }
sessions() { T ls -F '#{session_name}' 2>/dev/null | sort | tr '\n' ' '; }
pane() { T capture-pane -p -t "=$1:" 2>/dev/null; }

# 生产命令串里裸调 `tmux`——shim 导到隔离 socket（同 tmux-guarded-acceptance.sh 的做法）。
BIN="$SP/bin"; mkdir -p "$BIN"
printf '#!/bin/sh\nexec %s -L %s "$@"\n' "$TMUX_BIN" "$SOCK" > "$BIN/tmux"; chmod +x "$BIN/tmux"
# 假 claude stand-in：先打一行"欢迎"（让第一轮稳定轮询有内容可稳定），然后循环读行，
# 收到 "/usage" 就打印固定的合成用量文本（不是真机抓来的真实格式，只用来验证抓屏机制本身
# 能不能正确把这段文本带出来）。
cat > "$BIN/FAKECLAUDE" <<'EOF'
#!/bin/sh
echo "Welcome to Claude Code (fake stand-in, not real claude)"
while IFS= read -r line; do
  if [ "$line" = "/usage" ]; then
    printf 'Current session\n  38%%\nResets in 2h 14m\n'
  fi
done
EOF
chmod +x "$BIN/FAKECLAUDE"
export PATH="$BIN:$PATH"

PASS=0; FAIL=0
ck() { if [ "$2" = "$3" ]; then printf 'PASS | %-56s | %s\n' "$1" "$3"; PASS=$((PASS+1));
       else printf 'FAIL | %-56s | 期望=%s 实得=%s\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }

echo "===== 场景 1：正常路径 —— 建会话+送键+抓屏+清理，最终拿到 FAKECLAUDE 的固定回应 ====="
reset
OUT="$(bash -c "$(CMD normal)")"
ck "抓到的文本含 FAKECLAUDE 的固定回应（38%）" "true" \
  "$(printf '%s' "$OUT" | grep -q '38%' && echo true || echo false)"
ck "探针会话用完即清（不残留）" "" "$(sessions)"

echo
echo "===== 场景 2：撞名残留 —— 探针名字前有个不相关的旧会话，探针须清场重建而非被卡住 ====="
# 用独立 slug（collision）而非复用场景 1 的"z"——场景 1 的探针也会挂一个 3s 后触发的看门狗
# （独立于主流程是否已自然完成），若两个场景撞同一个会话名，场景 1 遗留的看门狗可能在场景 2
# 刚建好同名新会话时杀过来，制造纯测试脚本层面的假竞态，跟被测代码本身无关。
reset
# 另建一个不相关的"陪衬"会话保持 server 存活——否则杀掉唯一的撞名会话会让整个 tmux server
# 退出（tmux 的既有行为：最后一个会话被杀，server 就没有理由继续跑），紧接着的 new-session
# 虽然会自动拉起新 server，但这条竞态本身不是本场景想测的东西（本场景测的是"撞名清场"，
# 不是"server 生死"），加一个陪衬会话把这条无关变量固定住。
T new-session -d -s decoy-keep-server-alive
T new-session -d -s ccm-usage-collision 'sh -c "while true; do sleep 1; done"'
sleep 0.3
ck "清场前：撞名会话已存在（前置条件）" "ccm-usage-collision decoy-keep-server-alive " "$(sessions)"
OUT="$(bash -c "$(CMD collision)")"
ck "清场重建后仍正确拿到 FAKECLAUDE 的固定回应（证明旧会话被换掉，不是卡在旧会话里）" "true" \
  "$(printf '%s' "$OUT" | grep -q '38%' && echo true || echo false)"
ck "用完仍清理干净（新建的那个也不残留，陪衬会话不受影响）" "decoy-keep-server-alive " "$(sessions)"

echo
echo "===== 场景 3：自毁看门狗 —— 探针中途被打断（发送完 payload 后进程即被杀），会话仍在" \
     "看门狗超时窗口内自行消失，不需要人工介入 ====="
reset
# 用短看门狗（3s）的命令变体；`timeout` 在"起会话+挂看门狗+送 payload"之后、"送 /usage+抓屏
# +清理"完成之前就把整条流水线杀掉,模拟"SSH 通道中途断开、cc-monitor 那次 exec 跑不完"。
# 0.6s 留足两头余量:会话创建是单次近乎瞬时的 tmux 调用(远早于 0.6s);而脚本不被打断时的
# 最短自然完成时间(两轮稳定轮询各自至少两次 0.5s 迭代才能判定"已稳定")在 2s 以上,0.6s
# 稳落在第一轮稳定轮询进行中,离两头都有充分安全边际。
timeout 0.6 bash -c "$(CMD watchdog)" >/dev/null 2>&1
ck "探针被打断后:会话在看门狗超时前仍短暂存活(前置条件,不是已经被前台清理路径顺便清掉)" "ccm-usage-watchdog " "$(sessions)"
sleep 3
ck "看门狗超时窗口后:会话已被自毁看门狗独立清理,不需要人工介入" "" "$(sessions)"

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
