#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""cc-bus 入口那一侧的判据本体:**每个动词只把一句交给后端**(A13-8)。

收件箱 · 已读位置 · 发信的规矩(信封 · 组 · 去重 · 限流 · 敲不敲)都住 cc-monitor 后端;两阶段读的行为
(令牌 · CAS · 截短自愈 · 分段)由后端单测钉(`tests/backend/control/cc_bus_store_tests.rs` · `cc_bus_rules_tests.rs`),
真跑在 `tests/e2e/backend-cc-bus.sh`。这一份只判脚本那一侧还成立的那几句:

  · `T1` **零数据面**:剥注释后,全 cc-bus 脚本里收件箱 · 已读位置 · 总线家目录的字样(`.jsonl` · `.pos` · `$BUS` ·
        `CC_BUS_HOME` · `inbox/`)零命中;**正控**:同一个认法在一段合成的老脚本上认得出。
  · `T2` **零锁 · 零规矩**:`flock` · `route_` 零命中(正控同上)。
  · `T3` **只经后端**:各动词本体问后端的动词(`ccbus_ask <动词>`)集合 == 登记表(两向相等);
        `whoami` 只问 `--session-self`。
  · `T4` **两边对得上**:脚本问的那几个动词 == 后端登记的 `bus-*` 帧命令(读 `control/cc_bus_wire.rs` 的 `NAME`,两向相等)。
  · `T5` **`recv` 就是 peek 接 commit**:入口 `cc-bus` 的 recv 那一臂先调 peek、再调 commit,入口里不渲染。
  · `T6` **手册页写了那几句**:`SKILL.md` 里 commit 的四个码(0 · 2 · 11 · 12)·「照常处理手上那段」· at-least-once。

**买不到**:不判后端那一侧对不对(那是 Rust 单测与 e2e 的事);注释只按「整行以 `#` 起头 ＋ 行尾 ` #`」剥,
here-doc 正文里的 `#` 不认(人群里只有入口的 usage 那一段 here-doc,里面没有被扫的词)。

## 跑法

    python3 tests/evidence/W24C-ccbus-twophase-ruler.py

`W24C_ROOT=<副本>` 只为死值验存在:把仓拷进草稿目录变异之后对着副本跑,不必动真工作树。日常跑一律不带。
"""

import os
import re
import sys
from pathlib import Path

ROOT = Path(os.environ.get("W24C_ROOT") or Path(__file__).resolve().parents[2])
SCRIPTS = ROOT / "src" / "shared" / "cc-bus" / "scripts"
SKILL = ROOT / "src" / "shared" / "cc-bus" / "SKILL.md"
WIRE = ROOT / "src" / "backend" / "control" / "cc_bus_wire.rs"

PASSED = []
FAILED = []


def ok(tag, msg):
    PASSED.append(tag)
    print(f"  ✓ {tag} {msg}")


def bad(tag, msg):
    FAILED.append(tag)
    print(f"  ✗ {tag} {msg}")


# ── 登记:本尺子该打出哪些标签(反空真的锚:两向集合对拍,不只是数数)──────────────
EXPECTED_TAGS = ["T1", "T2", "T3", "T4", "T5", "T6"]

# 各动词本体问后端的那一个动词(`ccbus_ask <动词>`)。入口 · 库 · 词典 · 安装脚本 · whoami · Stop 钩子不问(钩子经 peek / commit 两个本体)。
ASKS = {
    "cc-bus-send.sh": {"send"},
    "cc-bus-peek.sh": {"peek"},
    "cc-bus-commit.sh": {"commit"},
    "cc-bus-unread.sh": {"unread"},
    "cc-bus-list.sh": {"list"},
    "cc-bus-log.sh": {"inbox"},
}

DATA_WORDS = [r"\.jsonl\b", r"\.pos\b", r"\$BUS\b", r"\bCC_BUS_HOME\b", r"\binbox/"]
LOCK_WORDS = [r"\bflock\b", r"\broute_"]
# 正控:一段合成的老脚本(就地写在这里,不读仓里的历史),上面那两张词表各自都得认得出。
OLD_SHAPE = 'route_deliver() { ( flock 9; printf x >> "$BUS/inbox/$to.jsonl" ) 9>>"$f.lock"; echo 1 > "$BUS/state/$id.pos"; } # CC_BUS_HOME'


def strip_comments(text):
    out = []
    for line in text.splitlines():
        s = line.lstrip()
        if s.startswith("#"):
            continue
        out.append(re.sub(r"\s#\s.*$", "", line))
    return "\n".join(out)


def scripts():
    files = sorted(p for p in SCRIPTS.iterdir() if p.is_file() and (p.name == "cc-bus" or p.suffix == ".sh"))
    return {p.name: strip_comments(p.read_text(encoding="utf-8")) for p in files}


def hits(words, text):
    return [w for w in words if re.search(w, text)]


def check_words(tag, words, what):
    src = scripts()
    if len(src) < 10:
        bad(tag, f"只列到 {len(src)} 份脚本 —— 取法坏了,下面是空转")
        return
    ctrl = hits(words, OLD_SHAPE)
    if sorted(ctrl) != sorted(words):
        bad(tag, f"正控没过:合成的老脚本上只认出 {ctrl} —— 认法看不见那几个词,零命中是空真")
        return
    found = {n: hits(words, t) for n, t in src.items()}
    found = {n: w for n, w in found.items() if w}
    if found:
        bad(tag, f"脚本里还有{what}:{found}")
    else:
        ok(tag, f"{len(src)} 份脚本 × {len(words)} 个词 = 0 命中({what});正控认得出")


def check_t3():
    src = scripts()
    got = {}
    for n, t in src.items():
        verbs = set(re.findall(r"\bccbus_ask\s+([a-z]+)\b", t))
        if verbs:
            got[n] = verbs
    who = src.get("cc-bus-whoami.sh", "")
    if got != ASKS:
        bad("T3", f"问后端的动词与登记对不上:现打 {got},登记 {ASKS}")
    elif "--session-self" not in who or "ccbus_ask" in who:
        bad("T3", "whoami 不是只问 `--session-self`")
    else:
        ok("T3", f"{len(ASKS)} 个动词本体各只问后端它自己那一条;whoami 只问 --session-self")


def check_t4():
    names = set(re.findall(r'const NAME: &\'static str = "bus-([a-z]+)";', WIRE.read_text(encoding="utf-8")))
    asked = set().union(*ASKS.values())
    if not names:
        bad("T4", f"{WIRE} 里一条 `bus-*` 都没读到 —— 取法坏了")
    elif names != asked:
        bad("T4", f"后端的 bus-* {sorted(names)} 与脚本问的 {sorted(asked)} 不相等")
    else:
        ok("T4", f"脚本问的 {sorted(asked)} == 后端登记的 bus-*")


def check_t5():
    entry = (SCRIPTS / "cc-bus").read_text(encoding="utf-8")
    m = re.search(r"\n\s*recv\)\n(.*?);;", entry, re.S)
    if not m:
        bad("T5", "入口里找不到 recv 那一臂")
        return
    body = strip_comments(m.group(1))
    calls = re.findall(r"cc-bus-(peek|commit)\.sh", body)
    if calls != ["peek", "commit"] or "jq" in body:
        bad("T5", f"recv 不是 peek 接 commit:那一臂调了 {calls},入口里 jq={'jq' in body}")
    else:
        ok("T5", "recv 是入口里先 peek 后 commit,入口里不渲染")


def check_t6():
    text = SKILL.read_text(encoding="utf-8")
    need = {
        "rc 0": r"\|\s*0\s*\|",
        "rc 2": r"\|\s*2\s*\|",
        "rc 11": r"\|\s*11\s*\|",
        "rc 12": r"\|\s*12\s*\|",
        "照常处理手上那段": r"照常处理手上那段",
        "at-least-once": r"at-least-once",
    }
    miss = [k for k, p in need.items() if not re.search(p, text)]
    if miss:
        bad("T6", f"手册页缺这几句:{miss}")
    else:
        ok("T6", f"手册页写全了:{len(need)} 条(commit 四个码 ＋ rc=11 的处置 ＋ at-least-once)")


def main():
    print(f"# cc-bus 入口那一侧(量于 `{SCRIPTS}`)")
    check_words("T1", DATA_WORDS, "收件箱 · 已读位置 · 总线家目录的字样")
    check_words("T2", LOCK_WORDS, "锁 · 规矩")
    check_t3(); check_t4(); check_t5(); check_t6()
    print()
    got, want = set(PASSED) | set(FAILED), set(EXPECTED_TAGS)
    if got != want:
        print(f"ccbus-twophase: 登记的判定格与现打对不上 —— 没跑的 {sorted(want - got)} · 多出来的 {sorted(got - want)}")
        return 1
    if FAILED:
        print(f"ccbus-twophase: FAIL={len(FAILED)}({', '.join(FAILED)})")
        return 1
    print(f"ccbus-twophase: {len(PASSED)} passed(标签集合与登记的 {len(EXPECTED_TAGS)} 条两向相等)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
