#!/usr/bin/env python3
"""
性能台架 perfA 的 WebKitGTK 那一组：同一个世界、同一套页内动作（`actions.ts` 的 `__pa`）在 Linux 版的窗口内核里再量一遍。

    python3 tests/shots/perf/webkit-a.py [--runs 2] [--out <目录>] [--only idle,scroll-up,…] [--css <文件>] [--port <已起好的伺服口>]

私有 Xvfb、伺服、隔离同 `webkit.py`。WebKit 没有长任务：读数里长任务一列为 0，看卡帧与画完。
读数表与 `bench-a.mjs` 同一张（汇总由它出：`node bench-a.mjs --merge <本目录> --out <本目录>`）。
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
LONGEST = "5e550100-0000-4000-8000-000000000100"
MAIN = ["idle", "away-idle", "scroll-up", "scroll-down", "stream-active", "stream-background", "expand", "find", "outline", "needs", "palette", "acct", "new-session", "menu", "drawer", "agents", "jump", "idle"]
VIEWER = ["viewer-idle", "viewer-away-idle", "viewer-scroll", "viewer-find"]

ap = argparse.ArgumentParser()
ap.add_argument("--runs", type=int, default=2)
ap.add_argument("--out", default=os.path.join(REPO, ".build/perf-a-webkit"))
ap.add_argument("--only", default=None)
ap.add_argument("--css", default=None)
ap.add_argument("--port", type=int, default=None)
args = ap.parse_args()
only = set(args.only.split(",")) if args.only else None
os.makedirs(args.out, exist_ok=True)

probe = open(os.path.join(HERE, "probe.js"), encoding="utf8").read()
if args.css:
    css = open(args.css, encoding="utf8").read()
    probe += '\n;document.addEventListener("DOMContentLoaded", () => { const s = document.createElement("style"); s.textContent = %s; document.head.appendChild(s); });' % json.dumps(css)

ctx = GLib.MainContext.default()


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

    def js(self, body, ms=300_000):
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
        wait(lambda: box, 120_000, "load")
        self.wv.disconnect(h)

    def close(self):
        self.win.destroy()
        pump(200)


_seen = {}


def cpu_ms(root=None):
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
    stack = [root]
    while stack:
        pid = stack.pop()
        stack.extend(kids.get(pid, []))
        try:
            for t in os.listdir(f"/proc/{pid}/task"):
                try:
                    v = int(open(f"/proc/{pid}/task/{t}/schedstat").read().split()[0])
                    _seen[(pid, int(t))] = max(_seen.get((pid, int(t)), 0), v)
                except OSError:
                    pass
        except OSError:
            pass
    return sum(_seen.values()) / 1e6


def open_page(v, url):
    t0 = time.monotonic()
    v.load(url)
    v.js("for (;;) { if (window.__shots && window.__shots.state !== 'booting') return window.__shots.state; await new Promise((r) => setTimeout(r, 100)); }")
    if v.js("return window.__shots.state") != "done":
        raise RuntimeError(v.js("return window.__shots.error"))
    v.js("return await __perf.quiet(1000, 60000)")
    return (time.monotonic() - t0) * 1000


def run_all(v, names, run, where, rows):
    for name in names:
        if only and name not in only:
            continue
        c0 = cpu_ms()
        try:
            r = v.js("return await __pa.run(%s)" % json.dumps(name))
        except Exception as e:  # noqa: BLE001
            r = {"name": name, "error": str(e)[:300]}
        cpu = cpu_ms() - c0
        rows.append({"run": run, "where": where, **r, "cpu": cpu})
        print(f"  {where} {name}：CPU {cpu:.0f} · 画完 p95 {r.get('paintP95') or 0:.0f} / 最长 {r.get('paintMax') or 0:.0f} · 卡帧 {r.get('jankN', '-')} / 超出 {r.get('jankMs') or 0:.0f}{' · 错：' + r['error'] if 'error' in r else ''}", flush=True)


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def main():
    sandbox = os.path.join(REPO, ".build/perf-a-sandbox")
    os.makedirs(os.path.join(sandbox, "home"), exist_ok=True)
    env = {k: v for k, v in os.environ.items() if not k.startswith(("CCM_", "CLAUDE_", "ANTHROPIC_", "TMUX")) and k != "DBUS_SESSION_BUS_ADDRESS"}
    env["HOME"] = os.path.join(sandbox, "home")
    procs = []
    port = args.port
    try:
        if port is None:
            port = free_port()
            srv = subprocess.Popen(["node", os.path.join(HERE, "serve.mjs"), str(port)], cwd=REPO, env={**env, "CCM_SHOTS_SANDBOX": sandbox}, stdout=subprocess.PIPE, text=True)
            procs.append(srv)
            for line in srv.stdout:
                if f"READY {port}" in line:
                    break
        base = f"http://127.0.0.1:{port}"
        result = {"when": time.strftime("%Y-%m-%dT%H:%M:%S"), "engine": f"webkitgtk {WebKit2.get_major_version()}.{WebKit2.get_minor_version()}.{WebKit2.get_micro_version()}", "runs": args.runs, "load": {"start": list(os.getloadavg())}, "rows": []}
        for run in range(args.runs):
            # 不在清单里的试验项（`overlay-probe` 之类）在主窗口跑；`viewer-` 打头的在查看窗 / agent 窗口跑
            extra = [n for n in (only or []) if n not in MAIN and n not in VIEWER and not n.startswith("viewer-")]
            vextra = [n for n in (only or []) if n not in VIEWER and n.startswith("viewer-")]
            pages = [("main", f"{base}/index.html?scene=perf-main", MAIN + extra), ("viewer", f"{base}/viewer.html?scene=perf-viewer&viewer={LONGEST}", VIEWER + vextra), ("agent", f"{base}/viewer.html?scene=perf-agent&viewer={LONGEST}&run=agent-p0", VIEWER + vextra)]
            for where, url, names in pages:
                if only and not any(n in only for n in names):
                    continue
                v = View()
                try:
                    ms = open_page(v, url)
                    print(f"{where} 开好 {ms:.0f} ms", flush=True)
                    run_all(v, names, run, where, result["rows"])
                except Exception as e:  # noqa: BLE001
                    result["rows"].append({"run": run, "where": where, "name": "open", "error": str(e)[:300]})
                    print(f"{where} 没开起来：{e}", flush=True)
                finally:
                    v.close()
        result["load"]["end"] = list(os.getloadavg())
        with open(os.path.join(args.out, "perf-a.json"), "w") as f:
            json.dump(result, f, indent=1, ensure_ascii=False)
    finally:
        for p in procs:
            p.terminate()


if __name__ == "__main__":
    if os.environ.get("DISPLAY_READY") != "1":
        xv = subprocess.Popen(["bash", os.path.join(REPO, "tests/scripts/xvfb-free.sh")], stdout=subprocess.PIPE, text=True)
        n = xv.stdout.readline().strip()
        try:
            env = {**os.environ, "DISPLAY": f":{n}", "DISPLAY_READY": "1"}
            env.pop("WAYLAND_DISPLAY", None)
            env.pop("DBUS_SESSION_BUS_ADDRESS", None)
            sys.exit(subprocess.call([sys.executable, *sys.argv], env=env))
        finally:
            xv.terminate()
            subprocess.call(["bash", os.path.join(REPO, "tests/scripts/xvfb-free.sh"), "release", n, str(xv.pid)])
    main()
