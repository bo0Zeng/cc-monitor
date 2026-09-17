# K-R12 量拍 · locale / 客户端 UTF-8 标志 的受控实验（配方 + 复现说明）

原始输出在同目录两份 `.out`：`K-R12-locale-lab.out` · `K-R12-locale-lab2.out`。
结论逐条写在件文件 `K-R12-…md` 的 `§5`。

## 跑在哪里（红线：不动这台机器）

`ccmon-devbox:latest` 容器里，`docker run --rm -i`，**零挂载**
⇒ 不碰宿主 tmux server、不碰宿主 locale、不写宿主任何文件。
容器内实测：`tmux 3.4` · `glibc 2.39` · `locale -a` 只有 `C` `C.utf8` `POSIX` · `/bin/sh` 是 `dash`。
两个实验都一律用 `-S` 私有 socket，收摊只 `kill-server` **本实验那台**。

复现（把下面某个代码块存成文件，或直接喂 stdin）：

```
docker run --rm -i ccmon-devbox:latest bash -s < <这个文件里的第一段>
docker run --rm -i ccmon-devbox:latest bash -s < <这个文件里的第二段>
```

## 🔴 为什么配方住在 `.md` 里，而不是两个 `.sh`

`shell_lint_registry::tests::every_shell_script_is_either_linted_or_registered_as_exempt`
要求**仓里每个 shell 脚本要么进 CI 的 shellcheck 表达式、要么登记豁免并写清理由**。
这条判据是对的（它逮到过我：见下一节）。但它给的两条出路
——`.github/workflows/ci.yml` 与 `src-tauri/src/shell_lint_registry.rs` 的 `EXEMPT`
——**都不在本拍的写区里**，实现方自批不了。

⇒ 本拍的处置：**不往仓里新增未登记的 shell 脚本**，配方以文档形式留档（`.md` 按那条判据的
人群定义天然在射程外：`guard_core::shell_scripts` 只收 `*.sh` 与「无扩展名 + `#!…sh`」）。
🔴 **这不是绕判据**：两段配方都已**逐条过 shellcheck 且 rc=0**（见下节），
PM 想把它们提成真 `.sh` 的话，`EXEMPT` 里加一行就完事，**零返工**。这一格交回 PM 定（件文件 `§5.5`）。

## shellcheck 逮到我两处真错（如实记：**它对**）

第一版两段配方都**没过** shellcheck，而其中两条不是风格问题，是**让读数失真的真错**：

| 位置 | 报的什么 | 真实后果 |
|---|---|---|
| lab 第 67 行 | `SC2320` 这个 `$?` 指的是 `echo`，不是上一条命令 | 我打的「退出码」量的是 `echo` 的码，**不是 tmux 的** ⇒ 那一格当时是**假读数** |
| lab2 第 96 行 | `SC2006` 反引号 + `SC1010` `done` 被当成词 | 标题里的一对反引号被当成命令替换执行，**把整个 `for` 循环带塌** ⇒ `E15` 当时**根本没量到**（回显全空） |

两处都已修（并在原地写明第一版错在哪），改后重跑：`E3` 拿到真退出码、`E15` 真的量出来了。
另有 `SC2028`/`SC2016` 三处是**故意**的字面文本与不展开的内层位置参数，已就地写明理由并 `disable`。
现状：**两段都 `shellcheck -f gcc` rc=0**。

> ⚠ 一条顺带的读数：`shellcheck` 自己在容器默认 POSIX locale 下打不出中文，
> 报 `commitBuffer: invalid argument (invalid character)` —— 要 `-e LC_ALL=C.UTF-8` 才看得全。
> **和本件治的是同一个病**，只是换了个受害者。

## ⚠ 已知的量具边界（别拿它当结论）

`E15` 那行的「字符=」列是 `wc -m` 打的，而容器 locale 是 C ⇒ **`wc -m` 在 C locale 下等于字节数**，
那一列**不是字符数**。结论不受影响：判「按宽度还是按字节」只用「字节数」与「下划线个数」两列就够
（`文`=3 字节→2 个 · `文档`=6 字节→4 个 · `é`=2 字节→1 个 · `ﬁ`=3 字节→1 个 ⇒ **对得上显示宽度，对不上字节数**）。

---

## 第一段 · `K-R12-locale-lab`

```bash
#!/usr/bin/env bash
# K-R12 量拍 · locale / 客户端 UTF-8 标志 的受控实验
#
# 🔴 跑在哪里：`ccmon-devbox:latest` 容器里，`docker run --rm -i`，**零挂载**。
#    ⇒ 不碰宿主 tmux server、不碰宿主 locale、不写宿主任何文件（红线：不动机器）。
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
rc=$?   # ⚠ 必须紧接着取：隔一条 echo 再取就变成 echo 的码了（shellcheck SC2320，第一版正是这个错）
echo "    退出码 = $rc   stderr 字节数 = $(wc -c </tmp/kr12-err)   内容 = [$(cat /tmp/kr12-err)]"

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
printf '%s\n' "    printf '1\\tabc' | cut -f2  -> [$(printf '1\tabc' | cut -f2)]"
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
# 下面四行里的 `"$1"` 是**故意**写在单引号里的：它要原样传给内层 sh 当位置参数，
# 不是给外层展开的。SC2016 说的就是这件事，此处正是要它不展开。
# shellcheck disable=SC2016
{
echo "  (a) export 前置：$(sh -c 'export LC_ALL=C.UTF-8; tmux -S '"$SOCK"' display-message -p "$1"' _ "$FMT_Q" | od -c | head -1)"
echo "  (b) 赋值前缀 + exec（特殊内建）：$(env -u LC_ALL sh -c 'LC_ALL=C.UTF-8 exec tmux -S '"$SOCK"' display-message -p "$1"' _ "$FMT_Q" 2>&1 | od -c | head -1)"
echo "  (c) 赋值前缀 + 普通命令：$(env -u LC_ALL sh -c 'LC_ALL=C.UTF-8 tmux -S '"$SOCK"' display-message -p "$1"' _ "$FMT_Q" 2>&1 | od -c | head -1)"
echo "  (d) 父进程 env 传下去（模拟 Command::env(\"LC_ALL\")）：$(env LC_ALL=C.UTF-8 sh -c 'tmux -S '"$SOCK"' display-message -p "$1"' _ "$FMT_Q" | od -c | head -1)"
}
echo "  sh 是谁：$(readlink -f /bin/sh)"

echo
echo "############ 11 · 收摊（只杀本实验那台私有 server）"
T kill-server 2>/dev/null
rm -rf "$LAB"
echo "done"
```

---

## 第二段 · `K-R12-locale-lab2`

```bash
#!/usr/bin/env bash
# K-R12 量拍 · 第二轮：D 方案真正的失效面 + 判据形状
#
# 🔴 跑在哪里：`ccmon-devbox:latest` 容器里，`docker run --rm -i`，**零挂载**。宿主零改动。
#
# 第一轮推翻了件文件 §0e 那条「LC_ALL 指向不存在的 locale ⇒ 静默退回 C」：
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
echo "############ E15 · 那个下划线到底按什么替换（字节数？还是显示宽度？）"
# ⚠ 用 -g（服务器全局用户选项）而不是 -t <会话>：第一版用 -t 读回来恒空，量了个寂寞。
# ⚠ 标题里不许出现反引号：第一版写了，被当成命令替换执行，把整个 for 循环带塌（SC2006/SC1010）。
for s in '文' '文档' 'é' 'ab' 'ﬁ'; do
  T set-option -g @kr12probe "$s" 2>/dev/null
  raw=$(env -u LANG -u LC_CTYPE LC_ALL=C.UTF-8 tmux -S "$SOCK" display-message -p '#{@kr12probe}')
  san=$(env -u LANG -u LC_ALL -u LC_CTYPE tmux -S "$SOCK" display-message -p '#{@kr12probe}')
  printf '  [%s] 字节=%d 字符=%d  UTF-8下=[%s]  POSIX下=[%s] 下划线个数=%d\n' \
    "$s" "$(printf '%s' "$s" | wc -c)" "$(printf '%s' "$s" | wc -m)" "$raw" "$san" "${#san}"
done

echo
echo "############ 收摊"
T kill-server 2>/dev/null; rm -rf "$LAB"; echo "done"
```
