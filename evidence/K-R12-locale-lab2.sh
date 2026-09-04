#!/usr/bin/env bash
# K-R12 量拍 · 第二轮：D 方案真正的失效面 + 判据形状
#
# 🔴 跑在哪里：`ccmon-devbox:latest` 容器里，`docker run --rm -i`，**零挂载**。宿主零改动。
#    复现：docker run --rm -i ccmon-devbox:latest bash -s < evidence/K-R12-locale-lab2.sh
#
# 第一轮（lab.sh）推翻了件文件 §0e 那条「LC_ALL 指向不存在的 locale ⇒ 静默退回 C」：
# `LC_ALL=zz_ZZ.UTF-8` 实测**照样出真 TAB**。本轮量的是「那到底看的是什么」。

set -u
LAB=/tmp/kr12-lab2
SOCK=$LAB/sock
mkdir -p "$LAB/文档"
T() { tmux -S "$SOCK" "$@"; }
bytes() { od -c | head -2 | sed 's/^/        /'; }

T kill-server 2>/dev/null
env -u LANG -u LC_ALL -u LC_CTYPE tmux -S "$SOCK" new-session -d -s 'lab文档' -c "$LAB/文档"
sleep 1
FMT=$'#{pid}\t#{socket_path}'

# 判据：出真 TAB = 通道干净
verdict() {  # 读 stdin
  local o; o=$(cat)
  case "$o" in *"$(printf '\t')"*) echo "CLEAN" ;; *) echo "DIRTY" ;; esac
}
probe() {  # $1=描述  其余=env 赋值...   最后固定跑 display-message
  local desc=$1; shift
  local v; v=$(env -u LANG -u LC_ALL -u LC_CTYPE "$@" tmux -S "$SOCK" display-message -p "$FMT" | verdict)
  printf '  %-38s -> %s\n' "$desc" "$v"
}

echo "############ E11 · tmux 到底看哪个变量、看的是「存在」还是「字符串长相」"
probe "（什么都不设）"
probe "LC_ALL=C.UTF-8"                       LC_ALL=C.UTF-8
probe "LC_ALL=zz_ZZ.UTF-8（locale 不存在）"    LC_ALL=zz_ZZ.UTF-8
probe "LC_ALL=完全胡写.UTF8"                  LC_ALL=完全胡写.UTF8
probe "LC_ALL=utf8（连点都没有）"              LC_ALL=utf8
probe "LC_ALL=zh_CN.GB18030（真·非 UTF-8）"   LC_ALL=zh_CN.GB18030
probe "LC_ALL=C"                             LC_ALL=C
probe "LC_ALL=''（空串）"                     LC_ALL=
echo
echo "  -- 优先级：LC_ALL > LC_CTYPE > LANG ？"
probe "LC_ALL=C + LANG=C.UTF-8"              LC_ALL=C LANG=C.UTF-8
probe "LC_ALL='' + LANG=C.UTF-8"             LC_ALL= LANG=C.UTF-8
probe "LC_CTYPE=C + LANG=C.UTF-8"            LC_CTYPE=C LANG=C.UTF-8
probe "LANG=C.UTF-8（只有它）"                LANG=C.UTF-8
echo
echo "  -- -u 能不能盖过一个真·非 UTF-8 的 LC_ALL？"
v=$(env -u LANG -u LC_CTYPE LC_ALL=zh_CN.GB18030 tmux -S "$SOCK" -u display-message -p "$FMT" | verdict)
printf '  %-38s -> %s\n' "LC_ALL=zh_CN.GB18030 且 tmux -u" "$v"
v=$(env -i /usr/bin/tmux -S "$SOCK" -u display-message -p "$FMT" | verdict)
printf '  %-38s -> %s\n' "env -i（环境全清）且 tmux -u" "$v"
v=$(env -i /usr/bin/tmux -S "$SOCK" display-message -p "$FMT" | verdict)
printf '  %-38s -> %s\n' "env -i（环境全清）不加 -u" "$v"

echo
echo "############ E12 · 判据形状①：字段数下溢（< N）会不会误伤合法内容"
echo "  会话名里塞一个真 TAB，tmux 收不收："
o=$(T new-session -d -s "$(printf 'aa\tbb')" 2>&1); echo "    new-session -s 'aa<TAB>bb' -> rc=$? out=[$o]"
echo "  pane_current_path 里塞一个真 TAB（目录名带 TAB）："
mkdir -p "$LAB/$(printf 'p\tq')" 2>/dev/null && echo "    mkdir 带 TAB 的目录 -> OK" || echo "    mkdir 失败"
T new-session -d -s tabpath -c "$LAB/$(printf 'p\tq')" 2>/dev/null
sleep 0.5
LS_FMT=$'#{session_name}\t#{pane_current_path}\t#{pane_current_command}\t#{?session_attached,1,0}\t#{session_windows}\t#{@ccm_sid}'
echo "  UTF-8 客户端下逐行字段数（期望 6；带 TAB 的内容会 >6，不会 <6）："
env -u LANG -u LC_CTYPE LC_ALL=C.UTF-8 tmux -S "$SOCK" ls -F "$LS_FMT" \
  | awk -F'\t' '{printf "    行%d: 字段数=%d  首段=[%s]\n", NR, NF, $1}'
echo "  POSIX 客户端下逐行字段数（期望全 1）："
env -u LANG -u LC_ALL -u LC_CTYPE tmux -S "$SOCK" ls -F "$LS_FMT" \
  | awk -F'\t' '{printf "    行%d: 字段数=%d\n", NR, NF}'

echo
echo "############ E13 · 判据形状②：canary 的最小形状"
for c in '✓' '·' 'µ' 'À'; do
  cf="#{pid}"$'\t'"$c"
  a=$(env -u LANG -u LC_CTYPE LC_ALL=C.UTF-8 tmux -S "$SOCK" display-message -p "$cf" | awk -F'\t' '{print $NF}')
  b=$(env -u LANG -u LC_ALL -u LC_CTYPE tmux -S "$SOCK" display-message -p "$cf" | awk -F'\t' '{print $NF}')
  printf '  canary=%-3s  UTF-8下末段=[%s]  POSIX下末段=[%s]  判得出？%s\n' \
    "$c" "$a" "$b" "$([ "$a" = "$c" ] && [ "$b" != "$c" ] && echo YES || echo NO)"
done
echo "  纯 ASCII 的 canary 能不能只靠自己判（不靠字段数）："
cf="#{pid}"$'\t'"OK"
a=$(env -u LANG -u LC_CTYPE LC_ALL=C.UTF-8 tmux -S "$SOCK" display-message -p "$cf" | awk -F'\t' '{print $NF}')
b=$(env -u LANG -u LC_ALL -u LC_CTYPE tmux -S "$SOCK" display-message -p "$cf" | awk -F'\t' '{print $NF}')
printf '  canary=OK   UTF-8下末段=[%s]  POSIX下末段=[%s]  判得出？%s\n' \
  "$a" "$b" "$([ "$a" = "OK" ] && [ "$b" != "OK" ] && echo YES || echo NO)"

echo
echo "############ E14 · 「同一趟」成不成立：canary 与真数据是不是同一次输出"
echo "  一条命令同时拿数据与 canary（不新增 subprocess）："
env -u LANG -u LC_ALL -u LC_CTYPE tmux -S "$SOCK" display-message -p "$FMT"$'\t✓' | bytes
env -u LANG -u LC_CTYPE LC_ALL=C.UTF-8 tmux -S "$SOCK" display-message -p "$FMT"$'\t✓' | bytes

echo
echo "############ E15 · 那个 `_` 到底按什么替换（字节数？显示宽度？）"
for s in '文' '文档' 'é' 'ab'; do
  T set-option -t 'lab文档' @kr12probe "$s" 2>/dev/null
  raw=$(env -u LANG -u LC_CTYPE LC_ALL=C.UTF-8 tmux -S "$SOCK" display-message -p '#{@kr12probe}')
  san=$(env -u LANG -u LC_ALL -u LC_CTYPE tmux -S "$SOCK" display-message -p '#{@kr12probe}')
  printf '  [%s] 字节=%d  UTF-8下=[%s]  POSIX下=[%s] 长度=%d\n' \
    "$s" "$(printf '%s' "$s" | wc -c)" "$raw" "$san" "${#san}"
done

echo
echo "############ 收摊"
T kill-server 2>/dev/null; rm -rf "$LAB"; echo done
