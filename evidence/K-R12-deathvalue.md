# K-R12 真改拍 · §3 死值验（09-04，`k-r12c` 树，量于 `576a5af`）

> 🔴 **在哪儿跑的**：`docker run --rm -i --network none ccmon-devbox:latest sh -s`，**零挂载** ——
> 容器看不见项目、看不见宿主 `$HOME`、看不见 `/tmp/tmux-1000`。tmux server 一律起在容器内
> `-S /tmp/kr12dv/sock` 私有 socket 上，容器退出即消失。
> **宿主的 tmux 与 locale 一个字节都没动，也没有发出任何真 SSH。**
> 容器内：`tmux 3.4` · `locale -a` 只有 `C` `C.utf8` `POSIX`。
> 原始输出逐字在 `evidence/K-R12-deathvalue.out`；配方在本文件 §R（脚本已过 `shellcheck -s sh` **rc=0**）。

## §1 判据怎么切的

**只看输出本身**，不看配置。理由是上一拍量到的那条：tmux 判「客户端是不是 UTF-8」是对
`LC_ALL`→`LC_CTYPE`→`LANG` 的**第一个非空值**做**大小写不敏感的 `UTF-8`/`UTF8` 子串匹配**，
**从头到尾没问过 glibc** ⇒ **从配置文本推不出结果**（本文件 ④ 里 `zz_ZZ.UTF-8` 这个
根本不存在的 locale 是**干净**的，而 `C` 是**脏**的）。

- **段数** ＝ `awk -F<TAB> '{print NF}'`（分母 = 该格式串的列数）
- **真 TAB** ＝ 对整行做 `case "$line" in *"$TAB"*)`
- 展示时把 TAB 换成 `@` 好肉眼读；关键一格另打 `od -c` 做字节级复核

**「POSIX 客户端」＝ `env -u LANG -u LC_ALL -u LC_CTYPE`**（上一拍实测：什么都不设 ⇒ 脏）。
两趟 ①② **同一台 server、同一个会话、同一条命令**，只换客户端那一侧。

## §2 ① 改之前 / ② 改之后 —— 六处逐个，两个读数都在

| # | 调用点 | 列数 N | ① 改前（今天的生产写法） | ② 改后（本拍落的写法） |
|---|---|---|---|---|
| **S1** | `gate.rs list_sessions` | 3 | **段数 1 · 真TAB 无** `kr12_$0_cc-deadval1` | `-u` ⇒ **段数 3 · 真TAB 有** |
| **S2** | `gate.rs probe` | 3 | **段数 1 · 真TAB 无** `$0_cc-deadval1_1` | `-u` ⇒ **段数 3 · 真TAB 有** |
| **S3** | `watcher run_tmux_ls`（**有** `timeout` 那条分支 `:375`） | 6 | **段数 1 · 真TAB 无** | `LC_ALL=C.UTF-8` ⇒ **段数 6 · 真TAB 有** |
| **S3'** | 同上，**没有** `timeout` 那条分支（`:377`） | 6 | **段数 1 · 真TAB 无** | **同一行 `.env` 一起盖住** ⇒ **段数 6 · 真TAB 有** |
| **S4** | `watcher query_tmux_server` | 2 | **段数 1 · 真TAB 无** `15_/tmp/kr12dv/sock` | `LC_ALL=C.UTF-8` ⇒ **段数 2 · 真TAB 有** |
| **S5** | `tmux.rs list_remote_tmux` | 6 | **段数 1 · 真TAB 无** | `tmux -u ls` ⇒ **段数 6 · 真TAB 有** |
| **S6** | `tmux.rs build_guarded_tmux_cmd` 取值那条 | 2 | **段数 1 · 真TAB 无** `1_cc-deadval1` | `tmux -u display-message` ⇒ **段数 2 · 真TAB 有** |

⇒ **①「改之前六处里至少一处真的被改写」：六处全部被改写，一处不剩。**
⇒ **②「改之后不再被改写」：六处全部恢复，段数全部回到各自的 N。**

### 字节级复核（S5 那一行 `od -c`）

```
① 改前： k r 1 2  _  / t m p / k r 1 2 d v / _ _ _ _ / p r o j _ b a s h _ 0 _ 1 _ c c - d e a d v a l 1 \n
② 改后： k r 1 2 \t  / t m p / k r 1 2 d v / 346 226 207 346 241 243 / p r o j \t b a s h \t 0 \t 1 \t c c - …
```

**两半病在这一对里同时可见**：改前六个 `\t` 全没了（分隔符被吞），**而且 `文档` 那 6 个字节
变成了 4 个 `_`**（内容被改写 —— 按**显示宽度**替换，不按字节数）。改后 `346 226 207 346 241 243`
就是 `文档` 的原始 UTF-8 字节，一个不差。

### ③ 反向死值：`-u` 放错位置必须**响**

```
tmux ls -u -F ...            rc=1  command list-sessions: unknown flag -u
tmux display-message -u -p   rc=1  command display-message: unknown flag -u
```

⇒ 这就是本拍把「位置」单独立成一条判据的理由：`-u` 必须在**子命令之前**。
🔴 **而这个「响」在本仓两处会被压成静默**：`gate.rs` 两处**刻意不看退出码**、
`tmux.rs` 那条串把 stderr `2>/dev/null` 吞掉且不看 rc
⇒ 放错位置在生产里会退化成「一个会话都没有」/「会话不存在」。**所以判据必须钉位置，不能只钉存在。**

### ④ `LC_ALL` 那两档，复核过

| 设成什么 | 结果 | 说明 |
|---|---|---|
| `LC_ALL=`（**设了但空串**） | **段数 1 · 脏** | 落回 `LC_CTYPE`/`LANG`。写 `LC_ALL=${VAR}` 而 `VAR` 未定义就是这一档 |
| `LC_ALL=zz_ZZ.UTF-8`（**locale 根本不存在**） | **段数 6 · 干净** | tmux 不问 glibc，只做子串匹配 |

⇒ 🔴 **顺带把「`C.UTF-8` 万一没装怎么办」这个顾虑消掉了**：tmux 不查它装没装。
（本容器 `locale -a` 只有 `C` / `C.utf8` / `POSIX`，`C.UTF-8` 这个**带连字符的写法本身就不在表里**，
而它照样干净 —— 这一格就是证据。）

## §3 `J1` 那一刀：让段数真的下溢一次，断言它红

死值不是编的，是上面 ① 那几行**真 tmux 打出来的字节**原样搬进 Rust 测试。四条判据：

| 判据 | 住哪 | 喂什么 | 断言 |
|---|---|---|---|
| `a_dirty_tmux_channel_is_unobservable_never_sessions` | `watcher.rs` | ①/S5 那行脏字节 | `Unobservable`，**不是** `Sessions` |
| 同上第二段 | `watcher.rs` | 先干净后脏 | `diff_closed` **一个会话都不报死**、快照原样保留 |
| `the_underflow_predicate_only_fires_downward` | `watcher.rs` | 1/5/6/7 段 | 下溢红、恰好绿、**过溢绿** |
| `the_underflow_predicate_catches_the_real_dirty_bytes` | `gate.rs` | ①/S1、①/S2 那两行 | 同上，且 `@ccm_sid` **未设**（`$0\t\t1`）不许判红 |
| `a_dirty_line_underflows_and_an_overflowing_line_is_still_dropped_today` | `tmux.rs` | 脏/干净/过溢 | 判据 + `parse_tmux_ls` 的实际处置 |

**第二段是本组里最要紧的一格**：它量的不是「有没有那行 `if`」，是**后果**。
通道一脏，`session_names` 只取第 1 段、永远取得到 ⇒ 它把**整行**当会话名
⇒ 下一轮 `diff_closed` 会把**所有活着的会话**算成「消失了」并 retire 掉。
判成 `Unobservable` 之后这条路是「什么都不结论、快照不动」。

## §4 变异验：这些判据真的会红吗（不红的死值不是死值）

同一棵树上把三处保护各删一次，跑同一批判据（跑完已逐条改回，`git diff` 复核只剩测试新增）：

| 变异 | 结果 |
|---|---|
| **M1** `classify_tmux_probe` 去掉下溢那一臂 | `a_dirty_tmux_channel_is_unobservable_never_sessions` **FAILED**（`right: Unobservable`） |
| **M2** `gate.rs list_sessions` 去掉 `-u` | `both_tmux_call_sites_ask_for_a_utf8_client_before_the_subcommand` **FAILED** |
| **M3** 门那条 `display-message` 去掉 `-u` | `both_cross_ssh_tmux_reads_…` **FAILED** · `the_prevention_layer_and_the_door_layer_coexist_…` **FAILED** |

### 🔴 M3 那一格顺带买到了 ㈢ 要的证据

**M3 之下，`K-R23` 的门测试 `a_dirty_channel_that_lost_the_tab_must_not_open_the_door` 照样 `ok`。**

这不是巧合，是两件事本来就该分开：`K-R23` 喂的是「假设通道已经脏了」，它**结构上看不见**
`-u` 在不在；`K-R12` 的判据管的是「通道会不会脏」，它**看不见**门拒不拒。
⇒ **两层各有各的判官，删掉任一层，另一层的判据都不会替它兜底。**
这正是「两件的处置必须能共存」在判据面上的样子。

## §R 配方（脚本原文）

⚠ **刻意不落成 `evidence/*.sh`**：`shell_lint_registry` 要求仓里每个 shell 脚本要么进 CI 的
shellcheck 表达式、要么登记豁免，而 `.github/workflows/ci.yml` 与 `src-tauri/src/shell_lint_registry.rs`
**都不在本拍写区**（上一拍 `§5.5` 一 已被这条判据逮过一次，处置照旧）。
本段**已过 `shellcheck -s sh` rc=0**；PM 若要它当可执行量具留仓，`EXEMPT` 加一行即可，零返工。

跑法：`docker run --rm -i --network none ccmon-devbox:latest sh -s < 本段`

```sh
set -u

T=$(printf '\t')
echo "== 环境 =="
tmux -V
echo "locale -a: $(locale -a | tr '\n' ' ')"
echo

ROOT=/tmp/kr12dv
SOCK=$ROOT/sock
CWD="$ROOT/文档/proj"          # 刻意带非 ASCII —— 顺带看「内容被改写」那一半
mkdir -p "$CWD"

# 生产格式串，从三个源文件逐字抄来（TAB 用 $T 拼，免得看不见）
FMT_LS="#{session_name}${T}#{pane_current_path}${T}#{pane_current_command}${T}#{?session_attached,1,0}${T}#{session_windows}${T}#{@ccm_sid}"
FMT_GATE_LIST="#{session_name}${T}#{session_id}${T}#{@ccm_sid}"
FMT_GATE_PROBE="#{session_id}${T}#{@ccm_sid}${T}#{session_windows}"
FMT_QUERY="#{pid}${T}#{socket_path}"
FMT_DOOR="#{session_windows}${T}#{@ccm_sid}"

# server 起在私有 socket 上。⚠ 建 server 的这一次**不是被测对象**，它只是把台子搭起来。
tmux -S "$SOCK" -u new-session -d -s kr12 -c "$CWD" 2>&1 || { echo "起不来 tmux"; exit 9; }
tmux -S "$SOCK" -u set-option -t kr12 @ccm_sid cc-deadval1 2>&1
echo "台子：会话 kr12，cwd=$CWD，@ccm_sid=cc-deadval1"
echo

verdict() {
  line=$(head -n 1)
  nf=$(printf '%s\n' "$line" | awk -F"$T" '{print NF}')
  case "$line" in
    *"$T"*) tab=有 ;;
    *)      tab=无 ;;
  esac
  printf '  段数=%-2s 真TAB=%s  ⇒ %s\n' "$nf" "$tab" "$(printf '%s' "$line" | tr "$T" '@')"
}

posix() { env -u LANG -u LC_ALL -u LC_CTYPE "$@"; }

echo "############ ① 改之前（今天的生产写法，POSIX 客户端）############"
echo "S1 gate.rs list_sessions   tmux list-sessions -F ..."
posix tmux -S "$SOCK" list-sessions -F "$FMT_GATE_LIST" | verdict
echo "S2 gate.rs probe           tmux display-message -p -t ..."
posix tmux -S "$SOCK" display-message -p -t '=kr12:' "$FMT_GATE_PROBE" | verdict
echo "S3 watcher run_tmux_ls     sh -c 'exec timeout -s KILL 5 tmux ls -F ...'（有 timeout 那条分支）"
posix sh -c "exec timeout -s KILL 5 tmux -S '$SOCK' ls -F '$FMT_LS' 2>/dev/null" | verdict
echo "S3' 同上，**没有 timeout 那条分支**（:377 —— 漏了它在装了 timeout 的机器上永远看不见）"
posix sh -c "exec tmux -S '$SOCK' ls -F '$FMT_LS' 2>/dev/null" | verdict
echo "S4 watcher query_tmux_server sh -c 'exec tmux display-message -p ...'"
posix sh -c "exec tmux -S '$SOCK' display-message -p '$FMT_QUERY' 2>/dev/null" | verdict
echo "S5 tmux.rs list_remote_tmux（生产命令串逐字，只多一个 -S 私有 socket）"
posix sh -c "if command -v tmux >/dev/null 2>&1; then tmux -S '$SOCK' ls -F '$FMT_LS' 2>/dev/null || true; else printf 'NO_TMUX\n'; fi" | verdict
echo "S6 tmux.rs build_guarded_tmux_cmd 里那条取值 display-message"
posix sh -c "tmux -S '$SOCK' display-message -p -t '=kr12:' '$FMT_DOOR' 2>/dev/null" | verdict
echo
echo "  —— 字节级复核（S5 那行 od -c，头 3 行）——"
posix tmux -S "$SOCK" ls -F "$FMT_LS" | head -1 | od -c | head -3
echo

echo "############ ② 改之后（本拍落的写法，同一台 server、同一个 POSIX 客户端）############"
echo "S1 gate.rs   -u 在子命令之前"
posix tmux -S "$SOCK" -u list-sessions -F "$FMT_GATE_LIST" | verdict
echo "S2 gate.rs   -u 在子命令之前"
posix tmux -S "$SOCK" -u display-message -p -t '=kr12:' "$FMT_GATE_PROBE" | verdict
echo "S3 watcher   LC_ALL=C.UTF-8 挂在 sh 上（有 timeout 分支）"
posix env LC_ALL=C.UTF-8 sh -c "exec timeout -s KILL 5 tmux -S '$SOCK' ls -F '$FMT_LS' 2>/dev/null" | verdict
echo "S3' watcher  **同一行 env** 也盖住了没有 timeout 那条分支 ← 这就是不用 -u 的全部理由"
posix env LC_ALL=C.UTF-8 sh -c "exec tmux -S '$SOCK' ls -F '$FMT_LS' 2>/dev/null" | verdict
echo "S4 watcher   LC_ALL=C.UTF-8 挂在 sh 上"
posix env LC_ALL=C.UTF-8 sh -c "exec tmux -S '$SOCK' display-message -p '$FMT_QUERY' 2>/dev/null" | verdict
echo "S5 tmux.rs   tmux -u ls -F ..."
posix sh -c "if command -v tmux >/dev/null 2>&1; then tmux -S '$SOCK' -u ls -F '$FMT_LS' 2>/dev/null || true; else printf 'NO_TMUX\n'; fi" | verdict
echo "S6 tmux.rs   tmux -u display-message -p -t ..."
posix sh -c "tmux -S '$SOCK' -u display-message -p -t '=kr12:' '$FMT_DOOR' 2>/dev/null" | verdict
echo
echo "  —— 字节级复核（S5 那行 od -c，头 3 行）——"
posix tmux -S "$SOCK" -u ls -F "$FMT_LS" | head -1 | od -c | head -3
echo

echo "############ ③ 反向死值：把 -u 放错位置，必须**响**（不是静默）############"
o=$(posix tmux -S "$SOCK" ls -u -F "$FMT_LS" 2>&1); echo "  tmux ls -u -F ...            rc=$? out=[$o]"
o=$(posix tmux -S "$SOCK" display-message -u -p "$FMT_QUERY" 2>&1); echo "  tmux display-message -u -p   rc=$? out=[$o]"

echo "############ ④ LC_ALL 的两档静默失效（件文件 §5.2 的 (a) 档，实测复核）############"
posix env LC_ALL= sh -c "exec tmux -S '$SOCK' ls -F '$FMT_LS' 2>/dev/null" | verdict
echo "  ↑ LC_ALL 设了但是**空串** ⇒ 落回 LC_CTYPE/LANG ⇒ 仍然脏（写 LC_ALL=\${VAR} 而 VAR 未定义就是这一档）"
posix env LC_ALL=zz_ZZ.UTF-8 sh -c "exec tmux -S '$SOCK' ls -F '$FMT_LS' 2>/dev/null" | verdict
echo "  ↑ locale **根本不存在**但长相带 UTF-8 ⇒ 干净（tmux 不问 glibc，只做子串匹配）"

tmux -S "$SOCK" kill-server 2>/dev/null
echo
echo "== 完 =="
```

## §5 自省：第一趟我踩了上一拍**同一个**反引号坑

第一趟跑出来的输出里夹着一行 `sh: 1: §5.2: not found` —— ④ 那个标题我写成了
`（\`§5.2\` 的 (a) 档）`，双引号里的**一对反引号被当成命令替换执行**了。
**这与上一拍 `§5.5` 二 被 `shellcheck` 逮到的 `SC2006`/`SC1010` 是同一个形状**
（那次它把整个 `for` 循环带塌，`E15` 根本没量到）。

- **这一次坏到什么程度：只吃掉了标题里的一小段文字，没有影响任何一格读数**
  （被吞的是 `echo` 的一部分，不是被测命令）。判完已修，**重跑一趟**，本文件与 `.out` 都是重跑后的。
- **处置**：把脚本先过一遍 `shellcheck -s sh`（rc=0）再当读数用。
  ⚠ 那次教训我读到了、也复述了，**还是又犯了一次** —— 复述不等于装上判据。
  下一拍起：`evidence/` 里任何要拿来当读数的 shell，**先 shellcheck，再跑**。
