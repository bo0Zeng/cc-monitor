#!/usr/bin/env python3
# 手机冒烟主路（模拟器 ＋ 沙箱后端，替身 agent）：从清空的 App 起，一步一步走（十步 ＋ 在聊天里答的 4d–4g），每步截屏、记时，末尾一张表。
# 前提：rig.sh up（沙箱 sshd ＋ 后端）· emu.sh up（模拟器）· debug 包已装；环境照 emu.sh env。入口是同目录 smoke.sh。
# 第 4 步后两小步（4b 模型 chip · 4c ［+］面板）走聊天屏输入那一圈。
# 每一步只看手机屏上与台架记录里看得到的东西（不读 App 内部）；判不出的写「看不出」，不当过。
import json
import os
import re
import subprocess
import sys
import time

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
import ui  # noqa: E402

# 屏上的字都照文案表取（App 与核心读的同一张表）：文案一改，冒烟跟着走，不靠抄一份字面。
_COPY = json.load(open(os.path.join(here, "..", "..", "src", "shared", "copy", "table.json"), encoding="utf-8"))["entries"]


def t(key, **args):
    s = _COPY[key]["zh"]
    for k, v in args.items():
        s = s.replace("{" + k + "}", str(v))
    return s


BOX = os.environ["CCM_SMOKE_BOX"]
OUT = os.environ.get("CCM_SMOKE_OUT", os.path.join(BOX, "..", "shots"))
PORT = os.environ["CCM_SMOKE_PORT"]
USER = subprocess.run(["id", "-un"], capture_output=True, text=True).stdout.strip()
PKG = "com.ccmonitor.mobile"
os.makedirs(OUT, exist_ok=True)
results = []
# 这一趟的记号：台架目录跨趟留着，旧会话里也有同样的句子 ⇒ 每句带上它，认会话只认这一趟的。
T = "s" + str(int(time.time()))[-5:]


def rig(*a, **k):
    return subprocess.run(["bash", os.path.join(here, "rig.sh"), *a], capture_output=True, text=True, **k)


def sh(cmd):
    return rig("ssh", cmd).stdout


def tmux_sessions():
    """台架 tmux 里此刻的会话名（服务端没起 ⇒ 空）。"""
    r = rig("tmux", "list-sessions", "-F", "#{session_name}")
    return set(r.stdout.split()) if r.returncode == 0 else set()


def tidy_tmux(run):
    """跑 run()，末尾（红了 · 中途退出也算）收掉这一趟在台架 tmux 里新建的会话：台架目录跨趟留着，
    不收的话一趟一趟攒下来，终端屏那一步的会话列表要翻好几屏、找不到这一趟那条。"""
    before = tmux_sessions()
    try:
        run()
    finally:
        for name in sorted(tmux_sessions() - before):
            rig("tmux", "kill-session", "-t", f"={name}")


def adb(*a):
    return ui.adb(*a).stdout.decode("utf-8", "replace")


def tap(text, nth=0, must=True):
    """点 [text]：要点的（must）等它出现并停稳再点（忙时界面晚几秒才画出来、或还在滑开）；可有可无的只看此刻。"""
    n = ui.settled(text, nth) if must else ui.find(text, nth)
    if n is None:
        if must:
            raise Step.Fail(f"屏上找不到「{text}」")
        return False
    adb("shell", "input", "tap", *map(str, ui.center(n)))
    return True


def tap_until(text, shows, secs=5, tries=3, what=None):
    """点 [text] 直到 [shows]（字，或一个判断）成立：忙时一下点在界面还没接手的那一刻会落空，没成、[text] 还在就再点，最多 [tries] 回。"""
    seen = shows if callable(shows) else (lambda: ui.find(shows) is not None)
    for _ in range(tries):
        tap(text)
        end = ui.within(secs)
        while time.time() < end:
            if seen():
                return
            time.sleep(0.5)
        if ui.find(text) is None:
            break
    raise Step.Fail(f"点了「{text}」，{what or '「' + shows + '」'}没出来")


# 文件屏一问（提交上传 · 列目录）的期限（秒，产品里的数）。
FILES_ASK_S = 20


def busy():
    """屏上有一条忙线（进度条）在：文件屏在等那台回话。"""
    return any(n.get("class") == "android.widget.ProgressBar" for n in ui.nodes())


def wait(text, secs=30):
    end = ui.within(secs)
    while time.time() < end:
        if ui.find(text) is not None:
            return True
        time.sleep(1)
    return False


def typed(s):
    adb("shell", "input", "text", s.replace(" ", "%s"))


def hide_keyboard():
    if "mInputShown=true" in adb("shell", "dumpsys", "input_method"):
        adb("shell", "input", "keyevent", "4")
        time.sleep(0.5)


def shot(name):
    p = os.path.join(OUT, f"{name}.png")
    open(p, "wb").write(ui.adb("exec-out", "screencap", "-p").stdout)
    return p


def start_app():
    # `am start -W` 等到那一屏画出第一帧才回：机器忙时冷启动要几十秒。
    adb("shell", "am", "start", "-W", "-a", "android.intent.action.MAIN", "-c", "android.intent.category.LAUNCHER",
        "-n", f"{PKG}/.MainActivity")


def foreground():
    """App 在前台：系统报的那个 resumed 的 activity 是它的。"""
    return any(PKG in ln for ln in adb("shell", "dumpsys", "activity", "activities").splitlines() if "ResumedActivity" in ln)


def launch(fresh=False):
    start_app()
    if fresh:
        if not wait(t("mobDrawer.server.bare"), 20):
            raise Step.Fail("App 没起来")
        return
    to_chat()


def to_chat(secs=30):
    """回到带抽屉按钮的那一屏（对话列表 · 终端屏没有顶栏，按返回回去）。App 不在前台（返回按出去了 · 闪退）就拉起来；
    只在 App 画出了自己的字、连着两回一样（不在动画途中）又没有抽屉按钮时才按返回。"""
    end = ui.within(secs)
    last = None
    while time.time() < end:
        if not foreground():
            start_app()
            last = None
            continue
        hide_keyboard()
        ns = ui.nodes()
        if ui.find_in(ns, t("mobDrawer.open.aria")) is not None:
            return
        # 冷启动那几十秒屏上只有一层空窗（没画出第一帧、截屏全黑）：那时按返回就把 App 按出去了，等它画出自己的字再说。
        now = [ui.label(n) for n in ns if n.get("package") == PKG]
        if any(now) and now == last:
            adb("shell", "input", "keyevent", "4")
            now = None
        last = now
        time.sleep(1)
    raise Step.Fail("回不到聊天屏（找不到抽屉按钮）")


def to_bottom():
    for _ in range(12):
        adb("shell", "input", "swipe", "540", "2000", "540", "500", "120")


def drawer_open():
    """抽屉开着：抽屉底下那颗齿轮（只有抽屉里有）在屏上。"""
    return any(n.get("content-desc") == t("mobDrawer.settings.aria") for n in ui.nodes())


def drawer(item):
    to_chat()
    tap_until(t("mobDrawer.open.aria"), drawer_open, what="抽屉")
    # 抽屉盖在那一屏上：同一个字底下那一屏也有（对话列表的顶栏标题就叫「对话」）⇒ 点抽屉里那一处（画在最上面、节点排在最后）。
    if sum(1 for n in ui.nodes() if item == n.get("text")) > 1:
        tap_last(item)
    else:
        tap(item)
    end = ui.within(10)
    while drawer_open() and time.time() < end:
        time.sleep(0.5)


def reset():
    """红了之后回到已知的起点：网 · 屏上键盘放回来，App 停掉重开到带抽屉按钮那一屏（停在半截的抽屉 · 框 · 别的 App 都不留）。"""
    adb("shell", "cmd", "connectivity", "airplane-mode", "disable")
    adb("shell", "svc", "wifi", "enable")
    restore_ime()
    adb("shell", "am", "force-stop", PKG)
    launch()


class Step:
    class Fail(Exception):
        pass

    # 上一步红了、收拾过：下一步要从某一条对话接着走的，先回到那一条（[back_to]）。
    after_red = False

    def __init__(self, name):
        self.name = name

    def __enter__(self):
        self.t0 = time.time()
        self.anr0 = len(ui.anrs)
        self.recovered, Step.after_red = Step.after_red, False
        return self

    def back_to(self, row):
        """上一步红了 ⇒ 先从对话列表点进 [row] 那一条（这一步的起点）；没红就在原地接着走。"""
        if not self.recovered:
            return
        drawer(t("mobDrawer.item.chats"))
        if not wait(row, 30):
            raise Step.Fail(f"对话列表里没有「{row}」（这一步接着它走，前面那步没起来）")
        tap(row)
        if not wait(t("chat.input.placeholder"), 20):
            raise Step.Fail(f"点了「{row}」没进那一条")

    def __exit__(self, kind, err, tb):
        took = time.time() - self.t0
        p = shot(self.name)
        ok = kind is None
        anr = len(ui.anrs) - self.anr0
        res = ("过" if ok else f"没过：{err}") + (f"（期间应用无响应 {anr} 回）" if anr else "")
        results.append((self.name, res, f"{took:.1f}s", f"{ui.patience():.1f}", p))
        print(f"{'ok ' if ok else 'RED'} {self.name}  {took:.1f}s  ×{ui.patience():.1f}  {'' if ok else err}  {p}", flush=True)
        if not ok:
            # 红的那一刻屏上有什么（控件树 ＋ 前台是谁），分得清是没画出来还是画错了；再回到起点，后面那几步不跟着连红。
            focus = [ln.strip() for ln in adb("shell", "dumpsys", "window").splitlines() if "mCurrentFocus" in ln]
            open(p[:-4] + ".xml", "w").write(adb("exec-out", "uiautomator", "dump", "/dev/tty") + "\n" + "\n".join(focus) + "\n")
            try:
                reset()
            except Step.Fail as e:
                print(f"    收拾不回起点：{e}", flush=True)
            Step.after_red = True
        return kind is Step.Fail


def tmux_name_of(text):
    """记录里有这一句的那条会话在台架 tmux 里叫什么。"""
    for line in rig("tmux", "list-panes", "-a", "-F", "#{session_name} #{@ccm_sid}").stdout.splitlines():
        name, _, sid = line.partition(" ")
        if sid and text in sh(f"cat ~/.claude/projects/*/{sid}.jsonl 2>/dev/null; true"):
            return name
    return None


_alias = []


def host_alias():
    """模拟器里够到宿主机的那个地址 ＝ 模拟器的网关（问模拟器自己，不写死；问到一次就记着：飞行模式下问不出来）。"""
    if _alias:
        return _alias[0]
    route = adb("shell", "ip", "route", "get", "8.8.8.8").split()
    if "via" not in route:
        raise Step.Fail(f"问不出模拟器的网关：{' '.join(route)}")
    _alias.append(route[route.index("via") + 1])
    return _alias[0]


def fab_under_notice(fab_text, notice_text):
    """同一帧里：悬浮按钮（含 [fab_text] 的那个可点的块）与提示条（含 [notice_text] 的那一条）的矩形有没有交叠。"""
    ns = ui.nodes()

    def first(text):
        return next((n for n in ns if text in (n.get("text"), n.get("content-desc"))), None)

    def area(b):
        return (b[2] - b[0]) * (b[3] - b[1])

    def inside(outer, inner):
        return outer[0] <= inner[0] and outer[1] <= inner[1] and outer[2] >= inner[2] and outer[3] >= inner[3]

    f, c = first(fab_text), first(notice_text)
    if c is None:
        raise Step.Fail(f"这一帧里没有提示「{notice_text}」")
    if f is None:
        return True  # 按钮整个被盖住时，无障碍树里连它的字都不出

    fb, cb = ui.bounds(f), ui.bounds(c)
    fab = min((ui.bounds(n) for n in ns if n.get("clickable") == "true" and inside(ui.bounds(n), fb)), key=area, default=fb)
    # 提示条：装得下那颗按钮字、又比它宽出一大截的最小那一块（按钮本身不算）。
    bars = [ui.bounds(n) for n in ns if inside(ui.bounds(n), cb) and ui.bounds(n)[2] - ui.bounds(n)[0] > 3 * (cb[2] - cb[0])]
    bar = min(bars, key=area, default=cb)
    return not (fab[2] <= bar[0] or bar[2] <= fab[0] or fab[3] <= bar[1] or bar[3] <= fab[1])


def tap_in_row(label, button):
    """点 [label] 那一行上的 [button]（同一行：纵向范围重叠）。"""
    ns = ui.nodes()
    row = next((n for n in ns if n.get("text") == label), None)
    if row is None:
        raise Step.Fail(f"屏上找不到「{label}」那一行")
    y = ui.center(row)[1]
    for n in ns:
        if n.get("text") == button:
            x1, y1, x2, y2 = ui.bounds(n)
            if y1 - 40 <= y <= y2 + 40:
                adb("shell", "input", "tap", str((x1 + x2) // 2), str((y1 + y2) // 2))
                return
    raise Step.Fail(f"「{label}」那一行上没有「{button}」")


def open_import():
    """钥匙页点悬浮的「+」（按无障碍名找）开「添一把钥匙」那一张、点「搬进来」（提示条收起时「+」在往回落，那一下可能点空）。"""
    tap_until(t("mobIdentity.fab.aria"), t("mobIdentity.add.import"))
    tap_until(t("mobIdentity.add.import"), t("mobIdentity.import.pem"))



def type_key(key):
    """把整把私钥敲进「粘贴那段私钥」那一格，敲完读回框里的字逐字核：输入法偶尔把一行 base64 换成词（实测过），
    对不上就全选删掉重敲，最多三回。核的是框里真有的字，不是「最后一行在不在」。"""
    want = "\n".join(key)
    # 先把屏上键盘停掉：没有输入法时 `input text` 的按键直接进框，联想 · 纠错插不进来（停不掉也照敲，读回照核）。
    #   调用方先 [stop_ime]，点完「搬进来」再 [restore_ime]：中途放回来它会再弹出来，收它的那一下返回键可能把框一起关掉。
    _type_key(key, want)


_IME = ""


def stop_ime():
    """停掉屏上键盘（钥匙那一框用）：按键直接进框，也不再有键盘盖住框里的按钮。"""
    global _IME
    _IME = adb("shell", "settings", "get", "secure", "default_input_method").strip()
    if _IME and _IME != "null":
        adb("shell", "ime", "disable", _IME)


def restore_ime():
    """[type_key] 停掉的屏上键盘放回来。"""
    if _IME and _IME != "null":
        adb("shell", "ime", "enable", _IME)
        adb("shell", "ime", "set", _IME)


def _type_key(key, want):
    for _ in range(3):
        for line in key:
            typed(line)
            adb("shell", "input", "keyevent", "66")
        end = ui.within(20)
        got = ""
        while time.time() < end:
            got = next((n.get("text") for n in ui.nodes() if "END OPENSSH" in (n.get("text") or "")), "") or ""
            if got.strip() == want.strip():
                return
            time.sleep(1)
        adb("shell", "input", "keycombination", "113", "29")
        adb("shell", "input", "keyevent", "67")
        time.sleep(1)
    bad = next((f"第 {i + 1} 行 {g!r}" for i, (w, g) in enumerate(zip(want.split("\n"), got.split("\n"))) if w != g), "行数不一样")
    raise Step.Fail(f"钥匙敲不进框里（三回都与原文不一致：{bad[:160]}）")

def fill(field, value):
    """点 [field] 那一格、全选删掉、敲 [value]，读回屏上有它才算：忙时点下去那一格还没拿到焦点，敲的字会丢，没读回就再来一回。"""
    for _ in range(2):
        tap(field)
        adb("shell", "input", "keycombination", "113", "29")
        adb("shell", "input", "keyevent", "67")
        typed(value)
        if wait(value, 10):
            return
    raise Step.Fail(f"「{field}」那一格敲不进「{value}」")


def add_host(name):
    if ui.find(t("mobDrawer.open.aria")) is not None:
        drawer(t("mobDrawer.server.named", name=""))
    tap_until(t("mobHosts.fab.aria"), t("mobHostEdit.field.name"))
    fill(t("mobHostEdit.field.name"), name)
    fill(t("mobHostEdit.field.host"), host_alias())
    fill("22", PORT)
    fill(t("mobHostEdit.field.user"), USER)
    hide_keyboard()
    adb("shell", "input", "swipe", "540", "1600", "540", "400", "400")
    tap("smoke")
    tap(t("mobHostEdit.page.save"))
    end = ui.within(10)
    while ui.find(t("mobHostEdit.page.save")) is not None and time.time() < end:
        time.sleep(0.5)


def claude_models(cell="models"):
    """那台给 claude 的模型 · 权限档（session-new-facts 那一家的 `models` / `permissionModes`）。"""
    facts = json.loads(subprocess.run([sys.executable, os.path.join(here, "frames.py"), "session-new-facts", "{}"],
                                      capture_output=True, text=True).stdout or "{}")
    agents = (facts.get("data") or {}).get("agents") or []
    got = next((a[cell] for a in agents if a.get("kind") == "claude"), [])
    if not got:
        raise Step.Fail(f"那台没给 claude 的 {cell}：{facts!r}"[:200])
    return got


def pane_has(name, text, secs):
    """那个 tmux 会话的屏上在 [secs] 秒内出现 [text]。"""
    end = ui.within(secs)
    while True:
        if text in rig("tmux", "capture-pane", "-p", "-t", name or "").stdout:
            return True
        if time.time() >= end:
            return False
        time.sleep(1)


def switch_model(name):
    """终端屏的命令面板里换模型：列的是那台给的那一家的模型（session-new-facts），点一个 ⇒ session-model。
    替身读到 `/model <值>` 就落那一家的读回 ⇒ 结局「换成了」，那台写好的那一句上屏（Toast，经无障碍事件看）；
    回包解不开 · 没换成 ⇒ 提示条「发送失败」。"""
    facts = json.loads(subprocess.run([sys.executable, os.path.join(here, "frames.py"), "session-new-facts", "{}"],
                                      capture_output=True, text=True).stdout or "{}")
    agents = (facts.get("data") or {}).get("agents") or []
    models = next((a["models"] for a in agents if a.get("kind") == "claude"), [])
    if not models:
        raise Step.Fail(f"那台没给 claude 的模型：{facts!r}"[:200])
    pick = models[-1]
    hide_keyboard()
    tap(t("mobDefaultButtons.label.command"))
    time.sleep(1.5)
    row = ui.find(pick["text"])
    if row is None:
        raise Step.Fail(f"命令面板里没有「{pick['text']}」")
    said = t("beSessionModel.set.saved", model=pick["value"])
    # 无障碍事件流和 `uiautomator dump` 抢同一个口：看事件的这几秒里只点 · 只读台架，不 dump 屏。
    events = subprocess.Popen(["adb", "shell", "uiautomator", "events"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    try:
        time.sleep(1)
        adb("shell", "input", "tap", *map(str, ui.center(row)))
        end = ui.within(20)
        while time.time() < end and f"> /model {pick['value']}" not in rig("tmux", "capture-pane", "-p", "-t", name).stdout:
            time.sleep(1)
        got = time.time() < end
        time.sleep(4)
    finally:
        events.terminate()
    seen = events.communicate(timeout=5)[0]
    if not got:
        raise Step.Fail(f"点了「{pick['text']}」，那个会话里没收到 /model {pick['value']}")
    if t("mobTerm.model.sendFailed") in seen:
        raise Step.Fail("换模型那一问上屏的是「发送失败」（回包没解开或没换成）")
    if said not in seen:
        raise Step.Fail(f"没看到那台写好的那一句「{said}」")


def long_press(text):
    n = ui.find(text)
    if n is None:
        raise Step.Fail(f"屏上找不到「{text}」")
    x, y = ui.center(n)
    adb("shell", "input", "swipe", str(x), str(y), str(x), str(y), "900")
    time.sleep(1.5)


def tap_last(text):
    """同一个字有几处（对话框标题与确认键同字）时点最后一处：确认键在最下。"""
    end = ui.within(10)
    while not (hits := [n for n in ui.nodes() if text == n.get("text")]) and time.time() < end:
        time.sleep(0.5)
    if not hits:
        raise Step.Fail(f"屏上找不到「{text}」")
    adb("shell", "input", "tap", *map(str, ui.center(hits[-1])))


def files_page():
    """文件页那几步：列目录 · 进一层 · 预览 · 新建 · 改名 · 删除 · 上传 · 下载；读写都问那台后端（台架盘上看结果）。"""
    d = f"files{T}"
    # 上几趟留下的先清掉（台架家里的），这一趟那一份要在第一屏里。
    sh(f"rm -rf ~/work/files* && mkdir -p ~/work/{d} && printf 'hello {T}\\n' > ~/work/{d}/a.txt")
    drawer(t("mobDrawer.item.files"))
    if not wait("work", 30):
        raise Step.Fail("家目录没列出来")
    tap("work")
    if not wait(d, 15):
        raise Step.Fail("进不了 work")
    tap(d)
    if not wait("a.txt", 15):
        raise Step.Fail(f"进不了 {d}")
    long_press("a.txt")
    tap(t("rsFilewinWorkspace.label.preview"))
    if not wait(f"hello {T}", 15):
        raise Step.Fail("预览没出那一份的字")
    shot("11-预览")
    tap(t("mobKit.act.close")); time.sleep(1)
    mk = t("rsFilewinWriteops.label.mkdir")
    tap(mk); time.sleep(1)
    typed(f"new{T}"); hide_keyboard()
    tap_last(mk)
    if not wait(f"new{T}", 15) or "new" + T not in sh(f"ls ~/work/{d}"):
        raise Step.Fail("新建目录没落到那台")
    long_press(f"new{T}")
    rn = t("rsFilewinWriteops.label.rename")
    tap(rn); time.sleep(1)
    for _ in range(12):
        adb("shell", "input", "keyevent", "67")
    typed(f"ren{T}"); hide_keyboard()
    tap_last(rn)
    if not wait(f"ren{T}", 15) or "ren" + T not in sh(f"ls ~/work/{d}"):
        raise Step.Fail("改名没落到那台")
    long_press(f"ren{T}")
    de = t("rsFilewinWriteops.label.delete")
    tap(de); time.sleep(1)
    tap_last(de)
    time.sleep(3)
    if "ren" + T in sh(f"ls ~/work/{d}"):
        raise Step.Fail("删除没落到那台")
    # 下载：点文件 ⇒ 存进 App 自己的外部文件目录。
    tap("a.txt")
    if not wait(t("mobFiles.download.done", path="")[:3], 20):
        raise Step.Fail("下载没说做成")
    got = adb("shell", "cat", f"/sdcard/Android/data/{PKG}/files/a.txt")
    if f"hello {T}" not in got:
        raise Step.Fail(f"下载下来的不是那一份：{got[:80]!r}")
    # 上传：系统选文件那一屏里点台架推进去的那一份。
    up = f"up{T}.txt"
    open(os.path.join(OUT, up), "w").write(f"up {T}\n")
    adb("push", os.path.join(OUT, up), f"/sdcard/Download/{up}")
    adb("shell", "am", "broadcast", "-a", "android.intent.action.MEDIA_SCANNER_SCAN_FILE", "-d", f"file:///sdcard/Download/{up}")
    time.sleep(2)
    # 下载那条提示在的时候按钮让开（可能整个被盖住），等它收起再点。
    if not wait(t("rsFilewinUpload.label.upload"), 15):
        raise Step.Fail("找不到上传按钮")
    tap(t("rsFilewinUpload.label.upload"))
    if not wait(up, 15):
        tap("Downloads", must=False) or tap("Download", must=False)
        if not wait(up, 10):
            raise Step.Fail("选文件那一屏里找不到推进去的那一份")
    tap(up)
    end = ui.within(30)
    while time.time() < end and f"up {T}" not in sh(f"cat ~/work/{d}/{up} 2>/dev/null; true"):
        time.sleep(1)
    if time.time() >= end:
        raise Step.Fail("上传没落到那台")
    # 落到那台之后手机还要等提交那一问回话、再重列一次（各一问，期限各 20 秒、在产品里），两问都在路上时屏上那条忙线不收：
    # 等它收（＝回包到了、重列完了）再看列表，不按秒数猜机器多快。
    end = ui.within(2 * FILES_ASK_S)
    while time.time() < end and busy():
        time.sleep(1)
    if busy():
        raise Step.Fail("上传落到那台之后，提交 · 重列两问的期限都过了，忙线还在")
    if ui.find(up) is None:
        raise Step.Fail("上传完、重列也回来了，列表里没有它")


# 轮换屏那一步用的两份策略（合成的：照号牌顺序用；一格「权重容差」）。写进台架家里的策略目录（不是真家目录）。
SMOKE_STRATEGY = """// 照号牌顺序用（冒烟台架）。
/*--- rotation
{
  "format": 5,
  "tz": "UTC",
  "settings": { "accounts": [{ "id": "work" }, { "id": "team" }, { "id": "lab" }], "close": 25 },
  "form": [
    { "key": "close", "kind": "percent", "scope": "all", "min": 0, "max": 100, "default": 25,
      "label": { "zh": "权重容差" }, "help": { "zh": "冒烟台架的一格" } }
  ]
}
---*/
fn decide(ctx) {
  let order = [];
  let why = #{};
  for r in ctx.settings.accounts { order.push(r.id); why[r.id] = "照号牌顺序"; }
  #{ order: order, why: why, hold_until: (), wake: #{ at: (), when: [] }, memo: () }
}
"""


def frames(cmd, args, within=20000):
    out = subprocess.run([sys.executable, os.path.join(here, "frames.py"), cmd, json.dumps(args), str(within)],
                         capture_output=True, text=True).stdout
    return json.loads(out or "{}")


SMOKE_ALIAS = "smk"


def seed_alias():
    """台架那台的一条别名（起在 tmux 里、名字自动取），经别名页那条写命令建；已有就不再建。"""
    book = (frames("profiles-read", {}).get("data") or {})
    if any(p.get("name") == SMOKE_ALIAS for p in book.get("profiles") or []):
        return
    r = frames("profiles-write", {"changes": [{"op": "set", "was": None, "form": {"name": SMOKE_ALIAS, "tmux": {"mode": "auto"}}}],
                                  "fingerprint": book.get("fingerprint")})
    if not (r.get("data") or {}).get("wrote"):
        raise Step.Fail(f"台架建不了别名：{r!r}"[:300])


def seed_rotation():
    """台架里的账号库（work · team · lab）与两份策略；跨趟留着，再跑一遍不出错就行。"""
    frames("accounts-init", {"name": "work"})
    for n in ("team", "lab"):
        frames("accounts-add", {"name": n, "kind": "subscription"})
    import base64
    b = base64.b64encode(SMOKE_STRATEGY.encode()).decode()
    sh(f"mkdir -p ~/.cc-monitor/strategies && for n in smoke-a smoke-b; do echo {b} | base64 -d > ~/.cc-monitor/strategies/$n.rhai; done")


def lane_head(text):
    """时间轴里号名那一列的 [text]（同名的号牌在上面，取最靠下、靠左的那一个）。"""
    ns = [n for n in ui.nodes() if n.get("text") == text and ui.center(n)[0] < 300]
    return max(ns, key=lambda n: ui.center(n)[1], default=None)


ROOT = os.path.join(here, "..", "..")
MACHINE = "smokebox"


def build_id():
    """源码里这一版的身份（`src/backend/lib.rs` 的 `BUILD_ID`）。"""
    src = open(os.path.join(ROOT, "src", "backend", "lib.rs"), encoding="utf-8").read()
    return src.split('pub const BUILD_ID: &str = "', 1)[1].split('"', 1)[0]


def landing_id():
    """台架落点那一份字节自报的身份（扫戳，同远端那条 grep；没有 ⇒ None）。"""
    out = sh("LC_ALL=C grep -aoE '<<ccm-build-id:[A-Za-z0-9_.-]+:ccm-build-id>>' ~/.cc-monitor/bin/ccm 2>/dev/null | sort -u")
    ids = [ln.split(":", 1)[1].rsplit(":", 1)[0] for ln in out.split() if ln]
    return ids[0] if len(ids) == 1 else None


def older_backend():
    """一份「旧版」后端：本树编的那份，身份换成同长度的更早一格（`p10o-…` ⇒ `p10n-…`；只改字面，别的一字不动）。"""
    mine = build_id()
    m = re.fullmatch(r"p(\d+)([b-z])-(.+)", mine)
    if m is None:
        raise Step.Fail(f"这一版的身份 {mine!r} 拼不出同长度的更早一格")
    old = f"p{m[1]}{chr(ord(m[2]) - 1)}-{m[3]}"
    src = os.environ.get("CCM_SMOKE_BACKEND", os.path.join(ROOT, ".build", "backend", "debug", "cc-monitor-backend"))
    dest = os.path.join(BOX, "bin", "ccm-older")
    open(dest, "wb").write(open(src, "rb").read().replace(mine.encode(), old.encode()))
    os.chmod(dest, 0o755)
    return dest, old


def commands():
    """台架记下的每条进来的命令：`(时刻秒, 命令)`（时刻是本机钟，同 `time.time()` 那一天）。"""
    out = []
    for ln in open(os.path.join(BOX, "logs", "commands.log"), encoding="utf-8", errors="replace"):
        hms, _, cmd = ln.rstrip("\n").partition(" ")
        try:
            h, m, sec = hms.split(":")
            out.append((int(h) * 3600 + int(m) * 60 + float(sec), cmd))
        except ValueError:
            continue
    return out


def today_secs(t_wall):
    lt = time.localtime(t_wall)
    return lt.tm_hour * 3600 + lt.tm_min * 60 + lt.tm_sec + (t_wall % 1)


def wait_any(texts, secs):
    end = ui.within(secs)
    while time.time() < end:
        if any(ui.find(x) is not None for x in texts):
            return True
        time.sleep(1)
    return False


def deploy_and_time(button, name):
    """点［安装］/［更新］：等它装完、换常驻、接上。回 `(点下 → 问那台, 问那台 → 换常驻)` 两段秒数（台架命令记录的时刻）；
    点下之后连着截几张（过程那几句一闪而过，截下来挑着看）。"""
    before = sum(1 for _, c in commands() if c.endswith("--resident-ensure --replace"))
    tapped = time.time()
    tap(button)
    for i in range(10):
        shot(f"{name}-过程-{i}")
        time.sleep(0.3)
    end = ui.within(300)
    done_word = t("rsSftp.deploy.done", buildId="", machine="", path="", reason="").split(" ")[0]
    while time.time() < end:
        texts = [n.get("text") or "" for n in ui.nodes()]
        if any(x.startswith(done_word) for x in texts):
            break
        failed = [x for x in texts if "失败" in x or "未安装" in x]
        if failed:
            raise Step.Fail(f"没装上：{failed[0]}")
        time.sleep(0.5)
    else:
        raise Step.Fail("五分钟没等到「已安装后端」那一句")
    cmds = [(at, c) for at, c in commands() if at >= today_secs(tapped) - 1]
    replaced = [at for at, c in cmds if c.endswith("--resident-ensure --replace")]
    if sum(1 for _, c in commands() if c.endswith("--resident-ensure --replace")) <= before or not replaced:
        raise Step.Fail("装完没换常驻（台架没收到 --resident-ensure --replace）")
    if landing_id() != build_id():
        raise Step.Fail(f"落点那一份自报 {landing_id()!r}，不是这一版 {build_id()!r}")
    asked = [at for at, c in cmds if c == "uname -s -m"]
    first = (asked[0] - today_secs(tapped)) if asked else None
    trip = (replaced[0] - asked[0]) if asked else None
    size = os.path.getsize(os.path.join(ROOT, "src", "frontend", "shell", "embedded-backends", "cc-monitor-backend-x86_64"))
    print(f"   点下 → 问那台 {first:.1f} s（解包 ＋ 交给核心）· 问那台 → 换常驻 {trip:.1f} s（计划 ＋ 放 {size / 1e6:.1f} MB ＋ 读回）", flush=True)
    return first, trip


def deploy_steps():
    """装后端两步：那台没有后端 ⇒［安装］⇒ 接上；那台是旧版 ⇒［更新］⇒ 换常驻、接上的是这一版。"""
    with Step("1b-装后端"):
        if not wait(t("remoteHealth.head.notDeployed"), 30):
            raise Step.Fail("没装的那一台没说「未装」")
        deploy_and_time(t("machineState.fix.install"), "1b")
        if not wait_any([t("history.section.today"), t("history.list.empty")], 60):
            raise Step.Fail("装完没接上（对话列表没出来）")
    with Step("1c-升级后端"):
        old_bin, old_id = older_backend()
        rig("land", old_bin)
        if not wait(t("peerVersion.said.old", machine=MACHINE), 60):
            raise Step.Fail(f"那台是旧版 {old_id}，却没说要更新")
        shot("1c-要更新")
        deploy_and_time(t("machineState.fix.update"), "1c")
        end = ui.within(60)
        while time.time() < end and ui.find(t("peerVersion.said.old", machine=MACHINE)) is not None:
            time.sleep(1)
        if ui.find(t("peerVersion.said.old", machine=MACHINE)) is not None:
            raise Step.Fail("换了常驻还说要更新（接上的不是这一版）")
    if landing_id() != build_id():
        # 升级那一步没走完、那台还是旧版：放回这一版再往下走（后面几步不该跟着它红）。
        rig("land", os.environ.get("CCM_SMOKE_BACKEND", os.path.join(ROOT, ".build", "backend", "debug", "cc-monitor-backend")))
    with Step("1d-主机设置核对版本"):
        # 主机设置里那一行：通着、已是这一版 ⇒ 按钮是「核对版本」，按下去说「已是最新」、一个字节不写、不换常驻。
        before = sum(1 for _, c in commands() if c.endswith("--resident-ensure --replace"))
        drawer(t("mobDrawer.server.named", name=""))
        time.sleep(1.5)
        # 编辑在主机行尾的 ⋯ 里：先点 ⋯，再在弹出的那一列里点「编辑」。
        tap(t("mobHosts.row.more", label=MACHINE))
        time.sleep(1)
        tap(t("mobHosts.row.edit"))
        for _ in range(4):
            if ui.find(t("mobHostEdit.backend.check")) is not None:
                break
            adb("shell", "input", "swipe", "540", "1600", "540", "700", "400")
            time.sleep(1)
        tap(t("mobHostEdit.backend.check"))
        up_to_date = t("rsSftp.deploy.upToDate", buildId=build_id(), machine="", path="").split(" ")[0]
        end = ui.within(60)
        while time.time() < end and not any((n.get("text") or "").startswith(up_to_date) for n in ui.nodes()):
            time.sleep(1)
        if not any((n.get("text") or "").startswith(up_to_date) for n in ui.nodes()):
            raise Step.Fail("核对版本没说「已是最新」")
        if sum(1 for _, c in commands() if c.endswith("--resident-ensure --replace")) != before:
            raise Step.Fail("同版也换了常驻")
        shot("1d-主机设置")
        # 回那台的对话列表（后面几步从那里接着走）。
        adb("shell", "input", "keyevent", "4")
        time.sleep(1.5)
        drawer(t("mobDrawer.item.chats"))


def main():
    tidy_tmux(walk)


def walk():
    # 深浅：CCM_SMOKE_NIGHT=yes|no 时先把模拟器切到那一套（截图两套各走一趟用）；不设不动。
    if os.environ.get("CCM_SMOKE_NIGHT") in ("yes", "no"):
        adb("shell", "cmd", "uimode", "night", os.environ["CCM_SMOKE_NIGHT"])
    adb("shell", "pm", "clear", PKG)
    adb("shell", "pm", "grant", PKG, "android.permission.POST_NOTIFICATIONS")
    key = open(os.path.join(BOX, "sshd", "client_ed25519")).read().splitlines()

    only = os.environ.get("CCM_SMOKE_ONLY")
    if only == "deploy":
        # 只走装后端那两步（调样子 · 截图 · 量上传用）。
        with Step("1-加机器连上"):
            rig("land", "none")
            launch(fresh=True)
            tap(t("mobIdentity.page.title")); time.sleep(1)
            open_import()
            tap(t("mobIdentity.import.name")); typed("smoke")
            tap(t("mobIdentity.import.pem")); time.sleep(0.5)
            for line in key:
                typed(line)
                adb("shell", "input", "keyevent", "66")
            tap(t("mobIdentity.import.ok"))
            if not wait("SHA256:", 10):
                raise Step.Fail("整把钥匙没导进来")
            tap(t("mobKit.act.back")); time.sleep(1)
            add_host(MACHINE)
            drawer(t("mobDrawer.item.chats"))
            if not wait(t("machineState.fix.install"), 30):
                raise Step.Fail("那台没有后端，问题条上没给［安装］")
        deploy_steps()
        return report()
    if only == "files":
        # 只走文件页（台架里已有 smokebox、钥匙已导；调样子 · 截图用）。
        with Step("1-加机器连上"):
            launch(fresh=True)
            tap(t("mobIdentity.page.title")); time.sleep(1)
            open_import()
            tap(t("mobIdentity.import.name")); typed("smoke")
            tap(t("mobIdentity.import.pem")); time.sleep(0.5)
            for line in key:
                typed(line)
                adb("shell", "input", "keyevent", "66")
            hide_keyboard(); tap(t("mobIdentity.import.ok"))
            if not wait("SHA256:", 10):
                raise Step.Fail("整把钥匙没导进来")
            tap(t("mobKit.act.back")); time.sleep(1)
            add_host("smokebox")
            drawer(t("mobDrawer.item.chats"))
        with Step("11-文件页"):
            files_page()
        return report()

    with Step("1-加机器连上"):
        # 那台先没有后端：手机自己装（下一步）。
        rig("land", "none")
        launch(fresh=True)
        tap(t("mobIdentity.page.title")); time.sleep(1)
        stop_ime()
        open_import()
        tap(t("mobIdentity.import.name")); typed("smoke")
        tap(t("mobIdentity.import.pem")); time.sleep(0.5)
        typed(key[0])  # 只贴一行：要当场拒
        tap(t("mobIdentity.import.ok")); time.sleep(2)
        if ui.find("imported") is not None:
            raise Step.Fail("只贴进一行的钥匙照样存下了")
        # 拒的那一句是条长提示（带「复制详情」）：它在的时候「+」要让开、不被盖住，接着点得到。
        if not wait(t("detail.act.copy"), 5):
            raise Step.Fail("拒了却没出那条提示")
        if fab_under_notice(t("mobIdentity.fab.aria"), t("detail.act.copy")):
            raise Step.Fail("提示盖住了「+」（按钮没让开）")
        open_import()
        tap(t("mobIdentity.import.name")); typed("smoke")
        tap(t("mobIdentity.import.pem")); time.sleep(0.5)
        # `input text` 敲进去是异步的、输入法还可能改字：type_key 读回框里的字逐字核过才往下走。
        type_key(key)
        tap(t("mobIdentity.import.ok"))
        # 刚装上的包头一回存钥匙要先建本机加密库（Keystore），机器忙时十几秒。
        ok = wait("SHA256:", 40)
        restore_ime()
        if not ok:
            raise Step.Fail("整把钥匙没导进来")
        tap(t("mobKit.act.back")); time.sleep(1)
        add_host(MACHINE)
        drawer(t("mobDrawer.item.chats"))
        if not wait(t("machineState.fix.install"), 30):
            raise Step.Fail("那台没有后端，问题条上没给［安装］")

    deploy_steps()

    with Step("2-对话列表"):
        sh(f'tmux new -d -s seed{T} -c ~/work claude; sleep 3; tmux send-keys -t seed{T} "{T} seed" Enter; '
           f'tmux new -d -s needs{T} -c ~/work claude; sleep 3; tmux send-keys -t needs{T} "ask {T} needs" Enter')
        if not wait(f"{T} seed", 30):
            raise Step.Fail("历史清单没出新会话")
        if not wait(t("needs.bar.unknown"), 20):
            raise Step.Fail("需手动没出来")
        # 台架跨趟留着旧会话（上一趟中途停下的那条也还在等），它们按分钟走、几秒里不变：
        # 看的是「有没有一行在走表」，这一趟刚起的那条按秒走，一定在里面。
        def waits():
            return [n.get("text") for n in ui.nodes() if t("needs.bar.subBare") in (n.get("text") or "")]

        first = waits()
        time.sleep(4)
        later = waits()
        if not first or first == later:
            raise Step.Fail(f"已等没走表（{first!r} → {later!r}）")
        rig("tmux", "send-keys", "-t", f"needs{T}", "y", "Enter")

    with Step("3-新建会话"):
        # 新建会话打开就是填好的（目录 ＝ 那台最近用过的第一个）；点目录 chip → 浏览… → 进一层 → 选这个目录，起在那一层。
        sh(f"mkdir -p ~/work/{T}dir")
        drawer(t("mobDrawer.item.newChat"))
        if not wait("~/work", 20):
            raise Step.Fail("目录 chip 没填上那台最近的目录")
        tap_until("~/work", t("mobNewSession.dir.browse"), what="目录抽屉")
        tap(t("mobNewSession.dir.browse"))
        if not wait(f"{T}dir", 20):
            raise Step.Fail("浏览屏里没列出那一层的文件夹")
        tap(f"{T}dir")
        time.sleep(2)
        tap(t("mobNewSession.dir.pick"))
        if not wait(f"~/work/{T}dir", 10):
            raise Step.Fail("选了目录，chip 没换成它")
        fill(t("chat.input.placeholder"), f"{T} new session")
        hide_keyboard()
        tap(t("mobChat.input.send"))
        if not wait(f"替身收到：{T} new session", 40):
            raise Step.Fail("答不回来")
        if f"{T}dir" not in sh("ls ~/.claude/projects/"):
            raise Step.Fail("那条会话没起在选的目录里")

    with Step("4-聊天屏") as st:
        st.back_to(f"{T} new session")
        fill(t("chat.input.placeholder"), f"{T} long")
        hide_keyboard()
        tap(t("mobChat.input.send"))
        if not wait(t("cards.thinking.title"), 20):
            raise Step.Fail("过程没出来")
        time.sleep(4)
        for _ in range(12):
            adb("shell", "input", "swipe", "540", "700", "540", "2000", "150")
        if not wait(f"{T} new session", 5):
            raise Step.Fail("往上翻翻不到第一句")
        tap(t("cards.thinking.title"))
        if not wait("替身在想。", 5):
            raise Step.Fail("思考展不开")

    with Step("4b-换模型 chip") as st:
        st.back_to(f"{T} new session")
        # 聊天屏输入框下那颗模型 chip：抽屉里列那台给的模型，点一个 ⇒ session-model；读回 done 才换字（替身落的读回是那个值）。
        name = tmux_name_of(f"{T} new session")
        pick = claude_models()[-1]
        hide_keyboard()
        tap(t("session.model.title"))
        if not wait(pick["text"], 10):
            raise Step.Fail(f"模型抽屉里没有「{pick['text']}」")
        shot("4b-模型抽屉")
        tap(pick["text"])
        if not pane_has(name, f"> /model {pick['value']}", 20):
            raise Step.Fail(f"点了「{pick['text']}」，那个会话里没收到 /model {pick['value']}")
        end = ui.within(20)
        while time.time() < end and ui.find(t("session.model.title")) is not None:
            time.sleep(1)
        if ui.find(t("session.model.unsure")) is not None:
            raise Step.Fail("chip 上方说换模型结果未知（没读回）")
        if ui.find(pick["value"]) is None:
            raise Step.Fail(f"chip 没换成读回的字「{pick['value']}」")
        # 权限档 chip：抽屉里列那台给的档（session-new-facts）。替身不画档位那一行、也不认切档键（按下去的键会混进它读的下一行），这里只开抽屉不切。
        tap(t("session.permission.title"))
        modes = claude_models("permissionModes")
        if not wait(modes[0]["text"], 10):
            raise Step.Fail(f"权限档抽屉里没有「{modes[0]['text']}」")
        shot("4b-权限档抽屉")
        adb("shell", "input", "keyevent", "4")
        time.sleep(1)

    with Step("4c-加号面板") as st:
        st.back_to(f"{T} new session")
        # ［+］：命令清单是那台给这条会话的（session-commands），点一条只填进输入框；压缩上下文不确认、经 session-say 送；清空会话先确认，取消就不送。
        name = tmux_name_of(f"{T} new session")
        hide_keyboard()
        tap(t("mobPlus.sheet.title"))
        if not wait(t("mobPlus.row.commands"), 15):
            raise Step.Fail("面板里没有「命令」那一行")
        shot("4c-面板")
        tap(t("mobPlus.row.commands"))
        if not wait("/init", 10):
            raise Step.Fail("命令清单里没有 /init")
        shot("4c-命令清单")
        tap("/init")
        time.sleep(1.5)
        if ui.find("/init ") is None:
            raise Step.Fail("点了 /init 没填进输入框")
        if pane_has(name, "> /init", 2):
            raise Step.Fail("点了命令就发出去了（应只填进输入框）")
        tap("/init ")
        for _ in range(8):
            adb("shell", "input", "keyevent", "67")
        hide_keyboard()
        tap(t("mobPlus.sheet.title"))
        if not wait(t("mobPlus.row.clear"), 15):
            raise Step.Fail("面板里没有「清空会话」")
        tap(t("mobPlus.row.clear"))
        if not wait(t("session.clear.body"), 5):
            raise Step.Fail("清空会话没先确认")
        shot("4c-清空先确认")
        tap(t("kit.dialog.cancel"))
        time.sleep(2)
        if pane_has(name, "> /clear", 2):
            raise Step.Fail("取消了照样送了 /clear")
        tap(t("mobPlus.sheet.title"))
        if not wait(t("mobPlus.row.compact"), 15):
            raise Step.Fail("面板里没有「压缩上下文」")
        tap(t("mobPlus.row.compact"))
        if not pane_has(name, "> /compact", 20):
            raise Step.Fail("压缩上下文没送进那个会话")

    # 4d–4g：在聊天里答（会话是第 3 步手机起的：ccm 起会话时垫了答题 hook，替身照真的那样把请求交给它）。
    def answer_key():
        """钉条上的［答］：只认字正好是它的那一颗（「答题口：」那类正文里也有这个字）。"""
        return next((n for n in ui.nodes() if n.get("text") == t("mobNeeds.act.answer")), None)

    def ask_up(said, head, title=None, before=None):
        """发一句让替身停在框上；钉条出来（那台写的那一行）⇒ 点［答］升起抽屉（[title] ＝ 抽屉头一行，给了就核；[before] 升抽屉之前先看一眼）。"""
        fill(t("chat.input.placeholder"), said)
        hide_keyboard()
        tap(t("mobChat.input.send"))
        if not wait(head, 30):
            raise Step.Fail(f"钉条没出来（{head}）")
        to_bottom()
        if before:
            before()
        key = answer_key()
        if key is None:
            raise Step.Fail("钉条上没有［答］（答题口没挂上？）")
        adb("shell", "input", "tap", *map(str, ui.center(key)))
        time.sleep(1.5)
        if title and not wait(title, 5):
            raise Step.Fail(f"抽屉头一行不是「{title}」")

    def answered(said, want):
        """那一步的结果里落了答题口给的决定（替身原样写进记录），这一轮跟着答完。"""
        end = ui.within(30)
        while time.time() < end:
            log = sh("cat ~/.claude/projects/*/*.jsonl 2>/dev/null; true")
            if f"替身收到：{said}" in log:
                break
            time.sleep(1)
        def said_by_hook(ln):
            try:
                content = (json.loads(ln).get("message") or {}).get("content")
            except ValueError:
                return False
            return any("答题口：" in str(c.get("content")) and want in str(c.get("content")) for c in content if isinstance(c, dict)) if isinstance(content, list) else False

        rec = [ln for ln in log.splitlines() if said_by_hook(ln)]
        if not rec:
            raise Step.Fail(f"记录里没有答题口给的「{want}」")
        if answer_key() is not None:
            raise Step.Fail("答完了钉条还在")

    def step_badge():
        """流里在等的那一步：工具卡上那一枚是那台写好的「等批准」（needs.call 认的那一步）。"""
        want = t("beSession.needs.approve")  # 钉条头一行里也有这几个字：只认字正好是它的那一枚
        end = ui.within(20)
        while time.time() < end and not any(n.get("text") == want for n in ui.nodes()):
            adb("shell", "input", "swipe", "540", "1500", "540", "500", "150")
            time.sleep(1)
        if not any(n.get("text") == want for n in ui.nodes()):
            raise Step.Fail("流里那张在等的工具卡上没有「等批准」")
        shot("4d-工具卡等批准")

    with Step("4d-答批准") as st:
        st.back_to(f"{T} new session")
        ask_up(f"{T} ask go", t("needs.bar.approve", tool="Bash"), before=step_badge)
        always = t("beAsk.choice.allowAlways", rule=t("beAsk.rule.dirs", dirs="/w", scope=t("beAsk.scope.session")))
        for want in (t("beAsk.choice.allow"), always, t("beAsk.choice.denyStop")):
            if ui.find(want) is None:
                raise Step.Fail(f"抽屉里没有「{want}」")
        shot("4d-批准抽屉")
        tap(always)
        answered(f"{T} ask go", '"behavior": "allow"')

    with Step("4e-答提问") as st:
        st.back_to(f"{T} new session")
        ask_up(f"{T} quiz go", t("needs.bar.answer"), t("mobAsk.head.page", head=t("needs.bar.answer"), i=1, n=2))
        tap("Red")
        shot("4e-提问第一题")
        tap(t("mobAsk.act.next")); time.sleep(1)
        tap("Pear")
        tap(t("beAsk.choice.answer"))
        answered(f"{T} quiz go", '"Which color?": "Red"')

    with Step("4f-批计划") as st:
        st.back_to(f"{T} new session")
        ask_up(f"{T} plan go", t("needs.bar.plan"))
        if ui.find(t("beAsk.choice.planAuto")) is None:
            raise Step.Fail("抽屉里没有「自动模式」")
        shot("4f-批计划抽屉")
        tap(t("beAsk.choice.planManual"))
        answered(f"{T} plan go", '"mode": "default"')

    with Step("4g-别处先答了") as st:
        st.back_to(f"{T} new session")
        name = tmux_name_of(f"{T} ask go")
        ask_up(f"{T} ask again", t("needs.bar.approve", tool="Bash"))
        rig("tmux", "send-keys", "-t", name or "", "y", "Enter")  # 终端那一侧先答了
        if not wait(t("askBlock.gone.elsewhere"), 20):
            raise Step.Fail("抽屉里没出「已在终端答过或已关闭」")
        shot("4g-别处先答了")
        tap(t("mobKit.act.close"))
        time.sleep(1)
        if answer_key() is not None:
            raise Step.Fail("别处答完了钉条还在")

    with Step("4h-在终端里打开") as st:
        st.back_to(f"{T} new session")
        # ⋯ 里那一项：那台名单上认这条会话的终端（第二行是那个 tmux 会话名），点了终端屏开一个标签附着它。
        name = tmux_name_of(f"{T} new session")
        hide_keyboard()
        tap(t("history.row.more"))
        if not wait(t("sessionHead.act.openTerm"), 15):
            raise Step.Fail("⋯ 里没有「在终端里打开」")
        if not name or ui.find(name) is None:
            raise Step.Fail(f"「在终端里打开」第二行不是那个 tmux 会话名（{name}）")
        shot("4h-菜单")
        tap(t("sessionHead.act.openTerm"))
        def attached():
            rows = rig("tmux", "list-sessions", "-F", "#{session_name} #{session_attached}").stdout.splitlines()
            return any(r.rsplit(" ", 1)[0] == name and r.rsplit(" ", 1)[-1] != "0" for r in rows)

        end = ui.within(30)
        while time.time() < end and not attached():
            time.sleep(1)
        if time.time() >= end:
            raise Step.Fail(f"点了没附着到 {name}")
        shot("4h-终端屏")
        adb("shell", "input", "keyevent", "4")
        time.sleep(2)

    with Step("6-断网再连") as st:
        st.back_to(f"{T} new session")
        name = tmux_name_of(f"{T} new session")
        adb("shell", "cmd", "connectivity", "airplane-mode", "enable")
        # 断着：两颗 chip 灰、上方那一行写那台离线（连接那一层判的，桌面同一句）；网回来那一行撤掉。
        offline = t("newSession.machine.offline", machine=MACHINE)
        said_offline = wait(offline, 20)
        rig("tmux", "send-keys", "-t", name or "", f"{T} offline", "Enter")
        time.sleep(6)
        adb("shell", "cmd", "connectivity", "airplane-mode", "disable")
        time.sleep(10)
        to_bottom()
        if not wait(f"替身收到：{T} offline", 60):
            raise Step.Fail("网回来之后没补上断开那段")
        to_bottom()
        missing = [t for t in (f"替身收到：{T} offline",) if ui.find(t) is None]
        if missing:
            raise Step.Fail(f"漏了：{missing}")
        if not adb("shell", "pidof", PKG).strip():
            raise Step.Fail("App 没了（断网闪退）")
        if not said_offline:
            raise Step.Fail("断着时 chip 上方没写那台离线")
        if ui.find(offline) is not None:
            raise Step.Fail("网回来了还写着离线")

    with Step("7-换网") as st:
        st.back_to(f"{T} new session")
        # 换一张网（Wi-Fi 关掉 ⇒ 落到移动数据，再开回来）：SSH 那层重连换一条，常驻流在新的那条上当场重接；断着那段补上。
        name = tmux_name_of(f"{T} new session")
        adb("shell", "svc", "wifi", "disable")
        time.sleep(4)
        rig("tmux", "send-keys", "-t", name or "", f"{T} switched", "Enter")
        time.sleep(4)
        adb("shell", "svc", "wifi", "enable")
        time.sleep(6)
        to_bottom()
        if not wait(f"替身收到：{T} switched", 60):
            raise Step.Fail("换网之后没补上那一段")
        if not adb("shell", "pidof", PKG).strip():
            raise Step.Fail("App 没了（换网闪退）")

    with Step("8-杀进程"):
        # 进程整个没了（停掉，连前台服务一起）：内存里的续点都没了；重开 ⇒ 核心重接、先取尾巴，被杀那段的话照样在。
        name = tmux_name_of(f"{T} new session")
        adb("shell", "am", "force-stop", PKG)
        time.sleep(2)
        if adb("shell", "pidof", PKG).strip():
            raise Step.Fail("进程没停掉")
        rig("tmux", "send-keys", "-t", name or "", f"{T} while dead", "Enter")
        time.sleep(4)
        launch()
        drawer(t("mobDrawer.item.chats"))
        if not wait(f"{T} new session", 30):
            raise Step.Fail("重开之后对话列表里没有那条会话")
        tap(f"{T} new session")
        time.sleep(3)
        to_bottom()
        if not wait(f"替身收到：{T} while dead", 60):
            raise Step.Fail("重开之后没有被杀那段的话")

    with Step("8b-离线打开"):
        # 断着网重开 App：会话列表与那一条的最后一屏照本地缓存画（上次从那台读到的原样）；网回来跟上断着那段来的话。
        name = tmux_name_of(f"{T} new session")
        adb("shell", "am", "force-stop", PKG)
        adb("shell", "cmd", "connectivity", "airplane-mode", "enable")
        time.sleep(3)
        try:
            launch()
            drawer(t("mobDrawer.item.chats"))
            if not wait(f"{T} new session", 30):
                raise Step.Fail("断网时对话列表是空的（没上本地缓存）")
            if not wait(t("newSession.machine.offline", machine=MACHINE), 10):
                raise Step.Fail("断着那台的会话行没写离线")
            shot("8b-离线列表")
            tap(f"{T} new session")
            time.sleep(3)
            to_bottom()
            if not wait(f"替身收到：{T} while dead", 20):
                raise Step.Fail("断网时那一条的最后一屏没上本地缓存")
            shot("8b-离线那一条")
            rig("tmux", "send-keys", "-t", name or "", f"{T} back online", "Enter")
            time.sleep(4)
        finally:
            adb("shell", "cmd", "connectivity", "airplane-mode", "disable")
        time.sleep(8)
        # 断着网起的进程里 SSH 一次都没通过：网回来没有谁替它重拨（重连那一条循环归 SSH 层），这里退回列表再点进来、由那一屏重新建连；
        # 同一条对话还在（应用级持有），接上以后照缓存里的续点对齐，断着那段来的话补上。
        drawer(t("mobDrawer.item.chats"))
        if not wait(f"{T} new session", 30):
            raise Step.Fail("网回来之后对话列表里没有那条会话")
        tap(f"{T} new session")
        time.sleep(3)
        to_bottom()
        if not wait(f"替身收到：{T} back online", 60):
            raise Step.Fail("网回来之后没跟上断着那段来的话")

    with Step("9-终端屏"):
        # 从别名起：台架那台配一条别名（别名页那条写命令），终端屏「用哪个启动」那一排列的是它（profiles-read），
        # 点它 ⇒ 那个 shell 里敲 `ccm @名`、在台架 tmux 里起会话；再从 tmux 会话列表接进去，两侧打字都进那个会话、命令面板换模型。
        seed_alias()
        before = set(tmux_sessions())
        drawer(t("mobDrawer.item.terminal"))
        if not wait(SMOKE_ALIAS, 30):
            raise Step.Fail(f"终端屏「用哪个启动」那一排没列出那台的别名 {SMOKE_ALIAS}")
        shot("9a-终端屏-别名")
        hide_keyboard()
        tap(SMOKE_ALIAS)
        end = ui.within(30)
        while time.time() < end and not (set(tmux_sessions()) - before):
            time.sleep(1)
        fresh = sorted(set(tmux_sessions()) - before)
        if not fresh:
            raise Step.Fail(f"点了 {SMOKE_ALIAS}，台架 tmux 里没起新会话（ccm @{SMOKE_ALIAS} 没跑起来）")
        name = fresh[0]
        if not pane_has(name, "就绪", 30):
            raise Step.Fail(f"别名起的会话 {name} 里替身没就绪")
        end = ui.within(10)
        while ui.find(SMOKE_ALIAS) is not None and time.time() < end:
            time.sleep(0.5)
        if ui.find(SMOKE_ALIAS) is not None:
            raise Step.Fail("按过一个别名之后那一排没收起")
        adb("shell", "input", "keyevent", "4")
        drawer(t("mobDrawer.item.terminal"))
        hide_keyboard()
        tap(t("mobDefaultButtons.label.tmux"))
        if not wait(name, 15) or not any(n.get("text") == name for n in ui.nodes()):
            raise Step.Fail(f"tmux 会话列表里没有 {name}")
        attached = sum(1 for r in rig("tmux", "list-clients", "-F", "#{session_name}").stdout.split() if r == name)
        tap_in_row(name, t("mobTmux.row.attach"))
        end = ui.within(20)
        while time.time() < end and sum(1 for r in rig("tmux", "list-clients", "-F", "#{session_name}").stdout.split() if r == name) <= attached:
            time.sleep(1)
        typed(f"{T} from phone terminal")
        adb("shell", "input", "keyevent", "66")
        rig("tmux", "send-keys", "-t", name, f"{T} from desk", "Enter")
        if not (pane_has(name, f"> {T} from phone terminal", 20) and pane_has(name, f"> {T} from desk", 20)):
            raise Step.Fail("两侧打的字没都进那个会话")
        switch_model(name)
        adb("shell", "input", "keyevent", "4")

    with Step("10-Codex"):
        launch()
        # 哪几家是那台给的（session-new-facts）：新建会话顶栏下那一颗 agent chip 列得出，手机自己不认识 Codex。
        facts = json.loads(subprocess.run([sys.executable, os.path.join(here, "frames.py"), "session-new-facts", "{}"],
                                          capture_output=True, text=True).stdout or "{}")
        names = {a.get("kind"): a.get("text") for a in (facts.get("data") or {}).get("agents") or []}
        if "codex" not in names or "claude" not in names:
            raise Step.Fail(f"那台没给两家：{names!r}")
        drawer(t("mobDrawer.item.newChat"))
        if not wait(names["claude"], 20):
            raise Step.Fail("新建会话没画 agent chip")
        tap_until(names["claude"], names["codex"], what="agent 抽屉里那台能起的几家")
        tap(names["codex"])
        time.sleep(1)
        hide_keyboard()
        fill(t("chat.input.placeholder"), f"{T} codex")
        hide_keyboard()
        tap(t("mobChat.input.send"))
        if not wait(f"替身收到：{T} codex", 40):
            raise Step.Fail("Codex 答不回来")

    with Step("11-文件页"):
        launch()
        files_page()

    report()


def report():
    with Step("12-轮换屏"):
        # 从会话进轮换屏（聊天屏「⋯」→ 轮换）· 换策略 · 长按号名拖到最上那条成临时条目 · 改一格 · 存；每一样都核那台的记录。
        seed_rotation()
        started = frames("session-new", {"agent": "claude", "cwd": "~/work", "place": "tmux", "local": False, "ticket": f"{T}rot",
                                         "prompt": f"{T} rotation", "rotation": {"strategy": "smoke-a"}}, 45000)
        sid = (started.get("data") or {}).get("sid")
        if not sid:
            raise Step.Fail(f"台架起不了带轮换的会话：{started!r}"[:300])
        launch()
        drawer(t("mobDrawer.item.chats"))
        if not wait(f"{T} rotation", 40):
            raise Step.Fail("对话列表里没有那条带轮换的会话")
        tap(f"{T} rotation")
        time.sleep(3)
        tap(t("history.row.more"))
        time.sleep(1)
        tap(t("mobChat.more.rotation"))
        if not wait("smoke-a", 20):
            raise Step.Fail("轮换屏没画出那条会话用的策略")
        if not wait(t("rot.h.cards"), 10) or ui.find(t("mobRot.cards.hint")) is None:
            raise Step.Fail("轮换屏没画出轮换账号那一块")
        shot("12a-轮换屏")
        tap("smoke-a")
        if not wait("smoke-b", 10):
            raise Step.Fail("策略抽屉里没有 smoke-b")
        tap("smoke-b")
        end = ui.within(20)
        while time.time() < end and ((frames("rotation-view", {"target": {"sid": sid}}).get("data") or {}).get("strategy") or {}).get("id") != "smoke-b":
            time.sleep(1)
        if time.time() >= end:
            raise Step.Fail("选了 smoke-b，那台记的那条会话没换过去")
        if not wait("smoke-b", 10):
            raise Step.Fail("换了策略，屏上没跟着换")
        to_bottom()
        time.sleep(1)
        head = lane_head("team")
        top = next((n for n in ui.nodes() if n.get("content-desc") == t("rot.h.sim")), None)
        if head is None or top is None:
            raise Step.Fail("模拟里找不到 team 那一行或最上那条")
        (x1, y1), (x2, y2) = ui.center(head), ui.center(top)
        adb("shell", "input", "draganddrop", str(x1), str(y1), str(x2), str(y2), "1500")
        end = ui.within(20)
        while time.time() < end and "team" not in (((frames("rotation-view", {"target": {"sid": sid}}).get("data") or {}).get("decision") or {}).get("temp") or []):
            time.sleep(1)
        if time.time() >= end:
            raise Step.Fail("拖了 team 到最上那条，那台没记成临时条目")
        if not wait(t("rot.sim.manual", n=1), 10):
            raise Step.Fail("排了临时条目，模拟的小标题没出「手动 1 段」")
        shot("12b-临时条目")
        adb("shell", "input", "swipe", "540", "700", "540", "1600", "200")
        time.sleep(1)
        box = ui.find("25")
        if box is None:
            raise Step.Fail("找不到「权重容差」那一格的数")
        adb("shell", "input", "tap", *map(str, ui.center(box)))
        time.sleep(0.5)
        adb("shell", "input", "keyevent", "123")
        for _ in range(4):
            adb("shell", "input", "keyevent", "67")
        typed("30")
        adb("shell", "input", "keyevent", "66")
        if not wait(t("rot.top.save"), 15):
            raise Step.Fail("改了一格，底部没出没存条")
        shot("12c-没存")
        tap(t("rot.top.save"))
        end = ui.within(20)
        while time.time() < end and not re.search(r'"close":\s*30(\.0)?\b', sh("cat ~/.cc-monitor/strategies/smoke-b.rhai")):
            time.sleep(1)
        if time.time() >= end:
            raise Step.Fail("点了存，smoke-b 那份文件里没写进 30")
        hide_keyboard()
        if wait(t("rot.top.save"), 3):
            raise Step.Fail("存完没存条还在")
        # 另一个进口：设置 → 机器与账号 → 轮换（这台新会话用的那份），点得到。
        adb("shell", "input", "keyevent", "4")
        time.sleep(1)
        drawer(t("mobDrawer.settings.aria"))
        if not wait(t("mobSettings.entry.rotation"), 10):
            raise Step.Fail("设置里没有「机器与账号 → 轮换」")
        tap(t("mobSettings.entry.rotation"))
        if not wait(t("rot.top.here", machine="smokebox"), 15):
            raise Step.Fail("设置进的轮换屏顶栏不是「设置 · 那台」")

    print()
    print("| 步 | 结果 | 用时 | 期限放宽 | 截图 |")
    print("|---|---|---|---|---|")
    for name, res, took, slack, p in results:
        print(f"| {name} | {res} | {took} | ×{slack} | {p} |")
    if ui.anrs:
        # 应用无响应照实记（时刻 · 框上的字 · 系统留的那份记录），关掉接着走过的步不算绿。
        open(os.path.join(OUT, "anr.txt"), "w").write(adb("shell", "dumpsys", "dropbox", "--print", "data_app_anr"))
        for at, said in ui.anrs:
            print(f"应用无响应 {at}：{said}")
        print(f"有 {len(ui.anrs)} 回应用无响应（系统记录在 {os.path.join(OUT, 'anr.txt')}），这一趟不算绿")
    json.dump({"steps": results, "anrs": ui.anrs}, open(os.path.join(OUT, "results.json"), "w"), ensure_ascii=False, indent=1)
    sys.exit(0 if all(r[1] == "过" for r in results) and not ui.anrs else 1)


if __name__ == "__main__":
    main()
