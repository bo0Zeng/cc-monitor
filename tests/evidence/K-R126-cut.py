#!/usr/bin/env python3
"""K-R126 变异台 —— 往 `relay/server.rs` 上切刀、并保证切得回来。

住址：`<本仓>/evidence/K-R126-cut.py`。被测对象 = **本文件所在的那棵工作树**
（`__file__` 现算，不收「树」参数；同一住址指两棵树是 `brief` 12 点名的静默假读数）。

纪律（`brief` 硬规则 7）：每一刀**先断言锚点恰好命中 N 次**，命中数不对就拒绝落地，
落地之后印一行「变异已落地」。备份住在 `/tmp/kr126-cut-<树名>-<路径指纹>/`，
`revert` 从备份整份还原（不走 `git checkout` —— 那会把同轮真正的修一起冲掉）。

⚠ 刀与刀之间**有重叠面**（`repro` 之外那几刀都切在同一个 `match outcome` 上）⇒
  重叠的两刀一起 `apply` 会在第二刀的锚点断言上被拒，这是**刻意的**：拒了比静默叠上去好。

用法：
  K-R126-cut.py list
  K-R126-cut.py apply <刀名> [<刀名> ...]
  K-R126-cut.py revert
  K-R126-cut.py status
"""
import hashlib
import os
import pathlib
import re
import shutil
import sys

WT = pathlib.Path(__file__).resolve().parent.parent
TARGET = WT / "remote-daemon-proto" / "src" / "relay" / "server.rs"
_fp = hashlib.sha256(str(WT).encode()).hexdigest()[:8]
BAK = pathlib.Path("/tmp") / f"kr126-cut-{WT.name}-{_fp}"

# 本件新加的那两条判据的名字。**只有一个住址**，阴性对照那一刀取这里。
NEW_JUDGMENTS = [
    "a_peer_that_left_costs_the_stub_one_connection_not_its_listener",
    "a_stub_failure_that_is_not_the_peer_leaving_still_brings_the_stub_down_loudly",
]

# 闭集那个具名常量的逐字原文 —— 摘成员 / 加成员两刀都切它。
_SET = """    const PEER_LEFT_KINDS: [std::io::ErrorKind; 3] = [
        std::io::ErrorKind::BrokenPipe,
        std::io::ErrorKind::ConnectionReset,
        std::io::ErrorKind::ConnectionAborted,
    ];
"""

# 「对端走了」那一档的收场 —— 修本体就住在这几行里。下面三刀都切它，各切一面。
_OUTCOME_HEAD = """                match outcome {
                    Ok(()) => {}
                    // 下游先走 —— 合法的客户端行为，只丢这一条连接，`listener` 照常接下一发。
                    Err(e) if peer_is_gone(&e) => {
                        aborted_c.fetch_add(1, Ordering::SeqCst);
                    }
"""

# ── 刀表 ──────────────────────────────────────────────────────────────────
# 每一刀：(锚点, 锚点该命中几次, 换上去的东西, 一句话)
CUTS: dict[str, tuple[str, int, str, str]] = {
    # ① 复现刀：把假上游的**每一块**都往后推 —— 这不是「让机器变慢」，
    #    是把 CI 上那个「桩写到一半、下游已经走了」的窗口**从概率变成必然**。
    #    它单独落地**不改任何判定逻辑**，只改时序 ⇒ 修好了的树上它必须仍然绿。
    "repro-slow-upstream": (
        "    fn send_chunk(s: &mut TcpStream, sent: &AtomicU64, payload: &[u8]) -> std::io::Result<()> {\n"
        "        let mut frame = format!(\"{:x}\\r\\n\", payload.len()).into_bytes();\n",
        1,
        "    fn send_chunk(s: &mut TcpStream, sent: &AtomicU64, payload: &[u8]) -> std::io::Result<()> {\n"
        "        std::thread::sleep(std::time::Duration::from_millis(300)); // K-R126 复现刀\n"
        "        let mut frame = format!(\"{:x}\\r\\n\", payload.len()).into_bytes();\n",
        "假上游每写一块前先睡 300ms ⇒ 下游（`head -n 1` 的桩启动器）必定已经走了",
    ),
    # ② 死值验刀①：把收场那一档**整个退回修之前** —— 对端走了也照旧 panic。
    "restore-expect": (
        _OUTCOME_HEAD,
        1,
        """                match outcome {
                    Ok(()) => {}
                    // K-R126 死值验刀①：退回修之前 —— 对端走了照旧 panic（＝ `.expect()` 那一版的行为）。
                    Err(e) if peer_is_gone(&e) => {
                        let _ = &aborted_c;
                        panic!("end: {e:?}");
                    }
""",
        "收场退回修之前：对端走了也 panic ⇒ 桩线程炸、listener 被一起 drop",
    ),
    # ③ 单断：`aborted` 照记（采集面自检过得了），但 `listener` 仍然跟着走。
    #    它把新判据的**后半截**单独摁出来 —— 少了这一刀，「listener 还活着」那几行
    #    的牙全靠刀② 的采集面自检代买，那是买不到的（`brief` 9「N 格单断」）。
    "listener-dies-after-abort": (
        _OUTCOME_HEAD,
        1,
        """                match outcome {
                    Ok(()) => {}
                    // K-R126 死值验单断：记数照记，但这一条连接把 listener 一起带走。
                    Err(e) if peer_is_gone(&e) => {
                        let _ = &e;
                        aborted_c.fetch_add(1, Ordering::SeqCst);
                        break;
                    }
""",
        "记了 aborted 但仍收掉 listener ⇒ 只打「listener 还活着」那半截",
    ),
    # ④ 单断：好连接也被记成 abort ⇒ 只打最后那一行「第二发不许也算 abort」。
    "count-good-connections-as-aborts": (
        "                match outcome {\n                    Ok(()) => {}\n",
        1,
        "                match outcome {\n"
        "                    // K-R126 死值验单断：好连接也记成 abort。\n"
        "                    Ok(()) => {\n"
        "                        aborted_c.fetch_add(1, Ordering::SeqCst);\n"
        "                    }\n",
        "走完的好连接也记进 aborted ⇒ 只打最后那一行「第二发不许也被记成 abort」",
    ),
    # ⑤ 量具刀（**不是变异**）：给每条连接的收场各印一行，用来做「谁撞上了这一形」的普查。
    #    它不改任何判定，只加输出 ⇒ 落地之后整族必须仍然绿。
    "probe-connection-outcomes": (
        _OUTCOME_HEAD,
        1,
        """                match outcome {
                    Ok(()) => {
                        eprintln!("[KR126CENSUS] ok");
                    }
                    // 下游先走 —— 合法的客户端行为，只丢这一条连接，`listener` 照常接下一发。
                    Err(e) if peer_is_gone(&e) => {
                        eprintln!("[KR126CENSUS] abort {:?}", e.kind());
                        aborted_c.fetch_add(1, Ordering::SeqCst);
                    }
""",
        "量具：每条连接的收场印一行（ok / abort）—— 不改判定，只加输出",
    ),
    # ⑥ 闭集**被摘掉一个成员** —— 判据该在「地板」那一条上红（两边 len 不等）。
    "shrink-peer-left-set": (
        _SET,
        1,
        """    const PEER_LEFT_KINDS: [std::io::ErrorKind; 2] = [
        std::io::ErrorKind::BrokenPipe,
        std::io::ErrorKind::ConnectionAborted,
    ];
""",
        "从「对端走了」的闭集里摘掉 `ConnectionReset`",
    ),
    # ⑦ 闭集**多了一个成员** —— 同一条地板反方向。
    "widen-peer-left-set": (
        _SET,
        1,
        """    const PEER_LEFT_KINDS: [std::io::ErrorKind; 4] = [
        std::io::ErrorKind::BrokenPipe,
        std::io::ErrorKind::ConnectionReset,
        std::io::ErrorKind::ConnectionAborted,
        std::io::ErrorKind::TimedOut,
    ];
""",
        "往「对端走了」的闭集里塞进第四种 `TimedOut`",
    ),
    # ⑧ 把**所有**错都当成「对端走了」—— 夹具从此再也不会大声炸。
    "swallow-every-error": (
        "    fn peer_is_gone(e: &std::io::Error) -> bool {\n"
        "        PEER_LEFT_KINDS.contains(&e.kind())\n"
        "    }\n",
        1,
        "    fn peer_is_gone(e: &std::io::Error) -> bool {\n"
        "        let _ = e;\n"
        "        true // K-R126 死值验：响度整个关掉\n"
        "    }\n",
        "`peer_is_gone` 恒真 ⇒ 任何错都只丢一条连接，桩再也不大声炸",
    ),
}

# 动态刀：锚点要从当前源码里现算（整块函数），不能写死在表里。
DYN = {
    f"drop-{n}": (n, f"把 `{n}` **整段**拿掉（含头注）—— 阴性对照用")
    for n in NEW_JUDGMENTS
}
# ──────────────────────────────────────────────────────────────────────────


def _fn_block(src: str, fnname: str) -> str:
    """返回 `fn <fnname>` 那一整段（含它上面连着的头注与属性行）的逐字文本。

    括号配平从 `fn` 那一行的第一个 `{` 开始数；
    往上吃掉紧挨着的 `///` 头注与 `#[...]` 属性行 —— 删判据要连头注一起删，
    否则留下一段指向不存在的判据的注释（`testing.md` 五⑷：那比没有注释更坏）。
    """
    m = re.search(rf"^([ \t]*)fn {re.escape(fnname)}\(", src, re.M)
    if not m:
        sys.exit(f"❌ 源码里找不到 `fn {fnname}`")
    start = m.start()
    # 往上吃头注 / 属性
    lines = src[:start].splitlines(keepends=True)
    while lines:
        t = lines[-1].strip()
        if t.startswith("///") or t.startswith("#[") or t.startswith("//"):
            start -= len(lines[-1])
            lines.pop()
        else:
            break
    # 往下括号配平
    i = src.index("{", m.start())
    depth, j = 0, i
    while j < len(src):
        if src[j] == "{":
            depth += 1
        elif src[j] == "}":
            depth -= 1
            if depth == 0:
                break
        j += 1
    end = j + 1
    while end < len(src) and src[end] == "\n":
        end += 1
    return src[start:end]


def _resolve(name: str, src: str) -> tuple[str, int, str, str]:
    if name in CUTS:
        return CUTS[name]
    if name in DYN:
        fnname, why = DYN[name]
        return (_fn_block(src, fnname), 1, "", why)
    sys.exit(f"❌ 没有这一刀：{name}")


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
    for name, (fnname, why) in DYN.items():
        print(f"  {name}  （动态：整段 `fn {fnname}`）— {why}")


def cmd_status():
    src = TARGET.read_text(encoding="utf-8")
    print(f"被测树：{WT}")
    print(f"md5(server.rs) = {hashlib.md5(src.encode()).hexdigest()}")
    for name in list(CUTS) + list(DYN):
        anchor, n, new, _ = _resolve(name, src)
        landed = (new in src) if new else (anchor not in src)
        print(f"  {name}: 已落地={landed} · 锚点现命中={src.count(anchor)}（该 {n}）")


def cmd_apply(names):
    _ensure_bak()
    src = TARGET.read_text(encoding="utf-8")
    pre = []
    for name in names:
        anchor, want, new, why = _resolve(name, src)
        hit = src.count(anchor)
        if hit != want:
            sys.exit(f"❌ 锚点命中 {hit} 次，该 {want} 次 ⇒ 这一刀不落地（{name}）")
        print(f"· 锚点断言过了：{name} 命中 {hit} 次（该 {want}）")
        pre.append((anchor, want, new, why))
        src = src.replace(anchor, new, 1)
    TARGET.write_text(src, encoding="utf-8")
    after = TARGET.read_text(encoding="utf-8")
    for name, (anchor, _w, repl, why) in zip(names, pre):
        # 落地自检：换字的那几刀要看见换上去的东西；删整段那一刀要看见锚点**没了**。
        ok = (repl in after) if repl else (anchor not in after)
        if not ok:
            sys.exit(f"❌ 落地自检没过：{name} —— 拒绝报「变异已落地」")
        print(f"变异已落地：{name} —— {why}")
    print(f"md5(server.rs) = {hashlib.md5(TARGET.read_bytes()).hexdigest()}")


def cmd_revert():
    f = BAK / "server.rs.orig"
    if not f.exists():
        sys.exit(f"❌ 没有备份（{f}）—— 拒绝假装还原了")
    # 🔴 **`copyfile` ＋ `os.utime`，不许换回 `copy2` —— 这里踩过一次，且仓里早有judge**：
    #   `copy2` 连 **mtime 一起还原**成备份那一刻的（旧的）⇒ 比上一趟编译的指纹还老
    #   ⇒ `cargo` 判「没变」**跳过重编** ⇒ 紧接着跑的那一趟，量的是**上一刀那个二进制**。
    #   它不报错、输出长得一模一样 —— 正是 `brief` 12 点名的**静默假读数**。
    #   实测症状（09-15 本件）：还原之后单跑新判据 200 趟，200 趟全红，
    #   而红的是**上一刀**那条断言。
    #   ⇒ 本仓纪律 ㉒（立于 `K-R75` 09-12）逐字禁的就是这一跳，判据住
    #     `evidence/K-R115-ruler.py`：目的地落在**被 git 跟踪的工作树内容**上的
    #     `copy2`/`copytree`/`copystat` 一律红。备份那一跳（目的地在 `/tmp`）不在射程里。
    shutil.copyfile(f, TARGET)
    f.unlink()
    os.utime(TARGET, None)
    print(f"· 已从备份整份还原：{TARGET}（并把 mtime 推到现在，逼 cargo 重编）")
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
