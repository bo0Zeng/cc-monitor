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
#   ⚠⚠ **文件名逐字是 `.build_id`(同目录下的隐藏文件),不是 `<二进制名>.build_id`**
#   ——`sftp.rs::marker_path` 是 `format!("{dir}/.build_id")`。08-13 写错成后者,
#   app 当场判「远端无版本标记」⇒ **把本脚本覆盖成内嵌二进制**,于是 daemon 用**真** `~/.claude`
#   起来了(只读铁律没破,但沙箱意图整个落空)。这一行写清楚,省得下一个人再踩。
#   (内容 = app 期望的 daemon build_id,如 `p1p-tmux-frame`),再把 daemonPath 指向它 →
#   deploy_decision=Skip、脚本存活。(见 src-tauri/src/sftp.rs::deploy_decision +
#   ssh_source EXPECTED_DAEMON_BUILD_ID)
E2E_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO=$(CDPATH= cd -- "$E2E_DIR/.." && pwd)
: "${CCM_E2E_CLAUDE_DIR:=/tmp/e2e-remote-claude}"
: "${CCM_E2E_DAEMON:=$REPO/remote-daemon-proto/target/debug/cc-monitor-remote}"
# ★★ 〔`P0b` 第十拍 08-13〕**同目录的 `daemon-path` 文件优先于下面的自愈**。
#
# 病:全链跑法要求把**本脚本的副本**放进一个目录(见上面的部署告警),而副本一旦离开仓,
# `$REPO` 就解析到了别处 ⇒ 上面那个默认落空 ⇒ 走下面的自愈 ⇒ **静默换成另一个二进制**
#(实测:换成了 `~/.cc-monitor/bin/` 里那份**陈旧**的部署产物,hello 报的 build_id 是旧的)。
# 自愈本身是对的(没有它,副本根本起不来);错在它**不吭声**——`#60` 的九拍排除链里,
# 「被测二进制被悄悄换掉」这件事从头到尾没人看得见。
# ⇒ 副本旁边放一个 `daemon-path` 文件(内容 = 绝对路径)就能钉死用哪个;
#   env 传不进来(SSH exec 不带),文件是唯一传得进来的东西。
if [ -f "$E2E_DIR/daemon-path" ]; then
  _pinned=$(cat "$E2E_DIR/daemon-path")
  [ -x "$_pinned" ] && CCM_E2E_DAEMON="$_pinned"
fi
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
# ## ★★ 查 `#60` 时用这个 tap 排除到哪一步了〔`P0b` 08-13，九拍的结论〕
#
# 同一个 daemon 二进制、同样的 fixture 形状，三条起法的读数：
#   · 离线直起（参数/时序/陈旧 pidfile/目录存在与否，四种变体）⇒ **正常**
#   · **手动** `ssh 127.0.0.1 <本脚本>` ⇒ **正常**（`hello · session_added · 2× tmux_sessions`）
#   · **app 经 SSH 起** ⇒ `hello` + 1 帧 `tmux_sessions` 之后**全哑**，stderr 也一字不说
# ⇒ **daemon 本身、参数、时序、fixture、SSH 全被排除**；断点在 **app 那侧怎么起/怎么管这条连接**。
#
# 下一格该试的（差别只剩 app 特有的那几样）：
#   ① **stdin** —— `P2`/`P4d` 之后 monitor 会把入方向命令写进 daemon 的 stdin，
#      而手动那条 stdin 是空的。**把 stdin 接到 /dev/null 再 exec**，看是否一摘就正常；
#   ② 连接管理（keepalive / 重连 / 与每 10s 一次的账号查询并行）；
#   ③ `--with-bg` 子进程与父的生命周期。
#
# 〔`P0b` 第七拍 08-13〕**可选的帧 tap**：设了 `CCM_E2E_FRAME_TAP` 就把 daemon 的 stdout
# 抄一份到那个文件。全链套件失败时，这是唯一能回答「**daemon 到底发了什么**」的口子 ——
# 在它之前只能看 monitor 记了什么，而那分不清「没发」与「发了没收到」。
# ⚠ `stdbuf -oL` 不能省：不加的话 tee 到管道会变**块缓冲**，把帧攒住、改变时序。
# ⚠ 不设就是**原样 exec**（与本改动之前逐字同行为）——默认路径一个字节不变。
#
# ★★ 〔第十拍 08-13〕**给它一个默认值**。理由与 `CCM_E2E_CLAUDE_DIR`/`CCM_E2E_TMUX_SOCK`
# 逐字相同:**经 SSH exec 本脚本时 env 不带 `CCM_E2E_*`** ⇒ 靠调用方传是传不进来的,
# 而没有 tap 就回到「只能看 monitor 记了什么」——那分不清「没发」与「发了没收到」,
# 正是这条排除链前六拍卡住的原因。本脚本是**测试 fixture**,写 /tmp 是它的本分。
: "${CCM_E2E_FRAME_TAP:=/tmp/ccm-e2e-daemon-frames.tap}"
if [ -n "${CCM_E2E_FRAME_TAP:-}" ]; then
  # ★ 开跑先**自报家门**:哪个二进制、什么 build_id。上面那条自愈会换二进制,
  #   不报出来的话,一次跑完你无法回答「我刚才测的是谁」。
  {
    echo "=== wrapper 起 daemon: $CCM_E2E_DAEMON"
    echo "    claude_dir=$CCM_E2E_CLAUDE_DIR  pid=$$  $(date -Iseconds)"
    "$CCM_E2E_DAEMON" --daemon-probe 2>/dev/null | head -1
  } >> "${CCM_E2E_FRAME_TAP}.err" 2>&1
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
