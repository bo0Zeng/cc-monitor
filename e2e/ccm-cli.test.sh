#!/bin/bash
# ccm CLI 的 shell 级测试（unify-launch F02）。
#
# 全部走 `--print` 断言命令串——不真起 agent、不碰 tmux（tmux 行为由
# e2e/tmux-target-acceptance.sh 那套真机 harness 管）。
#
# 跑法：bash e2e/ccm-cli.test.sh   （npm run test:ccm-cli）
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
CCM="$REPO/shared/ccm"

PASS=0; FAIL=0
ck() { # ck <描述> <期望> <实得>
  if [ "$2" = "$3" ]; then printf 'PASS | %s\n' "$1"; PASS=$((PASS+1))
  else printf 'FAIL | %s\n      期望: %s\n      实得: %s\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi
}
# **必须同时隔离 CCM_ACCTS_MANIFEST**：不隔离的话本机 manifest 的 isDefault 账号会被注入
# 每条黄金串（"零修饰 = 今天的 ccm()" 是**基座**语义，不带账号）。默认号注入另有专测。
# **全文件恒隔离 CLAUDE_CONFIG_DIR**：本机开发者本人就可能正跑在某个隔离账号下（这里真的
# 踩过——CLAUDE_CONFIG_DIR=/home/zbl/.claude-accts/z 是本次开发时的真实环境）。account-reset
# 修复后 ccm 会真的读这个变量，不隔离会让测试结果随"是谁在跑测试"而漂移。
ccm() { env -u CLAUDE_CONFIG_DIR CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=/nonexistent bash "$CCM" "$@" 2>&1; }

UNSET="unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION"

echo "===== 契约：动作 × 修饰 ====="
# F06b-1c（C4）：resume 的 exec 段现在是一段**配方** —— `--print` 打的是配方不是值
# （见 `shared/ccm` 的 `resolve_recipe` 头注：求值推迟到那条串真正被执行时，`--print` 才
# 保持纯的）。下面**手写一份期望文本**，⚠ **刻意不从 ccm 里取** —— 从 ccm 取就是同义反复，
# 实现怎么变期望就怎么变，这三条黄金串等于不存在。
# ⚠ 它仍是**逐字等值**断言，没有降成 `contains`：三条断言各自的意图
#（`--resume <sid>` 拼对了 / `--model` 被 export / resume 不做 auto 解析）在新串里逐字可见。
RECIPE() { # RECIPE <sid> <本地兜底的 exec 串>
  # ⚠ **`U-NP④`（08-14）订正**：这段手写期望自 `P4e`（08-13，daemon 改成**自己找**）起就腐了 ——
  #   它还停在「只认 `$CCM_DAEMON_BIN`」的老配方，于是下面三条黄金串**静默常红**。
  #   期望文本必须跟着 `DAEMON_BIN_RECIPE` 走（顺序 `$CCM_DAEMON_BIN` → 部署落点 → PATH），
  #   还有 `P4e` 的 `timeout 3` 与 F10/08-13 的 `set -f`。
  local sid_json="{\"sessionId\":\"$1\"}"
  printf '%s' '_ccm_c=""; _ccm_db=""; if [ "${CCM_NO_DAEMON:-}" != 1 ]; then for _ccm_x in "${CCM_DAEMON_BIN:-}" "$HOME/.cc-monitor/bin/cc-monitor-remote" "$(command -v cc-monitor-remote 2>/dev/null)"; do if [ -n "$_ccm_x" ] && [ -x "$_ccm_x" ]; then _ccm_db="$_ccm_x"; break; fi; done; unset _ccm_x; fi; if [ -n "$_ccm_db" ]; then _ccm_c="$(printf '"'"'%s'"'"' '"'"''"$sid_json"''"'"' | $(command -v timeout >/dev/null 2>&1 && printf '"'"'timeout 3 '"'"') "$_ccm_db" --resolve 2>/dev/null | grep -o '"'"'"command":"[^"]*"'"'"' | head -1 | cut -d'"'"'"'"'"' -f4)"; fi; if [ -n "$_ccm_c" ]; then set -f; exec $_ccm_c; else '"$2"'; fi'
}

ck "零修饰（--cwd .）：最终 exec 与今天 ccm() 逐字节一致" \
   "$UNSET; cd '.' && exec claude" \
   "$(ccm --cwd . --print)"
ck "resume <sid>" \
   "$UNSET; cd '/p' && $(RECIPE abc-123 "exec claude --resume abc-123")" \
   "$(ccm resume abc-123 --cwd /p --print)"
ck "--resume <sid> 与 resume <sid> **等价**（cc-monitor 今天的拼法，零改动即正确）" \
   "$(ccm resume abc-123 --cwd /p --print)" \
   "$(ccm --resume abc-123 --cwd /p --print)"
ck "--resume=<sid> 等号形式" \
   "$(ccm resume abc-123 --cwd /p --print)" \
   "$(ccm --resume=abc-123 --cwd /p --print)"
# U9a 2026-08-02：codex 的黄金串多了 cc-bus 身份注入那一段。
# **它一直都在真 exec 那条路上**（`shared/ccm::derive_bus_id`，codex 沙箱够不着 tmux socket ⇒
# 会话名必须经 env 透进去），只是 `--print` 从来没说 —— 而整个仓拿 `--print` 当离线预言机。
# 打印的是**配方不是值**（`TMUX` 判断留在串里、执行时才求值），所以这条串对宿主 `TMUX`
# 仍然逐字节稳定，不需要给这个 helper 加 `env -u TMUX`（那就成了为实现让路改判据）。
ck "--agent codex：换启动器 + 无嵌套 env + cc-bus 身份配方" \
   "if [ -n \"\${TMUX:-}\" ]; then _ccm_bus=\"\$(tmux display-message -p \"#S\" 2>/dev/null)\"; [ -n \"\$_ccm_bus\" ] && export CC_BUS_ID=\"\$_ccm_bus\"; unset _ccm_bus; fi; cd '/p' && exec codex" \
   "$(ccm --agent codex --cwd /p --print)"
ck "--agent codex 不支持 resume → 报错" \
   "ccm: agent=codex 不支持 resume（无 resume flag）" \
   "$(ccm resume x --agent codex --cwd /p --print)"
ck "--launcher 覆盖默认启动器" \
   "$UNSET; cd '/p' && exec mycc --resume s1" \
   "$(ccm resume s1 --cwd /p --launcher mycc --print)"
ck "--base：显式 unset CLAUDE_CONFIG_DIR（#75 逃生口）" \
   "unset CLAUDE_CONFIG_DIR; $UNSET; cd '/p' && exec claude" \
   "$(ccm --cwd /p --base --print)"
ck "--model：export ANTHROPIC_MODEL（F08，闭合 R14）" \
   "export ANTHROPIC_MODEL='opus'; $UNSET; cd '/p' && $(RECIPE s1 "exec claude --resume s1")" \
   "$(ccm resume s1 --cwd /p --model opus --print)"
ck "--model=<名> 等号形式" \
   "$(ccm resume s1 --cwd /p --model opus --print)" \
   "$(ccm resume s1 --cwd /p --model=opus --print)"
ck "-- 之后透传给 agent，含特殊字符正确 quote" \
   "$UNSET; cd '/p' && exec claude 'a b' 'x'\''y'" \
   "$(ccm --cwd /p --print -- "a b" "x'y")"

# ★★ audit-0805 F10 / 报告 I-11：**resume × daemon × passthru 这一格此前零覆盖**。
#
# `ccm resume` 且未显式 --launcher 时，argv 向 daemon 的 `--resolve` 要；而 daemon 的协议里
# **根本没有 passthru 这个字段**（`ResumeSpec` 六个字段无它）⇒ 它给的串不可能带用户的透传参数。
# 而那条路是 `exec $_ccm_c`，**整条换掉 argv** ⇒ `ccm resume <sid> -- --xxx`
# **装了 daemon 就丢参数、没装就不丢**。
#
# 上面那条既有用例走的是 `new` 路、无 daemon ⇒ 覆盖不到这一格。
ck "resume 走 daemon 配方时，-- 透传不许被整条换掉（I-11）" \
   "1 1" \
   "$(r="$(ccm resume s1 --cwd /p --print -- --flag-x)"; \
      printf '%s %s' \
        "$(printf '%s' "$r" | grep -c 'exec \$_ccm_c --flag-x')" \
        "$(printf '%s' "$r" | grep -c 'exec claude --resume s1 --flag-x')")"

# 反向：不带 `--` 时配方必须**逐字与从前相同**（防「补透传」写成无条件加东西）。
ck "resume 不带 -- 时配方里不许多出任何参数" \
   "0" \
   "$(ccm resume s1 --cwd /p --print | grep -c 'exec \$_ccm_c ')"
ck "--account 与 --base 互斥" \
   "ccm: --account 与 --base 互斥" \
   "$(ccm --cwd /p --account z --base --print)"
ck "未知 agent 报错" \
   "ccm: 未知 agent: gpt（支持 claude|codex）" \
   "$(ccm --agent gpt --cwd /p --print)"
ck "未知选项报错" \
   "ccm: 未知选项: --nope（用 --help 看用法）" \
   "$(ccm --nope --print)"
ck "attach 动作" \
   "tmux attach -t '=cc-foo:'" \
   "$(ccm attach cc-foo --print)"

echo
echo "===== 账号三态（D 审计 B1/B2 回归）====="
ACCTMP="$(mktemp -d)"; mkdir -p "$ACCTMP/z" "$ACCTMP/b"
cat > "$ACCTMP/m.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$ACCTMP/z", "isDefault": true },
  { "name": "b", "configDir": "$ACCTMP/b", "isDefault": false } ] }
JSON
acct() { env -u CLAUDE_CONFIG_DIR CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/m.json" bash "$CCM" "$@" 2>&1; }
ck "显式 --account 注入其 configDir" \
   "export CLAUDE_CONFIG_DIR='$ACCTMP/b'; $UNSET; cd '/p' && exec claude" \
   "$(acct --cwd /p --account b --print)"
# B1：die 在 \$(...) 里只杀子 shell —— 曾"报错后照跑"，落到继承来的账号上且 rc=0
ck "账号不存在 → 中止（rc≠0，且不得吐出 exec）" \
   "ccm: 账号 'nope' 不可用（不在 $ACCTMP/m.json，或其目录不存在）。可用: z b" \
   "$(acct --cwd /p --account nope --print)"
ck "账号不存在 → rc=2" "2" \
   "$(acct --cwd /p --account nope --print >/dev/null 2>&1; echo $?)"
# B2：cc-acct-iso 搬走凭据后基座常已无 .credentials.json —— 不落默认号则 cc/cct 掉进未登录目录
ck "不传 --account → 落 manifest 的 isDefault（复刻旧 _cc_acct_last 粘滞）" \
   "export CLAUDE_CONFIG_DIR='$ACCTMP/z'; $UNSET; cd '/p' && exec claude" \
   "$(acct --cwd /p --print)"
ck "--base → 显式不注入（#75 逃生口，压过默认号）" \
   "unset CLAUDE_CONFIG_DIR; $UNSET; cd '/p' && exec claude" \
   "$(acct --cwd /p --base --print)"
ck "无账号库 → 退化为基座（不报错）" \
   "$UNSET; cd '/p' && exec claude" \
   "$(ccm --cwd /p --print)"
ck "--account 与 --model 组合：账号目录先、模型偏好次（顺序即契约，见 launch-dimensions.ts order）" \
   "export CLAUDE_CONFIG_DIR='$ACCTMP/b'; export ANTHROPIC_MODEL='sonnet'; $UNSET; cd '/p' && exec claude" \
   "$(acct --cwd /p --account b --model sonnet --print)"
rm -rf "$ACCTMP"

echo
echo "===== 动作/目录语义（D 审计：auto 只对 new 生效）====="
ck "resume 不做 auto 解析（cc-monitor 已 cd 到会话目录，再解析会跑到 git 仓父目录）" \
   "$UNSET; cd '$PWD' && $(RECIPE s1 "exec claude --resume s1")" \
   "$(ccm --resume s1 --print)"
ck "resume 后跟 flag → 报错（别把 --tmux 当 sid）" \
   "ccm: resume 需要 <sid>" \
   "$(ccm resume --tmux --print)"
ck "attach 后跟 flag → 报错" \
   "ccm: attach 需要 <会话名>" \
   "$(ccm attach --print)"

echo
echo "===== 账号继承（F03 综合设计时发现的 bug 回归）====="
# 真机复现过：cc-monitor 把「远端 resume 命令」配成 ccm 时，实际调用形态是
#「外层已 export 好账号 X 的 CLAUDE_CONFIG_DIR，再 exec ccm --resume <sid>（不带任何账号 flag）」。
# 若 ccm 无脑落 manifest 默认号，会把 cc-monitor 精心选中的账号**静默覆盖**——账号选择完全失效，
# 且正是这轮建议用户使用的配置会踩中的场景。
# **必须 `-u TMUX -u TMUX_PANE`**：下面 R08 那几条测的是**容器路径**，而 ccm 有一条
# 「已在 tmux 内且未给会话名 → 就地起，不建嵌套」的分支。开发者在 tmux 里跑这套件时，
# `--tmux` 会落进那条分支、根本不走容器路径，于是 4 条 R08 断言假红（CI 上无 TMUX 所以
# 一直看不出来）。实测：同一份 HEAD，`TMUX` 有无决定 44/0 还是 40/4。
# 测什么就要固定什么，不能让环境替测试选路径。
inherit_acct() { CLAUDE_CONFIG_DIR="$ACCTMP/b" env -u TMUX -u TMUX_PANE CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/m.json" bash "$CCM" "$@" 2>&1; }
ACCTMP="$(mktemp -d)"; mkdir -p "$ACCTMP/z" "$ACCTMP/b"
cat > "$ACCTMP/m.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$ACCTMP/z", "isDefault": true },
  { "name": "b", "configDir": "$ACCTMP/b", "isDefault": false } ] }
JSON
ck "外层已继承账号 b（无 --account/--base）→ 保留 b，不被默认号 z 静默覆盖"    "$UNSET; cd '/p' && exec claude"    "$(inherit_acct --cwd /p --print)"
ck "裸终端（无继承）仍落 manifest 默认号 z"    "export CLAUDE_CONFIG_DIR='$ACCTMP/z'; $UNSET; cd '/p' && exec claude"    "$(env -u CLAUDE_CONFIG_DIR CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/m.json" bash "$CCM" --cwd /p --print 2>&1)"
ck "--base 显式清空，不受继承影响"    "unset CLAUDE_CONFIG_DIR; $UNSET; cd '/p' && exec claude"    "$(inherit_acct --cwd /p --base --print)"
ck "--account 显式指定，优先级最高（覆盖继承的 b）"    "export CLAUDE_CONFIG_DIR='$ACCTMP/z'; $UNSET; cd '/p' && exec claude"    "$(inherit_acct --cwd /p --account z --print)"

# R08（2026-07-28 实测复现）：R11 的修法在**容器路径上留了个洞**。
# 上面那条注释说"两个场景用同一条 if 天然区分"，**只对非容器路径成立**——
# 非容器路径下 `exec` 保留进程环境，"尊重继承值"= 什么都不做就已经对了；
# 但容器路径下 send-keys 打进的是 **tmux server fork 的新 shell**，
# `update-environment` 默认列表不含 CLAUDE_CONFIG_DIR（这正是本项目要治的那个病），
# 于是继承值在 L1 边界被吃掉，内层 ccm 看到空值 + 无账号 flag → 落 manifest 默认号 z。
# 症状与 R11 同型且同样隐蔽：**看起来生效了，只是换成了错的号。**
# 命中条件：启动器自己建容器（cct = ccm --tmux）+ 账号只靠继承环境变量传递。
# 载荷在 --print 里是 `'\''` 转义形态（外层再被单引号包一层），直接 grep 裸引号模式会全部落空
# ——我第一版就写错成这样，三条里两条假红。先反转义 `'\''` → `'` 再匹配。
# （R02 记的第三个失效模式的同型：断言模式与被测输出的真实形态不符 = 假信号。）
unesc() { sed "s/'\\\\''/'/g"; }
ck "R08：容器路径 + 继承账号 b → 内层载荷必须显式带上 b（不能靠继承穿 tmux 边界）" \
   "yes" \
   "$(inherit_acct --tmux --cwd /p --print | unesc | grep -qF "$ACCTMP/b" && echo yes || echo no)"
ck "R08：容器路径 + 继承账号 b → 内层绝不能落到默认号 z" \
   "yes" \
   "$(inherit_acct --tmux --cwd /p --print | unesc | grep -qF "$ACCTMP/z" && echo no || echo yes)"
ck "R08：容器路径 + --base → 内层显式 --base（不受继承影响，issue #75 逃生口不被削弱）" \
   "yes" \
   "$(inherit_acct --tmux --cwd /p --base --print | unesc | grep -qF -- '--base' && echo yes || echo no)"
ck "R08：容器路径 + 显式 --account z → 内层带 --account z（优先级不变）" \
   "yes" \
   "$(inherit_acct --tmux --cwd /p --account z --print | unesc | grep -qF -- "'--account' 'z'" && echo yes || echo no)"
ck "R08：容器路径 + 裸终端（无继承）→ 内层仍落默认号 z（粘滞体验不回退）" \
   "yes" \
   "$(env -u CLAUDE_CONFIG_DIR -u TMUX -u TMUX_PANE CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/m.json" bash "$CCM" --tmux --cwd /p --print 2>&1 | unesc | grep -qF -- "'--account' 'z'" && echo yes || echo no)"
rm -rf "$ACCTMP"

echo
echo "===== --cwd auto 与旧 _cc_resolve_target 对拍（5 种布局）====="
TMPROOT="$(mktemp -d)"
trap 'rm -rf "$TMPROOT"' EXIT
export CC_WORKSPACE="$TMPROOT/workspace"; mkdir -p "$CC_WORKSPACE"
mkdir -p "$TMPROOT/plain" "$TMPROOT/repo/sub/deep"
( cd "$TMPROOT/repo" && git init -q . 2>/dev/null )
FAKEHOME="$TMPROOT/home"; mkdir -p "$FAKEHOME"

# CCM_CONFIG 指到一个临时 config，把 workspace 对齐到对照值
CFG="$TMPROOT/ccm-config"; printf 'CCM_WORKSPACE=%s\n' "$CC_WORKSPACE" > "$CFG"
cmp_cwd() {
  local desc="$1" dir="$2" home="${3:-$HOME}" got want
  want="$( cd "$dir" && HOME="$home" CC_WORKSPACE="$CC_WORKSPACE" bash -c '
      if [ "$PWD" = "$HOME" ]; then REPLY="$CC_WORKSPACE"
      else g="$(git rev-parse --show-toplevel 2>/dev/null)"; [ -n "$g" ] && REPLY="$(dirname "$g")" || REPLY="$PWD"; fi
      printf "%s" "$REPLY"' )"
  got="$( cd "$dir" && HOME="$home" CCM_SELF=/usr/local/bin/ccm CCM_CONFIG="$CFG" \
      CCM_ACCTS_MANIFEST=/nonexistent bash "$CCM" --print 2>&1 | sed -n "s/.*cd '\\([^']*\\)' && .*/\\1/p" )"
  ck "$desc" "$want" "$got"
}
cmp_cwd "布局1：在 \$HOME → 工作区"        "$FAKEHOME" "$FAKEHOME"
cmp_cwd "布局2：git 仓根 → 仓的父目录"      "$TMPROOT/repo"
cmp_cwd "布局3：git 仓子目录 → 仓的父目录"  "$TMPROOT/repo/sub/deep"
cmp_cwd "布局4：非 git 目录 → 目录自己"     "$TMPROOT/plain"
cmp_cwd "布局5：工作区自身（非 git）→ 自己" "$CC_WORKSPACE"

echo
echo "===== 会话名派生：与前端 deriveTmuxName **真值对拍**（跨语言漂移守卫）====="
# 同规则 = 终端 cct 与 app「开新 Claude」在同一目录造出同一个名字 → 幂等接回同一会话。
# 不与手写期望比，与 src/remote-launch.ts 的真实实现比。
# **必须 env -u TMUX**：CLI 在 tmux 内会退化成"就地起"（不建嵌套会话），
# 那时 --print 没有 tmux 命令序列可抓。生产路径是 `ssh -t … bash -lic`，$TMUX 本就不存在。
# ⚠ **`^[{ ]*` 不能省**〔`U-NP④` 08-14 顺手修的既有腐坏〕：`P3sc`（08-13）把撞名改成
# 「响亮失败」时，把 `tmux new-session` 包进了 `{ … || { …; exit 3; }; }` ——
# 于是这条 `sed` 的 `^tmux` 锚点**零命中**，下面 5 条跨语言对拍**全部拿到空串、静默常红**。
# 这正是「判据的匹配单位跟不上事实的形状」那一族：报的是「对拍不一致」，真因是抽取器失灵。
name_of() { env -u TMUX CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=/nonexistent bash "$CCM" --tmux --cwd "$1" --print 2>&1 \
            | sed -n "s/^[{ ]*tmux new-session -d -s \\('[^']*'\\|[^ ]*\\) .*/\\1/p" | tr -d "'"; }
if command -v npx >/dev/null 2>&1; then
  for d in /home/pi/proj "/home/pi/a  b" /home/pi/proj/// / /home/pi/.hidden.dir; do
    want="$(cd "$REPO" && npx --no-install tsx -e "
      import {deriveTmuxName} from './src/remote-launch.ts';
      process.stdout.write(deriveTmuxName(process.argv[1]));
    " "$d" 2>/dev/null)"
    got="$(name_of "$d")"
    # tmux 名撞名时 CLI 会加 -2/-3；此处只比基名（测试环境不建会话，故恒等基名）
    ck "deriveTmuxName 对拍: $d" "$want" "$got"
  done
else
  # U0（2026-08-01）：**缺 npx 不许静默 SKIP。**
  #
  # 原来这里是 `echo "SKIP | 无 npx，跳过跨语言对拍"`，看着很客气，实际后果是：
  # 5 条断言不跑 ⇒ 合计 PASS 从 44 掉到 39 ⇒ `assert-pass-floor.sh ccm-cli 44` 判红，
  # 诊断写的是「断言数缩水 / 套件被削弱」—— **真因是环境缺工具，报的是代码退化**。
  # 排查的人会去翻这个套件最近改了什么，而那里什么也没发生。
  #
  # 而被跳掉的这 5 条不是可有可无：它们是 `deriveTmuxName`（TS）与
  # `derive_tmux_name`（bash）之间**唯一**的真值对拍 —— 跨语言双写点的漂移守卫。
  # 少了它，两边规则各自演化不会有任何信号（E49 记的就是这条）。
  #
  # ⇒ 改成 fail-closed 且**诊断说真话**。CI 上 npx 恒在，这条只会在本机裸环境触发。
  FAIL=$((FAIL + 1))
  echo "FAIL | 跨语言对拍无法运行：**找不到 npx** —— 环境缺工具，不是套件退化"
  echo "     | 被跳过的是 deriveTmuxName(TS) ↔ derive_tmux_name(bash) 的真值对拍，"
  echo "     | 即跨语言双写点唯一的漂移守卫（E49）。装上 node/npx 再跑，或明确接受此处无守卫。"
fi

echo
echo "===== 身份：daemon 前置检查（U-NP④，2026-08-14 用户裁定「ccm做到必须走daemon」）====="
# 这一节**真跑 ccm**（不是 `--print`），因为它验的是「跑到哪一步、退出码是多少、
# launcher 到底有没有被 exec」—— 那三件 `--print` 一件都答不了。
#
# ⚠ 本节替换掉的是原先「身份 poller 的 sid 解析」那 9 条：那条 **每会话一条、与会话同寿、
#   每秒一轮** 的 poller 已被整条删除（连同它的解析器 `_ccm_sid_from_file`），
#   `@ccm_sid` 改由 daemon 的 `control/identity_tag.rs` 在 pidfile inotify 上打。
#   删掉的判据不是「少测了」——它测的那个东西不存在了；新的一节测的是**替代契约**。
#
# ★ **PATH 里刻意没有 tmux**：失败那条路上一条 tmux 命令都不该起（前置检查排在
#   所有 tmux 调用之前）。若哪天有人把检查挪到 tmux 调用之后，这里会以"跑去碰 tmux"
#   的形式暴露出来，而不是安静地绿。C7i：本节全程**不碰任何 tmux server**。
DTMP="$(mktemp -d)"
mkdir -p "$DTMP/bin" "$DTMP/home" "$DTMP/proj"
# launcher 留痕：exec 到了才会有这个文件。
cat > "$DTMP/bin/mark" <<MARK
#!/bin/sh
: > "$DTMP/ran"
MARK
chmod +x "$DTMP/bin/mark"
# 一个"存在且可执行"的假 daemon —— 前置检查只查得到不查跑得起（如实边界，见 ccm 头注）。
printf '#!/bin/sh\nexit 0\n' > "$DTMP/bin/fake-daemon"; chmod +x "$DTMP/bin/fake-daemon"
cp "$(command -v bash)" "$DTMP/bin/bash" 2>/dev/null || ln -s "$(command -v bash)" "$DTMP/bin/bash"

# 受控运行：空环境 + 只有 bin/ 的 PATH（**没有 tmux、没有 cc-monitor-remote**）。
idrun() { # idrun <额外 env…> —— stdout/stderr 落文件，回显退出码
  rm -f "$DTMP/ran"
  env -i HOME="$DTMP/home" PATH="$DTMP/bin" \
      CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=/nonexistent \
      "$@" bash "$CCM" --cwd "$DTMP/proj" --launcher "$DTMP/bin/mark" \
      > "$DTMP/out" 2> "$DTMP/err"
  printf '%s' "$?"
}

RC="$(idrun TMUX=/faux/socket,1,0)"
ck "★ 在 tmux 里 + 找不到 daemon ⇒ **响亮失败**（rc=2，不是静默降级）" "2" "$RC"
ck "★ 失败时**没有** exec launcher（会话不许在没有身份的情况下起来）" \
   "no" "$([ -f "$DTMP/ran" ] && echo yes || echo no)"
ck "失败信息里说得出是缺什么" "yes" \
   "$(grep -q '找不到 daemon' "$DTMP/err" && echo yes || echo no)"
ck "失败信息里说得出**怎么办**（查找顺序 / 逃生口）" "yes" \
   "$(grep -q 'CCM_DAEMON_BIN' "$DTMP/err" && grep -q 'CCM_NO_DAEMON' "$DTMP/err" && echo yes || echo no)"

RC="$(idrun TMUX=/faux/socket,1,0 CCM_DAEMON_BIN="$DTMP/bin/fake-daemon")"
ck "找得到 daemon ⇒ 照常起（rc=0）" "0" "$RC"
ck "找得到 daemon ⇒ launcher 真被 exec 了" "yes" \
   "$([ -f "$DTMP/ran" ] && echo yes || echo no)"
ck "★ 找得到 daemon ⇒ stderr **一个字都没有**（别把正常路径变吵）" "" "$(cat "$DTMP/err")"

RC="$(idrun TMUX=/faux/socket,1,0 CCM_NO_DAEMON=1)"
ck "逃生口 CCM_NO_DAEMON=1 ⇒ 放行（rc=0）" "0" "$RC"
ck "★ 逃生口**照样说一句**（明示放弃身份 ≠ 闷声降级）" "yes" \
   "$(grep -q 'CCM_NO_DAEMON=1' "$DTMP/err" && echo yes || echo no)"

RC="$(idrun)"
ck "不在 tmux 里 ⇒ 不拦（rc=0）—— 那里根本没有地方放 @ccm_sid，拦了也换不来身份" "0" "$RC"
ck "★ 不在 tmux 里也**说一句**（旧版这里有窗口标题，随轮询一起没了，不许闷声）" "yes" \
   "$(grep -q '不在 tmux 里' "$DTMP/err" && echo yes || echo no)"

RC="$(env -i HOME="$DTMP/home" PATH="$DTMP/bin" TMUX=/faux/socket,1,0 \
      CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=/nonexistent \
      bash "$CCM" --cwd "$DTMP/proj" --agent codex --launcher "$DTMP/bin/mark" \
      > "$DTMP/out" 2> "$DTMP/err"; printf '%s' "$?")"
ck "codex 没有身份面（agent_has_identity 为假）⇒ 不要求 daemon、不吵" "0|" \
   "$RC|$(cat "$DTMP/err")"

# `--print` 是**纯的**：它在前置检查之前就退出了，缺 daemon 也照样吐串（rc=0）。
RC="$(env -i HOME="$DTMP/home" PATH="$DTMP/bin" TMUX=/faux/socket,1,0 \
      CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=/nonexistent \
      bash "$CCM" --cwd "$DTMP/proj" --print > "$DTMP/out" 2>/dev/null; printf '%s' "$?")"
ck "★ --print 不受前置检查影响（预言机不许因为这台机器没装 daemon 就哑掉）" "0" "$RC"

rm -rf "$DTMP"

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
