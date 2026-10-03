#!/usr/bin/env bash
# F05a：**本机后端进程「起与看住」的真进程验收**。
#
# 与单测的分工：单测断言「重启策略与路径解析怎么答」（纯函数）；
# **本脚本断言「监护器在真进程上到底干了什么」** —— 起真 `cc-monitor-backend`、把它杀掉、
# 看它自己回来；再喂一个必崩的二进制，看它在上限内被判死而不是无限自旋。
# 门禁只锁判定不锁行为是 R1 的教训（三门禁全绿仍放行过一个让 send-keys 完全失效的改动）。
#
# ★ 真进程那部分住在 Rust 侧的 `#[ignore]` 测试里（`supervise()` 的 API 在那儿），
#   本脚本负责三件 Rust 测试不该管的事：
#     ① **隔离** —— 私有 tmux（shim 强插 `-L`；被监护的后端一起来就往 tmux server
#        装三条全局 hook，**没有开关**；不隔离就是去改用户真实 tmux 的状态）；
#     ② 临时工作目录（必崩脚本、空目录探测都在里面造）；
#     ③ 断言计数与收尾格式（与全仓其余 18 套逐字一致）。
#
# ## ★★ `K-R7`（08-31）：**本套件多接了两条 —— 而它们此前是普通 `#[test]`**
#
# 〔用 08-29〕逐字：「**你只能做产品, 不能动机器**」·「**以后所有开发测试都不允许直接在本机跑**」。
# 在此之前，下面这两条起真后端的测试是**普通 `#[test]`**（只由 `cfg(embedded_backends)` 门着）：
#
#   · `local_backend_host::tests::the_local_backend_host_can_be_stopped_and_started_again`
#   · `local_backend::tests::the_local_backend_host_really_registers_an_inbound_client`
#
# ⇒ **任何人在铺了 `src/frontend/shell/embedded-backends/` 的树上跑一次 `cargo test`**（包括用户自己
# clone 下来跑一遍）**都会改这台机器的 tmux 全局状态**。已经真发生过三次
# （08-26 实现方 · 08-27 PM · 08-29 PM）。
# ⇒ 两条都改成 `#[ignore]` + `CCM_E2E_TMUX_SHIM_BIN` fail-closed，**并接到本脚本这条带 shim 的路上**。
# 人群那一侧由 `local_backend_host.rs` 的
# `every_test_that_starts_the_real_backend_demands_a_private_tmux` 守着（按「二进制哪来的」派生，
# 不看属性，两个文件一起扫）。
#
# ⚠ 那两条 + `the_local_tmux_frames_really_land_in_the_ledger`〔散文墓碑〕（已删，见下面 `EMB_TESTS`）都由 `cfg(embedded_backends)` 门着，
#   而 `embedded-backends/` 是 gitignore 的 ⇒ **干净 clone 与 CI 上它们不编译进来**，
#   本脚本那时跑到的仍是原来那几条。**别把「本脚本绿了」读成「那三条验过了」** ——
#   下面 `RAN` 那个自检印的是真实跑成的条数，以它为准。
#
# 红线：**绝不碰用户真实的 tmux server**（unset TMUX + 私有 TMUX_TMPDIR）；不碰真 ~/.claude。
# 跑法：bash tests/e2e/local-backend-supervise.sh   （npm run test:local-backend）
. "$(cd "$(dirname "$0")" && pwd)/sandbox-env.sh"  # 无条件清掉继承来的 CCM_* / CLAUDE* / ANTHROPIC_* / TMUX* / CC_BUS_*
set -uo pipefail

# `C7i` 隔离：走**共享原语**（`P0e` 08-12）。shim 强插 `-L <本趟私有名>`。
# ⚠⚠ 本套件与别的不同：它还要把隔离**传给被监护的 backend**（backend 自己会跑 `tmux ls`）。
#   原来传的是 `TMUX_TMPDIR` —— 而 `$TMUX` 一有值就会压过它（08-11 事故的机制）。
#   ⇒ 改传 **shim 目录**：backend 的 PATH 前面挂上它，它 shell out 的 tmux 一样被强插 `-L`。
# shellcheck source=tests/e2e/tmux-shim.sh
. "$(cd "$(dirname "$0")" && pwd)/tmux-shim.sh" e2eLocalBackend
E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$E2E_DIR/../.." && pwd)"
BACKEND="${CCM_E2E_BACKEND:-$REPO/.build/backend/debug/cc-monitor-backend}"
WORK="$(mktemp -d /tmp/e2e-lb.XXXXXX)"
# 家目录换成本趟沙箱：后端会读 / 写的 `~/.cc-monitor/` 那一族（账号库 · 中转钥匙 · 凭据表 …）都跟着家走，
#   不许落到开发机的真家目录里。工具链先钉住（只读，与被测数据无关）。
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export HOME="$WORK/home"
mkdir -p "$HOME"
CLAUDE_DIR="$WORK/claude"; mkdir -p "$CLAUDE_DIR/projects"

# **本套件自己的后端**：二进制是 `$BACKEND`、环境里带着本套件的 `$WORK`
#   （Rust 那侧起的每一个都继承 `CCM_E2E_WORK` / 沙箱 `HOME`，两样都在 `$WORK` 底下）。
#   认的是进程表里的事实（`/proc/<pid>/exe` ＋ `environ`），不是命令行长相 ——
#   从前收尾是 `pkill -f "$BACKEND"`：模式杀，连带命令行里含这条路径的**调用方 shell**（09-27 实发：
#   跑本套件的那条循环被它打死）与同一个二进制上别的套件的后端。
ours_backends() {
  local p exe real
  real="$(readlink -f -- "$BACKEND")"
  for p in /proc/[0-9]*; do
    exe="$(readlink -- "$p/exe" 2>/dev/null)" || continue
    [ "$exe" = "$real" ] || continue
    tr '\0' '\n' < "$p/environ" 2>/dev/null | grep -qF "=$WORK" || continue
    echo "${p#/proc/}"
  done
}

cleanup() {
  set +e
  # 收掉可能残留的被监护进程（测试自己会 stop()，这里是兜底）
  #
  # ★★ **这一步与下面删 shim 的顺序是承重的**〔`K-P1` 08-26 实测教训〕：
  #    漏网的 backend **还活着**，而 `tmux_shim_cleanup` 把 shim 目录 `rm -rf` 掉之后，
  #    它下一次装 tmux hook（watcher 换人时会重装）沿 PATH 找不到 shim ⇒ **落到真 tmux 上**
  #    ⇒ 用户真实 server 的 `[50]` 槽位被盖成它的 pid。本轮真发生过一次。
  #    ⇒ 先收进程、等它真的走了，再删 shim。
  # shellcheck disable=SC2046
  kill $(ours_backends) 2>/dev/null
  # 给它一拍走完（`pkill` 只是把信号发出去）。**不是定时器**：一次性的收尾等待。
  sleep 1
  # C7i 红线〔08-11 事故后〕：**socket 用 `-S` 显式给死**，不靠 `unset TMUX` + `TMUX_TMPDIR`。
  # 那条依赖是「漏一次就出事」的形态 —— 我在一条探针里漏了 unset，打到用户真实 server 上，
  # 9 个真实会话没了。`-S <绝对路径>` 不受 $TMUX 影响，漏什么都打不偏。
  # （形状抄 graylight-suite.sh:36，那里早就是这么写的。）
  tmux_shim_cleanup
  rm -rf -- "$WORK"
}
trap cleanup EXIT

[ -x "$BACKEND" ] || { echo "backend 二进制不存在/不可执行：$BACKEND（先 cd src/backend && cargo build）"; exit 1; }

echo "== F05a 本机后端监护 · 真进程验收 =="
echo "backend     : $BACKEND"
echo "tmux shim: $TMUX_SHIM_BIN（-L $TMUX_SHIM_SOCK，绝不碰用户真实 tmux server）"
echo

OUT="$WORK/rust.log"
# ⚠ **别写成 `cargo test … | tee`** —— 管线会把退出码藏起来。落文件再回显。
# ★★ **两趟，两个过滤串**：真进程判据今天住**两个**模块 ——
#    `local_backend`（F05a：监护那条路）与 `local_backend_host`（K-P1：常驻那条路）。
#
# ⚠⚠ **为什么不是一趟写两个过滤串**（`… --nocapture … local_backend local_backend_host`）：
#    libtest 确实取并集，**但那样第二个过滤串没有任何东西钉着**。
#    monitor 侧那条断链判据（`shared_crate_registry::every_ignored_test_still_has_someone_who_triggers_it`）
#    的抽取器是「`--nocapture` 之后的**第一个**非 flag token，取完就 `break`」
#    ⇒ 一行只登记得到一个过滤串。写成一行的话，**改掉 `local_backend_host` 那族的测试名不会有任何东西红**，
#    而 `cargo test` 跑零条测试**退出码是 0** —— 两边都绿，测试其实再没执行过。
#    ⇒ 一族一趟，每趟自己那一行都带着自己的过滤串。
# ⚠⚠ **第一趟的过滤串是 `local_backend::`（带 `::`），不是 `local_backend`**：libtest 按子串认，
#    `local_backend` 同时命中 `local_backend_host::…` ⇒ 常驻那族被跑了**两遍**，第一遍接上的后端还活着，
#    第二遍的「第一个宿主」当场报「已经在跑」（主线本来就红的那一格）。带 `::` 只命中监护那一族。
#    （`shared_crate_registry` 的抽取器截到 `:` 为止，登记到的仍是 `local_backend`，与原来同一个串。）
# ⚠ `--test-threads=1` 是**承重的**：`local_backend_host` 那两条要 `set_var("HOME")`，
#    那是进程级的事实，并发跑会互相踩。
: > "$OUT"
RC=0
(
  cd "$REPO/src/frontend/shell" && \
  CCM_E2E_BACKEND="$BACKEND" \
  CCM_E2E_TMUX_SHIM_BIN="$TMUX_SHIM_BIN" \
  CCM_E2E_CLAUDE_DIR="$CLAUDE_DIR" \
  CCM_E2E_WORK="$WORK" \
  cargo test --lib -- --ignored --nocapture --test-threads=1 local_backend::
) >>"$OUT" 2>&1 || RC=$?
(
  cd "$REPO/src/frontend/shell" && \
  CCM_E2E_BACKEND="$BACKEND" \
  CCM_E2E_TMUX_SHIM_BIN="$TMUX_SHIM_BIN" \
  CCM_E2E_CLAUDE_DIR="$CLAUDE_DIR" \
  CCM_E2E_WORK="$WORK" \
  cargo test --lib -- --ignored --nocapture --test-threads=1 local_backend_host
) >>"$OUT" 2>&1 || RC=$?
sed -n '/^test /p;/^E2E-OK/p;/^test result/p' "$OUT"

pass=0; fail=0
ok()  { printf '  PASS %s\n' "$1"; pass=$((pass+1)); }
bad() { printf '  FAIL %s\n' "$1"; fail=$((fail+1)); }

echo
# ── 抽取器自检：Rust 那一路真的跑起来了吗 ──────────────────────────────────
# 「没输出」不是绿：编译失败、过滤器打错、`--ignored` 拼错都会得到 0 个测试。
#
# ⚠ **别按 `^test … ok$` 数**（首版就是这么写的，当场 BROKEN 而其实全绿）：
#   加了 `--nocapture` 之后，测试自己 println 的内容会插在 `test <名> ... ` 与 `ok`
#   之间，于是那个 `ok` **落在了下一行**，整行匹配永远为 0。
#   ⇒ 认权威的那一行：`test result: ok. N passed`。
#   这次是**fail-closed 救的**（报 BROKEN 而不是绿），否则就是一次伪造的绿。
# ⚠ **两趟之后不能再 `tail -1`** —— 那只会拿到最后一趟的数，
#   前一趟跑了几条就没人数了（而「少跑了一整族」正是这条自检要抓的形状）。
#   ⇒ **两趟相加**。同理下面那条「标记数 < 跑成的测试数」比的也是总数。
RAN=$(sed -n 's/^test result: [A-Za-z]*\. \([0-9]*\) passed.*/\1/p' "$OUT" \
        | awk '{ s += $1 } END { print s+0 }')
RAN=${RAN:-0}
if [ "$RAN" -ge 1 ]; then ok "抽取器：Rust 侧真跑了 $RAN 条 ignore 测试"
else
  echo "  BROKEN Rust 侧一条 ignore 测试都没跑成（RAN=$RAN，rc=$RC）—— 下面全部断言会零命中"
  tail -25 "$OUT"; exit 2
fi

# ── 每条 E2E-OK 标记 = 一条断言。**导出式自检**：不写硬编码数字地板 ────────
# 定框 §4：「同一个数不许两侧各写一份」。门禁与 CI 都不钉本套件的条数；这里只查「Rust 报的 ok 数与标记数自洽」。
#
# ⚠ **别锚 `^E2E-OK`**（首版这么写，漏掉一半）：`--nocapture` 下每个测试的**第一条** println
#   被拼在 `test <名> ... ` 后面，不在行首；只有第二条起才顶格。同一个坑的第二种形状。
#   ⇒ 用 `grep -o 'E2E-OK .*'` 取标记本体，不管它前面有什么。
MARKS=$(grep -c 'E2E-OK ' "$OUT")
while IFS= read -r line; do ok "${line#E2E-OK }"; done < <(grep -o 'E2E-OK .*' "$OUT")

if [ "$RC" -ne 0 ]; then
  bad "Rust 侧退出码 $RC —— 有 ignore 测试失败（见下）"
  grep -E '^(thread|assertion|  left|  right)' "$OUT" | head -20
fi

# 每条 ignore 测试至少产一个标记；标记数少于测试数 ⇒ 有测试提前 return 了。
# ⚠ **这一条对「新接进来的测试」是有门槛的**：接进本套件的每一条
#    都必须**至少打一个 `E2E-OK` 标记**，否则这条自检会红，而红的理由是**假的**
#    （不是「断言没走完」，是「那条从来不打标记」）。本轮接进来的两条各补了标记。
if [ "$MARKS" -lt "$RAN" ]; then
  bad "标记数 $MARKS < 跑成的测试数 $RAN —— 有测试提前退出、断言没走完"
fi

# **按环境显式分支**：下面这三条起真后端的判据由 `cfg(all(embedded_backends, linux, x86_64))` 门着
#   （`build.rs` 只在 `src/frontend/shell/embedded-backends/` 两个 arch 都齐时置 cfg；那个目录 gitignore）。
#   没铺 ⇒ 它们不编译进来 ⇒ 那几条断言记 **SKIP** 并说原因；PASS ＋ SKIP 恒等总条数（门禁 `exact-with-skip`，照 `backend-gate2`）。
#   ⚠ SKIP 只许出现在「环境真不够」时：落点齐了、本机也是 Linux x86_64，却一条都没跑 ⇒ **FAIL**（多半是换了落点之后
#   `build.rs` 没重跑 —— `touch src/frontend/shell/build.rs`，见 `local_backend_host_tests.rs` 那段复跑纪律）；只跑了一部分 ⇒ 也 FAIL。
EMB_TESTS=(
  local_backend::tests::the_local_backend_host_really_registers_an_inbound_client
  local_backend_host::tests::the_local_backend_host_can_be_stopped_and_started_again
)
# 三条 → 两条：`the_local_tmux_frames_really_land_in_the_ledger`〔散文墓碑〕删了 —— 它钉的是「本机 tmux 快照帧真落进
#   monitor 那本 tmux 原文账」，而那本账随会话 / tmux 账本进后端删了、快照帧删了（后端那本账的真 tmux 实测是
#   `graylight-backend-frames.sh`）。它打 2 条标记 ⇒ 11 → 9。
# 那两条合起来打的断言标记数（09-28 在铺了落点的非 ASCII 路径树上现打：24 − 15）。
EMB_MARKS=9
skip=0
emb_ran=0
for t in "${EMB_TESTS[@]}"; do grep -qF "test $t " "$OUT" && emb_ran=$((emb_ran + 1)); done
emb_ready=0
if [ -f "$REPO/src/frontend/shell/embedded-backends/cc-monitor-backend-x86_64" ] \
   && [ -f "$REPO/src/frontend/shell/embedded-backends/cc-monitor-backend-aarch64" ] \
   && [ "$(uname -s) $(uname -m)" = "Linux x86_64" ]; then emb_ready=1; fi
if [ "$emb_ran" -eq "${#EMB_TESTS[@]}" ]; then :
elif [ "$emb_ran" -eq 0 ] && [ "$emb_ready" -eq 0 ]; then
  skip=$EMB_MARKS
  printf '  SKIP %s 条断言（%s 条起真后端的判据）：本树没铺 src/frontend/shell/embedded-backends/ 两个 arch（或本机不是 Linux x86_64）—— build.rs 不置 cfg(embedded_backends)，它们不编译进来\n' "$EMB_MARKS" "${#EMB_TESTS[@]}"
elif [ "$emb_ran" -eq 0 ]; then
  bad "落点齐了、本机也是 Linux x86_64，那 ${#EMB_TESTS[@]} 条却一条都没跑 —— build.rs 没重跑？（touch src/frontend/shell/build.rs）"
else
  bad "那 ${#EMB_TESTS[@]} 条只跑了 $emb_ran 条 —— 门它们的 cfg 分叉了"
fi

# Rust 那侧自己收尸（`E2eSandbox` 的 `Drop` 按句柄收）；跑完还活着的就是**漏网**的 ——
#   从前靠收尾那句模式杀兜着，漏了也看不见（两趟过滤串重叠那次，第一趟接上的后端就是这样留到第二趟、
#   让它当场报「已经在跑」）。收尾照样会收掉它们，但先记一条红。
LEFT="$(ours_backends | tr '\n' ' ')"
if [ -n "${LEFT// /}" ]; then bad "Rust 侧跑完还有本套件起的后端活着（pid $LEFT）—— 有一条判据没收尸"
else ok "Rust 侧跑完没有漏网的后端（进程表里按 exe ＋ \$WORK 认）"; fi

echo
echo "===== 合计 PASS=$pass FAIL=$fail SKIP=$skip ====="
[ "$fail" -eq 0 ]
