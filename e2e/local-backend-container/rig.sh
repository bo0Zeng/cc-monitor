#!/usr/bin/env bash
# **干净 Linux 容器台架** —— 把发版产物 `.deb` 装进一台干净 Linux 机，
# 然后回答一个问题：**用户装上之后，第一条命令能不能用。**
#
# ## 它答的是哪几件（件 `K-R139`）
#
#   `KR139D1` 后端起不起得来 · `ccm` 在**新 shell** 里敲不敲得到 · `--ccm-probe` 回不回得出六行
#   `KR139D2` `R80` 的 POSIX 那一臂：**证实 / 证伪 / 半通** 三选一
#
# ## 形状
#
#   一条**自建 `--internal` docker 网络** ── 一个具名容器（普通用户 `tester`）
#
# ## 🔴 网络策略（本台架自己裁的，理由写在这里）
#
# 选的是 **自建 `--internal` 网络**，不是 `--network none`，也不是放开出网。三条理由：
#   1. **`--internal` 没有出网** —— 被测的那一版够不着外面，也够不着用户这台机的网。
#      这一条**不是断言，是量出来的**：`P2` 里有一格真去连一个外部地址，连得上就判红。
#   2. **网络对象真的被建出来、又真的被删掉** —— `--network none` 用的是 docker 预置的
#      `none` 网络，什么都不用建也就什么都不用删 ⇒ 收尾那条「自建网络跑完没了」会变成
#      **空真**（闸死了照样绿）。自建一个，收尾那一格才有分母。
#   3. 代价如实写：`--internal` **测不到中转的上游那一跳**（relay / 远端部署 / 拉取）。
#      ⇒ 那几格本台架**不报**，归「没测到」，见件文件 `§3`。
#
# ## 跑完不留东西
#
# 容器、自建网络、临时目录跑完都删；宿主的 `docker network ls` 与 `ip link` 跑前跑后**逐字比**。
# 开跑前还会先查一遍「上一趟有没有留垃圾」——**刻意不做「先强删再建」**：那样清理这一步
# 被拆掉也照样绿（第二趟会替第一趟擦屁股），而这里要的正是「没清干净时第二趟当场红」。
# ⚠ 那条 `ip link` 逐字比有一个**已知假红**（件 `K-R136`）：带加速网卡的机器上 VF 会内核热插拔，
#   与本台架无关。撞上了照实报，**别去改这条自检**。
#
# ## 宿主上写了什么
#
# **一个字节都不写。** 产物由调用方事先下到 `$LBC_ARTIFACTS`（默认见下），
# 本脚本只读它、`docker cp` 进容器；临时目录用 `mktemp -d` 并在 `trap` 里删。
# 宿主上不跑任何一条被测命令（定框 `K31` 逐字：「以后所有开发测试都不允许直接在本机跑」）。
#
# 用法：
#   # 先把发版产物下下来（本脚本不联网、不替你下）。
#   # 🔴 **tag 与文件名都别手打** —— 手打的那一刻它就开始过期（见下面「被测的是哪一版」）：
#   #   gh release download "$(gh release view --repo <owner>/cc-monitor --json tagName -q .tagName)" \
#   #     --repo <owner>/cc-monitor --pattern '*_amd64.deb' --pattern 'monitor' --dir <某个目录>
#   bash e2e/local-backend-container/build-image.sh
#   LBC_ARTIFACTS=<某个目录> bash e2e/local-backend-container/rig.sh
#   bash e2e/local-backend-container/rig.sh --clean    # 只清上一趟崩掉时留下的容器/网络
#
# 环境变量：
#   LBC_IMAGE      默认 ccmon-lbc:latest（由 build-image.sh 建）
#   LBC_ARTIFACTS  发版产物所在目录（必须含 .deb；`monitor` 裸二进制可选）
#   LBC_DEB        .deb 的文件名。**不给就从 $LBC_ARTIFACTS 里发现**（见下）；
#                  给了就以它为准（要指名测某一份旧包时用）。
#
# ## 🔴 被测的是哪一版 —— **发现出来的，不是写死的**
#
# 这里原先写死 `cc-monitor_3.8.0_amd64.deb`（`K-R143` 09-15 逮到，同族共 5 处）。
# 它与 SPICE 那条 `listen`、门禁那条 `SKILL=` **是同一形：写死、今天恰好对、换一次就断**。
# 而这一处比那两处更坏，因为它**会静默地量错东西**：上面用法里那条 `gh release download`
# 也带着同一个版本号 ⇒ 照着文档走的人下回来的就是**旧包**，台架照跑照绿，
# 报告上写着「装机通过」，测的却是上一版。**「改成 3.8.1」只是把日期往后挪一格。**
#
# ⇒ 治法是**把那个字面量整个拿掉**：「测哪一份」这件事的事实**已经在盘上**——
#   调用方下进 `$LBC_ARTIFACTS` 的那一份就是。字面量只是它的第二住址，而第二住址会漂。
#   `$LBC_ARTIFACTS` 里**恰好一个** `.deb` ⇒ 就是它；**0 个或 ≥2 个 ⇒ 判红并列出实得**，
#   **不许静默挑一个**（静默挑 = 把「量错了」重新造出来）。被测文件名 · sha256 · 字节数
#   三样每趟都印，「这一趟测的是哪一版」永远在读数里。
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
IMG="${LBC_IMAGE:-ccmon-lbc:latest}"
NET="ccmon-lbc-net"
CT="ccmon-lbc-box"
CT_BARE="ccmon-lbc-bare"
ART="${LBC_ARTIFACTS:-}"
# 空 ＝ 「没点名」⇒ P1 里从 $ART 发现。**这里刻意没有版本号字面量**（理由见头注）。
DEB="${LBC_DEB:-}"

# 产品侧的两个落点（**只写在这里一处**，下面全部引用它）。
DIR_REAL='.cc-monitor/bin'      # install_local_ccm_entry 的真落点（local_backend.rs:1657）
DIR_SNIPPET='.local/bin'        # shared/ccm-aliases.sh 那行 PATH 加的那个

# 产品写 rc 时用的围栏（`src-tauri/src/sftp.rs` 的 CCM_PROFILE_BEGIN / _END 逐字）。
FENCE_BEGIN='# === cc-monitor remote ccm BEGIN ==='
FENCE_END='# === cc-monitor remote ccm END ==='

pass=0
fail=0
SNAP=""

line() { printf '%s\n' "------------------------------------------------------------"; }
ok()   { echo "  PASS $1"; pass=$((pass + 1)); }
no()   { echo "  FAIL $1"; fail=$((fail + 1)); }
chk()  { if [ "$2" = "$3" ]; then ok "$1"; else no "$1: 期望[$3] 实得[$2]"; fi; }
# `note` = **读数**，不是判定。三选一那类结论事先不知道答案，硬塞进 PASS/FAIL 就是预设结论。
note() { echo "  读数 $1"; }

cleanup_rig() {
  docker rm -f "$CT" "$CT_BARE" >/dev/null 2>&1
  docker network rm "$NET" >/dev/null 2>&1
}

on_exit() {
  cleanup_rig
  [ -n "$SNAP" ] && rm -rf -- "$SNAP"
  return 0
}

finish() { # $1 = 退出码
  echo "===== 合计 PASS=$pass FAIL=$fail ====="
  exit "$1"
}

if [ "${1:-}" = "--clean" ]; then
  cleanup_rig
  echo "[rig] 已清掉 $CT / $CT_BARE / $NET（若在）"
  exit 0
fi

command -v docker >/dev/null 2>&1 || { echo "需要 docker" >&2; exit 2; }

# 在容器里以**普通用户 tester** 跑一条登录+交互 shell。
# 🔴 `-lic` 不是 `-lc`：Ubuntu 的 `~/.bashrc` 头一行就是
#    `case $- in *i*) ;; *) return;; esac` —— **非交互 shell 会在读到我们那一段之前就 return**。
#    产品自己的探针（`ccm_probe.rs::probe_with`）用的也正是 `bash -lic`，这里与它同口径。
tsh() { docker exec -u tester -e HOME=/home/tester "$CT" bash -lic "$*" 2>/dev/null; }
# 对照口径：非交互登录 shell。两者**分开报** —— 差别本身就是一条读数。
tshl() { docker exec -u tester -e HOME=/home/tester "$CT" bash -lc "$*" 2>/dev/null; }
rsh() { docker exec -u root "$CT" bash -c "$*" 2>&1; }

# 敲一个名字，回 `FOUND <路径或类型>` / `NOTFOUND`。
# ⚠ 用 `command -v`：它认得**函数 / 别名 / PATH 上的文件**三样，而 `which` 只认第三样 ——
#   本件要判的「半通」那一档正好活在前两样里，用 `which` 会把它整档看不见。
probe_name() { # $1=名字 $2=口径函数名
  local n="$1" f="$2" out
  out="$($f "command -v $n" | head -1)"
  if [ -n "$out" ]; then printf 'FOUND %s' "$out"; else printf 'NOTFOUND'; fi
}

# ══════════════════════════════════════════════════════════════════
echo "== P0 判据：运行面不许出现宿主网络 / 全权限 / 加能力"
if bash "$HERE/guard-run-netns.sh"; then
  ok "运行面零命中宿主网络 / 全权限 / 加能力"
else
  no "运行面出现宿主网络或全权限 —— 那就不再是「隔离好」了"
  finish 9
fi
line

echo "== P1 前置：镜像 · 产物 · 残留 · 宿主快照"
if ! docker image inspect "$IMG" >/dev/null 2>&1; then
  echo "::error::没有镜像 $IMG ⇒ 先跑：bash e2e/local-backend-container/build-image.sh" >&2
  no "镜像 $IMG 不在"
  finish 2
fi
ok "镜像在：$IMG"

if [ -z "$ART" ] || [ ! -d "$ART" ]; then
  echo "::error::\$LBC_ARTIFACTS 没给或不是目录（实得[${ART:-空}]）。" >&2
  echo "::error::本脚本**刻意不联网下载** —— 产物由调用方事先下好，见头注用法。" >&2
  no "产物目录不在"
  finish 2
fi

# 被测 .deb：点名了就用点名的；没点名就**发现**。0 个或 ≥2 个一律判红并列出实得 ——
# 静默挑一个就是把「量的是上一版」这条病重新造出来（理由见头注「被测的是哪一版」）。
if [ -n "$DEB" ]; then
  note "被测 .deb 由 \$LBC_DEB 点名：$DEB"
else
  DEB_CAND=()
  while IFS= read -r f; do DEB_CAND+=("$f"); done \
    < <(find "$ART" -maxdepth 1 -type f -name '*.deb' -printf '%f\n' 2>/dev/null | LC_ALL=C sort)
  case "${#DEB_CAND[@]}" in
    1) DEB="${DEB_CAND[0]}"
       note "被测 .deb 是**发现**出来的（$ART 里恰好一个 .deb）：$DEB" ;;
    0) echo "::error::$ART 里一个 .deb 都没有 ⇒ 没东西可测。目录实得：$(find "$ART" -maxdepth 1 -type f -printf '%f ' 2>/dev/null)" >&2
       no "产物目录里没有 .deb"
       finish 2 ;;
    *) echo "::error::$ART 里有 ${#DEB_CAND[@]} 个 .deb ⇒ **不许替你挑**（挑错就是量了上一版）。" >&2
       echo "::error::实得：${DEB_CAND[*]}" >&2
       echo "::error::⇒ 用 LBC_DEB=<文件名> 点名，或把目录里只留下要测的那一份。" >&2
       no "产物目录里有 ${#DEB_CAND[@]} 个 .deb，分不出测哪一份"
       finish 2 ;;
  esac
fi
if [ ! -f "$ART/$DEB" ]; then
  echo "::error::点名的产物不在：\$LBC_ARTIFACTS/$DEB（LBC_ARTIFACTS=[$ART]）。" >&2
  no "产物不在"
  finish 2
fi
DEB_SHA="$(sha256sum "$ART/$DEB" | awk '{print $1}')"
ok "产物在：$DEB"
note "被测 .deb 的 sha256 = $DEB_SHA"
note "被测 .deb 的字节数 = $(stat -c %s "$ART/$DEB")"

leftover=""
for c in "$CT" "$CT_BARE"; do
  [ -n "$(docker ps -aq -f "name=^${c}$")" ] && leftover="$leftover $c"
done
[ -n "$(docker network ls -q -f "name=^${NET}$")" ] && leftover="$leftover $NET"
if [ -n "$leftover" ]; then
  no "上一趟没清干净，盘上还留着：$leftover"
  echo "::error::台架不替上一趟擦屁股（那会让「清理」这一步被拆掉也照样绿）。" >&2
  echo "::error::⇒ 先跑 bash e2e/local-backend-container/rig.sh --clean，再重跑。" >&2
  finish 7
fi
ok "盘上没有上一趟的残留"

SNAP="$(mktemp -d /tmp/ccmon-lbc-snap.XXXXXX)"
trap on_exit EXIT
docker network ls > "$SNAP/net.before" 2>&1
ip link            > "$SNAP/link.before" 2>&1
ok "宿主快照已取（docker network ls · ip link）"
line

# ══════════════════════════════════════════════════════════════════
echo "== P2 起台架：一条自建 --internal 网络 + 一个具名容器"
docker network create --internal "$NET" >/dev/null 2>&1 || { no "建自建网络失败"; finish 3; }
docker run -d --name "$CT" --network "$NET" "$IMG" sleep infinity >/dev/null 2>&1 \
  || { no "起容器失败"; finish 3; }

# ★ 正向读数：这一格是下面「跑完都没了」那几条的**分母** —— 第一次就没建出来的话，那几条会恒绿。
chk "自建网络确实建出来了（$NET）" "$(docker network ls -q -f "name=^${NET}$" | wc -l)" "1"
chk "容器确实在跑（$CT）" "$(docker inspect -f '{{.State.Running}}' "$CT" 2>/dev/null)" "true"

# 🔴 隔离是**量出来的**，不是写在头注里就算数的。
#   往一个公网地址发一个 TCP SYN，3 秒超时。`--internal` 之下这一定要失败。
EGRESS="$(docker exec "$CT" timeout 3 bash -c ': >/dev/tcp/1.1.1.1/443' 2>&1; echo "rc=$?")"
if printf '%s' "$EGRESS" | grep -q 'rc=0'; then
  # ⚠ 这一句里**不许用反引号写 --internal**：双引号串里的反引号是命令替换，
  #   bash 会真的去执行 `--internal`（SC2215 逮的就是它）。用单引号形的排版代替。
  no "容器居然出得了网（$EGRESS）—— 自建网络的 --internal 没起作用，这就不是隔离"
else
  ok "容器出不了网（实得 $EGRESS）—— 隔离是量出来的，不是声称的"
fi
line

# ══════════════════════════════════════════════════════════════════
echo "== P3 一台**裸** ubuntu:24.04 上 dpkg -i 会怎样（依赖这一格单独量）"
docker run -d --name "$CT_BARE" --network "$NET" ubuntu:24.04 sleep infinity >/dev/null 2>&1 \
  || { no "起裸容器失败"; finish 3; }
docker cp "$ART/$DEB" "$CT_BARE:/tmp/$DEB" >/dev/null 2>&1
BARE_OUT="$(docker exec -u root "$CT_BARE" dpkg -i "/tmp/$DEB" 2>&1; echo "rc=$?")"
BARE_RC="$(printf '%s' "$BARE_OUT" | tail -1)"
note "裸机 dpkg -i 退出码：$BARE_RC"
printf '%s\n' "$BARE_OUT" | sed 's/^/    | /' | tail -12
note "裸机 dpkg -i 之后 /usr/bin/monitor 在不在：$(docker exec "$CT_BARE" sh -c '[ -e /usr/bin/monitor ] && echo 在 || echo 不在')"
docker rm -f "$CT_BARE" >/dev/null 2>&1
line

# ══════════════════════════════════════════════════════════════════
echo "== P4 依赖齐了的机器上装 .deb"
docker cp "$ART/$DEB" "$CT:/tmp/$DEB" >/dev/null 2>&1
INS_OUT="$(rsh "dpkg -i /tmp/$DEB; echo rc=\$?")"
INS_RC="$(printf '%s' "$INS_OUT" | tail -1)"
printf '%s\n' "$INS_OUT" | sed 's/^/    | /' | tail -8
chk ".deb 装上了" "$INS_RC" "rc=0"
note "装完之后 dpkg -L 落了几个文件：$(rsh 'dpkg -L cc-monitor | wc -l' | tr -d '\r')"
note "落在 PATH 目录（/usr/bin）下的：$(rsh 'dpkg -L cc-monitor | grep "^/usr/bin/" | tr "\n" " "')"
# 🔴 这一格是本件的要害之一：**包里有没有一个叫 ccm 的东西**。
CCM_IN_DEB="$(rsh 'dpkg -L cc-monitor | grep -c "/ccm$"' | tr -d '\r')"
note ".deb 里名字恰好是 ccm 的文件数 = $CCM_IN_DEB（分母 = dpkg -L 全部条目）"
line

# ══════════════════════════════════════════════════════════════════
echo "== P5 **新 shell** 里敲三个名字（装完 .deb、还没跑过 app）"
for n in ccm cc cct; do
  note "[-lic 交互] $n ⇒ $(probe_name "$n" tsh)"
done
for n in ccm cc cct; do
  note "[-lc 非交互] $n ⇒ $(probe_name "$n" tshl)"
done
note "此刻 \$HOME/$DIR_REAL 在不在：$(tsh "[ -d \$HOME/$DIR_REAL ] && echo 在 || echo 不在")"
note "此刻 \$HOME/$DIR_SNIPPET 在不在：$(tsh "[ -d \$HOME/$DIR_SNIPPET ] && echo 在 || echo 不在")"
note "此刻 tester 的 PATH：$(tsh 'echo $PATH')"
line

# ══════════════════════════════════════════════════════════════════
echo "== P6 起一次 app（Xvfb）—— 这是把 ccm 放下来的**那条生产路径**"
# `install_local_ccm_entry` 的唯一生产调用点在 `start_or_extract` 里（local_backend.rs:1657），
# 而它由 app 启动时的 `start_local_backend()` 带起来（lib.rs）⇒ **不起 app 就测不到真落点**。
docker exec -u tester -e HOME=/home/tester \
  -e WEBKIT_DISABLE_COMPOSITING_MODE=1 -e WEBKIT_DISABLE_DMABUF_RENDERER=1 \
  -d "$CT" bash -lc \
  'xvfb-run -a dbus-run-session -- /usr/bin/monitor >/tmp/app.log 2>&1' >/dev/null 2>&1
# 给它一点时间把后端释放出来并起进程。
sleep 20
note "app 进程数（pgrep -c monitor）：$(docker exec "$CT" sh -c 'pgrep -c -x monitor || echo 0' | tr -d '\r')"
note "后端进程数（pgrep -cf cc-monitor-remote）：$(docker exec "$CT" sh -c 'pgrep -cf cc-monitor-remote || echo 0' | tr -d '\r')"
note "\$HOME/$DIR_REAL 里现在有什么：$(tsh "ls -1 \$HOME/$DIR_REAL 2>/dev/null | tr '\n' ' '")"
note "app 日志尾（/tmp/app.log 末 6 行）："
docker exec "$CT" sh -c 'tail -6 /tmp/app.log 2>/dev/null' | sed 's/^/    | /'
note "监听口：$(docker exec "$CT" sh -c 'ss -ltnp 2>/dev/null | tail -n +2 | wc -l' | tr -d '\r') 个"
line

# ══════════════════════════════════════════════════════════════════
echo "== P7 **新 shell** 里再敲一遍（app 跑过之后、还没装 rc 片段）"
for n in ccm cc cct; do
  note "[-lic 交互] $n ⇒ $(probe_name "$n" tsh)"
done
note "\$HOME/$DIR_REAL 在不在：$(tsh "[ -d \$HOME/$DIR_REAL ] && echo 在 || echo 不在")"
note "\$HOME/$DIR_SNIPPET 在不在：$(tsh "[ -d \$HOME/$DIR_SNIPPET ] && echo 在 || echo 不在")"
note "rc 里有没有产品的围栏：$(tsh "grep -c 'cc-monitor remote ccm BEGIN' \$HOME/.bashrc 2>/dev/null || echo 0") 处"
line

# ══════════════════════════════════════════════════════════════════
echo "== P8 把产品那段 rc 片段装进去（逐字 shared/ccm-aliases.sh + 产品的围栏），再敲"
# ⚠ 这一步模拟的是**用户在设置面板里点了「装 ccm 别名块」**之后 rc 的样子。
#   片段取自**本树的** `shared/ccm-aliases.sh`（后端 `sftp.rs` 也是 `include_str!` 同一份文件
#   ⇒ 这里与产品写进去的那一份同源），**一个字节都不改**。
SNIPPET="$HERE/../../shared/ccm-aliases.sh"
if [ -f "$SNIPPET" ]; then
  note "rc 片段取自 $SNIPPET（md5 $(md5sum "$SNIPPET" | awk '{print $1}')）"
  docker cp "$SNIPPET" "$CT:/tmp/ccm-aliases.sh" >/dev/null 2>&1
fi
if docker exec "$CT" sh -c '[ -f /tmp/ccm-aliases.sh ]' 2>/dev/null; then
  docker exec -u tester -e HOME=/home/tester "$CT" bash -c \
    "printf '\n%s\n' '$FENCE_BEGIN' >> \$HOME/.bashrc && cat /tmp/ccm-aliases.sh >> \$HOME/.bashrc && printf '%s\n' '$FENCE_END' >> \$HOME/.bashrc" >/dev/null 2>&1
  ok "rc 片段已装（围栏逐字取自 sftp.rs 的 CCM_PROFILE_BEGIN/_END）"
  note "装完之后 rc 里围栏处数：$(tsh "grep -c 'cc-monitor remote ccm BEGIN' \$HOME/.bashrc")"
  for n in ccm cc cct; do
    note "[-lic 交互] $n ⇒ $(probe_name "$n" tsh)"
  done
  note "装完之后 tester 的 PATH：$(tsh 'echo $PATH')"
  # 🔴 「找得到」不等于「跑得动」：`cc` / `cct` 是 shell 函数，`command -v` 对它们恒真，
  #    而它们体内调的是 `ccm`。**真敲一下**才知道那一层通不通。
  note "真敲 cc（退出码 + 首行）：$(tsh 'cc --help >/dev/null 2>&1; echo rc=$?')"
  note "真敲 ccm（退出码）：$(tsh 'ccm --help >/dev/null 2>&1; echo rc=$?')"
else
  no "本树里没有 shared/ccm-aliases.sh（找的是 $SNIPPET）—— P8 整段没跑"
fi
line

# ══════════════════════════════════════════════════════════════════
echo "== P9 --ccm-probe 那六行"
# 两条路分开问（`ccm_probe.rs` 头注逐字：这两条问的不是同一件事）：
#   ① 直接问**我们放下去的那一份**（路径我们自己知道，不经 shell）
#   ② 问**用户 PATH 上那个**（非走登录 shell 不可）
P_DIRECT="$(tsh "\$HOME/$DIR_REAL/ccm --ccm-probe 2>/dev/null")"
note "① 直接问 \$HOME/$DIR_REAL/ccm：行数 = $(printf '%s' "$P_DIRECT" | grep -c . )"
printf '%s\n' "$P_DIRECT" | sed 's/^/    | /'
P_PATH="$(tsh 'command -v ccm >/dev/null 2>&1 && ccm --ccm-probe 2>/dev/null || printf NO_CCM\\n')"
note "② 问 PATH 上那个（产品探针 CCM_PROBE_CMD 的口径）：行数 = $(printf '%s' "$P_PATH" | grep -c . )"
printf '%s\n' "$P_PATH" | sed 's/^/    | /'
# ③ 包里那个 /usr/bin/cc-monitor-remote 自己答不答（argv[0] 不叫 ccm）
# 🔴 **必须带 timeout** —— 09-15 头一趟就是在这里挂住的：`argv[0]` 不叫 `ccm` 时它**不走 ccm 那一支**
#    （`control/ccm/mod.rs:16` 逐字：basename 是 `ccm` 才进），`--ccm-probe` 既不被认、也不让它退，
#    进程就那么停着（实测 12 分半没退，最后手工 `kill -9` 才放行）。
#    ⚠ 不带 timeout 的话，「它不答」这一格会表现成**整个台架挂死**，而挂死读不出是哪一格坏了。
P_SIDE="$(timeout 10 docker exec "$CT" /usr/bin/cc-monitor-remote --ccm-probe 2>&1 | head -8)"
SIDE_RC=$?
note "③ 问 /usr/bin/cc-monitor-remote（argv[0] 不叫 ccm）：行数 = $(printf '%s' "$P_SIDE" | grep -c . ) · timeout 退出码 = $SIDE_RC（124 = 10 秒内没退）"
printf '%s\n' "$P_SIDE" | sed 's/^/    | /'
# 起的那个进程不会自己走 ⇒ 台架自己收尸，别留给收尾那一步
docker exec "$CT" pkill -f -- '--ccm-probe' >/dev/null 2>&1
line

# ══════════════════════════════════════════════════════════════════
echo "== P10 卸载 → 看残留"
rsh "dpkg -r cc-monitor >/dev/null 2>&1; echo rc=\$?" | sed 's/^/    | /'
note "卸完 /usr/bin/monitor 在不在：$(docker exec "$CT" sh -c '[ -e /usr/bin/monitor ] && echo 在 || echo 不在')"
note "卸完 /usr/bin/cc-monitor-remote 在不在：$(docker exec "$CT" sh -c '[ -e /usr/bin/cc-monitor-remote ] && echo 在 || echo 不在')"
note "卸完 \$HOME/$DIR_REAL 在不在：$(tsh "[ -d \$HOME/$DIR_REAL ] && echo 在 || echo 不在")"
note "卸完 \$HOME/$DIR_REAL 里还剩：$(tsh "ls -1 \$HOME/$DIR_REAL 2>/dev/null | tr '\n' ' '")"
note "卸完 rc 里围栏还剩：$(tsh "grep -c 'cc-monitor remote ccm BEGIN' \$HOME/.bashrc 2>/dev/null || echo 0") 处"
line

# ══════════════════════════════════════════════════════════════════
echo "== P11 收尾：自己清理，宿主逐字复原"
cleanup_rig
sleep 1
chk "容器跑完没了" "$(docker ps -aq -f "name=^${CT}$" | wc -l)" "0"
chk "裸容器跑完没了" "$(docker ps -aq -f "name=^${CT_BARE}$" | wc -l)" "0"
chk "自建网络跑完没了" "$(docker network ls -q -f "name=^${NET}$" | wc -l)" "0"

docker network ls > "$SNAP/net.after" 2>&1
ip link            > "$SNAP/link.after" 2>&1
if diff -q "$SNAP/net.before" "$SNAP/net.after" >/dev/null 2>&1; then
  ok "宿主 docker network ls 跑前跑后逐字相同"
else
  no "宿主 docker network ls 变了：$(diff "$SNAP/net.before" "$SNAP/net.after" | tr '\n' ' ')"
fi
if diff -q "$SNAP/link.before" "$SNAP/link.after" >/dev/null 2>&1; then
  ok "宿主 ip link 跑前跑后逐字相同"
else
  no "宿主 ip link 变了（⚠ 已知假红 K-R136：加速网卡 VF 热插拔）：$(diff "$SNAP/link.before" "$SNAP/link.after" | tr '\n' ' ')"
fi

TMPDIR_KEEP="$SNAP"
rm -rf -- "$SNAP"
if [ -d "$TMPDIR_KEEP" ]; then
  no "临时目录没删掉：$TMPDIR_KEEP"
else
  ok "临时目录跑完没了"
fi
line

[ "$fail" -eq 0 ] && finish 0
finish 1
