#!/usr/bin/env python3
"""第三方许可声明的生成器：产物里随分发附上依赖的版权与许可全文。

产出两份（都进仓，都由本脚本生成，不手改）：
  · `THIRD-PARTY-NOTICES.txt` —— 随安装包（`tauri.sidecar.conf.json` 的 `bundle.resources`）与 Release 资产分发；
  · `tests/evidence/third-party-not-shipped.txt` —— 锁文件里有、但不进产物的包（只在别的平台 / 只开发期用）。
两份合起来 == 两份 `Cargo.lock` 的第三方包；`package-lock.json` 的生产依赖全在声明里。
门禁 `release-gate`（`tests/evidence/release-ruler.py` ⑰）两向核这两句；发版那一趟（`release.yml`）跑 `--check`。

来源：
  · Rust（后端 `src/backend` ＋ 壳与文件窗口 `src/frontend/shell` 整个工作区 ＋ 手机嵌的 Rust 核心 `src/mobile/core-rs/rust`）：
    cargo-about（版本钉在 `CARGO_ABOUT_VERSION`），离线读 cargo 注册表缓存里各包自带的许可文件；目标平台与收纳的许可住
    `tests/scripts/third-party-about.toml`；开发期依赖不收。
  · npm：`package-lock.json` 里不带 `dev` 的包，读 `node_modules/` 里各包自带的许可文件（要先 `npm ci`）。
  · Maven（手机 `:app` 打进 APK 的那一档）：坐标读 `src/mobile/app/gradle.lockfile` 里 `releaseRuntimeClasspath` 那几行，
    许可读 `tests/scripts/third-party-maven.tsv`（坐标 · 许可 · 出处）；锁里有、表里没有 ⇒ 生成失败，表里多出来的也失败。
    依赖变了：先在 `src/mobile` 重写锁（`./gradlew :app:dependencies --configuration releaseRuntimeClasspath --write-locks`），
    再照新坐标自带的 POM 补表。
  · 仓里原样放进来的第三方文件（手机端 `src/mobile` 的预编库与字体 · 两端共用的 `src/shared` 里的字体，不经 cargo / npm 解析）：
    登记在 `VENDORED_FILES`，许可全文跟着放在仓里；每个发版附的整仓源码包里有它们。`VENDORED_ROOTS` 下被跟踪的二进制
    （aar · jar · so · 字体）每一份都得落在某一行的位置里，漏登记 ⇒ 生成失败。
四家同一道：依赖的许可（SPDX 表达式）不在 `third-party-about.toml` 的 `accepted` 名单里 ⇒ 生成失败，点出是哪几个。

用法（仓根下）：
    python3 tests/scripts/third-party-notices.py           # 重新生成两份
    python3 tests/scripts/third-party-notices.py --check   # 生成到临时处、与仓里那两份逐字比，不同 ⇒ 退出码 1
cargo-about 找法：PATH 上版本对的那个 ⇒ `.build/tools/bin/cargo-about` ⇒ 都没有就 `cargo install` 进 `.build/tools`（不碰家目录）。
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CARGO_ABOUT_VERSION = "0.9.2"
CONFIG = ROOT / "tests/scripts/third-party-about.toml"
NOTICES = ROOT / "THIRD-PARTY-NOTICES.txt"
NOT_SHIPPED = ROOT / "tests/evidence/third-party-not-shipped.txt"
#: (清单, 是不是整个工作区)
TREES = [("src/backend/Cargo.toml", False), ("src/frontend/shell/Cargo.toml", True),
         # 手机嵌的 Rust 核心（`libmobcore.so`，进 APK）：后端那一棵 workspace 的成员，依赖闭包单算（拨号核心 ＋ jni）。
         ("src/mobile/core-rs/rust/Cargo.toml", False)]
LOCKS = ["src/backend/Cargo.lock", "src/frontend/shell/Cargo.lock"]
LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|notice)", re.I)
#: (组件, 版本, SPDX, 在仓里的位置（目录以 / 结尾）, 许可全文所在文件, 说明)
VENDORED_FILES = [
    ("termlib", "0.1.2", "Apache-2.0", "src/mobile/libs/m2/org/connectbot/termlib/",
     "src/mobile/libs/LICENSES/Apache-2.0.txt",
     "connectbot/termlib 的预编 aar（含 sources.jar）；打过 src/mobile/libs/termlib-patches/ 里的补丁，是改过的版本"),
    ("libvterm", "termlib 0.1.2 内含", "MIT", "src/mobile/libs/m2/org/connectbot/termlib/0.1.2/termlib-0.1.2.aar",
     "src/mobile/libs/LICENSES/libvterm-MIT.txt", "编进 termlib aar 的 libjni_cb_term.so"),
    ("JetBrains Mono", "2.305", "OFL-1.1", "src/mobile/core-ui/src/main/res/font/",
     "src/mobile/libs/LICENSES/JetBrainsMono-OFL-1.1.txt", "jetbrains_mono_regular.ttf · jetbrains_mono_medium.ttf"),
    ("Phosphor Icons", "2.1", "MIT", "src/mobile/core-ui/src/main/res/font/phosphor_regular.ttf",
     "src/mobile/libs/LICENSES/Phosphor-MIT.txt", "Phosphor regular 图标字体的子集（只留手机用到的码位，src/mobile/scripts/phosphor-subset.py 从 egui-phosphor 0.14.0 自带的那份做）"),
    ("Gradle Wrapper", "8.14.3", "Apache-2.0", "src/mobile/gradle/wrapper/gradle-wrapper.jar",
     "src/mobile/libs/LICENSES/Apache-2.0.txt", "手机端构建用的 Gradle 启动器，不进 app"),
    ("KaTeX fonts", "ratex-katex-fonts 0.1.15", "OFL-1.1", "src/shared/fonts/katex/",
     "src/shared/fonts/katex/OFL.txt",
     "画公式用的 19 份 KaTeX TTF（未改动，取自 crates.io 的 ratex-katex-fonts 0.1.15，它取自 KaTeX）；桌面打进前端产物、手机构建时铺进 assets"),
]
#: 原样放第三方文件的目录：这几处下被跟踪的二进制都得在 `VENDORED_FILES` 里登记。
VENDORED_ROOTS = ["src/mobile", "src/shared"]
VENDORED_BINARY = re.compile(r"\.(aar|jar|so|ttf|otf|woff2?)$", re.I)
#: 手机 `:app` 进发布包的那一档 Maven 依赖：坐标只认 Gradle 锁文件（`releaseRuntimeClasspath`，测试 · 基准 · 构建插件不在里面），
#: 许可只认 `MAVEN_TABLE`（每个坐标一行：许可 · 出处，出处是它自带的 POM 或包里的许可文件）。两份都在仓里，不跑 Gradle、不联网。
MAVEN_LOCK = ROOT / "src/mobile/app/gradle.lockfile"
MAVEN_CONF = "releaseRuntimeClasspath"
MAVEN_TABLE = ROOT / "tests/scripts/third-party-maven.tsv"
#: Maven 那一段的许可全文：许可 → (仓里那份全文, 组头写的名字)。用到的许可缺全文 ⇒ 生成失败。
MAVEN_TEXTS = {"Apache-2.0": ("src/mobile/libs/LICENSES/Apache-2.0.txt", "Apache License 2.0")}

MIT_TEXT = """MIT License

Copyright (c) {holder}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"""


def die(msg):
    print("third-party-notices: " + msg, file=sys.stderr)
    sys.exit(2)


def cargo_about():
    def version_of(exe):
        try:
            out = subprocess.run([exe, "--version"], capture_output=True, text=True, check=True).stdout
        except (OSError, subprocess.CalledProcessError):
            return None
        return out.split()[-1] if out.strip() else None

    local = ROOT / ".build/tools/bin/cargo-about"
    for exe in (shutil.which("cargo-about"), str(local)):
        if exe and version_of(exe) == CARGO_ABOUT_VERSION:
            return exe
    print("third-party-notices: 装 cargo-about %s 进 .build/tools（只此一次）…" % CARGO_ABOUT_VERSION, file=sys.stderr)
    subprocess.run(["cargo", "install", "--locked", "--features", "cli", "--root", str(ROOT / ".build/tools"),
                    "cargo-about@" + CARGO_ABOUT_VERSION], check=True)
    if version_of(str(local)) != CARGO_ABOUT_VERSION:
        die("装完了还是找不到 cargo-about %s" % CARGO_ABOUT_VERSION)
    return str(local)


def is_ours(pkg):
    """本仓自己的包：路径来源（锁里没有 `source`）。"""
    return pkg.get("source") is None


def accepted_licenses():
    m = re.search(r"^accepted = \[(.*?)\]", CONFIG.read_text(encoding="utf-8"), re.M | re.S)
    return set(re.findall(r'"([^"]+)"', m.group(1))) if m else set()


def satisfied(expr, accepted):
    """一条 SPDX 许可表达式照名单收不收：`OR` 有一支收就收 · `AND` 每一支都收才收 · `X WITH 例外` 整条照名单；认不出的写法不收。"""
    toks = re.findall(r"\(|\)|[^\s()]+", expr or "")
    pos = 0

    def peek():
        return toks[pos] if pos < len(toks) else None

    def atom():
        nonlocal pos
        t = peek()
        if t is None or t in ("OR", "AND", "WITH", ")"):
            raise ValueError(expr)
        pos += 1
        if t == "(":
            v = either()
            if peek() != ")":
                raise ValueError(expr)
            pos += 1
            return v
        if peek() == "WITH":
            pos += 1
            ex = peek()
            if ex is None or ex in ("OR", "AND", "WITH", "(", ")"):
                raise ValueError(expr)
            pos += 1
            return "%s WITH %s" % (t, ex) in accepted
        return t in accepted

    def both():
        nonlocal pos
        v = atom()
        while peek() == "AND":
            pos += 1
            v = atom() and v
        return v

    def either():
        nonlocal pos
        v = both()
        while peek() == "OR":
            pos += 1
            v = both() or v
        return v

    try:
        ok = either()
    except ValueError:
        return False
    return ok and pos == len(toks)


def refuse_off_list(source, items):
    """`items`：[(依赖, 许可)]。有一个的许可不在名单里 ⇒ 生成失败，点出是哪几个（四家同一道）。"""
    accepted = accepted_licenses()
    off = sorted("%s (%s)" % (who, lic) for who, lic in items if not satisfied(lic, accepted))
    if off:
        die("%s 依赖的许可不在 %s 的 accepted 名单里：%s —— 换掉这个依赖，或人定了再把许可加进名单"
            % (source, CONFIG.name, off))


def rust_doc_groups(doc, manifest, groups):
    """cargo-about 对一棵树的结果并进 `groups`（许可 → 用它的第三方包）。"""
    unknown = sorted("%s %s" % (c["package"]["name"], c["package"]["version"])
                     for c in doc["crates"] if c["license"] == "Unknown" and not is_ours(c["package"]))
    if unknown:
        die("这几个第三方包认不出许可（%s）：%s —— 在 %s 里给它们写 clarify" % (manifest, unknown, CONFIG.name))
    picked = []
    for lic in doc["licenses"]:
        users = {"%s %s" % (u["crate"]["name"], u["crate"]["version"])
                 for u in lic["used_by"] if not is_ours(u["crate"])}
        picked += [(u, lic["id"]) for u in users]
        if users:
            groups.setdefault((lic["id"], lic["name"], lic["text"].strip("\n")), set()).update(users)
    refuse_off_list("Rust（%s）" % manifest, picked)
    return groups


def rust_groups():
    exe = cargo_about()
    groups = {}
    for manifest, workspace in TREES:
        subprocess.run(["cargo", "fetch", "--locked", "--manifest-path", str(ROOT / manifest)], check=True,
                       stdout=subprocess.DEVNULL)
        with tempfile.TemporaryDirectory() as td:
            out = Path(td) / "about.json"
            cmd = [exe, "generate", "--frozen", "--format", "json", "-c", str(CONFIG),
                   "-m", str(ROOT / manifest), "-o", str(out)]
            if workspace:
                cmd.append("--workspace")
            r = subprocess.run(cmd, capture_output=True, text=True)
            if r.returncode != 0:
                die("cargo-about 在 %s 上失败：\n%s" % (manifest, r.stderr[-4000:]))
            doc = json.loads(out.read_text(encoding="utf-8"))
        rust_doc_groups(doc, manifest, groups)
    return groups


def npm_entries():
    lock = json.loads((ROOT / "package-lock.json").read_text(encoding="utf-8"))
    out = []
    for key, meta in sorted(lock["packages"].items()):
        if not key or meta.get("dev"):
            continue
        name = key.rsplit("node_modules/", 1)[-1]
        pdir = ROOT / key
        if not (pdir / "package.json").is_file():
            die("%s 不在盘上 —— 先 npm ci" % key)
        pj = json.loads((pdir / "package.json").read_text(encoding="utf-8"))
        spdx = pj.get("license") or meta.get("license") or "?"
        files = sorted(p for p in pdir.iterdir() if p.is_file() and LICENSE_FILE.match(p.name))
        texts = [p.read_text(encoding="utf-8", errors="replace").strip("\n") for p in files
                 if not p.name.lower().endswith(".spdx")]
        if not texts:
            holder = None
            for p in files:
                m = re.search(r"^PackageCopyrightText:\s*(.+)$", p.read_text(encoding="utf-8"), re.M)
                holder = holder or (m and m.group(1).strip())
            authors = pj.get("authors") or ([pj["author"]] if pj.get("author") else [])
            holder = holder or ", ".join(a if isinstance(a, str) else a.get("name", "") for a in authors)
            if "MIT" not in re.split(r"[\s()]+", spdx) or not holder:
                die("%s 没带许可全文（许可 %s）—— 补一条做法再生成" % (name, spdx))
            texts = [MIT_TEXT.format(holder=holder).strip("\n")]
        out.append((name, meta["version"], spdx, texts))
    refuse_off_list("npm", [("%s %s" % (name, ver), spdx) for name, ver, spdx, _ in out])
    return out


def vendored_entries():
    tracked = subprocess.run(["git", "ls-files", "-z", "--", *VENDORED_ROOTS], capture_output=True, text=True,
                             check=True).stdout.split("\0")
    out = []
    for name, ver, spdx, where, text_file, note in VENDORED_FILES:
        hits = [t for t in tracked if t == where or (where.endswith("/") and t.startswith(where))]
        if not hits:
            die("VENDORED_FILES 里 %s 登记的位置 %s 一份被跟踪的文件都没有 —— 删掉这一行或改位置" % (name, where))
        text_path = ROOT / text_file
        if not text_path.is_file():
            die("%s 的许可全文 %s 不在盘上" % (name, text_file))
        out.append((name, ver, spdx, where, note, text_path.read_text(encoding="utf-8").strip("\n")))
    stray = sorted(t for t in tracked if VENDORED_BINARY.search(t)
                   and not any(t == w or (w.endswith("/") and t.startswith(w)) for _, _, _, w, _, _ in VENDORED_FILES))
    if stray:
        die("%s 下这几份第三方二进制没登记许可：%s —— 在 VENDORED_FILES 里加一行" % (" · ".join(VENDORED_ROOTS), stray))
    refuse_off_list("仓里原样放的文件", [("%s %s" % (name, ver), spdx) for name, ver, spdx, _, _, _ in out])
    return out


def maven_groups():
    """许可 → [(坐标 `组:件 版本`, 出处)]。"""
    if not MAVEN_LOCK.is_file():
        die("%s 不在 —— 在 src/mobile 里写锁再生成" % MAVEN_LOCK.relative_to(ROOT).as_posix())
    locked = {"%s:%s %s" % (g, a, v) for g, a, v, confs in
              re.findall(r"^([^#\s:]+):([^:\s]+):([^=\s]+)=(\S+)$", MAVEN_LOCK.read_text(encoding="utf-8"), re.M)
              if MAVEN_CONF in confs.split(",")}
    if not locked:
        die("%s 里没有 %s 那一档" % (MAVEN_LOCK.name, MAVEN_CONF))
    table = {}
    for n, line in enumerate(MAVEN_TABLE.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        cols = line.split("\t")
        if len(cols) != 3 or not all(cols) or cols[0].count(":") != 2:
            die("%s 第 %d 行要写成 `组:件:版本<TAB>许可<TAB>出处`" % (MAVEN_TABLE.name, n))
        g, a, v = cols[0].split(":")
        table["%s:%s %s" % (g, a, v)] = (cols[1], cols[2])
    if locked - table.keys():
        die("锁里有、%s 里没有许可的：%s —— 照它自带的 POM 补一行" % (MAVEN_TABLE.name, sorted(locked - table.keys())))
    if table.keys() - locked:
        die("%s 里有、锁里没有的：%s —— 删掉这几行" % (MAVEN_TABLE.name, sorted(table.keys() - locked)))
    refuse_off_list("Maven", [(coord, table[coord][0]) for coord in sorted(locked)])
    groups = {}
    for coord in sorted(locked):
        lic, src = table[coord]
        if lic not in MAVEN_TEXTS:
            die("%s 用的许可 %s 在仓里没有全文 —— 在 MAVEN_TEXTS 里登记一份" % (coord, lic))
        groups.setdefault(lic, []).append((coord, src))
    return groups


def lock_crates():
    got = set()
    for lock in LOCKS:
        text = (ROOT / lock).read_text(encoding="utf-8")
        for name, ver, src in re.findall(
                r'\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"\n(?:source = "([^"]+)")?', text):
            if src:
                got.add("%s %s" % (name, ver))
    return got


def render():
    groups = rust_groups()
    npm = npm_entries()
    lines = [
        "cc-monitor — 第三方许可声明 / Third-party notices",
        "",
        "本程序包含下列第三方组件。各组件的版权归其作者所有，按各自的许可分发；下面逐组附上许可全文。",
        "This program includes the third-party components listed below, each distributed under its own",
        "license. The full license texts follow.",
        "",
        "（本文件由 tests/scripts/third-party-notices.py 生成，不手改。）",
        "",
        "",
        "================================ Rust crates ================================",
    ]
    declared = set()
    for (lid, lname, text), users in sorted(groups.items(), key=lambda kv: (kv[0][0], sorted(kv[1])[0])):
        lines += ["", "-------- %s (%s) --------" % (lname, lid), "Used by:"]
        lines += ["    crate: %s" % u for u in sorted(users)]
        lines += ["", text, ""]
        declared |= users
    lines += ["", "================================ npm packages ================================"]
    for name, ver, spdx, texts in npm:
        lines += ["", "-------- %s %s (%s) --------" % (name, ver, spdx), "    npm: %s %s" % (name, ver)]
        for t in texts:
            lines += ["", t, ""]
    lines += ["", "================================ Maven artifacts (mobile app) ================================"]
    for lic, users in sorted(maven_groups().items()):
        lines += ["", "-------- %s (%s) --------" % (MAVEN_TEXTS[lic][1], lic), "Used by:"]
        lines += ["    maven: %s" % c for c, _ in users]
        lines += ["", (ROOT / MAVEN_TEXTS[lic][0]).read_text(encoding="utf-8").strip("\n"), ""]
    lines += ["", "=================== files vendored in the repo (src/mobile · src/shared) ==================="]
    for name, ver, spdx, where, note, text in vendored_entries():
        lines += ["", "-------- %s %s (%s) --------" % (name, ver, spdx), "    vendored: %s %s — %s" % (name, ver, where),
                  "    %s" % note, "", text, ""]
    notices = "\n".join(lines).replace("\r\n", "\n").replace("\r", "\n").rstrip("\n") + "\n"

    lock = lock_crates()
    stray = sorted(declared - lock)
    if stray:
        die("声明里有、锁文件里没有：%s" % stray)
    rest = sorted(lock - declared)
    not_shipped = "\n".join([
        "# 锁文件里有、但不进产物的第三方包（只在别的平台 / 只开发期 / 没开的特性用）。",
        "# 由 tests/scripts/third-party-notices.py 生成（== 两份 Cargo.lock 的第三方包 − THIRD-PARTY-NOTICES.txt 里的），不手改。",
    ] + ["crate: %s" % c for c in rest]) + "\n"
    return notices, not_shipped


def main():
    check = "--check" in sys.argv[1:]
    notices, not_shipped = render()
    if not check:
        NOTICES.write_text(notices, encoding="utf-8", newline="\n")
        NOT_SHIPPED.write_text(not_shipped, encoding="utf-8", newline="\n")
        print("third-party-notices: 写好 %s（%d 字节）与 %s" % (NOTICES.name, len(notices.encode()), NOT_SHIPPED.name))
        return 0
    bad = [p.relative_to(ROOT).as_posix() for p, want in ((NOTICES, notices), (NOT_SHIPPED, not_shipped))
           if not p.is_file() or p.read_bytes() != want.encode("utf-8")]
    if bad:
        print("::error::第三方许可声明与现打的依赖对不上：%s —— 跑 python3 tests/scripts/third-party-notices.py 重新生成再提交"
              % ", ".join(bad))
        return 1
    print("third-party-notices: 仓里那两份与现打的依赖逐字一致")
    return 0


if __name__ == "__main__":
    os.chdir(ROOT)
    sys.exit(main())
