#!/bin/bash
# P3t-Y5：**本机会话真的活在 tmux 里且可接**。
#
# 与 e2e/ccm-acceptance.sh 的分工：那个验 ccm CLI 自己的行为；本脚本验的是
# **cc-monitor 渲染出来的那一条本机拉起命令**在真 tmux 上干了什么。
#
# ★★ 命令串**不在本脚本里手抄** —— 从 Rust 渲染器的出口取
# （`history.rs::emit_local_launch_command_for_e2e`）。手抄一份的话，
# 渲染器改了、脚本没改，实测照样绿：那就成了「验我自己抄得对不对」。
#
# ★★ C7i 红线（2026-08-11 事故后立）：**tmux 命令一律带 socket 选择器**。
#   隔离靠 `$BIN/tmux` 这个 shim（exec 真 tmux 并强插 `-L <私有名>`），
#   **不靠**「记得 unset TMUX / 设 TMUX_TMPDIR」—— 那种形态漏一次就打到用户真实 server 上
#   （实测事故：9 个真实会话没了）。shim 在 PATH 最前，且下面有一条**前置断言**
#   核实它真的被选中；选不中就当场退出，绝不降级去裸跑。
#
# C7d：**绝不起真 claude**。launcher 指向 e2e/fake-claude。
#
# 跑法：bash e2e/p3t-local-tmux.sh
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
SOCK=p3tY5
SID=p3te2e01
TMUXNAME=p3te2e01-cc
# ★★ launcher 名字**独一无二**，PATH 上不可能有第二个。见下面 ② 的事故记录。
LAUNCHER=p3tfakecc
TMUX_BIN="$(command -v tmux)" || { echo "需要 tmux"; exit 1; }

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  PASS  $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL  $1"; }

TMP="$(mktemp -d /tmp/p3t-y5.XXXXXX)"
# 收尾：**只收自己那台**（`-L` 选择器在，绝不裸 kill-server）。
cleanup() {
  "$TMUX_BIN" -L "$SOCK" kill-session -t "=$TMUXNAME:" 2>/dev/null
  "$TMUX_BIN" -L "$SOCK" kill-server 2>/dev/null
  [ -n "${P3T_KEEP:-}" ] && { echo "  [keep] $TMP"; return; }
  rm -rf -- "$TMP"
}
trap cleanup EXIT

BIN="$TMP/bin"; mkdir -p "$BIN"
# ① tmux shim —— ccm 内部裸调的 `tmux` 全部导到隔离 socket。
printf '#!/bin/sh\nexec %s -L %s "$@"\n' "$TMUX_BIN" "$SOCK" > "$BIN/tmux"; chmod +x "$BIN/tmux"
# ② 假 launcher —— fake-claude 顶替 `claude`（C7d）。它前台常驻 sleep，
#    所以 `pane_current_command` 读得到、会话不会秒退。
#
#    ⚠ 中间夹一层**记账 shim**，理由有两条，都是实测逼出来的：
#    ① fake-claude 最后是 `exec sleep` —— 进程映像被替换，`/proc/<pid>/cmdline` 里的
#       `--resume` **当场消失**，事后再查什么都查不到。证据必须在它 exec 之前落盘。
#    ② fake-claude 的 argv.log 写在 `$CLAUDE_CONFIG_DIR` 下，而渲染出来的串带 `--base`
#       ⇒ ccm 会把那个变量 **unset 掉**（那正是 `--base` 的语义）⇒ 日志落到 fake-claude
#       自己的缺省目录，不在本次隔离目录里。第一版就是在这儿失败的 —— 而那次失败本身
#       是个**正面证据**：`--base` 真的生效了。
#    shim 把 argv 与它看到的 `CLAUDE_CONFIG_DIR` 一起记进本次隔离目录，两件事一次证完。
#
#    ★★★ **它的名字不能叫 `claude`** —— 2026-08-11 实测事故：
#    生产送法是 `bash -lic`，**登录 shell 会重跑 profile 并把 `~/.local/bin` 重排到 PATH 最前**
#    ⇒ 名为 `claude` 的 shim 被顶掉，解析到的是用户**真实的 claude**（`C7d` 逐字禁的那件事，
#    真起了两次）。当时的前置断言量的是**外层 shell** 的 `command -v tmux` —— 量错了地方，
#    于是「隔离没生效」这件事以 PASS 的形式呈现。
#    ⇒ 改用一个只在本次隔离目录里存在的名字，经 `--launcher` 传给 ccm。
#      PATH 上谁在前都盖不住它 —— 这是**结构保证**，不是「记得把 PATH 摆对」那种纪律。
cat > "$BIN/$LAUNCHER" <<SHIM
#!/bin/sh
{ printf 'CFG=%s argv=' "\${CLAUDE_CONFIG_DIR:-<unset>}"; for a in "\$@"; do printf '%s ' "\$a"; done; printf '\n'; } >> "$TMP/launcher.log"
exec "$HERE/fake-claude" "\$@"
SHIM
chmod +x "$BIN/$LAUNCHER"
# ③ ccm 本体上 PATH（渲染出来的串以 `ccm` 开头）。
ln -sf "$REPO/shared/ccm" "$BIN/ccm"

export PATH="$BIN:$PATH"
export CCM_SELF="$REPO/shared/ccm"
# 隔离账号库 / 工作区 / 预信任写入点，绝不碰用户真实文件（同 ccm-acceptance.sh 的手法）
ACCTS="$TMP/accts"; mkdir -p "$ACCTS/z"
cat > "$ACCTS/accounts.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$ACCTS/z", "isDefault": true, "mode": "isolated" } ] }
JSON
printf 'CCM_ACCTS_MANIFEST=%s\nCCM_WORKSPACE=%s\n' "$ACCTS/accounts.json" "$TMP/ws" > "$TMP/ccm-config"
mkdir -p "$TMP/ws" "$TMP/proj"
export CCM_CONFIG="$TMP/ccm-config"
export CCM_CLAUDEJSON="$TMP/claude.json" CCM_CODEXTOML="$TMP/config.toml"
export CLAUDE_CONFIG_DIR="$TMP/fakehome"
export CCM_FAKE_CWD="$TMP/proj" CCM_FAKE_SLEEP=120
unset CLAUDECODE CLAUDE_CODE_SESSION_ID
# ccm 在 tmux 内会退化成「就地起」（见 shared/ccm）—— 那时要验的容器行为根本不发生。
# ⚠ 这一句是**为了让被测行为发生**，不是隔离手段；隔离由上面的 shim 独自负责。
unset TMUX TMUX_PANE

echo "== P3t-Y5：本机拉起真的建出 tmux 会话 =="

# ── 前置断言 0：**在生产用的那种 shell 里**量，不是在外层量。
#
# ⚠ 第一版在外层 shell 量 `command -v tmux` 就报 PASS —— 而命令真正跑在 `bash -lic` 里，
#   那是登录 shell、PATH 被 profile 重排过。**量错了地方，于是「隔离没生效」以 PASS 呈现。**
#   这一条现在一律经 `bash -lic` 问，问的就是待会儿真正解析命令的那个环境。
probe_in_login_shell() { bash -lic "command -v $1" 2>/dev/null | tail -1; }

GOT_TMUX="$(probe_in_login_shell tmux)"
if [ "$GOT_TMUX" != "$BIN/tmux" ]; then
  echo "  ABORT 隔离没生效：登录 shell 里的 tmux 是 '$GOT_TMUX'，不是 shim $BIN/tmux"
  echo "        绝不降级裸跑 —— 那会把会话建到用户真实 tmux server 上（C7i 红线）。"
  exit 2
fi
ok "隔离前置：登录 shell 里的 tmux 是 shim（$GOT_TMUX）"

GOT_LAUNCHER="$(probe_in_login_shell "$LAUNCHER")"
if [ "$GOT_LAUNCHER" != "$BIN/$LAUNCHER" ]; then
  echo "  ABORT 假 launcher 没被选中：登录 shell 里的 $LAUNCHER 是 '$GOT_LAUNCHER'"
  echo "        绝不降级 —— 那会起用户真实的 claude（C7d 红线）。"
  exit 2
fi
ok "C7d 前置：登录 shell 里的 $LAUNCHER 是假 launcher（$GOT_LAUNCHER）"

# ★ 上面两条是「我要的在」。**第一版的教训是那还不够** —— 当时「tmux shim 确实赢了」为真，
#   而同一条 PATH 上「claude shim 没赢」也为真，两者互不牵连。
#   ⇒ 下面取到命令串之后，还要断言串里**显式带了** `--launcher <假的>`：
#     那样 ccm 根本不会去 PATH 上找 `claude`，「真 claude 不许被起」就不再依赖 PATH 顺序。

# ── 取生产渲染器的真输出
RAW="$(cd "$REPO/src-tauri" && P3T_E2E_SID="$SID" P3T_E2E_TMUX="$TMUXNAME" P3T_E2E_LAUNCHER="$LAUNCHER" \
  cargo test --no-default-features --lib -- --ignored --nocapture emit_local_launch_command_for_e2e 2>/dev/null)"
CMD="$(printf '%s' "$RAW" | sed -n 's/.*P3T_CMD<<<\(.*\)>>>.*/\1/p')"
if [ -z "$CMD" ]; then
  echo "  ABORT 取不到渲染器输出 —— e2e 无对象可跑（别当绿过）"
  exit 2
fi
ok "拿到生产渲染器的串：$CMD"

case "$CMD" in
  *"--tmux=$TMUXNAME"*) ok "串里带 --tmux=$TMUXNAME（会话容器）" ;;
  *) bad "串里没有 --tmux=$TMUXNAME —— 本件的正题没落地：$CMD" ;;
esac
# C7d：串里必须显式指定假 launcher。缺了它 ccm 会去 PATH 上找 `claude` —— 那是真的。
case "$CMD" in
  *"--launcher $LAUNCHER"*) ok "C7d：串里显式指定了假 launcher（$LAUNCHER）" ;;
  *) echo "  ABORT 串里没有 --launcher $LAUNCHER —— 会起真 claude（C7d）：$CMD"; exit 2 ;;
esac

# ── 按生产的送法跑它：`bash -lic <cmd>`，stdio 全 null、脱离进程组
#    （`launch.rs::build_local_posix_argv` / `launch_local_posix` 逐字同形）。
#    ⚠ 没有 tty ⇒ ccm 建完会话后那次 attach 会失败，而**会话留下** ——
#    这正是 P3t 要拿到的东西：进程活在 tmux 里有真 tty，等人来接。
setsid bash -lic "$CMD" </dev/null >/dev/null 2>&1 &
LAUNCH_PID=$!

# 轮询等会话出现（固定 sleep 是机器速度的赌注，ccm-acceptance.sh 已记过这个教训）
n=0
while [ $n -lt 100 ]; do
  if "$TMUX_BIN" -L "$SOCK" has-session -t "=$TMUXNAME:" 2>/dev/null; then break; fi
  n=$((n+1)); sleep 0.2
done

if "$TMUX_BIN" -L "$SOCK" has-session -t "=$TMUXNAME:" 2>/dev/null; then
  ok "tmux -L $SOCK ls 看得到会话 $TMUXNAME"
else
  bad "私有 socket 上没有会话 $TMUXNAME —— 容器没建起来"
  echo "  ---- tmux ls ----"; "$TMUX_BIN" -L "$SOCK" ls 2>&1 | sed 's/^/  /'
fi

# ── 前台命令是被拉起的那个（DoD 逐字）
n=0; PANECMD=""
while [ $n -lt 100 ]; do
  PANECMD="$("$TMUX_BIN" -L "$SOCK" list-panes -t "=$TMUXNAME:" -F '#{pane_current_command}' 2>/dev/null | head -1)"
  case "$PANECMD" in sleep|claude|fake-claude) break ;; esac
  n=$((n+1)); sleep 0.2
done
case "$PANECMD" in
  # fake-claude 最后 `exec sleep` 保 PID 不变 ⇒ 前台命令是 sleep。
  # 它证明的是「被拉起的那个进程还在前台活着」，不是「真 claude 能用」（见 §4 射程）。
  sleep|"$LAUNCHER"|fake-claude) ok "会话前台命令是被拉起的那个（$PANECMD）" ;;
  *) bad "会话前台命令是 '$PANECMD' —— 不是被拉起的那个" ;;
esac

# ── 它真的可接（DoD 的「且可接」）
if "$TMUX_BIN" -L "$SOCK" list-panes -t "=$TMUXNAME:" >/dev/null 2>&1; then
  ok "会话可寻址（list-panes 命中，attach 的前提）"
else
  bad "会话寻址不到 —— 接不上"
fi

# ── 拉起器真的被调到了，且拿到的是这个 sid（不是空会话、不是别人的会话）
n=0
while [ $n -lt 50 ]; do
  [ -s "$TMP/launcher.log" ] && break
  n=$((n+1)); sleep 0.2
done
if grep -q -- "--resume $SID" "$TMP/launcher.log" 2>/dev/null; then
  ok "拉起器收到了 --resume $SID（命令真的进了容器）"
else
  bad "拉起器没收到 --resume $SID —— 会话建了但命令没进去"
  [ -f "$TMP/launcher.log" ] && sed 's/^/  /' "$TMP/launcher.log"
fi

# ── `--base` 真的生效：容器里的 CLAUDE_CONFIG_DIR 是**未设**，不是外层那个。
#    这一格是顺带白得的（第一版的失败逼出来的），但它守的是真东西：
#    `--base` 的语义是「显式不注入」，若它没生效，账号就会被外层环境静默带进去。
if grep -q "CFG=<unset>" "$TMP/launcher.log" 2>/dev/null; then
  ok "--base 生效：容器里 CLAUDE_CONFIG_DIR 未设（不是外层那个）"
else
  bad "--base 没生效：容器里仍看得到 CLAUDE_CONFIG_DIR（账号会被静默带进去）"
  [ -f "$TMP/launcher.log" ] && sed 's/^/  /' "$TMP/launcher.log"
fi

kill "$LAUNCH_PID" 2>/dev/null
echo
echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
