#!/bin/bash
# U9a「保住清单差分对拍」：把主计划 S10 里那七条散文式的「U9 之后必须保住」，
# 变成会红的判据。跑的是**真 `shared/ccm`**，不是 shim、不是手搓字符串。
#
# ## 为什么单独一套，而不是塞进既有五套
#
# 既有五套各有各的题目：`ccm-cli` 验 CLI 语法契约（全走 `--print`、全 `env -u TMUX`）、
# `ccm-print-parity` 验「渲染器的意图能被 ccm 接住」、`ccm-acceptance` 验真 tmux 行为、
# `ccm-pretrust` 验信任写入、`cc-spawn-uplift` 验 cc-spawn 那条路。
# **没有一套比对「`--print` 说的」与「真跑做的」**——而那正是 U9b 搬决策时最容易漏的地方：
# `--print` 那段与真 exec 那段（`shared/ccm` 里 `do_print` 分支 vs 其后的「非容器路径」段）
# 是两份手写副本，搬一份漏一份，今天不会红。**行号刻意不写** —— 它们本轮就漂了三次。
#
# ## 三组
#
# - **A 组 print↔exec 环境差分**：同一组 flag，`--print` 那条串跑出来的环境，
#   必须与真跑出来的环境在「ccm 受控的那几个键」上逐字相等。
#   ★ 本组开张时就抓到一条真的：codex + 已在 tmux 内时，exec 路 `export CC_BUS_ID`
#     而 `--print` 只字未提（实测 `cd '…' && exec codex`）。
# - **B 组 `CCM_ENV`**：S10 七项里**唯一全仓零覆盖**的一项（摸底 `grep -rn CCM_ENV`
#   只命中 ccm 自己与计划文档）。它是「真正非 shell 不可」的那一条，U9b 之后也必须还在。
# - **C 组 `--ccm-probe` 契约**：`src-tauri/src/ccm_probe.rs::parse_probe_output` 靠**字面** `name=ccm`
#   判「装没装」，`src/launch-render-cli.ts::CLI_REQUIRED_CAPS` 靠 `capabilities=` 决定
#   走 CLI 渲染器还是兜底。两处都只对**手写 fixture** 测过。
#   ⚠ 精确说法（审计订正）：真脚本的 probe 输出**并非全无覆盖** —— `cc-spawn-uplift` 主流程
#   不设 `CCM_BIN`，于是 `cc-spawn` 解析到真 `shared/ccm` 并对 `detach`/`tmux-size` 两项
#   fail-closed，那 21 条间接盖住了这两项。**零覆盖的是**：首行 `name=ccm` · `version=` ·
#   `agents=` · TS 侧那 7 项 `CLI_REQUIRED_CAPS`。少一项能力 ⇒ app 静默退到兜底渲染器
#   （丢账号保真度），用户看不见。
#
# ## 差分不能单独用（血泪 10 的形状）
#
# 「print == exec」两边一起坏掉时是绿的。所以每一条保住项**同时**有一条**绝对断言**
# （「它必须在」），差分只负责「两份副本不许分家」。
#
# 跑法：bash e2e/ccm-contract-parity.sh   （npm run test:ccm-contract-parity）
set -o pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
CCM="$REPO/shared/ccm"

# ★★ **fail-closed：本套件对 `jq` 是硬依赖**〔K-C1 D 阶段审计 `I4`，08-24 补〕。
#   `K-C1` 起 `_acct_prefix` 用 `jq` 把夹具 manifest 翻成 `--list-accounts` 帧形状；
#   缺 `jq` 它**不报错、只少吐账号行** ⇒ ccm 拿到空表，症状是 `可用: (无账号库)`，
#   **诊断指向账号库、不指向缺 jq**。照同目录先例（`cc-bus-queue-drain.sh`/`daemon-cc-bus.sh`）当场停。
command -v jq >/dev/null 2>&1 || {
  echo "需要 jq —— 本套件的 daemon stub 靠它把夹具 manifest 翻成 --list-accounts 帧形状；"
  echo "     缺它会静默少吐账号行（症状看着像「账号库坏了」）。这是环境缺工具，不是套件退化。"
  exit 1; }

PASS=0; FAIL=0
ck() { if [ "$2" = "$3" ]; then printf 'PASS | %s\n' "$1"; PASS=$((PASS+1))
       else printf 'FAIL | %s\n      期望: %s\n      实得: %s\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }

W="$(mktemp -d)"; trap 'rm -rf "$W"' EXIT
mkdir -p "$W/bin" "$W/proj" "$W/acct-z" "$W/acct-b" "$W/home"
CWD="$W/proj"

# ===== tmux shim：**绝不碰用户真 tmux server** =====
# 只需要两件事：`display-message -p '#S'` 回一个会话名（CC_BUS_ID 派生源），
# 其余（set-option 等）一律吞掉回 0。PATH 前置 ⇒ ccm 与它起的 poller 都只看得到这份。
cat > "$W/bin/tmux" <<'SHIM'
#!/bin/sh
if [ "$1" = "display-message" ]; then printf 'faux-sess\n'; fi
exit 0
SHIM
chmod +x "$W/bin/tmux"

# ★★ `U-NP④`（2026-08-14）：**本套件必须让 ccm「查得到 daemon」，否则每一条真跑都会被
#     身份前置检查挡下**（用户裁定「ccm做到必须走daemon」：在 tmux 里起 claude 而找不到
#     daemon ⇒ exit 2）。而 `base_env` 恰恰同时满足那三个条件（`TMUX` 有值 · agent=claude ·
#     `HOME` 是空的沙箱）。
# ⇒ 在**部署落点**放一份「读完 stdin、什么都不答」的 stub。它同时保住了本套件原有的两组语义：
#   · 身份前置检查：**查得到** ⇒ 放行（本套件不测身份，测的是 argv/env 平价）；
#   · `resume` 那条路：daemon **答不出命令** ⇒ 照旧落回本地那条（"诚实降级"那几条判据要的就是这个）。
# ⚠ 别改成「不放 stub」——那测的就不再是平价，而是身份检查会不会把整套件打红。
# ★★ `K-C1`（08-24）：这些 stub 现在还要会答 **`--list-accounts`** ——
#   账号解析从「ccm 自己读 manifest」改成「问 daemon」之后，本套件每一条真跑都会先问它一次。
#   不答的话 ccm 会**降级读文件并往 stderr 说一句**（那是 `§0b` 裁的行为，不是 bug）,
#   于是本套件测的就不再是生产形状了（生产上 daemon 在位）。
#   ⇒ 给 stub 加一段 `--list-accounts` 前缀：把 `<accts-dir>/accounts.json` 原样翻成帧形状。
#   **单一事实源仍是那份 manifest**，不在 stub 里手抄账号表。
_acct_prefix() { cat <<'PRE'
if [ "$1" = --list-accounts ]; then
  d=""; while [ $# -gt 0 ]; do [ "$1" = --accts-dir ] && d="$2"; shift; done
  printf '{"accountZeroAware":true,"acctsDir":"%s","count":0,"enabled":true,"error":null,"kind":"accounts-meta","manifestPath":"%s/accounts.json","sharedStore":null,"updatedAt":null}\n' "$d" "$d"
  jq -c '.accounts[] | {configDir:(.configDir // null),email:"",exists:true,isDefault:(.isDefault // false),loggedIn:false,mode:"isolated",name:.name}' "$d/accounts.json" 2>/dev/null
  exit 0
fi
PRE
}
mkdir -p "$W/home/.cc-monitor/bin"
_null_daemon() { # 把部署落点恢复成「答得出账号、答不出 resume 命令」的那份（A′e 会临时覆盖它）
  { printf '#!/bin/sh\n'; _acct_prefix; printf 'cat >/dev/null\nexit 0\n'; } \
    > "$W/home/.cc-monitor/bin/cc-monitor-remote"
  chmod +x "$W/home/.cc-monitor/bin/cc-monitor-remote"
}
_null_daemon

cat > "$W/accounts.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$W/acct-z", "isDefault": true },
  { "name": "b", "configDir": "$W/acct-b", "isDefault": false } ] }
JSON

# ===== 受控环境 =====
# `TMUX` **要设**（这是与既有五套的关键差别：它们全 `env -u TMUX`，于是 CC_BUS_ID
# 那条分支从来没被任何判据走到过）。`CLAUDECODE`/`CLAUDE_CODE_ENTRYPOINT` 预置成有值，
# 这样「嵌套 env 要被 unset」才有可观测的差别（否则两边都是「本来就没有」）。
BASE_EXTRA=()
base_env() {
  # 四个嵌套标记**全部显式置值**：不这么做的话，跑在 Claude Code 里时其中两个是从
  # 开发者环境漏进来的真值、跑在 CI 上时压根不存在 ⇒ 「claude 清得干净」这条断言的
  # 宽度会随环境变（本机 4 个、CI 1 个），是典型的环境依赖型假绿。
  # `HOME` 也换掉：`--base` 那格会 unset `CLAUDE_CONFIG_DIR`，ccm 的身份回填 poller
  # 于是回落到 `$HOME/.claude/sessions/<pid>.json`。今天只是一次只读 `openat`（ENOENT），
  # 但「测试进程摸到用户真实数据目录」这件事本身不该靠「它恰好只读」来保证。
  env -u CLAUDE_CONFIG_DIR -u ANTHROPIC_MODEL -u CC_BUS_ID -u CCM_ENV -u CCM_ENV_PROBE \
      CLAUDECODE=1 CLAUDE_CODE_ENTRYPOINT=cli \
      CLAUDE_CODE_SESSION_ID=fake-sid CLAUDE_CODE_CHILD_SESSION=1 \
      TMUX=/faux/socket,1,0 PATH="$W/bin:$PATH" HOME="$W/home" \
      CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$W/accounts.json" \
      "${BASE_EXTRA[@]}" "$@"
}

# ccm 受控的键（其余如 PATH/HOME 是宿主噪声，比了只会假红）。
CCM_KEYS='^(CLAUDE_CONFIG_DIR|ANTHROPIC_MODEL|CC_BUS_ID|CLAUDECODE|CLAUDE_CODE_ENTRYPOINT|CLAUDE_CODE_SESSION_ID|CLAUDE_CODE_CHILD_SESSION|CCM_ENV_PROBE)='
ccm_keys() { grep -E "$CCM_KEYS" | LC_ALL=C sort | tr '\n' '|'; }

# 真跑：`--launcher env` ⇒ 最终 `exec env` ⇒ stdout 就是**真实**环境。
# 用文件重定向而非 `$(...)`：claude 那条路会留一个身份回填 poller 在后台，
# 命令替换会等它关掉 stdout（多等 1 秒/次）。
actual_env() {
  base_env bash "$CCM" --cwd "$CWD" --launcher env "$@" > "$W/a.out" 2>&1
  ccm_keys < "$W/a.out"
}

# 预言：同一组 flag 的 `--print` 串，在**同一个基础环境**里跑一遍。
predicted_env() {
  base_env bash "$CCM" --cwd "$CWD" --launcher env "$@" --print > "$W/p.line" 2>&1
  base_env bash -c "$(cat "$W/p.line")" > "$W/p.out" 2>&1
  ccm_keys < "$W/p.out"
}

pair() { # pair <标签> <flags…>
  local label="$1" a; shift
  a="$(actual_env "$@")"
  # **差分自检**：两边都空的时候差分是绿的（ccm 在这条 flag 组合上整体 die 就是这个形状）。
  # 与 C 组的「抽取器自检」同形 —— 先证明「有东西可比」，再比。
  ck "A · 真跑确实产出了环境（差分自检）：$label" "yes" "$([ -n "$a" ] && echo yes || echo no)"
  ck "A · print↔exec 环境一致：$label" "$a" "$(predicted_env "$@")"
}

echo "===== A 组：--print 说的 == 真跑做的 ====="
# ★ 这一格是本套件的开张理由：修复前 exec 侧有 CC_BUS_ID=faux-sess、print 侧没有。
pair "codex（CC_BUS_ID 派生）"            --agent codex
pair "claude（嵌套 env 清理）"             --agent claude
pair "claude + --account b"               --agent claude --account b
pair "claude + --model opus"              --agent claude --model opus
pair "codex + --account b + --model opus" --agent codex --account b --model opus

# `--base` 要有意义，基础环境里必须**先有**一个 CLAUDE_CONFIG_DIR 让它去 unset。
# **必须再带一个 `--model`**：`claude + --base` 单独跑的话，受控键集合会被清成**空集**
# （config_dir 被 unset、四个嵌套标记被 unset、codex 专属的 CC_BUS_ID 又不适用）⇒
# 差分退化成 `"" == ""`。上面那条自检就是逮到这个的（第一次跑当场红）。
BASE_EXTRA=(CLAUDE_CONFIG_DIR="$W/acct-z")
pair "claude + --base + --model（#75 逃生口）" --agent claude --base --model opus
BASE_EXTRA=()

echo
echo "===== A″ 组：账号解析走 daemon 之后，**print 路与 exec 路必须同源**（K-C1）====="
# ★ 为什么 A 组盖不住这一格：A 组的 `pair "claude + --account b"` 比的是 print↔exec 的**一致**,
#   而账号解析改走 daemon 之后，两条路**各自**都要问一次 daemon（`--print` 那条也问 ——
#   `config_dir` 是**值**、逐字进命令串，没法像 `BUS_ID_RECIPE` 那样推迟求值）。
#   ⇒ 只要两条路一起坏（比如两条都退回读文件），A 组照样全绿。
# ⇒ 本组的夹具让 **daemon 与文件答不同的目录**，于是「拿到哪一个」直接说出它走了哪条路。
mkdir -p "$W/acct-daemon-b"
cat > "$W/bin/dm-acct" <<STUB
#!/bin/sh
if [ "\$1" = --list-accounts ]; then
  printf '%s\n' '{"accountZeroAware":true,"acctsDir":"x","count":1,"enabled":true,"error":null,"kind":"accounts-meta","manifestPath":"x","sharedStore":null,"updatedAt":null}'
  printf '%s\n' '{"configDir":"$W/acct-daemon-b","email":"","exists":true,"isDefault":true,"loggedIn":false,"mode":"isolated","name":"b"}'
  exit 0
fi
cat >/dev/null
exit 0
STUB
chmod +x "$W/bin/dm-acct"
# ★★ 〔`K-P2` `F` 拍 09-04；用@「ccm不要管找不到, 统一走后端」〕**第二份后端 —— 反向那一对的新对照。**
# 账号解析没有本地退路了 ⇒ 从前那一对「`CCM_NO_DAEMON=1` ⇒ 落回 manifest 那份」今天读到的是
# `exit 4`，它证不了 provenance。而它要买的是「上一对不是恒真」——「值真的跟着后端的答案走」。
# ⇒ 换一份**答另一套目录**的后端：同一条代码路径，只有输入不同 —— provenance 正是后者。
sed "s|$W/acct-daemon-b|$W/acct-b|" "$W/bin/dm-acct" > "$W/bin/dm-acct2"
chmod +x "$W/bin/dm-acct2"
# ★ 自检必须问「daemon **实际答了什么**」，不是比两个路径字面量 —— 那两个字符串恒不相等,
#   于是「把假 daemon 改成答与 manifest 相同的目录」这一刀在它眼里毫无变化 ⇒ 恒绿。
#   （`e2e/ccm-cli.test.sh` 那节的同款自检 08-24 就是这么栽的，这里一起改。）
ck "A″ · 夹具自检：daemon **实际答的** b 的 configDir 与 manifest 里那个刻意不同" "differ" \
   "$(_mf="$(jq -r '.accounts[]|select(.name=="b")|.configDir' "$W/accounts.json" 2>/dev/null)"
      _dm="$("$W/bin/dm-acct" --list-accounts --accts-dir "$W" 2>/dev/null \
             | jq -r 'select(.name=="b")|.configDir' 2>/dev/null)"
      if [ -z "$_mf" ] || [ -z "$_dm" ]; then echo "抽取器坏了:[mf=$_mf][dm=$_dm]"
      elif [ "$_mf" != "$_dm" ]; then echo differ; else echo "same:[$_dm]"; fi)"
_a2="$(base_env CCM_DAEMON_BIN="$W/bin/dm-acct" bash "$CCM" --cwd "$CWD" --launcher env --account b 2>/dev/null \
        | grep '^CLAUDE_CONFIG_DIR=')"
ck "★ A″ · **exec 路**：--account b 的 configDir 来自 daemon（不是 manifest）" \
   "CLAUDE_CONFIG_DIR=$W/acct-daemon-b" "$_a2"
base_env CCM_DAEMON_BIN="$W/bin/dm-acct" bash "$CCM" --cwd "$CWD" --launcher env --account b --print \
  > "$W/a2.line" 2>/dev/null
ck "★ A″ · **print 路**跑出来的是同一个值（两条路同源；只钉 exec 路时「print 退回读文件」会存活）" \
   "CLAUDE_CONFIG_DIR=$W/acct-daemon-b" \
   "$(base_env bash -c "$(cat "$W/a2.line")" 2>/dev/null | grep '^CLAUDE_CONFIG_DIR=')"
# ★ 反向那一对〔`F` 拍 09-04 换了对照物〕：同一夹具、**换一份后端** ⇒ **两条路都**跟着换。
#   只有正向那一对时，「值恒是那个」也能全绿。
#   ⚠ 从前这里换的是「有没有后端」（`CCM_NO_DAEMON=1` ⇒ 落回文件）—— 那是**两条不同的代码路径**；
#     今天换的是「后端说什么」（同一条路径，只有输入不同），而 provenance 正是后者。
ck "A″ · 反向（exec 路）：**换一份后端** ⇒ 值跟着换" "CLAUDE_CONFIG_DIR=$W/acct-b" \
   "$(base_env CCM_DAEMON_BIN="$W/bin/dm-acct2" bash "$CCM" --cwd "$CWD" --launcher env --account b 2>/dev/null \
        | grep '^CLAUDE_CONFIG_DIR=')"
base_env CCM_DAEMON_BIN="$W/bin/dm-acct2" bash "$CCM" --cwd "$CWD" --launcher env --account b --print \
  > "$W/a2b.line" 2>/dev/null
ck "A″ · 反向（print 路）：同样跟着换（两条路同源）" "CLAUDE_CONFIG_DIR=$W/acct-b" \
   "$(base_env bash -c "$(cat "$W/a2b.line")" 2>/dev/null | grep '^CLAUDE_CONFIG_DIR=')"
# 🔴 **补一格：`CCM_NO_DAEMON=1` 不再是逃生口** —— 它现在只是把失败原因换了一格。
#   这一格是上面那一对的**第三条腿**：它把「关掉后端会怎样」这个问题的今天的答案钉住，
#   免得下一个人照旧以为「关掉 ⇒ 落回文件」。
BASE_EXTRA=(CCM_NO_DAEMON=1)
base_env CCM_DAEMON_BIN="$W/bin/dm-acct" bash "$CCM" --cwd "$CWD" --launcher env --account b \
  > "$W/a2c.out" 2> "$W/a2c.err"; _a2c_rc=$?
ck "A″ · CCM_NO_DAEMON=1 ⇒ **响亮失败**（rc=4 唯一失败面，不再落回 manifest）" "4" "$_a2c_rc"
ck "A″ · 而且说的是**那一格**（明示整条关掉 ≠ 找不到）" "yes" \
   "$(grep -q 'CCM_NO_DAEMON=1（明示整条关掉 daemon）' "$W/a2c.err" && printf yes || printf no)"
BASE_EXTRA=()

echo
echo "===== A′ 组：print↔exec 的 **argv** 一致（A 组只比 env，且六格全是 new）====="
# ★ 这一组的开张理由是一个**存活的反例**（F06b-1b，2026-08-04）：
# 往 `--print` 的非容器出口里塞一句「现在就去问 daemon 要 command」，打印串从
# `exec claude --resume abc-123` 变成 `exec claude --resume STUB-FROM-DAEMON`
# —— 而 `ccm-print-parity`(12) 与本套件(31) **两套全绿**。
#
# 两个洞，都真：
#   ① `ccm-print-parity` 的 resume 场景全是 `--tmux` 的，`--print` 只展示**外层 tmux
#      编排命令**，在容器出口就 exit 了 ⇒ **非容器 resume 的打印路一条判据都没走到过**。
#   ② A 组比的是 `ccm_keys`（**环境键**），argv 分家它看不见；六个 pair 又全是默认动作
#      `new`，**resume 那条路本身没有任何 print↔exec 差分**。
#
# ⇒ 于是「把 `--resolve` 接进 resume」这件事今天**没有任何安全网**：print 与 exec 各接
# 一半、或只接一边，两套 e2e 都会安静地绿。本组就是接线之前先立的那张网。
# ⚠ 同 A 组：差分不能单独用（两边一起坏掉时是绿的）⇒ 每格都配一条**绝对断言**。
cat > "$W/bin/argvstub" <<'STUB'
#!/usr/bin/env bash
printf 'ARGV|%s\n' "$*"
STUB
chmod +x "$W/bin/argvstub"

# 真跑：`--launcher argvstub` ⇒ 最终 `exec argvstub …` ⇒ stdout 就是**真实** argv。
actual_argv() {
  # ⚠ 动作（resume/new/attach）**必须是第一个位置参数**，所以 `"$@"` 排在 flag 前面
  #   —— 第一版写反了，ccm 当场 die「多余的位置参数」，被下面那条差分自检逮住。
  base_env bash "$CCM" "$@" --cwd "$CWD" --launcher "$W/bin/argvstub" > "$W/aa.out" 2>&1
  grep '^ARGV|' "$W/aa.out" | head -1
}
# 预言：同一组 flag 的 `--print` 串，在同一个基础环境里跑一遍。
predicted_argv() {
  base_env bash "$CCM" "$@" --cwd "$CWD" --launcher "$W/bin/argvstub" --print > "$W/pa.line" 2>&1
  base_env bash -c "$(cat "$W/pa.line")" > "$W/pa.out" 2>&1
  grep '^ARGV|' "$W/pa.out" | head -1
}
pair_argv() { # pair_argv <标签> <flags…>
  local label="$1" a; shift
  a="$(actual_argv "$@")"
  # 差分自检：两边都空时差分是绿的（ccm 整体 die 就是这个形状）。先证明「有东西可比」。
  ck "A′ · 真跑确实产出了 argv（差分自检）：$label" "yes" "$([ -n "$a" ] && echo yes || echo no)"
  ck "A′ · print↔exec argv 一致：$label" "$a" "$(predicted_argv "$@")"
}
pair_argv "resume（本组的正题：F06b 要接 --resolve 的就是这条）" resume abc-123 --agent claude
pair_argv "resume + --model（修饰不许只落一边）"                  resume abc-123 --agent claude --model opus
pair_argv "new（对照组：证明差分不是只对 resume 有效）"            --agent claude

# ===== A′-daemon：daemon 在位时的那条路（F06b-1c 接的就是它）=====
# ⚠ 上面三对**看不见新路**：它们靠 `--launcher argvstub` 才能观察 argv，而 ccm 里
#   「显式 `--launcher` 优先于 daemon 建议」⇒ 一给 launcher 就绕开 daemon 了。
#   ⇒ 观察 daemon 那条路只能换个法子：**让假 daemon 自己回一条以 argvstub 为首的命令**。
#   这条是接线当天就发现的洞（网立好了，却盖不住自己要接的那条路）。
{ printf '#!/bin/sh\n'; _acct_prefix
  printf 'cat >/dev/null\nprintf %s "{\\"command\\":\\"%s/argvstub --resume FROM-DAEMON\\",\\"mode\\":\\"PtyInject\\"}"\n' "'%s\\n'" "$W/bin"
} > "$W/bin/faux-daemon"
chmod +x "$W/bin/faux-daemon"

actual_argv_d() {
  base_env CCM_DAEMON_BIN="$W/bin/faux-daemon" bash "$CCM" "$@" --cwd "$CWD" > "$W/ad.out" 2>&1
  grep '^ARGV|' "$W/ad.out" | head -1
}
predicted_argv_d() {
  base_env CCM_DAEMON_BIN="$W/bin/faux-daemon" bash "$CCM" "$@" --cwd "$CWD" --print > "$W/pd.line" 2>&1
  base_env CCM_DAEMON_BIN="$W/bin/faux-daemon" bash -c "$(cat "$W/pd.line")" > "$W/pd.out" 2>&1
  grep '^ARGV|' "$W/pd.out" | head -1
}
AD="$(actual_argv_d resume abc-123 --agent claude)"
ck "A′d · 真跑确实产出了 argv（差分自检）" "yes" "$([ -n "$AD" ] && echo yes || echo no)"
ck "A′d · print↔exec argv 一致（daemon 在位）" "$AD" "$(predicted_argv_d resume abc-123 --agent claude)"
# 绝对断言：证明 argv **真的来自 daemon**，不是「配方写了但没人走」。
ck "A′d · daemon 在位时 argv 必须来自 daemon（不是本地那条）" "ARGV|--resume FROM-DAEMON" "$AD"
# ★ 反向：**同一条命令、只是 daemon 不在**，必须落回本地那条（诚实降级，不是报错）。
#   这条与上一条成对 —— 只有上一条时，「永远走 daemon」也能绿。
#
# ⚠⚠ **不能用 `actual_argv`**（第一版就这么写，变异 Y2「拿不到就 die」**存活**）：
#   `actual_argv` 靠 `--launcher argvstub` 观察 argv，而显式 `--launcher` 恰好**绕开整个
#   daemon 块** ⇒ 那条判据**结构上就走不到降级路**，它测的是另一条路。
#   ★ 一般化：**观察手段本身改变了被观察的那条路** —— 判据的探针不许是被测分支的开关。
#   ⇒ 改用 PATH 上的 `claude` shim 观察：不给 `--launcher`，走的就是真实的默认启动器那条。
cat > "$W/bin/claude" <<'STUB'
#!/usr/bin/env bash
printf 'ARGV|%s\n' "$*"
STUB
chmod +x "$W/bin/claude"
actual_argv_nolauncher() {   # 不给 --launcher ⇒ 默认启动器 = PATH 上的 claude shim
  base_env bash "$CCM" "$@" --cwd "$CWD" > "$W/an.out" 2>&1
  grep '^ARGV|' "$W/an.out" | head -1
}
ck "A′d · 降级观察面自检：不给 --launcher 时确实观察得到 argv" "ARGV|--resume abc-123" \
   "$(actual_argv_nolauncher resume abc-123 --agent claude)"
# ⚠ `U-NP④` 订正措辞：这一格原写「daemon **不在**时必须落回本地」。今天"不在"是另一种结局
#   （身份前置检查 exit 2，见本文件头部那段 stub 的理由），而这条判据要钉的从来是
#   **「daemon 答不出命令时 argv 走本地那条」** —— 那正是部署落点上那份 null stub 制造的局面。
#   两件事分开：`ccm-cli.test.sh` 的「daemon 前置检查」一节钉"不在"，这里钉"答不出"。
ck "A′d · daemon 答不出命令时必须落回本地（诚实降级，不是报错）" "ARGV|--resume abc-123" \
   "$(env -u CCM_DAEMON_BIN bash -c 'true'; actual_argv_nolauncher resume abc-123 --agent claude)"
# ★ 显式 --launcher 必须压过 daemon 的建议（不许静默失效）。
ck "A′d · 显式 --launcher 优先于 daemon 建议" "ARGV|--resume abc-123" \
   "$(base_env CCM_DAEMON_BIN="$W/bin/faux-daemon" bash "$CCM" resume abc-123 --agent claude \
        --cwd "$CWD" --launcher "$W/bin/argvstub" 2>&1 | grep '^ARGV|' | head -1)"

# ===== A′e：`P4e` —— **不给 `CCM_DAEMON_BIN` 也找得到 daemon** 〔08-13〕=====
# 病：这条接线从 F06b 就在，但那个变量的**唯一生产注入点**是 cc-monitor 起子进程时
#     ⇒ `ccm → daemon` 只在 cc-monitor 拉起的 shell 里活着，而 skill 跑在普通 shell 里
#     —— 恰恰是它失灵的场合，且失灵是**静默**的。
# ⚠ 本组必须**不设** CCM_DAEMON_BIN，否则测的还是老路（`P4e §3` 提前记下的失效方式：
#   「在 cc-monitor 拉起的 shell 里测，那个变量有值 ⇒ 判据恒绿」）。
mkdir -p "$W/home/.cc-monitor/bin"
cp "$W/bin/faux-daemon" "$W/home/.cc-monitor/bin/cc-monitor-remote"
# `base_env` 把 HOME 换成 `$W/home` ⇒ 上面这一份就是查找次序里的第二档。
AE="$(actual_argv_nolauncher resume abc-123 --agent claude)"
ck "A′e · 不给 CCM_DAEMON_BIN 也能找到 daemon（部署落点）" "ARGV|--resume FROM-DAEMON" "$AE"
# ★★ **配方那侧也得找得到** —— 这一格是 D 阶段变异 M17 逼出来的：
#   把 `--print` 吐的配方偷偷退回老规则（只认 `CCM_DAEMON_BIN`），上面那条照样绿，
#   因为它只走 exec 路。而 `--print` 是 cc-monitor 的**渲染等价面**：配方与真跑脱钩，
#   意味着 app 渲染出来的命令与 ccm 真正会做的事**不是一回事**（F03 立那组网就是为这个）。
predicted_argv_nolauncher() {
  base_env bash "$CCM" "$@" --cwd "$CWD" --print > "$W/pe.line" 2>&1
  base_env bash -c "$(cat "$W/pe.line")" > "$W/pe.out" 2>&1
  grep '^ARGV|' "$W/pe.out" | head -1
}
ck "A′e · print↔exec 一致（靠 discovery 找到的 daemon）" "$AE" \
   "$(predicted_argv_nolauncher resume abc-123 --agent claude)"
# ★ 这一对**成对才有区分力**：只有上一条时，「永远走 daemon」也能绿；
#   只有下一条时，「永远不走 daemon」也能绿（今天之前它就是这样）。
# ⚠ **数组不能做命令前缀**：`BASE_EXTRA=(X=1) some_func` 不会把它带进去（首版这么写，
#   这条当场红 —— 它其实钉住了「前缀没生效」这个事实，报得对）。照本文件既有写法：先赋值、后复位。
#
# ★★ 〔`K-P2` `F` 拍 09-04〕**对照物换了，理由写清楚。**
#   从前这一条用 `CCM_NO_DAEMON=1`（「整条关掉 ⇒ 落回本地」）。用@09-04「统一走后端」之后
#   那条命令**没有本地可落**了：账号那条腿先报 `exit 4`（它也没有退路）
#   ⇒ 照旧写的话，这一条读到的是「整趟没跑起来」，而它标签说的是「argv 走本地那条」。
#   ⇒ 换成**部署落点上那份「答得出账号、答不出 `--resolve`」的 stub**（`_null_daemon`）——
#   那正是 `--resolve` 那笔**登记在案的静默欠账**唯一还够得着的局面，也是这一条一直要钉的东西。
_null_daemon
ck "A′e · 后端在、只是答不出 --resolve ⇒ argv 落回本地那条（登记在案的静默退路）" "ARGV|--resume abc-123" \
   "$(actual_argv_nolauncher resume abc-123 --agent claude)"
# 🔴 补一格：**`CCM_NO_DAEMON=1` 不再是逃生口**（它现在只把失败原因换一格）。
BASE_EXTRA=(CCM_NO_DAEMON=1)
base_env bash "$CCM" resume abc-123 --agent claude --cwd "$CWD" > "$W/ae2.out" 2> "$W/ae2.err"; _ae2_rc=$?
BASE_EXTRA=()
ck "A′e · CCM_NO_DAEMON=1 ⇒ **响亮失败**（rc=4，不再落回本地）" "4" "$_ae2_rc"
ck "A′e · 而且 argv 一个都没产出（不是「跑了本地那条」）" "" "$(grep '^ARGV|' "$W/ae2.out" | head -1)"
cp "$W/bin/faux-daemon" "$W/home/.cc-monitor/bin/cc-monitor-remote"
# `--print` 必须**纯**：同一条命令，装没装 daemon 吐出的字节必须逐字相同
#（吐的是**配方**不是查找结果 —— 与 BUS_ID_RECIPE 同一条纪律）。
#
# ★★ `K-C1`（08-24）**这一条拆成了两格，且是收紧不是放宽** —— 原因写清楚：
#   原来它比的是 `2>&1`（stdout+stderr）。`K-C1` 之后账号解析在 daemon 缺位时会**降级并往
#   stderr 说一句**（`§0b` 裁的：降级不许闷声）⇒ 「装没装 daemon 连 stderr 都逐字相同」
#   **今天是假的，而且是有意让它假的**。照原样留着，它钉的就不再是「配方纯不纯」，
#   而是「不许有任何降级提示」—— 那会把一条已裁的行为判成回归。
#   ⇒ 拆成：① **stdout（那条命令串）**逐字相同 —— 这才是「配方不是查找结果」的原意，
#            而且它比原版**更严**：原版里 stdout 的差异可以被 stderr 的差异掩盖成同一个 `differs`，
#            分不清是哪一半变了；
#         ② 差别**只**落在 stderr 的那一句降级提示上（新增的一格：证明差别是有意的、
#            且只在诊断面上，没有漏到命令串里）。
# ★★ 〔`K-P2` `F` 拍 09-04〕**这一对的对照物也换了，而且它买到的东西变强了。**
#   从前比的是「装了 daemon / `CCM_NO_DAEMON=1`」两趟的 `--print` 输出 ——
#   而后者今天整趟 `exit 4`，两趟根本没得比。
#   ⇒ 换成比「**部署落点那份答得出 `--resolve` / 答不出 `--resolve`**」两趟：
#   这才是「配方吐的是**配方**、不是**查找结果**」的正题 ——
#   配方里那段查找与调用是**执行时**才求值的，所以后端答什么都不该改动那条串一个字节。
#   ⚠ 比从前严：从前那一对里「有没有 daemon」还会改动**账号**那一段（`config_dir` 是值），
#   今天两趟的账号那一段完全相同 ⇒ 任何差异都只可能出在 resume 那一段上。
ck "A′e · --print 的**命令串**不因后端答不答得出 --resolve 而变（配方不是查找结果）" "same" \
   "$(_p1="$(base_env bash "$CCM" resume abc-123 --cwd "$CWD" --print 2>/dev/null)"
      _null_daemon
      _p2="$(base_env bash "$CCM" resume abc-123 --cwd "$CWD" --print 2>/dev/null)"
      cp "$W/bin/faux-daemon" "$W/home/.cc-monitor/bin/cc-monitor-remote"
      [ "$_p1" = "$_p2" ] && echo same || echo differs)"
ck "A′e · 而且两趟 stderr **都是空的**（`--print` 是纯的：它一个请求都不发，也没什么可降级的）" "quiet|quiet" \
   "$(_e1="$(base_env bash "$CCM" resume abc-123 --cwd "$CWD" --print 2>&1 >/dev/null)"
      _null_daemon
      _e2="$(base_env bash "$CCM" resume abc-123 --cwd "$CWD" --print 2>&1 >/dev/null)"
      cp "$W/bin/faux-daemon" "$W/home/.cc-monitor/bin/cc-monitor-remote"
      printf '%s|%s' \
        "$([ -z "$_e1" ] && echo quiet || echo "noisy:[$_e1]")" \
        "$([ -z "$_e2" ] && echo quiet || echo "noisy:[$_e2]")")"
# ⚠ `U-NP④`：这里原来是 `rm -f` —— 删掉之后**后面每一条真跑都会撞上身份前置检查**。
#   改成恢复成"答不出"的那份（见本文件头部 `_null_daemon` 的理由）。
_null_daemon

# 绝对断言：差分两边一起坏掉时的最后一道。
ck "A′ · resume 真跑的 argv 必须逐字带 --resume <sid>" "ARGV|--resume abc-123" \
   "$(actual_argv resume abc-123 --agent claude)"
ck "A′ · resume 的 --print 串也必须说出同一句" "ARGV|--resume abc-123" \
   "$(predicted_argv resume abc-123 --agent claude)"

echo
echo "===== A 组绝对断言（差分两边一起坏掉时它们才是最后一道）====="
ck "codex 在 tmux 内：真跑必须 export CC_BUS_ID=<会话名>" "CC_BUS_ID=faux-sess" \
   "$(actual_env --agent codex | tr '|' '\n' | grep '^CC_BUS_ID=')"
ck "codex 在 tmux 内：--print 也必须说出这一句（U9a 修复点）" "CC_BUS_ID=faux-sess" \
   "$(predicted_env --agent codex | tr '|' '\n' | grep '^CC_BUS_ID=')"
ck "claude 不得被注入 CC_BUS_ID（会盖掉 @cc_id 细分）" "" \
   "$(actual_env --agent claude | tr '|' '\n' | grep '^CC_BUS_ID=')"
ck "--account b 真跑注入其 configDir" "CLAUDE_CONFIG_DIR=$W/acct-b" \
   "$(actual_env --agent claude --account b | tr '|' '\n' | grep '^CLAUDE_CONFIG_DIR=')"
# `--model` 与 `--base` 原先**只有差分**，两边一起坏掉时全绿（审计变异 M6/M7 实证）。
# §33a 铁律 3 要求每条保住项都配一条绝对断言 —— 这两条就是补上的那两条。
ck "--model opus 真跑 export ANTHROPIC_MODEL" "ANTHROPIC_MODEL=opus" \
   "$(actual_env --agent claude --model opus | tr '|' '\n' | grep '^ANTHROPIC_MODEL=')"
# 继承值刻意用 **b**（≠ manifest 的默认号 z）：这样下面「不带 --base 时它还在」
# 同时证明了 R11 的「继承优先于默认号」，而不是与「默认号被注入」不可区分。
BASE_EXTRA=(CLAUDE_CONFIG_DIR="$W/acct-b")
ck "--base 真跑把继承来的 CLAUDE_CONFIG_DIR 清干净（#75 逃生口）" "" \
   "$(actual_env --agent claude --base | tr '|' '\n' | grep '^CLAUDE_CONFIG_DIR=')"
# 反面：同一继承环境下**不带** --base 时该值必须还在 —— 否则上一条会被「ccm 在这条路上
# 整体没跑起来」这种劣化冒充成功（期望空串型断言的固有弱点）。
ck "同一继承环境下不带 --base 时它必须还在（上一条的反面 + R11 继承优先）" "CLAUDE_CONFIG_DIR=$W/acct-b" \
   "$(actual_env --agent claude | tr '|' '\n' | grep '^CLAUDE_CONFIG_DIR=')"
BASE_EXTRA=()
ck "claude 的四个嵌套标记真跑后一个不剩" "" \
   "$(actual_env --agent claude | tr '|' '\n' | grep -E '^(CLAUDECODE|CLAUDE_CODE_ENTRYPOINT|CLAUDE_CODE_SESSION_ID|CLAUDE_CODE_CHILD_SESSION)=')"
ck "codex **不清** claude 的嵌套标记（agent_nested_env 逐 agent 不同）" "CLAUDECODE=1" \
   "$(actual_env --agent codex | tr '|' '\n' | grep '^CLAUDECODE=')"

echo
echo "===== B 组：eval \"\$CCM_ENV\"（S10 七项里唯一零覆盖的一条）====="
BASE_EXTRA=(CCM_ENV="export CCM_ENV_PROBE=from-ccm-env")
ck "真跑：CCM_ENV 被 eval 掉（不是原样透传、不是丢弃）" "CCM_ENV_PROBE=from-ccm-env" \
   "$(actual_env --agent claude | tr '|' '\n' | grep '^CCM_ENV_PROBE=')"
ck "--print：CCM_ENV 也在预言里" "CCM_ENV_PROBE=from-ccm-env" \
   "$(predicted_env --agent claude | tr '|' '\n' | grep '^CCM_ENV_PROBE=')"
ck "B · print↔exec 环境一致（带 CCM_ENV）" "$(actual_env --agent claude)" "$(predicted_env --agent claude)"
# 顺序：CCM_ENV 是**机器级**（代理等），必须先于会话级 env ——
# 反过来的话用户在 CCM_ENV 里设的 CLAUDE_CONFIG_DIR 会盖掉 --account 选的号。
BASE_EXTRA=(CCM_ENV="export CLAUDE_CONFIG_DIR=$W/acct-z")
ck "CCM_ENV 早于会话级 env：真跑时 --account 仍然赢" "CLAUDE_CONFIG_DIR=$W/acct-b" \
   "$(actual_env --agent claude --account b | tr '|' '\n' | grep '^CLAUDE_CONFIG_DIR=')"
# ★ **孪生条不能少**：`ccm_keys()` 做了 sort ⇒ 差分对**顺序**结构性失明。
# 只钉 exec 侧的话，把 `--print` 里的 `$CCM_ENV` 挪到会话级 env 之后 —— 预言机会输出
# 一条**落错账号**的命令串，而 21 条断言全绿（审计变异 M13 实证）。
ck "CCM_ENV 早于会话级 env：--print 侧同样（差分对顺序失明，必须单钉）" "CLAUDE_CONFIG_DIR=$W/acct-b" \
   "$(predicted_env --agent claude --account b | tr '|' '\n' | grep '^CLAUDE_CONFIG_DIR=')"
BASE_EXTRA=()

echo
echo "===== C 组：--ccm-probe 是跨语言契约，两个消费方都只测过 fixture ====="
# **必须隔离 `CCM_CONFIG`**：裸调会 `.` 掉用户真实的 `~/.config/ccm/config`（本机就有一份）。
# 今天那份是纯赋值所以无害，但配置里只要有一句输出就会把首行断言打掉 —— 那是假红，
# 而假红与假绿同样是坏信号（且与本文件其余每一处、另四套 ccm e2e 的口径不一致）。
PROBE="$(env CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent bash "$CCM" --ccm-probe 2>&1)"
ck "首行逐字 name=ccm（ccm_probe.rs::parse_probe_output 的判活依据）" "name=ccm" "$(printf '%s\n' "$PROBE" | head -1)"
ck "有 version= 行" "1" "$(printf '%s\n' "$PROBE" | grep -c '^version=')"
CAPS="$(printf '%s\n' "$PROBE" | sed -n 's/^capabilities=//p' | tr ',' '\n')"
# TS 侧要求的能力从**源码里抽**，不手抄——手抄一份等于又造一个双写点。
TS_CAPS="$(sed -n 's/^const CLI_REQUIRED_CAPS = \[\(.*\)\] as const;$/\1/p' "$REPO/src/launch-render-cli.ts" \
           | tr -d '" ' | tr ',' '\n' | grep -v '^$')"
TS_N="$(printf '%s\n' "$TS_CAPS" | grep -c .)"
# ★ 抽取器自检：抽空了的话下面那条"逐个都在"会**零命中零失败**地变绿。
ck "抽取器自检：CLI_REQUIRED_CAPS 抽到 ≥5 项（实得 $TS_N）" "yes" \
   "$([ "$TS_N" -ge 5 ] && echo yes || echo no)"
MISSING=""
for c in $TS_CAPS; do
  printf '%s\n' "$CAPS" | grep -qx "$c" || MISSING="$MISSING $c"
done
# **覆盖（⊇）不是相等**：ccm 多声明能力是允许的（今天就多 6 项），少声明才是病。
# 谁要是把这条收紧成相等，每加一个 flag 都会红 —— 那不是本条要防的东西。
ck "capabilities= 覆盖 TS 侧全部 CLI_REQUIRED_CAPS（⊇，不是 ==）" "" "$MISSING"
ck "agents= 行列出 claude 与 codex" "1" \
   "$(printf '%s\n' "$PROBE" | grep -c '^agents=claude,codex$')"

# ===== A′f：`P4e` 之后 daemon 是**自动**找到的 ⇒ 它坏掉的三种样子都要能兜住〔08-13〕=====
# ★ 为什么现在才要紧：`P4e` 之前这条路只在 cc-monitor 注入 env 时才活；
#   之后 ccm **自己会找** ⇒ 一个挂住的 daemon 会让**用户日常的 `ccm resume` 永远转圈**。
#   实测（修之前）：12 秒掐断才停。Rust 侧 `ccm_probe.rs` 为同一件事早就立过超时，
#   逐字「没有上限的话，用户点一次「恢复」就是永远转圈」——shell 侧补上同一条纪律。
# ⚠ 观察手段必须用 **PATH 上的 `claude` shim**，不能用 `--launcher`：
#   显式 `--launcher` **绕开整个 daemon 块**（A′d 那段头注逐字记着这条）。
#   08-13 我在这上面又栽了一次——拿 `--launcher` 探，三种坏 daemon 全「正常返回」。
# ⚠ `bad-hang` 也要**先**答完 `--list-accounts` 再挂〔`K-C1` 08-24〕：
#   本组量的是「**resume 那条路**挂住时会不会自己停」，若账号那条也陪着挂，
#   读数就变成两条路超时的**和**（3s+3s），量的东西就不是原来那个了。
# ⚠ 〔`K-P2` `F` 拍 09-04〕**三份坏 daemon 现在都要先答完 `--list-accounts`。**
#   上面那条纪律（`bad-hang` 要先答完账号再挂）原本只对挂住那一份成立，理由是
#   「若账号那条也陪着挂，读数就变成两条路超时的**和**」。
#   用@09-04「统一走后端」之后，账号那条**没有退路** ⇒ 不先答完的话它直接 `exit 4`，
#   本组量到的就是「账号那条腿失败了」，**而它标签说的是 resume 那条腿坏掉时怎么兜**。
#   ⇒ 同一条理由，射程从一份扩到三份。**这不是放宽：坏的仍然是 resume 那条腿。**
{ printf '#!/bin/sh\n'; _acct_prefix; printf 'sleep 300\n'; } > "$W/bin/bad-hang"; chmod +x "$W/bin/bad-hang"
{ printf '#!/bin/sh\n'; _acct_prefix; printf 'cat >/dev/null\necho 不是JSON\n'; } > "$W/bin/bad-garbage"; chmod +x "$W/bin/bad-garbage"
{ printf '#!/bin/sh\n'; _acct_prefix; printf 'exit 9\n'; }                       > "$W/bin/bad-broken";  chmod +x "$W/bin/bad-broken"
for _bad in hang garbage broken; do
  _t0=$(date +%s)
  _got="$(base_env CCM_DAEMON_BIN="$W/bin/bad-$_bad" bash "$CCM" resume abc-123 --agent claude \
            --cwd "$CWD" 2>&1 | grep '^ARGV|' | head -1)"
  _dt=$(( $(date +%s) - _t0 ))
  ck "A′f · daemon 坏成 $_bad ⇒ 仍落回本地那条" "ARGV|--resume abc-123" "$_got"
  # ★ 挂住那条要**自己停**：判据取「明显小于任何人的耐心」= 10s。
  ck "A′f · daemon 坏成 $_bad ⇒ 不永远转圈（${_dt}s < 10s）" \
     "yes" "$([ "$_dt" -lt 10 ] && echo yes || echo no)"
done

# ===== A′g：daemon 给的命令是**不 quote 展开**的 —— 两条性质各钉一格〔08-13〕=====
# ★ 背景：`exec $_ccm_c` 要的是**分词**（daemon 回的是一整串命令），
#   但不 quote 的展开**同时**会做路径名展开。而 daemon 可能是 PATH 上捡到的第三方二进制
#   （见 ccm 里 `DAEMON_BIN_RECIPE` 的查找次序）⇒ 这两条性质值得逐个钉死。
mkdir -p "$W/globdir" && touch "$W/globdir/aaa" "$W/globdir/bbb"
{ printf '#!/bin/sh\n'; _acct_prefix   # 〔`F` 拍〕账号那条腿没退路了 ⇒ 每份假 daemon 都要先答完它
  printf 'cat >/dev/null\nprintf %%s "{\\"command\\":\\"%s/argvstub GLOB *\\"}"\n' "$W/bin"
} > "$W/bin/daemon-glob"; chmod +x "$W/bin/daemon-glob"
ck "A′g · daemon 命令里的 \`*\` **不许**被 cwd 的文件名改写" "ARGV|GLOB *" \
   "$(base_env CCM_DAEMON_BIN="$W/bin/daemon-glob" bash "$CCM" resume abc-123 --agent claude \
        --cwd "$W/globdir" 2>&1 | grep '^ARGV|' | head -1)"
# ★★ 反向：**注入面必须保持干净** —— 这一格钉的是「别被优化成 `eval`」。
#
# ⚠⚠ payload 必须用 **`$(...)`**，不能用 `;`：08-13 实测，`;` 那种 payload
#   **区分不出 `eval`** —— `eval exec <cmd>; touch X` 里 `exec` 已经把进程换掉了，
#   分号后面本来就跑不到 ⇒ 变异 `eval` 版**存活**，我差点读成「判据管用」。
#   而命令替换是 `eval` 与普通展开的**真正分界**：不 eval 时 `$(…)` 原样是字面量，
#   eval 时它**当场执行**。实测两侧读数分明（原样打印 vs 标记文件生成）。
# ⚠ 标记文件用 `touch`，不做任何破坏性动作。
_MARK="$W/INJECTED"
rm -f "$_MARK"
printf '#!/bin/sh\ncat >/dev/null\nprintf %%s "{\\"command\\":\\"%s/argvstub A\\$(touch %s)B\\"}"\n' "$W/bin" "$_MARK" \
  > "$W/bin/daemon-inject"; chmod +x "$W/bin/daemon-inject"
base_env CCM_DAEMON_BIN="$W/bin/daemon-inject" bash "$CCM" resume abc-123 --agent claude \
  --cwd "$CWD" >/dev/null 2>&1
ck "A′g · daemon 命令里的 \`\$(…)\` **不许**被执行（别改成 eval）" "no" \
   "$([ -f "$_MARK" ] && echo yes || echo no)"

# ===== A′h：`P4e` 的**查找次序**逐档验一遍〔08-13〕=====
# 次序（`DAEMON_BIN_RECIPE` 头注写的）：`$CCM_DAEMON_BIN` > `~/.cc-monitor/bin/` > PATH。
# ★ 它此前**一档都没被判据看着** —— 而次序错了的症状是「用了另一个 daemon」，
#   两个 daemon 都能答话时**完全无声**。⇒ 让三档各答一个**可分辨**的串。
# ⚠ 用**沙箱 HOME**：那一档读的是 `$HOME/.cc-monitor/bin/`，绝不碰用户真实目录。
mkdir -p "$W/h3/home/.cc-monitor/bin" "$W/h3/pathbin"
for _k in env deploy path; do
  case "$_k" in
    env)    _f="$W/bin/dm-env" ;;
    deploy) _f="$W/h3/home/.cc-monitor/bin/cc-monitor-remote" ;;
    path)   _f="$W/h3/pathbin/cc-monitor-remote" ;;
  esac
  { printf '#!/bin/sh\n'; _acct_prefix
    printf 'cat >/dev/null\nprintf %%s "{\\"command\\":\\"%s/argvstub FROM-%s\\"}"\n' "$W/bin" "$_k"
  } > "$_f"
  chmod +x "$_f"
done
_h3() {  # $1=档位标签（只为可读，不参与判定）；其余=额外 env
  shift
  # ⚠ `TMUX` **必须显式给**〔`U-NP④` 08-14〕：本函数原来是让它从宿主环境漏进来的，
  #   而 `U-NP④` 之后「在不在 tmux 里」会改变结局（身份前置检查只在 tmux 里要求 daemon）
  #   ⇒ 不显式化的话，同一条判据在「开发者坐在 tmux 里」和「CI 裸 shell」上跑出两种结果。
  #   本组要测的是**查找次序**，那件事与 tmux 无关，所以把这个变量钉死、别让它漂。
  PATH="$W/h3/pathbin:$W/bin:$PATH" HOME="$W/h3/home" \
    env -u CLAUDE_CONFIG_DIR -u ANTHROPIC_MODEL -u CC_BUS_ID -u CCM_ENV -u CCM_ENV_PROBE \
        CLAUDECODE=1 CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent \
        TMUX=/faux/socket,1,0 \
        CCM_ACCTS_MANIFEST="$W/accounts.json" "$@" \
    bash "$CCM" resume abc-123 --agent claude --cwd "$CWD" 2>&1 | grep '^ARGV|' | head -1
}
ck "A′h · 三者齐全 ⇒ 用 \$CCM_DAEMON_BIN" "ARGV|FROM-env" \
   "$(_h3 env CCM_DAEMON_BIN="$W/bin/dm-env")"
ck "A′h · 无 env ⇒ 用部署落点 ~/.cc-monitor/bin/" "ARGV|FROM-deploy" \
   "$(_h3 deploy)"
rm -f "$W/h3/home/.cc-monitor/bin/cc-monitor-remote"
ck "A′h · 只剩 PATH ⇒ 用 PATH 上那份" "ARGV|FROM-path" "$(_h3 path)"
rm -f "$W/h3/pathbin/cc-monitor-remote"
# ★★ `U-NP④`（08-14）**这一格的结局变了，不是判据放宽**：
#   这一格原本断言「一个都没有 ⇒ 落回本地那条（`ARGV|--resume abc-123`）」。
#   用户裁定「ccm做到必须走daemon」之后，身份（`@ccm_sid`）**只**由 daemon 打、
#   ccm 里那条每秒轮询已删 ⇒ 在 tmux 里起 claude 而一个 daemon 都找不到，
#   正确的结局是**响亮失败**，不是「照跑，只是没有身份」（后者就是本件要根除的静默降级）。
#   ⇒ 拆成两格：**没有逃生口时必须失败** ＋ **明示逃生口时才落回本地**。
_h3_rc() { shift; PATH="$W/h3/pathbin:$W/bin:$PATH" HOME="$W/h3/home" \
    env -u CLAUDE_CONFIG_DIR -u ANTHROPIC_MODEL -u CC_BUS_ID -u CCM_ENV -u CCM_ENV_PROBE \
        CLAUDECODE=1 CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent \
        TMUX=/faux/socket,1,0 \
        CCM_ACCTS_MANIFEST="$W/accounts.json" "$@" \
    bash "$CCM" resume abc-123 --agent claude --cwd "$CWD" >/dev/null 2>&1; printf '%s' "$?"; }
# ★★ 〔`K-P2` `F` 拍 09-04〕**这两格的结局又变了一次，同样不是判据放宽。**
#   用@09-04「ccm不要管找不到, 统一走后端」之后：
#     · 「一个都没有」的码从 **2** 变 **4** —— 根因（这台机器上没有后端）与账号那条腿、
#       建会话那条腿是同一个，三条路从此走**同一个**失败面（`backend_unreachable`）。
#       给三个码的话，调用方要维护三份判法。
#     · `CCM_NO_DAEMON=1` **不再是逃生口**：账号那条腿也没有退路了，明示关掉之后照样 `exit 4`
#       ——它只把失败原因换了一格。⇒ 那一条从「落回本地」翻成「同一个码、说的是那一格」。
ck "A′h · 一个都没有 ⇒ **响亮失败**（rc=4 唯一失败面，不是悄悄没有身份）" "4" "$(_h3_rc none)"
ck "A′h · 一个都没有 + CCM_NO_DAEMON=1 ⇒ **照样 rc=4**（它不是逃生口，只是换了一格原因）" \
   "4" "$(_h3_rc none CCM_NO_DAEMON=1)"

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
