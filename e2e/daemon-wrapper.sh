#!/bin/sh
# auto-e2e F-E0:loopback-remote 的 daemon 包装器(测试 fixture,**非** daemon 改动)。
# 把 daemon 的 $CLAUDE_CONFIG_DIR 钉到一个一次性隔离目录(默认 /tmp/e2e-remote-claude),
# 让 app 经 loopback SSH 连上来时读的是 fixture 而**不是**真实 ~/.claude——否则本机会话会
# 同时以「本地 tab」和「远端 tab」双份出现(§ 双 tab)。config.json 的 daemonPath 指向本脚本即可。
# 默认 daemon 二进制 = 仓内 debug 构建;CCM_E2E_DAEMON / CCM_E2E_CLAUDE_DIR 可覆盖。
#
# ★重要(实测,F-E1 全链):app **会自动部署** daemon——若 daemonPath 同目录没有匹配当前
#   app 期望 build_id 的 `.build_id` 标记文件,app 会把内嵌 daemon 二进制**覆盖写到 daemonPath**
#   (把本脚本冲掉!)。故全链跑法:把本脚本(或其副本)放进一个目录,旁边放一个 `.build_id`
#   (内容 = app 期望的 daemon build_id,如 `p1p-tmux-frame`),再把 daemonPath 指向它 →
#   deploy_decision=Skip、脚本存活。(见 src-tauri/src/sftp.rs::deploy_decision +
#   ssh_source EXPECTED_DAEMON_BUILD_ID)
E2E_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO=$(CDPATH= cd -- "$E2E_DIR/.." && pwd)
: "${CCM_E2E_CLAUDE_DIR:=/tmp/e2e-remote-claude}"
: "${CCM_E2E_DAEMON:=$REPO/remote-daemon-proto/target/debug/cc-monitor-remote}"
# 仓内 debug 构建优先(CI 场景);缺失(如 worktree 未构建)时退到已部署/已构建的二进制。
# 经 SSH exec 本脚本时 env 不带 CCM_E2E_*,故默认必须能自愈到一个真存在的 daemon。
if [ ! -x "$CCM_E2E_DAEMON" ]; then
  # 顺序:仓内 release > app 已部署 bin/(随 app 更新,较新) > e2e/(可能陈旧,或缺 tmux_sessions 帧)。
  for c in \
    "$REPO/remote-daemon-proto/target/release/cc-monitor-remote" \
    "$HOME/.cc-monitor/bin/cc-monitor-remote" \
    "$HOME/.cc-monitor/e2e/cc-monitor-remote"; do
    if [ -x "$c" ]; then CCM_E2E_DAEMON="$c"; break; fi
  done
fi
# ★★★ **P0b（08-12）：daemon 也要落在套件那个私有 tmux socket 上。**
#
# `P0d` 把套件的 tmux 隔离从 `TMUX_TMPDIR` 换成 `-L <名>` shim（C7i 红线），
# 这一步是对的 —— 但它**把一个功能前提悄悄拿掉了**：
# 套件把会话建在 `-L e2eGray` 上，而 **daemon 跑在 SSH 那头、不继承本 shell 的 PATH**
# ⇒ 它 `tmux ls` 读的是**默认 socket**，**看不见 fixture 会话**
# ⇒ `session_added` 根本不发 ⇒ 全链套件从此测不到任何东西。
#
# ★ `P0d` 当时的判据为什么没抓到：它验的是「隔离生效 + 帧级套件 12/0」，
#   而**帧级那套的 daemon 与 tmux 在同一个 shell 里**（都吃 shim）⇒ 绿；
#   全链那套的 daemon 在 SSH 那头 ⇒ 断。**判据的射程比它自称的窄，
#   而窄的那一格恰好是全链。**
#
# ⇒ 由调用方经 `CCM_E2E_TMUX_SOCK` 告诉它用哪个 socket，wrapper 在这里造一份同款 shim
#   塞进 daemon 的 PATH。**不设就退回默认 socket**（与本改动之前逐字同行为）——
#   帧级那套不传它，照旧工作。
# 默认值不能省：本脚本经 SSH exec 时 env 不带 CCM_E2E_*（头注第 19 行的既定纪律）
# ⇒ 靠调用方传 env 行不通，必须像 CCM_E2E_CLAUDE_DIR 那样给一个与套件约定一致的默认。
# e2eGray = graylight-suite.sh 用的那个名字（两处是双写点，改一处要改两处）。
: "${CCM_E2E_TMUX_SOCK:=e2eGray}"
if [ -n "${CCM_E2E_TMUX_SOCK:-}" ]; then
  _real_tmux=$(command -v tmux 2>/dev/null)
  if [ -n "$_real_tmux" ]; then
    _shim=$(mktemp -d /tmp/e2e-daemon-tmuxshim.XXXXXX) || _shim=""
    if [ -n "$_shim" ]; then
      printf '#!/bin/sh\nexec %s -L %s "$@"\n' "$_real_tmux" "$CCM_E2E_TMUX_SOCK" > "$_shim/tmux"
      chmod +x "$_shim/tmux"
      PATH="$_shim:$PATH"; export PATH
    fi
  fi
fi
# 〔`P0b` 第七拍 08-13〕**可选的帧 tap**：设了 `CCM_E2E_FRAME_TAP` 就把 daemon 的 stdout
# 抄一份到那个文件。全链套件失败时，这是唯一能回答「**daemon 到底发了什么**」的口子 ——
# 在它之前只能看 monitor 记了什么，而那分不清「没发」与「发了没收到」。
# ⚠ `stdbuf -oL` 不能省：不加的话 tee 到管道会变**块缓冲**，把帧攒住、改变时序。
# ⚠ 不设就是**原样 exec**（与本改动之前逐字同行为）——默认路径一个字节不变。
if [ -n "${CCM_E2E_FRAME_TAP:-}" ]; then
  # ⚠⚠ **stderr 也要抄**〔第八拍 08-13〕：daemon 的 `tracing` 日志走 stderr，
  #   而它正是唯一会说出「watch failed / sessions dir does not exist / 我在盯哪」的地方。
  #   只抄 stdout 的那一版实测**问不出**「daemon 自己怎么看这件事」——
  #   全链里它 hello 之后只发一帧就沉默，而沉默的理由只可能写在 stderr 上。
  # ⚠ 落到**另一个文件**（`.err`），别与帧混在一起：帧那份要能直接按行解析。
  # ⚠⚠ **必须是 POSIX 写法**：本脚本的 shebang 是 `#!/bin/sh`（本机 = dash）。
  #   第一版用了 bash 的进程替换 `2> >(tee …)` —— 而我拿 `bash -n` 验的语法，**验错了 shell**：
  #   `sh -n` 当场报 `Syntax error: redirection unexpected`，daemon 于是**根本没起来**
  #   （两个 tap 都是 0 行，而我差点把「空 tap」读成「daemon 沉默」）。
  #   ⇒ stderr 直接**追加重定向到文件**：不经 tee、不丢诊断，SSH 那侧本来也不读它。
  exec env CLAUDE_CONFIG_DIR="$CCM_E2E_CLAUDE_DIR" "$CCM_E2E_DAEMON" "$@" \
    2>> "${CCM_E2E_FRAME_TAP}.err" \
    | stdbuf -oL tee -a "$CCM_E2E_FRAME_TAP"
fi
exec env CLAUDE_CONFIG_DIR="$CCM_E2E_CLAUDE_DIR" "$CCM_E2E_DAEMON" "$@"
