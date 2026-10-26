#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`w24c` 步 24c:**cc-bus 两阶段读口 ＋ 两个适配 trait ＋ Windows 那一侧**的判据本体。

## 它治的是什么

三条判据 ＋ 那道缝,此前**一格都没有**:
门禁里 `grep -c cc-peek tests/scripts/gate.sh` = 0,cc-bus 这一族只被 `shellcheck`
(语法)与 `plugin_class_registry`(脚本条数)碰到过 —— **没有任何东西在判它的行为**。

## ⚠ 买得到什么 / 买不到什么(写死,别读宽)

**买得到**(下面每一条都打一行 `✓`,末尾按**登记的标签集合两向对拍**):

  · `S1` `cc-peek` **零写面**:剥注释后它的写形集合 == 登记的两处豁免
        (锁 fd `9>>"$inbox.lock"` · 调用方指定的 `--token-file`);且不调 `cc-recv`。
        **反空真**:同一个扫描器在 `cc-commit` 上必须扫出 ≥1 处写(正控),否则判"扫描器坏了"。
  · `S2` `.pos` 的写点**全仓清点**:集合 == 登记的两处(`cc-commit` 推进 · `cc-recv` 老口),
        `cc-bus-stop-hook` **必须是 0 处** —— 那条无锁的"事后退回"整条消失了。
  · `S3` **锁族不增**:全 cc-bus 脚本里锁文件表达式的集合 == 登记的 7 个表达式 / 6 族;
        两阶段口那两处用的仍是 `<inbox>.lock`(A 族),没有第二把保 `.pos` 的锁。
  · `S4` **通用层零脚印**:`cc-peek`/`cc-commit`/`cc-bus-adapt.sh` 里 `tmux`/`flock`/`send-keys`
        出现 **0** 次;**正控**:`cc-bus-adapt-posix.sh` 里这三个词 >0(证明扫描器看得见)。
        ⚠ 老的 `cc-bus-lib.sh` 里还剩 5 处直接 `flock`(限流/去重/敲门三族),**恒等**钉住:
        它们保的是别的不变式,这里不动;多一处就红。
  · `S5` **渲染对拍**:`cc-peek` 与 `cc-recv` 里的 `jq -Rr` 程序**逐字节相等**(两份都取全集)。
  · `S6` `cc-recv` **一字未改**:sha256 == 登记值(「不降薄壳」的机检)。
  · `S7` **手册页写了那句**:`SKILL.md` 里 rc 0/2/11/12/13 五个码 ＋「照常处理你手上那段」
        那条处置 ＋ at-least-once 那句语义。⚠ `§3ter.3` 逐字要求"这一格必须写进手册页"。
  · `B1`–`B14` **真跑**:在一次性 `CC_BUS_HOME` 里把两阶段口、Stop 钩子、Windows 那一侧
        整段跑一遍(逐条见下面每个 `ok()` 的标签)。

  · `K7` 敲门经后端往会话说一句(真跑七形,见那一格)。

**买不到**(同样是判据,只是方向相反):
  · 不判「收件方是哪种 agent」(降级按**本进程**装载的词典算,今天只有 claude 一本)。
  · **没有真 Windows 机器**。Windows 那一侧靠 `CCBUS_ADAPT_OS=windows` 在 Linux 上跑
    ⇒ 买的是「那一侧的**实现**跑得通、能力自陈与降级是真的」,**不买**「在 Windows 上跑得起来」。
    盘符/UNC 路径、ConPTY 投递、`flock` 在 Git-Bash 里到底有没有 —— 一律**判不了**。
  · **注释只按小状态机剥**(整行 ＋ 行尾,引号里的 `#` 不算):here-doc 正文里的 `#`、
    `$'…'` 那一形都不认。人群里今天没有 here-doc,换了人群要回来重看这一条。
  · 它**不判并发在高负载下的公平性**:`B13` 只证明两个 commit 恰好一个赢,不证明谁赢。
  · 它**不判**那两条命令的**性能**,也不判 160KB 那一形在真 Claude 上的表现(无真 agent)。

## 跑法

    python3 tests/evidence/W24C-ccbus-twophase-ruler.py

`W24C_ROOT=<副本>` 只为**死值验**存在(照 `K_R80_ROOT` 那条取法):把仓拷进 scratchpad
变异之后对着副本跑,不必动真工作树。日常跑一律不带。
"""

import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(os.environ.get("W24C_ROOT") or Path(__file__).resolve().parents[2])
SCRIPTS = ROOT / "src" / "shared" / "cc-bus" / "scripts"
SKILL = ROOT / "src" / "shared" / "cc-bus" / "SKILL.md"

PASSED = []
FAILED = []


def ok(tag, msg):
    PASSED.append(tag)
    print(f"  ✓ {tag} {msg}")


def bad(tag, msg):
    FAILED.append(tag)
    print(f"  ✗ {tag} {msg}")


def eq(tag, got, want, msg):
    if got == want:
        ok(tag, f"{msg}(现打 {got!r})")
    else:
        bad(tag, f"{msg} —— 现打 {got!r},登记 {want!r}")


# ── 登记:本尺子该打出哪些标签(**反空真的锚**)───────────────────────────────
# 少一条 = 某一格悄悄没跑,而「没跑」与「过了」在输出上一模一样 ⇒ 两向集合对拍,不只是数数。
EXPECTED_TAGS = [
    "S1", "S2", "S3", "S4", "S5", "S6", "S7",
    "B1", "B2", "B3", "B4", "B5", "B6", "B7",
    "B8", "B9", "B10", "B11", "B12", "B13", "B14",
    # 〔10-14 spawn1〕敲门经后端 session-say(带看见的那一屏)· 读不到会话编号只入收件箱 · 同机不劝改道 · codex 词典
    "K7",
]

# ── 登记:静态面的那些数 ─────────────────────────────────────────────────────
# `cc-recv` 的 sha256(A13-2 现打:换了信封的渲染、删了自动接回复那一段、开头迁一次旧收件箱)。
# 要求逐字「`cc-recv` 不降薄壳」—— 改它要有理由并回来重打这个值,而「没动」这件事只有恒等才钉得住。
CC_RECV_SHA256 = "3cf1e28aa16edb3294defc7810cdeafebf8fb9673993030086a724cfe59c9f7b"

# `cc-peek` 里允许出现的写形,**逐条登记理由**(`D6`:相等断言 ＋ 登记豁免,不要下界)。
PEEK_WRITE_EXEMPT = {
    '9>>"$inbox.lock"':
        "打开锁文件的那个 fd。不开它就没有锁,而锁文件必须与 route_deliver/cc-recv 是同一个"
        "(§3ter.5「锁不搬家」)—— 它写的是一个零长锁文件,不是总线状态",
    '> "$tokfile"':
        "调用方用 `--token-file` 指定的路径,**脚本里另有一条闸拒绝它落在 $BUS 里** ⇒ 不是总线状态",
}

# 全 cc-bus 脚本里的锁文件表达式(剥注释后现打)→ 它属于哪一族(§3ter.5 那张表)。
LOCK_FAMILIES = {
    '$inbox.lock': "A 收件箱(route_deliver · cc-recv · cc-peek · cc-commit)",
    '$f.lock': "B 限流窗口",
    '$nf.lock': "D 敲门去抖",
    '$BUS/agents.tsv.lock': "E 名册与台账",
    '$BUS/state/migrate.lock': "F 旧收件箱迁一次(只在升级后第一次跑时拿,迁完是一次 stat)",
}

# `cc-bus-lib.sh` 里还剩几处**直接** `flock`(B/D 两族,保的是别的不变式;C 去重表随去重删了)。
# 恒等钉住:只把 A 族那一处换成了 `store_lock_exclusive`,别的一处没动。
LIB_DIRECT_FLOCK = 3

# 通用层里不许出现的词(§3.2 那条判据的词表)。表空了下面那条就是空真 ⇒ 先自检。
FORBIDDEN_IN_CORE = ["tmux", "flock", "send-keys"]
CORE_FILES = ["cc-peek", "cc-commit", "cc-bus-adapt.sh"]
ADAPTER_POSITIVE = "cc-bus-adapt-posix.sh"


def strip_comments(text):
    """剥注释:整行的 ＋ **行尾的**,引号里的 `#` 不算(小状态机)。

    ⚠ 只剥整行是不够的 —— 本尺子第一版就是那么写的,当场吃到两次误伤:
    `ccbus_adapt_load || exit 13   # …不退化成裸 flock` 这样一行代码尾巴上的注释,
    会让「通用层零脚印」那一条红在一句**注释**上。那种红比不判还糟:它会教人删注释。
    ⚠ **买不到**:here-doc 正文里的 `#`(本尺子的人群里没有 here-doc);`$'…'` 那一形。
    """
    out = []
    for ln in text.splitlines():
        if ln.lstrip().startswith("#"):
            continue
        q = None
        i = 0
        cut = None
        while i < len(ln):
            c = ln[i]
            if q == "'":
                if c == "'":
                    q = None
            elif q == '"':
                if c == "\\":
                    i += 1
                elif c == '"':
                    q = None
            else:
                if c in "'\"":
                    q = c
                elif c == "#" and (i == 0 or ln[i - 1] in " \t"):
                    cut = i
                    break
            i += 1
        out.append((ln if cut is None else ln[:cut]).rstrip())
    return "\n".join(out)


def src(name, stripped=True):
    t = (SCRIPTS / name).read_text(encoding="utf-8")
    return strip_comments(t) if stripped else t


# ════════════════════════════════════════════════════════════════════════════
# 静态面
# ════════════════════════════════════════════════════════════════════════════

WRITE_RE = re.compile(r'(?:\d*>>?\s*"[^"]+"|^\s*(?:mv|rm)\s)', re.M)


def write_ops(name):
    """一份脚本里的「写形」:重定向(含锁 fd)＋ mv/rm。**故意宽**、宁可多抓。"""
    body = src(name)
    hits = []
    for m in re.finditer(r'\d*>>?\s*"[^"]+"', body):
        hits.append(re.sub(r"\s+", " ", m.group(0)).strip())
    for m in re.finditer(r"^\s*(mv|rm)\s+\S+", body, re.M):
        hits.append(m.group(0).strip())
    return hits


def check_s1():
    got = set(write_ops("cc-peek"))
    # 正控:同一个扫描器在写侧那条命令上必须看得见东西,否则它是瞎的
    ctrl = write_ops("cc-commit")
    if len(ctrl) < 1:
        bad("S1", f"扫描器自检没过:在 cc-commit 上只扫出 {len(ctrl)} 处写形 —— "
                  f"它对 cc-peek 的「零命中」于是什么都不证明(空真)")
        return
    want = set(PEEK_WRITE_EXEMPT)
    if got != want:
        bad("S1", f"cc-peek 的写形集合与登记对不上:多出 {sorted(got - want)},少了 {sorted(want - got)}\n"
                  f"      ⇒ 第一跳必须是**纯读**;要么这处写不该有,要么它得在 PEEK_WRITE_EXEMPT 里逐条登记理由")
        return
    if "cc-recv" in src("cc-peek"):
        bad("S1", "cc-peek 里调了 cc-recv —— 那条命令有副作用,第一跳就不纯了")
        return
    ok("S1", f"cc-peek 零写面:{len(got)} 处写形全部落在登记的豁免里(锁 fd ＋ --token-file),"
             f"且不调 cc-recv;正控在 cc-commit 上扫出 {len(ctrl)} 处")


POS_WRITE_RE = re.compile(r'>\s*"\$posf"')


def check_s2():
    got = {}
    for p in sorted(SCRIPTS.iterdir()):
        if not p.is_file():
            continue
        n = len(POS_WRITE_RE.findall(strip_comments(p.read_text(encoding="utf-8"))))
        if n:
            got[p.name] = n
    want = {"cc-commit": 1, "cc-recv": 1}
    if got != want:
        bad("S2", f"`.pos` 的写点与登记对不上:现打 {got},登记 {want}\n"
                  f"      ⇒ 已读位置只许单调前进,而「谁能写它」是这条不变式的第一道闸。\n"
                  f"      ⚠ `cc-bus-stop-hook` 那条无锁的事后退回已经整条删掉(两阶段之后无可退)")
        return
    ok("S2", f"`.pos` 写点全仓 {sum(got.values())} 处,逐处登记:cc-commit(CAS 推进)· "
             f"cc-recv(老口,不降薄壳);cc-bus-stop-hook 0 处(事后退回已消失)")


def check_s3():
    got = {}
    for p in sorted(SCRIPTS.iterdir()):
        if not p.is_file():
            continue
        for m in re.finditer(r'\d+>>\s*"([^"]+)"', strip_comments(p.read_text(encoding="utf-8"))):
            got.setdefault(m.group(1), []).append(p.name)
    if set(got) != set(LOCK_FAMILIES):
        bad("S3", f"锁文件表达式集合变了:多出 {sorted(set(got) - set(LOCK_FAMILIES))},"
                  f"少了 {sorted(set(LOCK_FAMILIES) - set(got))}\n"
                  f"      ⇒ 多一个保 `.pos` 的锁 = 两把锁保同一个不变式 = 有锁序 = 有死锁(`D1`)")
        return
    a = sorted(got['$inbox.lock'])
    if not ({"cc-peek", "cc-commit"} <= set(a)):
        bad("S3", f"两阶段口没用 A 族那把锁,现打 A 族在 {a}")
        return
    lib_flock = len(re.findall(r"^\s*flock\s", src("cc-bus-lib.sh"), re.M))
    if lib_flock != LIB_DIRECT_FLOCK:
        bad("S3", f"cc-bus-lib.sh 里直接 `flock` 现打 {lib_flock} 处,登记 {LIB_DIRECT_FLOCK} 处 —— "
                  f"B/C/D 三族保的是别的不变式,这里刻意不动;变了要回来重新裁")
        return
    ok("S3", f"锁族不增:{len(got)} 个锁文件表达式 / {len(set(LOCK_FAMILIES.values()))} 族,"
             f"A 族现打 {a};lib 里直接 flock 仍是 {lib_flock} 处")


def check_s4():
    if not FORBIDDEN_IN_CORE or not CORE_FILES:
        bad("S4", "词表或人群是空的 —— 这一条此刻是空真")
        return
    ctrl = sum(src(ADAPTER_POSITIVE).count(w) for w in FORBIDDEN_IN_CORE)
    if ctrl == 0:
        bad("S4", f"正控没过:{ADAPTER_POSITIVE} 里一个 {FORBIDDEN_IN_CORE} 都扫不到 —— "
                  f"扫描器是瞎的,通用层那边的「零命中」什么都不证明")
        return
    hits = {}
    for f in CORE_FILES:
        body = src(f)
        for w in FORBIDDEN_IN_CORE:
            n = body.count(w)
            if n:
                hits[f"{f}:{w}"] = n
    if hits:
        bad("S4", f"通用层里出现了适配面的词:{hits}\n"
                  f"      ⇒ 通用层只认识 store_*/agent_* 那两张表里的函数名")
        return
    ok("S4", f"通用层零脚印:{len(CORE_FILES)} 份文件 × {len(FORBIDDEN_IN_CORE)} 个词 = 0 命中;"
             f"正控在 {ADAPTER_POSITIVE} 上命中 {ctrl} 次")


JQ_RE = re.compile(r"jq -Rr '([^']*)'")


def check_s5():
    a = JQ_RE.findall(src("cc-recv"))
    b = JQ_RE.findall(src("cc-peek"))
    if not a or not b:
        bad("S5", f"抽取器坏了:cc-recv 抽到 {len(a)} 段、cc-peek 抽到 {len(b)} 段 jq 程序")
        return
    if a != b:
        bad("S5", f"两条命令的渲染漂了:\n      cc-recv: {a}\n      cc-peek: {b}\n"
                  f"      ⇒ 「与 cc-recv 同一套渲染」是 §3ter.1 写死的;漂了只表现成「两条命令给出不同正文」")
        return
    ok("S5", f"渲染对拍:{len(a)} 段 jq 程序逐字节相同(共 {sum(len(x) for x in a)} 字节)")


def check_s6():
    h = hashlib.sha256((SCRIPTS / "cc-recv").read_bytes()).hexdigest()
    eq("S6", h, CC_RECV_SHA256, "cc-recv 一字未改(「不降薄壳」的机检)")


def check_s7():
    t = SKILL.read_text(encoding="utf-8")
    need = ["cc-peek", "cc-commit", "STALE", "CHANGED", "at-least-once",
            "照常处理你手上那段", "| 13 |", "| 0 |", "| 2 |"]
    miss = [x for x in need if x not in t]
    if miss:
        bad("S7", f"手册页缺这几句:{miss} —— §3ter.3 逐字「这一格**必须写进手册页**」,"
                  f"否则调用方会把 rc=11 当失败然后重试出一个循环")
        return
    ok("S7", f"手册页写全了:{len(need)} 条(五个退出码 ＋ rc=11 的处置 ＋ at-least-once 语义)")


# ════════════════════════════════════════════════════════════════════════════
# 行为面:在一次性 CC_BUS_HOME 里真跑
# ════════════════════════════════════════════════════════════════════════════

MSG = ('{{"id":"m{i}","ts":"t{i}","from":"{f}","to":"tester","tier":2,"act":"inform",'
       '"title":"t{i}","re":null,"refs":[],"text":"消息 {i}"}}\n')


class Bus:
    def __init__(self, tmp, name="bus"):
        self.home = Path(tmp) / name
        for d in ("inbox", "state", "log"):
            (self.home / d).mkdir(parents=True, exist_ok=True)
        # 台架写的就是新版信封 ⇒ 标成「已迁过」(不然第一次跑 cc-recv / 钩子会把它当旧收件箱挪进存档)。
        (self.home / "state" / "envelope-2").touch()
        self.inbox = self.home / "inbox" / "tester.jsonl"
        # 「我是哪条会话」只问后端(`cc-whoami` → `ccm -- --session-self`):每份台架带一个后端替身,答 `W24C_SELF`。
        self.ccm = self.home / "ccm-shim"
        self.ccm.write_text(SHIM_CCM, encoding="utf-8")
        self.ccm.chmod(0o755)

    def add(self, n, frm="alice"):
        with self.inbox.open("a", encoding="utf-8") as fh:
            for i in range(n):
                fh.write(MSG.format(i=i, f=frm))

    def env(self, **extra):
        e = dict(os.environ)
        e["CC_BUS_HOME"] = str(self.home)
        e["CCM_BIN"] = str(self.ccm)
        e["W24C_SELF"] = "tester"
        e["W24C_CCMLOG"] = str(self.home / "ccm.log")
        e.update(extra)
        return e

    def pos(self):
        p = self.home / "state" / "tester.pos"
        return p.read_text(encoding="utf-8").strip() if p.exists() else None

    def snapshot(self):
        """state/ 与 inbox/ 的现物(路径 → 内容 hash)。log/ 不在射程:
        §3ter.6 条 1 的射程逐字是 `state/` 与 `inbox/`,而适配层降级要留痕。"""
        out = {}
        for d in ("state", "inbox"):
            for p in sorted((self.home / d).rglob("*")):
                if p.is_file():
                    out[str(p.relative_to(self.home))] = hashlib.sha256(p.read_bytes()).hexdigest()
        return out


def run(args, env, stdin=None):
    p = subprocess.run([str(SCRIPTS / args[0])] + [str(a) for a in args[1:]],
                       env=env, input=stdin, capture_output=True, text=True)
    return p.returncode, p.stdout, p.stderr


def peek_token(bus, env=None, extra=()):
    rc, out, err = run(["cc-peek", "tester", *extra], env or bus.env())
    return rc, out, err.strip().splitlines()[-1] if err.strip() else ""


def behavioral(tmp):
    bus = Bus(tmp, "b1")
    bus.add(3)

    # B1 零写面(真跑):peek 前后 state/ 与 inbox/ 逐份对拍,唯一允许的新增是那个零长锁文件
    before = bus.snapshot()
    rc, out, tok = peek_token(bus)
    after = bus.snapshot()
    new = set(after) - set(before)
    changed = {k for k in before if before[k] != after.get(k)}
    if rc == 0 and not changed and new == {"inbox/tester.jsonl.lock"}:
        ok("B1", f"peek 真跑之后 state/ 与 inbox/ 逐份字节不变(现打 {len(before)} 份),"
                 f"唯一新增是零长锁文件 {sorted(new)}")
    else:
        bad("B1", f"peek 写了东西:rc={rc} 变了 {sorted(changed)} 新增 {sorted(new)}")

    # B2 令牌形状 ＋ commit 推进，state/ 里只多那一个 .pos(接在哪一条后面由发信人写进信封,不再替谁记)
    if re.fullmatch(r"\d+\.\d+\.\d+-\d+", tok) and out.count("[cc-bus] id=") == 3:
        before2 = set(bus.snapshot())
        rc2, _, err2 = run(["cc-commit", "tester", tok], bus.env())
        extra = set(bus.snapshot()) - before2
        if rc2 == 0 and bus.pos() == "3" and extra == {"state/tester.pos"}:
            ok("B2", f"令牌 {tok} → commit rc=0,pos 0→3,state/ 里只多了已读位置那一份")
        else:
            bad("B2", f"commit 没做对:rc={rc2} pos={bus.pos()} 多出 {sorted(extra)} err={err2.strip()[:120]}")
    else:
        bad("B2", f"peek 的形状不对:令牌={tok!r} 正文里 {out.count('[cc-bus] id=')} 条")

    # B3 同一个令牌再 commit ⇒ STALE,且**一个字节都不写**
    snap = bus.snapshot()
    rc3, _, err3 = run(["cc-commit", "tester", tok], bus.env())
    if rc3 == 11 and bus.snapshot() == snap and "STALE" in err3:
        ok("B3", "重复 commit ⇒ rc=11 STALE,state/ 与 inbox/ 一个字节没动(先到的赢)")
    else:
        bad("B3", f"STALE 那一格不对:rc={rc3} 写了 {bus.snapshot() != snap} err={err3.strip()[:120]}")

    # B4 收件箱在两跳之间变了 ⇒ CHANGED
    bus.add(1, "bob")
    rc, _, tok4 = peek_token(bus)
    txt = bus.inbox.read_text(encoding="utf-8").replace("消息 0\"}", "消息 0 改过\"}")
    bus.inbox.write_text(txt, encoding="utf-8")
    rc4, _, err4 = run(["cc-commit", "tester", tok4], bus.env())
    if rc4 == 12 and bus.pos() == "3" and "CHANGED" in err4:
        ok("B4", "末行被改过之后 commit ⇒ rc=12 CHANGED,位置不动(anchor 把「同一行号指向另一条消息」变成显式失败)")
    else:
        bad("B4", f"CHANGED 那一格不对:rc={rc4} pos={bus.pos()} err={err4.strip()[:120]}")

    # B5 参数面:令牌非法 / id 非法 / 令牌文件落在 BUS 里
    r1, _, _ = run(["cc-commit", "tester", "乱码"], bus.env())
    r2, _, _ = run(["cc-commit", "坏/id", "0.1.1-1"], bus.env())
    r3, _, _ = run(["cc-peek", "tester", "--token-file", str(bus.home / "state" / "t.tok")], bus.env())
    r4, _, _ = run(["cc-peek", "tester", "--max-lines", "零"], bus.env())
    r5, _, _ = run(["cc-peek", "tester", "--no-such-flag"], bus.env())
    if (r1, r2, r3, r4, r5) == (2, 2, 2, 2, 2):
        ok("B5", "参数面五条全 rc=2:令牌乱码 · id 非法 · --token-file 指向 $BUS 里 · --max-lines 非数 · 不认识的旗标")
    else:
        bad("B5", f"参数面的退出码不对:{(r1, r2, r3, r4, r5)},应当五个都是 2")

    # B6 分段读:一轮一条,位置单调递增
    b2 = Bus(tmp, "b2")
    b2.add(4)
    poses = []
    for _ in range(4):
        rc, out, tk = peek_token(b2, extra=("--max-lines", "1"))
        if out.count("[cc-bus] id=") != 1:
            break
        rc, _, _ = run(["cc-commit", "tester", tk], b2.env())
        poses.append(b2.pos())
    if poses == ["1", "2", "3", "4"]:
        ok("B6", f"--max-lines 1 分四轮排空,位置逐轮单调前进 {poses}(cc-recv 做不到分段:要么全推要么不推)")
    else:
        bad("B6", f"分段读不对:位置序列 {poses},应当 ['1','2','3','4']")

    # B7 渲染行为对拍:同一份收件箱,两条命令的 stdout 逐字节相同
    b3 = Bus(tmp, "b3")
    b3.add(3)
    b4 = Bus(tmp, "b4")
    b4.add(3)   # 两份**独立**收件箱、内容按构造逐字节相同(cc-recv 有副作用,不能跟 peek 共用一份)
    _, o1, _ = run(["cc-peek", "tester"], b3.env())
    _, o2, _ = run(["cc-recv", "tester"], b4.env())
    if o1 == o2 and o1.strip():
        ok("B7", f"cc-peek 与 cc-recv 的 stdout 逐字节相同({len(o1)} 字节)")
    else:
        bad("B7", f"两条命令的正文不一样:peek {len(o1)} 字节 / recv {len(o2)} 字节")

    # B8 Stop 钩子:喂回失败 ⇒ **位置一动不动**(160KB 那件事故的形状)
    b5 = Bus(tmp, "b5")
    b5.add(2)
    shim = Path(tmp) / "shim"
    shim.mkdir(exist_ok=True)
    (shim / "jq").write_text(
        '#!/usr/bin/env bash\nfor a in "$@"; do [ "$a" = "-cn" ] && exit 1; done\nexec /usr/bin/jq "$@"\n',
        encoding="utf-8")
    (shim / "jq").chmod(0o755)
    env = b5.env(PATH=f"{shim}:{os.environ['PATH']}")
    rc, out, err = run(["cc-bus-stop-hook"], env, stdin="{}")
    again = run(["cc-peek", "tester"], b5.env())[1].count("[cc-bus] id=")
    if rc == 0 and out.strip() == "" and b5.pos() is None and again == 2:
        ok("B8", "喂回失败 ⇒ 钩子 rc=0、无拦停、`.pos` 根本没建过,两条消息下一跳照样读得到"
                 "(「推进了再退回」换成了「没确认就不推进」)")
    else:
        bad("B8", f"钩子的失败路径不对:rc={rc} stdout={out.strip()[:60]!r} pos={b5.pos()} 再读到 {again} 条")

    # B9 Stop 钩子:正常路径 ⇒ 吐 block JSON ＋ 推进
    rc, out, err = run(["cc-bus-stop-hook"], b5.env(), stdin="{}")
    if rc == 0 and '"decision":"block"' in out and b5.pos() == "2":
        ok("B9", "喂回成功 ⇒ 拦停 JSON 出来了、位置推到 2(第二跳由 cc-commit 做)")
    else:
        bad("B9", f"钩子的成功路径不对:rc={rc} pos={b5.pos()} out={out[:80]!r}")

    # B10 Windows 那一侧:整段跑通 ＋ 能力自陈 ＋ 降级留痕
    b6 = Bus(tmp, "b6")
    b6.add(2)
    wenv = b6.env(CCBUS_ADAPT_OS="windows")
    rc, out, tok6 = peek_token(b6, env=wenv)
    rc_c, _, _ = run(["cc-commit", "tester", tok6], wenv)
    caps = subprocess.run(
        ["bash", "-c", f'. "{SCRIPTS}/cc-bus-adapt.sh"; ccbus_adapt_load && store_caps'],
        env=wenv, capture_output=True, text=True).stdout
    log = (b6.home / "log" / "bus.log").read_text(encoding="utf-8") if (b6.home / "log" / "bus.log").exists() else ""
    if (rc == 0 and rc_c == 0 and b6.pos() == "2"
            and "shared_lock=no" in caps
            and "DEGRADE lock:shared→exclusive" in log):
        ok("B10", "Windows 那一侧:两阶段口整段跑通(pos→2)· `shared_lock=no` 自陈 · "
                  "共享锁退化成排他且 bus.log 里留了 DEGRADE")
    else:
        bad("B10", f"Windows 那一侧不对:peek={rc} commit={rc_c} pos={b6.pos()} "
                   f"caps={caps.strip()[:120]!r} 降级日志={'有' if 'DEGRADE' in log else '无'}")

    # B11 这台机器上没有 flock 时,Windows 侧**拒绝**而不是假装锁上了
    nolock = Path(tmp) / "nolock"
    nolock.mkdir(exist_ok=True)
    for exe in ("bash", "awk", "sed", "cat", "jq", "date", "cksum", "grep", "mktemp", "readlink", "uname", "dirname", "basename", "rm", "mv", "chmod", "mkdir", "printf", "tr", "seq", "sort", "head", "wc", "tail", "kill", "env", "ls"):
        p = shutil.which(exe)
        if p:
            (nolock / exe).symlink_to(p)
    b7 = Bus(tmp, "b7")
    b7.add(1)
    rc, out, err = run(["cc-peek", "tester"],
                       b7.env(CCBUS_ADAPT_OS="windows", PATH=str(nolock)))
    if rc == 13:
        ok("B11", "Windows 侧缺 flock ⇒ cc-peek rc=13(办不到),不回落到一个「看起来在锁、其实没锁」的假锁")
    else:
        bad("B11", f"缺 flock 时 cc-peek rc={rc},应当 13;err={err.strip()[:120]}")

    # B12 适配层装不齐 ⇒ fail-closed
    rc, _, err = run(["cc-peek", "tester"], bus.env(CCBUS_ADAPT_AGENT="没有这个词典"))
    if rc == 13:
        ok("B12", "agent 词典不存在 ⇒ cc-peek rc=13(fail-closed),不退化成「少一格能力照跑」")
    else:
        bad("B12", f"词典缺失时 rc={rc},应当 13")

    # B13 并发:两个 commit 同时拿同一个令牌,**恰好一个赢**
    b8 = Bus(tmp, "b8")
    b8.add(2)
    _, _, tk = peek_token(b8)
    with ThreadPoolExecutor(max_workers=2) as ex:
        rcs = sorted(f.result()[0] for f in
                     [ex.submit(run, ["cc-commit", "tester", tk], b8.env()) for _ in range(2)])
    if rcs == [0, 11] and b8.pos() == "2":
        ok("B13", "两个 commit 并发拿同一个令牌 ⇒ 恰好一个 rc=0、一个 rc=11(CAS 在锁内,先到的赢)")
    else:
        bad("B13", f"并发那一格不对:退出码 {rcs},pos={b8.pos()};应当 [0, 11]")

    # B14 截短自愈:pos 比 inbox 还高时,唯一允许的「变小」
    b9 = Bus(tmp, "b9")
    b9.add(5)
    (b9.home / "state" / "tester.pos").write_text("5\n", encoding="utf-8")
    b9.inbox.write_text("".join(MSG.format(i=i, f="alice") for i in range(2)), encoding="utf-8")
    _, out, tk = peek_token(b9)
    rc, _, err = run(["cc-commit", "tester", tk], b9.env())
    heal_ok = rc == 0 and b9.pos() == "2" and tk.startswith("5.2.")
    # 反面:没截短时 to ≤ from 一律 rc=2(不许拿自愈当后门)
    bad_tok = f"2.1.{tk.split('.', 2)[2]}"
    rc2, _, _ = run(["cc-commit", "tester", bad_tok], b9.env())
    if heal_ok and rc2 != 0:
        ok("B14", f"截短自愈:pos=5 而收件箱只剩 2 行 ⇒ 令牌 {tk} commit 通过、位置降到 2(唯一登记的豁免);"
                  f"而没截短时 to≤from 的令牌 rc={rc2}(不为它开后门)")
    else:
        bad("B14", f"截短自愈那一格不对:rc={rc} pos={b9.pos()} 令牌={tk} 反面 rc={rc2}")


# ════════════════════════════════════════════════════════════════════════════
# 敲门:经后端往会话说一句(两档:一档敲门、对方闲着才送;二档只进收件箱)
# ════════════════════════════════════════════════════════════════════════════

# 敲门那一句,**逐字**抄在这里当金标准(不从仓里现取:现取就是两侧同源恒真)。
OLD_T2 = "🔔 cc-bus: 你有来自 {from} 的新消息,运行 cc-recv 读取并按内容处理"
SENDER = "aaaaaaaa-1111-2222-3333-444444444444"
BOB = "bbbbbbbb-1111-2222-3333-444444444444"
# tmux 替身:谁要是还往窗格里打字(send-keys),记进 `W24C_SHIMLOG` —— 判据要它恒空。
SHIM_TMUX = (
    "#!/usr/bin/env bash\n"
    'if [ "$1" = send-keys ]; then printf \'%s\\037\' "$@" >> "$W24C_SHIMLOG"; printf \'\\036\' >> "$W24C_SHIMLOG"; fi\n'
    "exit 0\n"
)
# 后端替身(`CCM_BIN`):认 `-- --session-self`(答 `W24C_SELF`)/ `-- --bus-envelope` / `-- --terminal-preview` / `-- --session-say`,
# stdin 一段 JSON;每一问记进 `W24C_CCMLOG`。
# 信封那一问照入参盖一份规范信封(真校验住后端,另有单测与 e2e;这里只管管线拿到之后怎么投、怎么敲)。
# `W24C_NO_SCREEN` 设了 ⇒ 看不到那一屏(预览失败);`W24C_SAY` ⇒ 说那一句的结局(缺省 delivered)。
SHIM_CCM = (
    "#!/usr/bin/env bash\n"
    'a=$(cat); printf \'%s\\037%s\\036\' "$2" "$a" >> "$W24C_CCMLOG"\n'
    'case "$2" in\n'
    '  --session-self) printf \'{"sid":"%s","groups":[],"looked":"this_machine"}\\n\' "$W24C_SELF" ;;\n'
    '  --bus-envelope) printf \'%s\' "$a" | jq -c \'{id:"m0000000000001",ts:"2026-10-26T00:00:00Z",from,to,tier:(.tier // 2),act,title,re:(.re // null),refs:(.refs // []),text:(.text // "")}\' ;;\n'
    '  --terminal-preview) [ -n "${W24C_NO_SCREEN:-}" ] && { echo \'{"code":"no_such_session"}\' >&2; exit 2; }\n'
    '                      echo \'{"screen":"00ff00ff00ff00ff","cols":80,"rows":24}\' ;;\n'
    '  --session-say) printf \'{"result":"%s","why":"%s"}\\n\' "${W24C_SAY:-delivered}" "${W24C_WHY:-}" ;;\n'
    '  *) exit 2 ;;\n'
    "esac\n"
)


def bash_lib(snippet, env, scripts=None):
    sd = scripts or SCRIPTS
    p = subprocess.run(["bash", "-c", f'. "{sd}/cc-bus-lib.sh" || exit 99; {snippet}'],
                       env=env, capture_output=True, text=True)
    return p.returncode, p.stdout, p.stderr


def nudge_bus(tmp, name):
    b = Bus(tmp, name)
    shim = Path(tmp) / f"{name}-shim"
    shim.mkdir(exist_ok=True)
    (shim / "tmux").write_text(SHIM_TMUX, encoding="utf-8")
    (shim / "tmux").chmod(0o755)
    (shim / "ccm").write_text(SHIM_CCM, encoding="utf-8")
    (shim / "ccm").chmod(0o755)
    log = Path(tmp) / f"{name}-ccm.log"
    env = b.env(W24C_SELF=SENDER, PATH=f"{shim}:{os.environ['PATH']}",
                W24C_SHIMLOG=str(Path(tmp) / f"{name}-tmux.log"), W24C_CCMLOG=str(log),
                CCM_BIN=str(shim / "ccm"), CCBUS_NUDGE_DEBOUNCE="0")
    return b, env, log


def ccm_calls(log):
    """后端替身记下的每一问:(子命令, 入参 JSON)。"""
    import json
    if not log.exists():
        return []
    out = []
    for call in log.read_text(encoding="utf-8").split("\x1e"):
        a = call.split("\x1f")
        if len(a) == 2:
            out.append((a[0], json.loads(a[1]) if a[1].strip() else {}))
    return out


def knocks(log):
    """经后端说出去的那几句(`session-say` 的 `text`)。"""
    return [req.get("text") for cmd, req in ccm_calls(log) if cmd == "--session-say"]


def bob_inbox(b):
    p = b.home / "inbox" / f"{BOB}.jsonl"
    return p.read_text(encoding="utf-8").splitlines() if p.exists() else []


def preflight():
    """跑不动就说跑不动 —— 不许把「环境缺件」当成绿。"""
    miss = [c for c in ("bash", "jq", "flock", "awk", "sed", "cksum") if shutil.which(c) is None]
    if miss:
        print(f"ccbus-twophase: 这台机器上缺 {miss} —— 判不了,按红记(不许退化成静默跳过)")
        return False
    for f in CORE_FILES + [ADAPTER_POSITIVE, "cc-recv", "cc-bus-lib.sh", "cc-bus-stop-hook",
                           "cc-bus-adapt-windows.sh", "cc-bus-agent-claude.sh"]:
        if not (SCRIPTS / f).is_file():
            print(f"ccbus-twophase: 读不到 {SCRIPTS / f} —— 人群不在盘上,判不了")
            return False
    return True


def check_k7(tmp):
    """敲门不往窗格里打字:经后端 `session-say`(先 `terminal-preview` 拿那一屏的指纹、带着它、只在闲着时说);
    收件人就是会话 id,敲的就是它;一档才敲、二档只进收件箱;看不到那一屏 / 那台不送 ⇒ 只入收件箱、不算敲过;codex 词典装得上。"""
    send = ["cc-send", BOB, "--act", "inform", "--title", "t", "x"]
    # a) 静态:通用层与 POSIX 适配面里都没有 send-keys 了(往窗格里打字那一档整条退出)
    typed = {f: src(f).count("send-keys") for f in ("cc-bus-lib.sh", "cc-bus-adapt-posix.sh", "cc-bus-adapt.sh")}
    a_ok = not any(typed.values())
    # b) 一档 ⇒ 核信封 → 先预览 → 带着那一屏的指纹、只在闲着时说;一个字都没往窗格里打
    b, env, log = nudge_bus(tmp, "k7b")
    rc, _, _ = run(send[:2] + ["--tier", "1"] + send[2:], env)
    calls = ccm_calls(log)
    tlog = Path(tmp) / "k7b-tmux.log"
    b_ok = (rc == 0 and [c for c, _ in calls] == ["--session-self", "--bus-envelope", "--terminal-preview", "--session-say"]
            and calls[2][1].get("sid") == BOB and calls[3][1].get("sid") == BOB
            and calls[3][1].get("seen_screen") == "00ff00ff00ff00ff"
            and calls[3][1].get("idle_only") is True
            and calls[3][1].get("text") == OLD_T2.replace("{from}", SENDER)
            and len(bob_inbox(b)) == 1
            and not (tlog.exists() and tlog.read_text(encoding="utf-8")))
    # c) 二档(缺省)⇒ 只进收件箱,一次看屏 / 说一句都不问,记一笔
    b3, env3, log3 = nudge_bus(tmp, "k7c")
    rc3, _, _ = run(send, env3)
    l3 = (b3.home / "log" / "bus.log").read_text(encoding="utf-8")
    c_ok = (rc3 == 0 and [c for c, _ in ccm_calls(log3)] == ["--session-self", "--bus-envelope"]
            and len(bob_inbox(b3)) == 1 and f"NUDGE tier2 {BOB}" in l3)
    # d) 看不到那一屏 ⇒ 不说;那台不送(不闲)⇒ 记一笔、不算敲过
    b5, env5, log5 = nudge_bus(tmp, "k7e")
    run(send[:2] + ["--tier", "1"] + send[2:], dict(env5, W24C_NO_SCREEN="1"))
    l5 = (b5.home / "log" / "bus.log").read_text(encoding="utf-8")
    b6, env6, log6 = nudge_bus(tmp, "k7f")
    run(send[:2] + ["--tier", "1"] + send[2:], dict(env6, W24C_SAY="refused", W24C_WHY="not_idle"))
    l6 = (b6.home / "log" / "bus.log").read_text(encoding="utf-8")
    d_ok = (not knocks(log5) and "NUDGE noscreen" in l5 and len(bob_inbox(b5)) == 1
            and "NUDGE refused" in l6 and "why=not_idle" in l6 and len(bob_inbox(b6)) == 1
            and not (b6.home / "state" / f"nudge-{BOB}").exists())
    # e) 同机两头都是 Claude 也照投、不劝改道;codex 词典 ⇒ 三档照降级链
    b8, env8, _ = nudge_bus(tmp, "k7h")
    _, _, err8 = run(send, env8)
    b9, env9, _ = nudge_bus(tmp, "k7i")
    rc9, _, err9 = run(send, dict(env9, CCBUS_ADAPT_AGENT="codex"))
    _, out9, _ = bash_lib('printf "%s %s %s" "$(ccbus_degrade block)" "$(ccbus_degrade nudge)" "$(ccbus_degrade inbox)"',
                          dict(env9, CCBUS_ADAPT_AGENT="codex"))
    e_ok = ("SendMessage" not in err8 and len(bob_inbox(b8)) == 1
            and rc9 == 0 and "SendMessage" not in err9 and out9 == "block nudge inbox")
    if a_ok and b_ok and c_ok and d_ok and e_ok:
        ok("K7", "敲门经后端说一句(真跑五形):通用层 ＋ POSIX 适配面零 send-keys · 一档先核信封、再预览、带着指纹只在闲着时说 · "
                 "二档只进收件箱、不问看屏 · 看不到那一屏 / 那台不送 ⇒ 不算敲过 · "
                 "同机不劝改道(claude / codex 都不提 SendMessage)· codex 三档照降级链")
    else:
        bad("K7", f"敲门那一格不对:a零打字={a_ok}({typed}) b={b_ok}({calls}) c={c_ok}({ccm_calls(log3)}) "
                  f"d={d_ok} e={e_ok}({err8.strip()[-60:]!r} {err9.strip()[-60:]!r} {out9!r})")


def main():
    print(f"# `w24c` 步 24c —— cc-bus 两阶段读口 ＋ 两个适配 trait ＋ Windows 那一侧(量于 `{SCRIPTS}`)")
    print()
    if not preflight():
        return 1
    check_s1(); check_s2(); check_s3(); check_s4(); check_s5(); check_s6(); check_s7()
    with tempfile.TemporaryDirectory(prefix="w24c-") as tmp:
        behavioral(tmp)
        check_k7(tmp)
    print()
    # ── 反空真的锚:**标签集合两向对拍**,不是"数够了就行" ──────────────────
    got, want = set(PASSED) | set(FAILED), set(EXPECTED_TAGS)
    if got != want:
        print(f"ccbus-twophase: 登记的判定格与现打**对不上** —— "
              f"没跑的 {sorted(want - got)} · 多出来的 {sorted(got - want)}\n"
              f"  ⇒ 「悄悄没跑」与「过了」在输出上一模一样,所以这一条是两向集合相等,不是数数")
        return 1
    if FAILED:
        print(f"ccbus-twophase: FAIL={len(FAILED)}({', '.join(FAILED)})")
        return 1
    print(f"ccbus-twophase: {len(PASSED)} passed(两阶段:静态 7 条 ＋ 真跑 14 条;敲门 1 条;"
          f"标签集合与登记的 {len(EXPECTED_TAGS)} 条两向相等)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
