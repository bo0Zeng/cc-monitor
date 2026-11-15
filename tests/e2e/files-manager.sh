#!/usr/bin/env bash
# 文件管理面的**真进程端到端**：起沙箱后端（家目录在本趟临时目录里、私有 tmux），经真的帧路径
# （`--stream` 的入方向读循环 → 命令表 → `Run::BlockingFace` → 写面）把文件管理那几条写命令各走一遍，
# 再跑一组寻址本机的 `transfer-copy`（本机两个目录之间：本机那一端两头都是它自己）。
#
# 验三样：
#   ① 落盘结果对（建 · 建目录 · 改名 · 删 · 存 · 复制 · 解压 · 分块提交 · 上传提交 · 一组 `transfer-copy`）；
#   ② 越界路径被拒、盘上一个字节不多（上跳段 · 绝对路径 · 根里一条指向根外的链接 · 改名顶掉已在的目标）；
#   ③ 只有文件管理那一族写得出来：同一棵树上跑一遍只读的 `files-*`（列 · 看 · 读 · 窥），树前后逐字节相同；
#      「写句柄只递给文件管理那一族」的类型那一半由 `readonly_guard` 的判据钉（本套件判的是行为那一半）。
#
# 单测不够的地方：单测直接调写面的函数，绕过了入方向读循环、命令表的档位与分派那一层递句柄 ——
#   本套件从 stdin 的帧进、stdout 的应答帧出，盯的正是那一层。
# 红线：不碰真家目录（HOME 指本趟沙箱）· 不碰缺省 tmux server（`sandbox-env.sh` 装私有 shim）· 不改后端行为（只跑它）。
. "$(cd "$(dirname "$0")" && pwd)/sandbox-env.sh"
# shellcheck source=tests/e2e/tmux-shim.sh
. "$(cd "$(dirname "$0")" && pwd)/tmux-shim.sh" e2eFiles
set -euo pipefail

E2E_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$E2E_DIR/../.." && pwd)"
BACKEND="${CCM_E2E_BACKEND:-${CCM_E2E_BACKEND_BIN:-$REPO/.build/backend/debug/cc-monitor-backend}}"
[ -x "$BACKEND" ] || { echo "backend 二进制不存在/不可执行：$BACKEND"; exit 1; }

WORK="$(mktemp -d /tmp/e2e-files.XXXXXX)"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
REAL_HOME="$HOME"
export HOME="$WORK/home"
mkdir -p "$HOME/.cc-monitor" "$WORK/claude/projects"
cleanup() {
  set +e
  rm -rf -- "$WORK"
  tmux_shim_cleanup
}
trap cleanup EXIT
: "$REAL_HOME"

echo "== 文件管理面 · 真进程端到端 =="
echo "backend: $BACKEND"
set +e
CLAUDE_CONFIG_DIR="$WORK/claude" python3 -I - "$BACKEND" "$WORK" <<'PY'
import atexit, hashlib, io, json, os, subprocess, sys, tarfile, threading, time, queue

backend, work = sys.argv[1:3]
home = os.environ["HOME"]
root = os.path.join(work, "root")
outside = os.path.join(work, "outside")
os.makedirs(root); os.makedirs(outside)
ceiling = float(os.environ.get("WALL_CEILING_S", "300"))

PASS = FAIL = SKIP = 0
def ok(m):
    global PASS; PASS += 1; print(f"  PASS {m}")
def bad(m, d=""):
    global FAIL; FAIL += 1; print(f"  FAIL {m}" + (f"\n       {d}" if d else ""))
def check(cond, m, d=""):
    ok(m) if cond else bad(m, d)

class Backend:
    """一个 `--stream` 的沙箱后端：往 stdin 写帧、按 id 收应答帧。"""
    def __init__(self, env_home, tag):
        env = dict(os.environ, HOME=env_home)
        self.err = open(os.path.join(work, f"{tag}.stderr"), "wb")
        self.p = subprocess.Popen([backend, "--", "--stream"], stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=self.err, env=env, cwd=work)
        self.replies, self.n = {}, 0
        self.cv = threading.Condition()
        self.hello = None
        threading.Thread(target=self._read, daemon=True).start()
        atexit.register(self.close)
        with self.cv:
            self.cv.wait_for(lambda: self.hello is not None, timeout=ceiling)
    def _read(self):
        for line in self.p.stdout:
            try:
                f = json.loads(line)
            except ValueError:
                continue
            with self.cv:
                k = f.get("kind")
                if k == "hello":
                    self.hello = f
                elif k == "reply":
                    self.replies[f.get("id")] = f
                self.cv.notify_all()
    def ask(self, cmd, args):
        self.n += 1
        rid = f"e2e-{cmd}-{self.n}"
        self.p.stdin.write((json.dumps({"id": rid, "cmd": cmd, "args": args}) + "\n").encode())
        self.p.stdin.flush()
        with self.cv:
            if not self.cv.wait_for(lambda: rid in self.replies, timeout=ceiling):
                return {"ok": False, "code": "e2e_timeout", "message": f"{cmd} 没有应答"}
            return self.replies.pop(rid)
    def close(self):
        try:
            self.p.stdin.close()
        except OSError:
            pass
        self.p.terminate()
        try:
            self.p.wait(timeout=ceiling)
        except subprocess.TimeoutExpired:
            self.p.kill()

def tree(d):
    """一棵树的逐字节样子：相对路径 → (类型, 内容摘要 / 链接文本, 权限位)。"""
    out = {}
    for base, dirs, files in os.walk(d):
        for n in sorted(dirs) + sorted(files):
            p = os.path.join(base, n)
            rel = os.path.relpath(p, d)
            st = os.lstat(p)
            if os.path.islink(p):
                out[rel] = ("link", os.readlink(p), 0)
            elif os.path.isdir(p):
                out[rel] = ("dir", "", st.st_mode & 0o7777)
            else:
                out[rel] = ("file", hashlib.sha256(open(p, "rb").read()).hexdigest(), st.st_mode & 0o7777)
    return out

def code_of(r):
    return r.get("code") or ""

def outcomes(r):
    """一组那几条（复制 · 删 · 改权限）的逐项结局：`[(state, code)]`；没回包 ⇒ 空。"""
    return [(o.get("state"), o.get("code")) for o in ((r.get("data") or {}).get("outcomes") or [])]

def all_done(r):
    return r.get("ok") and outcomes(r) and all(st == "done" for st, _ in outcomes(r))

A = Backend(home, "local")
if A.hello is None:
    bad("本机沙箱后端发出 hello", open(os.path.join(work, "local.stderr"), "rb").read()[-400:])
    print(f"===== 合计 PASS={PASS} FAIL={FAIL} ====="); sys.exit(1)
ok("本机沙箱后端发出 hello")

# ── ① 各条写命令走一遍，落盘结果对 ───────────────────────────────────────────
r = A.ask("files-create", {"root": root, "rel": "a.txt", "content": "hello\n"})
check(r.get("ok") and open(os.path.join(root, "a.txt"), "rb").read() == b"hello\n", "files-create 新建一份、内容对", r)
r = A.ask("files-create", {"root": root, "rel": "a.txt", "content": "again\n"})
check(code_of(r) == "exists" and open(os.path.join(root, "a.txt"), "rb").read() == b"hello\n",
      "files-create 撞了已在的 ⇒ exists，原文件一个字节不动", r)
r = A.ask("files-mkdir", {"root": root, "rel": "d"})
check(r.get("ok") and os.path.isdir(os.path.join(root, "d")), "files-mkdir 建出目录", r)
r = A.ask("files-rename", {"root": root, "from": "a.txt", "to": "d/b.txt"})
check(r.get("ok") and not os.path.exists(os.path.join(root, "a.txt"))
      and open(os.path.join(root, "d/b.txt"), "rb").read() == b"hello\n", "files-rename 同根内移动", r)
r = A.ask("files-create", {"root": root, "rel": "d/c.txt", "content": "c\n"})
check(r.get("ok") and open(os.path.join(root, "d/c.txt"), "rb").read() == b"c\n", "files-create 建进子目录", r)
r = A.ask("files-copy", {"root": root, "items": [{"from": "d", "to": "d2"}]})
check(all_done(r) and open(os.path.join(root, "d2/b.txt"), "rb").read() == b"hello\n"
      and open(os.path.join(root, "d2/c.txt"), "rb").read() == b"c\n", "files-copy 复制目录", r)
r = A.ask("files-delete", {"root": root, "items": [{"rel": "d2/b.txt"}]})
check(all_done(r) and not os.path.exists(os.path.join(root, "d2/b.txt")), "files-delete 删一份文件", r)
r = A.ask("files-delete", {"root": root, "items": [{"rel": "d2", "recursive": True}]})
check(all_done(r) and not os.path.exists(os.path.join(root, "d2")), "files-delete 递归删目录", r)

# 解压：包放进根里，解到一个新目录
buf = io.BytesIO()
with tarfile.open(fileobj=buf, mode="w") as t:
    for name, body in [("x/one.txt", b"one\n"), ("x/two.txt", b"two\n")]:
        ti = tarfile.TarInfo(name); ti.size = len(body); ti.mode = 0o644
        t.addfile(ti, io.BytesIO(body))
open(os.path.join(root, "pack.tar"), "wb").write(buf.getvalue())
r = A.ask("files-extract", {"root": root, "rel": "pack.tar"})
landed = [p for p in os.listdir(root) if p.startswith("pack")]
got = {}
for base, _, files in os.walk(root):
    for f in files:
        if f in ("one.txt", "two.txt"):
            got[f] = open(os.path.join(base, f), "rb").read()
check(r.get("ok") and got == {"one.txt": b"one\n", "two.txt": b"two\n"}, "files-extract 解出包里的两份", (r, landed))

# 分块提交：两块进暂存区 → 拼起来原地覆盖一份文本
key = "0123456789abcdef0123456789abcdef"
body = b"chunk-one|chunk-two\n"
old = open(os.path.join(root, "d/c.txt"), "rb").read()
r1 = A.ask("files-stage-chunk", {"key": key, "seq": 0, "content": "chunk-one|"})
r2 = A.ask("files-stage-chunk", {"key": key, "seq": 1, "content": "chunk-two\n"})
r = A.ask("files-commit-text", {"root": root, "rel": "d/c.txt", "key": key, "chunks": 2, "bytes": len(body),
                                 "expect": {"sha256": hashlib.sha256(old).hexdigest()}})
check(r1.get("ok") and r2.get("ok") and r.get("ok") and open(os.path.join(root, "d/c.txt"), "rb").read() == body,
      "files-stage-chunk ×2 ＋ files-commit-text 拼成一份、原地覆盖", (r1, r2, r))
staging = os.path.join(home, ".cc-monitor", "staging")
check(not [n for n in os.listdir(staging) if n.startswith(key)] if os.path.isdir(staging) else True,
      "提交之后这一键的块一块不留")

# 上传提交（分块形）：两块进暂存区 → 拼成 .part → 改名上位到一个新落点
key2 = "fedcba9876543210fedcba9876543210"
up = b"upload-body\n"
A.ask("files-stage-chunk", {"key": key2, "seq": 0, "content": "upload-"})
A.ask("files-stage-chunk", {"key": key2, "seq": 1, "content": "body\n"})
r = A.ask("files-commit-upload", {"root": root, "rel": "up.bin", "key": key2, "chunks": 2, "bytes": len(up),
                                   "expect": {"sha256": hashlib.sha256(up).hexdigest()}})
check(r.get("ok") and open(os.path.join(root, "up.bin"), "rb").read() == up, "files-commit-upload 上位到新落点", r)

# ── ② 越界路径被拒，盘上一个字节不多 ──────────────────────────────────────────
os.symlink(outside, os.path.join(root, "out-link"))
before_out = tree(outside)
for cmd, args, what in [
    ("files-create", {"root": root, "rel": "../escape.txt", "content": "x"}, "上跳段"),
    ("files-create", {"root": root, "rel": os.path.join(outside, "abs.txt"), "content": "x"}, "绝对路径"),
    ("files-create", {"root": root, "rel": "out-link/via-link.txt", "content": "x"}, "根里一条指向根外的链接"),
    ("files-mkdir", {"root": root, "rel": "out-link/dir"}, "建目录穿过链接"),
    ("files-rename", {"root": root, "from": "up.bin", "to": "out-link/moved.bin"}, "改名到根外"),
]:
    r = A.ask(cmd, args)
    check(not r.get("ok") and code_of(r) in ("refused", "bad_args", "bad_path"),
          f"{cmd} {what} ⇒ 拒（{code_of(r) or '没拒'}）", r)
# 一组那一条：整组照答，越界的那一项自己记成没做成（码 refused · bad_args · bad_path），不是整组被拒。
r = A.ask("files-copy", {"root": root, "items": [{"from": "up.bin", "to": "out-link/copied.bin"}]})
got = outcomes(r)
check((not r.get("ok") and code_of(r) in ("refused", "bad_args", "bad_path"))
      or (len(got) == 1 and got[0][0] == "failed" and got[0][1] in ("refused", "bad_args", "bad_path")),
      f"files-copy 复制到根外 ⇒ 那一项拒（{got or code_of(r) or '没拒'}）", r)
check(tree(outside) == before_out and not os.path.exists(os.path.join(work, "escape.txt")),
      "根外一个字节没多")
open(os.path.join(root, "keep.txt"), "wb").write(b"keep\n")
r = A.ask("files-rename", {"root": root, "from": "up.bin", "to": "keep.txt"})
check(code_of(r) == "exists" and open(os.path.join(root, "keep.txt"), "rb").read() == b"keep\n"
      and os.path.exists(os.path.join(root, "up.bin")), "改名顶不掉已在的目标（不覆盖改名）", r)

# ── ③ 只读的那几条在同一棵树上跑一遍，树前后逐字节相同 ────────────────────────────
snap = tree(root)
for cmd, args in [("files-ls", {"path": root}), ("files-stat", {"path": os.path.join(root, "keep.txt")}),
                  ("files-read", {"path": os.path.join(root, "keep.txt")})]:
    A.ask(cmd, args)
check(tree(root) == snap, "只读那几条（ls · stat · read）跑完，树逐字节没变")

# ── 传输：一组寻址本机的 `transfer-copy`（本机两个目录之间；两台之间那一维由读数脚本 `tests/evidence/sftp-loopback.py` 判）──
src_dir = os.path.join(work, "xfer-src"); os.makedirs(src_dir)
dst_dir = os.path.join(work, "xfer-dst"); os.makedirs(dst_dir)
payload = os.urandom(300 * 1024)
open(os.path.join(src_dir, "src.bin"), "wb").write(payload)
def copy_local(to_dir, name, **more):
    return A.ask("transfer-copy", dict({"from": "<local>", "to": "<local>", "to_dir": to_dir,
                                        "items": [{"path": os.path.join(src_dir, "src.bin"), "name": name}]}, **more))
r = copy_local(dst_dir, "got.bin")
check(all_done(r) and open(os.path.join(dst_dir, "got.bin"), "rb").read() == payload
      and not [n for n in os.listdir(dst_dir) if n.endswith(".part")],
      "transfer-copy 本机 → 本机：落点逐字节对、.part 不留", r)
# 落点已有同名、没答 ⇒ 整组不做、回 exists 带同名清单，已有那一份一个字节不动。
open(os.path.join(dst_dir, "got.bin"), "wb").write(b"mine\n")
r = copy_local(dst_dir, "got.bin")
names = [c.get("name") for c in ((r.get("data") or {}).get("conflicts") or [])]
check(not r.get("ok") and code_of(r) == "exists" and names == ["got.bin"]
      and open(os.path.join(dst_dir, "got.bin"), "rb").read() == b"mine\n",
      "transfer-copy 落点已有同名、没答 ⇒ exists 带那一项、已有那一份不动", r)
# 答了覆盖 ⇒ 盖成源的字节。
r = copy_local(dst_dir, "got.bin", decide={"0": "overwrite"})
check(all_done(r) and open(os.path.join(dst_dir, "got.bin"), "rb").read() == payload,
      "transfer-copy 答了覆盖 ⇒ 落点盖成源的字节", r)
# 落点目录是相对路径 ⇒ 整组 bad_args、一个字节不落（后端进程的工作目录钉在 `work`，那里先建好 `rel`：照相对路径解就会落进去）。
os.makedirs(os.path.join(work, "rel"))
r = copy_local("rel", "x.bin")
check(not r.get("ok") and code_of(r) == "bad_args",
      f"transfer-copy 落点目录是相对路径 ⇒ bad_args（{code_of(r) or outcomes(r)}）", r)
check(sorted(os.listdir(dst_dir)) == ["got.bin"] and os.listdir(os.path.join(work, "rel")) == [],
      "被拒的那一组一个字节没落")

A.close()
print(f"===== 合计 PASS={PASS} FAIL={FAIL} SKIP={SKIP} =====")
sys.exit(0 if FAIL == 0 and SKIP == 0 else 1)  # SKIP 没有登记过的豁免 ⇒ 判红
PY
rc=$?
set -e
left="$(e2e_leftovers "$HOME")"
if [ -n "$left" ]; then
  echo "  FAIL 本趟起的后端没走干净："; echo "$left"; exit 1
fi
exit "$rc"
