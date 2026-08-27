#!/usr/bin/env bash
# F05a：**本机后端进程「起与看住」的真进程验收**。
#
# 与单测的分工：单测断言「重启策略与路径解析怎么答」（纯函数）；
# **本脚本断言「监护器在真进程上到底干了什么」** —— 起真 `cc-monitor-remote`、把它杀掉、
# 看它自己回来；再喂一个必崩的二进制，看它在上限内被判死而不是无限自旋。
# 门禁只锁判定不锁行为是 R1 的教训（三门禁全绿仍放行过一个让 send-keys 完全失效的改动）。
#
# ★ 真进程那部分住在 Rust 侧的 `#[ignore]` 测试里（`supervise()` 的 API 在那儿），
#   本脚本负责三件 Rust 测试不该管的事：
#     ① **隔离** —— 私有 `TMUX_TMPDIR`（被监护的 daemon 一起来就往 tmux server 装三条
#        全局 hook，**没有开关**；不隔离就是去改用户真实 tmux 的状态）；
#     ② 临时工作目录（必崩脚本、空目录探测都在里面造）；
#     ③ 断言计数与收尾格式（与全仓其余 18 套逐字一致）。
#
# 红线：**绝不碰用户真实的 tmux server**（unset TMUX + 私有 TMUX_TMPDIR）；不碰真 ~/.claude。
# 跑法：bash e2e/local-backend-supervise.sh   （npm run test:local-backend）
set -uo pipefail

# `C7i` 隔离：走**共享原语**（`P0e` 08-12）。shim 强插 `-L e2eLocalBackend`。
# ⚠⚠ 本套件与别的不同：它还要把隔离**传给被监护的 daemon**（daemon 自己会跑 `tmux ls`）。
#   原来传的是 `TMUX_TMPDIR` —— 而 `$TMUX` 一有值就会压过它（08-11 事故的机制）。
#   ⇒ 改传 **shim 目录**：daemon 的 PATH 前面挂上它，它 shell out 的 tmux 一样被强插 `-L`。
TMUX_SHIM_SOCK=e2eLocalBackend
# shellcheck source=e2e/tmux-shim.sh
. "$(cd "$(dirname "$0")" && pwd)/tmux-shim.sh"
E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$E2E_DIR/.." && pwd)"
DAEMON="${CCM_E2E_DAEMON:-$REPO/remote-daemon-proto/target/debug/cc-monitor-remote}"
WORK="$(mktemp -d /tmp/e2e-lb.XXXXXX)"
CLAUDE_DIR="$WORK/claude"; mkdir -p "$CLAUDE_DIR/projects"

cleanup() {
  set +e
  # 收掉可能残留的被监护进程（测试自己会 stop()，这里是兜底）
  #
  # ★★ **这一步与下面删 shim 的顺序是承重的**〔`K-P1` 08-26 实测教训〕：
  #    漏网的 daemon **还活着**，而 `tmux_shim_cleanup` 把 shim 目录 `rm -rf` 掉之后，
  #    它下一次装 tmux hook（watcher 换人时会重装）沿 PATH 找不到 shim ⇒ **落到真 tmux 上**
  #    ⇒ 用户真实 server 的 `[50]` 槽位被盖成它的 pid。本轮真发生过一次。
  #    ⇒ 先收进程、等它真的走了，再删 shim。
  pkill -f "$DAEMON" 2>/dev/null
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

[ -x "$DAEMON" ] || { echo "daemon 二进制不存在/不可执行：$DAEMON（先 cd remote-daemon-proto && cargo build）"; exit 1; }

echo "== F05a 本机后端监护 · 真进程验收 =="
echo "daemon     : $DAEMON"
echo "tmux shim: $TMUX_SHIM_BIN（-L $TMUX_SHIM_SOCK，绝不碰用户真实 tmux server）"
echo

OUT="$WORK/rust.log"
# ⚠ **别写成 `cargo test … | tee`** —— 管线会把退出码藏起来。落文件再回显。
# ★★ **两趟，两个过滤串**〔`K-P1` 08-26〕：真进程判据今天住**两个**模块 ——
#    `local_backend`（F05a：监护那条路）与 `local_daemon`（K-P1：常驻那条路）。
#
# ⚠⚠ **为什么不是一趟写两个过滤串**（`… --nocapture … local_backend local_daemon`）：
#    libtest 确实取并集，**但那样第二个过滤串没有任何东西钉着**。
#    monitor 侧那条断链判据（`shared_crate_registry::every_ignored_test_still_has_someone_who_triggers_it`）
#    的抽取器是「`--nocapture` 之后的**第一个**非 flag token，取完就 `break`」
#    ⇒ 一行只登记得到一个过滤串。写成一行的话，**改掉 `local_daemon` 那族的测试名不会有任何东西红**，
#    而 `cargo test` 跑零条测试**退出码是 0** —— 两边都绿，测试其实再没执行过。
#    ⇒ 一族一趟，每趟自己那一行都带着自己的过滤串。
# ⚠ `--test-threads=1` 是**承重的**：`local_daemon` 那两条要 `set_var("HOME")`，
#    那是进程级的事实，并发跑会互相踩。
: > "$OUT"
RC=0
(
  cd "$REPO/src-tauri" && \
  CCM_E2E_DAEMON="$DAEMON" \
  CCM_E2E_TMUX_SHIM_BIN="$TMUX_SHIM_BIN" \
  CCM_E2E_CLAUDE_DIR="$CLAUDE_DIR" \
  CCM_E2E_WORK="$WORK" \
  cargo test --lib -- --ignored --nocapture --test-threads=1 local_backend
) >>"$OUT" 2>&1 || RC=$?
(
  cd "$REPO/src-tauri" && \
  CCM_E2E_DAEMON="$DAEMON" \
  CCM_E2E_TMUX_SHIM_BIN="$TMUX_SHIM_BIN" \
  CCM_E2E_CLAUDE_DIR="$CLAUDE_DIR" \
  CCM_E2E_WORK="$WORK" \
  cargo test --lib -- --ignored --nocapture --test-threads=1 local_daemon
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
# ⚠ 〔`K-P1` 08-26〕**两趟之后不能再 `tail -1`** —— 那只会拿到最后一趟的数，
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
# 定框 §4：「同一个数不许两侧各写一份」。数字地板只写在 CI 的
# `assert-pass-floor.sh local-backend <n>` 那一处。这里只查「Rust 报的 ok 数与标记数自洽」。
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

# 三条 ignore 测试，每条至少产一个标记；标记数少于测试数 ⇒ 有测试提前 return 了。
if [ "$MARKS" -lt "$RAN" ]; then
  bad "标记数 $MARKS < 跑成的测试数 $RAN —— 有测试提前退出、断言没走完"
fi

echo
echo "===== 合计 PASS=$pass FAIL=$fail ====="
[ "$fail" -eq 0 ]
