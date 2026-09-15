#!/usr/bin/env python3
"""K-R126 变异台 —— 往 `relay/server.rs` 上切刀、并保证切得回来。

住址：`<本仓>/evidence/K-R126-cut.py`。被测对象 = **本文件所在的那棵工作树**
（`__file__` 现算，不收「树」参数；同一住址指两棵树是 `brief` 12 点名的静默假读数）。

纪律（`brief` 硬规则 7）：每一刀**先断言锚点恰好命中 N 次**，命中数不对就拒绝落地，
落地之后印一行「变异已落地」。备份住在 `/tmp/kr126-cut-<树名>-<路径指纹>/`，
`revert` 从备份整份还原（不走 `git checkout` —— 那会把同轮真正的修一起冲掉）。

用法：
  K-R126-cut.py list
  K-R126-cut.py apply <刀名> [<刀名> ...]
  K-R126-cut.py revert
  K-R126-cut.py status
"""
import hashlib
import pathlib
import shutil
import sys

WT = pathlib.Path(__file__).resolve().parent.parent
TARGET = WT / "remote-daemon-proto" / "src" / "relay" / "server.rs"
_fp = hashlib.sha256(str(WT).encode()).hexdigest()[:8]
BAK = pathlib.Path("/tmp") / f"kr126-cut-{WT.name}-{_fp}"

# ── 刀表 ──────────────────────────────────────────────────────────────────
# 每一刀：(锚点, 锚点该命中几次, 换上去的东西, 一句话)
CUTS: dict[str, tuple[str, int, str, str]] = {
    # ① 复现刀：把假上游的**每一块**都往后推 —— 这不是「让机器变慢」，
    #    是把 CI 上那个「桩写到一半、下游已经走了」的窗口**从概率变成必然**。
    "repro-slow-upstream": (
        "fn send_chunk(s: &mut TcpStream, sent: &AtomicU64, payload: &[u8]) {\n",
        1,
        "fn send_chunk(s: &mut TcpStream, sent: &AtomicU64, payload: &[u8]) {\n"
        "        std::thread::sleep(std::time::Duration::from_millis(300)); // K-R126 复现刀\n",
        "假上游每写一块前先睡 300ms ⇒ 下游（`head -n 1` 的桩启动器）必定已经走了",
    ),
}
# ──────────────────────────────────────────────────────────────────────────


def _ensure_bak():
    BAK.mkdir(parents=True, exist_ok=True)
    f = BAK / "server.rs.orig"
    if not f.exists():
        shutil.copy2(TARGET, f)
        print(f"· 备份已落：{f}")
    return f


def cmd_list():
    print(f"被测树：{WT}")
    print(f"被切文件：{TARGET}")
    for name, (_, n, _, why) in CUTS.items():
        print(f"  {name}  （锚点该命中 {n} 次）— {why}")


def cmd_status():
    src = TARGET.read_text(encoding="utf-8")
    print(f"被测树：{WT}")
    print(f"md5(server.rs) = {hashlib.md5(src.encode()).hexdigest()}")
    for name, (anchor, n, new, _) in CUTS.items():
        print(f"  {name}: 已落地={new in src} · 锚点现命中={src.count(anchor)}（该 {n}）")


def cmd_apply(names):
    _ensure_bak()
    src = TARGET.read_text(encoding="utf-8")
    for name in names:
        if name not in CUTS:
            sys.exit(f"❌ 没有这一刀：{name}")
        anchor, want, new, why = CUTS[name]
        hit = src.count(anchor)
        if hit != want:
            sys.exit(f"❌ 锚点命中 {hit} 次，该 {want} 次 ⇒ 这一刀不落地（{name}）")
        print(f"· 锚点断言过了：{name} 命中 {hit} 次（该 {want}）")
        src = src.replace(anchor, new, 1)
    TARGET.write_text(src, encoding="utf-8")
    for name in names:
        assert CUTS[name][2] in TARGET.read_text(encoding="utf-8")
        print(f"变异已落地：{name} —— {CUTS[name][3]}")
    print(f"md5(server.rs) = {hashlib.md5(TARGET.read_bytes()).hexdigest()}")


def cmd_revert():
    f = BAK / "server.rs.orig"
    if not f.exists():
        sys.exit(f"❌ 没有备份（{f}）—— 拒绝假装还原了")
    shutil.copy2(f, TARGET)
    f.unlink()
    print(f"· 已从备份整份还原：{TARGET}")
    print(f"md5(server.rs) = {hashlib.md5(TARGET.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    c = sys.argv[1]
    if c == "list":
        cmd_list()
    elif c == "status":
        cmd_status()
    elif c == "apply":
        cmd_apply(sys.argv[2:])
    elif c == "revert":
        cmd_revert()
    else:
        sys.exit(f"❌ 不认识：{c}")
