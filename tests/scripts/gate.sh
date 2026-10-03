#!/usr/bin/env bash
# 出货前的**唯一闸门**：三道门跑一遍，末尾只吐一行裁决。
#
# # 它解决的是一个**过程**问题，不是技术问题
#
# `C7` 的门禁是「全量 npm test + 全量 cargo test，`0 passed` 不是绿」。三条命令分散着，
# 于是很容易写成 `cargo test … && npm test … && git commit -F -` 这种一条龙 ——
# **08-13 我就这么干了一次，`1140 passed; 1 failed` 的那行滚过去没人看，红着就出货了**
#（下一拍单独 commit 订正）。
#
# 病根不是「忘了看」，是**读数与动作被塞进同一条命令**：一条龙的输出很长，
# 而 `git commit` 的成功回显在最后一行，看起来像「都好了」。
#
# ⇒ 本脚本把三道门〔量于 08-13·当时就那三道门；今天跑几格看下面那段自述节，别拿这一句当读数〕
#   （+ 下面那道 `generated` 生成物漂移检查，K-A1 第四轮补的）收成一条命令，
# 并且**只在最后打一行裁决**（`GATE: OK` / `GATE: FAIL …`）。
# 用法就一句纪律：**先跑它、看见 `GATE: OK`，再单独敲 `git commit`。**
# ⚠ 它**故意不提交任何东西**、也不接 `--commit` 之类的开关 —— 那会把刚拆开的两件事又焊回去。
#
# 本脚本跑哪几格，以下面各行行首的 `run_gate` / `run_gate_sum` / `run_e2e` 与 `if gate_wants <名>` 为准 ——
# 这里不抄名单、不写格数（裁决行的格数与名单从这一趟真判过的格现算；收据 `.build/gate-receipt.json` 记着
# 真跑过哪几格、每格用了几毫秒）。CI 的 Linux job 就是调本脚本（`GATE_ONLY=<格>`），命令只住这里一处；
# 留在 Windows job 里的那几步（门禁跑不了真 Windows）由 `coverage` · `audit` 两格从 `ci.yml` 按步骤名现取原样跑。
#
# 真机 e2e 一律在本脚本里跑（`run_e2e`）；Windows 那一维由 `winchk` · `winchk-backend` · `winlink` 三格交叉编译盖，
# 真 Windows 上的行为本脚本判不了（射程表 `GATE_BLIND` 在裁决那一刻逐条印出来）。
# ── 起跑第一件事：**无条件**摘掉从开发机会话继承来的环境变量（按前缀，不按名单）─────────────────
# 门禁常在 Claude Code / cc-monitor 起的会话里、tmux 窗格里跑；这几族是**那个会话**的状态，不是这棵树的：
#   `CCM_*`（本机后端的监听口 · 中转口 · stderr 日志 · 凭据落点，都指真 `~/.cc-monitor`）·
#   `CLAUDE*`（会话、账号目录）· `ANTHROPIC_*`（上游与中转）· `TMUX*`（所在窗格 · socket 目录）· `CC_BUS_*`（总线身份）。
# 测试一继承，读数就跟着开发机走，更坏的是动到真东西：10-02 一趟门禁里 e2e 起的后端带着 `CCM_LISTEN_PORT`
#   去绑真端口、把真 `stderr.log` 轮转了两次。按前缀摘，新长一个同族变量也跟着摘，不靠调用方记得。
# 要这些变量的测试一律自己设（e2e 各套最前面还会再清一遍：`tests/e2e/sandbox-env.sh`）。
# ⚠ 代价：`CCM_PWSH` 这类「开发者显式打开一组手测」的开关也一起摘了 —— 那几条本来就不在门禁里跑。
# 只印名字，不印值（里面有令牌）。`env-sandbox` 那一格带着指向假「真目录」的这几族起一趟门禁，判零写入、零监听。
gate_scrubbed=()
for gate_v in $(compgen -e); do
  case "$gate_v" in
    CCM_*|CLAUDE*|ANTHROPIC_*|TMUX*|CC_BUS_*) gate_scrubbed+=("$gate_v"); unset "$gate_v" ;;
  esac
done
unset gate_v
set -uo pipefail

# 🔴 **仓根 = 本脚本的上两级**（`tests/scripts/gate.sh` ⇒ `../..`）。
# 〔2026-09-18 修〕原文是 `/..`，那是脚本还住顶层时的写法；09-17 重组把它搬进
# `tests/scripts/` 之后少了一级 ⇒ 它 `cd` 到的是 `<repo>/tests`，于是 `src/backend`、
# `tests/e2e/*.sh`、`node_modules/.bin` 全部解错 ⇒ **十几格一起红，而且红的形状是
# 「找不到文件 / 退出码 127」，看起来像代码坏了**。同族的 45 个脚本在 `78bcb195`
# 那一笔里修过了，**唯独漏了门禁自己** —— 一个「检查别人的东西」自己没被检查。
cd "$(dirname "$0")/../.." || exit 2
fails=()
printf '  ·    %-14s %s\n' "环境" "摘掉了 ${#gate_scrubbed[@]} 个从开发机会话继承来的变量：${gate_scrubbed[*]:-（一个都没有）}"


# ── 〔被谁调用〕`GATE_ONLY` 子集 ＋ 一张**跑过的收据**（`G4` 空洞③，09-20）──────────
#
# ## 问题：这道门此前**只有人手动跑**
#
# 现打（本拍复打，读数与那条一致）：
#   `grep -c 'bash tests/scripts/gate.sh' .github/workflows/ci.yml` ⇒ 落地前是 **0**；
#   `.git/hooks/` 下零个非 sample 钩子；`tests/hooks/` 下当时只有一份 `pre-commit`，
#   而那一份**默认是死的**（要人手 `git config core.hooksPath` 才活）。
# ⇒ **此前没有任何东西强制它在出货前跑过。** 而「跑了」与「没跑」在终端上**一模一样**
#   —— 两边都是**什么都没有**。这正是本仓反复治的那一形，长在门禁自己身上。
#
# ## 🔴 反空真：「装了钩子」**不等于**「它跑过」
#
# 一个 hook 文件躺在盘上、一个 job 写在 `ci.yml` 里，这两件事**一个字都没说那一趟真跑了**。
# 同一族的另一半本仓已经现打过读数（`tests/scripts/hooks-are-runnable.sh` 头注，git 2.43.0）：
# 没有可执行位时 git **忽略这个 hook 并照常提交**，`rc=0`，只留一句**可以关掉**的 advice hint。
# ⇒ 光看退出码、光看盘上有没有文件，**分不开「跑了」与「跳过了」**。
#
# ★ 取法：**这一趟自己开一张收据**（`GATE_RECEIPT`，默认 `.build/gate-receipt.json`，
#   那个目录 `.gitignore` 已经整棵忽略 ⇒ 不脏工作树、也不用动写区外的 `.gitignore`）。
#   收据里记的都是**只有真跑过这一趟才拿得到**的东西：
#     ① 这份 `gate.sh` 的 sha256 —— 换了版本开的收据不算这一份门禁开的；
#     ② 这棵树的 tree oid ＋ 工作树脏不脏 —— 别的树上那一趟不算这一趟；
#     ③ 这一趟**真的判过**哪几格（`GATE_RAN`，**现算**，不是抄的清单）；
#     ④ 被 `GATE_ONLY` 挡掉哪几格（`GATE_SKIPPED`，**现算**）；
#     ⑤ 每格的墙钟（`ms`，毫秒）、e2e 前置那一趟编译（`e2e_prep_ms`）与整趟（`total_ms`）—— 「门禁更快」拿它当基线。
#   判它的判据本体是 `tests/evidence/K-G4C-gate-receipt.py` —— 它**刻意不在本脚本里跑**：
#   本脚本跑得了它就说明本脚本跑了，那是同源恒真。它是给**调用方**（`tests/hooks/pre-push`）
#   在门禁**之后**跑的那一条：门禁跳过了 ⇒ 没有收据 / 收据陈了 ⇒ 当场红。CI 的 job 直接判门禁的退出码。
#   🔴 **那条判据的反空真锚是「`ran ∪ skipped` 与本文件现打的格名两向集合相等」** ——
#     收据写空了、或本文件被读成空串，两个集合当场分叉。单向包含在表被清空时恒真。
#
# ## `GATE_ONLY`：**子集是明着少跑，不是静默少跑**
#
# 给 `GATE_ONLY` 一串空格分隔的格名 ⇒ 只跑那几格（e2e 那四格用套件短名）。
# 四条纪律焊在下面的代码里，一条都不是装饰：
#   ① `GATE_ONLY` 里有一个名字不是盘上真有的格 ⇒ **红**。拼错**不许**静默降级成「少跑一格」；
#   ② 跳过的格**逐字印出来**，不许只印跑了的那几格；
#   ③ 只要跳过了一格，裁决行就**不许**是 `GATE: OK`，换成 `GATE: PARTIAL` ——
#      🔴 **`GATE: OK` 这四个字的意思只有一个：盘上每一格都跑过了。** 不许有第二种意思；
#   ④ 自检探针（`自检①`–`自检⑩`）**不受 `GATE_ONLY` 影响**：它们是量具的量具，
#      被过滤掉就等于把门禁自己的自检关了，而那一形在输出上看不出来。
#      ⇒ `gate_selftest` / `gate_selftest_e2e` 里各有一句 `local GATE_PROBE=1`（动态作用域）。
#   ⚠ 探针一律跑在 `$( )` 里（子 shell）⇒ 它们**进不了** `GATE_RAN`/`GATE_SKIPPED` 那两张表，
#     那不是巧合，是 `found_cells()` 只认行首调用、刻意不把探针数成格的同一条边界。
GATE_ONLY="${GATE_ONLY:-}"
GATE_RECEIPT="${GATE_RECEIPT:-.build/gate-receipt.json}"
GATE_DECLARED=()   # 盘上声明过的格（短名，现算）
GATE_GROUPS=()     # 盘上声明过的组名（`GATE_ONLY` 里写组名 = 点名组里每一格；今天只有 `e2e`）
GATE_RAN=()        # 这一趟**命令真的执行过并被判过**的格（规范名，现算）
GATE_SKIPPED=()    # 这一趟被 GATE_ONLY 挡掉的格（短名，现算）
declare -A GATE_MS=()   # 这一趟每格的墙钟（毫秒，规范名 → 数），进收据
GATE_PREP_MS=0          # e2e 前置那一趟 cargo build 的墙钟（不是格，单记）
GATE_T0=""              # 整趟起点，下面定义完 `gate_now_ms` 就取

# 现在的毫秒数（`EPOCHREALTIME` 是微秒精度；去掉小数点那一位，与 locale 的小数点写法无关）。
gate_now_ms() { local t="${EPOCHREALTIME//[!0-9]/}"; printf '%s' "$(( t / 1000 ))"; }
gate_fmt_ms() { printf '%d.%d 秒' "$(( $1 / 1000 ))" "$(( $1 % 1000 / 100 ))"; }
GATE_T0="$(gate_now_ms)"
# 一格判完：记进 `GATE_RAN`，墙钟记进 `GATE_MS`。`$2` 是这一格起跑那一刻的 `gate_now_ms`。
gate_ran() { GATE_RAN+=("$1"); GATE_MS["$1"]=$(( $(gate_now_ms) - $2 )); }
gate_took() { gate_fmt_ms "${GATE_MS[$1]:-0}"; }

# 这一格这一趟要不要跑。返回 0 = 跑。`$2`（可选）是它所在的组名。
# ⚠ 副作用：把短名记进 `GATE_DECLARED`、组名记进 `GATE_GROUPS`。
gate_wants() {
  if [ "${GATE_PROBE:-0}" = 1 ]; then return 0; fi
  GATE_DECLARED+=("$1")
  if [ -n "${2:-}" ]; then GATE_GROUPS+=("$2"); fi
  if [ -z "$GATE_ONLY" ]; then return 0; fi
  case " $GATE_ONLY " in
    *" $1 "*) return 0 ;;
  esac
  if [ -n "${2:-}" ]; then
    case " $GATE_ONLY " in *" $2 "*) return 0 ;; esac
  fi
  GATE_SKIPPED+=("$1")
  printf '  skip %-14s %s\n' "$1" "GATE_ONLY 没点它 ⇒ 这一格这一趟**没判**（收据里如实记着，裁决行因此不许是 GATE: OK）"
  return 1
}

# `GATE_ONLY` 里每一个名字都得是盘上真有的格 —— 拼错是**红**，不是「少跑一格」。
gate_check_only() {
  if [ -z "$GATE_ONLY" ]; then return 0; fi
  local tok d known
  for tok in $GATE_ONLY; do
    known=0
    for d in ${GATE_DECLARED[@]+"${GATE_DECLARED[@]}"} ${GATE_GROUPS[@]+"${GATE_GROUPS[@]}"}; do
      if [ "$tok" = "$d" ]; then known=1; break; fi
    done
    if [ "$known" -ne 1 ]; then
      fails+=("GATE_ONLY 里的 \`$tok\` 不是盘上任何一格的名字 —— \
拼错一个字就等于**静默少跑一格**，而少跑与跑过在终端上一模一样 ⇒ 一律按红记。\
盘上现打这几格（短名）：${GATE_DECLARED[*]}")
    fi
  done
}

# 一串字符串印成 JSON 数组。⚠ 格名里没有引号与反斜杠（`found_cells()` 那三条正则决定了这件事），
#   所以这里不做转义 —— 哪天格名里真出现引号，是那一边该改，不是这里该补一层猜。
gate_json_arr() {
  local i first=1
  printf '['
  for i in "$@"; do
    if [ "$first" -eq 1 ]; then first=0; else printf ', '; fi
    printf '"%s"' "$i"
  done
  printf ']'
}

# 落一张收据。**两条路都要落**（`OK`/`PARTIAL` 与 `FAIL`）——
# 只在绿那一支落，就买不到「红过一趟、然后有人把红的那一格删了」那一形。
gate_write_receipt() {
  local verdict="$1" sha tree head dirty
  sha="$(sha256sum "$0" 2>/dev/null | cut -d' ' -f1)"
  sha="${sha:-<数不出 sha256>}"
  tree="$(git rev-parse 'HEAD^{tree}' 2>/dev/null)"
  tree="${tree:-<不在 git 仓里>}"
  head="$(git rev-parse HEAD 2>/dev/null)"
  head="${head:-<不在 git 仓里>}"
  if git diff --quiet HEAD 2>/dev/null; then dirty=false; else dirty=true; fi
  mkdir -p "$(dirname "$GATE_RECEIPT")" 2>/dev/null
  {
    printf '{\n'
    printf '  "verdict": "%s",\n' "$verdict"
    printf '  "gate_sha256": "%s",\n' "$sha"
    printf '  "tree": "%s",\n' "$tree"
    printf '  "head": "%s",\n' "$head"
    printf '  "dirty": %s,\n' "$dirty"
    printf '  "when": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '  "gate_only": "%s",\n' "$GATE_ONLY"
    printf '  "ran": '    ; gate_json_arr ${GATE_RAN[@]+"${GATE_RAN[@]}"}        ; printf ',\n'
    printf '  "skipped": '; gate_json_arr ${GATE_SKIPPED[@]+"${GATE_SKIPPED[@]}"}; printf ',\n'
    printf '  "ms": {'
    local i first=1
    for i in ${GATE_RAN[@]+"${GATE_RAN[@]}"}; do
      if [ "$first" -eq 1 ]; then first=0; else printf ', '; fi
      printf '"%s": %s' "$i" "${GATE_MS[$i]:-0}"
    done
    printf '},\n'
    printf '  "e2e_prep_ms": %s,\n' "$GATE_PREP_MS"
    printf '  "total_ms": %s\n' "$(( $(gate_now_ms) - GATE_T0 ))"
    printf '}\n'
  } > "$GATE_RECEIPT" || {
    printf 'GATE: 收据落不了盘（%s 写不进去）—— 后面那条 K-G4C-gate-receipt.py 会因此红，那是对的\n' "$GATE_RECEIPT"
    return 0
  }
  printf 'GATE: 收据 —— %s（verdict=%s · 真判过 %s 格 · 跳过 %s 格 · tree %s · dirty %s）\n' \
    "$GATE_RECEIPT" "$verdict" "${#GATE_RAN[@]}" "${#GATE_SKIPPED[@]}" "${tree:0:12}" "$dirty"
}

# ── 失败诊断（`K-R22` 09-04）：**红的那一格必须自带「为什么红」** ────────────────
#
# 病灶逐字：本文件把子命令的输出吃进变量，而**失败支只记退出码、把输出整个丢掉**
# ⇒ 终端上只剩一行 `GATE: FAIL —— cargo（退出码 101）`，哪条测试红的一个字都没有。
#
# ★★ **它比「少印一段日志」严重，理由是那个退出码装着好几件互不相关的事。**
#   `cargo` 的 `101` 在本仓至少是四件：① 真有测试红 ② **编译错误（测试一条都没跑）**
#   ③ **被内核杀掉（OOM）** ④ 环境不满足（`--network none` 下 blackhole 判据秒失败）。
#   四件事的处置完全不同，而**唯一能把它们分开的就是被丢掉的那段字**。
#   09-04 一天里 PM 与三路 agent **各自独立**撞上，四次都得把那一格重跑一遍才知道是哪一件。
#   ⇒ 这正是本区最贵那族病（**一个值装了几件事**）长在门禁自己身上。
#
# ★ **人群不是一个 `if`，是 6 处**（`K-R22 D1` 量于 `5e1d07b`，主尺 `grep -n 'fails+=('`
#   得 11 处判红支，真跑了命令的 10 处里**全丢 6 处**：`run_gate` 两支 · `run_gate_sum`
#   三支 · `run_e2e` 的「抓不到数」支）⇒ **修必须落在共用的这一段上**，落在某一个 `if`
#   里面只治得了六分之一。
#
# ★ 取法：**两段 + 一条兜底**，两侧都要避（印太少还得重跑；印太多把裁决那行埋掉）。
#   ① **关键行**：整段输出里匹配下面那张模式表的行（`-A1` 带一行下文），封顶 `GATE_DIAG_KEY` 行。
#      模式表按**形状**选，不按工具选 —— 每一条都点名它治的是哪一形：
#        `^error`              cargo 的 `error[E0xxx]:` · `error: could not compile` ·
#                              `error: test failed`；**被信号杀掉那一形也走它**
#                              （`error: could not compile … (signal: 9, SIGKILL: kill)`）
#        `^thread .+ panicked` panic 落点（自带 `文件:行:列`）；`-A1` 把断言原文带出来
#        `^failures:`          cargo 失败清单的头
#        `^test result: FAILED` 那一格的合计（几过几败）
#        `^ *Running`          正在跑哪个测试二进制 —— **被杀那一形唯一能说明「死在哪个包」的行**
#        `^npm error` `^npm ERR!` npm 两代前缀
#        `^::error::`          `tests/e2e/assert-pass-floor.sh` 自己的诊断
#        `^ *(FAIL|BROKEN|×|✗)` 本仓 bash e2e 与 `pb check` 自己的失败行
#        `^Diff in `           `cargo fmt --check` 指哪个文件哪一行不合排版
#                              〔09-10 补：fmt 那道门第一次被刀切红时，模式表**一条都没匹配上**
#                              （终端逐字「fmt 关键行：一条都没匹配上（共 72 行）」）——
#                              它自己那条兜底（原文尾部恒印）救了场，但「红的那一格必须
#                              自带为什么红」这句话当时只兑现了一半。⇒ 补这一形。
#                              ★ 它同时是 ① 这张表「天生会漏」那句话的**又一个实例**：
#                              加一道新门就可能带来一种新形状，**加门那一拍要顺手切一刀看诊断印不印得出来**〕
#   ② **原文尾部** `GATE_DIAG_TAIL` 行：**恒印**。
#   ③ 输出短于 `GATE_DIAG_WHOLE` 行 ⇒ 不摘要，**全印**（那一形上摘要的收益是负的）。
#
# ★★ **② 是 fail-closed 的那一半，别当冗余删掉。**
#   ① 是一张**模式表**，而模式表天生会漏（换了工具 / 换了措辞 / 本地化）。
#   ⚠ 直答 `K-R22 D2` 那问：**编译错误那一形没有 `failures:` 段** ——
#     它落在 `^error` 与 `-A1` 带出来的 `--> 文件:行:列` 上（死值验现打 5 行，件文件 `§5`）；
#     而**假如哪天它连 `^error` 都不匹配**，② 仍把最后几十行原样端上来
#     ⇒ **「印出零个字」在任何形状上都不可能**。这条比模式表准不准重要得多。
#
# ⚠ 每行加 `  | ` 前缀是**承重的，不是排版**：被测命令的输出里要是自己打了一行
#   `GATE: OK` 或 `  ok   xxx`，不带前缀就会**混进本脚本自己的裁决面**。
#
# ⚠ **它一个字都没碰任何一条判定**（硬边界：那五道门的口径不许动）——
#   `fails+=` 的条件、包数自检、`0 passed 不是绿`，逐字原样。本段只加「印什么」。
GATE_DIAG_KEY="${GATE_DIAG_KEY:-40}"
GATE_DIAG_TAIL="${GATE_DIAG_TAIL:-30}"
GATE_DIAG_WHOLE="${GATE_DIAG_WHOLE:-60}"
GATE_DIAG_PAT='^(error|npm error|npm ERR!|thread .+ panicked|failures:|test result: FAILED|::error::|Diff in )|^[[:space:]]*(FAIL|BROKEN|Running|×|✗)'

# ── `N-G1`（09-05）：**①段匹配之前先去一次色。同样一个字都没碰任何一条判定。** ────
#
# ★★ **病因是现打的，不是猜的。** 上一版这里的解释停在「模式表漏了这一形」，而
#   `N-F1b` / `N-F2` 两件的实现方连着两次在交回里写「关键行**一条都没匹配上**」
#   ⇒ 每一件的死值验都只剩「红了几条」，**是哪几条只能靠推断**。
#
#   `NG1D1` 的量法（量具住 `tests/evidence/N-G1-diag-match.py`，模式表从本文件现读，不复述）：
#   在沙箱里故意让一条前端判据红，把 `out="$(npm test 2>&1)"` 那一份**原始** stdout+stderr
#   拿去 `od -c` —— ⚠ **不是**门禁已经加过 `  | ` 前缀的日志（PM 就在那上面栽过一次）。
#
#   现打的读数：`vitest 4.1.10` 在**非 TTY**（命令替换 · `TERM=dumb` · `FORCE_COLOR`/
#   `NO_COLOR`/`CI` 三个都没设）下**照样上色**，2413 行输出里 **1584 行含 `ESC`**。
#   那行 ` FAIL ` 的头 15 个字节 `od -c` 逐字：
#     033  [  4  1  m  033  [  1  m  空格  F  A  I  L  空格
#   ⇒ **行首不是空白，是 `ESC[41m``ESC[1m`**（徽章的红底＋粗体）。
#   而模式表那一支是 `^[[:space:]]*(FAIL|…)`，`ESC`(0x1b) **不属于** `[[:space:]]`
#   ⇒ 锚点 `^` 之后第一个字节就不匹配。**那句「一条都没匹配上」是被这两个 ESC 挡出来的。**
#   同一份输出里 `   × <用例名>` 那行同样被 `ESC[31m` 挡着。
#
# ★ **每一格各量了一次**（分母：**7 份原始输出**，盖住走 `gate_diag` 的 **5 个门禁格**
#   —— `cargo` · `backend` · `npm` · `e2e` · `pb check`；全是现打，不是抽样。
#   ⚠ 第九格 `generated` 不进这个分母：它红了走的是 `git diff --stat`，根本不调 `gate_diag`）：
#     npm·vitest      2413 行 · 含 ESC **1584** 行 · 改前命中 **0** · 去色后 **2**  ← 只有它中招
#     npm·tsx（`✗`）   186 行 · 含 ESC 0 · 改前 **1** · 去色后 1
#     cargo·workspace 1740 行 · 含 ESC 0 · 改前 **11** · 去色后 11
#     cargo·backend     642 行 · 含 ESC 0 · 改前 **6**  · 去色后 6
#     cargo·编译错误    15 行 · 含 ESC 0 · 改前 **2**  · 去色后 2
#     e2e·地板不符      28 行 · 含 ESC 0 · 改前 **1**  · 去色后 1
#     pb check         40 行 · 含 ESC 0 · 改前 **3**  · 去色后 3
#   ⇒ **cargo / e2e / pb check 三形本来就是好的**（cargo 与本仓 bash e2e 在非 TTY 下不上色）；
#     病**只在 npm 的 vitest 那一形**上。
#   ⚠ **没量的，写出来**：Windows 上的任何一形 · CI runner 上的任何一形 ·
#     显式 `FORCE_COLOR=1` 的跑法 · npm 那 16 个 tsx 套件里除 `format` 外的 15 个。
#
# ★ 取法：**只在①段之前去一次色**，`sed` 只吃 CSI（`ESC [ … 一个字母`）。
#   ⚠ **② 原文尾部与 ③ 短输出全印，一个字节都没动。** 那两段的性质就是「**原样**端上来」，
#     给它们去色等于把「原样」降级成「差不多」；`NG1D4` 反过来钉住②。
#   ⚠ **不顺手扩模式表去治别的工具**（件计划 `§2` 逐字）：量出来的病是**色码**，不是表少了词。
#     扩表治不了下一个上色的工具，去色治得了；而表**天生会漏** —— 那正是②存在的理由。
#   ⚠ 去色的射程只到 CSI，**OSC（`ESC ] … BEL`）不在里面** —— 现打：7 份里 `ESC ]` **0 处**，
#     而且去色之后 7 份**残留 `ESC` 行全 0** ⇒ 这一形上这条 `sed` 是够的（换个工具就未必）。
#   ⚠ 去色只喂给 `grep`，也**只影响①段印出来的那几行**（印出来的是去色版，终端上更好读）；
#     `$out` 本身一个字节没改，②仍拿它原样端。
#
# ⚠ **它一个字都没碰任何一条判定**（硬边界，与上面那段头注同一条）——
#   `fails+=` 的条件、包数自检、`0 passed 不是绿`，逐字原样。本段仍然只改「印什么」。
#   ★ **这句话有读数，不是自称**：`run_gate` / `run_gate_sum` / `run_e2e` 三块**整块 md5**
#     在 `333fcde` 与本拍之间**逐字节相同**（`8d0ac685b949` / `a2da94965516` / `48f1c09cf1b0`），
#     量具住 `tests/evidence/N-G1-gate-fn-md5.py`（可复跑）。
#   ⚠ **一处别读窄**：本文件全文 `fails+=(` 从 **15** 涨到 **16**，涨的那一处**在 `gate_selftest` 里**
#     （下面新加的自检④）—— 与 `K-R22` 立探针①②③ 时同一个说法：**新加的一格自检，
#     不是改了哪一道旧门**。「逐字原样」说的是那五道门的判红条件，不是「全文一处没加」。
gate_decolor() { sed $'s/\033\\[[0-9;:?]*[a-zA-Z]//g'; }

gate_diag() {
  local name="$1"; local out="$2"
  local total key
  # 「输出为空」本身就是一条读数，不许静默 —— **被信号杀掉那一形长这样**。
  if [ -z "$out" ]; then
    printf '  ---- %s 诊断：被测命令 stdout+stderr **一个字都没有**（退出码非零而输出为空——多半是被信号杀掉，如 OOM）\n' "$name"
    return 0
  fi
  total="$(printf '%s\n' "$out" | wc -l)"
  if [ "$total" -le "$GATE_DIAG_WHOLE" ]; then
    printf '  ---- %s 失败原文（全 %s 行）----\n' "$name" "$total"
    printf '%s\n' "$out" | sed 's/^/  | /'
    printf '  ---- %s 诊断完 ----\n' "$name"
    return 0
  fi
  # N-G1：`gate_decolor` 只加在**①这一条管道**上（②在下面，拿的仍是 `$out` 原样）。
  key="$(printf '%s\n' "$out" | gate_decolor | grep -E -A1 "$GATE_DIAG_PAT" | grep -v '^--$' | head -n "$GATE_DIAG_KEY")"
  if [ -n "$key" ]; then
    printf '  ---- %s 关键行（%s 行输出里匹配到的，封顶 %s 行）----\n' "$name" "$total" "$GATE_DIAG_KEY"
    printf '%s\n' "$key" | sed 's/^/  | /'
  else
    printf '  ---- %s 关键行：一条都没匹配上（共 %s 行）—— 模式表漏了这一形，只看下面的尾部\n' "$name" "$total"
  fi
  printf '  ---- %s 原文尾部 %s 行（共 %s 行）----\n' "$name" "$GATE_DIAG_TAIL" "$total"
  printf '%s\n' "$out" | tail -n "$GATE_DIAG_TAIL" | sed 's/^/  | /'
  printf '  ---- %s 诊断完 ----\n' "$name"
}

# ★★ `K-G3`（09-01）第二个参数 `denom` 是**这个数的分母**，跟着绿行一起印出来。
#
# 它治的是**第 5 个洞**：`sort -rn | head -1` 取的是**所有 `N passed` 里的最大值**，
# 而 `npm test` 是 **17** 个套件（1 个 vitest `test:dom` + **16** 个 `tsx`）用 `&&` 串起来的。
#
# ★ **分母现打（`K-G3` 09-01，跑了一趟真 `npm test` 数命中行，不是抽样）**：
#   整趟输出里命中 `([0-9]+) (passed|个测试)` 的**只有 3 行** ——
#   `test:diff` 的 `17 passed, 0 failed`（`tests/frontend/ui/cards/diff.test.ts:234`）·
#   vitest 的 `117 passed`（Test Files）与 `1480 passed`（Tests）。
#   ⇒ **16 个 tsx 套件里有 15 个不带数字**（`all X tests passed` 那一形），**第 16 个（`diff`）带**，
#   但它的 17 被 `sort -rn` 吃掉 ⇒ **`n` 仍恒等于 `test:dom` 那一个数**。
#   ⚠ 件文件 `§0b-1` 逐字写的是「**16** 个 tsx 套件……**没有数字**」—— 那句是 **15/16**，
#   已在 `§4a` 订正；**结论不受影响**（多出来的那个数比它小，`max` 照样吃掉）。
#
# ⚠ 洞的准确形状（别读大）：那 16 套的**失败**逮得到 —— `&&` 链里任一非零退出码
#   都会走上面 `rc != 0` 那一支。逮不到的是「某套**跑了 0 个测试**却照样 exit 0」
#   （文件改名 / `describe` 被注释 / glob 没匹配上）：它照打那句 `all X tests passed`、
#   照退 0 ⇒ `n` 仍是 `test:dom` 的数 ⇒ 全绿。**`C7` 那条「0 passed 不是绿」，
#   在 16/17 的面上是空的。**
#
# ⚠ **本参数不是判据，是分母** —— 它一个字都没改上面那两条自检（禁止）。
#   买的只有一件事：**那行绿不再自称它不是的东西**。PM 08-29 逐字承认过被它骗：
#   「我这一整窗汇报里写的每一个 `npm 1512 passed`，读法都错了 —— 那不是
#   『npm 门跑了 1512 个测试』，是『`test:dom` 这一个套件 1512 个』。」
#   ⇒ 与 `K-R10` 给 `pb check` 那行加 `[$PB_WS]` 是同一条道理：
#   **一行不带分母的读数，不论数字是几都不算数。**
#
# ⚠ `fails` 那两支**刻意没动**：写死「`0 passed 不是绿` 这条自检一个字不许改」。
#   代价如实记：**红的那一行今天仍不带分母。** 要补得连着改那条自检的字面，归 PM 裁。
run_gate() {
  local name="$1"; local denom="$2"; shift 2
  gate_wants "$name" || return 0
  local out t0
  t0="$(gate_now_ms)"
  out="$("$@" 2>&1)"
  local rc=$?
  # 🔴 记在**命令执行完之后**，不是在派发之前 —— 收据里 `ran` 那一栏的意思必须是
  #   「这一格的命令真的跑过并被判过」，不是「这一格被点到过」。两者在收据上长得一样。
  gate_ran "$name" "$t0"
  # ⚠ **`rc=0` 不等于绿**：`0 passed` 也会 rc=0（`C7` 逐字：「0 passed 不是绿」）。
  #   ⇒ 两条都判：退出码 + 那行读数里的数字。
  local n
  n="$(printf '%s' "$out" | grep -oE '([0-9]+) (passed|个测试)' | grep -oE '[0-9]+' | sort -rn | head -1)"
  if [ "$rc" -ne 0 ]; then
    fails+=("$name（退出码 $rc）")
    gate_diag "$name" "$out"          # K-R22：判定一个字没动，只是把 $out 端出来
  elif [ -z "$n" ] || [ "$n" -eq 0 ]; then
    fails+=("$name（读数是 ${n:-<找不到>} —— 0 passed 不是绿）")
    gate_diag "$name" "$out"          # K-R22：这一支同样得说清「那它到底打了什么」
  else
    printf '  ok   %-14s %s passed（%s；分母：%s）\n' "$name" "$n" "$(gate_took "$name")" "$denom"
  fi
}

# ★★ `K-H2a`（08-27）：**`--workspace` 是补上来的 —— 在此之前，7 个共享 crate 的判据
#    一条都不在这道门里。**
#
# 起因：`K-H2a` 把 key 那件事落在新开的 `crates/creds-core`，写完 18 条判据、`GATE: OK`，
# 而 `cargo` 那个数**一条没涨**（1195 → 1195）。它正是 `KP3` 那个形状：
# 「有生成物 / 有判据」**不等于**「本地门禁拦得住」。
# 现打的分母（08-27，`cargo test -p <名> --lib` 逐个数）：
#   `guard-core 24 · creds-core 18 · codex-token-core 3 · acct-core 9 · branch-core 8 ·
#    gate-core 8 · shell-quote-core 1` ⇒ **71 条**，其中 **53 条是本件之前就有的存量**。
#   ⚠原文第三项是 `usage-core 11`，合计 **79**、存量 **61** —— 用量下线后那个 crate
#     改建成 `codex-token-core`，现打 `cargo test -p codex-token-core --lib` = **3 条**
#     ⇒ 合计 71、存量 53（存量 = 合计 − `K-H2a` 新开的 `creds-core` 18，两组数各自自洽）。
#
# 裸 `--workspace` 就是 `monitor` ＋ 共享 crate；成员集合由 `cargo metadata` 现取，与真跑到的包两向相等。
#
# ⚠ **CI 那一侧没跟着改**（`ci.yml` 不在 `K-H2a` 的写区）⇒ 从此**本地门禁比 CI 严**。
#   别把「本地绿」读成「CI 也会绿」。
# ★★ **它必须求和，不能沿用 `run_gate`** —— 08-27 实测：只把命令换成 `--workspace`、
#    读法照旧，那道门印的仍是 **1195**。
#
# 原因在 `run_gate` 的读法本身：`sort -rn | head -1` 取的是**所有 `N passed` 里的最大值**。
# 单包时只有一行，最大值 = 合计；**多包时它恒等于最大那个包**（`monitor` 的 1195），
# 于是往任何一个共享 crate 加判据，这个数**永远不动**。
# ⇒ 那正是本轮要治的病换了个位置又长出来一次：**命令的射程扩了，读数的射程没扩。**
#
# 本函数改成**逐行求和**，并且判**包的集合**：一个 crate 静默掉出 `--workspace`（改名 / members 漏登记）时，
# 合计只会**变小**，而「变小」和「有测试没跑」在终端上一模一样。
# 该跑到哪几个包从 `cargo metadata` 现取 —— 原先钉的是手抄的包数，每加/删一个 crate 就要回来改、多路合并时撞数。
# 跑到的包集合：每个被测 lib 在 cargo 输出里一行 `Running unittests src/lib.rs (…/deps/<lib 名>-<hash>)`。
gate_ran_libs() {
  sed -nE 's#^[[:space:]]*Running unittests .*[/\\]deps[/\\]([A-Za-z0-9_]+)-[0-9a-f]+(\.exe)?\)[[:space:]]*$#\1#p' | sort -u
}
# 一个 workspace 的成员里**该被 `cargo test --lib` 跑到**的 lib 名（`cargo metadata` 现取，不抄成员数）。
# ⚠ 不看 `[lib] test = false`：有人把一个成员的单测关掉，它就该在这里红，而不是悄悄从「该跑到」里出列。
gate_workspace_libs() {
  ( cd "$1" && cargo metadata --no-deps --offline --format-version 1 ) | python3 -c '
import json, sys
m = json.load(sys.stdin)
ids = set(m["workspace_members"])
LIB = {"lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"}
for p in m["packages"]:
    if p["id"] in ids:
        for t in p["targets"]:
            if LIB & set(t["kind"]):
                print(t["name"].replace("-", "_"))
' | sort -u
}
gate_shell_libs() { gate_workspace_libs src/frontend/shell; }

# 多包求和的那一格（`--workspace`）：`$2` 是一个函数名，它印出**该跑到**的 lib 名集合。
# 判：退出码 0 ＋ 该跑到的集合 == 输出里真跑到的集合（两向）＋ 合计 > 0。
# 一个成员静默掉出 `--workspace` 时合计只会变小，与「有测试没跑」在终端上一样 —— 集合相等认得出来，
# 而且成员是现取的：加/删 crate 不用回来改任何数。
run_gate_sum() {
  local name="$1"; local want_fn="$2"; shift 2
  gate_wants "$name" || return 0
  local out t0 rc want wrc got miss extra lines n pkgs
  t0="$(gate_now_ms)"
  want="$("$want_fn" 2>&1)"; wrc=$?
  out="$("$@" 2>&1)"; rc=$?
  gate_ran "$name" "$t0"
  got="$(printf '%s\n' "$out" | gate_ran_libs)"
  lines="$(printf '%s' "$out" | grep -oE '^test result: ok\. [0-9]+ passed')"
  pkgs="$(printf '%s' "$lines" | grep -c . || true)"
  n="$(printf '%s' "$lines" | grep -oE '[0-9]+' | paste -sd+ - | bc 2>/dev/null || echo 0)"
  miss="$(comm -23 <(printf '%s\n' "$want") <(printf '%s\n' "$got") | grep . | tr '\n' ' ')"
  extra="$(comm -13 <(printf '%s\n' "$want") <(printf '%s\n' "$got") | grep . | tr '\n' ' ')"
  if [ "$rc" -ne 0 ]; then
    fails+=("$name（退出码 $rc）")
    gate_diag "$name" "$out"
  elif [ "$wrc" -ne 0 ] || [ -z "$want" ]; then
    fails+=("$name（列不出该跑到的成员 —— $want_fn 退出码 $wrc：${want:-<空>}）")
  elif [ -n "$miss" ] || [ -n "$extra" ]; then
    fails+=("$name（成员与真跑到的包对不上：该跑没跑 [${miss% }] · 跑了却不是成员 [${extra% }] —— \
合计变小与「有测试没跑」在终端上一模一样，只有这条认得出来）")
    gate_diag "$name" "$out"
  elif [ -z "$n" ] || [ "$n" -eq 0 ]; then
    fails+=("$name（读数是 ${n:-<找不到>} —— 0 passed 不是绿）")
    gate_diag "$name" "$out"
  else
    printf '  ok   %-14s %s passed（%s；%s 个包合计，成员集合 == 跑到的包集合）\n' "$name" "$n" "$(gate_took "$name")" "$pkgs"
  fi
}

# ── 门禁自己的自检（`K-R22 D3`）：**「盘上有 echo」不等于「失败时真的印了」** ──────
#
# ★ 这一格**最容易假绿的写法**是钉一条 `grep -q gate_diag scripts/gate.sh` ——
#   那只证明**盘上有**，不证明**被走到**（`K-R18` 语料八：盘上有 ≠ 被走到）。
#   ⇒ 这里**真跑必红的合成命令**，断言那一趟的**标准输出里出现被测命令自己印的哨兵串**。
#   删掉任何一个 `gate_diag` 调用、或把取法改回「只记退出码」，这几条里至少一条当场红。
#
# ⚠ **成本与副作用 —— `N-G2`（09-05）现打，把上一版那句话改掉了两处。**
#   上一版逐字：「四条探针合计 ≈ 10 毫秒：**不碰 cargo / npm / 网络 / 文件系统**」。
#   量具 `tests/evidence/N-G2-selftest-cost.py`（把自检段真正会走到的那几块**原样切下来**跑，
#   不复算），沙箱 `ccmon-devbox:latest` 里各 9 趟交替跑、取中位数：
#     · **本拍十条（①–⑩）≈ 131 ms**（135.1 ms 减去 4.5 ms 的空跑基线 = **130.6 ms**）
#       ⚠ 同一把尺子隔一会儿复打一次得 **128.6 ms** ⇒ 噪声带大致 **129–131 ms**，
#         **小数位不是读数**；要精确到几毫秒就自己重打，别引这里的小数。
#     · 同一把尺子量基点 `5924b91` **那四条 ≈ 54 ms**（58.5 − 4.2 = 54.3 ms）
#   ⇒ 🔴 **「四条 ≈ 10 毫秒」这句话本身就不成立**（同一把尺子今天量得 54 ms，差 5 倍）；
#     `N-G2` 只答「新加六条要多少」= **≈ +76 ms**。每条探针要 fork 一个 `bash`，那才是大头。
#     ⚠ 这两个数量于**这台机器、这个镜像、这一拍**，是快照不是常量 —— 引用前重打。
#     ⚠ 它们量的是**自检段**，不是整趟门禁（同一拍那一趟墙钟 61 秒）；**两个分母不同，别相减**。
#   ⇒ 「不碰 cargo / npm / 网络」**仍然成立**；**「不碰文件系统」对探针⑩ 不成立**（见下）。
# ⚠ 上面 `N-G1` 那段头注里「全文 `fails+=(` 从 **15** 涨到 **16**」是**那一拍的快照**，
#   今天已经不是全文的数了。本拍现打 **17 处真的 `fails+=`** = 门那侧 **11** ＋ 探针那侧 **6**
#   （口径与量具住 `tests/evidence/N-G2-verdict-md5.py` 的【覆盖自证】，它按「整行 strip 后以 `#` 打头」
#    剔注释）。⚠ 裸 `grep -c` 会比它多几处 —— 多出来的是**头注里引它当主尺的那几行**，
#   **本行自己就是其中一行** ⇒ 那个数每写一句话就变一次，别拿它当读数。
# ⚠ 它们跑在**子 shell**（`$( )`）里 ⇒ 里面那几个 `fails+=` 落在数组副本上，
#   污染不到真裁决；能漏出来的只有标准输出，而那正是要断言的东西。
# ⚠ 探针② 走的是**编译错误那一形**（**没有 `failures:` 段**）—— `K-R22 D2` 特意问的那一格，
#   从此每趟出货都验一遍，不是只在死值验那天验过一次。
# ⚠ 探针③ 造的输出**长过 `GATE_DIAG_WHOLE`（不走「全印」那条捷径）且一条模式都不匹配**，
#   哨兵只出现在最后几行 ⇒ **它只能靠「原文尾部恒印」那半兜底才看得见**。
#   那半是 fail-closed 的承重墙，得有一条判据专门盯着它。
#   ★★ **`NG1D4` 就钉在这一条上**：把 `gate_diag` 里「原文尾部恒印」那两行删掉 ⇒ 探针③ 当场红。
#      09-05 现打验过（死值验 `G1M2`），不是读注释读出来的。
# ⚠ 探针④（`N-G1` 09-05）是探针③的**镜像**，钉的是**另一半**（①关键行真的匹配上了）：
#   它造的失败行**行首带 ANSI 色码**（`ESC[41m``ESC[1m` —— 就是 vitest 那一形逐字节量出来的样子），
#   哨兵**只在这一行上**，而后面垫的无关行条数是 `GATE_DIAG_TAIL + GATE_DIAG_WHOLE + 10`
#   ⇒ ★ 两条捷径**结构上都走不到**：总行数恒大于 `GATE_DIAG_WHOLE`（不走③「全印」）、
#     哨兵那行恒在**尾部 `GATE_DIAG_TAIL` 行之外**（②兜不住它）。
#   ⇒ **它只有在①真的匹配上时才看得见哨兵。** 这正是 `NG1D2` 的 acceptor 点名要挡的那条捷径：
#     「把上限调大让尾部把名字裹进来**不算**」—— 垫的条数跟着上限走，调多大都裹不进来。
#   ⚠ 垫的条数**现算、不写死**：写死一个常数，哪天有人把 `GATE_DIAG_TAIL` 调到比它大，
#     这条判据就静默退化成「②兜底了」，而输出长得一模一样。
#
# ★ 这是**新加的一格自检**，不是改了哪一道旧门 —— 那五道门的判定口径逐字未动。

# ── `N-G2`（09-05）：断「**这一条判定还在判**」的六条（探针⑤–⑩） ──────────────
#
# ★★ **病灶是现打的，不是推出来的**：把 `run_gate` 的 `if [ "$rc" -ne 0 ]` 改成 `-gt 1000`
#   （那一格从此**不按退出码判红**）⇒ **整趟门禁 `GATE: OK`，一格没红**。
#   `N-G1` 实现方自报（死值验 `G1M3`），**PM 独立复现过**（`audits/N-G1-PM.md` `§四 刀乙`）。
#
# ★ **上面那四条为什么接不住**：它们断的是「失败那条路**印没印出**被测命令的输出」。
#   单刀掏掉一条判定时，同一份合成输出会**落到隔壁那条判定上**：那一格照旧红、照旧调
#   `gate_diag`、照旧印出哨兵 ⇒ 四条探针全都满意。**替身接住了，牙一口没咬到。**
#
#   ⚠ **这不是漂移，是当初就没人立过这一格**（`NG2D1` 查了五处，头注之外的独立证据三条）：
#   `K-R22` 要修的是「门禁红了**不说为什么红**」，又禁它碰任何一条判定
#   （逐字：「不动那五道已有的门的判定口径」）；件文件 `KR22D3` 那张探针表把每条探针
#   「专门盯的失效」写成 `gate_diag` 调用被删 / `D2` 那格退化 / 兜底那半塌掉，并把射程逐字
#   写成「**`gate_diag` 本体 ＋ `run_gate` / `run_gate_sum` 两个失败支**，8 个调用点里的 2 个」；
#   `audits/K-R22-PM.md §五1` PM 逐条认了这个射程。⇒ **「哪一条判定在判」从来不在谁的射程里。**
#
# ★ 取法：喂一份**只有目标那一条判定拦得住**的合成输入，**两侧都断**：
#   ① **那一格的绿行不出现**（`  ok   ` 那个前缀）—— 这是本族探针唯一的牙：
#      目标判定被掏空 ⇒ 输入一路落到 `else` ⇒ 那道门**印出绿行** ⇒ 当场红。
#   ② **哨兵在**（失败支真走到、`gate_diag` 真把输出端出来）—— 挡**空真**：
#      门整个没跑起来时输出为空，光断①**会恒真**（`brief` 第 9 条那一族）。
#   ⚠ ①用的是**裸前缀 `  ok   `**，不是「`  ok   ` ＋ 探针自己的名字」：探针名是夹具的名字，
#     拿它当断言子串正是 `brief` 第 12 条点名的那条路。三个函数的绿行逐字都以 `  ok   ` 开头，
#     而每条探针的 `$out` 只装**它自己那一次调用**的标准输出（子 shell）⇒ 里面的绿行只可能是它的。
#
# ⚠ **「只有它拦得住」是每条探针的承重前提**，不是修辞：合成输入必须同时**满足**同一个函数里
#   其余每一条判定。逐条写在每条探针自己的注释里。写错了那一条就退化成「隔壁接住了」，
#   而**死值验会当场逮到**（掏掉目标判定却不红）—— 这一族每条都验过会红、也验过会绿。
#
# ⚠ **射程如实写：11 条判定盖住 6 条，盖不住 5 条。**
#   🔴 下面这几个行号**钉在 `5924b91` 上**（本件基点），**不是本文件此刻的行号** ——
#     本段一落地它们就往下推了。本文件此刻的行号**现算**：
#     `python3 tests/evidence/N-G2-verdict-md5.py <基点>` 的【覆盖自证】每趟都把五块的行范围
#     与那 11 条各自的行号印出来，**那份才是当下的读数**。
#   盖住：`run_gate` 两条（`5924b91:352` `:355`）· `run_gate_sum` 三条（`:409` `:413` `:418`）·
#         `run_e2e` 的退出码那条（`:626`）。
#   盖不住 5 条，逐条给理由（`§4` 登记）：
#     · `generated` 两条（`5924b91:560` `:564`）与 `pb check` 两条（`:714` `:736`）—— **行内，不是函数**，
#       **没有可以喂合成输入的入口**。三条出路各有代价：抽成函数 = 改判定的形状（件计划 `§2` 明禁）·
#       另写一份独立复算 = 「盘上有 ≠ 被走到」的假绿 · 造一次 `git diff` 非空要**改工作树**
#       （`NG2D5` 硬边界明禁）。⇒ **做不到**，不硬凑。
#     · `run_e2e` 的「抓不到「合计 PASS=」」那条（`5924b91:634`）—— 立它要 `rc=0` **且**抓不到那行，
#       而 `tests/e2e/assert-pass-floor.sh` **只有跑完一整套 npm 套件才退 0** ⇒ 立它就得每趟真跑一套。
#       ⇒ **没立**（代价与自检段「不碰 npm」那条承诺直接冲突）。
#
# 🔴 **探针不许改工作树**（`NG2D5` 硬边界）：这十条一个 git 写操作都没有、不往工作树落文件、
#   不碰 `~/.claude`。⚠ 一条例外如实写：探针⑩ **读**了 `tests/e2e/assert-pass-floor.sh`（起一个 `bash`）
#   ⇒ 上面那句「不碰文件系统」**对它不成立**，头注已改。它在**地板参数校验**那一步就 `exit 2`，
#   而那一步排在 `npm run` 与 `mktemp` **之前** ⇒ 走不到 npm、也不落任何文件。
#   ⚠ 它因此**依赖写区外一份文件的一行措辞**（那句 `地板必须是非负整数，实得：$FLOOR` 会把
#     参数原样回显，探针⑩ 的哨兵与那个 `合计 PASS=7` 就藏在参数里）。那份文件哪天不再回显参数，
#     哨兵当场消失 ⇒ **②那一侧红**，是**响的**退化，不是静默的。这正是两侧都断的理由。
gate_assert_judged() {
  local name="$1" out="$2" sentinel="$3" cut="$4"
  case "$out" in
    *"  ok   "*)
      fails+=("gate $name（**「$cut」这一条判定不再判了** —— 喂一份只有它拦得住的合成输入，\
那一格却印出了绿行 ⇒ 一条判定被掏空、门禁照旧放行，正是 N-G2 立这一族探针要治的那一形）")
      return 0 ;;
  esac
  case "$out" in
    *"$sentinel"*) ;;
    *) fails+=("gate $name（断「$cut」的那条探针**自己空转了** —— 绿行没出现，哨兵 $sentinel \
也不在输出里 ⇒ 合成输入根本没走到该走的失败支，这一格判不了，按红记，不许当成绿）") ;;
  esac
}

gate_probe_libs_a() { echo a; }
gate_probe_libs_ab() { printf 'a\nb\n'; }
gate_selftest() {
  # 🔴 探针**不受 `GATE_ONLY` 影响**（`gate_wants` 第一行读它）。bash 是动态作用域 ⇒
  #   这一句在本函数调出去的 `run_gate`/`run_gate_sum` 里也看得见。
  #   把量具的量具过滤掉，在输出面上**一个字都不会说**。
  local GATE_PROBE=1
  local probe
  probe="$(run_gate 自检① - bash -c 'printf "error: KR22-PROBE-A\n"; exit 3' 2>&1)"
  case "$probe" in
    *KR22-PROBE-A*) ;;
    *) fails+=("gate 自检①（run_gate 的失败支没把被测命令的输出印出来 —— K-R22 那一格被改回去了：\
门禁红了又不说为什么红）") ;;
  esac
  probe="$(run_gate_sum 自检② gate_probe_libs_a bash -c 'printf "error[E0425]: KR22-PROBE-B\n --> src/x.rs:1:1\n"; exit 101' 2>&1)"
  case "$probe" in
    *KR22-PROBE-B*) ;;
    *) fails+=("gate 自检②（run_gate_sum 的失败支在「编译错误」那一形上印不出东西 —— \
那一形没有 failures: 段，正是 K-R22 D2 点名要盖住的那格）") ;;
  esac
  probe="$(run_gate 自检③ - bash -c 'i=1; while [ $i -le 100 ]; do
      if [ $i -ge 97 ]; then printf "尾部第 %s 行 KR22-PROBE-C\n" "$i"; else printf "无关行 %s\n" "$i"; fi
      i=$((i + 1)); done; exit 4' 2>&1)"
  case "$probe" in
    *KR22-PROBE-C*) ;;
    *) fails+=("gate 自检③（模式表一条都没匹配上时，「原文尾部恒印」那半兜底没走到 —— \
fail-closed 的承重墙塌了：从此模式表漏掉的形状会退化成一个字都不印）") ;;
  esac
  # 探针④：失败行**行首带 ANSI 色码**（vitest 那一形），哨兵只在这一行上；
  # 垫的无关行条数现算，保证①之外的两条路（②尾部 / ③全印）**都够不着**它。
  probe="$(run_gate 自检④ - bash -c '
      printf "\033[41m\033[1m FAIL \033[22m\033[49m NG1-PROBE-D 行首带色的失败行\n"
      n=$(( $1 + $2 + 10 )); i=1
      while [ "$i" -le "$n" ]; do printf "无关行 %s\n" "$i"; i=$((i + 1)); done
      exit 5' _ "$GATE_DIAG_TAIL" "$GATE_DIAG_WHOLE" 2>&1)"
  case "$probe" in
    *NG1-PROBE-D*) ;;
    *) fails+=("gate 自检④（失败行行首带 ANSI 色码时，「关键行」那一段匹配不上 —— \
N-G1 治的正是这一形：vitest 在非 TTY 下照样上色，ESC 不是 [[:space:]]，\
于是每一趟红都退化成「一条都没匹配上」，死值验拿不到失败用例的名字）") ;;
  esac

  # ── 探针⑤–⑨（`N-G2`）：断「这一条判定还在判」。取法与承重前提见上面那段头注。────
  #
  # 探针⑤ · `run_gate` 的**退出码**那条。
  #   只有它拦得住：读数 `7 passed` ⇒ 「0 passed 不是绿」那条**已被满足**，掏掉退出码那条没人接。
  probe="$(run_gate 自检⑤ - bash -c 'printf "NG2-PROBE-E 7 passed\n"; exit 3' 2>&1)"
  gate_assert_judged 自检⑤ "$probe" NG2-PROBE-E "run_gate 的「退出码非零 ⇒ 红」"
  #
  # 探针⑥ · `run_gate` 的**「0 passed 不是绿」**那条。
  #   只有它拦得住：`exit 0` ⇒ 退出码那条**已被满足**，掏掉这条就一路落到绿行。
  probe="$(run_gate 自检⑥ - bash -c 'printf "NG2-PROBE-F 0 passed\n"; exit 0' 2>&1)"
  gate_assert_judged 自检⑥ "$probe" NG2-PROBE-F "run_gate 的「0 passed 不是绿」"
  #
  # 探针⑦ · `run_gate_sum` 的**退出码**那条。
  #   只有它拦得住：跑到的 {a} == 该跑的 {a} ⇒ 集合那条**已被满足**；
  #   合计 7 ≠ 0 ⇒ 「0 passed 不是绿」也**已被满足**。
  probe="$(run_gate_sum 自检⑦ gate_probe_libs_a bash -c 'printf "  Running unittests src/lib.rs (/x/deps/a-0a1b)\ntest result: ok. 7 passed\nNG2-PROBE-G\n"; exit 3' 2>&1)"
  gate_assert_judged 自检⑦ "$probe" NG2-PROBE-G "run_gate_sum 的「退出码非零 ⇒ 红」"
  #
  # 探针⑧ · `run_gate_sum` 的**成员集合自检**（采集面：真跑到的包 ≠ 该跑的成员）。
  #   只有它拦得住：`exit 0` ⇒ 退出码那条**已被满足**；合计 7 ≠ 0 ⇒ 「0 passed」那条也**已被满足**；
  #   只跑到 {a} 而该跑的是 {a, b} ⇒ 差的正是集合这一条。
  probe="$(run_gate_sum 自检⑧ gate_probe_libs_ab bash -c 'printf "  Running unittests src/lib.rs (/x/deps/a-0a1b)\ntest result: ok. 7 passed\nNG2-PROBE-H\n"; exit 0' 2>&1)"
  gate_assert_judged 自检⑧ "$probe" NG2-PROBE-H "run_gate_sum 的「成员集合 == 跑到的包集合」"
  #
  # 探针⑨ · `run_gate_sum` 的**「0 passed 不是绿」**那条。
  #   只有它拦得住：`exit 0` ＋ 跑到的 {a} == 该跑的 {a} ⇒ 前两条**都已被满足**，合计恰好是 0。
  probe="$(run_gate_sum 自检⑨ gate_probe_libs_a bash -c 'printf "  Running unittests src/lib.rs (/x/deps/a-0a1b)\ntest result: ok. 0 passed\nNG2-PROBE-I\n"; exit 0' 2>&1)"
  gate_assert_judged 自检⑨ "$probe" NG2-PROBE-I "run_gate_sum 的「0 passed 不是绿」"
}
gate_selftest

# ── `worktree-clean`：**前置条件** —— 仓里不许有第二份工作副本（第 25 格，09-19）──
#
# 🔴 **它排在所有格之前，这是刻意的。** 本仓有一族判据的人群是「走文件系统」
#   （`walk` / `read_dir` / `readdirSync` / `find` / `eslint .`），现打至少 8 份文件里
#   有这种取法。仓内一出现第二份工作副本（并发 agent 的 worktree、死值验的变异副本），
#   它们的人群就**静默膨胀**，然后一片**和本拍改动毫无关系**的红。
#   〔2026-09-19 一天绊了三次：`shell_lint_registry` · `bus_identity_registry` ·
#     `cc_bus_deploy` · `eslint 基线 7 → 1383` 同时红，每次都要花时间才认出来。〕
#
# 🔴 **更坏的那一半今天没发作但它在**：上面那几条是**恒等**断言所以红得响。
#   同族里凡是用**地板**（`>= N`）的，人群膨胀时**一声不吭地过去** ——
#   「多扫了 1138 个文件」在地板下和「扫对了」长得一模一样。**那才是本格的真正理由。**
#
# ⚠ 它**不修**那一族（八处以上的改动，另案），只让这个条件先出声、并说清
#   「这不是你的改动坏了」。⇒ 本格红时**先清副本再重跑**，别去追下面那些红。
run_gate worktree-clean '判过的条数（抽样的 4 个扩展名 `.sh`/`.mjs`/`.rs`/`.ts`，每个一条**恒等**断言：`git ls-files` 认的份数 == 走文件系统走出的份数）。⚠ **抽样不是全集** —— 挑的是那几条真出过事的判据在数的东西（`.sh`→shellcheck 那一族 · `.mjs`→eslint 基线 · `.rs`→`readonly_guard` 的分区恒等 · `.ts`→`tsc` 的 `want`）。⚠ 它买的是「仓里没有第二份工作副本」，**买不到**「所有判据的人群都对」——一份被 `.gitignore` 掉的源码同样会让走文件系统的判据多看一份，而本格按 gitignore 的口径算、看不见它' \
         python3 tests/evidence/K-W25-worktree-clean.py


# ── `hooks/` 里那份**会被执行**的东西，跑不跑得起来（`K-R82` 09-12，第 13 格）──────
#
# ★★ 问题是 `K-R80` 的转置读数：**12 格里 0 格看着 `hooks/`**，
#   而 `tests/hooks/pre-commit` 与另两棵 0 覆盖的树（`evidence/` · 仓根文件）**性质不同** ——
#   它**会被 git 执行**、跑在**每一次提交**上、**能改仓**（它挡的是 `C7` 那条
#   「`[profile.dev]` 不许进提交」）。
#
# 🔴 **本格落地那一趟就逮到一条真的**（现打，不是合成的）：
#   `tests/hooks/pre-commit` 在 index 里是 **`100644`** ⇒ 本仓 `core.filemode=false`，
#   `chmod +x` 从来没进过 git ⇒ **每一棵新 checkout 出来的工作树里它都是 644**，
#   而 git 对 644 的 hook 的处置是**忽略它并照常提交**（`rc=0` ＋ 一句可关掉的 advice hint，
#   现打读数在 `tests/evidence/K-R82-hooks-gate.md` `§1`）⇒ **那道挡在那些树里等于不在。**
#   本拍用 `git update-index --chmod=+x` 把它记进库里，本格从此盯着它不再掉。
#
# ⚠ **两句话分开判**（记忆条 `filemode-false-chmod-invisible`）：
#   「**盘上跑不跑得起来**」（`test -x`，跟 checkout 走）与「**库里记没记**」
#   （index mode，跟提交走）是两个互不相干的事实，本格各判一条、红了也分开说。
# ⚠ **失效方向**（件计划 `KR82D1` 逐字）：**只判「文件在不在」** —— 那和「它跑得起来」
#   是两件事。⇒ 判据里一条 `test -e` 都没有。
# ⚠ 判据本体住 `scripts/hooks-are-runnable.sh`（含 8 条阳性对照：三把尺子**正反各一条**，
#   挡「尺子瞎了」也挡「尺子恒红」）—— 放在那儿是为了能**对着变异过的副本**跑死值验，
#   不必去动真工作树。
# ⚠ 本格的数是**数出来的**（每个 hook 文件 3 条 ＋ 8 条阳性对照），跟 `fmt` 那几格的
#   「只有绿/红两态」不同 —— 往 `hooks/` 里加一份 hook，这个数会涨，那是对的。
run_gate hooks '每个被跟踪的 hook 文件 3 条（盘上可执行 · 库里记着可执行位 · 语法过得了它自己声明的解释器）＋ 8 条阳性对照。⚠本行原先写着「现打 hooks/ 下 1 个文件 ⇒ 11」——那是个**手抄的份数**，而本拍加了 `tests/hooks/pre-push` 之后盘上是 2 份 ⇒ 14。同 `copy2` 那一拍的订正：**摘掉抄来的数**，份数以判据本体自己印的那一行为准（它每趟从 `git ls-files tests/hooks/` 现算）。hooks/ 之外的任何一棵树本行都盖不到' \
         bash tests/scripts/hooks-are-runnable.sh

# ── 量具的**还原那一跳**有没有把旧 mtime 搬回被测树（`K-R115` `KR115D1`，09-14，第 14 格）──
#
# ## 问题：一条纪律立了一天，第二天在另一把量具里又长出来
#
# `K-R75`（09-12）：变异台 `restore` 用 `shutil.copy2` 把**旧 mtime** 一起搬回
#   ⇒ `cargo` 判「源码没变」⇒ 复用上一刀的产物 ⇒ **那一趟读到的是上一刀的回声**，整趟作废。
# `K-R102`（09-13）：**同形复发**在另一把量具上 —— `M6-final` 印 `GATE: OK · backend 755`，
#   而那条判据还在盘上、一趟都没跑。它自己逮住并重跑。
# ⇒ 当时的处置逐字是「每趟变异都要有一个『它真的重编过吗』的活体信号」——
#   **一句纪律，没有任何东西在执行它**。第二次发生就是证据。本格是那句纪律的机器面。
#
# ⚠ **它判的不是「源码里有没有 `copy2` 这个词」**：`shutil.copy2` 有正当用途
#   （造夹具 · 拷读数文件 · 把二进制搬进临时目录），那些一个都不该红。
#   判的是**这一次复制的目的地落不落在「被 git 跟踪的工作树内容」上** —— 落在那儿，
#   你在还原被测源码；落在临时目录 / 一个 git 里一份文件都没有的暂存目录，你在造夹具或备份。
# ⚠ 判据本体住 `tests/evidence/K-R115-ruler.py`（`--census` 只印人群表不判，供死值验对照）。
#   它的**诚实边界**（看不见 shell 串里的 `cp -a`、看不见 `tarfile`、判落点不判意图）
#   逐条写在那份文件的头注里，**这里不复述一份**（复述就会漂）。
# ⚠ 本格是**唯一一格盖到 `evidence/`** 的门。那棵树在 `K-R80` 的登记里此前是
#   「0 格覆盖，而这正是它的用途」（`[J3 陈账]` 死锁的泄压口）—— 本格落地之后那条登记要跟着改，
#   随动逐处交回 PM，`tests/evidence/K-R115-deathvalue.md` 里点名。
run_gate copy2 '`evidence/*.py` 里，`shutil` 保元数据复制族（copy2 · copytree · copystat）的**调用点**数，逐处判目的地；绿行那个数就是判过的调用点数。⚠ 本行原先写着「现打 176 份」——那是从判据本体那句现算的分母**手抄**过来的第二份，而本件落地前盘上已经是 183、落地后 185 ⇒ **摘掉那个抄来的数**，份数以 `tests/evidence/K-R115-ruler.py` 自己印的那一行为准。⚠ 只看 `evidence/` 下的 `.py`，别的目录、别的语言、shell 串里的 `cp -a` 本行一概盖不到' \
         bash -c 'python3 tests/evidence/K-R115-ruler.py'

# ── `shellcheck`：shell 脚本的 `--severity=error` 那一档 ───────────────────────────────────────
#
# 人群的**唯一住址**是下面这一行（CI 的那个 job 调本格，不另记一份）；
# `shell_lint_registry` 读这一行判「每个 shell 脚本要么在人群里、要么登记豁免」。
# ⚠ 本段刻意不让任何一行以 `#` ＋ 空格 ＋ 那个工具名开头 —— 那是它的指令语法，会把本文件自己判红。
# 反空真：展开出来的份数 > 0，且本文件自己必须在里面（正控）；`nullglob` 下一组 glob 一个都不匹配时
#   那一组就静默没了 —— 由 `shell_lint_registry`「盘上每个 shell 脚本都被某个 glob 盖住」接住，不靠数份数。
# ⚠ 只判 error 这一档；本地与 CI 是两份 shellcheck 二进制，版本可能不同。
GATE_SHELLCHECK_GLOBS='tests/e2e/*.sh src/shared/cc-bus/scripts/* src/shared/cc-bus/examples/cc-keepalive tests/e2e/fake-claude tests/e2e/weak-net/*.sh tests/e2e/local-backend-container/*.sh tests/evidence/*.sh tests/scripts/*.sh tests/hooks/*'
gate_shellcheck() {
  local out rc n self=0 f
  command -v shellcheck >/dev/null 2>&1 || {
    printf 'shellcheck: 这台机器上没有 shellcheck —— 判不了，按红记（不许退化成静默跳过）\n'
    return 1
  }
  local -a files=()
  shopt -s nullglob
  # shellcheck disable=SC2206  # 这里要的就是按空白拆开、再按 glob 展开
  files=($GATE_SHELLCHECK_GLOBS)
  shopt -u nullglob
  n=${#files[@]}
  for f in "${files[@]}"; do [ "$f" = tests/scripts/gate.sh ] && self=1; done
  if [ "$n" -eq 0 ] || [ "$self" -ne 1 ]; then
    printf 'shellcheck: 人群展开出 %s 份、本文件%s在里面 —— 展开坏了。「扫了 0 个」与「全都干净」在退出码上一模一样，按红记\n' \
           "$n" "$([ "$self" -eq 1 ] && echo || echo 不)"
    return 1
  fi
  out="$(LC_ALL=C.UTF-8 shellcheck --severity=error "${files[@]}" 2>&1)"
  rc=$?
  printf '%s\n' "$out" | head -80
  if [ "$rc" -ne 0 ]; then
    printf 'shellcheck: 退出码 %s\n' "$rc"
    return "$rc"
  fi
  printf 'shellcheck: %s passed（人群住本文件 GATE_SHELLCHECK_GLOBS 一处）\n' "$n"
}
run_gate shellcheck '不是「几条断言过了」：这个数是人群展开出来的 shell 文件份数（`--severity=error`）。⚠ 只判 error 这一档；`.ps1` 全仓零 lint' \
         gate_shellcheck

# ── 按步骤名照 `ci.yml` 原样跑某几步 —— 只给**留在 Windows job 里**的那几步用（`coverage` · `audit`）。
# 那几步的命令住 `ci.yml` 的 Windows job（门禁跑不了真 Windows，那个 job 保留），本文件不抄第二份。
# ⚠ 只认在仓根跑的步骤：名字与 `run:` 之间出现 `working-directory:` ⇒ 按红记；名字对不上 ⇒ 同样按红记。
gate_ci_step_body() {
  awk -v want="- name: $1" '
    { s = $0; sub(/^[ \t]+/, "", s) }
    st == 0 { if (index(s, want) == 1) st = 1; next }
    st == 1 {
      if (s ~ /^#/) next
      if (s ~ /^working-directory:/) { print "\001WORKDIR"; exit }
      if (s ~ /^run:[ \t]*[|>]-?[ \t]*$/) { match($0, /^ */); key = RLENGTH; st = 2; next }
      if (s ~ /^run:/) { sub(/^run:[ \t]*/, "", s); print s; exit }
      if (s ~ /^- / || s == "") exit
      next
    }
    st == 2 {
      if (s == "") { print ""; next }
      match($0, /^ */)
      if (RLENGTH <= key) exit
      if (ind == 0) ind = RLENGTH
      print substr($0, ind + 1)
    }
  ' .github/workflows/ci.yml
}
gate_ci_steps() {
  local label="$1"; shift
  local name body ran=0 rc
  for name in "$@"; do
    body="$(gate_ci_step_body "$name")"
    case "$body" in
      '')
        printf '%s: ci.yml 里找不到名字以「%s」打头、带 run: 的那一步 —— 判不了，按红记\n' "$label" "$name"
        return 2 ;;
      $'\001WORKDIR'*)
        printf '%s: ci.yml 里「%s」那一步带 working-directory，本函数只在仓根跑 —— 判不了，按红记\n' "$label" "$name"
        return 2 ;;
    esac
    printf '%s: ── 照 ci.yml 跑「%s」\n' "$label" "$name"
    bash -eo pipefail -c "$body" 2>&1
    rc=$?
    if [ "$rc" -ne 0 ]; then
      printf '%s: 「%s」退出码 %s —— CI 上同一步会红\n' "$label" "$name" "$rc"
      return "$rc"
    fi
    ran=$((ran + 1))
  done
  printf '%s: %s passed（ci.yml 里 %s 步原样跑过）\n' "$label" "$ran" "$ran"
}

# ── `e2e-smoke`：两步只读盘上文本的体检 ─────────────────────────────────────────────────────
# `tests/e2e/*.py` 过 `py_compile` · `src/shared/` 下带 shebang 的文件在 git 里是 100755（`exec-bit-guard.sh`）。
gate_e2e_smoke() {
  python3 -m py_compile tests/e2e/*.py || { printf 'e2e-smoke: tests/e2e/*.py 编不过\n'; return 1; }
  bash tests/e2e/exec-bit-guard.sh || return $?
  printf 'e2e-smoke: 2 passed（py 语法 · 可执行位）\n'
}
run_gate e2e-smoke '步数：`tests/e2e/*.py` 的 `py_compile` · `tests/e2e/exec-bit-guard.sh` 两步，只读盘上文本' \
         gate_e2e_smoke

# ── `release-gate`：**发版那条流水线的两件事**（`K-R124` `KR124D1`/`KR124D2`，09-15，第 20 格）──
#
# ## 问题：一条**从加进去那天起就不可能过**的守卫，在本地一个字都看不见
#
# `K-R114`（09-14，`d1a0552`）在 `ci.yml` 里加了「`release.yml` 手工触发守卫」，判据本体
# 整段写在 `run: |` 块里。而 **runner 会把 `run:` 里的 `${{ … }}` 先求值再交给 shell**
# ⇒ 它要比的那个字面渲染后变成 `"false"`，与盘上那串模板**在三个触发器上都必不相等**。
# 云端实打读数住 `tests/evidence/K-R123-发版读数.md § 1.3`。
#
# 🔴 **为什么坏了一个月没人看见**（这一半才是本格存在的理由）：那段判据用 `yaml.safe_load`
#   写，而沙箱镜像里 `python3 -c 'import yaml'` 是 `ModuleNotFoundError`
#   ⇒ **它在本地一次都跑不起来**；而 `ci.yml` 里它前面那一步（shellcheck）先红，
#   `-e` 带着它一起没执行 ⇒ 云端也**从来没露过面**。两头都看不见。
# ⇒ 本格把它收进本地门禁：判据本体搬到 `tests/evidence/K-R124-ruler.py`（不依赖 PyYAML，
#   自带 YAML 子集切块器），**CI 那一步与本格跑的是同一份文件**，不是两份抄件。
#
# ⚠ 射程与买不到的东西逐条写在那份文件的头注里，这里不复述一份（复述就会漂）。放在那儿也是为了能对着变异过的副本跑死值验
#   （`K_R124_ROOT=<副本>` / `RELEASE_WORKFLOW=<某份 release.yml>`），不必去动真工作树。
#
# ── 🔴 `19b`（09-19）：本格**多买了第三件事 —— 产字节那条路** ────────────────────
#
# 条 63 承诺三格平台，`G4`（上面 `platform` 那一格）已经把「**门禁盖到了哪几格**」对上了。
# 但那一格读的是**本文件**，它答不了另一半：「**发版那趟真的为那几格产字节吗**」。
# 两半必须分开，理由是硬的（现打）：三个落点全部 gitignore ⇒ **字节不进仓**，
# 三条产线**只由 `release.yml` 一个文件驱动** ⇒ 这张表的门禁**只能建在 `release.yml` 上**。
# ⇒ 本格从 `19b` 起同时判：⑨ 承诺的平台 ↔ 产线两向相等、本文件里「编后端」的步骤 ↔ 登记
#   两向相等、target triple 两向相等、runner 标签逐字；⑩ 每一处抠 `const BUILD_ID` /
#   身份戳界标的住址**实打读那份文件**、抠不出恰好一行就红；⑪ `build.rs` 那一侧
#   「抠不到」是一条**所有构建形态都响**的失败（`"unknown"` 兜底从类型上消失）；
#   ⑫ 本格 `muslbuild` 裁词里点名的工具链版本 == `release.yml` 真装的那两个。
# 🔴 ⑩ 有现物：步 9 把 `BUILD_ID` 搬进 `lib.rs` 时，`release.yml` 里**两处**抽取只改了一处，
#   另一处留在 `main.rs`（那里今天没有那个 const）⇒ 真发版会死在抽取上。本拍两件事一起做：
#   住址收进 `env.CCM_BACKEND_IDENTITY_SRC`（一处），并让判据每趟实打核它指得到真东西。
#
# ── ⑬：产字节那条路的**本机那一端** ─────────────────────────────
# 同一条路，`19b` 收了云端那一端（产线 ＋ 吃字节的 `build.rs`），`19c` 收本机那一端：
# 「bump 了 `BUILD_ID` 之后，谁把那两份内嵌字节重编回来」。在它之前那条配方**手抄在
# `build.rs` 的 panic 文案里**，而且与发版那趟**不是同一条路**（`rust-lld` vs `zigbuild`）。
# ⇒ 收成一条命令 `tests/scripts/re-embed.sh`，两侧配方由本格两向对拍。
# 🔴 ⑬d 也有现物：步 8 全仓改名之后 `src/frontend/shell/.gitignore` 还写着 `/embedded-daemons/`
#   与 `/native-daemon/` ⇒ **两个内嵌落点从那天起就没被挡住**（09-19 现打 `git check-ignore`
#   两条都不命中），而与 `release.yml` 文件头都还把「三个落点全部
#   gitignore」当硬事实在用 —— 那句话在本拍之前是假的。
# ⚠ **本格不因此变成「编译格」**：⑬ 一条字节都不编，⑬g 真跑的只是那条命令的 `--check`（只读）。
run_gate release-gate '判过的条数（`release.yml` 上逐行印出来的 PASS：三条地板 ＋ ①触发得了 ②手工默认不发布 ③`env.PUBLISH` 字面 ④两处发布步骤的闸 ⑤CI 门的闸 ⑥两处发布步骤各自的正文来源 ⑦生成器排在发布步骤前面 ⑧生成器吐得出本版正文 ＋⑨产字节那条路（承诺的平台 ↔ 产线 · 编后端的步骤 ↔ 登记 · target triple ↔ 登记，三条都是**两向集合相等**；每条产线步骤在那个 job 里 count()==1；runner 标签逐字）⑩每一处抠 `const BUILD_ID`／身份戳界标的住址，逐处计数相等 ＋ **实打去读那份源码**、抠不出恰好一行就红 ⑪`build.rs` 那一侧「抠不到」是所有构建形态都响的失败（`unknown` 兜底从类型上消失）⑫本文件 `muslbuild` 裁词点名的工具链版本 == `release.yml` 真装的那两个 ＋⑬`BUILD_ID` bump 的同拍步骤 re-embed（`tests/scripts/re-embed.sh` 是出路的**唯一住址**，`build.rs::REEMBED_CMD` 逐字指着它；本机那条配方与 `release.yml` 产字节那一步**同源** —— target **两向集合相等** ＋ 旗标逐字相同；它铺的 arch ↔ `build.rs` 吃的 arch **两向集合相等**；三个内嵌落点 ↔ `src/frontend/shell/.gitignore` 里带机检锚的那几行**两向集合相等**；`build.rs` 那两个内嵌函数的出路各点名那条命令 ≥2 处、代码行里不许再手抄第二条产字节配方；mtime 那张安全网仍看**两份**源码；末一条**真跑** `re-embed.sh --check`，要有数）。⚠ 它**不执行 GitHub 的表达式求值器**，也**不跑那条流水线** ⇒ 「盘上这几份文本满足这几条」不等于「云端那一趟会绿」——⑨ 尤其如此：「登记的那一步在文件里」≠「那一步在 runner 上编得出字节」，更不等于「那份字节在目标机器上跑得起来」，真机那一维仍是**判不了**；⚠ 「往 Release 上写」只认两种形状（`softprops/action-gh-release` 的 `uses:` · `run:` 里的 `gh release`/`gh api …/releases`），换第三种路子上传它看不见；⚠ 正文**写得对不对**它一个字都不判；⚠ ⑬ 那一组同一条边界 —— ⑬a–⑬f 全是**盘上文本**的对拍，「配方写得一样」≠「那条命令今天在这台机器上跑得出字节」（它要 zig ＋ cargo-zigbuild，本格一个都不装、不跑）；⑬g 真跑的只是 `--check`（**只读**），在一棵没铺字节的树上它只答得出「这里没有一份对不上的字节」，**不是**「字节是对的」，更不是「发版那一拍办完了」' \
         python3 tests/evidence/K-R124-ruler.py

# ── `muslbuild`：**远端 Linux 那一格**（G4 · 09-19，第 23 格）─────────────────
#
# 🔴 **它补的是登记的 G4 空洞①**，逐字：「**musl 那两个 target 在 CI 与
#    本地门禁里都是零命中**（只在 tag 那天编一次）」。远端 Linux 是**条 63 点名的三格
#    承诺平台之一**，而它在每次提交上一个字节都没人验 —— 坏了要等推 tag 那天才知道。
#
# 🔴 **用 `cargo zigbuild`、版本跟 `release.yml` 对齐，这不是洁癖**：
#    zig **0.14.0** ＋ cargo-zigbuild **0.23.0**。
#    版本不同 ⇒ 本格的绿**不代表发版那趟会绿**，而那正是这一格要买的东西。
#    🔴 **这句话从此有人核了**：上面 `release-gate` 那一格的 ⑫ 把
#      「本格裁词里点名的版本」与「`release.yml` 里真装的那两个」**两向对拍** ——
#      在那之前这是两处手抄的数，漂了没有任何东西会说话。
#    ⚠ 原文这里写着 `release.yml:168` / `:173` 两个**行号**，`19b` 删掉了：
#      行号会随那份文件的每一次改动而漂（本拍就漂了 20 多行），而它指的东西已经由 ⑫ 核着。
#    ⚠ 现打一条差异如实记：**宿主上装的是 zig 0.16.0**；沙箱镜像刻意钉 0.14.0 ——
#      门禁要代理的是**发版那条路**，不是这台开发机。
#
# ⚠⚠ **诚实边界，写死别读宽**：它买的是「**编得出静态字节**」。
#    **买不到**「那份字节在真的远端 Linux 上跑得起来」（没有真机、没有运行）；
#    **买不到** `--all-targets`（这里只编 bin：测试档在 musl 上要跑不要编，另一回事）。
run_gate muslbuild '不是数出来的数：两个 musl target 各一趟 `cargo zigbuild`，只有绿/红两态。分母 = **条 63 承诺的「远端 Linux」那一格的两个 arch**（`x86_64` ＋ `aarch64`），逐个编。⚠ 买的是「编得出静态字节」，**不买**「在真远端上跑得起来」（无真机、不运行）、**不买** test 档（只编 bin）。⚠ 工具链版本与 `release.yml` 对齐（zig 0.14.0 / cargo-zigbuild 0.23.0）—— 版本一漂，本格的绿就不再代表发版那趟会绿' \
         bash -c 'cd src/backend && n=0; for t in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do cargo zigbuild --target "$t" >/dev/null || { echo "musl: $t 编不过"; exit 1; }; n=$((n+1)); done; printf "muslbuild: %s passed（两个 arch 各一趟 cargo zigbuild，zig $(zig version)）\n" "$n"'

# ── `platform`：**承诺的平台 ↔ 门禁真跑的格**（G4 · 09-19，第 24 格）──────────
#
# 🔴 **上面那几格各自只说「我编得过」，没有任何东西说「该编的都编了」。**
#    条 63 点名三格承诺平台（本机 Windows x86_64 · 远端 Linux · 本机 Linux）＋
#    一格显式拒绝（Windows aarch64）。少一格门禁、或多一格没人登记的 target，
#    在今天的输出面上**一个字都不会说**。这一格买的就是那句话。
# ⚠ 它是**登记的机检**，不自己编任何东西 —— 判的是「门禁盖到了哪些平台」，
#   **不判那些平台上真的跑得起来**（那一维仍然判不了，逐字写在判据本体的头注里）。
run_gate platform '判过的条数（判据本体每趟现算并印在它自己那行上：P1 承诺表↔门禁格**两向集合相等** ＋ P2 每格一条逐字锚点 count()==1 ＋ P3 显式拒绝的那格全仓零脚印；原来的 P4「壳-折」随那一档放弃删了）。⚠ **反空真锚是 P1 那两向相等**，不是「承诺表里每条都找得到」——后者在表被清空时恒真。⚠ 它不编任何东西：判的是**门禁盖到了哪些平台**，不判那些平台上真跑得起来' \
         python3 tests/evidence/K-G4-platform-ledger.py

# ── `installface`：**安装面切件方案与量具的对账**（`K-R128`，09-15，第 21 格）──────
#
# ## 问题：第三块（`S1`–`S5`）要开工了，而撑着那个切法的两句话**一句都没有闸**
#
# 甲 `K-R117` `§3-3` 的**收工判据**逐字要「该件那一组的前端落点数恒等于 1」——
#   而 `ruler.py` 的 `§S5b` 今天**只出读数、不判红** ⇒ 那句话没有闸，
#   「这一组收干净了没有」只能靠人看那张表。
# 乙 **纪律 A**（「第二拍全程不改命令名 ⇒ 三份共用文件一字不动 ⇒ 五件写区才真不相交」）
#   **完全建立在纪律上，没有任何东西在盘它**，而 `S1` / `S5` 是方案里唯一许并跑的一对
#   ⇒ 一旦有人顺手改了个命令名，两件当场撞车**而且没人会响**。
#
# 🔴 **为什么非要进门禁**（`KR128D4` 的裁词）：这两条判定的失效方向**就是「没人在看」**。
#   不进门禁，唯一的跑者是「`S1`–`S5` 的实现方记得跑」—— 而件文件逐字写着那不是答案；
#   并跑窗口恰恰就是它全程静默的那一段。CI 那侧也够不着（本格断网跑，它是本地 evidence 量具）。
#
# 🔴 **它为什么第一天就是绿的**（`K-R128` `§0c`，这一条是承重的）：前端落点现打 8 份、目标 3，
#   把「恒等于 1」直接打开会**当场全红、而且要红到第三块做完** —— 那不是闸，是把门禁钉死。
#   ⇒ 装的是**棘轮**：每组当前落点**名单**钉在 `FRONTEND_PIN` 里，判**逐字相等**
#   （不是 `<=` —— `<=` 只防涨、不防「悄悄记错」）。份数一律 `len()` 现算，表里没有基数字面量。
#   ⇒ 任何人往上加一份落点、或把表改馊，**当场红**；某一件真收干净了，
#   **同一拍**把它那一行降下来 —— 降不动就是没做完。**不许改成 `<=`、不许把名单改大让今天好过。**
#
# ⚠ 射程与买不到的东西逐条写在 `tests/evidence/K-R117-ruler.py` 的头注里（`B1`–`B8`），
#   这里不复述一份（复述就会漂 —— 与上面两格同一条取法）。其中要在这儿点一句的只有一条：
#   **`src/frontend/shell/src/parity_ledger.rs` 那一份 `§S5e` 判不了**（那 22 条命令名就是从它解析出来的
#   ⇒ 空真），它的闸在 `§S5c` 的闭集判定 —— 别把这一格读成「三份共用文件都判了」。
run_gate installface '判过的条数（`§S5c`/`§S5d`/`§S5e` 三节逐条印出来的 PASS：22 条命令各归一组 ＋ 闭集并集两向 ＋ 五组交集空 ＋ 5 组前端落点棘轮 ＋ 22 条包装层入口两侧 ＋ `claims()` 10 个装/卸符号各有着落）。⚠ `ruler.py` 原有的 `R1`–`R7` **不在这个数里**（它们只在红的时候出声，没有逐条的「过了」事件）⇒ 这个数**不是**「那把尺子判过的全部条数」。⚠ 落点只认**调用形状** `.<命令>(`，只在注释/散文里提到命令名的**不算落点**（否则这把尺子可以靠删一条注释变绿）；别的调用形状（`invoke("<名>")` 直呼）它看不见，那一档逐处印在 `§S5d` 第二档里只出读数。⚠ 度量的是「几**份**文件」不是「几处引用」⇒ 往一份已经在名单里的文件里再加一处引用**不红**。⚠ `parity_ledger.rs` 那一份 `§S5e` **判不了**（空真），闸在 `§S5c`' \
         python3 tests/evidence/K-R117-ruler.py


# ── `ccbus-twophase`：**cc-bus 两阶段读口 ＋ 三个适配 trait ＋ Windows 那一侧**（`w24c`，09-19，第 24 格）──
#
# ## 问题：cc-bus 这一族此前**没有任何一格在判它的行为**
#
# 落地前现打：`grep -c cc-peek tests/scripts/gate.sh` = **0**。
# 这棵树以前只被两样东西碰过 —— `shellcheck`（语法那一档）与 `plugin_class_registry`
# （脚本**条数**）⇒ 「这条命令做的事对不对」这一维一格都没有。
# 而本轮加的正是**有副作用的那一跳**（推进已读位置），它错一次的形状是
# 「消息被消费掉却没人看见」（实测：160KB 积压那件事故，40 条一次性读不到了）。
#
# ⚠ 判据本体住 `tests/evidence/W24C-ccbus-twophase-ruler.py`，**射程与买不到的东西
#   逐条写在那份文件的头注里**，这里不复述一份（复述就会漂 —— 与上面几格同一条取法）。
#   要在这儿点一句的只有一条：**没有真 Windows 机器** —— 那一侧靠 `CCBUS_ADAPT_OS=windows`
#   在 Linux 上把实现整段跑一遍，买的是「实现跑得通、能力自陈与降级是真的」，
#   **不买**「在 Windows 上跑得起来」。
run_gate ccbus-twophase '判过的条数（判据本体每趟现算并印在它自己那几行上：静态 7 条 —— `cc-peek` 零写面（写形集合 == 登记的两处豁免，且正控要在 `cc-commit` 上扫出写）· `.pos` 写点全仓集合相等 · 锁族不增 · 通用层零脚印（带正控）· 与 `cc-recv` 的渲染逐字节对拍 · `cc-recv` 的 sha256 恒等 · 手册页那几句；真跑 14 条 —— 令牌/CAS/anchor/分段/并发/截短自愈/Stop 钩子两条路/Windows 那一侧四条；kinds 静态 2 条（敲门模板零正文 ＋ 正控）· 真跑 3 条；保活 1 条 —— 共 27）。⚠ **反空真锚不是这个数，是末尾那条「标签集合与登记两向相等」** —— 某一格悄悄没跑与它过了，在输出上一模一样。⚠ 它**不判**在真 Windows 上跑得起来（无真机）、不判性能、不判并发的公平性' \
         python3 tests/evidence/W24C-ccbus-twophase-ruler.py

# ── 格式漂移 ────────────────────────────────────────────────────────────────
#
# 🔴 **这一格补的是本文件头注里那条「归 PM」的第 ②**（09-10 落，PM）。
#   那条注释当时写着**不能加的理由**，逐字：「卡在沙箱镜像没装 `rustfmt` 组件」
#   ＋「在它改之前，把这两维写成一道门 = 把 55 棵树的门禁一起打红」。
#   **那个前提 09-10 被改掉了**：`.claude/devbox/Dockerfile` 加了一层
#   `rustup component add rustfmt`（仓外文件，不进版本控制）。
#
# ## 为什么它值一道门 —— 同一天付了两次学费
#
# 09-10 云端 Windows 那一格**连红两趟，两趟都红在 `cargo fmt --check`**
# （run `34460879900` @ `payload.rs:2308` · run `34467490069` @ `cc_bus.rs:2328`），
# 而两趟之前**本机沙箱门禁都是 `GATE: OK`**。
# 🔴 更贵的是它的位置：**`fmt` 是那一格的第一步** ⇒ 它红了之后
# `clippy` / `cargo test` / 生成物检查**全部 `skipped`**
# ⇒ 那两趟真正想验的东西（Windows 上 cc-bus 那条修复）**一次都没跑到**。
#
# ★ 定性：**本机门禁不是云端的超集，而「绿」这个字在两边长得一模一样。**
#   这一行就是把那句话变成假的。
#
# ⚠ **诚实边界，别读宽**：
#   · 它买的是「**排版与 rustfmt 一致**」，**买不到**「代码对」。
#   · 头注那条 ① （Windows 那半编不编得过，要 `--target x86_64-pc-windows-msvc`）
#     **今天仍然没买到** —— 沙箱镜像仍没装那个 target。**别把这一格读成两条都补上了。**
#   · 它跑在 `src/frontend/shell` 上（`--all` = 那个 workspace 的全部成员）；
#     `src/backend` 是**另一个 workspace**，本行盖不到它。
#     🔴 **`K-R80`（09-12）：那句话一个字没改，改的是它后面缺的那一格** ——
#     那棵树今天由下面 `fmt-backend` 那一行盖。**别再把这一句读成处置。**
run_gate fmt '不是数出来的数：`cargo fmt --all --check` 只有绿/红两态（rc=0 / rc=1），本格的「分母」是 `src/frontend/shell` 那个 workspace 的全部成员；`src/backend` 是另一个 workspace，本行盖不到（那一棵由下面 fmt-backend 那一格盖）' \
         bash -c 'cd src/frontend/shell && cargo fmt --all --check 2>&1 && echo "fmt: 1 passed"'

# ── backend 那棵树的格式漂移（`K-R80` 09-12）──────────────────────────────────
#
# 🔴 **本格买的是上面那句诚实注释的处置。**
#
# ## 病不是「没人知道」，是「知道了而没人补」
#
# 上面那一格的分母里逐字写着「`src/backend` 是另一个 workspace，本行盖不到」，
# 而那句话**每趟门禁都印在终端上** —— 它不是静默失效，是**一格「我盖不到那儿」的注释
# 被当成了处置**。`K-R79` 交回时报出：backend 那棵树 `cargo fmt --check` **在基点上就是红的**，
# PM 现打复核 **6 处 / 3 文件**（`agents/mod.rs` 1 · `control/ccm/argv.rs` 4 · `protocol_doc_guard.rs` 1）。
# ⇒ **说清了射程 ≠ 射程够。** 本仓反复抓这一形，这一次长在门禁自己身上。
#
# ## 为什么是**多一格**，不是**并成一棵**
#
# 把 `src/backend` 塞进 `src/frontend/shell` 那个 workspace 就能「顺便盖到」——
# **不许**。`K25` 裁的是「一份代码、每平台一份原生二进制」，而那棵树的 standalone
# 是**真架构约束**（它自己的 `Cargo.toml` 头注逐字：一个 workspace 会把这个 Linux-only 的
# backend 拖进 Windows CI 的 `cargo test --all`）。为一格排版去动两棵树的依赖关系，
# **代价远大于本格**。⇒ 多一行，各跑各的。
#
# ## 🔴 为什么是 `cargo fmt --check` 而**不是** `cargo fmt --all --check`
#
# **这一条是现打出来的，别顺手加 `--all` 去「对齐上面那一格」**（`K-R80` 09-12，
# 沙箱 `ccmon-devbox:latest`，`cargo fmt --all --check -v` 读它真喂给 rustfmt 的那串文件）：
# 在 `src/backend` 下加 `--all`，rustfmt 实收 **12 个 crate 根**，其中 **11 个不在这棵树里** ——
# `src/frontend/shell/build.rs` · `src/frontend/shell/src/lib.rs` · `src/frontend/shell/src/main.rs` ·
# `crates/{acct,branch,codex-token,creds,gate,guard,shell-quote}-core/src/lib.rs`，
# 以及当时还在的那份 vendored 第三方引擎的 `lib.rs`（今天整棵删了）。
#（成因：那棵树的 path 依赖指进 `../../src/frontend/shell`，`cargo fmt --all` 顺着它们走出去；
#  `cargo metadata --no-deps` 的 `workspace_members` 现打**只有 1 个**，两者不是一回事。）
# ⇒ 加 `--all` 会把别的树拉进这一格（那些树各有自己那一格管排版）。
# ⚠ 不加 `--all` 时 `cargo metadata` 那 11 个一个都不进来（同一趟 `-v` 现打：rustfmt 只收
#   `src/backend/main.rs` 一个根），读数 6 处不变 ⇒ **少的只有别人家那棵树。**
#
# ## ⚠ 诚实边界，别读宽
#   · 它买的是「**排版与 rustfmt 一致**」，**买不到**「代码对」——与上面那一格同一句话。
#   · 分母是**一个包** `cc-monitor-backend`，射程 = 从 `src/main.rs` 顺 `mod` 走得到的那些文件；
#     那棵树里**走不到的 `.rs` 文件本格看不见**（今天没有这样的文件，但那是事实不是判据）。
#   · `.github/workflows/ci.yml` 的 `backend` job **早就有这一步**（逐字同一条命令
#     `cargo fmt --check`，`working-directory: src/backend`）⇒ 本行**不是新买一条判据**，
#     是把「本机门禁不是云端的超集」这个已知缺口在这一维上补平。⚠ 因此 `ci.yml` **不用改**，
#     上面那条「三处一起改」的纪律与本行无关。
run_gate fmt-backend '不是数出来的数：`cargo fmt --check` 只有绿/红两态（rc=0 / rc=1），本格的「分母」是 `src/backend` 那个 workspace 的唯一成员 `cc-monitor-backend`；`src/frontend/shell` 由上面 fmt 那一格管，本行盖不到（刻意不加 --all，理由见上方注释）' \
         bash -c 'cd src/backend && cargo fmt --check 2>&1 && echo "fmt-backend: 1 passed"'

# ── Windows 那半编不编得过 ──────────────────────────────────────────────────
#
# 🔴 **本文件头注那条「归 PM」的第 ① —— 09-10 买到了。**
#
# ## 它买的是这个项目今年最贵的那一课
#
# cc-monitor v1 是 **Windows 专供**，而 **Windows 上编不过这件事在 08-13 到 09-09 之间
# 没有任何人发现**（云端 CI 自 08-05 起红在第一步，后面全部 `skipped`）——
# 09-09 那一趟修出来 **17 个互不相同的编译错地址**。
# 本机门禁跑在 Linux 上，那 `#[cfg(windows)]` 的 **67 处**（`src/frontend/shell/src`，现打 09-10）
# **根本不参与编译** ⇒ 它一次都没看见。
#
# ## 🔴 铁律 12 的刀（**这一格不是推的，是切出来的**）
#
# 在 `config.rs::atomic_replace`（`#[cfg(windows)]`）里放一行 `let _: u32 = "…";`：
#   · `cargo check -p monitor`（Linux 原生，= 门禁其余各格看得见的那一面）⇒ **退出码 0，全绿**
#   · `cargo check -p monitor --target x86_64-pc-windows-gnu`         ⇒ **退出码 101**，
#     并逐字点名 `error[E0308]: mismatched types` 在哪一行
# **两侧读数相反 —— 那正是这一格存在的全部理由。**
#
# ## 为什么是 `-gnu` 而不是 `-msvc`
#
# `-msvc` 扩不到 monitor 本体：`ring` · `libsqlite3-sys` · 四个 `tree-sitter-*` 都用 `cc-rs`
# 编 C，而它在 msvc target 上找 `lib.exe` ⇒ Linux 上没有（实测
# `error occurred in cc-rs: failed to find tool "lib.exe"`）。`-gnu` 走 mingw-w64，编得过。
# ⚠ **`-gnu` 是不是忠实代理，是量过的**：两者唯一的分歧点是 `target_env`，
# 而**全仓 `target_env` 命中 0 处** ⇒ 那 86 处一处都分辨不出这两者。
#
# ⚠ **本文件头注原本提议的窄买法（`-p creds-core --features harden --target …msvc`）
#   只盖 86 处里的 2 处（2.3%）** —— 它跑得通（实测 5.65s，与那条注释预测的 5.67s 对得上），
#   但 09-09 那 17 个编译错**全在 monitor 本体那 67 处里，它一个都逮不住**。⇒ 换成本行。
#
# ⚠⚠ **诚实边界，写死别读宽**：`cargo check` 买的是「**编得过**」——
#   **买不到「行为对」**（要真 Windows 机），**也买不到「MSVC 上链接得起来」**（`check` 不链接）。
#   真机行为那一格今天仍然是**判不了**，不是「通过」。
#
# ⚠ 依赖沙箱镜像装了 `mingw-w64` 与 `x86_64-pc-windows-gnu`（`.claude/devbox/Dockerfile`，
#   仓外、不进版本控制）。没装的机器上这一格会红在「找不到 target」——**那是对的**：
#   fail-closed 比静默跳过好。
# ⚠⚠ **`--all-targets` 是补的，它把本格的射程从「生产段」扩到「生产段 ＋ test 档」。**
#
# 问题（漏洞 2）：「**`winchk` 少 `--all-targets`，而兄弟格 `winchk-backend` 有**，
# 并注明『云端那 10 个错**全在 test 档**，所以 `--all-targets` 是**承重的**』
# ⇒ **同一个性质两把不同长度的尺子**。修它只要一个词。」
#
# ⇒ 本行加上之后，两格量的是**同一件事的同一个面**，只是包不同：
#   · `winchk`        = `src/frontend/shell` 的 `-p monitor` 一个包，生产段 ＋ test 档
#   · `winchk-backend` = `src/backend` 一个 crate，生产段 ＋ test 档
#
# ⚠ **它买不到的仍然一个字没变**（别因为射程变长就把这句读松）：
#   · 买的是「**编得过**」，**不是「行为对」**——那要一台真 Windows（`G2a`）。
#   · 本格是 `-gnu`，**MSVC ABI 专属的那一类照旧盖不到**（`check` 不链接，且沙箱里没有 zig）。
#   · **包**这一维没变：8 个共享 crate 仍然只有 `-p monitor` 依赖图里的那几个被顺带 check 到，
#     `creds-core` 的 `--features harden` 那 2 处**本行还是盖不到**（漏洞 4 还欠着）。
# ⚠ `--locked` 照旧带着：本格同时是 `src/frontend/shell/Cargo.toml ↔ Cargo.lock` 那条对账的落点
#   （`doc_claim_registry` 两处逐字点名「门禁 `winchk` 那一格的 `cargo check --locked`」）。
#   与 `winchk-backend` 刻意不带 `--locked` 的差别是**另一维**，别顺手抹平。
run_gate winchk '不是数出来的数：`cargo check --all-targets --target x86_64-pc-windows-gnu` 只有绿/红两态。射程 = `-p monitor` 与文件窗口包 `-p cc-monitor-filewin`（搬家前它的代码与判据都在 monitor 包里，射程不缩）两个包的**生产段 ＋ test 档**（`src/frontend/shell/src` 的 67 处 `cfg(windows)`；`--all-targets` 是 `A5` 补的，与兄弟格 `winchk-backend` 对齐 —— 那一格的读数逐字「云端那 10 个错全在 test 档」）；`src/backend` 那 17 处与 `creds-core` 那 2 处本行盖不到' \
         bash -c 'cd src/frontend/shell && cargo check --locked --all-targets -p monitor -p cc-monitor-filewin --target x86_64-pc-windows-gnu 2>&1 && echo "winchk: 1 passed"'

# ── `winchk-backend`：**backend 那棵树在 Windows 上编不编得过**（`K-R122` `KR122D2` 甲，09-14，第 18 格）──
#
# ## 问题：上面那一行自己写着「盖不到」，而那句话 09-14 兑现成了发版被拦
#
# 上面 `winchk` 那一行的分母逐字写着「`src/backend` 那 17 处与 `creds-core` 那 2 处
# 本行盖不到」。`K-R119` 推 `v3.8.0` 那一趟，云端 `Remote backend (Linux) lint + test`
# 那个 job 正是红在它的第 7 步（`cargo check --all-targets --target x86_64-pc-windows-msvc`）：
# **10 个编译错，全在 test 档**。⇒ **射程印在那一行上，而没有任何东西替它出声。**
#
# ## 🔴 与 CI 那一格的差别，逐条写清（别把本格读成「和 CI 一样」）
#
#   · **target 不同**：CI 用 `x86_64-pc-windows-msvc`，本格用 `x86_64-pc-windows-gnu`。
#     现打（09-14，本沙箱镜像）：msvc 那条在这里跑不了 —— `ring` 的 build script 要编 C，
#     cc-rs 找不到 `lib.exe` ⇒ rc=101，而它**根本走不到我们自己的代码**
#     （CI 那边是靠额外装一个 `zig` 把 `lib.exe` 这一环补上的，沙箱镜像里没有 zig，
#     而 `.claude/devbox/Dockerfile` 不在本件写区）。
#   · **两个 target 对本族缺陷等价**：那 10 个错全是「这个名字在 Windows 上不存在」
#     （`std::os::unix` / `libc::utimensat` / `libc::AT_FDCWD` / `Permissions::from_mode`）——
#     那是 `cfg(unix)` 这一维，与 ABI 无关。**现打验过**：同一份未修的源码在本格这条
#     `-gnu` 命令下逐字报 `due to 10 previous errors`、`(bin "cc-monitor-backend" test)`，
#     与 CI 那趟 msvc 的读数**同数同档**。
#   · ⚠ **它买不到 MSVC ABI 专属的那一类** —— 只在 msvc 上才犯的毛病（C 依赖的链接面、
#     MSVC 特有的 `#[link]`）本格盖不到。**那一格仍然只有 CI 有。**
#
# ⚠ **`--all-targets` 是承重的，别「简化」掉**：那 10 个错**一个都不在生产段**
#   （同一份源码在 `build-windows` 里原生编出过 `cc-monitor-backend.exe`）。
#   不加这个 flag，本格会在这一族缺陷上**全绿**。
# ⚠ 依赖沙箱镜像装了 `x86_64-pc-windows-gnu` 这个 target（现打在；`.claude/devbox/Dockerfile`
#   仓外、不进版本控制）。没装的机器上本格红在「找不到 target」—— fail-closed，那是对的。
# ⚠ 刻意**不带** `--locked`：CI 那一步也没带（`src/backend` 的锁文件由它自己的
#   `cargo test` 那一步管）。一个性质两把尺子是本区最贵那族病。
run_gate winchk-backend '不是数出来的数：`cargo check --all-targets --target x86_64-pc-windows-gnu` 只有绿/红两态。射程 = `src/backend` 这一个 crate 的**生产段 ＋ test 档**（云端那 10 个错全在 test 档，所以 `--all-targets` 是承重的）。⚠ 本格用的是 `-gnu`，云端用的是 `-msvc`（沙箱里没有 zig，`ring` 的 build script 缺 `lib.exe`）⇒ **MSVC ABI 专属的那一类本行盖不到**；`src/frontend/shell` 那棵树由上面 winchk 那一格盖' \
         bash -c 'cd src/backend && cargo check --all-targets --target x86_64-pc-windows-gnu 2>&1 && echo "winchk-backend: 1 passed"'

# ── `winlink`：**monitor 在 Windows 上链不链得起来**（第 30 格）──
#
# 守的要求：Win11 虚拟机上现打出来的缺陷 ——
# 「`-gnu` 交叉编 monitor **两个 profile 都链不过**：`monitor_lib.dll`（`[lib] crate-type`
# 里的 `cdylib`）导出序号超 65535（release 125 946 / dev 241 784）。门禁 `winchk` 只 `cargo check`，看不见」。
# ⇒ 上面 `winchk` 那一格的分母逐字写着「买不到『链接得起来』（`check` 不链接）」—— 这一格补的就是那半。
# 设计要求：「**本机 Windows**（x86_64） | ✅ **承诺** | 独立进程那个壳要有字节、要进门禁」。
#   以及「门禁补一格真链接、桌面不需要的 `cdylib` 收掉（4D WIN1）」。
#
# ## 它买什么 / 不买什么
#   · 买：`-p monitor` 的**两个二进制**（`cc-monitor` 主程序 ＋ `cc-monitor-filewin` 文件窗口）在
#     `x86_64-pc-windows-gnu` 上**真走一趟链接器**、链得出 `.exe`。`[lib]` 那一格收成 `rlib` 之后
#     不再产 dll（WIN1：全仓没有移动端，`cdylib` / `staticlib` 两格零消费者）；有人把 `cdylib` 加回来
#     ⇒ 这里当场红在 `export ordinal too large`（死值验住）。
#   · ⚠ 只链、不跑：「链出来的 exe 在 Windows 上起得来」要真机（`RT1.md` 那台虚拟机），本格判不了。
#   · ⚠ `-gnu` 不是 `-msvc`：发版那一格是 `windows-latest` 原生构建，MSVC 链接器那一类本格盖不到。
#   · ⚠ dev profile；release 那一档的链接本格不跑（`RT1-build-win.py` 走 release，它不在门禁上）。
#   · ⚠ 内嵌的本机后端字节此时**不在**（`native-backend/` 没铺）⇒ 链进去的是「没带后端」那一形，
#     与 `winchk` 同一形；带字节的那一形要 `RT1-build-win.py`。
run_gate winlink '不是数出来的数：`cargo build --bins --target x86_64-pc-windows-gnu`（dev）只有绿/红两态。射程 = `-p monitor` 的两个二进制（`cc-monitor` · `cc-monitor-filewin`）**真链接**一趟；⚠ 只链不跑（起不起得来要真机）· `-gnu` 不是 `-msvc` · release 那一档不链 · test 档不链（那一半归 `winchk` 的 `check`）' \
         bash -c 'cd src/frontend/shell && cargo build --locked -p monitor --bins --target x86_64-pc-windows-gnu 2>&1 && echo "winlink: 1 passed"'

# 该跑到的包 = `src/frontend/shell` workspace 的成员（`cargo metadata` 现取），与输出里真跑到的两向相等。
run_gate_sum cargo gate_shell_libs bash -c 'cd src/frontend/shell && cargo test --workspace --lib 2>&1'

# ★★ `K-G3`（09-01）：上面那个合计**还缺一个分母** —— `src/frontend/shell/embedded-backends/` 铺没铺。
#
# `build.rs:376` 只有在 `src/frontend/shell/embedded-backends/` 里两个 arch 的二进制**都在且 build_id 对得上**
# 时才 `println!("cargo:rustc-cfg=embedded_backends")`；那个目录被 `.gitignore` 挡着
# ⇒ **它跟着「铺没铺」走，不跟着 git 走**。挂 `#[cfg(embedded_backends)]` 的那一族全是
# 「本地后端真的能起来吗」：`sftp::embedded_backend_binaries_present_and_valid` ·
# `local_backend_host::the_local_backend_host_can_be_stopped_and_started_again` ·
# `local_backend::the_local_tmux_frames_really_land_in_the_ledger`〔散文墓碑〕（随 monitor 那本 tmux 原文账删了） ·
# `local_backend::the_local_backend_host_really_registers_an_inbound_client`。
#
# 病灶逐字（风险行 `5t`，PM 08-25 实测撞上、08-29 复打）：
# **「没有任何东西报出『这一跑少编了几条』」** —— 少编与「都跑了」在终端上一模一样，
# 因为那个合计只会**变小**，而变小没有任何东西认得出来。
#
# ⚠ **这一行只自报家门，不是判据**，理由是判不了：地板得是个常数，而同一份代码
#   铺了与没铺**本来就该是两个数**，钉死任何一个都会把另一种铺法误判成红。
#   真要买成判据得先有一张「铺法 ⇒ 应有条数」的映射，那张表今天盘上没有 ⇒ 交回 PM。
# ⚠ 行首刻意**不是** `ok` —— 它不判任何东西，写成 `ok` 就是把一条诊断伪装成一格绿。
if [ -d src/frontend/shell/embedded-backends ]; then
  printf '  分母 %-14s %s\n' "cargo" "本树铺了 src/frontend/shell/embedded-backends/ ⇒ embedded_backends cfg 会置上，「本地后端真的能起来吗」那一族在跑"
else
  printf '  分母 %-14s %s\n' "cargo" "本树未铺 src/frontend/shell/embedded-backends/ ⇒ embedded_backends cfg 不置 ⇒ 上面那个合计里少了「本地后端真的能起来吗」那一族（4 条，逐个点名见上方注释）"
fi

# ★ 生成物漂移（K-A1 第四轮 `R1`）：**改了 Rust 不跑生成，这里红。**
#
# 形状照 `.github/workflows/ci.yml` 那条「生成物必须最新（C05）」来 —— 它逐字是
# `git diff --exit-code -- ../../src/frontend/ui/generated`（那一步在 `src/frontend/shell` 目录下跑，所以带 `../`；
# 本脚本开头已经 `cd` 到仓根，所以不带），失败时印一句 `::error::` 提示「请跑
# npm run gen:types 并把 src/frontend/ui/generated/ 一起提交」再 `git diff --stat`。
# ⚠ 那条 CI 步骤的头注还写明了它**排除了什么**：它只买「已提交的生成物 == 从 Rust 源生成的」
# 这一半，另一半「TS 消费方 == 已提交的生成物」由 frontend job 的 `tsc` 买 —— 拆成两半的理由是
# **没有任何 job 同时有 Rust 和 node**（给 Rust job 加 `npm ci` 是分钟级，给 frontend job 加
# 整套 Tauri 编译是 CI 里最贵的东西）。本脚本两样都有，所以这一半在这里只值一条 git 命令。
#
# ⚠⚠ **位置是承重的：它必须排在上面那道 `cargo` 门之后。**
# `ts-rs` 的导出测试就住 `cargo test --lib` 里（`package.json` 的 `gen:types` 逐字就是
# `cd src/frontend/shell && cargo test --lib export_bindings`）⇒ 跑过那道门，`src/frontend/ui/generated/**` 已经被
# 按当前 Rust 源重写了一遍，这里的 `git diff` 才是「Rust 源 与 已提交版本」的差。
# 排在它**之前** ⇒ 检查的是一棵还没被重写的树，**恒绿 = 假绿**。
#
# 立项理由（K-A1 D 阶段审计实测：往 `RemoteAccount` 加一个字段而**不**跑生成，四条读数）：
#   · vitest 全量（含 `generated-boundary-guard` 那一族）**1467 全绿**
#   · `npx tsc --noEmit` **0 错**
#   · `cargo test --lib` **自己把 `src/frontend/ui/generated/RemoteAccount.ts` 重写了、然后报
#     `1181 passed; 0 failed`（绿）** ⇒ 本脚本原来那四道门**结构上一条都抓不到**
#   · 只有 CI 那条抓得到。
# 而本脚本头注自称「出货前的**唯一闸门**」—— 补上这一句才对得起那句话。
# ⚠ 顺带订正一句写在别处的假话：件计划 `KAY1③` 曾写「生成物一致性由
# `generated-boundary-guard` 那族 + `npm run gate` 守，改 Rust 不跑生成就红」——
# **在本行落地之前，后半个主语是假的**（订正记在件计划 `§1 KAY1③`）。
#
# ⚠ 射程如实写（它**抓不到**什么，三条）：
#   1. 它判**已跟踪文件的 diff** ⇒ 一个**全新**的生成物文件是 untracked，`git diff` 看不见。
#      那一格由 `tests/frontend/ui/generated-boundary-guard.vitest.ts` 的目录清单**逐项等号对拍**钉住
#      （它对 `src/frontend/ui/generated/` 做 `readdirSync` + 等号比对，新增文件必然让它红一次）。
#   2. 它不判生成物**内容对不对**（该不该 `ts(optional)` 之类）—— 那也是上面那一族的活。
#   3. 它判的是**工作树**，不判「你有没有真把它 commit 上去」（那一维归 `npm run verify:committed`，
#      与本脚本头注里那条分工一致）。
# ⚠ 本格**不走** `run_gate`（判定手写在下面那个 `case` 里）⇒ `gate_wants`/`GATE_RAN`
#   也得手接一次。漏接的形状是：`GATE_ONLY` 点不到它、而它照样跑，
#   于是收据里 `ran ∪ skipped` 少一格 ⇒ `K-G4C` 的两向相等当场分叉（那是**响的**）。
if gate_wants generated; then
gen_t0="$(gate_now_ms)"
git diff --quiet --exit-code -- src/frontend/ui/generated/
gen_rc=$?
gate_ran generated "$gen_t0"
case "$gen_rc" in
  0) printf '  ok   %-14s %s\n' "generated" "与 Rust 源一致（跑过上面那道 cargo 门之后再判的；$(gate_took generated)）" ;;
  1)
    printf '  FAIL %-14s %s\n' "generated" "src/frontend/ui/generated/ 与 Rust 源不一致："
    git diff --stat -- src/frontend/ui/generated/
    fails+=("generated（改了带 ts_rs::TS 的类型 ⇒ 跑 npm run gen:types 并把 src/frontend/ui/generated/ 一起提交）")
    ;;
  *)
    # 退出码既不是 0 也不是 1（如 128：不在 git 仓里）⇒ **判不了**。不许当成绿。
    fails+=("generated（git diff 退出码 $gen_rc —— 判不了，不许当成绿）")
    ;;
esac
fi

# ── `deadcode`：monitor 非 test 构建里的死代码，**零容忍** ───────────────────────────────────
#
# 门禁的 `cargo test` 看不见它（test 构建里那些项有调用方：测试自己）⇒ 单开一趟 `cargo check -p monitor`。
# 判：这一趟 rustc 报的 `dead_code` 诊断（`never used` · `never constructed` · `never read` 都算）一条都不许有；
#   有就逐条列出来（文件:行 ＋ 原话）。原先钉的是「`never used` 恰好 8 条」—— 一个手抄的数，
#   漏数 `never constructed`，而且每路合并都要回来改它。
# 只在别的平台上才有调用方的项，用 `#[cfg(…)]` 把它和调用方放在同一个平台上，别留着让这一格记它。
# 反空真：JSON 里必须有 `monitor_lib` 那一个 artifact（这一趟真走到了 monitor；缓存命中时 cargo 照样重放警告）。
# ⚠ 射程：`-p monitor` 这一趟编到的包（含它的 path 依赖）的非 test 段；`src/backend` 与 `#[cfg(test)]` 里的盖不到。
gate_deadcode() {
  local out rc
  out="$(cd src/frontend/shell && cargo check -p monitor --message-format=json 2>&1)"; rc=$?
  printf '%s\n' "$out" | python3 -c '
import json, sys
rc = int(sys.argv[1])
seen_lib, dead, errs = False, [], []
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("target", {}).get("name") == "monitor_lib":
        seen_lib = True
    if m.get("reason") != "compiler-message":
        continue
    msg = m["message"]
    if msg.get("level") == "error":
        errs.append(msg.get("rendered") or msg["message"])
    if (msg.get("code") or {}).get("code") == "dead_code":
        sp = [x for x in msg["spans"] if x.get("is_primary")] or msg["spans"] or [{}]
        dead.append("%s:%s  %s" % (sp[0].get("file_name", "?"), sp[0].get("line_start", "?"), msg["message"]))
if rc != 0:
    print("".join(errs)[-4000:])
    print("deadcode: cargo check 退出码 %d —— 判不了" % rc)
    sys.exit(rc)
if not seen_lib:
    print("deadcode: 输出里没有 monitor_lib 的 artifact —— 这一趟没走到 monitor，「零条死代码」不算数")
    sys.exit(1)
if dead:
    for d in sorted(set(dead)):
        print("  dead_code  " + d)
    print("deadcode: 非 test 构建里有 %d 条死代码（上面逐条）—— 删掉，或用 #[cfg(…)] 收到它真有调用方的那个平台" % len(set(dead)))
    sys.exit(1)
print("deadcode: 1 passed（monitor 非 test 构建零条 dead_code）")
' "$rc"
}
run_gate deadcode '不是数出来的数：`cargo check -p monitor`（非 test）的 `dead_code` 诊断零条才绿，有就逐条列出来' \
         gate_deadcode

# ── `clippy` / `appbuild`：`ci.yml` 那两步此前只在 CI 上跑 ─────────────────────────────
# `rust` job 的 `cargo clippy --workspace --all-targets`（不带 `-D warnings`，但 clippy 默认 deny 的那几类
# lint 与编译错照样红）· `linux-app-build` job 的 `cargo build`（真编 bin 并链接；上面 `cargo` 那格带 `--lib`、
# `deadcode` 那格只 check，两格都不链 Linux 上的那两个二进制）。
# ⚠ 三格成功时只印输出尾部两行：警告片段会原样带出源码里的散文（如某句注释里的「1277 passed」），
#   而 `run_gate` 取输出里最大的那个数 ⇒ 不滤就会把散文读成读数。红的时候整份输出照印。
run_gate clippy '不是数出来的数：`cargo clippy --workspace --all-targets` 只有绿/红两态，分母是 `src/frontend/shell` 那个 workspace 的全部成员与全部 target（含 test 档）。⚠ 与 CI 那一步同一条命令、不加 `-D warnings` ⇒ 警告不红，只有 deny 档的 lint 与编译错红；CI 那一步跑在 windows-latest 上，本格跑在 Linux 上（Windows 那一维由 `winchk` 盖）。本格墙钟〔量于 2026-09-30，本工作树〕首趟 73 秒、源码没变时 4 秒' \
         bash -c 'cd src/frontend/shell && out=$(cargo clippy --workspace --all-targets 2>&1); rc=$?; if [ "$rc" -ne 0 ]; then printf "%s\n" "$out"; exit "$rc"; fi; printf "%s\n" "$out" | tail -2; echo "clippy: 1 passed"'
run_gate appbuild '不是数出来的数：`cargo build`（dev）只有绿/红两态，射程 = `src/frontend/shell` 根包 `monitor` 的 lib 与两个二进制（`cc-monitor` · `cc-monitor-filewin`）在 Linux 上**真编真链**一趟，与 `ci.yml` 的 `linux-app-build` 那一步同一条命令。⚠ 只链不跑；release 档不编；前端产物（`dist/`）由 `npm` 那格里的真 vite 构建与 `tsc` 那格盖。本格墙钟〔量于 2026-09-30，本工作树〕首趟（依赖全量编译）106 秒' \
         bash -c 'cd src/frontend/shell && out=$(cargo build 2>&1); rc=$?; if [ "$rc" -ne 0 ]; then printf "%s\n" "$out"; exit "$rc"; fi; printf "%s\n" "$out" | tail -2; echo "appbuild: 1 passed"'

run_gate backend '单包 src/backend，只有一行 test result ⇒ 最大值 = 合计' \
         bash -c 'cd src/backend && cargo test 2>&1'
run_gate clippy-backend '不是数出来的数：`cargo clippy --all-targets` 只有绿/红两态，射程 = `src/backend` 那一个 crate 的全部 target，与 `ci.yml` 的 `backend` job 那一步同一条命令（不带 `-D warnings` ⇒ 只有 deny 档的 lint 与编译错红）。本格墙钟〔量于 2026-09-30，本工作树〕首趟 32 秒' \
         bash -c 'cd src/backend && out=$(cargo clippy --all-targets 2>&1); rc=$?; if [ "$rc" -ne 0 ]; then printf "%s\n" "$out"; exit "$rc"; fi; printf "%s\n" "$out" | tail -2; echo "clippy-backend: 1 passed"'
# ── `tsc`：**发版产物编不编得出来**，此前门禁一格都没有（`K-R118` `KR118D1` ②，09-14，第 16 格）──
#
# ## 问题：一条缺陷 09-12 进来、09-14 才被发现，而发现它的不是任何判据
#
# `tauri build` 的第一步是 `npm run build` ＝ `tsc && vite build`。09-14 `K-R114` 去**真编一次
# 发版产物**，那一步在 `src/frontend/ui/views/history.ts` 上红了 6 条 `TS2322` —— 而同一棵树的门禁
# **15 格全绿**（现打，`tests/evidence/K-R118-deathvalue.md#§A` 的 `M0`）。
#
# 🔴 **两条路同时断，这一格补的是第一条**：
#   ① 门禁 `npm` 那一格跑的是 `npm test`（16 个 tsx 套件 + `vitest run`）—— **不含 `tsc`**。
#      `tsx` 与 `vitest` 都是**转译**执行，`esbuild` 只剥类型不做类型检查 ⇒
#      一条纯类型错误在那一格下**一条都不会红**。
#   ② 云端 `.github/workflows/ci.yml` 里那条 `npx tsc --noEmit` **只在 `main` / tag / PR 上跑**，
#      而本分支这一族提交一次都没进过 `origin/main`。
#   ⇒ 这与 `audit-0805` 的 `3w`/`3x`/`3y` 是同一族病：**判据在，执行面没有**（`R73` 第五节）。
#
# ⚠ **射程如实写**：本格只跑 `tsc --noEmit`，也就是 `npm run build` 的**前一半**。
#   `vite build` 那一半（打包 / 产物体积 / 资源解析）、`cargo tauri build` 那一整段
#   （签名 · 打包 · installer），本格**一概盖不到**。
# ⚠ **它不是 `npm` 那一格的超集，也不是子集**：`npm` 买行为（跑起来对不对），
#   本格买类型（编不编得过）。两格都要。
#
# ## 第二条判定：**程序面没被掏空**（这一条是承重的，别删）
#
# `tsc --noEmit` 在一个**空程序**上退出码是 **0** —— 把 `tsconfig.json` 的 `include` 改小 /
# 改错，「一个文件都没检」与「全检过了」在退出码上**一模一样**。
# ⇒ 本格把 `--listFiles` 真读进程序的那批文件数出来，与**盘上现打**的 `src/` ＋ `tests/e2e/` 下
#   `.ts`/`.tsx`/`.mts` 份数对账，**两个数在同一趟里现打**，一个都不写死
#   （写死一个数，加一份文件就红，那种格三天就会被人调宽）。
run_gate tsc '不是「几条断言过了」：这个数是**这一趟真读进 tsc 程序**的仓内 `.ts`/`.tsx`/`.mts` 份数（`tsconfig.json` 的 include 现打是 `[\"src\", \"tests\"]`），并与盘上现打的份数**恒等对账**。🔴 **本行原先两侧都只数 `src` ＋ `tests/e2e`（210 份），而 tsc 真读进去的是 372 份** —— 两侧同时把 `tests/` 的其余 **162** 份剔掉，于是等式照样成立、本格照样绿。⚠ **那不是少印一个数，是一个静默洞**：有人把 `include` 收窄成 `[\"src\", \"tests/e2e\"]`，那 162 份当场不再被检，而 `want` 与 `got` 会一起掉到 210 ⇒ **仍然相等、仍然全绿**。本拍把两侧都改成按 `include` 的真值数（372 == 372），这条路才堵上。⚠ 只判类型（`npm run build` 的前一半）；`vite build` 与 `cargo tauri build` 那两段、以及仓根那几份不在 include 里的 `.ts`（`vite.config.ts` / `vitest.config.ts`），本行一概盖不到' \
         bash -c 'out=$(node_modules/.bin/tsc --noEmit --listFiles 2>&1); rc=$?; \
want=$(find src tests -type f \( -name "*.ts" -o -name "*.tsx" -o -name "*.mts" \) | wc -l | tr -d " "); \
got=$(printf "%s\n" "$out" | grep -v "/node_modules/" | grep -cE "/(src|tests)/.*\.(ts|tsx|mts)$"); \
printf "tsc: 盘上现打 %s 份仓内 .ts，这一趟真读进程序的 %s 份\n" "$want" "$got"; \
printf "%s\n" "$out" | grep -E "error TS" | head -60; \
if [ "$rc" -ne 0 ]; then printf "tsc: 退出码 %s —— 类型没编过。它就是 npm run build 的第一步，红着这棵树发不出产物\n" "$rc"; exit "$rc"; fi; \
if [ "$got" -ne "$want" ]; then printf "tsc: 真读进程序的 %s 份 != 盘上现打的 %s 份 —— tsconfig 的 include 被掏空或收窄了。空程序上 tsc 退出码也是 0，「一个文件都没检」与「全检过了」在退出码上一模一样，所以一律按红记\n" "$got" "$want"; exit 1; fi; \
printf "tsc: %s passed（仓内 %s 份 .ts 全部过 tsc --noEmit；两个数同一趟现打）\n" "$got" "$want"'

run_gate npm '17 个套件（16 tsx + 1 vitest）里只有 2 个打得出数字（test:dom 1480 · test:diff 17），而取最大值 ⇒ 这个数恒是 test:dom 的；另 15 个 tsx 套件只打「all X tests passed」，它们「跑了 0 个」这一格守不住（失败仍由 && 链的退出码守）' \
         npm test

# ── `coverage` / `audit`：`ci.yml` 的 `frontend` job 里此前只在 CI 上跑的三步 ─────────────────────
# 覆盖率逐文件地板（`tests/scripts/assert-coverage-floors.mjs`）点名的是具体文件：那个文件被删或改名、
# 清单还点着它 ⇒ 只有 CI 红。两步都按步骤名从 `ci.yml` 现取原样跑，地板与清单只住它们自己的文件。
run_gate coverage '这一趟 vitest（带 v8 覆盖率）真跑过的条数：`ci.yml` 的 `coverage floor (vitest jsdom)`（`npm run coverage`，`vitest.config.ts` 里的全局阈值）＋ `coverage per-file floors + zero-coverage ratchet`（逐文件地板与零覆盖棘轮）两步原样跑。⚠ 与 `npm` 那格是同一批 vitest 文件再跑一遍（这一趟带插桩，慢一截）；覆盖率只量 `src/**/*.ts`，tsx 套件与 Rust 一概不进分母。本格墙钟〔量于 2026-09-30，本工作树〕约 40 秒' \
         gate_ci_steps coverage "coverage floor (vitest jsdom)" "coverage per-file floors + zero-coverage ratchet"
# ⚠ 要联网：它问的是 npm registry **当下**的漏洞库 —— 断网时退出码非零、本格红（不静默跳过）；
#   同一棵树也可能因为库里新登了一条 high 而隔夜变红，那与 CI 上同一步的行为一致。
run_gate audit '步数：`ci.yml` 的 `npm audit (production deps, high)` 一步原样跑（`--omit=dev --audit-level=high`），只有绿/红两态。⚠ 要联网、判的是 registry 当下的漏洞库；dev 依赖与 high 以下的档本行不看。本格墙钟〔量于 2026-09-30，本工作树〕约 1 秒' \
         gate_ci_steps audit "npm audit (production deps, high)"

# ── 真机 e2e：每一套一行 `run_e2e <套件>` ─────────────────────────────────────────
#
# 判法复用 `tests/e2e/assert-pass-floor.sh`：套件退出码 0 ＋ 收尾 `合计 PASS=<n> FAIL=0` ＋ `n > 0`。
# 每套断言几条**只住在套件自己的输出里** —— 这里不钉数，CI 也不钉（CI 的 e2e job 就是调本门禁）。
# 几路同时给同一套加断言时，不再在几处手抄的数上撞车；读数照样印在绿行上、记进收据。
# `GATE_ONLY` 里写套件短名（`ccm-cli`），或写 `e2e` 一次点名全部套件（CI 那个 job 用它，套件名单不抄第二份）。
# 收据与裁决行里的规范名带前缀（`ccm tests/e2e/<套件>`）。
run_e2e() {
  local suite="$1"
  gate_wants "$suite" e2e || return 0
  local out rc n t0
  t0="$(gate_now_ms)"
  out="$(bash tests/e2e/assert-pass-floor.sh "$suite" 2>&1)"
  rc=$?
  gate_ran "ccm tests/e2e/$suite" "$t0"
  n="$(printf '%s' "$out" | grep -oE '合计 PASS=[0-9]+' | grep -oE '[0-9]+' | tail -1)"
  if [ "$rc" -ne 0 ]; then
    fails+=("ccm tests/e2e/$suite（退出码 $rc；PASS=${n:-<抓不到>}。诊断原文见下）")
    gate_diag "tests/e2e/$suite" "$out"
  elif [ -z "$n" ]; then
    fails+=("ccm tests/e2e/$suite（退出码 0 但抓不到「合计 PASS=<n>」—— 门没跑与门跑了结果是空\
在终端上一模一样，判不了，不许当成绿）")
    gate_diag "tests/e2e/$suite" "$out"
  else
    printf '  ok   %-14s %-22s PASS=%s（%s）\n' "ccm e2e" "$suite" "$n" "$(gate_took "ccm tests/e2e/$suite")"
  fi
}

# 探针⑩：`run_e2e` 的「退出码非零 ⇒ 红」还在判。给判法脚本两个参数 ⇒ 它在参数校验那一步 `exit 2`
#   并把参数原样回显（不跑 npm）；参数里带着 `合计 PASS=7` ⇒ 读数抓得到，掏掉退出码那条就会落到绿行。
gate_selftest_e2e() {
  local GATE_PROBE=1   # 同 `gate_selftest`：探针不受 `GATE_ONLY` 影响
  local probe
  probe="$(run_e2e '自检⑩ 合计 PASS=7 NG2-PROBE-J' 2>&1)"
  gate_assert_judged 自检⑩ "$probe" NG2-PROBE-J "run_e2e 的「退出码非零 ⇒ 红」"
}
gate_selftest_e2e

# e2e 的被测对象是后端二进制 ⇒ 先编出来（`backend` 那格的 `cargo test` 只编 test 版，不产 `debug/cc-monitor-backend`）。
# 它不进判定面：编不出来时每一套各自 fail-closed 红并说清原因。
gate_e2e_wanted() {
  if [ -z "$GATE_ONLY" ]; then return 0; fi
  local tok
  for tok in $GATE_ONLY; do
    case "$tok" in e2e) return 0 ;; esac
    if grep -qE "^run_e2e $tok\$" "$0"; then return 0; fi
  done
  return 1
}
if gate_e2e_wanted; then
gate_prep_t0="$(gate_now_ms)"
( cd src/backend && cargo build --bin cc-monitor-backend >/dev/null 2>&1 ) || true
GATE_PREP_MS="$(( $(gate_now_ms) - gate_prep_t0 ))"
printf '  ·    %-14s %s\n' "e2e 前置" "build 后端二进制（下面那几套 e2e 的被测对象；$(gate_fmt_ms "$GATE_PREP_MS")）"
else
printf '  ·    %-14s %s\n' "e2e 前置" "跳过（GATE_ONLY 一套 e2e 都没点 ⇒ 不白编那一趟 cargo build）"
fi

run_e2e ccm-print-parity
run_e2e ccm-cli
run_e2e ccm-contract-parity
run_e2e backend-cc-bus
# `backend-gate2` · `local-backend` 按环境显式分支：版本门 / 没铺 `embedded-backends/` 的格记 SKIP 并说原因；
#   未登记的 SKIP 由套件自己判红（收尾 `[ "$skip" -eq 0 ] || exit 1`）。
run_e2e backend-gate2
run_e2e local-backend
run_e2e restart-frames
run_e2e restart
run_e2e backend-tmux-late-server
run_e2e backend-sessions-rewatch
# `p3t-local-tmux` 跑在 `bash -lic` 里、依赖本机登录 shell：套件自己的 fail-closed 前置罩住（解析到的不是本树那个二进制就 ABORT）。
run_e2e p3t-local-tmux
run_e2e resume-frames
run_e2e cc-spawn-uplift
run_e2e inbound-frames
run_e2e graylight-frames
run_e2e backend-fork
run_e2e tmux-target
run_e2e cc-bus-queue-drain
run_e2e resume

# ── `env-sandbox`：带着指向「真目录」的会话环境起一趟门禁，那个目录零写入、那几个口零监听 ─────────────
# 「真目录」是一棵临时的假家（`~/.cc-monitor` 的样子：日志目录 ＋ 一份哨兵 stderr.log ＋ 监听口令牌文件），
# 「真口」是现找的三个空闲口。带着指向它们的 `CCM_*` / `CLAUDE_CONFIG_DIR` / `ANTHROPIC_BASE_URL` / `TMUX`
# 起一趟内层门禁，只跑 `backend-tmux-late-server`（它起常驻那一形的后端：环境漏进去，后端就会去绑那几个口、
# 轮转那份日志 —— 死值验现打过）。判：内层门禁绿 ＋ 假家前后逐份（路径 · 大小 · mtime）相同 ＋ 跑的全程
# 那两个口上没有监听（`ss` 每 0.1 秒看一次 —— 活得比 0.1 秒短的监听它看不见，那一形由「零写入」接：
# 后端起来第一件事是接日志）。墙钟约 25 秒（大头是那一套 e2e 本身）。
gate_env_sandbox() {
  local d p1 p2 p3 before after out rc w lis
  command -v ss >/dev/null 2>&1 || { printf 'env-sandbox: 这台机器上没有 ss —— 看不了监听，判不了\n'; return 1; }
  d="$(mktemp -d "${TMPDIR:-/tmp}/gate-env-sandbox.XXXXXX")" || return 1
  mkdir -p "$d/cc/logs/backend" "$d/claude"
  printf 'sentinel\n' > "$d/cc/logs/backend/stderr.log"
  printf 'sentinel-token\n' > "$d/cc/listen.token"
  read -r p1 p2 p3 < <(python3 -c 'import socket
s = [socket.socket() for _ in range(3)]
for x in s: x.bind(("127.0.0.1", 0))
print(*[x.getsockname()[1] for x in s])')
  before="$(find "$d" -printf '%P %s %T@\n' | sort)"
  ( while :; do ss -ltnH 2>/dev/null | awk '{print $4}' | grep -E ":($p1|$p2|$p3)\$"; sleep 0.1; done ) > "$d.listen" 2>/dev/null &
  w=$!
  out="$(env CCM_LISTEN_PORT="$p1" CCM_LISTEN_TOKEN_FILE="$d/cc/listen.token" CCM_RELAY_PORT="$p2" \
             CCM_BACKEND_STDERR_LOG="$d/cc/logs/backend/stderr.log" CCM_HISTORY_METADATA="$d/cc/history.json" \
             CCM_APIKEY_CREDENTIALS="$d/cc/apikey.json" CLAUDE_CONFIG_DIR="$d/claude" \
             ANTHROPIC_BASE_URL="http://127.0.0.1:$p3" TMUX="$d/tmux.sock,1,0" \
             GATE_ONLY=backend-tmux-late-server GATE_RECEIPT="$d.receipt.json" \
             bash tests/scripts/gate.sh 2>&1)"; rc=$?
  kill "$w" 2>/dev/null; wait "$w" 2>/dev/null
  after="$(find "$d" -printf '%P %s %T@\n' | sort)"
  lis="$(sort -u "$d.listen" | tr '\n' ' ')"
  rm -rf -- "$d" "$d.listen" "$d.receipt.json"
  if [ "$before" != "$after" ] || [ -n "${lis// /}" ]; then
    diff <(printf '%s\n' "$before") <(printf '%s\n' "$after") | head -20
    printf 'env-sandbox: 会话环境漏进去了 —— 假家有写入（上面的 diff）、或这几个口上出现过监听 [%s]\n' "${lis% }"
    return 1
  fi
  if [ "$rc" -ne 0 ] || ! printf '%s\n' "$out" | grep -q '^GATE: PARTIAL'; then
    printf '%s\n' "$out" | tail -30
    printf 'env-sandbox: 内层门禁退出码 %s、没有 PARTIAL 裁决 —— 判不了\n' "$rc"
    return 1
  fi
  printf 'env-sandbox: 1 passed（假家零写入、三个口零监听；内层门禁绿）\n'
}
run_gate env-sandbox '不是数出来的数：带着指向假「真目录」的会话环境起一趟内层门禁（一套起常驻后端的 e2e），零写入 ＋ 零监听才绿' \
         gate_env_sandbox

# ── `weak-net`：弱网台架（建镜像 · 跑台架）──────────────────────────────────────────────────
# 台架全程在 docker 里自建网络造网况（宿主网卡不动）；要本机有 docker 与 `NET_ADMIN`。镜像已在就跳过建镜像。
# 判法同 `run_e2e`：退出码 0 ＋ `合计 PASS=<n> FAIL=0` ＋ `n > 0`（`tests/e2e/weak-net/assert-floor.sh`），不钉条数。
gate_weaknet() {
  local out rc
  bash tests/e2e/weak-net/build-image.sh || { printf 'weak-net: 建台架镜像失败\n'; return 1; }
  out="$(bash tests/e2e/weak-net/assert-floor.sh 2>&1)"; rc=$?
  printf '%s\n' "$out"
  [ "$rc" -eq 0 ] || return "$rc"
  printf 'weak-net: %s passed（台架跑完、FAIL=0）\n' "$(printf '%s' "$out" | grep -oE '合计 PASS=[0-9]+' | grep -oE '[0-9]+' | tail -1)"
}
run_gate weak-net '台架跑完且 FAIL=0（四维网况 ＋ SSH，改前/改后两个读数）。⚠ 要 docker；只量容器里自建网络上的网况，真远端、真 Windows 一概不在' \
         gate_weaknet

# ── 这里原先是第 26 格 `f3-copy`（秤 F3 两向：零流量复制的包计数对拍，三方对拍 ＋ 两向锚点）。
#   它量的那条池子命令与核心随浏览 / 复制离开 SFTP 一起退役（窗口的复制走后端 `files-copy`），
#   判据本体那份台架文件一起删了 ⇒ 本格退役，29 格 → 28 格。它的形状（三方对拍 ＋ 写死的 pin）
#   仍是下面 `comm-boundary` 那一格的取法，说明留在那一格里。

# ── `comm-boundary` · `test-tiers`：两族判据**还在不在** ─────────────────────────────────────
#
# 两族都挂在一个模块上（通信层那一族在 monitor 的 lib · 测试层分级在 `guard-core` 的 lib）：那一行 `mod` 被摘掉时
# 整族一起消失，而 `cargo` 那一格只会合计小一点 ——「摘掉了」与「全绿」在终端上分不开。
# 判：那份判据文件里**声明的** `#[test]` 名字集合 == 这一趟 cargo **真跑过**的名字集合（两向）。
# 两侧异源（一侧读源码、一侧读 cargo 的运行时输出）；条数不再钉在本文件里 —— 加/删一条判据不用回来改数。
# ⚠ 买的是「这一族没有静默消失 / 没有哪条被 `#[ignore]` 挡掉」，不买它们判得对（那由各条的头注负责）；
#   这些条同时算在 `cargo` 那一格的合计里。
gate_family() {
  local label="$1" file="$2" pkg="$3" filter="$4" out rc declared ran miss extra
  [ -r "$file" ] || { printf '%s: 判据本体 %s 盘上读不到 —— 住址改了就回来改本格\n' "$label" "$file"; return 1; }
  declared="$(awk '/^[[:space:]]*#\[(tokio::)?test\]/{t=1; next} t && /fn [A-Za-z0-9_]+/{match($0, /fn [A-Za-z0-9_]+/); print substr($0, RSTART+3, RLENGTH-3); t=0}' "$file" | sort -u)"
  out="$(cd src/frontend/shell && cargo test -p "$pkg" --lib "$filter" 2>&1)"; rc=$?
  if [ "$rc" -ne 0 ]; then printf '%s\n' "$out" | tail -40; printf '%s: cargo test 退出码 %s —— 判不了\n' "$label" "$rc"; return "$rc"; fi
  ran="$(printf '%s\n' "$out" | sed -nE "s/^test .*${filter}([A-Za-z0-9_]+) \.\.\. ok\$/\1/p" | sort -u)"
  if [ -z "$declared" ]; then printf '%s: %s 里一条 `#[test]` 都没认出来 —— 读法与源码对不上，判不了\n' "$label" "$file"; return 1; fi
  miss="$(comm -23 <(printf '%s\n' "$declared") <(printf '%s\n' "$ran") | tr '\n' ' ')"
  extra="$(comm -13 <(printf '%s\n' "$declared") <(printf '%s\n' "$ran") | tr '\n' ' ')"
  if [ -n "${miss// /}" ] || [ -n "${extra// /}" ]; then
    printf '%s\n' "$out" | tail -25
    printf '%s: 声明的判据与真跑过的对不上 —— 声明了没跑 [%s] · 跑了却没在 %s 里声明 [%s]\n' "$label" "${miss% }" "$file" "${extra% }"
    return 1
  fi
  printf '%s: %s passed（%s 里声明的 #[test] 名字集合 == cargo 真跑过的集合）\n' "$label" "$(printf '%s\n' "$ran" | grep -c .)" "$file"
}
run_gate comm-boundary '判过的条数 = 通信层那一族这一趟真跑过的条数；声明的名字集合 == 真跑过的集合' \
         gate_family comm-boundary tests/frontend/shell/comm_boundary_registry_tests.rs monitor comm_boundary_registry::tests::
run_gate test-tiers '判过的条数 = 测试层分级那一族这一趟真跑过的条数；声明的名字集合 == 真跑过的集合' \
         gate_family test-tiers tests/common/guard-core/test_tiers_tests.rs guard-core test_tiers::


# pb check 不打「passed」，单独判：它自己会打 `FAIL=<n> BROKEN=<n>`。
#
# ★★ `K-R10`（09-01）：**查哪个计划工作区，由调用方用环境变量 `PB_WS` 给** ——
#    在此之前这里硬写着 `../.claude/planned-build/control-parity`。
#
# 它骗过的人是现打的：`K-R7` 收官前后，**实现方与 PM 各被它误导过一次** ——
# 门禁打绿的那行说的是 `control-parity`，而当时在做的是 `backend-consolidation`。
# ⇒ 这是本仓最高频的那族病（**量具的作用域对不上事实**）长在门禁自己身上。
#
# ⚠ **为什么是「调用方给」而不是「从工作树推导」** —— 推导那条路 09-01 摸底否掉了，
#   理由不是它难写，是**它的失效面形状和今天这个 bug 一模一样但更隐蔽**：
#   推错时它会印出一个**看起来完全合理的工作区名**，而硬写的常量至少肉眼可查。
#   拿一个更难发现的同族 bug 去换一个已经被发现的，不划算。
#  （盘上也没有权威映射：`.dispatch.json` 只有 2/54 条能点出工作树，
#    而且那两条用了两个不同的 key 名、两种路径写法。）
#
# ⚠ **为什么是环境变量而不是加一个位置参**：`.claude/devbox/gate` 的 arg2 已经是 TAG，
#   再加 arg3 会让「两参写法」被静默吃成 TAG —— 又一个静默失效面。
#
# ★★ **fail-closed：不给 `PB_WS` 就红，不回落任何默认值。**
#   这条是承重的，别改成 `${PB_WS:-control-parity}` 之类「友好」的写法：
#   回落默认 = 把今天这个 bug 原样搬进 `:-` 右边，而且从此**连硬写的常量都看不见了**。
#   ⚠ 直接跑 `npm run gate`（不经沙箱）会因此红 —— **那是设计**：红线本来就写着
#   门禁一律走沙箱，那条路本就不该是绿的，让它红是把纪律变成闸。
#
# ★★ **绿的那一行必须自报家门（逐字带上 `PB_WS`）**，`fails` 那一支同样带。
#   理由逐字：**这个 bug 骗过两个人靠的不是数字错，是那行字里没有任何能让人发现
#   它在说别人的信息。** 一行不带名字的 `ok pb check FAIL=0`，不论 FAIL 是几都不算数。
#
# ⚠ 名字**给错**那一侧不用在这里再判：`pb.py` 今天就已经 fail-closed
#   （不存在的目录 rc=3 · 没 `features/` rc=2 · 空目录 rc=2，三种都试过）⇒
#   在这里补一层「目录存不存在」是仪式。本处只治**同一性**（查的是不是你那个），不治存在性。
# 〔墓碑 —— `pb check` 那一格整格退役〕
#
# 原来这里是一道「查 planned-build 工作区」的门：没给 `PB_WS` 就红，理由逐字是
# 「这道门查哪个计划工作区必须由调用方指定；不许回落默认值：硬写一个名字正是 `K-R10`
# 治的那个 bug」。那条**拒绝猜**的纪律本身没错，今天不成立的是它的**对象**：
#
#   ① 本仓**没有** `.claude/planned-build/` —— 今天的设计与排期走的是仓外的那一族文档；
#   ② 它调的 `~/.claude-accts/z/skills/planned-build/bin/pb.py` **今天不在盘上**
#      ⇒ 就算给了 `PB_WS`，这道门也跑不起来。
#
# ⇒ 它不是「红」，是**没有可判的对象**。而让一道门在没有对象时自己闭嘴（跳过/回落）
#   正是本仓反复记账的那种病 ⇒ 不加「没目录就跳过」的口子，**整格删掉**。
# ⚠ 若哪天本仓真用起 planned-build，复活它要连同 `tests/evidence/N-G2-verdict-md5.py`
#   那份登记一起回来（那份尺子把本格登记成一个「行内格」）。

# ── 〔裁决·射程〕`GATE: OK` 那一行**不对什么负责**（`K-R122` `KR122D2` 乙，09-14）──────
#
# ## 问题：那一行今天**不带射程**，而它不等于「CI 会绿」
#
# `K-R119`（09-14）：同一棵树上本门禁 **16 格全绿**，推 tag 那一趟云端 **8 个 job 里 5 个红**
#（读数住 `tests/evidence/K-R119-发版读数.md § 六`）。四条差异里三条落在门禁自己**逐格印出来**的
# 射程之外 —— 读数在那儿，而**没有人把它读成「所以这三件事没人管」**；
# 第四条更直接：`shellcheck` 当时 16 格里**一格都没有**。
# ⇒ 本件甲那一半已经把其中两条收成了格（`shellcheck` · `winchk-backend`）。
#   **剩下的这几条今天仍然买不到，所以要在裁决那一刻逐字说出来。**
#
# ## 形状：`键|说明`，而**键是有牙的那一半**
#
# 说明是给人读的散文；条数现算（`${#GATE_BLIND[@]}`），不写死。
#
# ⚠ **它买不到什么**：这张表是**黑名单**，列不全 —— 它保证的是「**列出来的这几条不会悄悄
#   变成一句没人守的散文**」，不是「射程之外只有这几条」。
# ⚠ 只在 `GATE: OK` 那一支印。`GATE: FAIL` 那一支本来就没有在声称什么，那里再印一遍只会
#   把真正要看的诊断顶下去。
GATE_BLIND=(
  "windows-runner|Windows runner 上才犯的那一族 —— 本门禁的 npm / tsc / e2e 全跑在 Linux 上，路径分隔符恒是 /。K-R119 那趟云端 vitest 的唯一一条红（1 failed / 1725 passed）就是这一形，本机在构造上红不了"
  "ci-job-shape|.github/workflows/*.yml 里那些 job 自己的形状 —— 装了哪条工具链、runner 是谁、缓存与 needs 怎么连、每一步的 if 条件。Linux 那几个 job 的命令就是调本脚本（GATE_ONLY），命令只住这里；但 job 的环境（apt 装了什么、runner 是谁）本门禁不判。release-gate 割走了 release.yml 的一部分（触发器、发布闸、产字节那条路、BUILD_ID 的抠法、工具链版本，见 K-R124），其余每一步（打包 · 校验和 · 上传清单 · artifact 传递）仍然没人看；那些切片买的也只是「盘上这份文本满足这几条」"
  "msvc-abi|MSVC ABI 专属的那一类跨平台编译问题 —— 两格 Windows 交叉检查用的都是 -gnu（沙箱里没有 zig，ring 的 build script 缺 lib.exe）。只在 msvc 上才犯的毛病本门禁盖不到"
  "did-ci-actually-run|云端那条流水线到底跑没跑、绿没绿 —— 本门禁一次 gh run view 都不做。GATE: OK 说的是这棵树在本机这几格上的样子，不是它在云端的样子。ci.yml 的触发器只有 push(main/v*) 与 pull_request，而本仓不推送 ⇒ 那几个 job 在 GitHub runner 上从未起过；tests/hooks/pre-push 同理"
)
gate_print_blind() {
  printf 'GATE: 射程 —— 上面那行只对它自己那几格负责；下面这 %s 件事**本门禁不看**：\n' "${#GATE_BLIND[@]}"
  local item
  for item in "${GATE_BLIND[@]}"; do
    printf '  不看  %-20s %s\n' "${item%%|*}" "${item#*|}"
  done
}

# 裁决行里的格名用短名（e2e 那批去掉 `ccm tests/e2e/` 前缀）；格数与名单都从 `GATE_RAN` 现算，不抄。
gate_short_names() {
  local n out=""
  for n in ${GATE_RAN[@]+"${GATE_RAN[@]}"}; do out="${out:+$out · }${n#ccm tests/e2e/}"; done
  printf '%s' "$out"
}
# 墙钟：这一趟合计 ＋ 最慢的几格（逐格的数在每一格的绿行上、也在收据的 `ms` 里）。
gate_print_timing() {
  local total=$(( $(gate_now_ms) - GATE_T0 )) n ms line=""
  while IFS=$'\t' read -r ms n; do
    line="${line:+$line · }$n $(gate_fmt_ms "$ms")"
  done < <(for n in ${GATE_RAN[@]+"${GATE_RAN[@]}"}; do printf '%s\t%s\n' "${GATE_MS[$n]:-0}" "${n#ccm tests/e2e/}"; done | sort -rn | head -8)
  printf 'GATE: 墙钟 —— 合计 %s（其中 e2e 前置 %s）；最慢的几格：%s\n' "$(gate_fmt_ms "$total")" "$(gate_fmt_ms "$GATE_PREP_MS")" "${line:-（一格都没跑）}"
}

echo
gate_print_timing
# ── 〔被谁调用〕拼错的 `GATE_ONLY` 是**红**，不是「少跑一格」 ───────────────────
gate_check_only
# ── 〔被谁调用〕落收据。**三种裁词都落** —— 判它的是 `tests/evidence/K-G4C-gate-receipt.py` ──
# ⚠ 裁词**先算再落**：`OK` 只在「一格没红 **且** 一格没跳」时给。
#   少了后半个条件，`GATE_ONLY` 一设就能拿到一行 `GATE: OK` —— 那正是本段要焊死的那一形。
if [ "${#fails[@]}" -ne 0 ]; then
  gate_verdict=FAIL
elif [ "${#GATE_SKIPPED[@]}" -ne 0 ]; then
  gate_verdict=PARTIAL
else
  gate_verdict=OK
fi
gate_write_receipt "$gate_verdict"

if [ "${#fails[@]}" -eq 0 ] && [ "${#GATE_SKIPPED[@]}" -ne 0 ]; then
  # 🔴 **这一支刻意不印 `GATE: OK`。** 跑过的那几格全绿是真的，而「全绿」与「全都跑过」
  #   是两句话 —— 把它们印成同一行，就是本仓反复治的那一形（少跑与跑过在终端上一模一样）。
  printf 'GATE: PARTIAL —— 跑过的那 %s 格全绿，但 GATE_ONLY 挡掉了 %s 格：%s\n' \
    "${#GATE_RAN[@]}" "${#GATE_SKIPPED[@]}" "${GATE_SKIPPED[*]}"
  echo "**这不是 GATE: OK，不许拿它出货。** 出货要的是不带 GATE_ONLY 的那一趟。"
  gate_print_blind
  exit 0
fi
if [ "${#fails[@]}" -eq 0 ]; then
  # 格数与点名从这一趟真判过的格现算 —— 原先是一句手抄的「N 格全绿（…）」，每加/删一格都要回来改，改漏过不止一次。
  printf 'GATE: OK —— %s 格全绿（%s），可以出货\n' "${#GATE_RAN[@]}" "$(gate_short_names)"
  gate_print_blind
  exit 0
fi
# ★ `K-G3`（09-01）：分隔符**不能**走 `IFS='；'` —— `IFS` 是按**字节**认的，
#   而 `；`（U+FF1B）是 3 个字节，`${fails[*]}` 只会拿它的**第一个字节**去拼
#   ⇒ 两格以上一起红时，裁决行里印出来的是一个坏字节（`�`），后面那几格的名字被它糊住。
#   现打：本拍的死值验 `C1` / `C2` 两刀各撞到一次（两格同红）。一格红时看不出来 ——
#   这正是「只在多失败那一支才发作」的形状，而没人会为了看分隔符去造两格同红。
#   ⇒ 自己拼，不借 `IFS`。
joined=""
for f in "${fails[@]}"; do joined="${joined:+$joined；}$f"; done
printf 'GATE: FAIL —— %s\n' "$joined"
echo "**别提交**。先修，再重跑本脚本。"
exit 1
