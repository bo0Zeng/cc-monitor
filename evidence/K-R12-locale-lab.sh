#!/usr/bin/env bash
# K-R12 量拍 · locale / 客户端 UTF-8 标志 的受控实验
#
# 🔴 跑在哪里：`ccmon-devbox:latest` 容器里，`docker run --rm -i`，**零挂载**。
#    ⇒ 不碰宿主 tmux server、不碰宿主 locale、不写宿主任何文件（红线：不动机器）。
#    复现：docker run --rm -i ccmon-devbox:latest bash -s < evidence/K-R12-locale-lab.sh
#
# 量什么：D1（-u 与 LC_ALL 两种写法各自怎么落）· D2（LC_ALL 指向不存在 locale 时会怎样）
#         · D3（判据形状：canary 能不能验出通道脏了）
#
# ⚠ 所有格式串一律用 $'...\t...' 拿**真 TAB** —— Rust 源里 "\t" 是真 TAB 字节，
#   shell 里写 '\t' 会变成反斜杠+t，那是量错东西。

set -u
LAB=/tmp/kr12-lab
SOCK=$LAB/sock
CN=$LAB/文档
mkdir -p "$CN"
T() { tmux -S "$SOCK" "$@"; }
# 一律显式 -S 私有 socket；结尾 kill 的也只是这台私有 server。
bytes() { od -c | sed 's/^/      /'; }

echo "############ 0 · 环境"
tmux -V
echo "locale -a: $(locale -a | tr '\n' ' ')"
echo "LANG=${LANG:-<unset>}  LC_ALL=${LC_ALL:-<unset>}  LC_CTYPE=${LC_CTYPE:-<unset>}"
ldd --version | head -1

echo
echo "############ 1 · 起一台私有 server（会话名与 pane cwd 都带中文）"
T kill-server 2>/dev/null
env -u LANG -u LC_ALL -u LC_CTYPE tmux -S "$SOCK" new-session -d -s 'lab文档' -c "$CN" 2>&1
sleep 1
echo "  server 起来了？ $(T list-sessions -F '#{session_id}' 2>&1 | tr -d '\n')"

# 五种客户端设定 —— 同一台 server、同一条命令，只换客户端这一侧
run_mode() {  # $1=mode  $2..=tmux 参数
  local m=$1; shift
  case $m in
    POSIX)   env -u LANG -u LC_ALL -u LC_CTYPE tmux -S "$SOCK" "$@" 2>&1 ;;
    CUTF8)   env -u LANG -u LC_CTYPE LC_ALL=C.UTF-8 tmux -S "$SOCK" "$@" 2>&1 ;;
    DASH_U)  env -u LANG -u LC_ALL -u LC_CTYPE tmux -S "$SOCK" -u "$@" 2>&1 ;;
    BOGUS)   env -u LANG -u LC_CTYPE LC_ALL=zz_ZZ.UTF-8 tmux -S "$SOCK" "$@" 2>&1 ;;
    BOGUS_U) env -u LANG -u LC_CTYPE LC_ALL=zz_ZZ.UTF-8 tmux -S "$SOCK" -u "$@" 2>&1 ;;
    LANGONLY) env -u LC_ALL -u LC_CTYPE LANG=C.UTF-8 tmux -S "$SOCK" "$@" 2>&1 ;;
  esac
}

echo
echo "############ 2 · E2 复现：watcher.rs:452 那条格式串（#{pid}<TAB>#{socket_path}）"
FMT_Q=$'#{pid}\t#{socket_path}'
for m in POSIX CUTF8 DASH_U BOGUS BOGUS_U LANGONLY; do
  o=$(run_mode "$m" display-message -p "$FMT_Q"); rc=$?
  echo "  -- $m (rc=$rc)"; printf '%s' "$o" | bytes | head -3
done

echo
echo "############ 3 · E3〔D2 的核心〕LC_ALL 指向不存在的 locale：有没有任何一声"
echo "  setlocale 本身怎么说（同一条 glibc 路径）："
for L in C.UTF-8 zz_ZZ.UTF-8 xx_XX; do
  r=$(LC_ALL=$L locale charmap 2>&1); rc=$?
  echo "    LC_ALL=$L -> rc=$rc  charmap=$(printf '%s' "$r" | tr '\n' '|')"
done
echo "  tmux 在 BOGUS 下的 stderr（上面 E2 已把 2>&1 并进来了，这里单独看 stderr）："
env -u LANG -u LC_CTYPE LC_ALL=zz_ZZ.UTF-8 tmux -S "$SOCK" display-message -p "$FMT_Q" 2>/tmp/kr12-err >/dev/null
echo "    stderr 字节数 = $(wc -c </tmp/kr12-err)   内容 = $(cat /tmp/kr12-err)"
echo "    退出码 = $?"

echo
echo "############ 4 · E4 · -u 的位置约束（放到子命令后面会怎样）"
o=$(env -u LANG -u LC_ALL -u LC_CTYPE tmux -S "$SOCK" display-message -u -p "$FMT_Q" 2>&1); echo "  tmux display-message -u -p ... -> rc=$? out=[$o]"
o=$(env -u LANG -u LC_ALL -u LC_CTYPE tmux -S "$SOCK" ls -u -F '#{session_name}' 2>&1); echo "  tmux ls -u -F ...             -> rc=$? out=[$o]"

echo
echo "############ 5 · E5 · TMUX_LS_FMT（六列真 TAB，含中文 pane_current_path）"
LS_FMT=$'#{session_name}\t#{pane_current_path}\t#{pane_current_command}\t#{?session_attached,1,0}\t#{session_windows}\t#{@ccm_sid}'
for m in POSIX CUTF8 DASH_U; do
  o=$(run_mode "$m" ls -F "$LS_FMT")
  n=$(printf '%s' "$o" | awk -F'\t' '{print NF}')
  echo "  -- $m  切出的字段数=$n"; printf '%s' "$o" | bytes | head -4
done

echo
echo "############ 6 · E6 · cut -f 在「整行没有分隔符」时的行为（tmux.rs:441 的 Gate 2 依赖它）"
echo "    printf '1\\tabc' | cut -f2  -> [$(printf '1\tabc' | cut -f2)]"
echo "    printf '1_'      | cut -f2  -> [$(printf '1_' | cut -f2)]      <= 无 TAB 时整行被原样吐出"
echo "    printf '1_'      | cut -f1  -> [$(printf '1_' | cut -f1)]"
echo "    printf '1_'      | cut -sf2 -> [$(printf '1_' | cut -sf2)]     <= -s 才会压掉"
echo "    ⇒ 被改写后 sid=\"整行\" 非空 ⇒ [ -n \"\$sid\" ] 恒真"

echo
echo "############ 7 · E7 · canary：格式串尾巴挂一个固定非 ASCII 标记，能不能验出通道脏了"
CANARY_FMT=$'#{pid}\t#{socket_path}\t✓'
for m in POSIX CUTF8 DASH_U BOGUS; do
  o=$(run_mode "$m" display-message -p "$CANARY_FMT")
  last=$(printf '%s' "$o" | awk -F'\t' '{print $NF}')
  echo "  -- $m  末段=[$last]  末段==✓ ? $([ "$last" = "✓" ] && echo YES || echo NO)"
done

echo
echo "############ 8 · E8 · tmux 自己有没有一个「客户端是不是 UTF-8」的可读变量"
for m in POSIX CUTF8 DASH_U BOGUS; do
  echo "  -- $m  #{client_utf8}=[$(run_mode "$m" display-message -p '#{client_utf8}')]"
done

echo
echo "############ 9 · E9 · capture-pane -p 的输出走不走同一条 sanitize"
T send-keys -t 'lab文档' "printf '中文测试ABC\\n'" Enter
sleep 1
for m in POSIX CUTF8 DASH_U; do
  echo "  -- $m"; run_mode "$m" capture-pane -p -t 'lab文档' | grep -a 'ABC' | tail -1 | bytes | head -3
done

echo
echo "############ 10 · E10 · sh -c 串里两种写法各自站不站得住"
echo "  (a) export 前置：$(sh -c 'export LC_ALL=C.UTF-8; tmux -S '"$SOCK"' display-message -p "$1"' _ "$FMT_Q" | od -c | head -1)"
echo "  (b) 赋值前缀 + exec（特殊内建）：$(env -u LC_ALL sh -c 'LC_ALL=C.UTF-8 exec tmux -S '"$SOCK"' display-message -p "$1"' _ "$FMT_Q" 2>&1 | od -c | head -1)"
echo "  (c) 赋值前缀 + 普通命令：$(env -u LC_ALL sh -c 'LC_ALL=C.UTF-8 tmux -S '"$SOCK"' display-message -p "$1"' _ "$FMT_Q" 2>&1 | od -c | head -1)"
echo "  (d) 父进程 env 传下去（模拟 Command::env(\"LC_ALL\")）：$(env LC_ALL=C.UTF-8 sh -c 'tmux -S '"$SOCK"' display-message -p "$1"' _ "$FMT_Q" | od -c | head -1)"
echo "  sh 是谁：$(readlink -f /bin/sh)"

echo
echo "############ 11 · 收摊（只杀本实验那台私有 server）"
T kill-server 2>/dev/null
rm -rf "$LAB"
echo "done"
