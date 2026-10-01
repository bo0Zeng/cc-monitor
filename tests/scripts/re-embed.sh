#!/usr/bin/env bash
# `BUILD_ID` bump 的**同拍步骤**：re-embed —— 把内嵌的那几份后端字节重编并铺回落点。
#
# ── 它治的病（步 `19c`）────────────────────────────────────────────
#
# 协议面一变就要 bump `src/backend/lib.rs` 的 `const BUILD_ID`（那份谱系里每一条都写着
# 「不 bump 就不判 stale、不重装，整条能力在已部署的远端休眠」）。而 bump 的那一刻，
# `src/frontend/shell/embedded-backends/` 里那两份 musl 字节**立刻变旧** —— 它们是上一版源码编的，
# 身份戳里写的还是上一个 id。于是二选一，两条都坏：
#
#   · 有人在盯（今天就是这样）⇒ `src/frontend/shell/build.rs::embed_backends` 的**半 bump 守卫**
#     当场 `panic!`，**整棵树编不过**。2026-09-18 实地踩过一次：删用量 ⇒ bump `p2j`→`p2k`
#     ⇒ **四路 agent 同时编不过**。
#   · 没人在盯 ⇒ 装出去的是一份自报旧 id 的字节，已部署的远端**不判 stale、不重装**，
#     整轮改动在那边休眠（那次事故的孪生形）。
#
# ⇒ 单子的裁定逐字：「**把 re-embed 写成 bump 的同拍步骤**，别让它变成一次事故。」
#   本文件就是那个步骤，**全仓唯一的本机产字节入口**。
#
# ── 它与那张 mtime 安全网的关系（别读混）──────────────────────────────────────
#
# `build.rs::embed_backends` 里有一张 mtime 安全网：取 `lib.rs` 与 `main.rs` 两份源码的
# **较新**那个 mtime，比内嵌字节新就打一条 `cargo:warning=内嵌 backend … 比后端源码旧`。
# 它**是安全网，不是机制**，三条都是构造性的：
#   ① 它只在**已经出事之后**说话（字节已经旧了）；
#   ② 它说的是「旧了」，**不说怎么办** —— 出路要人去读 panic 文案里那一段；
#   ③ 它一条 `warning`，在几百行 cargo 输出里滚过去（本仓自己的账：长输出里那行
#      `1 failed` 会滚过去，08-13 实测红着出过一次货）。
# ⇒ 本文件补的是它缺的那一半：**一条真跑得起来的命令**，以及一条 `--check`
#   ——「盘上这几份字节与源码的 `BUILD_ID` 对不对得上」由它**当场用相等断言回答**，
#   不必等谁去编整棵树、也不必在 cargo 的输出里找那一行。
# ⚠ 安全网**一条没撤**：mtime 那条 warning 与三处 panic 全部留着（判据 ⑬f 钉着它别被改瘦）。
#
# ── 🔴 一个事实一个住址：配方与 `release.yml` 逐字同源 ───────────────────────────
#
# 下面 `REEMBED_BUILD_FLAGS` / `REEMBED_TARGETS` 与 `.github/workflows/release.yml` 的
# `Cross-compile backend for both musl targets` 那一步**是同一条配方**，由
# `tests/evidence/K-R124-ruler.py` 的 ⑬b **每趟两向对拍**（target 集合相等 ＋ 旗标逐字相同）。
# ⚠ 这不是洁癖：在本文件之前，「本机怎么重编这两份字节」那条配方**手抄在 `build.rs` 的
#   panic 文案里**，写的是 `cargo build --release --target … --config linker="rust-lld"`
#   —— 与发版那趟的 `cargo zigbuild --release --locked` **不是同一条路**，而 09-18 那次
#   真正救活那棵树的恰恰是 zigbuild 那条。⇒ 那段手抄的配方已经撤掉（判据 ⑬e 盯着它别回来）。
#
# 🔧 **`zig cc` 是承重的，别换回 `rust-lld`**〔08-25 本机实测，读数从 `build.rs` 搬来〕：
#   引 `rustls` → `ring` 之后，`ring` 的 build script **要编 C**，而 `rust-lld` 只是链接器。
#   逐条读数：`x86_64` 照做 rc=0；`aarch64` 照做 **rc=101**，死在
#   `error occurred in cc-rs: failed to find tool "aarch64-linux-musl-gcc"`。
#   `cargo zigbuild` 自带 `zig cc` ⇒ 两个 arch 都不用另装 C 交叉工具链。
#
# ── ⚠⚠ 诚实边界（写死，别读宽）────────────────────────────────────────────────
#
# 1. 🔴 **本机重编 ≠ 发版那一拍办完了。** `build.rs` 自陈逐字：「这样编出来的形态与发版 CI 的
#    zigbuild 产物**不同** —— static-pie / 未 strip / 不同 rustc」。本机这台上的
#    `cargo-zigbuild` 与 `zig` 版本也未必是 `release.yml` 钉的那两个（门禁 `muslbuild`
#    那一格钉的是 zig 0.14.0 / cargo-zigbuild 0.23.0，而宿主上现打是 zig 0.16.0）。
#    ⇒ 本命令买到的是「**开发期自洽 ＋ 裸 exe 恢复部署能力**」，**不是**「这一版发得出去」。
# 2. **买不到「那份字节在真的远端 Linux 上跑得起来」** —— 本文件不运行它，没有真机。
# 3. `--check` 在**没铺字节**的树上（＝ clone 下来的默认状态、CI、绝大多数开发树）
#    只答得出「没有字节 ⇒ 无半 bump 可言」。⇒ 它那一趟的绿**不代表**字节是对的，
#    只代表「这棵树上没有一份对不上的字节」。⚠ 那正是「地板在『变少』方向上是瞎的」
#    这条纪律在本文件里的形状：人群**从 `REEMBED_TARGETS` 派生**，不从「扫落点目录、
#    扫到几份算几份」来 —— 后者在目录被删空时恒绿。
# 4. **它不判「这份字节是不是伪造的」** —— 逐字照抄 `sftp.rs::bytes_carry_build_stamp` 头注：
#    「⚠ 买不到：**防篡改**。谁都能往一段字节里塞一个假戳。它防的是漂移与手滑……
#      不防恶意 —— 那要签名，不是戳。」
#
# ── 🔴**开发构建也要起得来本机后端** ─────────────────────────────
#
# 横切纪律 `D11`（用户逐字「不要退路 / 所有东西都不要假设后端没起来」）。在这之前，
# 开发构建里「本机后端起不来」**只有一句 cargo 警告**，而那句警告自己写着
# 「开发构建里这是正常的」——于是开发构建上窗口「走后端」那条主路**一直在走退路**，
# 本机上验证任何后端功能都先天带着这个洞。
# ⇒ 修的是**构建这一侧**，不是运行期（运行期那条解析链「exe 旁 → 本机 Linux 用内嵌 musl
#   → 本产物自带的那份」一个字都没动，也**没有**多一条「起不来就换一条路」的分支）：
#   · **开发构建的路径是明写的，而且就是发版那一条**：`--native` 编出本机原生后端、
#     铺进 `src/frontend/shell/native-backend/`，`build.rs::embed_native_backend` 把它内嵌进 exe，
#     运行期按原路自释放再起。**配方与发版那两步逐字同源**（判据 ⑬b 钉着）。
#   · **「起得来」从一句警告变成可判的**：本文件现在**真起一趟**那份字节
#     （[`native_starts`]：空 stdin、隔离的 HOME、一个一律失败的 tmux 替身），
#     读它 stdout 的第一行 —— **必须是一帧 hello，且它自报的 `build_id` 等于源码**。
#     身份戳对得上 ≠ 起得来（戳是编进字节的一串字，跑不起来的字节照样带着它）。
#   · `--check-dev`：开发构建的判词。与 `--check` 同一套，唯一的差别是
#     **本机那一份缺席 = 红**（`--check` 里缺席是 `skip` —— 那是给 CI / 没铺字节的树的）。
#
# ── 跑法 ──────────────────────────────────────────────────────────────────────
#
#   bash tests/scripts/re-embed.sh             # 重编两份 musl 字节并铺回落点，铺完自检
#   bash tests/scripts/re-embed.sh --check     # 只问「盘上的字节与源码对不对得上 · 铺了的起不起得来」，不产字节
#   bash tests/scripts/re-embed.sh --check-dev # 同上，但**本机那一份缺席就是红**：开发构建起得来本机后端吗
#   bash tests/scripts/re-embed.sh --native    # 本机那几份（裸 exe 自带的后端 · 文件窗口程序）重编并重铺，铺完按 --check-dev 自检
#   bash tests/scripts/re-embed.sh --clean     # 守卫给的第二条出路：删掉落点（自动部署诚实关闭）
#
# 退出码 0 = 过；1 = 有对不上的 / 抠不到身份 / 编不过 / 铺了却起不来 /（`--check-dev`）本机那一份没铺。
# 最后一行恒印 `re-embed: <N> passed（…）`，`N` = 上面逐行印出来的 PASS 条数。
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"

#: 🔴 与 `release.yml` 那一步**逐字同源**（判据 `K-R124` ⑬b 两向对拍）。
REEMBED_BUILD_FLAGS=(--release --locked)
REEMBED_TARGETS=(x86_64-unknown-linux-musl aarch64-unknown-linux-musl)

#: 身份的唯一住址 —— 与 `build.rs::backend_lib_rs()` / `release.yml` 的
#: `env.CCM_BACKEND_IDENTITY_SRC` 是同一份文件（步 9 把它从 `main.rs` 搬来）。
IDENTITY_SRC="$ROOT/src/backend/lib.rs"
#: 内嵌落点 —— 名字定死在 `build.rs` 的 `EMBEDDED_BACKENDS_DIR` / `NATIVE_BACKEND_DIR`
#: （判据 ⑬c/⑬d 把这两处与本文件、与 `src/frontend/shell/.gitignore` 三向钉在一起）。
EMBEDDED_DIR="$ROOT/src/frontend/shell/embedded-backends"
NATIVE_DIR="$ROOT/src/frontend/shell/native-backend"

pass=0
fail=0

ok()  { printf 'PASS  %s :: %s\n' "$1" "$2"; pass=$((pass + 1)); }
bad() { printf 'FAIL  %s :: %s\n' "$1" "$2"; fail=$((fail + 1)); }

# 从身份那份源码里抠一个 `const <名>: &str = "…";`，**恰好一行**才算数。
# 抠不到给空串，调用方按红记 —— **不许兜底成一个会参与比较的字符串**
# （「`"unknown"` 这个值必须从类型上消失」，事故住）。
src_const() {
  local name="$1" hits
  hits="$(grep -cE "^[[:space:]]*(pub )?const ${name}: &str = \"[^\"]+\";" "$IDENTITY_SRC" || true)"
  [ "$hits" = "1" ] || return 0
  sed -nE "s/^[[:space:]]*(pub )?const ${name}: &str = \"([^\"]+)\";.*/\2/p" "$IDENTITY_SRC" | head -1
}

# 一份字节自报的身份：扫它里面那段定长戳 `<开><id><关>`。
# 🔴 **恰好一个才是身份**（`build.rs::bytes_build_id` 头注逐字：「多个 ＝ 身份不唯一，
#    两种都不许当成答案」）。0 个 / 多个都回一个说明串，它必然不等于任何真 `BUILD_ID`，
#    于是落在下面那条相等断言的红这一侧 —— 不静默、不兜底。
bytes_id() {
  local f="$1" open="$2" close="$3" found n s
  # 界标之间**至少一个字符**：两个界标常量在 `.rodata` 里挨着时会读出一个空 id 的「戳」（MIG-3b 之后实测 3 处），
  # 它不是身份 —— 与 `build.rs::bytes_build_id` · `deploy-contract::stamp_scan_cmd` 同一条（空串不收）。
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
# ⚠ 它**不进缺省那一趟**：19c 的病灶是远端那两份 musl（09-18 那次编不过的就是它们），
#   本机这一份只在 Windows job 上铺、开发树上缺席是常态（`build.rs` 那条 warning 逐字说了）。
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

# 真起一趟文件窗口程序（空 stdin），回「退出码 第一行 stdout」。它读不到开窗种子就在 stdout 上说一行 `{"failed":…}`
# 再退（非 0）—— 那一行正是 monitor 开窗时读的就绪行 ⇒ 读得到它 = 这份字节在这台机器上起得来。不开窗、不碰任何文件。
filewin_starts() {
  local f="$1" sandbox rc line
  sandbox="$(mktemp -d)"
  cp "$f" "$sandbox/filewin-under-test"
  chmod +x "$sandbox/filewin-under-test"
  env -i HOME="$sandbox" PATH="/usr/bin:/bin" LANG=C.UTF-8 \
    timeout 20 "$sandbox/filewin-under-test" </dev/null >"$sandbox/out" 2>/dev/null && rc=0 || rc=$?
  line="$(head -n 1 "$sandbox/out" 2>/dev/null || true)"
  rm -rf "$sandbox"
  printf '%s %s' "$rc" "$line"
}

# 🔴**真起一趟**本机那一份，回它 stdout 的第一行（起不来就回空串）。
#
# 为什么是「hello 帧」而不是 `--version` 之类：后端的流模式**第一件事**就是写一帧 hello
# （`build_hello`，带 `build_id`），而 monitor 起本机后端走的正是这条路 ⇒ 问 hello
# 问的就是「monitor 真去起它时会看到什么」。stdin 喂空 ⇒ 读到 EOF 它自己退。
#
# ⚠ **隔离是承重的**（本仓铁律：起真后端的地方一律不许碰用户的真 tmux server）：
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
# 帧形现打（本机 `--native` 那一份，2026-09-24）：`{"kind":"hello","v":1,"build_id":"…",…}`。
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
  open="$(src_const BUILD_STAMP_OPEN)"
  close="$(src_const BUILD_STAMP_CLOSE)"

  if [ -n "$id" ]; then
    ok "源码身份抠得出（恰好一行）" "src/backend/lib.rs 的 const BUILD_ID = $id"
  else
    bad "源码身份抠得出（恰好一行）" \
        "在 src/backend/lib.rs 里抠不出**恰好一行** \`const BUILD_ID\` —— 身份又搬家了？住址是本文件顶上的 IDENTITY_SRC，与 build.rs::backend_lib_rs() 同一份"
  fi
  if [ -n "$open" ] && [ -n "$close" ]; then
    ok "身份戳界标抠得出（各恰好一行）" "open=$open close=$close"
  else
    bad "身份戳界标抠得出（各恰好一行）" \
        "抠不出 BUILD_STAMP_OPEN / BUILD_STAMP_CLOSE —— 拿一对空界标去扫，对**任何**字节都答不出身份"
  fi

  # 🔴 **人群从 `REEMBED_TARGETS` 派生，不从「扫落点目录」来。**
  #    后者在目录被删空时零命中地全绿，正是本仓那句「地板在『变少』方向上是瞎的」。
  #    ⚠ 这一条与「盘上有没有字节」无关 ⇒ 它是本命令在空树上唯一非空真的那几条之一。
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
    # 🔴**起不起得来**。只对「给这台机器编的」那一份问 —— 给别的 triple 编的那份
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
