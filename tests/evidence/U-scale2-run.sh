#!/usr/bin/env bash
# 秤 2（表第 2 行）的**一键复算**：打包探针 → 两个真引擎各跑一遍 →
# 出误差表 → 刷新门禁用的金标准。
#
# 真浏览器怎么起的（照抄，没有重新发明）：
#   · WebKitGTK 2.52.6  ——  PyGObject + gi WebKit2-4.1，必须 xvfb + 关合成，
#                            否则 web process 起不来。这是 Linux 侧 Tauri/wry 的同一引擎。
#   · Chromium 153      ——  Playwright 的 headless shell，生产 WebView2 的同引擎家族。
# 两个 runner 不进仓（取值约定 `window.__DONE` + `window.__RESULT`），住 `SCALE2_RIG` 指的目录；
# 缺省是 `/tmp/cv-reparent-probe`，而 `/tmp` 在有的机器上是内存盘、**会被清掉** ⇒ 最好放到盘上、用环境变量指过来。
# 清掉之后见本文件末尾那段「从零重建」。
#
# 用法：bash tests/evidence/U-scale2-run.sh          # 从仓根跑
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# 两处都可由环境变量改到盘上（`/tmp` 在有的机器上是内存盘，在那里搭 runner 撞过整机 OOM）：
#   SCALE2_WORK=<探针产物与读数>  SCALE2_RIG=<两个 runner 所在目录>
WORK="${SCALE2_WORK:-/tmp/scale2-height-truth}"
RIG="${SCALE2_RIG:-/tmp/cv-reparent-probe}"          # 两个 runner 所在目录
export SCALE2_WORK="$WORK"

cd "$ROOT"
mkdir -p "$WORK"

# ── 🔴 显示时区与 locale：与门禁那一侧钉同一对值（09-19）───────────────────
# 语料 94 张卡**全部带时间戳**，渲染走 `src/frontend/ui/format.ts` 的 `toLocaleTimeString([], …)`
# ⇒ 这两个值一变，字面与字数都变，**真高跟着变**。不钉，就等于把「重打金标准那一刻
# 这台机器碰巧是什么设置」烤进金标准 —— 那正是 09-19 在断网沙箱里逮到的那一形
# （宿主永远绿，因为它就是打金标准那台机器；容器与 CI 的 runner 一律红）。
#
# 🔴 **这两行必须与 `vitest.config.ts` 顶层那两行逐字相同。** 探针会把现打值采进
#    金标准的 `env.displayLocale` / `env.timeZone`，而 `tests/frontend/ui/scale2-height-truth.vitest.ts`
#    有一条判据拿它跟运行时对拍 —— 三处对不上，那一条当场红并说「判不了」。
export TZ="America/Los_Angeles"
# ⚠ 取值与形式的理由逐条写在 `vitest.config.ts` 顶层那一段（`LANG` 的 POSIX 形 ＋ 清掉更高优先级的）。
unset LC_ALL LC_TIME
export LANG="zh_CN.UTF-8"

# ── ① 语料 ────────────────────────────────────────────────────────────────
# 🔴 **默认不重采样。** 采样源 `~/.claude/projects/-home-user----work/`
# 里有一个**正在被写的 jsonl（当前这次会话自己的）**，每跑一次采样器它都长大一点
# ⇒ 均匀取样挑出的记录随时间漂。本轮实测踩过：同一天两次跑，语料从 86 张卡变成 87 张，
# `card-assistant` 的 p90 从 100.8% 变成 96.9% —— **读数在动，而被测的代码一行没改**。
# ⇒ 语料一旦冻进 `tests/__fixtures__/`，就当金标准的一部分；要换语料是一次**显式**动作，
#   换完必须连真高一起重打（本脚本 ②-⑤ 会做）。
if [ "${1:-}" = "--resample" ]; then
  echo "── ① 重新采样（⚠ 会换掉语料，金标准跟着作废）"
  npx tsx tests/evidence/U-scale2-sample-records.ts
else
  echo "── ① 语料：用已冻结的 tests/__fixtures__/scale2-height-records.jsonl（--resample 才重采）"
fi

echo "── ② 打包探针（vite，产物只落 $WORK/dist）"
npx vite build --config tests/evidence/U-scale2-vite.config.ts

cat > "$WORK/probe.html" <<'HTML'
<!doctype html>
<html lang="zh">
<head>
<meta charset="utf-8">
<title>秤 2 · 估高 vs 真实布局高度</title>
<link rel="stylesheet" href="./dist/cc-monitor.css">
<style>html,body{margin:0;padding:0}</style>
</head>
<body>
<script src="./dist/probe.js"></script>
</body>
</html>
HTML

echo "── ③ Chromium 153（Blink，= 生产 WebView2 同引擎家族）"
node "$RIG/run_chromium.mjs" "$WORK/probe.html" "$WORK/result-chromium.json"

echo "── ④ WebKitGTK 2.52.6（= Linux 侧 Tauri/wry 同一引擎；必须 xvfb + 关合成）"
WEBKIT_DISABLE_COMPOSITING_MODE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1 \
  LIBGL_ALWAYS_SOFTWARE=1 GDK_BACKEND=x11 \
  xvfb-run -a -s "-screen 0 1280x1024x24" \
  python3 "$RIG/run_webkitgtk.py" "$WORK/probe.html" "$WORK/result-webkitgtk.json"

echo "── ⑤ 出表 + 刷新金标准（tests/evidence/U-scale2-truth-golden.json）"
npx tsx tests/evidence/U-scale2-report.ts

cat <<'NOTE'

── runner 被清掉了怎么从零重建（两条都免 sudo、不进仓；目录用 SCALE2_RIG 指过来）──────
  Chromium：  浏览器用 Playwright 缓存里的 headless shell：
              npx playwright install chromium-headless-shell（装到 ~/.cache/ms-playwright/）。
              runner 零 npm 依赖（~40 行）：自己起 headless shell（--no-sandbox · --hide-scrollbars ·
              --remote-debugging-port=0 · 视口 900×700 · dpr 1），用 Node 自带的 WebSocket 说 CDP，
              开 probe.html、轮询 window.__DONE、把 window.__RESULT 写出去。缓存目录认
              PLAYWRIGHT_BROWSERS_PATH（HOME 指到沙箱里跑时要设它），缺省 ~/.cache/ms-playwright。
  WebKitGTK： 本机已有 libwebkit2gtk-4.1 + gi typelib WebKit2-4.1 + pygobject，
              零新增依赖；runner 就是 ~60 行 GTK3 + WebKit2（同样视口 900×700，同样等 __DONE 取 __RESULT）。
NOTE
