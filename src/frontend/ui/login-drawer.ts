/**
 * 登录抽屉（账号 · MCP 同一个）：「需手动」浮层里会话外那一行 · 账号页需登录那一行 · 扩展页 MCP 那一行都开这一个。
 *
 * 从上往下：标题 · ✕ ／ 目标行（号牌 ＋ 号名 · 机器；MCP 前面多一个服务器名）／［打开登录页］·［复制链接］· 截断的链接 ／
 * 「登录页完成后自动结束」（只在后端答 `auto` 时）／ 粘贴那一格（名字照 `paste`）·［粘贴］·［提交］／ 底部［取消］（作业没了时［关闭］）。
 *
 * 每个动作发什么：开抽屉同时 `login-start`（入参 ＝ 那一项的 `fix.args`，或照那一行拼的同一形）；［打开登录页］用系统浏览器开；
 * ［提交］`login-finish {job, pasted}`；［取消］· ✕ · Esc · 点遮罩 `login-cancel {job}`、不确认；［重新开始］再发一次 `login-start`。
 * 本机自己回跳：等推来的 `changed {login, key: job}`。失败后怎么画全照后端的 `again`（接着粘 · 重新开始 · 没法再来），不按码分。
 */
import { openUrl } from "@tauri-apps/plugin-opener";
import { openDrawer, type DrawerHandle } from "./kit/drawer";
import { button, setBusy, setDisabled } from "./kit/button";
import { field, type FieldHandle } from "./kit/field";
import { banner } from "./kit/banner";
import { icon } from "./kit/icon";
import { spinner } from "./kit/progress";
import { attachTooltip } from "./kit/tooltip";
import { copyText } from "./copy-table";
import { readClipboard, writeClipboard } from "./clipboard";
import { accountAvatarEl } from "./account-color";
import { accountLabel } from "./acct-words";
import { machineName } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { bindEvents } from "./events";
import type { PasteKind } from "./generated/PasteKind";
import { LoginFailed, loginCancel, loginFinish, loginStart, type LoginAgainHow, type LoginStartReq, type LoginStarted, type SignInState } from "./login-reads";
import s from "./login-drawer.module.css";

/** 两种目标只差两处：标题 · 粘贴那一格的名字与定义。 */
const TITLE: Record<LoginStartReq["kind"], () => string> = {
  account: () => copyText("login.sheet.account"),
  mcp: () => copyText("login.sheet.mcp"),
};
const PASTE: Record<PasteKind, { name: () => string; help: () => string }> = {
  code: { name: () => copyText("login.paste.code"), help: () => copyText("login.paste.codeHelp") },
  redirectUrl: { name: () => copyText("login.paste.redirectUrl"), help: () => copyText("login.paste.redirectUrlHelp") },
};

/** 抽屉宽（420）。 */
const WIDTH = 420;
/** ［复制链接］原位勾一下多久。 */
const COPIED_MS = 1500;

// ───────────────────────────── 推来的那一帧 ─────────────────────────────

/** 作业号 ⇒ 正开着的那个抽屉收推送的口。 */
const waiting = new Map<string, (st: SignInState) => void>();
/** 本窗口已经订了哪几台的 `changed/login`（一台订一次，之后一直在）。 */
const fed = new Map<Origin, Promise<void>>();

/** 订那台的 `changed/login`（起登录之前订好：本机自己回跳那一帧只推一次）。订不上 ⇒ 只是收不到自己结束的那一下，粘码照样能走。 */
function feed(origin: Origin): Promise<void> {
  let p = fed.get(origin);
  if (!p) {
    p = (async (): Promise<void> => {
      await bindEvents(
        {
          onLine: () => {},
          onSessionEnded: () => {},
          onChanged: (_o, topic, change) => {
            if (topic !== "login") return;
            for (const c of change.cells) {
              const st = signInOf(c.body);
              if (c.key !== null && st) waiting.get(c.key)?.(st);
            }
          },
        },
        { changed: [{ origin, topic: "login" }] },
      );
    })().catch((e: unknown) => console.warn("[login] 订不上登录推送：", e));
    fed.set(origin, p);
  }
  return p;
}

/** 推来那一帧的 `body`（`{state: "done"}` · `{state: "failed", said, detail}`）；形状不对 ⇒ `null`（不收）。 */
function signInOf(v: unknown): SignInState | null {
  if (v === null || typeof v !== "object") return null;
  const o = v as { state?: unknown; said?: unknown; detail?: unknown };
  if (o.state === "done") return { state: "done" };
  if (o.state === "failed" && typeof o.said === "string") return { state: "failed", said: o.said, detail: typeof o.detail === "string" ? o.detail : null };
  return null;
}

// ───────────────────────────── 抽屉 ─────────────────────────────

export interface LoginDrawerSpec {
  /** 登录那一台（帧命令问它；目标行里的机器名照它）。 */
  origin: Origin;
  /** 登录什么（`login-start` 的入参：那一项的 `fix.args`，或照那一行拼的同一形）。 */
  target: LoginStartReq;
  /** 登成了（抽屉已关）：调用方要重读的自己重读（清单 · 账号行也会随推送自己变）。 */
  onDone?: () => void;
}

/** 此刻在哪一步。 */
type Step =
  | { t: "starting" }
  | { t: "ready"; started: LoginStarted; submitting: boolean; pasteError: string | null }
  | { t: "failed"; said: string; detail: string; again: Exclude<LoginAgainHow, "paste"> };

/** 开登录抽屉（开着另一个 ⇒ 先关那一个）。 */
export function openLoginDrawer(spec: LoginDrawerSpec): DrawerHandle {
  void current?.close();
  const d = new LoginDrawer(spec);
  current = d.handle;
  return d.handle;
}

let current: DrawerHandle | null = null;

class LoginDrawer {
  readonly handle: DrawerHandle;
  private step: Step = { t: "starting" };
  /** 这一趟的作业号（还在 ⇒ 关抽屉要撤它）。 */
  private job: string | null = null;
  /** 第几趟（重新开始 ⇒ 加一；旧那一趟回来的不收）。 */
  private round = 0;
  private closed = false;
  private readonly body = document.createElement("div");
  private readonly foot = document.createElement("div");
  private readonly bottom: HTMLButtonElement;
  private paste: FieldHandle | null = null;

  constructor(private readonly spec: LoginDrawerSpec) {
    this.body.className = s.ldBody;
    this.bottom = button({ label: copyText("kit.dialog.cancel"), onClick: () => void this.handle.close() });
    this.bottom.dataset.action = "login-bottom";
    this.foot.appendChild(this.bottom);
    this.handle = openDrawer({
      title: TITLE[spec.target.kind](),
      body: this.body,
      foot: this.foot,
      width: WIDTH,
      scrim: true,
      onClose: () => this.onClosed(),
    });
    this.handle.root.dataset.loginKind = spec.target.kind;
    this.paint();
    void this.start();
  }

  /** 发 `login-start`（开抽屉那一下 ·［重新开始］）。 */
  private async start(): Promise<void> {
    const my = ++this.round;
    this.dropJob();
    this.step = { t: "starting" };
    this.paste = null;
    this.paint();
    await feed(this.spec.origin);
    let started: LoginStarted;
    try {
      started = await loginStart(this.spec.origin, this.spec.target);
    } catch (e) {
      if (my !== this.round || this.closed) return;
      const f = e instanceof LoginFailed ? e : new LoginFailed(String(e), "", "restart");
      // 起都没起成：没有作业，接着粘无从谈起 ⇒ 照「重新开始」画。
      this.step = { t: "failed", said: f.said, detail: f.detail, again: f.again === "none" ? "none" : "restart" };
      this.paint();
      return;
    }
    if (my !== this.round || this.closed) {
      void loginCancel(this.spec.origin, started.job);
      return;
    }
    this.job = started.job;
    waiting.set(started.job, (st) => this.pushed(started.job, st));
    this.step = { t: "ready", started, submitting: false, pasteError: null };
    this.paint();
    // 焦点从 ✕ 挪到主动作上（开抽屉那一下还没有别的可聚焦）。
    if (this.handle.root.contains(document.activeElement)) this.body.querySelector<HTMLElement>('[data-action="login-open"]')?.focus();
  }

  /** 推来的结局（只认这一趟的作业）。 */
  private pushed(job: string, st: SignInState): void {
    if (job !== this.job || this.closed) return;
    this.dropJob();
    if (st.state === "done") {
      this.finished();
      return;
    }
    // 作业自己收了 ⇒ 粘不了了，只能重新开始（推来的那一帧不带 `again`，见交回「后端缺的格」）。
    this.step = { t: "failed", said: st.said, detail: st.detail ?? "", again: "restart" };
    this.paint();
  }

  /** ［提交］。 */
  private async submit(): Promise<void> {
    const step = this.step;
    const job = this.job;
    if (step.t !== "ready" || step.submitting || job === null || !this.paste) return;
    const pasted = this.paste.input.value;
    if (pasted.trim() === "") return;
    this.step = { ...step, submitting: true, pasteError: null };
    this.paint();
    try {
      await loginFinish(this.spec.origin, job, pasted);
    } catch (e) {
      if (this.closed || this.job !== job) return;
      const f = e instanceof LoginFailed ? e : new LoginFailed(String(e), "", "restart");
      if (f.again === "paste") {
        this.step = { ...step, submitting: false, pasteError: f.said };
      } else {
        this.dropJob();
        this.step = { t: "failed", said: f.said, detail: f.detail, again: f.again };
      }
      this.paint();
      return;
    }
    if (this.closed || this.job !== job) return;
    this.dropJob();
    this.finished();
  }

  /** 登成了：抽屉关，不另弹 toast（那一行自己变了）。 */
  private finished(): void {
    void this.handle.close();
    this.spec.onDone?.();
  }

  /** 作业不再归这个抽屉（收了 · 没了 · 要换一趟）。 */
  private dropJob(): void {
    if (this.job !== null) waiting.delete(this.job);
    this.job = null;
  }

  /** 关了（✕ · Esc · 点遮罩 ·［取消］·［关闭］· 登成了）：作业还在 ⇒ 撤掉，不确认。 */
  private onClosed(): void {
    this.closed = true;
    if (current === this.handle) current = null;
    const job = this.job;
    this.dropJob();
    if (job !== null) void loginCancel(this.spec.origin, job);
  }

  // ───────────────────────────── 画 ─────────────────────────────

  private paint(): void {
    const step = this.step;
    const parts: HTMLElement[] = [this.who()];
    if (step.t === "failed") {
      const acts = step.again === "restart" ? [button({ label: copyText("login.act.restart"), kind: "primary", onClick: () => void this.start() })] : [];
      const b = banner("error", step.said, acts, step.detail);
      b.dataset.again = step.again;
      parts.push(b);
    } else {
      parts.push(this.links(step));
      if (step.t === "ready") {
        if (step.started.auto) {
          const auto = el("div");
          auto.className = s.ldAuto;
          auto.dataset.part = "auto";
          auto.append(spinner(), el("span", copyText("login.sheet.auto")));
          parts.push(auto);
        }
        const rule = el("div");
        rule.className = s.ldRule;
        parts.push(rule, this.pasteArea(step));
      }
    }
    this.body.replaceChildren(...parts);
    // 作业已经没了 ⇒ 底部是［关闭］不是［取消］。
    const label = step.t === "failed" ? copyText("kit.drawer.close") : copyText("kit.dialog.cancel");
    const span = this.bottom.lastElementChild;
    if (span) span.textContent = label;
  }

  /** 目标行：号 ⇒ 号牌 ＋ 号名 · 机器；MCP ⇒ 服务器名 · 号牌 ＋ 号名 · 机器（只摆名字，不成句）。 */
  private who(): HTMLElement {
    const t = this.spec.target;
    const row = el("div");
    row.className = s.ldWho;
    row.dataset.part = "who";
    const sep = (): HTMLElement => {
      const d = el("span", copyText("kit.text.sep").trim());
      d.className = s.ldDim;
      return d;
    };
    if (t.kind === "mcp") {
      const server = el("span", t.server);
      server.className = s.ldName;
      row.append(icon("plug", "compact"), server, sep());
    }
    if (t.account !== null) {
      const acct = el("span", accountLabel(t.account));
      if (t.kind === "account") acct.className = s.ldName;
      row.append(accountAvatarEl(t.account, { size: 16, origin: this.spec.origin }), acct);
    }
    const machine = el("span", machineName(this.spec.origin));
    machine.className = s.ldDim;
    row.append(sep(), machine);
    return row;
  }

  /** ［打开登录页］·［复制链接］· 截断的链接（起登录中 ⇒ 只有转圈的［打开登录页］）。 */
  private links(step: Step): HTMLElement {
    const row = el("div");
    row.className = s.ldLinks;
    const open = button({ label: copyText("login.act.open"), kind: "primary", icon: "front", onClick: () => (step.t === "ready" ? void this.openPage(step.started.url) : undefined) });
    open.dataset.action = "login-open";
    row.appendChild(open);
    if (step.t !== "ready") {
      setBusy(open, copyText("login.act.open"));
      return row;
    }
    const url = step.started.url;
    const copy = button({ label: copyText("login.act.copyLink"), kind: "ghost", icon: "copy", onClick: () => void this.copyLink(copy, url) });
    copy.dataset.action = "login-copy";
    const shown = el("span", url);
    shown.className = s.ldUrl;
    shown.title = url;
    row.append(copy, shown);
    return row;
  }

  /** 粘贴那一格：名字照 `paste`（ⓘ 悬停是那一格的定义）· 输入框 ·［粘贴］·［提交］；接着粘那一种失败的那一句在框下面。 */
  private pasteArea(step: Extract<Step, { t: "ready" }>): HTMLElement {
    const kind = PASTE[step.started.paste];
    const keep = this.paste?.input.value ?? "";
    const name = kind.name();
    const f = field({ label: name, value: keep, noteOnDemand: true });
    f.root.dataset.part = "paste";
    const label = f.root.querySelector("label");
    if (label) {
      const info = el("span");
      info.className = s.ldInfo;
      info.tabIndex = 0;
      const help = kind.help();
      info.setAttribute("aria-label", help);
      info.appendChild(icon("info", "compact"));
      attachTooltip(info, help, { immediate: true });
      label.appendChild(info);
    }
    f.input.spellcheck = false;
    if (step.pasteError !== null) f.setError(step.pasteError);
    f.setReadonly(step.submitting);
    const clip = button({ label: copyText("login.act.paste"), kind: "ghost", icon: "files", onClick: () => void this.fromClipboard() });
    clip.dataset.action = "login-paste";
    const submit = button({ label: copyText("login.act.submit"), kind: "primary", onClick: () => void this.submit() });
    submit.dataset.action = "login-submit";
    const sync = (): void => setDisabled(submit, f.input.value.trim() === "" ? name : null);
    f.input.addEventListener("input", () => {
      sync();
      if (f.root.dataset.error === "true" && this.step.t === "ready") {
        this.step = { ...this.step, pasteError: null };
        f.setError(null);
      }
    });
    f.input.addEventListener("keydown", (e) => {
      const ev = e as KeyboardEvent;
      if (ev.key === "Enter" && !ev.isComposing) {
        ev.preventDefault();
        void this.submit();
      }
    });
    if (step.submitting) {
      setDisabled(clip, copyText("login.act.submit"));
      setBusy(submit, copyText("login.act.submit"));
    } else {
      sync();
    }
    this.paste = f;
    const line = el("div");
    line.className = s.ldPasteLine;
    const wrap = el("div");
    wrap.className = s.ldPaste;
    wrap.appendChild(f.root);
    line.append(wrap, clip);
    const box = el("div");
    box.className = s.ldPasteBox;
    const act = el("div");
    act.className = s.ldSubmitRow;
    act.appendChild(submit);
    box.append(line, act);
    return box;
  }

  private async openPage(url: string): Promise<void> {
    try {
      await openUrl(url);
    } catch (e) {
      console.warn("[login] 打不开登录页：", e);
    }
  }

  private async copyLink(b: HTMLButtonElement, url: string): Promise<void> {
    try {
      await writeClipboard(url);
    } catch (e) {
      console.warn("[login] 复制链接没成：", e);
      return;
    }
    // 原位勾一下，不弹 toast。
    const ic = b.querySelector("svg");
    if (ic) ic.replaceWith(icon("check", "regular"));
    b.dataset.state = "copied";
    // 调度：一次性 ——［复制链接］那一勾过一会儿换回复制图标
    setTimeout(() => {
      b.querySelector("svg")?.replaceWith(icon("copy", "regular"));
      delete b.dataset.state;
    }, COPIED_MS);
  }

  /** ［粘贴］：从剪贴板原样填进那一格，不判像不像。 */
  private async fromClipboard(): Promise<void> {
    let got: string;
    try {
      got = await readClipboard();
    } catch (e) {
      console.warn("[login] 读不出剪贴板：", e);
      return;
    }
    const f = this.paste;
    if (!f || this.closed) return;
    f.input.value = got;
    f.input.dispatchEvent(new Event("input"));
    f.input.focus();
  }
}

function el(tag: string, t?: string): HTMLElement {
  const e = document.createElement(tag);
  if (t !== undefined) e.textContent = t;
  return e;
}
