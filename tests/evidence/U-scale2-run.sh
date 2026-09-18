#!/usr/bin/env bash
# 秤 2（`设计/17 §6` 表第 2 行）的**一键复算**：打包探针 → 两个真引擎各跑一遍 →
# 出误差表 → 刷新门禁用的金标准。
#
# 真浏览器怎么起的（照抄 `真相源/90 §3.1/§3.7`，没有重新发明）：
#   · WebKitGTK 2.52.6  ——  PyGObject + gi WebKit2-4.1，必须 xvfb + 关合成，
#                            否则 web process 起不来。这是 Linux 侧 Tauri/wry 的同一引擎。
#   · Chromium 153      ——  Playwright 的 headless shell，生产 WebView2 的同引擎家族。
# 两个 runner 都直接复用 `/tmp/cv-reparent-probe/` 下 G 路留下的那两个脚本
# （`run_webkitgtk.py` / `run_chromium.mjs`，取值约定 `window.__DONE` + `window.__RESULT`）。
# ⚠ 它们住 /tmp，**是会被清掉的**。清掉之后见本文件末尾那段「从零重建」。
#
# 用法：bash tests/evidence/U-scale2-run.sh          # 从仓根跑
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORK=/tmp/scale2-height-truth
RIG=/tmp/cv-reparent-probe          # G 路留下的两个 runner

cd "$ROOT"
mkdir -p "$WORK"

# ── ① 语料 ────────────────────────────────────────────────────────────────
# 🔴 **默认不重采样。** 采样源 `~/.claude/projects/-home-zbl----claudecode-frontend/`
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

── runner 被清掉了怎么从零重建（两条都免 sudo、不进仓）────────────────────
  Chromium：  mkdir -p /tmp/cv-reparent-probe && cd /tmp/cv-reparent-probe \
              && npm i playwright && npx playwright install chromium
              （装到 ~/.cache/ms-playwright/，约 658MB；回收 rm -rf 该目录）
  WebKitGTK： 本机已有 libwebkit2gtk-4.1 + gi typelib WebKit2-4.1 + pygobject，
              零新增依赖；runner 就是 ~60 行 GTK3 + WebKit2，原文见 `真相源/90 §3.7` 的产物清单。
NOTE
