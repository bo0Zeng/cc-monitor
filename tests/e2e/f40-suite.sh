#!/usr/bin/env bash
# 消息流虚拟化 E2E 套件。
#
# 前置(见 tests/e2e/README.md):
#   - Xvfb 显示器上跑着 `npx tauri dev`(debug 构建,DEV 探针已内建);
#   - 本机 monitor 日志可读(fe_perf/[e2e] 行是断言数据源——无 devtools 环境的唯一出口)。
# 断言:drain 阈值、冷开抖动密度、贴底、上翻补批账本下降、切 tab 贴底、合成 fork 会话折叠成段。
# 尾部优先(冷开只建视口那一截)由冷起补读的单测管(skeleton-view-cold-open.vitest.ts),这里不判。
# 退出码:全过 0 · 有败 1 · 台架不成立(这一趟没走冷起补读)2 —— 2 不算 PASS 也不记 FAIL,门禁照「非 0 即不绿」。
set -euo pipefail

DISPLAY="${E2E_DISPLAY:-:80}"
export DISPLAY
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
# 日志按 UTC 日期命名而 date +%F 是本地时区——跨午夜/异时区 shell 会拼错文件名
# (实测踩中:PDT shell vs UTC 命名)。默认取最新的 monitor.*.log。
LOG="${E2E_LOG:-$(ls -t "$HOME"/.cc-monitor/logs/monitor/monitor.*.log 2>/dev/null | head -1)}"
PROJ_DIR="$HOME/.claude/projects/-tmp-e2e-fork"
DRAIN_MAX_MS="${E2E_DRAIN_MAX_MS:-8000}"

[ -f "$LOG" ] || { echo "日志不存在:$LOG(dev 实例在跑吗?)"; exit 1; }

. "$REPO/tests/e2e/assert.sh"
# D 审计 R-1(两家共识):快照读取必须限定在本次触发之后的新增行——全量 grep 会在
# 触发静默失败(遮挡/焦点漂移)时消费上一轮的陈旧快照,断言假绿。SNAP_MARK 由
# snap_key 维护;触发后新增行里找不到快照 = 仪器失效,显式硬失败。
SNAP_MARK=0
snapline() { tail -n "+$((SNAP_MARK+1))" "$LOG" | grep "\[e2e\] snapshot" | tail -1 | sed 's/.*snapshot //'; }
# 找主窗 `win` · 取快照字段 `jget` · 取 tab 行点击点 `rowxy`（单测：f40-pick.test.sh）
# shellcheck source=tests/e2e/f40-lib.sh
. "$REPO/tests/e2e/f40-lib.sh"

echo "== F40 E2E 套件(display $DISPLAY) =="

# ── 0. 注入 fork fixture(reload 前,让重放带上它)────────────────────────
# watcher 只 emit **活跃**会话(Batch5-F20 假活跃修复:pidfile + PID 探活)——
# fixture 必须伴生活进程 pidfile,且 **pidfile 先落、jsonl 后落**(process_file
# 可能抢在 pidfile 之前跑,判非活跃后整文件静默跳过,实测踩过)
SID=$(python3 -c 'import uuid;print(uuid.uuid4())')
SID8="${SID:0:8}"
sleep 600 & HOST_PID=$!
# 起始时刻：先剥到最后一个 `) ` 再数（同 sandbox-env.sh 的 `proc_start`；本套件用真家目录，不 source 它）。
PROC_START=$(sed 's/.*) //' "/proc/$HOST_PID/stat" | awk '{print $20}')
PIDFILE="$HOME/.claude/sessions/$HOST_PID.json"
printf '{"pid":%s,"sessionId":"%s","cwd":"/tmp/e2e-fork","startedAt":%s,"procStart":"%s","kind":"interactive","entrypoint":"cli","status":"idle"}\n' \
  "$HOST_PID" "$SID" "$(date +%s%3N)" "$PROC_START" > "$PIDFILE"
cleanup_fixture() { rm -f "$PIDFILE"; kill "$HOST_PID" 2>/dev/null || true; rm -rf "$PROJ_DIR"; }
trap cleanup_fixture EXIT
sleep 1
python3 "$REPO/tests/e2e/gen-fork-session.py" "$PROJ_DIR" "$SID" >/dev/null
echo "fork fixture: $SID8 → $PROJ_DIR(宿主 pid=$HOST_PID)"

# ── 1. 触发整页 reload(HMR 全刷新→冷开),等冷起补读落盘 ──────────────────
# 冷起补读落地那一下探针停、落 `[e2e] jitter` 一行(只这一条路落它),同一刻落 fe_perf 汇总。
MARK=$(wc -l <"$LOG")
touch "$REPO/src/frontend/ui/main.ts"
echo "等待冷起补读落地([e2e] jitter + fe_perf 汇总)…"
COLD=0
for _ in $(seq 1 60); do
  # 先落变量再 grep(herestring 无 SIGPIPE;pipefail 下 grep -q 提前退出会误伤管道)
  PROBE_NEW=$(tail -n "+$((MARK+1))" "$LOG")
  PROBE_BLK=$(sed -n '/window=main/,/总耗时/p' <<<"$PROBE_NEW")
  if grep -q "\[e2e\] jitter" <<<"$PROBE_NEW" && grep -q "总耗时" <<<"$PROBE_BLK"; then COLD=1; break; fi
  sleep 2
done
if [ "$COLD" -ne 1 ]; then
  echo "台架不成立：这一趟没走冷起补读(120s 内没等到 [e2e] jitter 与主窗口汇总;DEV 构建?dev 实例在跑吗?)"
  exit 2
fi
NEW=$(tail -n "+$((MARK+1))" "$LOG")
# 主窗口块单独切片:tear-off/viewer 窗口 reload 会各自落一个 fe_perf 块(window=viewer-*),
# drained 行必须取 window=main 的那块
MAINBLK=$(sed -n '/window=main/,/总耗时/p' <<<"$NEW")

# ── 2. 启动断言:drain 阈值 / 冷开抖动密度 ───────────────────────────────
DRAIN=$(grep "batch payloads drained" <<<"$MAINBLK" | tail -1 | sed -E 's/.*\(\+([0-9]+)\).*/\1/')
[ "$DRAIN" -lt "$DRAIN_MAX_MS" ] && ok "drain ${DRAIN}ms < ${DRAIN_MAX_MS}ms" \
                                 || bad "drain ${DRAIN}ms 超阈值"
# 冷开抖动密度 = 密度绊线(标定见 src/frontend/ui/e2e-probe.ts 头注释):健康 ≈0.12-0.16 反转/帧,
# §21 病态 ≈1.0;断 ≤0.4——亚像素舍入摆动放行,逐帧震荡回归必绊。第 1 步已等到这一行。
JIT=$(grep "\[e2e\] jitter" <<<"$NEW" | tail -1)
FRAMES=$(sed -E 's/.*frames=([0-9]+).*/\1/' <<<"$JIT")
if [ "$FRAMES" -eq 0 ]; then
  ok "抖动无样本(冷开那一段 active tab 无内容——last-active 指向已消亡会话,环境态非回归)"
else
  DEN=$(sed -E 's/.*density=([0-9.]+).*/\1/' <<<"$JIT")
  python3 -c "exit(0 if float('$DEN') <= 0.4 else 1)" \
    && ok "冷开抖动密度=$DEN(≤0.4,frames=$FRAMES)" || bad "冷开抖动密度=$DEN 超绊线:$JIT"
fi

# ── 3. 贴底快照 ─────────────────────────────────────────────────────────
W=$(win) || { bad "找不到主窗口"; echo "FAIL"; exit 1; }
# tear-off/viewer 残留窗口可能盖住主窗(无 WM 无 z 序管理)——指针事件按 z 序命中,
# 必须先 raise 主窗,否则滚轮/点击全打到浮窗上(首跑实测)
xdotool windowraise "$W"; sleep 0.5
# XTEST 合成键盘进不了 WebKitGTK(实测),快照触发走鼠标:中键点状态栏
GEO=$(xdotool getwindowgeometry --shell "$W")
WX=$(sed -n 's/^X=//p' <<<"$GEO"); WY=$(sed -n 's/^Y=//p' <<<"$GEO")
WW=$(sed -n 's/^WIDTH=//p' <<<"$GEO"); WH=$(sed -n 's/^HEIGHT=//p' <<<"$GEO")
# 状态栏高 ~22px(styles.css #status-bar)→ 底边内缩 10px 必中;若改版需同步
SBX=$((WX + WW / 2)); SBY=$((WY + WH - 10))
snap_key() {
  SNAP_MARK=$(wc -l <"$LOG")
  xdotool mousemove --sync "$SBX" "$SBY" click 2; sleep 1
  tail -n "+$((SNAP_MARK+1))" "$LOG" | grep -q "\[e2e\] snapshot" \
    || { bad "快照触发未落地(中键丢失/遮挡?仪器失效,终止)"; echo "FAIL"; exit 1; }
}
snap_key
S1=$(snapline)
DIST=$(jget distBottom <<<"$S1"); ERR=$(jget err <<<"$S1"); P1=$(jget pending <<<"$S1")
[ "$DIST" -le 1 ] 2>/dev/null && ok "启动后贴底 distBottom=$DIST" || bad "未贴底:$S1"
[ "$ERR" = "null" ] && ok "状态栏无 ERR" || bad "状态栏错误:$ERR"

# ── 4. 上翻补批:滚动后账本下降、无错误 ───────────────────────────────────
TOP1=$(jget scrollTop <<<"$S1")
xdotool mousemove --sync $((WX + 650)) $((WY + 400)) # 消息流区（窗口内坐标，同 5.5）
xdotool click --repeat 300 --delay 4 4; sleep 2
snap_key
S2=$(snapline)
TOP2=$(jget scrollTop <<<"$S2"); P2=$(jget pending <<<"$S2"); ERR2=$(jget err <<<"$S2")
if [ "$P1" != "null" ] && [ "$P1" -gt 0 ] 2>/dev/null; then
  [ "$P2" -lt "$P1" ] 2>/dev/null && ok "上翻补批:pending $P1 → $P2" || bad "补批未触发:$S1 → $S2"
  # scrollTop 变化区分「滚轮没打进去」vs「补批坏了」(DoD)
  [ "$TOP2" != "$TOP1" ] && ok "滚动已发生:scrollTop $TOP1 → $TOP2" || bad "滚轮未生效:scrollTop 未变($TOP1)"
else
  ok "active tab 账本为空(小会话),补批场景由厚账 tab 段/单测覆盖(pending=$P1)"
fi
[ "$ERR2" = "null" ] && ok "补批后无 ERR" || bad "补批后错误:$ERR2"

# ── 5. 逐个点 tab:贴底 + 有内容;顺带找 fork fixture 断言折叠 ─────────────
# 键盘 Ctrl+Tab 同样进不了 webview,直接点竖直 tab 栏各行。行的点击点不写死像素:快照的 `rows` 是 e2e-probe.ts
# 当场量的(栏顶那排图标、「需手动」条、离线条都会把行往下推)。每点一行先看快照的 `activeRow` 是不是这一行、
# sid 跟着变了没有 —— 点到 tab 行以外(比如栏顶的「历史」,整页换走)当场报,不等下一次中键才报仪器失效。
pick_row() { # $1 = 第几行;照上一张快照 $S 里的点击点点它、再快照进 $S;没点中 ⇒ 硬失败
  local xy
  xy=$(rowxy "$1" <<<"$S") || { bad "快照里没有第 $1 个 tab 行:$S"; echo "FAIL"; exit 1; }
  xdotool mousemove --sync $((WX + ${xy% *})) $((WY + ${xy#* })) click 1; sleep 0.8
  snap_key; S=$(snapline)
  [ "$(jget activeRow <<<"$S")" = "$1" ] \
    || { bad "点第 $1 个 tab 行没点中(点到了 tab 行以外?):$S"; echo "FAIL"; exit 1; }
}
S=$S2
NROWS=$(python3 -c 'import json,sys;print(len(json.loads(sys.stdin.read()).get("rows") or []))' <<<"$S")
[ "$NROWS" -gt 0 ] || { bad "快照里没有 tab 行(探针没报 rows?页面不在会话视图?):$S"; echo "FAIL"; exit 1; }
FOUND_FORK=0; BIG_ROW=-1
PREV_ROW=$(jget activeRow <<<"$S"); PREV_SID=$(jget sid <<<"$S")
for i in $(seq 0 $((NROWS - 1))); do
  pick_row "$i"
  SIDN=$(jget sid <<<"$S"); TL=$(jget timeline <<<"$S"); DB=$(jget distBottom <<<"$S")
  if [ "$PREV_ROW" != "$i" ] && [ "$SIDN" = "$PREV_SID" ]; then
    bad "点第 $i 个 tab 行后快照的 sid 没变($SIDN):$S"
  fi
  PREV_ROW=$i; PREV_SID=$SIDN
  PN=$(jget pending <<<"$S")
  # 记住一个账本厚实的 tab(补批真实断言用)
  if [ "$BIG_ROW" -lt 0 ] && [ "$PN" != "null" ] && [ "$PN" -gt 250 ] 2>/dev/null; then BIG_ROW=$i; fi
  if [ "$SIDN" = "$SID8" ]; then
    FW=$(jget foldWraps <<<"$S")
    [ "$FW" -ge 1 ] 2>/dev/null && ok "fork fixture 折叠段 foldWraps=$FW" || bad "fixture 未折叠:$S"
    [ "$TL" -ge 4 ] 2>/dev/null && ok "fixture 渲染 timeline=$TL(≥4)" || bad "fixture 渲染不全:$S"
    FOUND_FORK=1
    [ "$BIG_ROW" -ge 0 ] && break
  fi
done
[ "$FOUND_FORK" -eq 1 ] || bad "循环切 tab 未找到 fixture($SID8)"
# 任取当前 tab 断言切换贴底
[ "$DB" -le 1 ] 2>/dev/null && ok "切 tab 后贴底 distBottom=$DB" || bad "切 tab 未贴底:$S"

# ── 5.5 真实补批断言:切到账本厚实的 tab,滚轮上翻,账本必须下降 ────────────
if [ "$BIG_ROW" -ge 0 ]; then
  pick_row "$BIG_ROW"
  B1=$S; PB1=$(jget pending <<<"$B1"); TB1=$(jget scrollTop <<<"$B1")
  xdotool mousemove --sync $((WX + 650)) $((WY + 400))
  xdotool click --repeat 300 --delay 4 4; sleep 2
  snap_key; B2=$(snapline); PB2=$(jget pending <<<"$B2"); EB=$(jget err <<<"$B2"); TB2=$(jget scrollTop <<<"$B2")
  [ "$PB2" -lt "$PB1" ] 2>/dev/null && ok "上翻补批(厚账 tab):pending $PB1 → $PB2" \
                                    || bad "厚账 tab 补批未触发:$B1 → $B2"
  [ "$TB2" != "$TB1" ] && ok "厚账滚动已发生:scrollTop $TB1 → $TB2" || bad "厚账滚轮未生效($TB1)"
  [ "$EB" = "null" ] && ok "厚账补批无 ERR" || bad "厚账补批错误:$EB"
else
  ok "未遇到厚账 tab(全部会话已渲染完/小会话)——补批由前段/单测覆盖"
fi

# ── 6. 清理 fixture(trap 兜底;tab 随宿主进程死亡归档、下次 reload 消失)──
cleanup_fixture; trap - EXIT
echo "fixture 已清理($PROJ_DIR + pidfile)"

finish
