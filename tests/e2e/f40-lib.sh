# shellcheck shell=bash
# f40-suite.sh 与 f40-pick.test.sh 共用的几样：找主窗 · 读快照字段 · 读 tab 行坐标。只定义函数，source 它不跑任何东西。

# 快照（一行 JSON，stdin）里取一个顶层字段；没有 ⇒ null
jget() { python3 -c "import json,sys;v=json.loads(sys.stdin.read()).get('$1');print(v if v is not None else 'null')"; }

win() {
  # 取主窗口（1100x800 的 cc-monitor；tear-off 窗口名为项目名，不匹配）。只认映射了的窗口（`--onlyvisible`）：
  # GTK 的 client leader 窗也叫 cc-monitor（10×10、没映射、号比主窗小），不加这一条取到它、点在屏幕空白处。
  local w
  for w in $(xdotool search --onlyvisible --class monitor 2>/dev/null); do
    if [ "$(xdotool getwindowname "$w" 2>/dev/null)" = "cc-monitor" ]; then echo "$w"; return 0; fi
  done
  echo ""; return 1
}

# 快照（stdin）里第 $1 个 tab 行的点击坐标「x y」（窗口内设备像素，e2e-probe.ts 的 `rows`）；没有这一行 ⇒ 不印、非零
rowxy() {
  python3 -c '
import json, sys
rows = json.loads(sys.stdin.read()).get("rows") or []
i = int(sys.argv[1])
if not 0 <= i < len(rows):
    sys.exit(1)
print(rows[i]["x"], rows[i]["y"])' "$1"
}
