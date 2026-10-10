#!/usr/bin/env python3
"""
性能台架的 WebKitGTK 那一组：Linux 版 cc-monitor 的窗口就是 WebKitGTK，同一个世界、同一个探针在它里面再量一遍。

    python3 tests/shots/perf/webkit.py [--runs 3] [--out <目录>] [--css <文件>] [--port <已起好的伺服口>]

- 私有 Xvfb（`tests/scripts/xvfb-free.sh` 挑号），一扇 1280×800 的 GTK 窗口里放一个 WebKitWebView；
  伺服用 `serve.mjs`（生产构建 ＋ 假后端），HOME 隔离进 `.build/perf-sandbox/`。不起后端、不起 claude、不碰 tmux。
- WebKit 没有 CDP、没有长任务 / Event Timing：点击用页里派发的鼠标事件（tab 栏的处理照常跑），
  「画出来」取点击之后第二个 rAF（切换之后第一帧整帧画完）；卡顿取窗口里的帧间隔（rAF 链）：> 50 ms 的帧数与合计超出；
  「接骨架那一帧」＝ 插骨架占位那一刻所在那一帧的帧间隔；「停住之后最长一帧」＝ 点下去 150 ms（宿主的停留判定）之后、切换那一帧画完之后结束的帧里最长那一帧（冷切时就是接骨架、补可见区那一帧）。
- 量法与 `bench.mjs` 同名的三项对齐：切一下（冷 / 热）· 连续快速切（20 下 / 每 200 ms，3 串）· 长会话（冷切进去 ＋ 往上滚 60 下）。
- 设置窗那一组（`--only settings`，与 `settings-bench.mjs` 对齐）：开窗 · 每一页点两遍 · 扩展页逐字筛选 · 关了再开（常驻内存走势）。
"""
import argparse
import json
import os
import socket
import subprocess
import sys
import time

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "../../.."))

ap = argparse.ArgumentParser()
ap.add_argument("--runs", type=int, default=3)
ap.add_argument("--out", default=os.path.join(REPO, ".build/perf-webkit"))
ap.add_argument("--css", default=None)
ap.add_argument("--port", type=int, default=None)
ap.add_argument("--only", default="switch,rapid,keys,long", help="switch,rapid,keys,long,boot 量主窗口；settings 量设置窗")
ap.add_argument("--shot", action="store_true", help="只截几张图看样子（开页 · 切到最长那条 · 往上滚一段 · 切到一条短的）")
ap.add_argument("--merge", default=None, help="几次分开跑的读数合成一张：目录1,目录2,…")
ap.add_argument("--dev", action="store_true", help="开发服务器（模块按源码路径可 import ⇒ 能给方法挂计时）")
ap.add_argument("--profile-pass", default="warm", help="计时记哪一遍：cold（第一次进每个 tab）/ warm")
ap.add_argument("--profile", default=None, help="开页安静后在页里跑的一段函数体（挂计时）；切一下那一项末尾把 window.__prof 存进 prof.json")
args = ap.parse_args()
only = set(args.only.split(","))
os.makedirs(args.out, exist_ok=True)

probe = open(os.path.join(HERE, "probe.js"), encoding="utf8").read()
if args.css:
    css = open(args.css, encoding="utf8").read()
    probe += '\n;document.addEventListener("DOMContentLoaded", () => { const s = document.createElement("style"); s.textContent = %s; document.head.appendChild(s); });' % json.dumps(css)

ctx = GLib.MainContext.default()
# 宿主的停留判定（`tab-stream-view.ts::STAY_MS`）：切进来停住这么久才接骨架 ⇒ 「停住之后最长一帧」从这里起算
STAY_MS = 150


def pump(ms):
    until = time.monotonic() + ms / 1000
    while time.monotonic() < until:
        ctx.iteration(False)
        time.sleep(0.001)


def wait(pred, ms, what):
    until = time.monotonic() + ms / 1000
    while not pred():
        if time.monotonic() > until:
            raise RuntimeError(f"等了 {ms} ms 还没等到：{what}")
        ctx.iteration(False)
        time.sleep(0.002)


class View:
    def __init__(self):
        self.win = Gtk.Window()
        self.win.set_default_size(1280, 800)
        ucm = WebKit2.UserContentManager()
        ucm.add_script(WebKit2.UserScript(probe, WebKit2.UserContentInjectedFrames.TOP_FRAME, WebKit2.UserScriptInjectionTime.START, None, None))
        self.wv = WebKit2.WebView.new_with_user_content_manager(ucm)
        st = self.wv.get_settings()
        st.set_enable_developer_extras(False)
        st.set_enable_write_console_messages_to_stdout(False)
        self.win.add(self.wv)
        self.win.show_all()

    def js(self, body, ms=120_000):
        """页里跑一段函数体（可以 `await`），交回 JSON 化的返回值。"""
        box = {}

        def done(wv, res):
            try:
                v = wv.call_async_javascript_function_finish(res)
                box["v"] = json.loads(v.to_json(0) or "null")
            except Exception as e:  # noqa: BLE001
                box["e"] = str(e)

        self.wv.call_async_javascript_function(body, -1, None, None, None, None, done)
        wait(lambda: box, ms, body[:80])
        if "e" in box:
            raise RuntimeError(box["e"])
        return box["v"]

    def load(self, url):
        box = {}
        h = self.wv.connect("load-changed", lambda _w, ev: box.setdefault("ok", True) if ev == WebKit2.LoadEvent.FINISHED else None)
        self.wv.load_uri(url)
        wait(lambda: box, 60_000, "load")
        self.wv.disconnect(h)

    def close(self):
        self.win.destroy()
        pump(200)


_seen = {}


def cpu_ms(root=None):
    """这一棵进程树（本进程 ＝ WebKit 的 UI 进程，连同它起的网页 / 网络 / GPU 进程）到此刻一共用了多少 CPU 毫秒（各线程 schedstat 第一格相加）。
    按线程记最后一次读数、只往上加：中途退掉的进程 / 线程留着它最后的读数（不然一退出，合计倒着走）。"""
    root = root or os.getpid()
    kids = {}
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        try:
            st = open(f"/proc/{d}/stat").read()
            ppid = int(st[st.rindex(")") + 2:].split()[1])
            kids.setdefault(ppid, []).append(int(d))
        except OSError:
            pass
    stack = [root]
    while stack:
        pid = stack.pop()
        stack.extend(kids.get(pid, []))
        try:
            for t in os.listdir(f"/proc/{pid}/task"):
                try:
                    v = int(open(f"/proc/{pid}/task/{t}/schedstat").read().split()[0])
                    key = (pid, int(t))
                    _seen[key] = max(_seen.get(key, 0), v)
                except OSError:
                    pass
        except OSError:
            pass
    return sum(_seen.values()) / 1e6


def rss_mb(root=None):
    """这一棵进程树此刻的常驻内存合计（MB，各进程 VmRSS 相加；共享页会重复算，只拿来比前后）。"""
    root = root or os.getpid()
    kids = {}
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        try:
            st = open(f"/proc/{d}/stat").read()
            kids.setdefault(int(st[st.rindex(")") + 2:].split()[1]), []).append(int(d))
        except OSError:
            pass
    kb = 0
    stack = [root]
    while stack:
        pid = stack.pop()
        stack.extend(kids.get(pid, []))
        try:
            for line in open(f"/proc/{pid}/status"):
                if line.startswith("VmRSS:"):
                    kb += int(line.split()[1])
        except OSError:
            pass
    return kb / 1024


def pct(xs, q, floor=0):
    if not xs:
        return floor
    s = sorted(xs)
    return max(floor, s[min(len(s) - 1, int(q * len(s)))])


TAB_AT = """
const t = document.querySelectorAll('#tab-bar .tab')[%d];
if (!t) return null;
t.scrollIntoView({ block: 'nearest' });
const m = /t(\\d+)-perf/.exec(t.textContent || '');
return { turns: m ? Number(m[1]) : null, active: t.classList.contains('active') };
"""

CLICK = """
const t = document.querySelectorAll('#tab-bar .tab')[%d];
const b = t.getBoundingClientRect();
const at = { bubbles: true, cancelable: true, clientX: b.left + b.width / 2, clientY: b.top + b.height / 2, button: 0 };
t.dispatchEvent(new PointerEvent('pointerdown', at));
t.dispatchEvent(new MouseEvent('mousedown', at));
t.dispatchEvent(new PointerEvent('pointerup', at));
t.dispatchEvent(new MouseEvent('mouseup', at));
t.dispatchEvent(new MouseEvent('click', at));
return true;
"""


def boot(v, url, result):
    b0 = time.monotonic()
    v.load(url)
    v.js("for (;;) { if (window.__shots && window.__shots.state !== 'booting') return window.__shots.state; await new Promise((r) => setTimeout(r, 100)); }")
    st = v.js("return window.__shots.state")
    if st != "done":
        raise RuntimeError(v.js("return window.__shots.error"))
    v.js("return await __perf.quiet(1000, 60000)")
    c0 = cpu_ms()
    pump(1500)
    idle = cpu_ms() - c0
    if args.profile:
        v.js(open(args.profile, encoding="utf8").read())
    boot_frames = v.js("return __perf.bootFrames.slice(1)")
    result["boot"].append({"ms": (time.monotonic() - b0) * 1000, "idleCpu": idle, "rss": rss_mb(), "bootJankN": len([d for d in boot_frames if d > 50]), "bootJankMs": sum(d - 16.7 for d in boot_frames if d > 50), "bootFrameMax": max(boot_frames or [0]), "nodes": v.js("return document.getElementsByTagName('*').length")})


def measured_click(v, i, watch_ms=1500):
    t = v.js(TAB_AT % i)
    v.js("return await __perf.quiet(300, 5000)")
    since = v.js("__perf.frameStart(); return performance.now()")
    c0 = cpu_ms()
    v.js(CLICK % i)
    pump(watch_ms)
    cpu = cpu_ms() - c0
    w = v.js("return __perf.since(%f)" % since)
    c = w["clicks"][0] if w["clicks"] else {}
    # 停住之后最长一帧：点下去 STAY_MS（宿主的停留判定）之后、且切换那一帧画完（第二个 rAF）之后结束的帧里最长的那一帧
    # （机器忙时切换那一帧本身就能拖过 150 ms —— 不能把它算成停住那一帧）
    stay = v.js("return __perf.frameAfter(%f, %f)" % (c.get("t0", since), max(STAY_MS, c.get("frame2") or 0)))
    attach = v.js("return __perf.attachFrame(%f)" % c.get("t0", since))
    frames = v.js("return __perf.frameStop()")
    over = [d for d in frames if d > 50]
    return {
        "tab": i,
        "turns": t["turns"],
        "cpu": cpu,
        "sync": c.get("sync"),
        "frame2": c.get("frame2"),
        "jankN": len(over),
        "jankMs": sum(d - 16.7 for d in over),
        "frameMax": max(frames) if frames else 0,
        "stayFrame": stay,
        "attachFrame": attach,
        "nodesOn": sum(x["on"] for x in w["mut"]),
        "nodesOff": sum(x["off"] for x in w["mut"]),
    }


def bench_switch(v, url, run, result):
    boot(v, url, result)
    n = v.js("return document.querySelectorAll('#tab-bar .tab').length")
    rows = []
    for ps in ("cold", "warm"):
        if args.profile and ps == "warm" and args.profile_pass == "cold":
            with open(os.path.join(args.out, "prof.json"), "w") as f:
                json.dump(v.js("return window.__prof ? window.__prof.dump() : null"), f, indent=1, ensure_ascii=False)
        if args.profile and ps == args.profile_pass:
            v.js("window.__prof && window.__prof.reset(); return 0")
        for i in range(n):
            if v.js(TAB_AT % i)["active"]:
                continue
            # `--profile`：冷切进长会话那几下各存一份方法计时（含调用顺序 —— 停住之后那一帧里谁先谁后）
            deep = args.profile and ps == "cold" and (v.js(TAB_AT % i)["turns"] or 0) >= 200
            if deep:
                v.js("window.__prof && window.__prof.reset(); return 0")
            rows.append({"run": run, "pass": ps, **measured_click(v, i)})
            if deep:
                with open(os.path.join(args.out, f"prof-cold-long-{i}.json"), "w") as f:
                    json.dump(v.js("return window.__prof ? { top: window.__prof.dump(), seq: window.__prof.seq ? window.__prof.seq() : null } : null"), f, indent=1, ensure_ascii=False)
        if v.js(TAB_AT % 0)["active"]:
            rows.append({"run": run, "pass": "warm", **measured_click(v, 1)})
    if args.profile and args.profile_pass == "warm":
        with open(os.path.join(args.out, "prof.json"), "w") as f:
            json.dump(v.js("return window.__prof ? window.__prof.dump() : null"), f, indent=1, ensure_ascii=False)
    print(f"  WebKit 切一下 第 {run + 1} 趟：{len(rows)} 次", flush=True)
    return rows


SEQ = [0, 3, 7, 12, 1, 5, 0, 9, 3, 14, 7, 2, 12, 20, 0, 6, 3, 11, 7, 12]


def bench_rapid(v, url, run, result):
    boot(v, url, result)
    n = v.js("return document.querySelectorAll('#tab-bar .tab').length")
    rows = []
    for burst in range(3):
        v.js("return await __perf.quiet(500, 8000)")
        since = v.js("__perf.frameStart(); return performance.now()")
        c0 = cpu_ms()
        for i in SEQ:
            t0 = time.monotonic()
            v.js(TAB_AT % (i % n))
            v.js(CLICK % (i % n))
            pump(max(0, 200 - (time.monotonic() - t0) * 1000))
        last = time.monotonic()
        settled = v.js("return await __perf.quiet(300, 15000)")
        pump(max(0, 2000 - settled))
        cpu = cpu_ms() - c0
        frames = v.js("return __perf.frameStop()")
        w = v.js("return __perf.since(%f)" % since)
        over = [d for d in frames if d > 50]
        syncs = [c["sync"] for c in w["clicks"] if c.get("sync") is not None]
        f2 = [c["frame2"] for c in w["clicks"] if c.get("frame2") is not None]
        rows.append({
            "run": run, "burst": burst, "cpu": cpu, "clicks": len(w["clicks"]),
            "syncP50": pct(syncs, 0.5), "syncMax": max(syncs or [0]),
            "frame2P50": pct(f2, 0.5), "frame2Max": max(f2 or [0]),
            "jankN": len(over), "jankMs": sum(d - 16.7 for d in over), "frameMax": max(frames or [0]),
            "nodesOff": sum(x["off"] for x in w["mut"]),
            "afterLast": settled,
        })
    print(f"  WebKit 连续快速切 第 {run + 1} 趟：3 串", flush=True)
    return rows


KEY = """
const init = { key: ']', code: 'BracketRight', bubbles: true, cancelable: true };
const t = document.activeElement || document.body;
t.dispatchEvent(new KeyboardEvent('keydown', init));
t.dispatchEvent(new KeyboardEvent('keyup', init));
return true;
"""


def bench_keys(v, url, run, result):
    """按住「下一个 tab」（缺省键 `]`）连切：30 下、每 40 ms 一下（键盘自动重复的量级），3 串。"""
    boot(v, url, result)
    if args.profile and args.profile_pass == "keys":
        v.js("window.__prof && window.__prof.reset(); return 0")
    rows = []
    for burst in range(3):
        v.js("return await __perf.quiet(500, 8000)")
        since = v.js("__perf.frameStart(); return performance.now()")
        c0 = cpu_ms()
        for _ in range(30):
            t0 = time.monotonic()
            v.js(KEY)
            pump(max(0, 40 - (time.monotonic() - t0) * 1000))
        settled = v.js("return await __perf.quiet(300, 15000)")
        cpu = cpu_ms() - c0
        frames = v.js("return __perf.frameStop()")
        w = v.js("return __perf.since(%f)" % since)
        over = [d for d in frames if d > 50]
        rows.append({"run": run, "burst": burst, "cpu": cpu, "jankN": len(over), "jankMs": sum(d - 16.7 for d in over), "frameMax": max(frames or [0]),
                     "nodesOff": sum(x["off"] for x in w["mut"]), "afterLast": settled})
    if args.profile and args.profile_pass == "keys":
        with open(os.path.join(args.out, "prof-keys.json"), "w") as f:
            json.dump(v.js("return window.__prof ? { top: window.__prof.dump(), hidden: window.__prof.hidden() } : null"), f, indent=1, ensure_ascii=False)
    print(f"  WebKit 按住切 第 {run + 1} 趟：3 串", flush=True)
    return rows


def bench_long(v, url, run, result):
    boot(v, url, result)
    n = v.js("return document.querySelectorAll('#tab-bar .tab').length")
    turns = [v.js(TAB_AT % i)["turns"] or 0 for i in range(n)]
    longest = turns.index(max(turns))
    first = measured_click(v, longest, 2500)
    v.js("return await __perf.quiet(500, 10000)")
    v.js("__perf.frameStart(); return 0")
    c0 = cpu_ms()
    for _ in range(60):
        v.js("document.querySelector('.stream.active').scrollBy(0, -600); return 0")
        pump(50)
    pump(1000)
    cpu = cpu_ms() - c0
    frames = v.js("return __perf.frameStop()")
    over = [d for d in frames if d > 50]
    print(f"  WebKit 长会话 第 {run + 1} 趟", flush=True)
    return {"run": run, "first": first, "cpu": cpu, "frameP50": pct(frames, 0.5), "frameP95": pct(frames, 0.95), "frameMax": max(frames or [0]),
            "jankN": len(over), "jankMs": sum(d - 16.7 for d in over), "scrollTop": v.js("return document.querySelector('.stream.active').scrollTop")}


SCLICK = """
const t = document.querySelector(%s);
if (!t) return false;
t.scrollIntoView({ block: 'nearest' });
const b = t.getBoundingClientRect();
const at = { bubbles: true, cancelable: true, clientX: b.left + b.width / 2, clientY: b.top + b.height / 2, button: 0 };
t.dispatchEvent(new PointerEvent('pointerdown', at));
t.dispatchEvent(new MouseEvent('mousedown', at));
t.dispatchEvent(new PointerEvent('pointerup', at));
t.dispatchEvent(new MouseEvent('mouseup', at));
t.dispatchEvent(new MouseEvent('click', at));
return true;
"""
NAV = '.settings-shell:not(.settings-shell-h) > .settings-nav .settings-nav-item[data-route-id="%s"]'
TAB = '.settings-page:not([hidden]) .settings-shell-h .settings-nav-item[data-route-id="%s"]'
LOCAL_PAGE = "machine:（本机）"


def s_click(v, sel, watch_ms=1500):
    v.js("return await __perf.quiet(300, 5000)")
    since = v.js("__perf.frameStart(); return performance.now()")
    c0 = cpu_ms()
    if not v.js(SCLICK % json.dumps(sel)):
        v.js("return __perf.frameStop()")
        return None
    pump(watch_ms)
    cpu = cpu_ms() - c0
    frames = v.js("return __perf.frameStop()")
    w = v.js("return __perf.since(%f)" % since)
    c = w["clicks"][0] if w["clicks"] else {}
    over = [d for d in frames if d > 50]
    return {"cpu": cpu, "sync": c.get("sync"), "frame2": c.get("frame2"), "jankN": len(over), "jankMs": sum(d - 16.7 for d in over), "frameMax": max(frames or [0]), "nodes": sum(x["on"] + x["off"] for x in w["mut"])}


def bench_settings(v, url, run, result):
    """设置窗：开窗 · 每一页点两遍（首次可见 / 回来）· 扩展页逐字筛选 · 关了再开 `RE_CYCLES` 轮的常驻内存。"""
    b0 = time.monotonic()
    v.load(url)
    v.js("for (;;) { if (window.__shots && window.__shots.state !== 'booting') return window.__shots.state; await new Promise((r) => setTimeout(r, 100)); }")
    if v.js("return window.__shots.state") != "done":
        raise RuntimeError(v.js("return window.__shots.error"))
    ready = (time.monotonic() - b0) * 1000
    v.js("return await __perf.quiet(1000, 60000)")
    boot_frames = v.js("return __perf.bootFrames.slice(1)")
    # 探针开窗那 15 s 自己起着一条 rAF 链 ⇒ 空闲 CPU 等它停了再量
    v.js("const dcl = performance.getEntriesByType('navigation')[0]?.domContentLoadedEventStart ?? 0; await new Promise((r) => setTimeout(r, Math.max(0, 15500 - (performance.now() - dcl)))); return 0")
    c0 = cpu_ms()
    pump(3000)
    opened = {"run": run, "ready": ready, "idleCpu": cpu_ms() - c0, "rss": rss_mb(), "nodes": v.js("return document.getElementsByTagName('*').length"),
              "bootJankN": len([d for d in boot_frames if d > 50]), "bootFrameMax": max(boot_frames or [0])}
    result["sopen"].append(opened)
    ids = v.js("return [...document.querySelectorAll('.settings-shell:not(.settings-shell-h) > .settings-nav .settings-nav-item')].map((e) => e.dataset.routeId)")
    for ps in ("first", "again"):
        for i in ids:
            m = s_click(v, NAV % i)
            if m:
                result["spages"].append({"run": run, "pass": ps, "kind": "machine" if i.startswith("machine:") else "top", "id": i, **m})
        s_click(v, NAV % LOCAL_PAGE)
        tabs = v.js("return [...document.querySelectorAll('.settings-page:not([hidden]) .settings-shell-h .settings-nav-item')].map((e) => e.dataset.routeId)")
        for i in tabs[1:] + tabs[:1]:
            m = s_click(v, TAB % i)
            if m:
                result["spages"].append({"run": run, "pass": ps, "kind": "tab", "id": i, **m})
    # 扩展页逐字筛选
    s_click(v, NAV % "ext", 2500)
    v.js("return await __perf.quiet(500, 8000)")
    since = v.js("__perf.frameStart(); return performance.now()")
    c0 = cpu_ms()
    for k in range(1, 8):
        t0 = time.monotonic()
        v.js("const s = document.querySelector('.settings-page:not([hidden]) .ext-search'); s.value = %s; s.dispatchEvent(new Event('input', { bubbles: true })); return 0" % json.dumps("tool-01"[:k]))
        pump(max(0, 80 - (time.monotonic() - t0) * 1000))
    v.js("const s = document.querySelector('.settings-page:not([hidden]) .ext-search'); s.value = ''; s.dispatchEvent(new Event('input', { bubbles: true })); return 0")
    v.js("return await __perf.quiet(300, 5000)")
    frames = v.js("return __perf.frameStop()")
    w = v.js("return __perf.since(%f)" % since)
    over = [d for d in frames if d > 50]
    result["sfilter"].append({"run": run, "cpu": cpu_ms() - c0, "jankN": len(over), "jankMs": sum(d - 16.7 for d in over), "frameMax": max(frames or [0]), "nodes": sum(x["on"] + x["off"] for x in w["mut"])})
    # 关了再开：Ctrl+W 藏窗 → 壳推一帧「拿到焦点」；WebKit 没有强制 GC，只看常驻内存与文档里的节点
    re = [{"cycle": 0, "rss": rss_mb(), "nodes": v.js("return document.getElementsByTagName('*').length")}]
    for c in range(1, RE_CYCLES + 1):
        v.js("document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', code: 'KeyW', ctrlKey: true, bubbles: true, cancelable: true })); return 0")
        pump(150)
        v.js("await window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event: 'tauri://focus', payload: true }); return 0")
        v.js("return await __perf.quiet(300, 8000)")
        for i in ("ext", LOCAL_PAGE, "machines"):
            s_click(v, NAV % i, 600)
        re.append({"cycle": c, "rss": rss_mb(), "nodes": v.js("return document.getElementsByTagName('*').length")})
    result["sreopen"].append({"run": run, "rows": re})
    print(f"  WebKit 设置窗 第 {run + 1} 趟", flush=True)


RE_CYCLES = 10


def summarize_settings(r):
    L = []
    if r.get("sopen"):
        L.append("## 设置窗 · 开窗")
        L.append("")
        L.append("| 趟 | 开好（ms） | 开窗 15 s 里 >50ms 帧 | 最长帧 | 空闲 3 s CPU | 常驻内存 MB（WebKit 全部进程） | 节点 |")
        L.append("|---|---|---|---|---|---|---|")
        for x in r["sopen"]:
            L.append(f"| {x['run'] + 1} | {x['ready']:.0f} | {x['bootJankN']} | {x['bootFrameMax']:.0f} | {x['idleCpu']:.0f} | {x['rss']:.0f} | {x['nodes']} |")
        L.append("")
    if r.get("spages"):
        L.append("## 设置窗 · 切页（每组 p50 / 最大，ms）")
        L.append("")
        L.append("| 页 | 遍 | 次 | CPU | 同步段 | 画出来（第二帧） | 卡帧个 | 最长帧 | 新建节点 |")
        L.append("|---|---|---|---|---|---|---|---|---|")
        groups = {}
        for x in r["spages"]:
            name = ("本机页" if x["id"] == LOCAL_PAGE else "其余机器页") if x["kind"] == "machine" else x["id"]
            groups.setdefault((name, x["pass"]), []).append(x)
        for (name, ps), xs in groups.items():
            def pm(k):
                vals = [x[k] or 0 for x in xs]
                return f"{pct(vals, 0.5):.0f} / {max(vals):.0f}"
            L.append(f"| {name} | {'首次' if ps == 'first' else '回来'} | {len(xs)} | {pm('cpu')} | {pm('sync')} | {pm('frame2')} | {pm('jankN')} | {pm('frameMax')} | {pm('nodes')} |")
        L.append("")
    if r.get("sfilter"):
        L.append("## 设置窗 · 扩展页逐字筛选（7 个字 ＋ 清空）")
        L.append("")
        L.append("| 趟 | CPU | 卡帧个 | 卡帧超出合计 | 最长帧 | 新建节点 |")
        L.append("|---|---|---|---|---|---|")
        for x in r["sfilter"]:
            L.append(f"| {x['run'] + 1} | {x['cpu']:.0f} | {x['jankN']} | {x['jankMs']:.0f} | {x['frameMax']:.0f} | {x['nodes']} |")
        L.append("")
    for x in r.get("sreopen", []):
        rows = x["rows"]
        L.append(f"## 设置窗 · 关了再开 第 {x['run'] + 1} 趟（没有强制 GC：常驻内存只看走势）")
        L.append("")
        L.append("轮 / 常驻内存 MB / 文档节点：" + " · ".join(f"{y['cycle']}:{y['rss']:.0f}/{y['nodes']}" for y in rows))
        L.append("")
    return "\n".join(L)


def snapshot(v, path):
    """整窗截一张（从这扇 GTK 窗口取像素；这台机器的 gi 没有 cairo 那一格转换，WebKit 自己的截图接口用不了）。"""
    from gi.repository import Gdk  # noqa: PLC0415

    pump(300)
    win = v.win.get_window()
    pb = Gdk.pixbuf_get_from_window(win, 0, 0, win.get_width(), win.get_height())
    pb.savev(path, "png", [], [])


def shots(v, url, result):
    boot(v, url, result)
    snapshot(v, os.path.join(args.out, "wk-boot.png"))
    n = v.js("return document.querySelectorAll('#tab-bar .tab').length")
    turns = [v.js(TAB_AT % i)["turns"] or 0 for i in range(n)]
    longest = turns.index(max(turns))
    v.js(CLICK % longest)
    v.js("return await __perf.quiet(500, 10000)")
    snapshot(v, os.path.join(args.out, "wk-long.png"))
    for _ in range(8):
        v.js("document.querySelector('.stream.active').scrollBy(0, -700); return 0")
        pump(120)
    v.js("return await __perf.quiet(500, 10000)")
    snapshot(v, os.path.join(args.out, "wk-long-up.png"))
    short = turns.index(min(turns))
    v.js(CLICK % short)
    v.js("return await __perf.quiet(500, 10000)")
    snapshot(v, os.path.join(args.out, "wk-short.png"))
    v.js(CLICK % longest)
    v.js("return await __perf.quiet(500, 10000)")
    snapshot(v, os.path.join(args.out, "wk-long-back.png"))
    print("截图：", args.out, flush=True)


def summarize(r):
    L = [f"# WebKitGTK 读数（{r['when']} · WebKitGTK {r['webkit']} · {r['runs']} 趟 · loadavg 开头 {'/'.join('%.1f' % x for x in r['load']['start'])} 结尾 {'/'.join('%.1f' % x for x in r['load']['end'])}）", ""]
    if r["boot"]:
        L.append(f"开页到安静：p50 {pct([b['ms'] for b in r['boot']], 0.5):.0f} ms · 安静时 1.5 s 的 CPU p50 {pct([b.get('idleCpu', 0) for b in r['boot']], 0.5):.0f} ms · DOM 节点 p50 {pct([b['nodes'] for b in r['boot']], 0.5)} · 常驻内存 p50 {pct([b.get('rss', 0) for b in r['boot']], 0.5):.0f} MB")
        L.append(f"开窗 15 s 里 >50ms 的帧 p50 {pct([b.get('bootJankN', 0) for b in r['boot']], 0.5):.0f} 个 · 超出合计 p50 {pct([b.get('bootJankMs', 0) for b in r['boot']], 0.5):.0f} ms · 最长一帧 p50 {pct([b.get('bootFrameMax', 0) for b in r['boot']], 0.5):.0f} ms")
        L.append("")
    if r["switch"]:
        L.append("## 切一下（p50 / p95，ms）")
        L.append("")
        L.append("| 组 | 次 | CPU（WebKit 全部进程） | 同步段 | 画出来（第二帧） | 卡帧个（>50ms） | 卡帧超出合计 | 最长帧 | 停住之后最长一帧 | 接骨架那一帧（次） | 新建节点 |")
        L.append("|---|---|---|---|---|---|---|---|---|---|---|")
        groups = [
            ("冷切（没建过卡）", [x for x in r["switch"] if x["pass"] == "cold"]),
            ("热切（建过）", [x for x in r["switch"] if x["pass"] == "warm"]),
            ("热切 · 长会话（≥200 轮）", [x for x in r["switch"] if x["pass"] == "warm" and (x["turns"] or 0) >= 200]),
            ("冷切 · 长会话（≥200 轮）", [x for x in r["switch"] if x["pass"] == "cold" and (x["turns"] or 0) >= 200]),
        ]
        for name, xs in groups:
            if not xs:
                continue

            def pp(k):
                vals = [x.get(k) or 0 for x in xs]
                return f"{pct(vals, 0.5):.0f} / {pct(vals, 0.95):.0f}"

            def hit(k):
                # 只在一部分下里有的读数（接骨架那一帧：只有接了骨架的那几下）
                vals = [x[k] for x in xs if x.get(k) is not None]
                return f"{pct(vals, 0.5):.0f} / {pct(vals, 0.95):.0f}（{len(vals)}）" if vals else "—"

            L.append(f"| {name} | {len(xs)} | {pp('cpu')} | {pp('sync')} | {pp('frame2')} | {pp('jankN')} | {pp('jankMs')} | {pp('frameMax')} | {pp('stayFrame')} | {hit('attachFrame')} | {pp('nodesOn')} |")
        L.append("")
    if r["rapid"]:
        xs = r["rapid"]
        L.append("## 连续快速切（20 下 / 每 200 ms；各串 p50 / 最大）")
        L.append("")
        L.append("| 串数 | CPU | 同步段 p50 | 同步段 最大 | 画出来 p50 | 画出来 最大 | 卡帧个 | 卡帧超出合计 | 最长帧 | 后台流新建节点 | 末下到安静 |")
        L.append("|---|---|---|---|---|---|---|---|---|---|---|")

        def pm(k):
            vals = [x[k] for x in xs]
            return f"{pct(vals, 0.5):.0f} / {max(vals):.0f}"

        L.append(f"| {len(xs)} | {pm('cpu')} | {pm('syncP50')} | {pm('syncMax')} | {pm('frame2P50')} | {pm('frame2Max')} | {pm('jankN')} | {pm('jankMs')} | {pm('frameMax')} | {pm('nodesOff')} | {pm('afterLast')} |")
        L.append("")
    if r.get("keys"):
        xs = r["keys"]
        L.append("## 按住「下一个 tab」（30 下 / 每 40 ms；各串 p50 / 最大）")
        L.append("")
        L.append("| 串数 | CPU | 卡帧个 | 卡帧超出合计 | 最长帧 | 后台流新建节点 | 末下到安静 |")
        L.append("|---|---|---|---|---|---|---|")

        def pk(k):
            vals = [x[k] for x in xs]
            return f"{pct(vals, 0.5):.0f} / {max(vals):.0f}"

        L.append(f"| {len(xs)} | {pk('cpu')} | {pk('jankN')} | {pk('jankMs')} | {pk('frameMax')} | {pk('nodesOff')} | {pk('afterLast')} |")
        L.append("")
    if r["long"]:
        L.append("## 长会话（最长那条，冷切进去 ＋ 往上滚 60 下）")
        L.append("")
        L.append("| 趟 | 首屏 CPU | 滚动 CPU | 首屏同步段 | 首屏画出来 | 帧 p50 | 帧 p95 | 最长帧 | 卡帧个 | 卡帧超出合计 |")
        L.append("|---|---|---|---|---|---|---|---|---|---|")
        for x in r["long"]:
            L.append(f"| {x['run'] + 1} | {x['first']['cpu']:.0f} | {x['cpu']:.0f} | {x['first']['sync'] or 0:.0f} | {x['first']['frame2'] or 0:.0f} | {x['frameP50']:.0f} | {x['frameP95']:.0f} | {x['frameMax']:.0f} | {x['jankN']} | {x['jankMs']:.0f} |")
        L.append("")
    return "\n".join(L)


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def main():
    sandbox = os.path.join(REPO, ".build/perf-sandbox")
    os.makedirs(os.path.join(sandbox, "home"), exist_ok=True)
    # 测试不碰用户的桌面会话：会话总线不带（同截图工具 `run.mjs::isolatedEnv`）
    env = {k: v for k, v in os.environ.items() if not k.startswith(("CCM_", "CLAUDE_", "ANTHROPIC_", "TMUX")) and k != "DBUS_SESSION_BUS_ADDRESS"}
    env["HOME"] = os.path.join(sandbox, "home")
    procs = []
    port = args.port
    try:
        if port is None:
            port = free_port()
            srv = subprocess.Popen(["node", os.path.join(HERE, "serve.mjs"), str(port), *(["--dev"] if args.dev else [])], cwd=REPO, env={**env, "CCM_SHOTS_SANDBOX": sandbox}, stdout=subprocess.PIPE, text=True)
            procs.append(srv)
            for line in srv.stdout:
                if f"READY {port}" in line:
                    break
        url = f"http://127.0.0.1:{port}/index.html?scene=perf-tabs"
        surl = f"http://127.0.0.1:{port}/settings.html?scene=perf-settings"
        load0 = os.getloadavg()
        result = {"when": time.strftime("%Y-%m-%dT%H:%M:%S"), "webkit": f"{WebKit2.get_major_version()}.{WebKit2.get_minor_version()}.{WebKit2.get_micro_version()}", "runs": args.runs, "load": {"start": load0}, "boot": [], "switch": [], "rapid": [], "keys": [], "long": [], "sopen": [], "spages": [], "sfilter": [], "sreopen": []}
        if args.shot:
            v = View()
            try:
                shots(v, url, result)
            finally:
                v.close()
        for run in range(args.runs):
            for name, fn in (("switch", bench_switch), ("rapid", bench_rapid), ("keys", bench_keys), ("long", bench_long), ("boot", lambda v, url, run, result: boot(v, url, result) or [])):
                if name not in only:
                    continue
                v = View()
                try:
                    got = fn(v, url, run, result)
                    if isinstance(got, list):
                        result[name].extend(got)
                    else:
                        result[name].append(got)
                finally:
                    v.close()
            if "settings" in only:
                v = View()
                try:
                    bench_settings(v, surl, run, result)
                finally:
                    v.close()
        result["load"]["end"] = os.getloadavg()
        with open(os.path.join(args.out, "perf-webkit.json"), "w") as f:
            json.dump(result, f, indent=1)
        table = summarize(result) + "\n" + summarize_settings(result)
        with open(os.path.join(args.out, "perf-webkit.md"), "w") as f:
            f.write(table)
        print(table)
    finally:
        for p in procs:
            p.terminate()


def merge():
    parts = [json.load(open(os.path.join(d, "perf-webkit.json"))) for d in args.merge.split(",")]
    m = {**parts[0], "runs": sum(p["runs"] for p in parts), "load": {"start": parts[0]["load"]["start"], "end": parts[-1]["load"]["end"]}}
    for key in ("boot", "switch", "rapid", "keys", "long"):
        m[key] = [{**x, "part": k} for k, p in enumerate(parts) for x in p.get(key, [])]
    with open(os.path.join(args.out, "perf-webkit.json"), "w") as f:
        json.dump(m, f, indent=1)
    t = summarize(m)
    with open(os.path.join(args.out, "perf-webkit.md"), "w") as f:
        f.write(t)
    print(t)


if __name__ == "__main__":
    if args.merge:
        merge()
        sys.exit(0)
    if os.environ.get("DISPLAY_READY") != "1":
        # 自己在一台私有 Xvfb 里重跑自己
        xv = subprocess.Popen(["bash", os.path.join(REPO, "tests/scripts/xvfb-free.sh")], stdout=subprocess.PIPE, text=True)
        n = xv.stdout.readline().strip()
        try:
            env = {**os.environ, "DISPLAY": f":{n}", "DISPLAY_READY": "1"}
            env.pop("WAYLAND_DISPLAY", None)
            env.pop("DBUS_SESSION_BUS_ADDRESS", None)  # WebKit 窗口也不碰用户的桌面会话（钥匙环 · 无障碍总线）
            sys.exit(subprocess.call([sys.executable, *sys.argv], env=env))
        finally:
            xv.terminate()
            subprocess.call(["bash", os.path.join(REPO, "tests/scripts/xvfb-free.sh"), "release", n, str(xv.pid)])
    main()
