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

cat > "$W/m.json" <<JSON
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
      CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$W/m.json" \
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
cat > "$W/bin/faux-daemon" <<STUB
#!/usr/bin/env bash
cat >/dev/null
printf '{"command":"$W/bin/argvstub --resume FROM-DAEMON","mode":"PtyInject"}\n'
STUB
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
ck "A′d · daemon 不在时必须落回本地（诚实降级，不是报错）" "ARGV|--resume abc-123" \
   "$(env -u CCM_DAEMON_BIN bash -c 'true'; actual_argv_nolauncher resume abc-123 --agent claude)"
# ★ 显式 --launcher 必须压过 daemon 的建议（不许静默失效）。
ck "A′d · 显式 --launcher 优先于 daemon 建议" "ARGV|--resume abc-123" \
   "$(base_env CCM_DAEMON_BIN="$W/bin/faux-daemon" bash "$CCM" resume abc-123 --agent claude \
        --cwd "$CWD" --launcher "$W/bin/argvstub" 2>&1 | grep '^ARGV|' | head -1)"

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

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
