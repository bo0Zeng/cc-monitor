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

# ★★ **fail-closed：本套件对 `jq` 是硬依赖**〔K-C1 D 阶段审计 `I4`，08-24 补〕。
#   `K-C1` 起，本文件那份**镜像式**假 daemon（`mk_mirror_daemon`）用 `jq` 把夹具
#   manifest 翻成 `--list-accounts` 的帧形状。缺 `jq` 时它**不报错，只是少吐账号行** ——
#   于是 ccm 拿到一张空表，症状是 `可用: (无账号库)`，**诊断指向账号库、不指向缺 jq**
#   （审计实测：有 jq 时 mirror 吐 2 行、无 jq 时只剩 meta 那 1 行）。
#   ⚠ **08-24 D2 订正一处点错的名**：第二轮这句话把 `mk_kd` 也算进「用 `jq`」那一栏，
#     而 `mk_kd`（KCY 那节的假 daemon 工厂）**一个 `jq` 都不用** —— 它是 `printf` 硬写三行 JSON。
#     本文件里真正**跑** `jq` 的地方（按名字指，不写行号 —— 行号会随本文件长胖而失真）：
#     `mk_mirror_daemon`（**带 `2>/dev/null`** ⇒ 「静默少吐」那个机制本身就在它身上）·
#     `KCY1` 那条比对 daemon 与 manifest 的夹具自检（跑在**外层** shell）·
#     `KREC` 的记账 shim 与它那条尺子自检（跑在 `env -i PATH=/usr/bin:/bin` 上）。
#     ★ **守卫本身的作用域仍是对的**：那几份假 daemon 都继承外层 `PATH`（`acct()` 用 `env -u …`
#     不限 PATH，parity/acceptance 是 `PATH="$W/bin:$PATH"` 前置）⇒ 顶上这道 `command -v jq`
#     量的正是它们将来要用的那条 PATH。错的只是**举例举错了一个函数名**。
#   ⇒ 照同目录既有先例（`e2e/cc-bus-queue-drain.sh` · `e2e/daemon-cc-bus.sh`）当场停，
#     并且**说真话**：这是环境缺工具，不是套件退化。本文件对 `npx` 早就是这个纪律。
command -v jq >/dev/null 2>&1 || {
  echo "需要 jq —— 本套件的假 daemon 靠它把夹具 manifest 翻成 --list-accounts 帧形状；"
  echo "     缺它会静默少吐账号行（症状看着像「账号库坏了」）。这是环境缺工具，不是套件退化。"
  exit 1; }
# ★ **两条 PATH 都要量**〔08-24 D2 `S6` 的连带面〕。上一条量的是**外层** PATH（那几份假 daemon
#   继承的就是它）。而 `KREC` 的记账 shim 转发给的是 `env -i PATH=/usr/bin:/bin` 上那一个
#   （`S6`：不这么取的话，开发机上装了别版 `jq` 时那几条判据测的是**另一个二进制**）。
#   两条 PATH 可以不一致（`jq` 只装在 `~/bin` 时）⇒ 那边缺 `jq` 会让 shim 变成 `exec "" "$@"`,
#   红出来是一串莫名其妙的断言。**同样当场停，同样说真话。**
env -i PATH="/usr/bin:/bin" sh -c 'command -v jq >/dev/null 2>&1' || {
  echo "需要 /usr/bin 或 /bin 上的 jq —— KREC 那节的记账 shim 只转发给那条 PATH 上的 jq。"
  echo "     这是环境缺工具，不是套件退化。"
  exit 1; }

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
# ★★ `K-C1`（08-24）**本组的夹具改了两处，都是为了让它测的是生产形状**：
#   ① `m.json` → `accounts.json`。daemon 的 `--list-accounts` **只收目录**（`--accts-dir`），
#      manifest 的文件名由它自己拼（`acct-core::MANIFEST_NAME`）⇒ 叫别的名字时 ccm 判得出
#      「这个问题 daemon 答不了」、降级读文件并**说一句**，于是这几条黄金串会多出一行 stderr。
#      生产路径本来就是 `<目录>/accounts.json`（默认值与 cc-acct-iso 的 `ACCTS_DIR` 都是），
#      夹具跟上去 = 测的是真形状，不是「顺手把判据改绿」。
#   ② `CCM_DAEMON_BIN` 钉到一份**假 daemon**。不钉的话查找次序会摸到
#      `$HOME/.cc-monitor/bin/cc-monitor-remote` —— 开发机上那是**用户的真二进制**、CI 上不存在
#      ⇒ 同一条判据在两处走**两条不同的路**（本仓最高频那类假信号）。
# ⚠⚠ **这几条黄金串不是 provenance 判据**：假 daemon 刻意**照抄**夹具 manifest ⇒ 文件与 daemon
#    答同一个值 ⇒ 它们分辨不出 ccm 读了哪个（两条路输出逐字节相同）。
#    「到底走了 daemon 没有」由下面那节「账号解析走 daemon」用**答不同值**的夹具钉。
ACCTMP="$(mktemp -d)"; mkdir -p "$ACCTMP/z" "$ACCTMP/b" "$ACCTMP/bin"
cat > "$ACCTMP/accounts.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$ACCTMP/z", "isDefault": true },
  { "name": "b", "configDir": "$ACCTMP/b", "isDefault": false } ] }
JSON
# 假 daemon：把 `<accts-dir>/accounts.json` 原样翻成 `--list-accounts` 的帧形状。
# **单一事实源仍是那份 manifest** —— 不在这里手抄一份账号表（抄了就有两份要同步）。
mk_mirror_daemon() { # mk_mirror_daemon <落点>
  cat > "$1" <<'MIRROR'
#!/bin/sh
[ "$1" = --list-accounts ] || { cat >/dev/null; exit 0; }
d=""; while [ $# -gt 0 ]; do [ "$1" = --accts-dir ] && d="$2"; shift; done
printf '{"accountZeroAware":true,"acctsDir":"%s","count":0,"enabled":true,"error":null,"kind":"accounts-meta","manifestPath":"%s/accounts.json","sharedStore":null,"updatedAt":null}\n' "$d" "$d"
jq -c '.accounts[] | {configDir:(.configDir // null),email:"",exists:true,isDefault:(.isDefault // false),loggedIn:false,mode:"isolated",name:.name}' "$d/accounts.json" 2>/dev/null
exit 0
MIRROR
  chmod +x "$1"
}
mk_mirror_daemon "$ACCTMP/bin/daemon"
acct() { env -u CLAUDE_CONFIG_DIR CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/accounts.json" CCM_DAEMON_BIN="$ACCTMP/bin/daemon" bash "$CCM" "$@" 2>&1; }
ck "显式 --account 注入其 configDir" \
   "export CLAUDE_CONFIG_DIR='$ACCTMP/b'; $UNSET; cd '/p' && exec claude" \
   "$(acct --cwd /p --account b --print)"
# B1：die 在 \$(...) 里只杀子 shell —— 曾"报错后照跑"，落到继承来的账号上且 rc=0
ck "账号不存在 → 中止（rc≠0，且不得吐出 exec）" \
   "ccm: 账号 'nope' 不可用（不在 $ACCTMP/accounts.json，或其目录不存在）。可用: z b" \
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
# `K-C1`：夹具改名 + 钉假 daemon，理由同上一组（那段头注逐条写了，别在这儿重抄）。
inherit_acct() { CLAUDE_CONFIG_DIR="$ACCTMP/b" env -u TMUX -u TMUX_PANE CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/accounts.json" CCM_DAEMON_BIN="$ACCTMP/bin/daemon" bash "$CCM" "$@" 2>&1; }
ACCTMP="$(mktemp -d)"; mkdir -p "$ACCTMP/z" "$ACCTMP/b" "$ACCTMP/bin"
cat > "$ACCTMP/accounts.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$ACCTMP/z", "isDefault": true },
  { "name": "b", "configDir": "$ACCTMP/b", "isDefault": false } ] }
JSON
mk_mirror_daemon "$ACCTMP/bin/daemon"
ck "外层已继承账号 b（无 --account/--base）→ 保留 b，不被默认号 z 静默覆盖"    "$UNSET; cd '/p' && exec claude"    "$(inherit_acct --cwd /p --print)"
ck "裸终端（无继承）仍落 manifest 默认号 z"    "export CLAUDE_CONFIG_DIR='$ACCTMP/z'; $UNSET; cd '/p' && exec claude"    "$(env -u CLAUDE_CONFIG_DIR CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/accounts.json" CCM_DAEMON_BIN="$ACCTMP/bin/daemon" bash "$CCM" --cwd /p --print 2>&1)"
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
   "$(env -u CLAUDE_CONFIG_DIR -u TMUX -u TMUX_PANE CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/accounts.json" CCM_DAEMON_BIN="$ACCTMP/bin/daemon" bash "$CCM" --tmux --cwd /p --print 2>&1 | unesc | grep -qF -- "'--account' 'z'" && echo yes || echo no)"
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
echo "===== 账号解析走 daemon（K-C1，〔用@08-24「ccm要换成调用后端」〕的便宜那半）====="
#
# ★★ **本节的要害全在夹具：manifest 与 daemon 必须答不同的值。**
#   「读文件」与「问 daemon」在两份数据一致时输出**逐字节相同** ⇒ 分辨不出它读了哪个。
#   上面「账号三态」那几条黄金串正是那种形状（假 daemon 照抄夹具 manifest）——
#   它们钉的是「注入了哪个值」，**不是**「值从哪儿来」。
#   ⇒ 这里 manifest 写 `from-file` / daemon 答 `from-daemon`，断言落在**哪一个出现在输出里**；
#     默认号也故意让两边指向**不同的账号**（文件说 z、daemon 说 d）。
#   变异 `KCM1`（让两边答同一个值）就是用来证明「夹具无效时本节会说话」的。
#
# ★ 全节走 `--print`：账号解析在它之前就发生（`config_dir` 是**值**、逐字进黄金串），
#   而 `--print` 之后那条真 exec 路会再往 stderr 打一句「不在 tmux 里」——
#   那句与账号无关，混进来会让下面几条「stderr 说了什么」的断言测到别的东西。
#   ⚠ 真起会话那条路（不是 `--print`）由 `e2e/ccm-acceptance.sh` 场景 7 在**真 tmux** 上验。
KTMP="$(mktemp -d)"
mkdir -p "$KTMP/accts" "$KTMP/from-file" "$KTMP/from-daemon" "$KTMP/dflt-file" "$KTMP/dflt-daemon" \
         "$KTMP/file-only" "$KTMP/bin" "$KTMP/nojq"
# ★★ 第三个账号 `f` 是 **daemon 不知道的那一个**〔K-C1 D 阶段审计 `B2`/`I2`，08-24 补〕。
#   为什么非它不可：原来的夹具**两侧账号名集合都是 `{z,d}`** ⇒「只读 daemon」与
#   「读了文件再取更宽的那个 + 按名去重 + daemon 胜出」在这套夹具上**逐字节同行为**
#   ⇒ 「daemon 在位时还去读一遍文件」这件事**不可能**被任何断言分辨（审计 `MU-A` 实测 189/189 全绿），
#   而 `§0b` 排除项③ 却写着「那正是 KCY1 的夹具专门要逮的东西」。
#   加一个**只有文件里有**的名字之后，「可用列表」就成了一把真的 provenance 尺子：
#   daemon 在位时它必须**恰好**是 daemon 那一份，`f` 一个字都不许漏进来。
#   ⚠ 它的目录**真实存在**（不是坏账号）——否则红的原因会变成「目录不存在」，射程就跑偏了。
cat > "$KTMP/accts/accounts.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$KTMP/from-file", "isDefault": true },
  { "name": "d", "configDir": "$KTMP/dflt-file", "isDefault": false },
  { "name": "f", "configDir": "$KTMP/file-only", "isDefault": false } ] }
JSON
# 假 daemon：**刻意答与文件不同的目录**，且把 isDefault 挪到另一个账号上。
# 顺带记账（每次被调用 append 一行）⇒ 可以断言「一趟往返」而不是「每问一次一趟」。
mk_kd() { # mk_kd <落点> <z 的 configDir> <d 的 configDir> [额外键]
  cat > "$1" <<EOF
#!/bin/sh
echo call >> "$KTMP/calls"
case "\$1" in
  --list-accounts)
    printf '%s\\n' '{"accountZeroAware":true,"acctsDir":"x","count":2,"enabled":true,"error":null,"kind":"accounts-meta","manifestPath":"x","sharedStore":null,"updatedAt":null}'
    printf '%s\\n' '{"configDir":"$2","email":"","exists":true,"isDefault":false,"loggedIn":false,"mode":"isolated","name":"z"${4:-}}'
    printf '%s\\n' '{"configDir":"$3","email":"","exists":true,"isDefault":true,"loggedIn":false,"mode":"isolated","name":"d"${4:-}}'
    exit 0 ;;
esac
cat >/dev/null; exit 0
EOF
  chmod +x "$1"
}
mk_kd "$KTMP/bin/daemon" "$KTMP/from-daemon" "$KTMP/dflt-daemon"
# 一份「答不出 --list-accounts」的 daemon（存在、可执行、但一个字都不说）。
printf '#!/bin/sh\necho call >> "%s"\ncat >/dev/null\nexit 0\n' "$KTMP/calls" > "$KTMP/bin/mute"
chmod +x "$KTMP/bin/mute"
# 一份答「目录不存在」的 daemon（验：目录存在性仍由 ccm 自己 `-d` 判，不吃 daemon 的 `exists`）。
mk_kd "$KTMP/bin/ghost" "$KTMP/no-such-dir" "$KTMP/no-such-dir-2"
# 一份多带一个**未知字段**的 daemon（前向兼容：daemon/aterm 那边加字段不许把我们打碎）。
mk_kd "$KTMP/bin/extra" "$KTMP/from-daemon" "$KTMP/dflt-daemon" ',"someFutureKey":{"a":1}'

# 受控运行。**HOME 换成 $KTMP** —— 查找次序第二档读的是 `$HOME/.cc-monitor/bin/`，
# 不换的话开发机上会摸到用户的真二进制、CI 上摸不到 ⇒ 同一条判据两台机器走两条路。
# ⚠ **额外 env 与 ccm 参数必须分开两个位置**（第一版把它们混在 `"$@"` 里，`env` 当场把
#   `--account` 读成自己的选项、`No such file or directory`，8 条断言拿到空串 —— 那是
#   「探针自己坏了」而报出来的却像「被测行为不对」）。第 2 个参数是 `'A=1 B=2'` 形态的额外 env。
K() { # K <daemon 路径或 -> <额外 env（可空）> <ccm 参数…>；stdout→out、stderr→err，回显 rc
  local d="$1" xe="$2"; shift 2
  rm -f "$KTMP/calls"; : > "$KTMP/out"; : > "$KTMP/err"
  local -a envs=(HOME="$KTMP" CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent
                 CCM_ACCTS_MANIFEST="$KTMP/accts/accounts.json")
  [ "$d" != - ] && envs+=(CCM_DAEMON_BIN="$d")
  # shellcheck disable=SC2086  # $xe 是本套件自己给的 'A=1 B=2'，要的正是分词
  env -i PATH="/usr/bin:/bin" "${envs[@]}" $xe bash "$CCM" --cwd /p --print "$@" \
      > "$KTMP/out" 2> "$KTMP/err"
  printf '%s' "$?"
}
KOUT() { cat "$KTMP/out"; }
KERR() { cat "$KTMP/err"; }
KCALLS() { [ -f "$KTMP/calls" ] && grep -c . "$KTMP/calls" || echo 0; }
# die 那句 `…不可用（…）。可用: <列表>` 里的**整段列表**（逐字，不是子串命中）。
# 降级提示那一行不含 `。可用: ` ⇒ `head -1` 取到的一定是 die 那行。
KAVAIL() { sed -n 's/^.*。可用: //p' "$KTMP/err" | head -1; }
# 那句降级提示里，四个**降级原因**标记一共命中了几个。裁定要的是「说得出是哪一格」
# ⇒ 正确答案恒为 **1**（互斥）。四条既有 `grep -q` 只查「这个子串在不在」，不查「是不是只有它」,
# 于是把四句并成一句（`§0b` 明令禁止的那一种合并）能把四条一起绕过 —— 审计 `MU-B` 实测 160/160 全绿。
# ⚠ 方向与直觉相反：改一个字**会红**（既有那四条会说话），**加字**才是洞 ⇒ 补的是这把「只许一格」的尺子。
# ⚠⚠ **它同时钉住了措辞，改措辞要连它一起改**〔审计 `S7`，08-24 补〕。名字写的是「四种降级原因
#   **互斥**」，但它量的其实是「**下面这四个字面串**各在不在」，不是「`why` 恰好属于四格之一」。
#   后果：**合法地改写某一格的措辞**（比如把「这台机器上找不到 daemon（cc-monitor-remote）」
#   缩成「找不到 daemon」）也会让它红，而红出来的话是「只许命中一格」⇒ **诊断指错方向**
#   （审计 `MU-B4` 实测：恰好 1 红，就是它，而既有那条 `grep -q '找不到 daemon'` 照旧绿）。
#   ⇒ 改 `shared/ccm` 里那四句话的措辞时，**这四个串要跟着改**；红了先看是不是自己刚改了措辞。
#   （不换成「按前缀/正则认一格」的理由：那会把「合并成一句含四个子串的话」这个真洞放回去 ——
#    那正是本条要挡的东西。逐字钉措辞是这把尺子的**代价**，不是它的 bug。）
KWHY() {
  local n=0 p
  for p in '这台机器上找不到 daemon（cc-monitor-remote）' \
           'CCM_NO_DAEMON=1（明示整条关掉 daemon）' \
           'daemon 的 --accts-dir 表达不了它' \
           '答不出 --list-accounts'; do
    grep -qF -- "$p" "$KTMP/err" && n=$((n+1))
  done
  printf '%s' "$n"
}
GOLD() { printf "export CLAUDE_CONFIG_DIR='%s'; %s; cd '/p' && exec claude" "$1" "$UNSET"; }

# ---- 夹具自检（先证明「有区分力」，再拿它去判事）----
ck "夹具自检：无 daemon 那条环境里**真的一个 daemon 都找不到**" "yes" \
   "$(K - "" --account z >/dev/null; grep -q '找不到 daemon' "$KTMP/err" && echo yes || echo no)"
# ★★ 这条自检**必须问「daemon 实际答了什么」**，不是比两个路径字面量。
#   第一版写的是 `[ "$KTMP/from-file" != "$KTMP/from-daemon" ]` —— 那两个字符串**恒不相等**,
#   于是变异 `KCM1`（把假 daemon 改成答与文件相同的目录）在它眼里**毫无变化** ⇒ 恒绿。
#   量的东西要与被判的东西对上：真去跑一次 daemon，把它答的 z 的 configDir 与 manifest 里那个比。
ck "夹具自检：daemon **实际答的** z 的 configDir 与 manifest 里那个刻意不同（KCM1 的靶子）" "differ" \
   "$(_mf="$(jq -r '.accounts[]|select(.name=="z")|.configDir' "$KTMP/accts/accounts.json" 2>/dev/null)"
      _dm="$("$KTMP/bin/daemon" --list-accounts --accts-dir "$KTMP/accts" 2>/dev/null \
             | jq -r 'select(.name=="z")|.configDir' 2>/dev/null)"
      if [ -z "$_mf" ] || [ -z "$_dm" ]; then echo "抽取器坏了:[mf=$_mf][dm=$_dm]"
      elif [ "$_mf" != "$_dm" ]; then echo differ; else echo "same:[$_dm]"; fi)"
# ★ 夹具前提要配一条**成对自检**〔审计 `S3`，08-24 补〕：账号 f 的目录**必须真实存在**。
#   第二轮给这条前提写的只是一句 ⚠ 注释 —— 而同一轮立的纪律正是「夹具前提要配一条成对自检」
#   （`N2`/`N5` 是两个范例）。审计 `MU-F2`（把 `$KTMP/file-only` 从 `mkdir -p` 里删掉）实测
#   **104/104 全绿** ⇒ 那条前提当时**空转**。今天它已经承重（`KCM6` 末块那条查的就是 `--account f`,
#   而 `account_config_dir` 会 `-d` 判目录）—— 正因为承重，更需要这条自检把诊断分开。
#   ⚠ **08-25 订正一句写宽了的话**〔审计 `S4`〕：这里原写「目录没了该红成「夹具坏了」，
#     **不该**红成「末块被 read 丢掉了」」。**实测（08-25 R7 又重切一次 `MU-F2`：锚点 1/1、判定行 126）
#     是两条都红**：这条自检 ＋「末块 f」那条。⇒ 它做到的是「让**正确的**诊断**也**出现，
#     两条一起看就分得清是夹具坏了还是解析坏了」，**不是**「让错的诊断**不**出现」。
ck "夹具自检：账号 f 的目录**真实存在**（不是坏账号；否则末块那条会红成「目录不存在」，射程跑偏）" "yes" \
   "$([ -d "$KTMP/file-only" ] && echo yes || echo no)"

# ---- KCY1：账号解析真的走了 daemon（量行为，不量源码）----
K "$KTMP/bin/daemon" "" --account z >/dev/null
ck "★ KCY1 · 显式 --account：daemon 在位 ⇒ configDir 来自 **daemon**（不是文件）" \
   "$(GOLD "$KTMP/from-daemon")" "$(KOUT)"
K - "" --account z >/dev/null
ck "KCY1 · 反向：同一夹具、无 daemon ⇒ 来自**文件**（成对才有区分力：只有上一条时「永远读文件」也能绿）" \
   "$(GOLD "$KTMP/from-file")" "$(KOUT)"
K "$KTMP/bin/daemon" "" >/dev/null
ck "★ KCY1 · 默认号那条路**也**走 daemon（文件说默认号是 z、daemon 说是 d ⇒ 拿到 d 的目录）" \
   "$(GOLD "$KTMP/dflt-daemon")" "$(KOUT)"
K - "" >/dev/null
ck "KCY1 · 反向：无 daemon ⇒ 默认号是**文件**说的那个（z）" \
   "$(GOLD "$KTMP/from-file")" "$(KOUT)"
# ★ 一趟往返：默认号那条路要答**两个**问题（谁是默认号 / 它的目录在哪）。
#   把 `acct_table_load` 挪进 `$(...)` 里 ⇒ 缓存被子 shell 吃掉 ⇒ 这里会变成 2。
K "$KTMP/bin/daemon" "" >/dev/null
ck "★ KCY1 · **一趟**往返答完全部问题（默认号那条路要答两个问题，daemon 仍只被调 1 次）" "1" "$(KCALLS)"
K "$KTMP/bin/daemon" "" --base >/dev/null
ck "KCY1 · --base ⇒ 压根不问 daemon（0 次；那条路不需要账号表，问了就是白付一次往返）" "0" "$(KCALLS)"
K "$KTMP/bin/daemon" "CLAUDE_CONFIG_DIR=$KTMP/from-file" >/dev/null
ck "KCY1 · 已继承 CLAUDE_CONFIG_DIR ⇒ 压根不问 daemon（0 次）" "0" "$(KCALLS)"
RC="$(K "$KTMP/bin/ghost" "" --account z)"
ck "★ KCY1 · daemon 说的目录**不存在** ⇒ 照旧 die（目录存在性由 ccm 自己 -d 判，不吃 daemon 的 exists）" \
   "2" "$RC"
# ⚠ 这条原来叫「上一条的可用列表**来自 daemon 的答案**」—— 那个名字比它量得到的强。
#   它是 `grep -q` **子串**判，而夹具两侧当时都是 `{z,d}` ⇒ 对 provenance 恒真
#   （审计 `MU-C` 实测 160/160 全绿）。**只改名、不动判定**：它真有的牙是「die 消息里那个列表
#   确实以 daemon 那一份开头」（退实现时它红过）。provenance 由紧跟的下一条钉。
ck "KCY1 · die 消息里那个列表**以 daemon 那份 z d 开头**（子串判 ⇒ provenance 由下一条钉，不是这条）" "yes" \
   "$(grep -q '可用: z d' "$KTMP/err" && echo yes || echo no)"
# ★★ 上面那条是 `grep -q` **子串**判：文件里多一个 `f` 时 `可用: z d f` 照样含 `可用: z d`
#   ⇒ 它对自己声称的 provenance **没有区分力**（审计 `MU-C`：把 `list_account_names` 改成
#   读文件，160/160 全绿）。下面这条改成**逐字取整段列表**，于是：
#     · `MU-C`（列表改读文件）⇒ 实得 `z d f` ⇒ 红；
#     · `MU-A`（daemon 在位时还去读文件、取更宽的那个）⇒ 实得 `z d f` ⇒ 红。
#   ⇒ `§0b` 排除项③「后端在位它就是**唯一**答案」这条性质，从此有判据。
ck "★ KCY1 · 可用列表**就是 daemon 那一份**：文件里那个 daemon 没有的 f 一个字都不许漏进来" \
   "z d" "$(KAVAIL)"
# 成对的自检：证明 `f` **真的**在文件里、且真的会出现 —— 否则上一条恒真（拿一个压根不存在的
# 名字去断言「它不出现」，是本仓最典型的那类空转判据）。
K - "" --account nope >/dev/null
ck "KCY1 · 夹具自检：无 daemon 时那份文件列表**确实更宽**（z d f）⇒ 上一条不是恒真" \
   "z d f" "$(KAVAIL)"
# ★★ 上面那一对钉的是「**用了**谁的值」。`§0b` 排除项③ 排的还有一半是「**读了**谁」——
#   「daemon 在位时也拿文件那份去校对」这一刀（读了不用）在**值**上一个字节都不差,
#   任何比输出的判据都逮不到它（审计 `MU-A` 第一刀实测 189/189 全绿）。
#   ⇒ 换一把尺子：**记账 `jq`** —— 每次 `jq` 被调用就记一行，行里同时含 **argv** 与
#     **stdin 指向谁**（`readlink /proc/$$/fd/0`）⇒「那份 manifest 的路径出现在某次 jq 的
#     argv 或 stdin 里」= 「有人让 jq 去解析它了」，与解析出来的值用不用**无关**。
#     `daemon_out_to_table` 走管道（stdin 读作 `pipe:[…]`、argv 里无文件）⇒ 不误报。
#   ⚠ **射程如实写明（08-24 D2 订正 + 加宽）**：第二轮这里写的是「只逮得到**有 jq 那条路**」——
#     **那句话把射程写宽了一档**。它当时只逮得到「把 manifest 路径**当 argv** 传给 jq」那一种读法：
#     审计 `MU-A1b`（同一位置、同样是「读了不用」，只改成 `jq … < "$CCM_ACCTS_MANIFEST"`）
#     实测 **104/104 全绿**，而自证显示 manifest 的内容**确实进了 jq**。现在 argv / stdin **两形都逮**
#     （紧跟的那条尺子自检把两形各跑一次，是这条判据的成对自检）。
#     **仍然逮不到的，逐条登记**（件文件 `§4 KC6g`）：
#       · 无 `jq` 时 `manifest_to_table` 是纯 bash 内建（零 fork）⇒ 那一格没有可观测事件；
#       · 内容**经第三方转手**再喂给 jq（`cat file | jq`、先读进变量再 `<<<`）⇒ stdin 是管道，读作 0；
#       · 压根不用 `jq`、直接 bash 内建读那份文件（同上，零 fork）。
#     ⇒ 它证得到的是「**没有人以 argv 或 stdin 直连的方式让 jq 读过那份 manifest**」,
#       **不是**「所有绕法」。别把这条读成「manifest 一个字节都没被碰过」。
# ⚠ `REALJQ` 必须在**与 `KREC` 这一节的 ccm 调用同一条 PATH** 上取〔审计 `S6` 08-24 补，
#   `D3 B3` 08-25 订正它的分母〕：`KREC()` 那一处跑在 `env -i PATH="$KTMP/recbin:/usr/bin:/bin"` 下，
#   这里若用**外层** PATH，开发机上装了别版 `jq`（`~/bin` / `asdf` / `nix`）时经这个 shim 的每一条
#   判据测的就是**另一个二进制**。
#   ⚠ **08-25 订正一句可证伪的话**：这里原写「本套件**其余每一处** ccm 调用都跑在
#     `env -i PATH="/usr/bin:/bin"` 下」，**是假的**。分母 = `grep -c 'bash "\$CCM"'` = **16** 个调用点
#     （量于 08-25），逐类：`env -i PATH="/usr/bin:/bin"` **4** 处 · `$KTMP/recbin:/usr/bin:/bin` 1 处 ·
#     `$KTMP/nojq` 1 处 · `$DTMP/bin` 3 处 · **`env -u …`（继承外层 PATH）7 处**。⇒ **4/16，不是 16/16。**
#   ⚠ **那 7 处继承外层 PATH 的调用没治，如实登记**〔`D3 B3` 的实质点〕：它们真跑 `manifest_to_table`
#     的 `jq` 分支 ⇒ 外层装了别版 `jq` 时测的是另一个二进制。**不改它们**是有理由的：那 7 处
#     （`ccm()` / `acct()` / `inherit_acct()` 等）钉的正是「**继承来的环境**」这件事，把 PATH 钉死
#     就把它们要测的东西测没了。⇒ 改成**把那个巧合变成一条判据**（下一行），红了就说明真出现了两份。
#   ⚠ 换成这条 PATH 之后**多了一个失败面**：顶上那道 fail-closed 量的是**外层** PATH，
#     而这里取的是 `/usr/bin:/bin` —— 两者可以不一致（jq 只装在 `~/bin` 时）。
#     那样 `REALJQ` 会是空串、shim 变成 `exec "" "$@"` ⇒ 一串莫名其妙的红。
#     ⇒ 顶上那道守卫**已经把这条 PATH 也一起量了**（两条各一句话，见文件头）。
REALJQ="$(env -i PATH="/usr/bin:/bin" sh -c 'command -v jq')"
# ★ 把上一段那句「今天两者恰好是同一个」从**巧合**变成**判据**〔08-25，`D3 B3` 的连带面〕。
#   红了不是套件坏了：是这台机器上真有两份 `jq`，那 7 处继承外层 PATH 的 ccm 调用测的就是另一个。
ck "★ 自检：外层 PATH 的 jq 与 /usr/bin:/bin 上的 jq 是**同一个文件**（那 7 处继承外层 PATH 的调用才与 shim 同源）" "same" \
   "$([ "$(command -v jq)" -ef "$REALJQ" ] && echo same || echo "外层=$(command -v jq) 瘦条=$REALJQ")"
mkdir -p "$KTMP/recbin"
cat > "$KTMP/recbin/jq" <<EOF
#!/bin/sh
_in="\$(readlink "/proc/\$\$/fd/0" 2>/dev/null)"
printf '%s\n' "\$* <stdin=\${_in}>" >> "$KTMP/jqcalls"
exec "$REALJQ" "\$@"
EOF
chmod +x "$KTMP/recbin/jq"
KREC() { # KREC <daemon 或 -> <ccm 参数…>：同 K()，但 PATH 前置一个记账 jq
  local d="$1"; shift
  rm -f "$KTMP/calls" "$KTMP/jqcalls"; : > "$KTMP/out"; : > "$KTMP/err"
  local -a envs=(HOME="$KTMP" CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent
                 CCM_ACCTS_MANIFEST="$KTMP/accts/accounts.json")
  [ "$d" != - ] && envs+=(CCM_DAEMON_BIN="$d")
  env -i PATH="$KTMP/recbin:/usr/bin:/bin" "${envs[@]}" bash "$CCM" --cwd /p --print "$@" \
      > "$KTMP/out" 2> "$KTMP/err"
}
KJQMF() { local n; n="$(grep -cF -- "$KTMP/accts/accounts.json" "$KTMP/jqcalls" 2>/dev/null)"; printf '%s' "${n:-0}"; }
# 尺子自检（成对的那一半，08-24 D2 `B1` 补）：**两种读法各跑一次**，证明这把尺子两形都看得见。
# 少了它，「manifest 一次都没被解析」那条会在「换个写法去读」时**静默失明** —— `MU-A1b` 就是那一刀。
: > "$KTMP/jqcalls"
env -i PATH="$KTMP/recbin:/usr/bin:/bin" sh -c \
    "jq -r '.accounts' '$KTMP/accts/accounts.json' >/dev/null 2>&1; jq -r '.accounts' < '$KTMP/accts/accounts.json' >/dev/null 2>&1"
ck "KCY1 · 尺子自检：记账 jq 对**路径当 argv**与**stdin 直连**两种读法都看得见（各 1 次 ⇒ 2）" "2" "$(KJQMF)"
KREC "$KTMP/bin/daemon" --account z
ck "★ KCY1 · daemon 在位 ⇒ 那份 manifest **一次都没被解析**（不校对、不合并、不「读了不用」）" \
   "0" "$(KJQMF)"
ck "KCY1 · 上一条的前提自检：这一跑确实走了 daemon（拿到的是 daemon 那个目录）" \
   "$(GOLD "$KTMP/from-daemon")" "$(KOUT)"
KREC - --account z
ck "KCY1 · 夹具自检：无 daemon 时那份 manifest **确实**被解析了 1 次 ⇒ 这把尺子会说话" \
   "1" "$(KJQMF)"

# ---- KCY2：降级策略是裁过的，判据钉住裁的那一条（退出码 **与** stderr 文本）----
# §0b 裁定 = 诚实降级 + **出声**。⇒ 只断退出码不够（那会让「出声」退化成「闷声」）。
RC="$(K - "" --account z)"
ck "★ KCY2 · 无 daemon ⇒ **照旧起得来**（rc=0，不是响亮失败）" "0" "$RC"
ck "★ KCY2 · 无 daemon ⇒ stderr **有那句话**（降级不许闷声）" "yes" \
   "$(grep -q '账号解析已降级' "$KTMP/err" && echo yes || echo no)"
ck "★ KCY2 · 那句话说得出**读的是哪个文件**（诊断得能定位）" "yes" \
   "$(grep -qF "$KTMP/accts/accounts.json" "$KTMP/err" && echo yes || echo no)"
ck "★ KCY2 · 那句话说得出**为什么**降级（这一格：找不到 daemon）" "yes" \
   "$(grep -q '找不到 daemon' "$KTMP/err" && echo yes || echo no)"
ck "★ KCY2 · 四种降级原因**互斥**：这一格只许命中一格（不许合并成一句「daemon 不可用」）" "1" "$(KWHY)"
ck "★ KCY2 · 那句话说清了**性质**（后端本该是唯一真相源 / 你拿到的是文件那一份）" "yes" \
   "$(grep -q '唯一真相源' "$KTMP/err" && grep -q '文件那一份' "$KTMP/err" && echo yes || echo no)"
K "$KTMP/bin/daemon" "" --account z >/dev/null
ck "★ KCY2 · daemon 在位 ⇒ stderr **一个字都没有**（别把正常路径变吵）" "" "$(KERR)"
K "$KTMP/bin/daemon" "CCM_NO_DAEMON=1" --account z >/dev/null
ck "KCY2 · CCM_NO_DAEMON=1 ⇒ 也降级、也说话，且说的是**那一格**（明示整条关掉）" "yes" \
   "$(grep -q 'CCM_NO_DAEMON=1（明示整条关掉 daemon）' "$KTMP/err" && echo yes || echo no)"
ck "KCY2 · 四种降级原因**互斥**：CCM_NO_DAEMON=1 这一格只许命中一格" "1" "$(KWHY)"
ck "KCY2 · CCM_NO_DAEMON=1 ⇒ 拿到的是文件那一份（逃生口真的把 daemon 那条关掉了）" \
   "$(GOLD "$KTMP/from-file")" "$(KOUT)"
cp "$KTMP/accts/accounts.json" "$KTMP/accts/m.json"
rm -f "$KTMP/calls"; : > "$KTMP/err"
env -i PATH="/usr/bin:/bin" HOME="$KTMP" CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent \
    CCM_ACCTS_MANIFEST="$KTMP/accts/m.json" CCM_DAEMON_BIN="$KTMP/bin/daemon" \
    bash "$CCM" --cwd /p --account z --print > "$KTMP/out" 2> "$KTMP/err"
ck "KCY2 · manifest 叫别的名字 ⇒ 说的是**那一格**（--accts-dir 表达不了它），不是含糊的「daemon 不可用」" "yes" \
   "$(grep -q 'daemon 的 --accts-dir 表达不了它' "$KTMP/err" && echo yes || echo no)"
ck "KCY2 · 四种降级原因**互斥**：--accts-dir 表达不了 这一格只许命中一格" "1" "$(KWHY)"
ck "KCY2 · 那一格**不许悄悄去问 daemon**（问了就是读了另一个文件）：调用次数 0" "0" "$(KCALLS)"
K "$KTMP/bin/mute" "" --account z >/dev/null
ck "KCY2 · daemon 在位但**答不出** ⇒ 说的是那一格，并落回文件" "yes" \
   "$(grep -q '答不出 --list-accounts' "$KTMP/err" && echo yes || echo no)"
ck "KCY2 · 四种降级原因**互斥**：答不出 这一格只许命中一格" "1" "$(KWHY)"
ck "KCY2 · daemon 答不出 ⇒ 值来自文件（诚实降级，不是报错、也不是空账号）" \
   "$(GOLD "$KTMP/from-file")" "$(KOUT)"
# ★ 反面：这句话**不许变成噪音**。压根没有账号库的机器上 ccm 就是个基座启动器。
rm -f "$KTMP/calls"; : > "$KTMP/err"
env -i PATH="/usr/bin:/bin" HOME="$KTMP" CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent \
    CCM_ACCTS_MANIFEST="$KTMP/accts/nope.json" \
    bash "$CCM" --cwd /p --print > "$KTMP/out" 2> "$KTMP/err"
ck "★ KCY2 · **无账号库 ⇒ 一个字都不说**（降级提示不许变成每次都吵的噪音）" "" "$(cat "$KTMP/err")"

# ---- KCY3（我们这一侧）：那个面加字段不许把我们打碎 ----
# ⚠ 如实边界：这条证的是「**我们**的解析器对新增字段是宽的」，**不是**「aterm 那边不碎」——
#   他们的 golden 向量在他们仓里，不在我们的扫描面上（件文件 §4 `KC6a`）。
K "$KTMP/bin/extra" "" --account z >/dev/null
ck "★ KCY3 · daemon 输出里多一个**未知字段** ⇒ 照样解析对（前向兼容；照 aterm 那条 golden 的形状）" \
   "$(GOLD "$KTMP/from-daemon")" "$(KOUT)"
ck "KCY3 · 多字段那次也不吵" "" "$(KERR)"
# ★★ 反面那一格：**如实钉住今天的空档**〔审计 `I6`，08-24 补〕。
#   前向兼容（那边**加**字段）我们是宽的，上面两条钉着。但那边**改键名 / 抽掉键**时会怎样，
#   今天一条判据都没有 —— 而 `--list-accounts` 那个面正在动（`K-A1` 08-24 刚往它加了
#   `authKind`/`authReady`）。实测的行为是：meta 那行在 ⇒ 我们判「答上了」⇒ 账号行一个都
#   解析不出来也**照收这张空表**，`_ccm_acct_src=daemon`、**不降级、不出声**
#   ⇒ 用户看到 `可用: (无账号库)` 而 manifest 明明在。
#   这条判据钉的是「**今天就是这个样子**」：它红了说明有人动了这一格，好坏都得被看见。
#   ⚠ 它**不会**因为真 daemon 改了帧形状而红（夹具在我们这一侧）——那一半住件文件 `§4 KC6g`。
#   ★★ **红了之后该干什么，分两种**〔审计 `S2`，08-24 补〕：
#     · 你是**动坏了**这一格 ⇒ 照红名单修回来；
#     · 你是**在修**这个空档（让空表也降级出声、或干脆判「答不出」）⇒ 那是**改善**，
#       但下面这两条会红，而红出来的话（「今天拿到的是空表，照旧 die」「而且一声不吭」）
#       **读起来像回归**。此时正确的动作是：**把这两条一起改成新行为**，并把件文件
#       `§4 KC6g` 里「本轮做到哪一步」那段的登记**删掉**（它记的就是这个空档）。
#     审计 `MU-G1` 实测的正是第二种：一次改善，代价恰好 2 红 = 下面这两条。
#   ⚠ 另如实登记（审计 `S1`）：下面第一条（rc=2）被第二条（整段 stderr 逐字）**完全覆盖** ——
#     审计造的 5 把会红它们的刀里**没有一把只红第一条**（`MU-C` 甚至只红第二条）。
#     留着不亏（rc 与 stderr 是两个面），但**别把它算进「有独立区分力」那一栏**。
cat > "$KTMP/bin/renamed" <<EOF
#!/bin/sh
echo call >> "$KTMP/calls"
case "\$1" in
  --list-accounts)
    printf '%s\\n' '{"accountZeroAware":true,"acctsDir":"x","count":2,"enabled":true,"error":null,"kind":"accounts-meta","manifestPath":"x","sharedStore":null,"updatedAt":null}'
    printf '%s\\n' '{"acctName":"z","cfgDir":"$KTMP/from-daemon","primary":false}'
    exit 0 ;;
esac
cat >/dev/null; exit 0
EOF
chmod +x "$KTMP/bin/renamed"
RC="$(K "$KTMP/bin/renamed" "" --account z)"
ck "★ KCY3/KC6g · 帧形状变了（账号行改键名）⇒ 今天拿到的是**空表**，照旧 die" "2" "$RC"
ck "KCY3/KC6g · …而且**一声不吭**：不算「答不出」⇒ 不降级、不提示（今天的空档，见件文件 §4）" \
   "ccm: 账号 'z' 不可用（不在 $KTMP/accts/accounts.json，或其目录不存在）。可用: (无账号库)" \
   "$(KERR)"

# ---- KCY4：能力协商面 ----
PROBE_K="$(env -i PATH="/usr/bin:/bin" HOME="$KTMP" CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent bash "$CCM" --ccm-probe 2>&1)"
ck "★ KCY4 · capabilities= 里有 account-via-daemon（消费者据此分辨新旧 ccm）" "1" \
   "$(printf '%s\n' "$PROBE_K" | sed -n 's/^capabilities=//p' | tr ',' '\n' | grep -cx 'account-via-daemon')"
ck "KCY4 · capabilities 变了 ⇒ 版本号跟着走（既有纪律：不能只改后者）" "version=3" \
   "$(printf '%s\n' "$PROBE_K" | grep '^version=')"
# ⚠ 描述里那对反引号**必须转义**〔08-24 逮到〕：不转义的话它是**命令替换**，
#   每跑一次套件就真去 exec 一个叫 `--help` 的命令、往 stderr 吐一行 `--help: 未找到命令`，
#   而且判定行打出来的名字**是被替换过的残句**（那一格看着像描述写漏了）。同文件另两处一直是转义的。
ck "KCY4 · 用法块里有那一行（\`--help\` 找得到它；Rust 侧 every_advertised_capability_has_a_usage_line 也查这个）" "yes" \
   "$(grep -q -- '--account-via-daemon' "$CCM" && echo yes || echo no)"

# ---- KCM6：**无 jq** 那条兜底（本仓此前没有任何门禁走得到它）----
# 08-24 摸底时在这一格逮到一个自己写的真缺陷（末块被 `read` 丢掉 ⇒ 表变空、
# 症状是「可用: (无账号库)」而 manifest 明明在）。装了 jq 的机器与 CI 都走 jq 那条 ⇒ 零覆盖。
for _b in bash sh sed; do
  _p="$(command -v "$_b" 2>/dev/null)"; [ -n "$_p" ] && ln -sf "$_p" "$KTMP/nojq/$_b"
done
# 「这一趟真起了哪些外部进程」的记录器〔审计 `B2` 的洞 ① 与 ③，08-24 换过一次尺子；
#  **08-25 再换一次 —— 上一版量的不是这个性质**，见下。洞 ②（只盖一条解析路）在下面
#  「每跑一次就地量一次」那里治〕。
# **别再去 grep `not found` 那句话**（第二轮的量法）—— 它量的是 stderr 里的英文措辞，两个致命面：
#   ① `cmd 2>/dev/null` 的 fork **逮不到**：bash 在 exec 失败前就已经把 fd 2 换成 `/dev/null`
#      （审计 `MU-K3` 实测 **0 红**；而 `cmd 2>/dev/null` 是真实代码里的常见写法，本文件自己就有）；
#   ② 它认的是**英文**措辞，而 bash 那句话是**本地化**的：本机 locale 打的是「未找到命令」。
#      今天没出事只是因为 `env -i` 顺手把 `LANG`/`LC_ALL` 也清了 —— 谁哪天透传一个 `LANG` 进来
#      （很常见的改动），那条判据就**永远读到 0、永远 PASS**，而且没有任何东西会提醒：**永久假绿**。
# ⚠⚠ **第三轮那把尺子（`command_not_found_handle` + `$0: ` 前缀）量的也不是这个性质**〔08-25，
#   R6 治的就是它〕。它的两半**都**建立在「**PATH 解析失败**」上：`command_not_found_handle` 只在
#   bash 走 PATH 查找失败时触发；`$0: ` 前缀只在 bash 自己 exec 失败时打。而**绝对路径根本不查
#   PATH**，**非 bash 子 shell**（本机 `/bin/sh -> dash`）既没有那个 handler、错误行前缀也不是 `$0: `
#   ⇒ 它量的是「**这台机器的瘦 PATH 上找不着**」，不是「**有没有外部依赖**」。
#   实测（08-25，六刀都注在 `acct_row_from_slice` 的 `[ -n "$n" ] || return 0` 之后，锚点各 1/1）：
#     `x="$(printf %s "$n" | /usr/bin/awk "{print}")"`   绝对路径 · **真存在** · **真被 exec**  ⇒ **0 红**
#     `/nonexistent-dir/xx 2>/dev/null || true`          绝对路径 ENOENT + 重定向              ⇒ **0 红**
#     `/nonexistent-dir/xx || true`                      同上、**只去掉重定向**（非空对照）      ⇒ 2 红
#     `sh -c "nosuchcmd_zz" 2>/dev/null || true`         非 bash 子 shell                      ⇒ **0 红**
#     `sh -c "nosuchcmd_zz" || true`                     同上、不重定向也一样                   ⇒ **0 红**
#     `x="$(printf %s "$n" | awk "{print}")"`            裸名字（非空对照，证明尺子没死）        ⇒ 2 红
#   ⇒ 第一刀注入的是一条**真的、真在跑的硬外部依赖**：把它换成一个会往文件追加一行的包装脚本，
#     **08-25 我自己重打** —— 那一跑里它真被 exec **9 次**，而全套全绿、没有任何信号（**当时**全套
#     **109 条**；这个总数每轮都涨，别拿它跟今天的对）。这份文件经 `sftp.rs`
#     推到**任意远端** ⇒ 一条 `/usr/bin/awk` 在 Alpine/busybox 上就是「换号这条路整条不通」。
# ⇒ **08-25 改成直接数「这一趟真起了哪些外部进程」**。两半互补、**互不重叠**：
#   **半 a（handler）**：bash 走 PATH 查找**失败**的**裸名字**（`awk`/`cut`）。它往文件写、不往
#     stderr 写 ⇒ **不吃重定向、不吃 locale**。（这一半原样留着，它在它那一形上是准的。）
#   **半 c（`DEBUG` 陷阱 ＋ `set -T`）**〔08-25 新增，替掉旧的半 b〕：`BASH_ENV` 里装一个 DEBUG
#     陷阱，逐条命令看 `$BASH_COMMAND` 的**命令字**（先剥掉两侧引号、剥掉 `VAR=值` / `exec` /
#     `command` / `time` / `!` 这些前缀词；命令字带 `$` 时**就地取值** —— `BASH_COMMAND` 是**未展开**
#     的原文，不取值的话 ccm 自己那句 `$to "$_ccm_db" --list-accounts` 一个字都读不到。
#     两条取值路：`$VAR` / `${VAR}` 走 `${!name}` 间接取；**其余 `${…}`**（`${a[0]}` · `${V:-默认}` ·
#     `${argv[@]}`）**交给 bash 自己 `eval` 展一次**〔08-25 R7 补，堵审计 `D4` 逮到的两个真缺口〕，
#     但先过一道**形状闸**：`${…}` 里出现 `}` `$(` 反引号 `(` `)` `;` `&` `|` `<` `>` `=` `?` 之一就**不碰**。
#     ⚠ 闸的三条理由**分量不一样，别当成一档**：
#       ① `$(` / 反引号 / `(` —— **量出来的**：把闸整道拿掉，`${VV:-$(tick)}` 当命令字时那条 `tick`
#          跑了 **2 次**（闸在时 1 次，= 被测自己该跑的次数）⇒ **尺子会把被测的副作用再做一遍**。
#       ② `=`（`${V:=x}` 赋值形）· ③ `?`（`${V:?}` 的展开**本身带一个 exit**）—— 这两条是**预防性**的，
#          **我没造出非空对照**：`${V:=x}` 闸开闸关事后 `V` 都是同一个值（被测自己也会赋），
#          `${V:?}` 闸开闸关被测都退（那是 bash 自己的语义，不是陷阱造成的）。写在这里是因为
#          「尺子不许写被测的变量、尺子里不许有 exit」是硬规矩，不是因为我逮到过。
#          代价今天是 **0**：`shared/ccm` 里 `${V:=…}` **0 处**、`${V:?…}` **0 处**
#          （`grep -o '\${[A-Za-z_][A-Za-z0-9_]*:\?=' shared/ccm | wc -l` 与同法的 `:?`，量于 08-25）。
#     ）：`type -t` 说它是 `file`（真外部二进制 —— `/usr/bin/awk` · `sh` · 以及**名字
#     恰好在瘦 PATH 上**的 `sed`）⇒ 记一次；或者它以 `/` `./` `../` 开头而 `type -t` 说不出
#     （绝对路径 ENOENT）⇒ 记一次。**`set -T` 让陷阱进函数、命令替换与子 shell**（实测：
#     `_ccm_acct_tab="$(manifest_to_table)"` 那层子 shell 里 `FUNCNAME` 是全的）。
#     它**一个字节都不看 stderr** ⇒ **重定向、locale 全都不吃**。
#   为什么两半不重叠：半 a 只在「裸名字 ＋ PATH 上找不着」时触发，而那一形 `type -t` 恰好读作**空**
#     且不以 `/` 开头 ⇒ 半 c 不记它。
#   ⚠ **这里原本写的是「其余每一形都归半 c」**——**一句没有分母、没有对冲的全体断言**，而且**被证伪过**
#     〔审计 `D4` 阻塞-2；08-25 R7 自己重切复现〕：`${a[0]}` 与 `${V:-默认}` 两形当命令字时
#     **两半都不记**，而那是真的、真在跑的外部依赖（留痕包装实测 **9 次 exec**，全套全绿 ——
#     那一刻是**修前**的盘，全套 **123 条**）。
#     ⇒ 今天这句话只许这么写：**我量过的那 17 形里**（下面「逮得到」12 ＋「逮不到」5，逐形一条读数），
#     除半 a 那一形之外，**逮得到的都归半 c、逮不到的两半都不归**；**分母是 17，不是「全部」**。
#     没量过的形该落哪一半，**这里给不出答案**——「逮不到」那张表就是这句话的对冲。
#   **旧的半 b（`grep -cF "$0: " stderr`）08-25 去掉了**，两条理由：① 它吃重定向（上表第 2/3 行）；
#     ② 它把**任何** `$0: ` 诊断行都多算成「命令没找着」—— `: > /nonexistent-dir/zz`（**零 fork、
#     纯 bash 内建**）在它下面读作 2 红，红出来的名字却写着「零外部依赖」（审计 `MU-REDIR`）。
#     半 c 在这一形上读 **0**，下面有一条反向对照钉住它。
# ★★ **射程 = 那三个解析函数的调用栈之内**（`acct_row_from_slice` / `manifest_to_table` /
#   `daemon_out_to_table`）。这是**故意收窄的，并且这一次收窄本身就是修复的一部分**：判据的名字
#   说的就是「**那两条解析路**上零外部依赖」，而整趟 `--print` 跑里**本来就有**外部进程 ——
#   08-25 实测（把射程放宽到整趟）：`sq` 里 **2 次 `sed`**。拿整趟的读数去判「解析路零依赖」
#   就是**尺子的作用域与被判对象对不上**（旧尺子读作 0 只是因为 `sed` **恰好在瘦 PATH 上**，
#   那正是它自己登记的瞎点 ①）。⇒ 栈外那一面由「身份前置检查」那节的既有断言、
#   以及下面那两条 `★ KC6d/整趟` 判据（单向棘轮）管，**不由这两条 KC6d 判据管**。
# ★ **逮得到 / 逮不到 —— 逐形实测，与 `shared/ccm` 那份头注是同一份话**〔铁律：射程只写在测试里
#   等于没写；改 `acct_row_from_slice` 的人读的是那边，跑这套的人读的是这边，**两处都写才算存在**〕。
#   ★ **分母 = 下面这 17 形**（逮得到 12 ＋ 逮不到 5），**不是「全部绕法」**。17 形逐形读数都是
#     08-25 R7 在**单形隔离探针**上重打的（把该形放进 `acct_row_from_slice` 单独跑、回读 KNF；
#     与 `KPROBE` 同一条 env 构造）。⚠ **量具不同，别把两种读数混着报**：这 17 个是**探针**读数，
#     「切一刀变 N 红」说的是**全套跑**出来的红数 —— 上一版就是把这两种混在一句里。
#   ⚠⚠ **这里不再写「每形变 N 红」，那个数写下来就会腐**〔审计 `D4` 重要-1：上一版写死「每形…变 2 红」，
#     而同一个 commit 新加的两条整趟棘轮让其中好几形变成了 4 红 —— **数写在那儿，而盘变了**〕。
#     真正稳的判法是**看红的是哪一族**：切一刀之后红的**恒是 `KC6d` 那一族**，几条取决于两件**会变**的事 ——
#     ① 今天 `KC6d` 判据共 **4 条**（热路径 2 ＋ 整趟 2，量于 08-25 R7）。量具要**行首锚定**：
#        `grep -c '^ck "★ KC6d'` ⇒ 4；**不锚定就读作 5**，因为**这一行注释自己**含那个字面
#        〔审计 `D4` 建议-3 的同族病 —— 我写这行时当场又犯了一次，靠自核逮住，故把量具连同这个坑一起留在这儿〕；
#     ② 注入的那个**命令名在不在整趟棘轮的白名单里**。08-25 R7 实测三形，正好把这两件都照出来：
#       裸名字 `awk` ⇒ **4 红**（4 条全中）· 非 bash 子 shell `sh` ⇒ **4 红** ·
#       **`sed` ⇒ 只有 2 红**（它在整趟那两条的白名单里，整趟不响，只响热路径）。
#     ⇒ **复刀请数「红的是不是 `KC6d` 这一族」，别数「几红」。**
#   **逮得到（12 形）**：① 裸名字（PATH 上找不着）· ② **绝对路径且真存在** · ③ 绝对路径 ENOENT ·
#     ④ **非 bash 子 shell**（`sh` 自己）· ⑤ 名字**恰好在瘦 PATH 上**的 · ⑥ 命令字是 `$VAR` / `${VAR}` ·
#     ⑦ `eval "$cmd …"` 里的（`eval` 重新解析 ⇒ 陷阱照样触发）· ⑧ 「找得到但**跑失败**」的
#     （`sed --nosuchopt` ⇒ 记到 `sed`，它照样是一次真 exec）· ⑨ **进程替换** `< <(…)` · ⑩ 后台 `&` ·
#     ★⑪ **数组元素当命令字**（`"${a[0]}"` / `exec "${argv[@]}"`）· ★⑫ **`${V:-默认}` 当命令字**。
#     ⚠ **这两形在 `shared/ccm` 里有多少处，量的时候必须钉两件事：只数生产段 · 数「出现次数」还是「行数」**。
#     08-25 R7 实测（先 `strip_hash_comment_lines` 剥掉整行注释再数）：`${V:-…}` **生产段 22 处**
#     （按行数是 19）；`exec "${…[@]}"` **1 处**。同一条命令**不剥注释**读作 25 —— 多出来的三处
#     **就是这份注释自己在举例**。〔审计 `D4` 报的是 21，那是**行数口径 ＋ 剥注释前**的数：
#     两个口径都没错，错在拿它们互相对账。这正是本仓最高频的那一族「量具的作用域跟被判对象对不上」。〕
#     ⑪⑫ 是 **08-25 R7 新堵的**〔审计 `D4` 逮到的两个真缺口〕：在此之前它们**两半都不记**，
#     而它们是真的、真在跑的外部依赖 —— 留痕包装实测 **9 次 exec**，全套全绿、零信号
#     （**修前**那一刻全套 123 条）。
#     ⑪⑫ 各有一条自检钉着（⑬⑭），别删。
#   **逮不到（5 形）**：① 经非 bash 子进程再起的**孙命令**（只记到 `sh` 本身：实测 `/bin/sh -c "…/tick"`
#     ⇒ 记到 `/bin/sh`，而 `tick` 真跑了 1 次）；② **命令字**是 `$(...)` 拼出来的；③ **位置参数**当命令字
#     （`set -- /x; "$1"`）；★④ `${V:=默认}` 赋值形；★⑤ `${V:?…}` 形。
#     ④⑤ 是**形状闸主动排掉的**（不是漏了 —— 理由与代价见上方半 c 那段：尺子不许写被测的变量、
#     尺子里不许有 exit；今天 `shared/ccm` 里这两形各 0 处 ⇒ 排掉不花覆盖）。
#     ①②③ 是**真缺口**：在热路径上加一条这样的真依赖，全套今天照样全绿 —— 08-25 R7 重切
#     「位置参数」那一形（`set -- <包装>; "$1" …`，锚点 1/1、判定行 126）：包装**真被 exec 9 次**，
#     而全套 **126/0**、一条都没红。
# ⚠ 陷阱里**一个 `[[ =~ ]]` 都不许有**：`acct_row_from_slice` 是 `[[ =~ ]]` 之后**紧接着**读
#   `${BASH_REMATCH[1]}`，而 DEBUG 陷阱正好在这两条之间触发 —— 陷阱一碰 `BASH_REMATCH`
#   就把被测代码解析坏（读出空 name）。全部用 `case`。同理陷阱**首行存 `$?`、每条 return 都把它
#   原样送回去**（不存的话被测代码看到的 `$?` 是陷阱的）。这两条 08-25 各实证过一次。
cat > "$KTMP/cnf.bash" <<EOFCNF
_KA="$KTMP/cnf"; _KC="$KTMP/ext"; _KAALL="$KTMP/cnfall"; _KCALL="$KTMP/extall"
_kin() {   # 调用栈里有没有那三个解析函数（= 射程判据，半 a / 半 c 共用一份）
  case " \${FUNCNAME[*]} " in
    *" acct_row_from_slice "*|*" manifest_to_table "*|*" daemon_out_to_table "*) return 0 ;;
  esac
  return 1
}
_kext() {
  local _r=\$? _c=\$BASH_COMMAND _w _v _t _b _o _p
  while :; do
    _w=\${_c%% *}
    case "\$_w" in \"*\") _w=\${_w#\"}; _w=\${_w%\"} ;; \'*\') _w=\${_w#\'}; _w=\${_w%\'} ;; esac
    case "\$_w" in                       # 命令字是 \`\$VAR\` / \`\${VAR}\` ⇒ 就地取值（BASH_COMMAND 是**未展开**的原文）
                                         # ⚠ **这里只管这两形**，其余 \`\${…}\` 在下面那个 \`else\` 里
      '\${'*'}') _v=\${_w#'\${'}; _v=\${_v%'}'} ;;
      '\$'*)     _v=\${_w#'\$'} ;;
      *)         _v= ;;
    esac
    case "\$_v" in ''|*[!A-Za-z0-9_]*|[0-9]*) _v= ;; esac
    if [ -n "\$_v" ]; then
      _w=\${!_v-}; _w=\${_w%% *}
    else                                 # 不是光秃秃一个名字的 \`\${…}\`（\`\${a[0]}\` · \`\${V:-默认}\` · \`\${argv[@]}\`）
      case "\$_w" in                     # ⇒ **交给 bash 自己展一次**，但先过下面那道形状闸
        '\${'*'}')
          _v=\${_w#'\${'}; _v=\${_v%'}'}
          case "\$_v" in                 # 闸：带**可执行成分**或**会写/会退**的一律不碰（逐条理由见上方头注）
            ''|*'}'*|*'\$('*|*'\`'*|*'('*|*')'*|*';'*|*'&'*|*'|'*|*'<'*|*'>'*|*'='*|*'?'*) ;;
            *) eval "_w=\\\${\$_v}" 2>/dev/null || _w=; _w=\${_w%% *} ;;
          esac ;;
      esac
    fi
    case "\$_w" in
      ''|*=*|exec|command|time|!)
        # 丢掉这个前缀词之前先**把它补到括号平衡**〔审计 \`D4\` 重要-2，08-25 R7 修〕：
        # \`VAR="\$(cmd 参数…)"\` 这一形里 \`\${_c%% *}\` 只切到 \`VAR="\$(cmd\`，
        # 再往下走就**走进了那层命令替换**，而那一层自己还有一次 DEBUG 触发（\`set -T\`）
        # ⇒ **同一次 exec 记两遍**。补齐再整段丢掉，就落在命令替换**之后**。
        _b=\${_c%% *}
        while :; do
          _o=\${_b//[!(]/}; _p=\${_b//[!)]/}
          [ \${#_o} -le \${#_p} ] && break
          case "\$_c" in *" "*) _c=\${_c#* }; _b=\$_b' '\${_c%% *} ;; *) return \$_r ;; esac
        done
        case "\$_c" in *" "*) _c=\${_c#* }; continue ;; *) return \$_r ;; esac ;;
      *) break ;;
    esac
  done
  _t=\$(type -t "\$_w" 2>/dev/null)
  case "\$_t" in
    file) ;;
    *) case "\$_w" in /*|./*|../*) ;; *) return \$_r ;; esac ;;
  esac
  printf '%s\n' "\$_w" >> "\$_KCALL"
  _kin && printf '%s\n' "\$_w" >> "\$_KC"
  return \$_r
}
command_not_found_handle() {
  printf '%s\n' "\$1" >> "\$_KAALL"
  _kin && printf '%s\n' "\$1" >> "\$_KA"
  return 127
}
trap '_kext' DEBUG
set -T
EOFCNF
# ★★ **瘦 PATH 那一跑的环境只许有一份**〔08-24 实测逼出来的：见下〕。尺子自检与被测跑必须走
#   **同一条 env 构造** —— 第一版把 `BASH_ENV=…` 分别写在两处，于是我造了一刀「只把被测那一跑的
#   `BASH_ENV` 删掉」：**自检照样绿、两条热路径判据读作 0、全套全绿**（当时 109 条）⇒ 尺子早已失明而没人知道。
#   那正是本轮在治的那个病（「尺子的作用域与被判对象对不上」）在**我自己刚写的代码**里的复发。
#   ⇒ 抽成一份数组，两边都用它。**这个读数每加一条自检就变，别当常量**（R4 写「1 红」是自检只有
#   一条时的数；R6 重打成「10 红」；08-25 R7 又重打）：删掉里面的 `BASH_ENV`（锚点 1/1，判定行 **126**）
#   ⇒ **13 红** = 自检 ①–⑨（期望非零那 9 条）＋ ⑫（活体对照）＋ ⑬⑭⑮（R7 新加的三条）；
#   **⑩⑪ 期望 0，尺子全瞎时照旧绿** —— 反向对照本来就该这样，
#   而 ⑫ 存在的理由正是「差集恒空」这种绿不算数。
NOJQ_ENV=(env -i PATH="$KTMP/nojq" BASH_ENV="$KTMP/cnf.bash")
KRESET() { : > "$KTMP/err"; : > "$KTMP/cnf"; : > "$KTMP/ext"; : > "$KTMP/cnfall"; : > "$KTMP/extall"; }
NOJQ() { # NOJQ <daemon 或 -> <manifest> [账号名，默认 z]
  local d="$1" m="$2" a="${3:-z}"
  KRESET
  local -a envs=(HOME="$KTMP" CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$m")
  [ "$d" != - ] && envs+=(CCM_DAEMON_BIN="$d")
  "${NOJQ_ENV[@]}" "${envs[@]}" bash "$CCM" --cwd /p --account "$a" --print 2>"$KTMP/err"
}
# 上一次 NOJQ / KPROBE 那一跑里，**射程内**这把尺子**记下的外部命令字次数**（两半相加、互不重叠）。
# ⚠ **名字只许承诺它真在数的东西**〔审计 `D4` 重要-2；08-25 R7 订正 ＋ 修〕。这里原写
#   「起过的外部进程**次数**」，**那是假的**：`VAR="$(<空变量> cmd …)"` 这一形**同一次 exec 记两遍**
#   —— 外层那次触发剥掉 `VAR="$(cmd` 这个前缀词之后**走进了那层命令替换**，而那一层自己还有
#   一次触发。R7 自己重打（单形隔离探针，包装脚本每跑一次留一行痕）：
#     `_zto=; _zx="$($_zto <包装> …)"`  ⇒ KNF **2**，包装**真 exec 1 次**   ← 翻倍
#     `_zx="$(<包装> …)"`（非空对照）    ⇒ KNF **1**，真 exec 1 次
#     裸调用 `<包装> …`（非空对照）      ⇒ KNF **1**，真 exec 1 次
#   ⇒ **只有「空变量当命令字」那一形会翻倍**。而那正是 `shared/ccm` 的 `acct_table_load` 里
#   `out="$($to "$_ccm_db" --list-accounts …)"` 用的惯用法（活体：整趟那本账里 daemon 记了 2 次，
#   而判据名写的是「那**一次** daemon 往返」）。
#   R7 把它修了（前缀词**补到括号平衡**再丢，见 `_kext`），并由**自检⑮**钉住。
#   ⇒ 今天这句话的分母：**在我量过的那 17 形 ＋ ⑮ 那一形上**，它与真 exec 次数相等；**没量过的形
#   不敢担保** —— 它的定义始终是「陷阱**记下的命令字次数**」，不是「进程数」。别再把名字写大一格。
KNF() {
  local a b
  a="$(grep -c . "$KTMP/cnf" 2>/dev/null)"; b="$(grep -c . "$KTMP/ext" 2>/dev/null)"
  printf '%s' "$(( ${a:-0} + ${b:-0} ))"
}
# 上一次那一跑里，**整趟**（不限射程）起过的、**声明清单之外**的外部命令名。空 = 没有新依赖。
# 它是「栈外那一面」的账：谁哪天给 `--print` 这条路加一条新的外部依赖，这里会当场多出一个名字。
# ⚠ **这句话的射程 = 上面那张「逮得到」表的 12 形，不是「任何新依赖」**〔审计 `D4` 阻塞-2 的连带面；
#   08-25 R7 订正〕。它记的名字来自**同一个** `_kext` / `command_not_found_handle`
#   ⇒ **尺子逮不到的形，这本账同样一个名字都不多**。原写法是一句没有分母的全体断言，而它**被证伪过**：
#   `${a[0]}` / `${V:-默认}` 两形在 R6 那把尺子下是**真依赖而这本账全空**（R7 已把这两形堵上）。
#   今天仍然漏的是「逮不到」表那 5 形 —— 其中**孙命令 · `$(...)` 拼命令字 · 位置参数**是真缺口。
# ⚠ **刻意做成单向棘轮，不是「集合逐字相等」**〔08-25 实测逼出来的〕：先前写的是「集合 == sed」，
#   于是 `MU-E1`（删末块守卫 ⇒ 表变空 ⇒ ccm 提前 die ⇒ 连 `sq` 都没跑到）把它一起打红，实得空串。
#   那是**诊断指错方向**（红出来写着「外部依赖」，真因是解析丢了末块）—— 正是审计 `B2` 判的那个病。
#   单向之后：解析坏掉 ⇒ 差集空 ⇒ 这条不响（该响的是 `KCM6` 那几条）；**加一条新依赖才响**。
# ⚠ 它是「差集该是空的」那一族 ⇒ **空真风险**（闸死了 `[] == []` 照样成立）。
#   下面「尺子自检⑫」是它的**活体对照**：故意起一个清单外的外部进程，它必须出现在差集里。
KALLNEW() { # KALLNEW <允许出现的名字，ERE 全词> —— 差集，去重排序、空格分隔
  cat "$KTMP/cnfall" "$KTMP/extall" 2>/dev/null | sed "s#^$KTMP#<KTMP>#" | sort -u \
    | grep -vxE "$1" | tr '\n' ' ' | sed 's/ *$//'
}
ck "自检：nojq PATH 里**真的没有 jq**（不然下面两条测的还是 jq 那条）" "0" \
   "$(ls "$KTMP/nojq" | grep -cx jq)"
# ★★ **尺子自检：一形一条，不再是一条盖三形的单点**〔08-25，审计 `S2` + 本轮 DoD〕。
#   第三轮那条自检是**一条判据盖三形**（读数 3），有两个后果：① 它只覆盖了旧尺子**碰巧会说话**的
#   那三形，`绝对路径 · 真存在` / `绝对路径 ENOENT + 重定向` / `非 bash 子 shell` 这三形**一形都没测**
#   （而那三形正是旧尺子的瞎点）；② 它是**单点**：谁把期望 3 改成 2，全套（**当时** 109 条）里一条都不会红，
#   而两条热路径判据从此永远读 0（审计 `S2` 逐字：「那条尺子自检承住了四刀但它是单点」）。
#   ⇒ 拆成**一形一条**，外加**两条反向对照**（不许算的东西不许算）。每一形单独失明 ⇒ 单独一条红。
#   ⚠ **别删这一族**：删了它们，两条 `KC6d` 判据会在尺子失明之后永远读 0、永远 PASS。
KPROBE() { # KPROBE <in|out> <一行 shell>...  —— 把这几行放进一个**射程内 / 射程外**的函数里跑，回读 KNF
  local where="$1" fn; shift
  if [ "$where" = in ]; then fn=acct_row_from_slice; else fn=not_a_parser_at_all; fi
  KRESET
  { printf '%s() {\n' "$fn"; printf '%s\n' "$@"; printf '  return 0\n}\n%s x\n' "$fn"; } > "$KTMP/nfprobe"
  "${NOJQ_ENV[@]}" LANG=zh_CN.UTF-8 LC_ALL=zh_CN.UTF-8 bash "$KTMP/nfprobe" >/dev/null 2>"$KTMP/err"
  KNF
}
ck "尺子自检① 裸名字 fork（PATH 上找不着 ⇒ 半 a 记）" "1" \
   "$(KPROBE in "awk 'BEGIN{}' </dev/null")"
ck "尺子自检② 裸名字 ＋ **stderr 被重定向**（半 a 不吃重定向）" "1" \
   "$(KPROBE in "cut -f1 </dev/null 2>/dev/null")"
ck "尺子自检③ **绝对路径 ENOENT**（半 c 记）" "1" \
   "$(KPROBE in "/nonexistent-dir/xx || true")"
ck "尺子自检④ **绝对路径 ENOENT ＋ 重定向** ⇒ 照样记〔08-25 新钉：旧尺子在这一形读 **0**〕" "1" \
   "$(KPROBE in "/nonexistent-dir/xx 2>/dev/null || true")"
ck "尺子自检⑤ **绝对路径 ＋ 真存在的二进制** ⇒ 照样记〔08-25 新钉：旧尺子读 **0**，而它是真依赖〕" "1" \
   "$(KPROBE in "$KTMP/nojq/sed -n 1p </dev/null")"
ck "尺子自检⑥ **非 bash 子 shell**（dash 没有 handler、前缀也不是 \$0:）〔08-25 新钉：旧尺子读 **0**〕" "1" \
   "$(KPROBE in "sh -c 'nosuchcmd_zz' 2>/dev/null || true")"
ck "尺子自检⑦ 名字**恰好在瘦 PATH 上**的外部进程 ⇒ 照样记〔08-25 新钉：旧尺子登记过的瞎点 ①〕" "1" \
   "$(KPROBE in "sed -n 1p </dev/null")"
# ⑧ 是**加法自检**：三形同跑必须读作 3。少一形读 2；**读 4 有两个已知因由，别只想到一个**：
#   ① 两半重叠计数；② **同一次 exec 记两遍**（`VAR="$(<空变量> cmd …)"` 那一形，R7 修掉了、由 ⑮ 钉着）。
#   ⑧ 这三形都是**简单命令、没有命令替换** ⇒ 因由 ② 在它身上不会发生；写在这里是防止下一个人
#   看见 4 就直接跳到「两半重叠」上去（那正是「诊断指错方向」那一族）。「两半互不重叠」
#   这句话在这里是**可证伪**的，不是一句断言。（KPROBE 全程透传 `LANG`/`LC_ALL=zh_CN.UTF-8`
#   ⇒ 上面每一形都已经是在中文 locale 下量的，两半都不认措辞。）
ck "尺子自检⑧ 三形同跑 ⇒ 恰好 3（两半相加、互不重叠；中文 locale 下量的）" "3" \
   "$(KPROBE in "awk 'BEGIN{}' </dev/null" "/nonexistent-dir/xx 2>/dev/null || true" "sh -c 'nosuchcmd_zz' 2>/dev/null || true")"
ck "尺子自检⑨ **命令字是变量**（\`\$to \"\$db\" …\` —— ccm 调 daemon 就是这一形）⇒ 就地取值" "1" \
   "$(KPROBE in "to=" "db=$KTMP/nojq/sed" "\$to \"\$db\" -n 1p </dev/null")"
ck "尺子自检⑩ **反向对照**：纯内建 ＋ 重定向失败 **不许**算成外部依赖（旧半 b 在这一形假红 2）" "0" \
   "$(KPROBE in ": > /nonexistent-dir/zz || true")"
ck "尺子自检⑪ **反向对照**：**射程之外**的外部进程不算（射程 = 那三个解析函数的栈内）" "0" \
   "$(KPROBE out "$KTMP/nojq/sed -n 1p </dev/null" "awk 'BEGIN{}' </dev/null 2>/dev/null || true")"
# ⑫ 是上面那本「整趟差集」的**活体对照**（差集该是空的 ⇒ 空真风险，见 KALLNEW 头注）：
#   这一跑故意在**射程外**起一个外部进程，并把清单换成一个匹配不上任何东西的名字 ⇒ 差集必须非空。
KPROBE out "$KTMP/nojq/sed -n 1p </dev/null" >/dev/null
ck "尺子自检⑫ **活体对照**：整趟那本账不是空真（射程外起一个清单外的进程 ⇒ 差集里必须有它）" \
   "<KTMP>/nojq/sed" "$(KALLNEW 'no-such-name-at-all')"
# ⑬⑭ 是 **08-25 R7 补的两条正向自检**〔审计 `D4` 的两个真缺口，见上方「逮得到 ⑪⑫」〕。
#   ⚠ **号接在末尾、不重排 ⑩⑪⑫**：重排会一次改掉三条判据的**名字**，而判据名是 CI 地板对账
#   与下一轮复刀的锚（`assert-pass-floor` 那一族按条数、审计按名字）—— 为了「号连着好看」
#   去动三个锚，代价大过收益。读的时候把 ⑬⑭ 当成 ⑨ 的后邻（都是正向、都断言 1）。
#   ⚠ **别删这两条**：删了它们，那道形状闸退化回去也不会有任何信号 —— 这两形在 R6 那把尺子上
#   正是「真依赖 ＋ 全套全绿」（**修前**那一刻全套 123 条）。
ck "尺子自检⑬ **数组元素当命令字**（\`exec \"\${argv[@]}\"\` —— \`shared/ccm\` 自己就有 1 处）⇒ 就地取值" "1" \
   "$(KPROBE in "_za=($KTMP/nojq/sed)" "\"\${_za[0]}\" -n 1p </dev/null")"
ck "尺子自检⑭ **\`\${VAR:-默认}\` 当命令字**（\`shared/ccm\` **生产段** 22 处 \`\${VAR:-…}\`）⇒ 就地取值" "1" \
   "$(KPROBE in "\${SEDX:-$KTMP/nojq/sed} -n 1p </dev/null")"
# ⑮ 钉的是**计数口径**，不是「逮不逮得到」：`VAR="$(<空变量> cmd …)"` 这一形逮得到，
#   但修之前**同一次 exec 记两遍**（读 2）。⑮ 断言它读 **1**。⚠ 这一条与两条 `KC6d/热路径`
#   判据**不是同一件事**：那两条断言 0，翻倍不会让 0 变非 0 ⇒ **它们逮不到这个病**，只有 ⑮ 逮得到。
ck "尺子自检⑮ **计数口径**：\`VAR=\"\$(<空变量> cmd …)\"\` 同一次 exec 只算 **1**（\`shared/ccm:acct_table_load\` 就是这一形）" "1" \
   "$(KPROBE in "_zto=" "_zx=\"\$(\$_zto $KTMP/nojq/sed -n 1p </dev/null)\"")"
# ★★ **热路径零外部依赖**，这一条有名字〔审计 `I5`，08-24 补；`B2` ②，08-24 补齐第二条解析路〕。
#   在此之前守这条性质的是「身份前置检查」那节的 2 条既有断言（瘦 PATH 只有 bash ⇒ 任何外部
#   进程都会以 `command not found` 暴露）。它们**无名、诊断指错方向**（红出来写的是「正常路径
#   变吵了」，下一个人不会想到是「热路径多了一条外部依赖」），**且射程不含无 jq 那条解析路** ——
#   审计 `MU-K2` 实测：把 `awk` 加进 `acct_row_from_slice`，92/92 全绿。
#   ⚠⚠ **无 jq 那条路有两个解析器，要各钉一条**〔审计 `B2` ② 逮到的洞〕：`daemon_out_to_table`
#     的 `else` 分支与 `manifest_to_table`+`acct_row_from_slice`。第二轮只在**最后一跑**之后量一次，
#     而 `NOJQ()` 每跑开头都 `: > "$KTMP/cnf"`/`err` ⇒ daemon 那条路的读数被下一跑冲掉了
#     （审计 `MU-K4`：把 `awk` 加进 `daemon_out_to_table` 的 `else`，**0 红**，而它真跑、真报
#     `awk: command not found`）。⇒ 下面**该量的那两跑各就地量一次**。
#   ⚠ **08-25 订正一句写宽了的话**〔审计 `S1`〕：这里原写「下面**每跑一次就地量一次**」，
#     而实数是 **4 跑 / 2 量**（量于 08-25 R7 重打：`grep -c '\$(NOJQ '` ⇒ **4**，
#     `grep -c '^ck .*\$(KNF)'` ⇒ **2**）。⚠ **量 `$(KNF)` 必须行首锚定到 `ck`**：
#     照抄 `grep -cF '$(KNF)'` 会读作 **3**，多出来的那一处**就是这句注释自己**
#     〔审计 `D4` 建议-3；同族的坑上面 `KC6d` 计数那处也踩过一次〕——
#     形状与它自己批评第二轮的那一处一模一样。**今天没有覆盖后果**（没被量的那两跑
#     ——「首块 z」与「pretty-print」——走的解析器与被量的「末块 f」那跑**是同一份**
#     `manifest_to_table`+`acct_row_from_slice`；两条解析路各一条读数已经把面盖全了）
#     ⇒ 订正的是**那句话**，不是补两条同义反复的判据。**要量的是「每条解析路一次」，不是「每跑一次」。**
#   ⚠ **射程不写在这里，这里只指路**〔08-25 删掉了原本贴在这儿的那份射程声明〕。
#     它描述的是**上一版**尺子，三句话里两句今天是假的：①「绝对路径 ENOENT（`$0: ` 前缀记）」——
#     那半（半 b）08-25 已经删了，机制不存在了；②「**不吃重定向**」—— 那正是本轮在治的**那句
#     可证伪的假话**本身（`/nonexistent-dir/xx 2>/dev/null` 旧尺子读 0，去掉重定向读 2）；
#     ③「往热路径加 `sed` 它逮不到」—— 今天**逮得到**（尺子自检⑦ 正在断言相反的事，
#     08-25 实测把 `sed` 加进 `acct_row_from_slice` ⇒ 2 红）。
#     ⇒ 整段删掉，**不在这里造第二份**：同一件事在这个文件里只该有**一处**权威住址 ——
#     `KNF()` 上方那段「★ 逮得到 / 逮不到 —— 逐形实测」（与 `shared/ccm` 里 `acct_row_from_slice`
#     的头注是同一份话）。★ 这一处能活到今天，正是因为它离判据**更近** —— 下一个人读的是近的那份。
ck "★ KCM6 · 无 jq + daemon 在位 ⇒ 仍然拿 daemon 那份" \
   "$(GOLD "$KTMP/from-daemon")" "$(NOJQ "$KTMP/bin/daemon" "$KTMP/accts/accounts.json")"
ck "★ KC6d/热路径 · 无 jq + **daemon 那条**解析路（daemon_out_to_table 的 else）上零外部依赖" "0" "$(KNF)"
# ★ **栈外那一面的账**〔08-25 新增〕。上一条判的是**射程内**（三个解析函数的栈内）零外部依赖；
#   这一条把**整趟** --print 起过的外部命令**名集合**钉住 —— 谁哪天给这条路加一条新的外部依赖，
#   这里当场多出一个名字 —— **射程同上：只对「逮得到」那 12 形成立**，逮不到的形这里同样不响
#   〔08-25 R7 订正，别在这里再造一句无分母的全体断言〕。今天这个集合说的是实话：
#   daemon 那一次往返（有意的）＋ sq 里两次 sed。★ 「**一次**」今天是真的：R7 修掉计数翻倍之前，
#   旁账实测这一跑里 daemon 记的是 **2** 次（同一次往返记两遍），修后是 **1** 次 —— 见 `KNF()` 头注。
#   ⚠ 它不是「零依赖」，**别把这一条读成那一条**：两条判的是两个面，名字里各自写明了。
ck "★ KC6d/整趟 · 无 jq + daemon 在位 ⇒ 整趟没有**声明清单之外**的外部命令（清单：sed · 那一次 daemon 往返）" "" \
   "$(KALLNEW 'sed|<KTMP>/bin/daemon')"
ck "KCM6 · 无 jq + 无 daemon ⇒ 文件那条兜底真的解析出来了（**首块** z）" \
   "$(GOLD "$KTMP/from-file")" "$(NOJQ - "$KTMP/accts/accounts.json")"
# ★★ **这一条为什么查 `f`（末块那个账号）—— 那份话的唯一权威住址在 `shared/ccm`**
#   （`manifest_to_table` 里 `|| [ -n "$chunk" ]` 那段头注）。**这里只指路，不在这里造第二份。**
#   ★ 原本贴在这儿的是那份话的**逐字副本**，而它三句全假（08-25 R7 逐句重切）：
#   「`MU-E1` 恰好 1 红且不是这一条」—— 实得 **2 红**，其中一条**正是下面这一条**；
#   「逮住它的是 pretty-print 那条」—— 是 ③④ **两条**；「改成查 `f`」—— `git diff f334ddc..fa4a469`
#   证否，实为**保留**旧那条（改名「首块 z」）＋**另加**一条查 `f` 的。R6 把 `shared/ccm`
#   那一份订正了、**漏了这一份**（谱系第 7 次）。
#   ★ **它能活到今天正是因为它离判据更近**（判据就在下一行）—— 下一个人读的是近的那份。
#   ⇒ 近的那份只许是**指路**：副本会被单独订正漏掉，指路不会。
ck "★ KCM6 · 无 jq + 无 daemon ⇒ **末块**那个账号 f 也解析得出来（末块守卫 \"|| [ -n \$chunk ]\" 的靶子）" \
   "$(GOLD "$KTMP/file-only")" "$(NOJQ - "$KTMP/accts/accounts.json" f)"
ck "★ KC6d/热路径 · 无 jq + **文件**那条解析路（manifest_to_table + acct_row_from_slice）上零外部依赖" "0" "$(KNF)"
ck "★ KC6d/整趟 · 无 jq + 无 daemon ⇒ 同上，且清单里**没有** daemon（这条路一次往返都不该起）" "" \
   "$(KALLNEW 'sed')"
# pretty-print + 键序反转：旧那条 grep 兜底在这一格是**静默失灵**的（它要求 name 排在 configDir 前）。
# ⚠ 这里原写 `cat > "…/pretty/accounts.json" 2>/dev/null || mkdir -p …` —— **重定向在
#   `2>/dev/null` 生效之前就求值了**，所以目录还不在时那句报错**照样打到 stderr**：基线跑
#   每次都有一行「…/accts/pretty/accounts.json: 没有那个文件或目录」。功能没坏（`||` 兜住了），
#   但它是**基线噪音**，而基线噪音会让下一个人把真信号读漏〔审计 `D4` 建议-4；非 R6 引入〕。
mkdir -p "$KTMP/accts/pretty"
cat > "$KTMP/accts/pretty/accounts.json" <<JSON
{
  "version": 1,
  "accounts": [
    { "configDir": "$KTMP/from-file",
      "isDefault": true,
      "name": "z" }
  ]
}
JSON
ck "★ KCM6 · 无 jq + pretty-print + **键序反转** ⇒ 照样解析对（旧兜底在这一格静默失灵）" \
   "$(GOLD "$KTMP/from-file")" "$(NOJQ - "$KTMP/accts/pretty/accounts.json")"

rm -rf "$KTMP"

echo
echo "===== JSONENC：shell 侧真 JSON 编码器（K-P2 D 阶段第一拍，09-03）====="
# ★★ **为什么这一节从 `shared/ccm` 里把那段真文本抽出来跑，而不是在这儿另写一份**
#   在这儿另写一份 = **两份实现互证**，那正是本仓最贵的那族病。
#   `shared/ccm` 的 `DAEMON_BIN_RECIPE` 头注逐字反对过同一形状：「照 `BUS_ID_RECIPE` 的先例
#   本该写两份『逐行同构、改一边必须改另一边』—— 这次更进一步：**只写一份字符串**」。
#   ⇒ 照 `e2e/ccm-rbind-title.sh` 那条既有先例（它也是 `sed … "$ROOT/shared/ccm"` 抽 `FMT`），
#   **抽生产文件里的那一段来 `eval`** ⇒ 测的就是将来真跑的那几行，不是它的复制品。
#
# ★★ **主判据是「与 `jq` 逐字节相等」，不是「塞进去能取出来」**
#   往返（encode → decode）**在两侧用同一套错规则时照样通过** —— 那是本仓治过多次的形状，
#   所以它在这里只当**旁证**（三条，名字里写明「弱判据」）。
#   主判据用 `jq -Rs .` 当预言机：**另一份实现、另一种语言、另一批作者**。
#   ⚠ 能这么写的前提是编码器**一条可选项都不留**（RFC 8259 允许 `/`、U+007F、非 ASCII
#     转或不转）—— 留了就得配一张「这几种输入允许不一样」的例外表，
#     而**例外表会替以后所有真 bug 挡枪**。理由逐字写在 `shared/ccm` 的 `json_str` 头注里。
#
# ★ **最后一条是尺子自检**：拿**被换掉的那个模板**（`printf '"%s"'`，即旧那处
#   `printf '{"sessionId":"%s"}'` 的编码部分）当被测对象跑同一族输入，它必须在**恰好 19 条**
#   上与 `jq` 分歧。那条一绿，上面那一族才叫「有牙」；它红了就是**输入集被人悄悄改软了**。
JSONLIB="$(sed -n '/BEGIN json-encoder/,/END json-encoder/p' "$CCM")"
JHAS() { case "$JSONLIB" in *"$1"*) printf yes ;; *) printf no ;; esac; }
ck "JSONENC/抽取器自检① · 抽到的那一段行数 ≥ 20（防「抽到一行也照样绿」）" "yes" \
   "$([ "$(printf '%s\n' "$JSONLIB" | wc -l)" -ge 20 ] && printf yes || printf no)"
ck "JSONENC/抽取器自检② · 里面真有 json_str 的函数定义（不是抽到一堆注释）" "yes" "$(JHAS 'json_str() {')"
ck "JSONENC/抽取器自检③ · 里面真有控制字符表 _CCM_JSON_CTRL" "yes" "$(JHAS '_CCM_JSON_CTRL=')"
eval "$JSONLIB"

# ── 会咬人的输入集。**一处定义，三族判据共用** ──────────────────────────────
# ⚠ 每一条都要说得出「它咬的是哪一种写法」，否则就是凑数：
#   双引号/反斜杠 = 最基本的两条必转；结尾反斜杠 = 「把 `"` 吃掉」那一类；
#   字面 `\n` = 「把两个字符当成一个」那一类；换行/制表/回车 = 有短名的控制字符；
#   退格/换页/垂直制表 = 短名与 `\u` 两条路的分界；`\001`/`\007`/`\016`/ESC/`\037`/DEL
#   = 没有短名的那一段（DEL 是 RFC 说「可选」而 jq 选了转的那一格）；
#   中文/emoji = 多字节透传；`{"a":1}` = 已经像 JSON 的串；单引号/正斜杠/`$`/反引号
#   = **不该被动的**（过度转义与转义不足是同一族病的两面）；真 payload = 生产形态；
#   全控制字符串 = 31 个一次打完，防「只对我举的例子成立」。
J_NAMES=(空串 双引号 反斜杠 结尾反斜杠 字面反斜杠n 换行 制表 回车 退格 换页 垂直制表 \
         SOH01 BEL07 SO0E ESC1B US1F DEL7F 中文 emoji 像JSON 单引号 正斜杠 美元反引号 \
         真payload 全控制字符)
J_INS=('' 'a"b' 'a\b' 'a\' 'a\nb' "$(printf 'a\nb')x" "$(printf 'a\tb')" "$(printf 'a\rb')" \
       "$(printf 'a\bb')" "$(printf 'a\fb')" "$(printf 'a\vb')" \
       "$(printf 'a\001b')" "$(printf 'a\007b')" "$(printf 'a\016b')" "$(printf 'a\033b')" \
       "$(printf 'a\037b')" "$(printf 'a\177b')" '中文测试' '🔥火' '{"a":1}' "a'b" 'a/b' 'a$b`c' \
       "export CLAUDE_CONFIG_DIR='/home/z/.claude-accts/z'; exec claude --model 'x\"y'" \
       "$(printf 'a\001\002\003\004\005\006\007\010\011\012\013\014\015\016\017\020\021\022\023\024\025\026\027\030\031\032\033\034\035\036\037b')")

JORA() { printf '%s' "$1" | jq -Rs .; }   # 独立预言机：jq 自己的编码器
_ji=0
while [ "$_ji" -lt "${#J_INS[@]}" ]; do
  ck "JSONENC/oracle · ${J_NAMES[$_ji]}（与 jq -Rs . 逐字节相等）" \
     "$(JORA "${J_INS[$_ji]}")" "$(json_str "${J_INS[$_ji]}")"
  _ji=$((_ji+1))
done

# ── 族二：**手算的**逐字期望。不经 jq —— 万一 jq 与我们同时错，这一族仍是独立的一票 ──
ck "JSONENC/手算 · 空串" '""' "$(json_str '')"
ck "JSONENC/手算 · 双引号" '"a\"b"' "$(json_str 'a"b')"
ck "JSONENC/手算 · 反斜杠" '"a\\b"' "$(json_str 'a\b')"
ck "JSONENC/手算 · 结尾反斜杠（不许把收尾那个 \" 吃掉）" '"a\\"' "$(json_str 'a\')"
ck "JSONENC/手算 · 字面 \\n 两个字符，不许被当成换行" '"a\\nb"' "$(json_str 'a\nb')"
ck "JSONENC/手算 · 真换行 ⇒ 短名 \\n" '"a\nb"' "$(json_str "$(printf 'a\nb')")"
ck "JSONENC/手算 · 制表 ⇒ 短名 \\t" '"a\tb"' "$(json_str "$(printf 'a\tb')")"
# ⚠ 期望串里那个反斜杠**单独用变量给** —— 源码里刻意不出现「反斜杠紧跟 u」那个写法：
#   它会被各路工具（编辑器 / 补丁工具 / 本文件自己的 `eval`）当成转义序列吃掉，
#   而吃掉之后期望值变成一个**真控制字符**，判据就从「要转义」悄悄变成「不要转义」。
#   09-03 落这一节时**当场踩了一次**，见件文件 `§16-3` 的 `E3`。
J_BS='\'
ck "JSONENC/手算 · 无短名的控制字符 ⇒ 反斜杠u0001（小写十六进制）" "\"a${J_BS}u0001b\"" "$(json_str "$(printf 'a\001b')")"
ck "JSONENC/手算 · U+007F 也转（RFC 说可选，本编码器不留可选项）" "\"a${J_BS}u007fb\"" "$(json_str "$(printf 'a\177b')")"
ck "JSONENC/手算 · 已经像 JSON 的串要被当成**数据**" '"{\"a\":1}"' "$(json_str '{"a":1}')"
ck "JSONENC/手算 · 中文原样透传（不转 \\uXXXX、不动字节）" '"中文测试"' "$(json_str '中文测试')"
ck "JSONENC/手算 · 单引号/正斜杠/\$/反引号**一个都不许动**（过度转义与转义不足同族）" \
   '"a'"'"'b a/b a$b`c"' "$(json_str "a'b a/b a\$b\`c")"

# ── 族三：往返。**弱判据，只当旁证** —— 两侧用同一套错规则时它照样绿 ──
ck "JSONENC/往返（弱判据·旁证）· 引号反斜杠" 'a"b\c' "$(json_str 'a"b\c' | jq -r .)"
ck "JSONENC/往返（弱判据·旁证）· 控制字符" "$(printf 'a\nb\tc\001d')" \
   "$(json_str "$(printf 'a\nb\tc\001d')" | jq -r .)"
ck "JSONENC/往返（弱判据·旁证）· 多字节" '中文🔥' "$(json_str '中文🔥' | jq -r .)"

# ── 尺子自检：旧那个模板必须在这一族输入上大量分歧 ──────────────────────────
# ⚠ **19 是现打的**（09-03，`jq-1.7`，上面那 25 条输入）：模板只在 6 条上碰巧对
#   （空串 · 中文 · emoji · 单引号 · 正斜杠 · `$`反引号 —— 全是「本来就不用转」的）。
#   ⇒ 这个数变小 = 有人把上面那个输入集改软了；变大 = 加了新的会咬人的输入（那要连这里一起改）。
J_TPL() { printf '"%s"' "$1"; }   # 旧那处 `printf '{"sessionId":"%s"}'` 的编码部分，逐字同形
_jd=0; _ji=0
while [ "$_ji" -lt "${#J_INS[@]}" ]; do
  [ "$(J_TPL "${J_INS[$_ji]}")" = "$(JORA "${J_INS[$_ji]}")" ] || _jd=$((_jd+1))
  _ji=$((_ji+1))
done
ck "JSONENC/尺子自检 · 被换掉的那个**模板**在这 ${#J_INS[@]} 条输入上与 jq 分歧的条数（一绿才说明上面那族有牙）" \
   "19" "$_jd"

# ── 生产接线：**上面测的那个函数，生产路径真的在用它** ────────────────────────
# ⚠ 没有这两条，上面 40 多条判据可以在「json_str 定义了但没人调」的情况下全绿 ——
#   那正是本工作区反复在治的「早写好、零调用方」。
#   ⇒ 走 `--print`（纯的、不起 tmux 不起 agent），看 `resolve_recipe` 吐出来的那段里
#   sid 是不是**编码过的**。期望串是**手写的**，不从 ccm 取（从 ccm 取就是同义反复）。
ck "JSONENC/接线 · --print 的 resume 配方里，含双引号的 sid 被真编码了（不是模板拼进去）" "yes" \
   "$(case "$(ccm resume 'a"b' --cwd /p --print)" in *'{"sessionId":"a\"b"}'*) printf yes ;; *) printf no ;; esac)"
# ⚠ 这一条**两层**：先 `json_str`（单引号在 JSON 里不用转）、再 `sq`（把 `'` 变成 `'\''`）。
#   期望串在这里用双引号拼出来，别在 case 模式里手叠三层引号 —— 那是写错了也看不出来的形状。
J_SQEXP="{\"sessionId\":\"a'\\''b\"}"
ck "JSONENC/接线 · 含单引号的 sid：json_str 不转它、sq 转它，两层各司其职" "yes" \
   "$(case "$(ccm resume "a'b" --cwd /p --print)" in *"$J_SQEXP"*) printf yes ;; *) printf no ;; esac)"
# ⚠ 这一条咬得最狠：**旧模板会把一个生 TAB 直接塞进 JSON 字符串里**（那是非法 JSON），
#   而 `--print` 那条路上没有任何别的东西会说话。
J_TABEXP="{\"sessionId\":\"a${J_BS}tb\"}"
ck "JSONENC/接线 · 含制表符的 sid 变成短名 \\t（旧模板会塞一个生 TAB 进 JSON）" "yes" \
   "$(case "$(ccm resume "$(printf 'a\tb')" --cwd /p --print)" in *"$J_TABEXP"*) printf yes ;; *) printf no ;; esac)"

echo
echo "===== WIRE：真发请求那一半（K-P2 D 阶段第二拍，09-03）====="
# ★★ **这一节买的那句话**：
#     「**ccm 真的把请求发给了 daemon，而且发出去的就是它该发的那个 JSON**」。
#
#   上面 `JSONENC/接线` 那三条只走 `--print`，看的是**配方文本** ——
#   文本里有那个 JSON，**不等于那些字节真的上了线**（`--print` 根本不发请求）。
#   这一节把**真 exec 路**（`shared/ccm` 的 `resolve_from_daemon`）跑起来，
#   看的是**落到 daemon stdin 上的那一份字节**。
#
# ★★ **判据分两族，而且它们必须分得开** ——
#   · 「**发了**」= 有个进程被调用了（`calls` 有一行、argv 是 `--resolve`）。
#     ⚠ 这一族**很弱**：随便发什么字节它都成立，连发个空串也成立。
#   · 「**发对了**」= 落盘的那份 stdin **逐字节**等于它该发的那个 JSON。
#   ⇒ 下面 `WIRE/分得开` 那四条是这两族的**成对自检**：在一份「只把落盘那份 stdin
#     改掉一个字节」的假 daemon 底下，「发了」两条**照样绿**、「发对了」**当场分歧**。
#     **少了那四条，「发了」那族看起来像在测接线，其实什么都没测。**
#
# ★ **用现成的桩，不发明第二套**：这份假 daemon 与本文件既有的 `mk_kd` / `mk_mirror_daemon`
#   同形（`cat > 落点` + `chmod +x` + 记账），只多做一件事：**把 stdin 原样落盘**。
#   ⇒ **不需要真 daemon、不需要真 tmux、不起真 agent**：daemon 回一帧
#     `{"command":"echo <标记>"}`，ccm 那句 `exec $_ccm_c` 就 exec 了 `echo`。
#
# ⚠ **不在 tmux 里跑是刻意的，也是这条路走得通的前提**：`shared/ccm` 里 `agent_has_identity`
#   那段在 `$TMUX` **未设**时只往 stderr 说一句、**不拦**（设了才 `die`）。
#   `env -i` 已把 `TMUX` 摘掉 ⇒ 本节**一条 tmux 命令都不起**。
#
# ⚠ **本节的射程，写出来别读大了**：它证的是「ccm 这一侧把那串字节交给了那个二进制」。
#   **daemon 那一侧收不收、`check_field` 拒不拒**（`control/launch.rs` 对含控制字符的字段
#   是拒收的）**本节一个字都没测** —— 红线禁起真 daemon。件文件 `§18` 逐条登记。
WTMP="$(mktemp -d)"
mkdir -p "$WTMP/bin" "$WTMP/spool" "$WTMP/cwd"

# 落盘式假 daemon：记一次调用 ＋ 记 argv ＋ **把 stdin 原样落盘**，然后回一帧。
# 第 4 个参数是「落盘前先过一道」的过滤器（默认 `cat` = 原样）——`WIRE/分得开` 那份用它改一个字节。
mk_wire_daemon() { # mk_wire_daemon <落点> <spool 目录> <回帧里的 command 串> [落盘过滤器]
  cat > "$1" <<EOF
#!/bin/sh
echo call >> "$2/calls"
printf '%s\n' "\$*" >> "$2/argv"
${4:-cat} > "$2/stdin.bin"
printf '{"command":"$3"}\n'
exit 0
EOF
  chmod +x "$1"
}
mk_wire_daemon "$WTMP/bin/daemon" "$WTMP/spool" 'echo WIRE_EXEC_OK'
# 首字节 `{` 换成 `[` —— **恰好一个字节**（下面用到的 sid 里一个 `{` 都没有）。
mk_wire_daemon "$WTMP/bin/daemon-corrupt" "$WTMP/spool" 'echo WIRE_EXEC_OK' "tr '{' '['"
# 本地兜底那条要 `exec claude` ⇒ **必须给个 shim**，否则拿不到 daemon 时会去摸这台机器上真的那个。
cat > "$WTMP/bin/claude" <<'WSHIM'
#!/bin/sh
printf 'WIRE_LOCAL %s\n' "$*"
WSHIM
chmod +x "$WTMP/bin/claude"

W() { # W <daemon 路径或 -> <sid>：**真跑 exec 路**（不是 --print）；stdout→out、stderr→err
  local d="$1" sid="$2"
  rm -rf "$WTMP/spool"; mkdir -p "$WTMP/spool"
  local -a envs=(HOME="$WTMP" CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent
                 CCM_ACCTS_MANIFEST=/nonexistent)
  [ "$d" != - ] && envs+=(CCM_DAEMON_BIN="$d")
  env -i PATH="$WTMP/bin:/usr/bin:/bin" "${envs[@]}" \
      bash "$CCM" resume "$sid" --cwd "$WTMP/cwd" > "$WTMP/out" 2> "$WTMP/err"
}
# ⚠ 文件不在时的分支**用 `-f` 先判，不靠 `2>/dev/null`**：`< 不存在的文件` 是**外层 shell**
#   在起 `wc` 之前就失败的，那句 `No such file` 由外层 shell 打，`wc` 自己的 `2>/dev/null`
#   **盖不住**（第一版就是这样，套件输出里混进一行看着像真错的噪声）。
WSTDIN() { [ -f "$WTMP/spool/stdin.bin" ] && cat "$WTMP/spool/stdin.bin"; }
WBYTES() { [ -f "$WTMP/spool/stdin.bin" ] || { printf '缺'; return 0; }; wc -c < "$WTMP/spool/stdin.bin"; }
WCALLS() { [ -f "$WTMP/spool/calls" ] || { printf '0'; return 0; }; wc -l < "$WTMP/spool/calls"; }
WARGV()  { [ -f "$WTMP/spool/argv" ] && cat "$WTMP/spool/argv"; }
WOUT()   { cat "$WTMP/out"; }
# 「它**该发**的那个 JSON」——**编码那一段拿 `jq -Rs .` 当预言机**（另一份实现、另一种语言、
# 另一批作者），外面那层 `{"sessionId":…}` 是协议形状（`remote-daemon-proto` 的
# `control/resolve_query.rs`：stdin 收 `ResumeSpec`，`sessionId` 必填）。
# ⚠ **刻意不从 ccm 里取期望** —— 从 ccm 取就是同义反复。
WEXP() { printf '{"sessionId":%s}' "$(JORA "$1")"; }

# ── 夹具自检：先证这把尺子会说话 ─────────────────────────────────────────────
W "$WTMP/bin/daemon" 'k-p2-d2'
ck "WIRE/夹具自检① · 落盘式假 daemon 在位 ⇒ 它真的被调用了（calls 恰好 1 行）" "1" "$(WCALLS)"
ck "WIRE/夹具自检② · 那份 stdin 真的落下来了（字节数 > 0，不是「文件都没有也照样绿」）" "yes" \
   "$([ "$(WBYTES)" != 缺 ] && [ "$(WBYTES)" -gt 0 ] && printf yes || printf no)"
W - 'k-p2-d2'
ck "WIRE/夹具自检③ · **没有 daemon 时 calls 是 0** ⇒ ①那条不是恒真（少了这条，①测不出任何东西）" \
   "0" "$(WCALLS)"
ck "WIRE/夹具自检④ · 没有 daemon ⇒ 走本地兜底那条 exec（诚实降级，不是静默什么都不干）" \
   "WIRE_LOCAL --resume k-p2-d2" "$(WOUT)"

# ── 族一：「**发了**」。⚠ 弱 —— 只要有个进程被调用就成立 ─────────────────────
W "$WTMP/bin/daemon" 'k-p2-d2'
ck "WIRE/发了① · daemon 被调用**恰好一次**（不是 0，也不是「每问一次一趟」）" "1" "$(WCALLS)"
ck "WIRE/发了② · 调用时的 argv 逐字是 --resolve（不是别的子命令、不是空）" "--resolve" "$(WARGV)"
ck "WIRE/发了③ · daemon 回的那条 command **真的被 exec 了** ⇒ 这是一次真往返，不是「调用了但没用回帧」" \
   "WIRE_EXEC_OK" "$(WOUT)"

# ── 族二：「**发对了**」。看落盘那份 stdin 的字节 ────────────────────────────
# ⚠ `$(cat …)` 会把**结尾换行吃掉** ⇒ 内容与字节数**分成两条**，
#   否则「多吐一个换行」这一格会静默通过（生产那行 `printf '{"sessionId":%s}'` 是**不带换行**的）。
ck "WIRE/发对了① · 纯 sid：落盘 stdin 逐字节 = 手写的那个 JSON" \
   '{"sessionId":"k-p2-d2"}' "$(WSTDIN)"
ck "WIRE/发对了② · 纯 sid：落盘 stdin **恰好 23 字节**（不多一个换行、不少一截）" "23" "$(WBYTES)"

# 会咬人的 sid 集。**一处定义，两条路（exec 路 / 配方路）共用**。
W_SNAMES=(纯sid 双引号 制表 反斜杠 换行 多字节 真payload)
W_SIDS=('k-p2-d2' 'a"b' "$(printf 'a\tb')" 'a\b' "$(printf 'a\nb')" '中文🔥' \
        "export CLAUDE_CONFIG_DIR='/home/z/.claude-accts/z'; exec claude --model 'x\"y'")
_wi=0
while [ "$_wi" -lt "${#W_SIDS[@]}" ]; do
  W "$WTMP/bin/daemon" "${W_SIDS[$_wi]}"
  ck "WIRE/发对了·预言机 · ${W_SNAMES[$_wi]}：落盘 stdin = {\"sessionId\":<jq -Rs . 的编码>}（逐字节）" \
     "$(WEXP "${W_SIDS[$_wi]}")" "$(WSTDIN)"
  _wi=$((_wi+1))
done

# ── 族三：**手算的**逐字期望（不经 jq —— 万一 jq 与我们同时错，这一族仍是独立的一票）──
# ⚠ 反斜杠**单独用 `J_BS` 变量给**：源码里刻意不出现「反斜杠紧跟 t」那个写法 ——
#   它会被各路工具当成转义序列吃掉，而吃掉之后期望值变成一个**真 TAB**，
#   判据就从「要转义」悄悄变成「不要转义」，**而它照样是绿的**。
#   09-03 `D1` 落 `JSONENC` 那节时当场踩过一次（件文件 `§16-3` 的 `E3`）。
W "$WTMP/bin/daemon" "$(printf 'a\tb')"
ck "WIRE/发对了·手算 · 含制表符的 sid ⇒ 线上是短名（旧模板会把一个**生 TAB** 塞进 JSON 串 = 非法 JSON）" \
   "{\"sessionId\":\"a${J_BS}tb\"}" "$(WSTDIN)"
W "$WTMP/bin/daemon" 'a"b'
ck 'WIRE/发对了·手算 · 含双引号的 sid ⇒ 线上是转义过的（旧模板会把一个值劈成两个）' \
   '{"sessionId":"a\"b"}' "$(WSTDIN)"
ck "WIRE/往返（弱判据·旁证）· 落盘那份 stdin 自己是合法 JSON，且 .sessionId 取回来 = 原 sid" \
   'a"b' "$(WSTDIN | jq -r .sessionId)"

# ── ★★ 尺子自检：把「发了」与「发对了」**拆开** ──────────────────────────────
# 一份**只把落盘那份 stdin 改掉一个字节**的假 daemon（首字节 `{` → `[`）。
# 它证的是：「发了」那两条在它底下**照样绿**（有个进程被调用了、argv 也对），
# 而「发对了」**当场分歧** ⇒ 两族判据真的分得开
#   「**有个进程被调用**」与「**发出去的就是它该发的那个 JSON**」。
# ⚠ 这四条是**成对自检**，不是凑数：拆掉它们，上面「发了」那族就退化成「有东西被跑过」。
W "$WTMP/bin/daemon-corrupt" 'k-p2-d2'
ck "WIRE/分得开① · 改了字节的 daemon 底下，「**发了**」照样绿：calls 仍是 1" "1" "$(WCALLS)"
ck "WIRE/分得开② · 改了字节的 daemon 底下，「**发了**」照样绿：argv 仍是 --resolve" "--resolve" "$(WARGV)"
ck "WIRE/分得开③ · 而「**发对了**」当场分歧（落盘 stdin ≠ 它该发的那个 JSON）" "differs" \
   "$([ "$(WSTDIN)" = "$(WEXP 'k-p2-d2')" ] && printf same || printf differs)"
printf '%s' "$(WEXP 'k-p2-d2')" > "$WTMP/expect.bin"
ck "WIRE/分得开④ · 分歧**只有一个字节** ⇒ 钉住「改一个字节就够红」，不是靠改一大片才红" "1" \
   "$(cmp -l "$WTMP/expect.bin" "$WTMP/spool/stdin.bin" 2>/dev/null | wc -l)"

# ── 族四：`--print` 吐的那行**真跑一遍**，与 exec 路上线的字节逐字节相同 ──────
# ⚠ 上面 `JSONENC/接线` 只看**配方文本**里有没有那个 JSON；这里把那行**真的执行掉**，
#   看它上线的字节。两条路各写一份实现（`resolve_from_daemon` / `resolve_recipe`）
#   ⇒ 它们分家过一次，`--print` 那半就白测了。
# ★ 这是 `e2e/ccm-contract-parity.sh` A′ 组（print↔exec 的 argv 差分）在**线上字节**这一维的同一条纪律。
WRECIPE() { # WRECIPE <sid>：取 --print 那行 → 真跑一遍（stdout→out）
  rm -rf "$WTMP/spool"; mkdir -p "$WTMP/spool"
  local line
  line="$(env -i PATH="$WTMP/bin:/usr/bin:/bin" HOME="$WTMP" CCM_SELF=/usr/local/bin/ccm \
          CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=/nonexistent \
          bash "$CCM" resume "$1" --cwd "$WTMP/cwd" --print 2>/dev/null)"
  env -i PATH="$WTMP/bin:/usr/bin:/bin" HOME="$WTMP" CCM_DAEMON_BIN="$WTMP/bin/daemon" \
      sh -c "$line" > "$WTMP/out" 2>/dev/null
}
_wi=0
while [ "$_wi" -lt 5 ]; do
  WRECIPE "${W_SIDS[$_wi]}"
  ck "WIRE/配方路 · ${W_SNAMES[$_wi]}：--print 那行真跑一遍，上线字节 = 该发的那个 JSON（逐字节）" \
     "$(WEXP "${W_SIDS[$_wi]}")" "$(WSTDIN)"
  _wi=$((_wi+1))
done
ck "WIRE/配方路 · 配方跑出来也真的 exec 了 daemon 回的那条 command（两条路同一个终点）" \
   "WIRE_EXEC_OK" "$(WOUT)"

rm -rf "$WTMP"

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
