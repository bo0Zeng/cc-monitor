#!/usr/bin/env python3
"""N-G1 的量具：拿一份**原始**失败输出，答 `NG1D1` 那三问。

住址（唯一定位到这一份）：`evidence/N-G1-diag-match.py`，被测对象是
`scripts/gate.sh` 里那一个具名常量 `GATE_DIAG_PAT` ＋ `gate_diag` 的①段取法。
它**不猜**模式表的内容：`--gate <path>` 从 gate.sh 里把 `GATE_DIAG_PAT=` 那一行现读出来
（闭集只许有一个住址 —— 这里只给住址，不复述成员）。

⚠ 射程（别读宽）：
  · 它量的是**行首匹配**这一件事，不量「诊断读起来好不好」。
  · 输入必须是**原始** stdout+stderr（`out="$(cmd 2>&1)"` 那一份）。
    **门禁已经加过 `  | ` 前缀的日志不算原始输出** —— 那正是 PM 09-05 栽的那一跤。
  · 去色的口径只盖 CSI（`ESC [ ... 字母`）。OSC（`ESC ] ... BEL`）不在射程里，
    今天量过的五形里一处都没有。

用法：
  python3 evidence/N-G1-diag-match.py --gate scripts/gate.sh <原始输出文件> [...]
  python3 evidence/N-G1-diag-match.py --gate scripts/gate.sh --show <一份> # 连①段实得一起印
"""
from __future__ import annotations

import argparse
import hashlib
import pathlib
import re
import subprocess
import sys

CSI = re.compile(rb"\x1b\[[0-9;:?]*[a-zA-Z]")


def read_pat(gate: pathlib.Path) -> str:
    """从 gate.sh 现读 `GATE_DIAG_PAT` —— 不在本文件里复述那张表。"""
    for line in gate.read_text(encoding="utf-8").splitlines():
        if line.startswith("GATE_DIAG_PAT="):
            body = line.split("=", 1)[1].strip()
            if body.startswith("'") and body.endswith("'"):
                return body[1:-1]
            raise SystemExit(f"GATE_DIAG_PAT 不是单引号字面量，量具认不出来：{line!r}")
    raise SystemExit(f"{gate} 里找不到 GATE_DIAG_PAT= —— 它搬家了，本量具跟着改")


def grep_hits(pat: str, data: bytes) -> int:
    """用 grep -E 现打命中数 —— 与 gate.sh 同一个引擎，不拿 python re 冒充它。"""
    p = subprocess.run(["grep", "-c", "-E", pat], input=data, capture_output=True)
    return int(p.stdout.strip() or 0)


def key_section(pat: str, data: bytes, cap: int) -> bytes:
    """复刻 gate_diag 的①段取法（`grep -E -A1 | grep -v '^--$' | head -n cap`）。"""
    a = subprocess.run(["grep", "-E", "-A1", pat], input=data, capture_output=True).stdout
    b = subprocess.run(["grep", "-v", "^--$"], input=a, capture_output=True).stdout
    c = subprocess.run(["head", "-n", str(cap)], input=b, capture_output=True).stdout
    return c


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--gate", required=True, help="scripts/gate.sh 的路径（模式表现读）")
    ap.add_argument("--cap", type=int, default=40, help="GATE_DIAG_KEY，默认 40")
    ap.add_argument("--show", action="store_true", help="把①段的实得逐行印出来")
    ap.add_argument("files", nargs="+")
    args = ap.parse_args()

    pat = read_pat(pathlib.Path(args.gate))
    print(f"模式表现读自 {args.gate}（长 {len(pat)} 字节，sha1 {hashlib.sha1(pat.encode()).hexdigest()[:12]}）")
    print(f"分母：本次量了 {len(args.files)} 份原始输出\n")
    hdr = f"{'原始输出':34} {'总行':>6} {'含ESC行':>7} {'改前命中':>8} {'去色后命中':>10}"
    print(hdr)
    print("-" * len(hdr))
    for f in args.files:
        data = pathlib.Path(f).read_bytes()
        total = data.count(b"\n")
        esc = sum(1 for ln in data.splitlines() if b"\x1b" in ln)
        before = grep_hits(pat, data)
        after = grep_hits(pat, CSI.sub(b"", data))
        print(f"{pathlib.Path(f).name:34} {total:6d} {esc:7d} {before:8d} {after:10d}")
        if args.show:
            print("  ---- 改前①段实得 ----")
            got = key_section(pat, data, args.cap)
            print("  （空 —— 一条都没匹配上）" if not got else
                  "\n".join("  | " + ln for ln in got.decode("utf-8", "replace").splitlines()))
            print("  ---- 改后①段实得（去色后）----")
            got = key_section(pat, CSI.sub(b"", data), args.cap)
            print("  （空 —— 一条都没匹配上）" if not got else
                  "\n".join("  | " + ln for ln in got.decode("utf-8", "replace").splitlines()))
            print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
