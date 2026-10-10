#!/usr/bin/env bash
# `BUILD_ID` bump 的同拍步骤：把内嵌的那几份后端字节重编并铺回落点。全仓唯一的本机产字节入口。
# bump `src/backend/lib.rs` 的 `const BUILD_ID` 那一刻，`src/frontend/shell/embedded-backends/` 里那两份 musl 字节就变旧了：
#   发版那趟的内嵌校验会红；本机手工打包则装出去一份自报旧 id 的字节，已部署的远端不判 stale、不重装。
# `--check` 当场用相等断言回答「盘上这几份字节与源码的 `BUILD_ID` 对不对得上」；发版那一侧是 `release.yml` 的内嵌校验。
#
# 配方与 `release.yml` 的 `Cross-compile backend for both musl targets` 那一步同源：`REEMBED_BUILD_FLAGS` / `REEMBED_TARGETS`
#   由 `tests/evidence/K-R124-ruler.py` 每趟两向对拍（target 集合相等 ＋ 旗标逐字相同）。
# `zig cc` 是承重的，别换成 `rust-lld`：`ring` 的 build script 要编 C，`rust-lld` 只是链接器，aarch64 上会找不到
#   `aarch64-linux-musl-gcc`；`cargo zigbuild` 自带 `zig cc`，两个 arch 都不用另装 C 交叉工具链。
#
# 买不到的：
# 1. 本机重编 ≠ 这一版发得出去：形态与发版 CI 的产物不同（static-pie / 未 strip / 不同 rustc），本机的 zig 与
#    cargo-zigbuild 也未必是 `release.yml` 钉的版本。买到的是开发期自洽 ＋ 裸 exe 恢复部署能力。
# 2. 那份字节在真的远端 Linux 上跑得起来 —— 本文件不在远端运行它。
# 3. `--check` 在没铺字节的树上（clone 下来的默认状态、CI）只答得出「这棵树上没有一份对不上的字节」，不代表字节是对的。
#    人群从 `REEMBED_TARGETS` 派生，不从「扫落点目录」来 —— 后者在目录被删空时恒绿。
# 4. 防篡改：身份戳防的是漂移与手滑，不防恶意（那要签名）。
#
# 开发构建也要起得来本机后端：`--native` 编出本机原生后端铺进 `src/frontend/shell/native-backend/`，
#   `build.rs::embed_native_backend` 把它内嵌进 exe（配方与发版那两步同源）。「起得来」是真起一趟那份字节判的
#   （`native_starts`：空 stdin、隔离的 HOME、一律失败的 tmux 替身），stdout 第一行必须是一帧 hello、自报的 `build_id` 等于源码。
#   `--check-dev` 与 `--check` 同一套，差别只在本机那一份缺席 = 红（`--check` 里是 skip，给 CI / 没铺字节的树）。
#
# 跑法：
#   bash tests/scripts/re-embed.sh             # 重编两份 musl 字节并铺回落点，铺完自检
#   bash tests/scripts/re-embed.sh --check     # 只问「盘上的字节与源码对不对得上 · 铺了的起不起得来」，不产字节
#   bash tests/scripts/re-embed.sh --check-dev # 同上，但本机那一份缺席就是红：开发构建起得来本机后端吗
#   bash tests/scripts/re-embed.sh --native    # 本机那几份（裸 exe 自带的后端 · 文件窗口程序）重编并重铺，铺完按 --check-dev 自检
#   bash tests/scripts/re-embed.sh --clean     # 守卫给的第二条出路：删掉落点（自动部署诚实关闭）
#
# 退出码 0 = 过；1 = 有对不上的 / 抠不到身份 / 编不过 / 铺了却起不来 /（`--check-dev`）本机那一份没铺。
# 最后一行恒印 `re-embed: <N> passed（…）`，`N` = 上面逐行印出来的 PASS 条数。
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"

#: 与 `release.yml` 那一步逐字同源（`K-R124-ruler.py` 两向对拍）。
REEMBED_BUILD_FLAGS=(--release --locked)
REEMBED_TARGETS=(x86_64-unknown-linux-musl aarch64-unknown-linux-musl)

#: 身份的唯一住址 —— 与 `release.yml` 的
#: `env.CCM_BACKEND_IDENTITY_SRC` 是同一份文件。
IDENTITY_SRC="$ROOT/src/backend/lib.rs"
#: 身份戳两个界标的住址 —— 契约 crate（后端拼戳 · monitor 扫字节同一份；`release.yml` 的 `env.CCM_STAMP_MARKS_SRC` 同一份文件）。
MARKS_SRC="$ROOT/src/common/deploy-contract/src/lib.rs"
#: 内嵌落点 —— 名字定死在 `build.rs` 的 `EMBEDDED_BACKENDS_DIR` / `NATIVE_BACKEND_DIR`
#: （`K-R124-ruler.py` 把这两处与本文件、与 `src/frontend/shell/.gitignore` 三向钉在一起）。
EMBEDDED_DIR="$ROOT/src/frontend/shell/embedded-backends"
NATIVE_DIR="$ROOT/src/frontend/shell/native-backend"

pass=0
fail=0

ok()  { printf 'PASS  %s :: %s\n' "$1" "$2"; pass=$((pass + 1)); }
bad() { printf 'FAIL  %s :: %s\n' "$1" "$2"; fail=$((fail + 1)); }

# 从一份源码里抠一个 `const <名>: &str = "…";`，**恰好一行**才算数（`$2` 缺省 = 身份那份）。
# 抠不到给空串，调用方按红记 —— **不许兜底成一个会参与比较的字符串**
# （`"unknown"` 这类兜底值会混进相等断言）。
src_const() {
  local name="$1" file="${2:-$IDENTITY_SRC}" hits
  hits="$(grep -cE "^[[:space:]]*(pub )?const ${name}: &str = \"[^\"]+\";" "$file" || true)"
  [ "$hits" = "1" ] || return 0
  sed -nE "s/^[[:space:]]*(pub )?const ${name}: &str = \"([^\"]+)\";.*/\2/p" "$file" | head -1
}

# 一份字节自报的身份：扫它里面那段定长戳 `<开><id><关>`。
# 恰好一个才是身份（与 `deploy-contract::identity_of_bytes` 同一条）。0 个 / 多个都回一个说明串，
#   它必然不等于任何真 `BUILD_ID`，于是落在相等断言的红这一侧。
bytes_id() {
  local f="$1" open="$2" close="$3" found n s
  # 界标之间**至少一个字符**：两个界标常量在 `.rodata` 里挨着时会读出一个空 id 的「戳」，
  # 它不是身份 —— 与 `deploy-contract::identity_of_bytes` · `deploy-contract::stamp_scan_cmd` 同一条（空串不收）。
  found="$(LC_ALL=C grep -aoE "${open}[[:alnum:]_.-]+${close}" "$f" | sort -u || true)"
  n="$(printf '%s' "$found" | grep -c . || true)"
  if [ "$n" != "1" ]; then
    printf '<问出 %s 个身份戳>' "$n"
    return 0
  fi
  s="${found#"$open"}"
  s="${s%"$close"}"
  printf '%s' "$s"
}

do_build() {
  local t arch
  for t in "${REEMBED_TARGETS[@]}"; do
    printf '==> cargo zigbuild %s --target %s\n' "${REEMBED_BUILD_FLAGS[*]}" "$t"
    ( cd "$ROOT/src/backend" && cargo zigbuild "${REEMBED_BUILD_FLAGS[@]}" --target "$t" )
  done
  mkdir -p "$EMBEDDED_DIR"
  for t in "${REEMBED_TARGETS[@]}"; do
    arch="${t%%-*}"
    cp "$ROOT/.build/backend/$t/release/cc-monitor-backend" \
       "$EMBEDDED_DIR/cc-monitor-backend-$arch"
    printf '==> 铺好 src/frontend/shell/embedded-backends/cc-monitor-backend-%s\n' "$arch"
  done
}

# 本机那一份（裸 exe 自己带着的后端）。与 `release.yml` 的
# `Build local backend (native)` ＋ `Stage native backend for self-extract` 同一条配方。
# 它不进缺省那一趟：缺省那趟只管远端那两份 musl；本机这一份只在 Windows job 上铺，开发树上缺席是常态。
do_native() {
  local triple exe
  triple="$(rustc -vV | sed -n 's/^host: //p')"
  case "$triple" in *windows*) exe=".exe" ;; *) exe="" ;; esac
  printf '==> cargo build %s（本机 target %s）\n' "${REEMBED_BUILD_FLAGS[*]}" "$triple"
  ( cd "$ROOT/src/backend" && cargo build "${REEMBED_BUILD_FLAGS[@]}" )
  mkdir -p "$NATIVE_DIR"
  cp "$ROOT/.build/backend/release/cc-monitor-backend$exe" "$NATIVE_DIR/cc-monitor-native"
  # 名字里没有 triple ⇒ 「给哪个平台编的」只能靠这份旁挂清单
  # （`build.rs::embed_native_backend` 的 ① 号硬校验读它，对不上当场 panic）。
  printf '%s\n' "$triple" > "$NATIVE_DIR/cc-monitor-native.target"
  printf '==> 铺好 src/frontend/shell/native-backend/cc-monitor-native（＋ .target = %s）\n' "$triple"
  # 文件窗口程序（单文件的 monitor 靠它开窗）。它是 monitor 包的另一个 `[[bin]]`，monitor 的 build.rs 在同一趟 cargo 里
  #   吃不到它 ⇒ 先单编、铺好，之后再编 monitor 才嵌得进去。与 `release.yml` 两个 job 的 `Build local filewin (native)` ＋
  #   `Stage native filewin for self-extract` 同一条配方；落点名字定死在 `build.rs::NATIVE_FILEWIN_FILE`（判据对拍）。
  printf '==> cargo build %s --bin cc-monitor-filewin（本机 target %s，src/frontend/shell）\n' "${REEMBED_BUILD_FLAGS[*]}" "$triple"
  ( cd "$ROOT/src/frontend/shell" && cargo build "${REEMBED_BUILD_FLAGS[@]}" --bin cc-monitor-filewin )
  cp "$ROOT/.build/shell/release/cc-monitor-filewin$exe" "$NATIVE_DIR/cc-monitor-filewin"
  printf '%s\n' "$triple" > "$NATIVE_DIR/cc-monitor-filewin.target"
  printf '==> 铺好 src/frontend/shell/native-backend/cc-monitor-filewin（＋ .target = %s）\n' "$triple"
}

# 真起一趟文件窗口程序（空 stdin），回「退出码 就绪行」。它读不到开窗种子就在 stderr 上说一行
# `ccm-filewin-ready {"failed":…}` 再退（非 0）—— 那一行正是 monitor 开窗时读的就绪行（stdout 是通道）
# ⇒ 读得到它 = 这份字节在这台机器上起得来。不开窗、不碰任何文件。
filewin_starts() {
  local f="$1" sandbox rc line
  sandbox="$(mktemp -d)"
  cp "$f" "$sandbox/filewin-under-test"
  chmod +x "$sandbox/filewin-under-test"
  env -i HOME="$sandbox" PATH="/usr/bin:/bin" LANG=C.UTF-8 \
    timeout 20 "$sandbox/filewin-under-test" </dev/null >/dev/null 2>"$sandbox/err" && rc=0 || rc=$?
  # 就绪那一行在 stderr、带 `ccm-filewin-ready ` 打头（filewin-contract::READY_MARK；stdout 是通道），stderr 上别的行是诊断。
  line="$(sed -n 's/^ccm-filewin-ready //p' "$sandbox/err" 2>/dev/null | head -n 1 || true)"
  rm -rf "$sandbox"
  printf '%s %s' "$rc" "$line"
}

# 真起一趟本机那一份，回它 stdout 的第一行（起不来就回空串）。
#
# 为什么是「hello 帧」而不是 `--version` 之类：后端的流模式**第一件事**就是写一帧 hello
# （`build_hello`，带 `build_id`），而 monitor 起本机后端走的正是这条路 ⇒ 问 hello
# 问的就是「monitor 真去起它时会看到什么」。stdin 喂空 ⇒ 读到 EOF 它自己退。
#
# 隔离是承重的（起真后端的地方一律不许碰用户的真 tmux server）：
#   · `env -i`：一个环境变量都不继承 ⇒ `$TMUX` 不在，监听口那几个开关也不在（⇒ 走 stdio）；
#   · `HOME` 指一个空的临时目录 ⇒ 它看不到用户的 agent 家目录，一个字节都读不到；
#   · `PATH` 最前面挂一个**一律失败**的 `tmux` 替身 ⇒ tmux 那半只会被报成「观测不到」。
# ⚠ `timeout` 兜住「它起来了却不写 hello、也不退」那一形 —— 那一形同样是「起不来」。
native_starts() {
  local f="$1" sandbox line
  sandbox="$(mktemp -d)"
  mkdir -p "$sandbox/bin" "$sandbox/home"
  printf '#!/bin/sh\nexit 1\n' > "$sandbox/bin/tmux"
  cp "$f" "$sandbox/bin/backend-under-test"
  chmod +x "$sandbox/bin/tmux" "$sandbox/bin/backend-under-test"
  # `-- --stream`：argv 不以 `--` 加后端词打头时这份字节是 ccm（去 exec `claude`），问不到 hello ——
  #   monitor 起本机后端也是带着后端词起的（`local_backend.rs::STREAM_WORD`）。
  line="$(env -i HOME="$sandbox/home" PATH="$sandbox/bin:/usr/bin:/bin" LANG=C.UTF-8 \
            timeout 20 "$sandbox/bin/backend-under-test" -- --stream </dev/null 2>/dev/null | head -n 1 || true)"
  rm -rf "$sandbox"
  printf '%s' "$line"
}

# 一行 hello 帧里抠 `build_id`。抠不到给空串（调用方按红记）。
# 帧形：`{"kind":"hello","v":1,"build_id":"…",…}`。
hello_build_id() {
  printf '%s' "$1" | grep -q '^{"kind":"hello",' || return 0
  printf '%s' "$1" | sed -nE 's/.*"build_id":"([^"]*)".*/\1/p'
}

do_clean() {
  rm -rf "$EMBEDDED_DIR" "$NATIVE_DIR"
  printf '==> 已删两个内嵌落点 —— 自动部署与自释放诚实关闭，编译立刻恢复\n'
}

do_check() {
  # 1 = 开发构建的判词（`--check-dev`）：本机那一份缺席就是红。
  local require_native="${1:-0}"
  local id open close arch t f got present=0

  id="$(src_const BUILD_ID)"
  open="$(src_const STAMP_OPEN "$MARKS_SRC")"
  close="$(src_const STAMP_CLOSE "$MARKS_SRC")"

  if [ -n "$id" ]; then
    ok "源码身份抠得出（恰好一行）" "src/backend/lib.rs 的 const BUILD_ID = $id"
  else
    bad "源码身份抠得出（恰好一行）" \
        "在 src/backend/lib.rs 里抠不出**恰好一行** \`const BUILD_ID\` —— 身份又搬家了？住址是本文件顶上的 IDENTITY_SRC，与 release.yml 的 env.CCM_BACKEND_IDENTITY_SRC 同一份"
  fi
  if [ -n "$open" ] && [ -n "$close" ]; then
    ok "身份戳界标抠得出（各恰好一行）" "open=$open close=$close"
  else
    bad "身份戳界标抠得出（各恰好一行）" \
        "在 src/common/deploy-contract/src/lib.rs 里抠不出 STAMP_OPEN / STAMP_CLOSE —— 拿一对空界标去扫，对**任何**字节都答不出身份"
  fi

  # 人群从 `REEMBED_TARGETS` 派生，不从「扫落点目录」来（后者在目录被删空时零命中地全绿）。
  #   这一条与盘上有没有字节无关 ⇒ 是本命令在空树上不空真的那几条之一。
  ok "落点人群从 REEMBED_TARGETS 派生" \
     "${#REEMBED_TARGETS[@]} 个 target ⇒ ${REEMBED_TARGETS[*]}（＋本机内嵌那一份，若铺了）"

  if [ -z "$id" ] || [ -z "$open" ] || [ -z "$close" ]; then
    printf '::error:: 身份抠不出来 ⇒ 下面那几条**判不了，按红记**（绝不退化成「没有字节，于是绿」）\n'
    return 1
  fi

  for t in "${REEMBED_TARGETS[@]}"; do
    arch="${t%%-*}"
    f="$EMBEDDED_DIR/cc-monitor-backend-$arch"
    if [ ! -f "$f" ]; then
      printf 'skip  内嵌 backend %s :: 没铺（这棵树上没有这一份字节 —— 无半 bump 可言，也无自动部署）\n' "$arch"
      continue
    fi
    present=$((present + 1))
    got="$(bytes_id "$f" "$open" "$close")"
    if [ "$got" = "$id" ]; then
      ok "内嵌 backend $arch 与源码同一版" "字节自报 [$got] == 源码 [$id]"
    else
      bad "内嵌 backend $arch 与源码同一版" \
          "字节自报 [$got]，源码是 [$id] —— **半 bump**：装上去会被判 StaleBuild 并无限重装"
    fi
  done

  f="$NATIVE_DIR/cc-monitor-native"
  if [ -f "$f" ]; then
    present=$((present + 1))
    got="$(bytes_id "$f" "$open" "$close")"
    if [ "$got" = "$id" ]; then
      ok "本机内嵌后端与源码同一版" "字节自报 [$got] == 源码 [$id]"
    else
      bad "本机内嵌后端与源码同一版" \
          "字节自报 [$got]，源码是 [$id] —— **半 bump**：它会以 cc-monitor-backend-[$got] 之名落到用户盘上"
    fi
    # 起不起得来。只对「给这台机器编的」那一份问 —— 给别的 triple 编的那份
    #   在这里本来就起不来，而那一格 `build.rs` 的 ① 号硬校验已经会当场 panic。
    local staged host hello hid
    staged="$(tr -d '[:space:]' < "$NATIVE_DIR/cc-monitor-native.target" 2>/dev/null || true)"
    host="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')"
    if [ -n "$host" ] && [ "$staged" = "$host" ]; then
      hello="$(native_starts "$f")"
      hid="$(hello_build_id "$hello")"
      if [ -n "$hid" ] && [ "$hid" = "$id" ]; then
        ok "本机内嵌后端起得来（真起一趟，第一行是 hello）" "hello 自报 build_id [$hid] == 源码 [$id]"
      else
        bad "本机内嵌后端起得来（真起一趟，第一行是 hello）" \
            "第一行读作 [${hello:0:160}]，抠出的 build_id [$hid]，源码 [$id] —— 身份戳对得上 ≠ 起得来：monitor 真去起它时看到的就是这一行"
      fi
    else
      printf 'skip  本机内嵌后端起不起得来 :: 它是给 [%s] 编的、这台是 [%s] —— 不在这里起（错 triple 那一形由 build.rs 当场拦）\n' "$staged" "$host"
    fi
  elif [ "$require_native" = "1" ]; then
    bad "开发构建起得来本机后端（本机那一份在盘上）" \
        "src/frontend/shell/native-backend/cc-monitor-native 没铺 ⇒ 这棵树编出来的 exe **起不了本机后端**（D11：这不是「开发构建里的正常情况」）。出路：bash tests/scripts/re-embed.sh --native"
  else
    printf 'skip  本机内嵌后端 :: 没铺（这棵树编出来的 exe 起不了本机后端 —— 开发构建要它就跑 --native，判它用 --check-dev）\n'
  fi

  # 文件窗口程序：铺了、而且是给这台编的 ⇒ 真起一趟（`filewin_starts`）。没铺：只 skip（开窗走 exe 旁边那一份）。
  local wf="$NATIVE_DIR/cc-monitor-filewin" wstaged whost wout wrc wline wshape
  if [ -f "$wf" ]; then
    present=$((present + 1))
    wstaged="$(tr -d '[:space:]' < "$NATIVE_DIR/cc-monitor-filewin.target" 2>/dev/null || true)"
    whost="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')"
    if [ -n "$whost" ] && [ "$wstaged" = "$whost" ]; then
      wout="$(filewin_starts "$wf")"
      wrc="${wout%% *}"
      wline="${wout#* }"
      case "$wline" in
        '{"failed":'*) wshape=1 ;;
        *) wshape=0 ;;
      esac
      if [ "$wrc" != "0" ] && [ "$wshape" = "1" ]; then
        ok "内嵌文件窗口程序起得来（真起一趟，空种子）" "退出码 $wrc，就绪行 [${wline:0:80}]"
      else
        bad "内嵌文件窗口程序起得来（真起一趟，空种子）" \
            "退出码 [$wrc]，第一行读作 [${wline:0:160}] —— 该是一行 {\"failed\":…} 且非 0 退出：monitor 开窗时读不到就绪行"
      fi
    else
      printf 'skip  内嵌文件窗口程序起不起得来 :: 它是给 [%s] 编的、这台是 [%s] —— 不在这里起\n' "$wstaged" "$whost"
    fi
  else
    printf 'skip  内嵌文件窗口程序 :: 没铺（单文件的 monitor 只在旁边有它时开得了文件窗口）\n'
  fi

  if [ "$fail" -ne 0 ]; then
    printf -- '---- 红 %d 条 ----\n' "$fail"
    printf '::error:: 半 bump。出路二选一：① bash tests/scripts/re-embed.sh（重编重铺，同拍把账平掉）；② bash tests/scripts/re-embed.sh --clean（删掉落点，自动部署诚实关闭，编译立刻恢复）\n'
    return 1
  fi
  printf 're-embed: %d passed（分母 = 上面逐行印出来的 PASS 条数；盘上现打 %d 份内嵌字节。' "$pass" "$present"
  printf '⚠ 诚实边界：0 份时本行的绿只代表「这棵树上没有一份对不上的字节」，'
  printf '**不代表**「发版那一拍办完了」—— 本机 zigbuild 的形态与发版 CI 不同，'
  printf '也没有任何东西验过那份字节在真远端上跑得起来。逐条射程见本文件头注）\n'
  return 0
}

case "${1:---reembed}" in
  --check) do_check 0 ;;
  --check-dev) do_check 1 ;;
  --clean) do_clean ;;
  --native) do_native; do_check 1 ;;
  --reembed) do_build; do_check 0 ;;
  *)
    printf 'usage: %s [--check|--check-dev|--clean|--native]\n' "$0" >&2
    exit 2
    ;;
esac
