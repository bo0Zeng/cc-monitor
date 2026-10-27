#!/usr/bin/env bash
# f40-suite 点对地方的那几样（f40-lib.sh）：不起 app、不碰显示器，PATH 最前放一个假 xdotool。
#   - 找主窗：进程里有两个叫 cc-monitor 的窗口 —— GTK 的 client leader 窗（10×10、没映射、号小）和主窗；只认主窗。
#   - tab 行坐标：从快照的 `rows` 里取第 i 行的 x y（e2e-probe.ts 报的，窗口内设备像素），越界 ⇒ 不印、非零。
# 跑法：bash tests/e2e/f40-pick.test.sh
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=tests/e2e/f40-lib.sh
. "$HERE/f40-lib.sh"

pass=0; fail=0
ok()  { echo "  ✓ $1"; pass=$((pass+1)); }
bad() { echo "  ✗ $1"; fail=$((fail+1)); }

FAKE="$(mktemp -d)"; trap 'rm -rf "$FAKE"' EXIT
# 假 xdotool：2097153 是 leader 窗（未映射），2097156 是主窗；`--onlyvisible` 只回映射了的那个。
cat > "$FAKE/xdotool" <<'SH'
#!/usr/bin/env bash
case "$1" in
  search)
    case " $* " in
      *" --onlyvisible "*) echo 2097156 ;;
      *) printf '2097153\n2097156\n' ;;
    esac ;;
  getwindowname) echo cc-monitor ;;
  *) exit 1 ;;
esac
SH
chmod +x "$FAKE/xdotool"
PATH="$FAKE:$PATH"

echo "== f40 点对地方 =="
got=$(win)
[ "$got" = 2097156 ] && ok "找主窗：跳过没映射的 leader 窗，选中 2097156" || bad "找主窗：选中了 ${got:-<空>}（要 2097156，主窗）"

SNAP='{"sid":"abcd1234","rows":[{"x":96,"y":70,"active":true},{"x":96,"y":108,"active":false}],"activeRow":0}'
got=$(rowxy 1 <<<"$SNAP")
[ "$got" = "96 108" ] && ok "第 1 行坐标 96 108" || bad "第 1 行坐标：得 ${got:-<空>}（要 96 108）"
got=$(rowxy 0 <<<"$SNAP")
[ "$got" = "96 70" ] && ok "第 0 行坐标 96 70" || bad "第 0 行坐标：得 ${got:-<空>}（要 96 70）"
if got=$(rowxy 2 <<<"$SNAP"); then bad "越界那一行该非零退出（印了 ${got:-<空>}）"; else
  [ -z "$got" ] && ok "越界那一行：不印、非零" || bad "越界那一行印了 $got"; fi
if got=$(rowxy 0 <<<'{"active":null}'); then bad "快照没有 rows 该非零退出（印了 ${got:-<空>}）"; else ok "快照没有 rows：非零"; fi
got=$(jget activeRow <<<"$SNAP")
[ "$got" = 0 ] && ok "activeRow 取得到 0" || bad "activeRow：得 ${got:-<空>}"

echo "合计 PASS=$pass FAIL=$fail"
[ "$fail" -eq 0 ]
