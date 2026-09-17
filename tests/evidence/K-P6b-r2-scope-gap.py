#!/usr/bin/env python3
"""`K-P6b` 第二轮的量具：① 把 `§0b` 那四条重打到本树尖 ② 把「E 落 `dial/mod.rs`」这条路
撞上的**写区缺口**逐条打出来。

# 它判什么

- **①（`D1②`）**：`§0b` 四条 —— 协议零改动 · 六根针 · additive · `Overflow.lost` 不动。
  其中 **additive 第一轮判不了**（无 diff），本轮盘上有 diff 了 ⇒ 这一格由 `git diff --numstat`
  的**删除行数**与**被改文件数**答，不由散文答。
- **②（`D2` 的前置）**：候选 E 的代理若住 `remote-daemon-proto/src/dial/mod.rs`，
  要**动到哪几份文件**；逐份标「在不在第二轮那 6 项逐文件写区里」。

# 它**不判**什么（说清楚，别让人读大）

- **不判 E 好不好、也不判该不该选 E**（那是 `D1③`，第一轮答过，选路仍挂在用户那里）。
- **不判 Windows 真机行为**：全程只读盘上文本与 `git`，**不起任何进程去连任何东西**。
- **不判「写区该不该扩」** —— 它只报「照这条路走会撞到哪几份文件、那几份在不在写区里」。
  扩不扩是 PM 的裁量。
- 第 ② 部分的每一条都是**盘上事实 + 一条判据的字面**，不是「我编译过一次」——
  **本量具一次 `cargo` 都不跑**（跑门禁是沙箱的活）。

# 分母怎么数的

- 「连 `russh` 都没有」的分母：`remote-daemon-proto/Cargo.lock` 里 `^name = "…"` 的**全部**包名，
  以及 `Cargo.toml` 里 `[dependencies]`/`[dev-dependencies]` 两节的**非注释**行。
- 「调用点」的分母：`git grep -n` 在**跟踪文件**里按 `connect_session(` 字面量数，
  **按 `#[cfg(test)]` 剥测试段**（剥法与 `K-P6b-ruler-diff.py` 那把「对的尺子」同源：
  `guard`档 = 逐个剥带花括号体的 `#[cfg(test)] mod X { … }`）。
  ⚠ 它**数不到** `use` 别名与函数指针 —— 与 `K-P6-dial-census.py` 头注同一个洞，不声称堵住。
- 行号一律**带校验位**：每个被点名的行号旁边印那一行的**逐字内容**（现读，不是抄的）。

用法：`python3 evidence/K-P6b-r2-scope-gap.py [base] [head]`（默认 `b4530d8` / `HEAD`）
"""

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]

# 第二轮派工单 §二 逐字那 6 项（**目录项按前缀认**）。
WRITE_AREA = [
    "features/K-P6b-把拨号搬出界面进程.md",
    "src-tauri/src/ssh_source.rs",
    "remote-daemon-proto/src/dial/mod.rs",
    "remote-daemon-proto/src/main.rs",
    "remote-daemon-proto/src/build_id_guard.rs",
    "evidence",
]


def sh(args, cwd=ROOT):
    p = subprocess.run(args, cwd=str(cwd), capture_output=True, text=True)
    return p.returncode, p.stdout, p.stderr


def git(*args):
    rc, out, err = sh(["git"] + list(args))
    if rc not in (0, 1):
        raise SystemExit(f"git {' '.join(args)} 失败 rc={rc}: {err}")
    return out


def in_write_area(path):
    """那一份在不在第二轮的逐文件写区里。`evidence` 是目录项 ⇒ 按前缀认。"""
    for w in WRITE_AREA:
        if path == w or path.startswith(w.rstrip("/") + "/"):
            return True
    return False


def mark(path):
    return "【在写区】" if in_write_area(path) else "🔴【不在写区】"


def line_at(relpath, n):
    """校验位：把那一行的逐字内容现读出来（`rstrip` 掉行尾换行，不改别的）。"""
    p = ROOT / relpath
    if not p.exists():
        return "<文件不存在>"
    lines = p.read_text(encoding="utf-8", errors="replace").splitlines()
    if not (1 <= n <= len(lines)):
        return f"<越界：本文件只有 {len(lines)} 行>"
    return lines[n - 1]


def cite(relpath, n):
    return f"    {relpath}:{n} 逐字 ⇒ {line_at(relpath, n)!r}"


# ── 剥测试段：与「对的那把尺子」同源（逐个剥带花括号体的 `#[cfg(test)] mod X { … }`）──
def strip_test_modules(src):
    out = []
    i = 0
    anchor = "#[cfg(test)]"
    while True:
        k = src.find(anchor, i)
        if k < 0:
            out.append(src[i:])
            break
        # 锚点之后第一个 `{`：只有它在同一段声明里（mod X {）才剥
        j = src.find("{", k)
        head = src[k:j] if j > 0 else ""
        if j < 0 or "mod " not in head or "\n\n" in head:
            out.append(src[i : k + len(anchor)])
            i = k + len(anchor)
            continue
        out.append(src[i:k])
        depth = 0
        m = j
        while m < len(src):
            if src[m] == "{":
                depth += 1
            elif src[m] == "}":
                depth -= 1
                if depth == 0:
                    break
            m += 1
        i = m + 1 if m < len(src) else len(src)
    return "".join(out)


def h(title):
    print()
    print("=" * 78)
    print(title)
    print("=" * 78)


def main():
    base = sys.argv[1] if len(sys.argv) > 1 else "b4530d8"
    head = sys.argv[2] if len(sys.argv) > 2 else "HEAD"
    head_sha = git("rev-parse", head).strip()
    print(f"树 = {ROOT}")
    print(f"base = {base}  ({git('rev-parse', base).strip()})")
    print(f"head = {head}  ({head_sha})")
    print(f"工作树 git status --porcelain 行数 = {len(git('status','--porcelain').splitlines())}")

    # ══ ① §0b 四条 ══════════════════════════════════════════════════════════
    h("① §0b 四条 —— 重打到 head")

    # ①-1 协议零改动
    d = git("diff", base, head, "--", "remote-daemon-proto/src/wire.rs",
            "remote-daemon-proto/src/inbound.rs")
    print(f"[①-1 协议零改动] git diff {base} {head} -- wire.rs inbound.rs ⇒ {len(d.encode())} 字节")
    print(f"  ⇒ {'成立（差集为空）' if not d.strip() else '🔴 不成立'}")
    # 反空真对照：同一把尺子量一份**真的动过**的文件，必须给出非空
    ctl = git("diff", base, head, "--", "remote-daemon-proto/src/dial/mod.rs")
    print(f"  非空对照（同一把尺子量 dial/mod.rs）⇒ {len(ctl.encode())} 字节"
          f" —— {'对照非空，尺子在动' if ctl.strip() else '🔴 对照也空 ⇒ 这把尺子没在跑'}")

    # ①-2 六根针
    src = (ROOT / "remote-daemon-proto/src/single_stream_guard.rs").read_text(encoding="utf-8")
    k = src.find("const PINS")
    blk = src[k:]
    pins = re.findall(
        r'\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,\s*(\d+)\s*,\s*(\d+)\s*,', blk)
    print(f"\n[①-2 六根针] `PINS` 现打 {len(pins)} 条（分母 = `single_stream_guard.rs` 里 "
          f"`const PINS` 之后所有 `(\"f\",\"n\",N,M,` 形状的元组头）")
    for f, n, a, b in pins:
        print(f"    {f} / {n!r} ⇒ file_want={a} crate_want={b}")
    # 这几根针本轮动没动：整块 md5 比两版
    import hashlib
    old = git("show", f"{base}:remote-daemon-proto/src/single_stream_guard.rs")
    new = git("show", f"{head}:remote-daemon-proto/src/single_stream_guard.rs")
    print(f"    整文件 md5：{base} = {hashlib.md5(old.encode()).hexdigest()[:12]} · "
          f"head = {hashlib.md5(new.encode()).hexdigest()[:12]} ⇒ "
          f"{'一字未动' if old == new else '🔴 动过'}")

    # ①-3 additive —— 本轮的正题
    print(f"\n[①-3 additive] git diff --numstat {base} {head}（分母 = 这一段 diff 里的**全部**文件）")
    rows = []
    for ln in git("diff", "--numstat", base, head).splitlines():
        if not ln.strip():
            continue
        add, dele, path = ln.split("\t", 2)
        rows.append((add, dele, path))
    tot_add = sum(int(a) for a, _, _ in rows if a != "-")
    tot_del = sum(int(d) for _, d, _ in rows if d != "-")
    for a, dl, p in rows:
        print(f"    +{a:<5} -{dl:<5} {p}   {mark(p)}")
    print(f"    合计 {len(rows)} 份 · +{tot_add} · -{tot_del}")
    touched_existing = [p for a, dl, p in rows if dl != "-" and int(dl) > 0]
    print(f"    有删除行的文件 {len(touched_existing)} 份 ⇒ "
          f"{'**additive 成立**（一行都没删、一行都没改）' if not touched_existing else '🔴 additive 不成立'}")
    # 新增文件 vs 改动已有文件（additive 的第二个面）
    names = git("diff", "--name-status", base, head).splitlines()
    added = [l.split("\t", 1)[1] for l in names if l.startswith("A\t")]
    modified = [l.split("\t", 1)[1] for l in names if l.startswith("M\t")]
    print(f"    新增文件 {len(added)} 份：{added}")
    print(f"    改动已有文件 {len(modified)} 份：{modified}")
    print("    ⚠ **这一格今天买得很便宜**：那唯一一份新文件参不参与编译，见 ②-G7。")

    # ①-4 Overflow.lost 不动
    print(f"\n[①-4 Overflow.lost 不动]")
    for n in (387, 391):
        print(cite("remote-daemon-proto/src/wire.rs", n))
    dw = git("diff", base, head, "--", "remote-daemon-proto/src/wire.rs")
    print(f"    git diff {base} {head} -- wire.rs ⇒ {len(dw.encode())} 字节 ⇒ "
          f"{'成立' if not dw.strip() else '🔴 不成立'}")

    # ══ ② 写区缺口 ═════════════════════════════════════════════════════════
    h("② E 落 `remote-daemon-proto/src/dial/mod.rs` 这条路撞到的文件，逐份标写区")

    # G1 daemon crate 没有 russh
    print("[②-G1] daemon crate 今天**一条 `russh` 依赖都没有**")
    lock = (ROOT / "remote-daemon-proto/Cargo.lock").read_text(encoding="utf-8")
    pkgs = re.findall(r'^name = "([^"]+)"$', lock, re.M)
    russh_pkgs = [p for p in pkgs if p.startswith("russh")]
    print(f"    remote-daemon-proto/Cargo.lock 包名 {len(pkgs)} 个（分母）⇒ "
          f"`russh*` {len(russh_pkgs)} 个 {russh_pkgs}")
    mlock = (ROOT / "src-tauri/Cargo.lock").read_text(encoding="utf-8")
    mpkgs = re.findall(r'^name = "([^"]+)"$', mlock, re.M)
    mrussh = [p for p in mpkgs if p.startswith("russh")]
    print(f"    对照 src-tauri/Cargo.lock 包名 {len(mpkgs)} 个 ⇒ `russh*` {len(mrussh)} 个 {mrussh}"
          f" —— 对照非空 ⇒ 这把尺子会数 `russh`，上面那个 0 不是尺子坏了")
    toml = (ROOT / "remote-daemon-proto/Cargo.toml").read_text(encoding="utf-8")
    noncomment = [l for l in toml.splitlines() if l.strip() and not l.lstrip().startswith("#")]
    print(f"    Cargo.toml 非注释行 {len(noncomment)} 行（分母）⇒ 含 `russh` 的 "
          f"{len([l for l in noncomment if 'russh' in l])} 行"
          f"（全文提到 `russh` 的 {len([l for l in toml.splitlines() if 'russh' in l])} 行**都是注释**）")
    print(f"    ⇒ 要让 `dial/` 真的拨 SSH，得改 `remote-daemon-proto/Cargo.toml` "
          f"{mark('remote-daemon-proto/Cargo.toml')}")

    # G1a：「不动 Cargo.toml 也能拨」的三条路，逐条查过再说走不通
    print("\n[②-G1a] 「不动 `Cargo.toml` 也让 `dial/` 拨得动」我查了三条路")
    sshish = [p for p in pkgs if re.search(r"ssh|sftp", p, re.I)]
    print(f"    路① 复用 daemon 现有依赖：lock 里 {len(pkgs)} 个包（分母）中"
          f"名字含 `ssh`/`sftp` 的 {len(sshish)} 个 {sshish} ⇒ 走不通")
    crates = ["acct-core", "branch-core", "creds-core", "gate-core",
              "guard-core", "shell-quote-core", "usage-core"]
    hit = []
    for c in crates:
        f = ROOT / "src-tauri/crates" / c / "Cargo.toml"
        if f.exists() and re.search(r"ssh", f.read_text(encoding="utf-8"), re.I):
            hit.append(c)
    print(f"    路② 借道那 7 个 path 依赖 crate（分母 = {len(crates)} 个）："
          f"Cargo.toml 里提到 `ssh` 的 {len(hit)} 个 {hit} ⇒ 走不通")
    print("    路③ 起一个外部 `ssh` 客户端进程：daemon 里起进程本身做得到（见 ④），"
          "但那要求目标机上**有**一个外部客户端，\n"
          "         并且把 **host key 指纹校验**与**凭据面**整个交出去 —— "
          "与 `ssh_source` 今天自己校验指纹这件事直接冲突。\n"
          "         ⚠ **这条我没有量它可不可行**，只是说它不是「免费」的那种躲法。")

    # G2 connect_session 的返回类型是 russh 类型
    print("\n[②-G2] `connect_session` 交出去的是一个 **russh 的进程内句柄** —— 它跨不了进程边界")
    for n in (34, 701, 704):
        print(cite("src-tauri/src/ssh_source.rs", n))

    # G3 调用点分布
    print("\n[②-G3] `connect_session` 的**生产段**调用点分布（分母 = 跟踪的 `.rs`，剥测试段）")
    tracked = [p for p in git("ls-files", "*.rs").splitlines() if p]
    hits = {}
    for p in tracked:
        try:
            t = (ROOT / p).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        prod = strip_test_modules(t)
        c = prod.count("connect_session(")
        if c:
            hits[p] = c
    total = sum(hits.values())
    print(f"    跟踪 `.rs` {len(tracked)} 份（分母）⇒ 命中 {len(hits)} 份 / {total} 处")
    for p in sorted(hits):
        print(f"    {hits[p]} 处  {p}   {mark(p)}")
    outside = [p for p in hits if not in_write_area(p)]
    print(f"    ⇒ 其中 {len(outside)} 份在写区外：{sorted(outside)}")
    print("    ⚠ 这个数含**定义行本身**与 `connect_via_jump` 里那次递归调用；"
          "它数不到 `use` 别名与函数指针。")

    # G4 新子命令 ⇒ 文档对拍面
    print("\n[②-G4] 往 `SUBCOMMANDS` 加一条 ⇒ **两半判据**，而写区只给了其中一半")
    for n in (1236, 1237):
        print(cite("remote-daemon-proto/src/main.rs", n))
    doc = (ROOT / "doc/IPC-PROTOCOL.md").read_text(encoding="utf-8")
    lines = doc.splitlines()
    sec = next(i for i, l in enumerate(lines) if l.startswith("## 10. "))
    nxt = next((i for i in range(sec + 1, len(lines)) if lines[i].startswith("## ")), len(lines))
    seg = "\n".join(lines[sec:nxt])
    print(f"    doc/IPC-PROTOCOL.md §10 = 第 {sec+1}..{nxt} 行（分母）⇒ `--relay` 在其中命中 "
          f"{seg.count('--relay')} 处 ⇒ 先例走的就是这条路")
    print(f"    判据：`protocol_doc_guard::every_dispatched_subcommand_appears_in_the_protocol_doc`")
    print(f"    ⇒ 加一条子命令要改 `doc/IPC-PROTOCOL.md` {mark('doc/IPC-PROTOCOL.md')}")

    # G5 DISPATCH_FILES
    print("\n[②-G5] `dial/mod.rs` 里只要出现一个 `\"--` 字面量，它就得进 `DISPATCH_FILES`")
    pg = (ROOT / "remote-daemon-proto/src/protocol_doc_guard.rs").read_text(encoding="utf-8")
    k = pg.find("const DISPATCH_FILES")
    blk = pg[k:pg.find("];", k)]
    # ⚠ 每条登记是 `("x.rs", include_str!("x.rs"))` —— **同一个名字出现两次**。
    # 头一版没去重，把 7 份印成了 14 份；去重之后再断言「两次出现的是同一个名字」。
    raw = re.findall(r'"([^"]+\.rs)"', blk)
    files = sorted(set(raw))
    assert len(raw) == 2 * len(files), f"登记表形状变了：raw={raw}"
    print(f"    现打 `DISPATCH_FILES` {len(files)} 份（分母 = 该常量块内 `\"*.rs\"` "
          f"字面量 {len(raw)} 个，每份登记两次）：{files}")
    print(f"    判据：`protocol_doc_guard::…::dispatch_registry_is_complete`"
          f"（扫全 crate 生产段找 `\"--`，要求与登记表**相等**）")
    print(f"    ⇒ 可能要改 `remote-daemon-proto/src/protocol_doc_guard.rs` "
          f"{mark('remote-daemon-proto/src/protocol_doc_guard.rs')}")

    # G6 no_timer_guard 人群
    print("\n[②-G6] 零定时器护栏的人群**递归覆盖** `src/` ⇒ `dial/` 一进来就在闸里")
    print(cite("remote-daemon-proto/src/no_timer_guard.rs", 6))
    ntg = (ROOT / "remote-daemon-proto/src/no_timer_guard.rs").read_text(encoding="utf-8")
    print(f"    `REGISTERED_DURATION_USES` 住址：no_timer_guard.rs（现打 "
          f"{ntg.count('REGISTERED_DURATION_USES')} 处提及）")
    ss = (ROOT / "src-tauri/src/ssh_source.rs").read_text(encoding="utf-8")
    print(f"    而拨号那一段**头一句就带定时器**：")
    for n in (712, 713, 714):
        print(cite("src-tauri/src/ssh_source.rs", n))
    print(f"    ⇒ 若代理要保住 keepalive/超时，要改 `remote-daemon-proto/src/no_timer_guard.rs` "
          f"{mark('remote-daemon-proto/src/no_timer_guard.rs')}")

    # G7 dial/mod.rs 今天不参与编译
    print("\n[②-G7] `dial/mod.rs` 今天**不参与编译**（`①-3` 那格 additive 便宜就便宜在这里）")
    mrs = (ROOT / "remote-daemon-proto/src/main.rs").read_text(encoding="utf-8")
    n_moddial = len(re.findall(r"^\s*(?:pub )?mod dial;", mrs, re.M))
    print(f"    main.rs 里 `mod dial;` 声明 {n_moddial} 处（分母 = main.rs 全文行）")
    dial = (ROOT / "remote-daemon-proto/src/dial/mod.rs").read_text(encoding="utf-8")
    dl = dial.splitlines()
    code = [l for l in dl if l.strip() and not l.lstrip().startswith("//")]
    print(f"    dial/mod.rs {len(dl)} 行 ⇒ 非注释非空 {len(code)} 行 {code}")

    h("③ 汇总：照 §0d 那条路走，要动而**写区里没有**的文件")
    need = [
        ("remote-daemon-proto/Cargo.toml", "G1 · 代理要拨 SSH 就要 `russh`（今天 daemon crate 零依赖）"),
        ("doc/IPC-PROTOCOL.md", "G4 · 加子命令必须进 §10 的代码跨度，否则 protocol_doc_guard 红"),
        ("remote-daemon-proto/src/protocol_doc_guard.rs", "G5 · 若 dial/mod.rs 里出现 `\"--` 字面量"),
        ("remote-daemon-proto/src/no_timer_guard.rs", "G6 · 若代理保住 keepalive/超时"),
        ("src-tauri/src/sftp.rs", "G3 · connect_session 的生产调用点之一"),
        ("src-tauri/src/port_forward.rs", "G3 · connect_session 的生产调用点之一"),
    ]
    for p, why in need:
        print(f"  {mark(p)} {p}\n      {why}")
    print("\n  ⚠ 这张表是**「照这条路走会撞到」**，不是「非改不可的最小集」——")
    print("     后两份可以靠「不动 `connect_session` 的签名、只加一条并行的路」躲开（`§2.2` 也不许动它们）；")
    print("     前四份躲不开：它们是判据/依赖，不是调用点。")

    # ══ ④ 顺手核一条散文（不挂 dod，交 PM 定去向）═════════════════════════════
    h("④ 顺手核：`relay/mod.rs` 头注那句「全 crate 唯一起进程口」")
    print("  那句话逐字（现读）：")
    rm = (ROOT / "remote-daemon-proto/src/relay/mod.rs").read_text(encoding="utf-8")
    for i, l in enumerate(rm.splitlines(), 1):
        if "唯一起进程口" in l:
            print(f"    remote-daemon-proto/src/relay/mod.rs:{i} 逐字 ⇒ {l!r}")
    print("  现打（分母 = daemon crate `src/` 递归全部跟踪 `.rs`，**剥测试段**后数 "
          "`Command::new(` 与 `process::Command`）：")
    spawn = {}
    for p in tracked:
        if not p.startswith("remote-daemon-proto/src/"):
            continue
        t = (ROOT / p).read_text(encoding="utf-8", errors="replace")
        c = strip_test_modules(t).count("Command::new(")
        if c:
            spawn[p] = c
    for p in sorted(spawn):
        print(f"    {spawn[p]} 处  {p}")
    print(f"    合计 {len(spawn)} 份 / {sum(spawn.values())} 处")
    print("  🔴 **上面这个数不许直接拿去断「唯一」** —— 它**剥了测试段、没剥注释、没剥字符串字面量**，"
          "而 `main.rs` 那 2 处现读全是散文/字面量（自查逮到的，头一版差点报出去）。")
    print("  ⇒ 改用**逐处点名 + 校验位**：下面这几处是我逐行读过的**真调用**，"
          "只要有一处成立，那句「唯一」就已经是假的：")
    for rel, n in [("remote-daemon-proto/src/control/kill.rs", 63),
                   ("remote-daemon-proto/src/control/launch.rs", 356),
                   ("remote-daemon-proto/src/observe/watcher.rs", 465),
                   ("remote-daemon-proto/src/observe/watcher.rs", 501)]:
        print(cite(rel, n))
    print("  ⇒ 🔴 `plugin/` **不是**全 crate 唯一起进程口，那句括号里的话今天是假的。")
    print("  ⚠ 这一格**不挂 dod**，也**不在本轮写区**（`relay/mod.rs` 不在那 6 项里）⇒ 只报，不改。")
    print("  ⚠ 诚实边界：我**没有**穷举全 crate 的起进程写法（`tokio::process`、全路径写法、"
          "间接封装都没数）⇒ 我给的是**点名的反例**，不是一个全集基数。")


if __name__ == "__main__":
    main()
