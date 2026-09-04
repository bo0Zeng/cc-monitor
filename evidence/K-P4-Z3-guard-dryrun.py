#!/usr/bin/env python3
"""K-P4 摸底拍（`KP4Z3`）的量具 —— **那几道后端护栏今天是什么，新形状下还挡不挡得住**。

住址：`<工作树>/evidence/K-P4-Z3-guard-dryrun.py`（默认工作树 `.claude/worktrees/k-p4b`）

它做三件事：

  一 · **现打**三道护栏今天的判据面（**从 Rust 源码里读出来**，不手抄）：
       `readonly_guard`（写盘）· `no_timer_guard`（自己醒来）· `platform/fallback_guard`（平台回退）。
       连同它们各自的**人群怎么画的**、**反空真地板**、**跑在门禁哪一格**。

  二 · **空转（dry-run）**：把「按 `§0f` 新题面要搬进后端的那几段」当**候选载荷**，
       用**复刻的**判据跑一遍，报「哪条判据会红、红在哪一行」。
       ⚠⚠ **复刻件是代用品**：判据本体是 Rust 的那三份，本尺子的复刻只用来**估**。
       为此复刻件自带两条自检：① 拿判据自己写在源码里的**阳性样本**喂进来必须红；
       ② 拿今天的 daemon 生产段喂进来必须**全绿**（那三道今天在门禁里就是绿的）。
       两条有一条不成立 ⇒ 打印 `CRASH`，本尺子的读数当场作废。

  三 · **`#[cfg(windows)]` 逃不逃得掉**：三道护栏的人群都是**从磁盘读文本**、
       只剥 `#[cfg(test)]`，⇒ 把代码塞进 `#[cfg(windows)]` **不会**让它掉出人群。
       本尺子拿合成样本把这一条打出来（Linux 上跑的门禁照样看得见 Windows 那支）。

⚠ 射程（写在前面）：
  · 「会不会红」判的是**判据的文本形状**，不是「搬完之后代码长什么样」——
    真搬的时候代码会改写（比如换掉睡眠环），那时读数要重打。
  · 本尺子**不判**「这条护栏该不该放宽」。那是 PM 的事，本拍只给形状与住址。

跑法：
    python3 evidence/K-P4-Z3-guard-dryrun.py
"""

import argparse
import re
import sys
from pathlib import Path

PROJ = Path("/home/zbl/文档/claudecode-frontend")
DEFAULT_WT = PROJ / ".claude" / "worktrees" / "k-p4b"


# ————————————————————————————————————————————————————————————— 判据面：从源码里读，不手抄
def literal_array(src: str, name: str):
    """读一张 `const NAME: &[&str] = &[ "a", "b" ];` 的字面量表。读不到返回 None。"""
    m = re.search(rf"const {name}: &\[&str\] = &\[(.*?)\];", src, re.S)
    if not m:
        return None
    return re.findall(r'"((?:[^"\\]|\\.)*)"', m.group(1))


def periodic_patterns(src: str):
    """`no_timer_guard::periodic_wake_patterns` 是**运行时拼**的（防自指）——把 format! 对拼回来。"""
    m = re.search(r"fn periodic_wake_patterns\(\) -> Vec<String> \{(.*?)\n    \}", src, re.S)
    if not m:
        return None
    out = []
    for a, b in re.findall(r'format!\("([^"]*)\{\}"\s*,\s*"([^"]*)"\)', m.group(1)):
        out.append(a + b)
    return out


def strip_cfg_test(src: str) -> str:
    """复刻 `readonly_guard::strip_cfg_test`：只剥行首 `#[cfg(test)]` 之后的那一个块/声明。"""
    ATTR = "#[cfg(test)]"
    out, rest, first = [], src, True
    while True:
        idx = rest.find(ATTR) if (first and rest.startswith(ATTR)) else rest.find("\n" + ATTR)
        if idx == -1:
            out.append(rest)
            break
        cut = idx if (first and rest.startswith(ATTR)) else idx + 1
        out.append(rest[:cut])
        tail = rest[cut:]
        brace, semi = tail.find("{"), tail.find(";")
        if semi != -1 and (brace == -1 or semi < brace):
            rest = tail[semi + 1:]
        elif brace == -1:
            break
        else:
            depth, i = 0, brace
            while i < len(tail):
                if tail[i] == "{":
                    depth += 1
                elif tail[i] == "}":
                    depth -= 1
                    if depth == 0:
                        break
                i += 1
            rest = tail[i + 1:]
        first = False
    return "".join(out)


def production_code(src: str) -> str:
    """复刻 `guard_core::production_code`：剥 `#[cfg(test)]` **再剥 `//` 注释**。

    ⚠ 两道护栏用的剥法**不是同一份**，这一格差别是承重的：
      · `readonly_guard` 只剥 `#[cfg(test)]`（**连注释一起扫**，它自陈那是 fail-closed）；
      · `no_timer_guard` 走 `production_code`（**注释剥掉**）—— 否则头注里写着
        `Duration::from_secs` 的那四处会把它自己打红（本尺子第一版就是这么红的）。
    本函数的行尾剥法是**朴素版**（按行数引号奇偶决定切不切），比 `guard_core` 那份
    （识别 raw/byte string、跨行字符串）**弱**；弱的方向是「少剥 ⇒ 多命中 ⇒ 偏红」，
    而下面的自检②要求它在今天的 daemon 生产段上恰好 0 命中 —— 那一格就是它的验收。
    """
    kept = []
    for line in strip_cfg_test(src).split("\n"):
        if line.lstrip().startswith("//"):
            continue
        i = line.find("//")
        if i != -1 and line[:i].count('"') % 2 == 0:
            line = line[:i]
        kept.append(line)
    return "\n".join(kept)


def hits(text: str, needles, label):
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        for pat in needles:
            if pat in line:
                out.append((label, pat, n, line.strip()[:74]))
    return out


def block_tail(body: str) -> str:
    """复刻 `fallback_guard::block_tail`：块的**最后一个有效表达式行**。"""
    ls = [l.strip() for l in body.splitlines()]
    ls = [l for l in ls if l and not l.startswith("//") and l not in ("{", "}")]
    return ls[-1].rstrip(";}").strip() if ls else ""


def fabricates(body: str):
    out = []
    if any(t == "true" for t in re.split(r"[^A-Za-z0-9_]", body)):
        out.append("块体里出现裸 `true`")
    t = block_tail(body)
    if t.startswith("Some(") or t.startswith("Ok("):
        out.append(f"块体最后一个表达式是 `{t}` —— 凭空造了一个「成功」值")
    return out


def body_after(src: str, header_line_idx: int):
    """从某一行起按花括号配平取块（含该行）。取不到返回 None。"""
    lines = src.splitlines()
    depth, started, out = 0, False, []
    for j in range(header_line_idx, len(lines)):
        depth += lines[j].count("{") - lines[j].count("}")
        out.append(lines[j])
        if "{" in lines[j]:
            started = True
        if started and depth <= 0:
            return "\n".join(out)
    return None


def cut_fn(src: str, pattern: str):
    """按正则找函数头行，切它的体。返回 (起始行号, 文本)；找不到返回 None。"""
    rx = re.compile(pattern)
    for i, line in enumerate(src.splitlines()):
        if rx.search(line):
            b = body_after(src, i)
            if b:
                return i + 1, b
    return None


def daemon_prod(rd: Path):
    """daemon crate 生产段，跳过三份护栏自身 —— 与三道护栏的采集口径同形。

    返回 {rel: (只剥 cfg(test) 的文本, 连注释也剥的文本)} —— 两道护栏各用各的那一份。
    """
    out = {}
    for p in sorted(rd.rglob("*.rs")):
        if p.name in ("readonly_guard.rs", "no_timer_guard.rs", "fallback_guard.rs"):
            continue
        rel = str(p.relative_to(rd))
        src = p.read_text(encoding="utf-8", errors="replace")
        out[rel] = (strip_cfg_test(src), production_code(src))
    return out


# ————————————————————————————————————————————————————————————— 候选载荷：按搬家粒度分四档
PAYLOAD_FNS = {
    "P1 只搬拉窗那三步": [
        (r"^pub fn activate\(hwnd", "activate(win)"),
        (r"^pub fn verify_binding\(binding", "verify_binding(win)"),
        (r"^fn process_creation_filetime\(pid", "process_creation_filetime(win)"),
    ],
    "P2 + 扫窗与绑定": [
        (r"^fn find_window_by_marker_substr\(marker", "find_window_by_marker_substr(win)"),
        (r"^\s*pub fn try_bind\(&self, sid", "RemoteHwndCache::try_bind(win)"),
        (r"^\s*pub fn try_bind_with_retry\(&self, sid", "try_bind_with_retry(win)"),
        (r"^pub fn get_parent_pid\(pid", "get_parent_pid(win)"),
    ],
    "P3 + 两个缓存": [
        (r"^impl SidHwndCache", "impl SidHwndCache"),
        (r"^impl RemoteHwndCache", "impl RemoteHwndCache"),
    ],
    "P4 整个 bind.rs（上限）": [(r"^//! ", "bind.rs 全文")],
}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--wt", default=str(DEFAULT_WT))
    args = ap.parse_args()
    wt = Path(args.wt)
    rd = wt / "remote-daemon-proto" / "src"
    ro = (rd / "readonly_guard.rs").read_text(encoding="utf-8")
    nt = (rd / "no_timer_guard.rs").read_text(encoding="utf-8")
    fb = (rd / "platform" / "fallback_guard.rs").read_text(encoding="utf-8")
    bind_src = (wt / "src-tauri/src/bind.rs").read_text(encoding="utf-8")

    print("═══ 一 · 三道护栏今天是什么（判据面从源码读出，不手抄）")
    fs_pat = literal_array(ro, "FS_MUTATION_PATTERNS")
    wl_pat = literal_array(ro, "WHITELIST_STILL_FORBIDDEN")
    wake = periodic_patterns(nt)
    calls = literal_array(nt, "CALL_FORMS")
    prim = literal_array(fb, "PRIMARY_CFGS")
    fall = literal_array(fb, "FALLBACK_CFGS")
    for label, v in (("FS_MUTATION_PATTERNS", fs_pat), ("WHITELIST_STILL_FORBIDDEN", wl_pat),
                     ("periodic_wake_patterns", wake), ("CALL_FORMS", calls),
                     ("PRIMARY_CFGS", prim), ("FALLBACK_CFGS", fall)):
        if not v:
            print(f"  CRASH: 读不出 `{label}` —— 判据面挪过位置了，本尺子当场作废")
            return 2
        print(f"  {label:<26} {len(v):>2} 条  {v}")

    wl_mod = re.search(r'const WRITE_WHITELIST_MODULE: &str = "([^"]+)"', ro)
    print(f"  唯一写盘白名单模块        {wl_mod.group(1) if wl_mod else '× 读不出'}")
    print("  人群（三道都一样的那半）  `CARGO_MANIFEST_DIR/src` **递归**读 `.rs` 文本，"
          "只剥 `#[cfg(test)]`")
    print("  ⇒ 判据是**文本**判据，不是编译判据 ⇒ 平台 cfg 不影响它看不看得见（见 三）")
    print(f"  fallback_guard 的人群更小：只扫 `src/platform/`"
          f"（`platform_sources()`，命中 {len(list((rd / 'platform').rglob('*.rs')))} 个 .rs）")

    print()
    print("═══ 二 · 复刻件自检（两条都过才许看下面的读数）")
    pos_ro = "fn f() { std::fs::write(p, b).unwrap(); }"          # readonly_guard 自己的阳性样本
    pos_nt = "fn f() { std::thread::sleep(step); }"               # no_timer_guard 禁的第一形
    h1 = hits(pos_ro, fs_pat, "写盘")
    h2 = hits(pos_nt, wake, "自醒")
    print(f"  ① 阳性样本命中：写盘 {len(h1)} 条 · 自醒 {len(h2)} 条"
          f"  ⇒ {'OK' if h1 and h2 else 'CRASH'}")
    if not (h1 and h2):
        return 2
    prod = daemon_prod(rd)
    total_bytes = sum(len(v[1]) for v in prod.values())
    viol_ro, viol_nt = [], []
    for rel, (raw_prod, cmt_stripped) in prod.items():
        if rel == (wl_mod.group(1) if wl_mod else ""):
            continue
        viol_ro += [(rel,) + h for h in hits(raw_prod, fs_pat, "写盘")]
        viol_nt += [(rel,) + h for h in hits(cmt_stripped, wake, "自醒")]
    print(f"  ② 今天的 daemon 生产段：{len(prod)} 个 .rs · {total_bytes} 字节 · "
          f"写盘命中 {len(viol_ro)} · 自醒命中 {len(viol_nt)}  ⇒ "
          f"{'OK（与门禁今天的绿对得上）' if not viol_ro and not viol_nt else 'CRASH（复刻件与本体不一致）'}")
    for v in (viol_ro + viol_nt)[:8]:
        print(f"       {v}")
    if viol_ro or viol_nt:
        return 2

    print()
    print("═══ 三 · `#[cfg(windows)]` 逃不逃得掉（合成样本，Linux 上判）")
    synth = ('#[cfg(windows)]\npub fn focus(h: isize) -> Result<(), String> {\n'
             '    std::thread::sleep(std::time::Duration::from_millis(100));\n'
             '    let _ = std::fs::write(cache, b"x");\n    Ok(())\n}\n')
    print(f"  写盘命中 {len(hits(synth, fs_pat, 'w'))} · 自醒命中 {len(hits(synth, wake, 'w'))}"
          f" ⇒ 塞进 `#[cfg(windows)]` **照样命中**（判据读的是文本）")
    print(f"  同一段若落在 `src/platform/` 下：fallback_guard 判 {fabricates(synth)}")
    print("  ⚠ 这一格是本尺子最要紧的一条：新形状（Windows 后端里有拉窗那一段）"
          "**不会**因为加了平台 cfg 就绕开这两道。")

    print()
    print("═══ 四 · 候选载荷空转 —— 按搬家粒度四档，各自会红在哪")
    seen = []
    for stage, fns in PAYLOAD_FNS.items():
        chunks = []
        for pat, label in fns:
            if label == "bind.rs 全文":
                chunks.append((label, 1, strip_cfg_test(bind_src)))
                continue
            got = cut_fn(bind_src, pat)
            if not got:
                print(f"  CRASH: 切不出 `{label}`（正则 {pat}）—— 尺子坏了，不是「没命中」")
                return 2
            ln, body = got
            chunks.append((label, ln, body))
        seen += [c[0] for c in chunks]
        acc = "\n".join(c[2] for c in chunks)
        hro = hits(strip_cfg_test(acc), fs_pat, "写盘")
        hnt = hits(production_code(acc), wake, "自醒")
        print(f"\n  【{stage}】切出 {len(chunks)} 段 · 共 {len(acc.splitlines())} 行"
              f"（累计，不含前几档）")
        for label, ln, body in chunks:
            print(f"      · {label}  bind.rs:{ln}  {len(body.splitlines())} 行")
        print(f"      readonly_guard 会红：{len(hro)} 处" + ("" if hro else "  ⇒ 不红"))
        for _, pat, n, line in hro[:8]:
            print(f"         `{pat}`  {line}")
        print(f"      no_timer_guard 会红：{len(hnt)} 处" + ("" if hnt else "  ⇒ 不红"))
        for _, pat, n, line in hnt[:8]:
            print(f"         `{pat}`  {line}")

    print()
    print("═══ 四·5 · 缓存那一档为什么不红 —— 写盘那一句藏在**宿主的 helper** 里")
    utils = (wt / "src-tauri/src/utils.rs").read_text(encoding="utf-8")
    got = cut_fn(utils, r"^pub fn atomic_write_json")
    if not got:
        print("  CRASH: 切不出 `atomic_write_json` —— 尺子坏了")
        return 2
    ln, body = got
    huro = hits(strip_cfg_test(body), fs_pat, "写盘")
    print(f"  `crate::utils::atomic_write_json`  src-tauri/src/utils.rs:{ln}"
          f"  {len(body.splitlines())} 行 ⇒ 判据命中 {len(huro)} 处：")
    for _, pat, n, line in huro:
        print(f"      `{pat}`  {line}")
    call_sites = [(n, l.strip()) for n, l in enumerate(bind_src.splitlines(), 1)
                  if "atomic_write_json" in l and not l.strip().startswith("//")]
    print(f"  bind.rs 里调它的地方：{len(call_sites)} 处 {[n for n, _ in call_sites]}"
          f"  —— **调用行本身一个针都不命中**（针表里没有 `atomic_write_json`）")
    print("  ⇒ 「缓存搬进后端 ⇒ readonly_guard 当场红」这句话**要看 helper 跟不跟着搬**：")
    print("     跟着搬 ⇒ 红（上面那几处）· 不搬（继续调宿主）⇒ 那就不是「搬进后端」")
    dfw = (rd / "control" / "fork_write.rs")
    if dfw.exists():
        fw = production_code(dfw.read_text(encoding="utf-8"))
        print(f"  后端今天唯一的写盘落点 `control/fork_write.rs`："
              f"`.create_new(true)` {fw.count('.create_new(true)')} 处 · "
              f"`.open(` {fw.count('.open(')} 处（判据要求两数相等）")

    print()
    print("═══ 五 · 那两个「拉窗」调用会不会被这两道判据看见")
    win_needles = ["SetForegroundWindow", "EnumWindows", "IsWindow", "AttachThreadInput",
                   "ShowWindow", "keybd_event"]
    covered = [w for w in win_needles if any(w in p for p in fs_pat + wake + calls)]
    print(f"  Win32 拉窗构件 {win_needles}")
    print(f"  被这三张针表覆盖的：{covered if covered else '一个都没有'}")
    print("  ⇒ 两道护栏拦的都**不是拉窗本身**：一道拦写盘、一道拦自己醒来。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
