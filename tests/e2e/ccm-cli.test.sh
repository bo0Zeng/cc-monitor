#!/bin/bash
# ccm CLI 的 shell 级测试（unify-launch F02）。
#
# 全部走 `--print` 断言命令串——不真起 agent、不碰 tmux（tmux 行为由
# tests/e2e/tmux-target-acceptance.sh 那套真机 harness 管）。
#
# 跑法：bash tests/e2e/ccm-cli.test.sh   （npm run test:ccm-cli）
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"

# ★★ `K-R48` 第二拍（09-11）：被测对象从 `shared/ccm`（bash）换成**后端二进制本体**。
#   〔用@09-11 `K33`〕逐字「后端**只有一个**，**不要有什么 bash 脚本**，**不要有什么单独的 ccm**」。
#   `intercept` 认的是 `argv[0]` 的 basename ⇒ 做一条叫 `ccm` 的软链指过去。
# 🔴 **fail-closed**：没 build 就响亮退出，不许静默回落到 PATH 上碰巧有的那一份
#   （那正是本文件全篇隔离纪律要治的那一族：测试结果不许随「是谁在跑测试」而漂移）。
CCM_NATIVE="${CARGO_TARGET_DIR:-$REPO/.build/backend}/debug/cc-monitor-remote"
[ -x "$CCM_NATIVE" ] || {
  echo "::error::找不到原生入口 $CCM_NATIVE —— 先 \`cd src/backend && cargo build --bin cc-monitor-remote\`" >&2
  exit 2; }
CCMDIR="$(mktemp -d)"; trap 'rm -rf "$CCMDIR"' EXIT
ln -s "$CCM_NATIVE" "$CCMDIR/ccm"
CCM="$CCMDIR/ccm"

# ⚠ 〔`K-R48` 第二拍 09-11〕**这里原来有两道 `jq` 的 fail-closed 硬依赖闸，本轮删了。**
#   它们守的是本文件那几份**假 daemon**（`mk_mirror_daemon` 用 `jq` 把夹具 manifest 翻成
#   `--list-accounts` 的帧形状、`KREC` 的记账 shim 转发给 `/usr/bin:/bin` 上那一个）——
#   而那几份假 daemon 与它们服务的判据本轮一起删了：同一个进程之下没有「帧」这回事，
#   账号表由后端自己读那份 manifest。**本文件今天一处都不用 `jq`**（`grep -c jq` 自己看）。
#   ⚠ **`npx` 那条纪律仍在**：下面会话名派生那一节靠 `npx tsx` 真跑前端那个函数做跨语言对拍。

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
# ★★ 〔`K-R48` 第二拍 09-11〕**`CCM_DAEMON_BIN` 那一栏没了，`CCM_ACCTS_MANIFEST` 那一栏留着。**
#   从前本套件要**自带一份后端**（`FAKED`）：账号解析没有本地退路，不给后端的话每一条判据
#   都会死在 `exit 4` 上。今天敲的那个命令**就是**后端 ⇒ 那个变量在原生实现里现打 `grep -rn`
#   **零命中**，留着它等于在夹具里摆一个谁也不读的旋钮。
#   ⚠ `CCM_ACCTS_MANIFEST` 仍是 `<目录>/accounts.json` 这个形态（不是裸 `/nonexistent`）——
#     那是**生产上那个值的形状**，与「后端是谁」无关。
ccm() { env -u CLAUDE_CONFIG_DIR CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=/nonexistent/accounts.json "$CCM" "$@" 2>&1; }

UNSET="unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION"

echo "===== 契约：动作 × 修饰 ====="
# ★★ 〔`K-R48` 第二拍 09-11〕**这个 helper 的期望文本整块换了 —— 换的是文本，不是判据的意图。**
#
# 从前 resume 的 exec 段是一段**配方**（`_ccm_c=""; _ccm_db=""; for _ccm_x in …`）：
# bash 那侧必须**推迟求值**，因为「这个会话该怎么起」的答案住在**另一个进程**里，
# 而 `--print` 不许去发请求。`shared/ccm::resolve_recipe` 的头注逐字写着这条。
#
# 🔴 **一个后端之后，那条配方没有指称对象了**：敲的那个命令**就是**后端 ——
# `resume` 那一问在**进程内**直接答（`control::resolve_query::resolve_json_for_inbound`），
# 于是 `--print` 吐的是**答案本身**：`set -f; exec <后端给的那条命令> <透传…>`。
# 那 34 条量「上线字节」的判据本轮判 `N`（verdicts 第 200–233 行）。
#
# ⚠ **`set -f` 不许省，它是这条串上唯一一处安全性质**：后端回的是一整条命令串，
#   要被 shell 拆成词才跑得了（`exec $cmd`，不是 `exec "$cmd"`），而拆词那一步会顺手做
#   路径名展开 ⇒ 命令里一个 `*` 会被 cwd 的文件名顶掉。〔第一拍差点丢掉这一格；
#   daemon 侧判据 `a_command_from_the_backend_is_never_rewritten_by_the_shell` 盯着它。〕
#
# ⚠ 下面**仍是手写一份期望文本**，**刻意不从 ccm 里取** —— 从 ccm 取就是同义反复，
#   实现怎么变期望就怎么变，这几条黄金串等于不存在。
# ⚠ 它仍是**逐字等值**断言，没有降成 `contains`：三条断言各自的意图
#（`--resume <sid>` 拼对了 / `--model` 被 export / resume 不做 auto 解析）在新串里逐字可见。
RECIPE() { # RECIPE <sid> <本地兜底的 exec 串>
  # `$1`（sid）今天不再进期望文本：从前它要拼进那段配方里的 `{"sessionId":"…"}`，
  # 而进程内直接答之后，sid 只出现在 `$2` 那条 exec 串里。**保留这个形参是有意的**：
  # 调用点逐字写着它测的是哪个 sid，去掉会让三条调用行读起来像在测同一件事。
  printf 'set -f; %s' "$2"
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

# ★★ audit-0805 F10 / 报告 I-11：**resume × 后端答案 × passthru 这一格此前零覆盖**。
#
# `ccm resume` 且未显式 --launcher 时，那条 exec 串**整条来自后端对 `resume` 的答案**，
# 而那个答案里**根本没有 passthru 这个概念**（`ResumeSpec` 六个字段无它）
# ⇒ 用户的 `-- --xxx` 只能由 ccm 这一侧**接在后面**。接漏了就是「答得上就丢参数、答不上就不丢」。
#
# ⚠ 〔`K-R48` 第二拍 09-11〕**尺子的字面跟着实现换了一次，性质一个字没变**：
#   从前那条串里有个 `exec $_ccm_c`（配方，执行时才求值）⇒ 尺子数的是 `exec $_ccm_c --flag-x`；
#   今天答案是进程内直接算出来的 ⇒ 同一件事的字面是 `set -f; exec claude --resume s1 --flag-x`。
#   **两栏仍然是「新旧两种写法各数一次」**：左栏（旧配方形）必须 0 —— 它今天是**反向对照**，
#   钉住「那条配方真的不在了」；右栏（真答案 + 透传）必须 1。
ck "resume 的后端答案上，-- 透传不许被整条换掉（I-11）" \
   "0 1" \
   "$(r="$(ccm resume s1 --cwd /p --print -- --flag-x)"; \
      printf '%s %s' \
        "$(printf '%s' "$r" | grep -c 'exec \$_ccm_c --flag-x')" \
        "$(printf '%s' "$r" | grep -c 'set -f; exec claude --resume s1 --flag-x')")"

# 反向：不带 `--` 时那条串必须**逐字与从前相同**（防「补透传」写成无条件加东西）。
ck "resume 不带 -- 时那条串里不许多出任何参数" \
   "0" \
   "$(ccm resume s1 --cwd /p --print | grep -c 'exec claude --resume s1 ')"
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
#   ② 〔原第②条：`CCM_DAEMON_BIN` 钉到一份假 daemon，免得查找次序摸到开发机上用户的真二进制〕
#      **`K-R48` 第二拍 09-11 作废** —— 没有查找次序了，敲的那个命令就是后端。
#      它治的那条病（同一条判据在开发机与 CI 上走两条不同的路）今天由文件顶上那道
#      `[ -x "$CCM_NATIVE" ]` fail-closed 顶着：被测对象**只可能**是本工作树刚 build 出来的那一份。
ACCTMP="$(mktemp -d)"; mkdir -p "$ACCTMP/z" "$ACCTMP/b" "$ACCTMP/bin"
cat > "$ACCTMP/accounts.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$ACCTMP/z", "isDefault": true },
  { "name": "b", "configDir": "$ACCTMP/b", "isDefault": false } ] }
JSON
# ⚠ 〔`K-R48` 第二拍 09-11〕**这里原来有一份镜像式假 daemon（`mk_mirror_daemon`），本轮删了。**
#   它的活是「把夹具 manifest 原样翻成 `--list-accounts` 帧形状」，好让 bash 那侧跨进程问到账号表。
#   今天后端**自己读**那份 manifest（`CCM_ACCTS_MANIFEST` 仍是唯一事实源），中间那一跳没有了。
#   下面那几个 helper 里留着的 `CCM_DAEMON_BIN=` 也一并去掉：原生实现现打 `grep -rn` **零命中**，
#   留着它等于在夹具里摆一个谁也不读的旋钮。
acct() { env -u CLAUDE_CONFIG_DIR CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/accounts.json" "$CCM" "$@" 2>&1; }
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
inherit_acct() { CLAUDE_CONFIG_DIR="$ACCTMP/b" env -u TMUX -u TMUX_PANE CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/accounts.json" "$CCM" "$@" 2>&1; }
ACCTMP="$(mktemp -d)"; mkdir -p "$ACCTMP/z" "$ACCTMP/b" "$ACCTMP/bin"
cat > "$ACCTMP/accounts.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$ACCTMP/z", "isDefault": true },
  { "name": "b", "configDir": "$ACCTMP/b", "isDefault": false } ] }
JSON
ck "外层已继承账号 b（无 --account/--base）→ 保留 b，不被默认号 z 静默覆盖"    "$UNSET; cd '/p' && exec claude"    "$(inherit_acct --cwd /p --print)"
ck "裸终端（无继承）仍落 manifest 默认号 z"    "export CLAUDE_CONFIG_DIR='$ACCTMP/z'; $UNSET; cd '/p' && exec claude"    "$(env -u CLAUDE_CONFIG_DIR CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/accounts.json" "$CCM" --cwd /p --print 2>&1)"
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
   "$(env -u CLAUDE_CONFIG_DIR -u TMUX -u TMUX_PANE CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/accounts.json" "$CCM" --tmux --cwd /p --print 2>&1 | unesc | grep -qF -- "'--account' 'z'" && echo yes || echo no)"
rm -rf "$ACCTMP"

echo
echo "===== 不给 --cwd = 站在哪儿起在哪儿（5 种布局，一格都不许跳）====="
TMPROOT="$(mktemp -d)"
trap 'rm -rf "$TMPROOT"' EXIT
export CC_WORKSPACE="$TMPROOT/workspace"; mkdir -p "$CC_WORKSPACE"
mkdir -p "$TMPROOT/plain" "$TMPROOT/repo/sub/deep"
( cd "$TMPROOT/repo" && git init -q . 2>/dev/null )
FAKEHOME="$TMPROOT/home"; mkdir -p "$FAKEHOME"

# 🔴 〔`K-R58` 09-11 · `KR58D3` · `K37` 第三条〕**这一组被翻过来了，不是被删掉。**
#
#   上一版这里逐字写着「`--cwd auto` 与旧 `_cc_resolve_target` 对拍」，`want` 那一半是
#   那段旧 bash 的逐字复刻：**在 $HOME 跳工作区 · 在 git 仓跳仓的父目录**。
#   〔用@09-11 逐字〕「`cc` 默认就起会话就行，**跳目录是我自己的设置，不要搞进 app**。」
#   ⇒ 那两档按 `K37`（「把这个行为去掉，用户还做不做得到同一件事」= 做得到 ⇒ 偏好，出去）
#   删了，于是**把旧语义钉死的正是这 5 条判据本身** —— 本件不删它们，改成钉新语义：
#   同样这 5 种布局，今天一律该停在原地。布局 1/2/3 就是那两档旧分支的现场，
#   它们从「证明会跳」变成「证明不跳」，**射程一格没少**。
#
# 🔴 **`CCM_WORKSPACE` 这里是故意还在导出的**：`KR58D3` 的失效方向逐字是
#   「把猜挪进别处（比如挪成一个默认开着的开关）」。设着它、站在 $HOME、答案仍必须是
#   $HOME —— 那一档要是哪天悄悄回来，这一条当场红。（后端今天**根本不读**这个变量了：
#   `Env` 里那个 `workspace` 字段跟着删了。）
#
# ⚠ `want` 取 `pwd -P`（物理路径）而不是 `$PWD`：`mktemp -d` 在有符号链接的 `/tmp` 上
#   两者不同值，而 Rust 的 `current_dir()` 给的是物理路径 —— 那会是一次跟本题无关的假红。
cmp_cwd() {
  local desc="$1" dir="$2" home="${3:-$HOME}" got want
  want="$( cd "$dir" && pwd -P )"
  got="$( cd "$dir" && HOME="$home" CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent \
      CCM_WORKSPACE="$CC_WORKSPACE" \
      CCM_ACCTS_MANIFEST=/nonexistent/accounts.json "$CCM" --print 2>&1 | sed -n "s/.*cd '\\([^']*\\)' && .*/\\1/p" )"
  ck "$desc" "$want" "$got"
}
cmp_cwd "布局1：在 \$HOME（设着 CCM_WORKSPACE）→ 仍是 \$HOME，不跳工作区" "$FAKEHOME" "$FAKEHOME"
cmp_cwd "布局2：git 仓根 → 仍是仓根，不跳仓的父目录"     "$TMPROOT/repo"
cmp_cwd "布局3：git 仓子目录 → 仍是那个子目录"          "$TMPROOT/repo/sub/deep"
cmp_cwd "布局4：非 git 目录 → 目录自己"                 "$TMPROOT/plain"
cmp_cwd "布局5：工作区自身（非 git）→ 自己"             "$CC_WORKSPACE"

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
name_of() { env -u TMUX CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=/nonexistent/accounts.json "$CCM" --tmux --cwd "$1" --print 2>&1 \
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


# ══════════════════════════════════════════════════════════════════════════════
# ⚠ 〔`K-R48` 第二拍 09-11〕**本文件原来有 264 条断言，本轮删到 46 条。**
#
# 〔用@09-11 `K33`〕逐字「后端**只有一个**，**不要有什么 bash 脚本**，**不要有什么单独的 ccm**」
# ⇒ `shared/ccm` 删了，本套件的被测对象换成后端二进制本体。
# 删掉的 218 条**逐条判词住 `evidence/K-R48-356-verdicts.tsv`**（第 21–284 行是本套件那 264 条），
# 按族：
#   · 身份 daemon 前置检查 15 条（第 67–81）—— 「在 tmux 里找不到 daemon ⇒ 响亮失败」那一族。
#     **同一个二进制之下「找不到 daemon」这个概念不存在了**：敲的那个命令就是后端。
#   · 账号解析走 daemon 一整节（第 84–152）—— 往返次数 / 帧形状 / 无 jq 纯 bash 解析路 /
#     「这段 bash 起了几个外部进程」的记账尺子。语义那一半已落成 daemon 侧 Rust 判据
#     （`the_account_table_has_exactly_one_source` · `picking_an_account_never_falls_back_to_a_different_one`
#      · `needs_account_table`）。
#   · `JSONENC` 47 条（第 153–199）—— 那个**手写的 bash JSON 编码器**与 `jq -Rs .` 的逐字节对拍。
#     Rust 侧是 `serde_json`，**不是我们的实现，不必我们来测**。
#   · `WIRE` 85 条（第 200–284）—— 「发了 / 发对了 / 不可达 / 撞名 / 缺省尺寸 / 控制字符」。
#     同一个进程之下**没有「上线字节」这回事**；其中「那几件事一件都不许丢」已落成 Rust 判据
#     `the_container_launch_goes_through_the_one_door_with_every_field_intact`。
#
# 🔴 **两族是本拍实测推翻第一拍判词的，写在这里而不是藏进 TSV**（第一拍判 `M-repoint`，
#    本拍指过去之后发现它们没有指称对象 ⇒ 降级为 `N`）：
#   ① 第 82–83（`--print` 不受身份前置检查影响 ＋ 它的非空对照）——
#      **非空对照那一条断的正是 `rc=2`**，而那道检查随第 67–81 那族一起没了。
#      留下第 82 条单条 = 一条恒真（那正是第 83 条当初被加进来要防的东西）。
#   ② 第 281–284（`WIRE/launch/--print` 的纯性 4 条）—— 第 281 条数的是「假 daemon 被调了几次」，
#      而原生实现根本不去调任何外部 daemon ⇒ **恒 0，空真**。
#      余下三条的性质仍在别处守着：`--print` 吐本机 tmux 编排由上面「账号继承」那 5 条
#      容器路黄金串逐字钉住；「stderr 一个字都没有」由本文件每一条黄金串的 `2>&1` 口径钉住
#      （`ccm()` 把 stderr 并进被比的串 —— 吵一个字就当场不等）。
# ══════════════════════════════════════════════════════════════════════════════

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
