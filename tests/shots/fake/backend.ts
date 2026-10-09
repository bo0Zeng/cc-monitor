/**
 * 页里的假后端：答产品代码发出的 Tauri 命令（`load_config` …）、帧命令（`chan_call` 里的 op）与订阅（`chan_subscribe`）。
 * 答不上的一律记进 `unhandled`（工具最后列出来），并按真后端「不认」那一形拒绝 —— 不悄悄给空。
 */
import type { SessionStreamFrame } from "../../../src/frontend/ui/generated/SessionStreamFrame";
import { Refuse, type World } from "./types";
import { copyText } from "../../../src/frontend/ui/copy-table";

type Emit = (event: string, payload: unknown) => unknown;

/** 真后端出口给每个时刻添 `…Text` 的那两条（`common::time::with_texts`）。 */
const TIMED_OPS = new Set(["quota-read", "rotation-session-read"]);
const TIME_KEYS = ["at", "seenAt", "resetsAt", "fromResetsAt", "since"];

/** 假后端替真后端出口写时刻的字（当天 `HH:MM` · 别的天 `MM-DD HH:MM` · 别的年带年；按截图机的本地钟）。 */
function withTexts(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(withTexts);
  if (v === null || typeof v !== "object") return v;
  const out: Record<string, unknown> = {};
  const now = new Date();
  for (const [k, x] of Object.entries(v)) {
    out[k] = withTexts(x);
    if (TIME_KEYS.includes(k) && typeof x === "number") {
      const d = new Date(x * 1000);
      const p = (n: number) => String(n).padStart(2, "0");
      const hm = `${p(d.getHours())}:${p(d.getMinutes())}`;
      const md = `${p(d.getMonth() + 1)}-${p(d.getDate())}`;
      out[`${k}Text`] = d.toDateString() === now.toDateString() ? hm : d.getFullYear() === now.getFullYear() ? `${md} ${hm}` : `${d.getFullYear()}-${md} ${hm}`;
    }
  }
  return out;
}

const enc = new TextEncoder();
const dec = new TextDecoder();

interface Sub {
  id: number;
  origin: string;
  kind: string;
}

export class FakeBackend {
  readonly unhandled: string[] = [];
  private emit: Emit = () => undefined;
  private readonly subs = new Map<number, Sub>();
  /** 报过「清单报完了」的那几台（壳那一侧「各台都报完」那一拍按机器表算，这里照样算）。 */
  private readonly listedOrigins = new Set<string>();

  constructor(readonly world: World) {}

  attachEmitter(emit: Emit): void {
    this.emit = emit;
  }

  private miss(what: string): void {
    if (!this.unhandled.includes(what)) this.unhandled.push(what);
    console.warn(`[shots] 假后端答不上：${what}`);
  }

  invoke(cmd: string, args: Record<string, unknown>): unknown {
    switch (cmd) {
      case "chan_call":
        return this.chanCall(args);
      case "chan_offer":
        return { ops: Object.keys(this.world.ops), unavailable: [], stoppable: [] };
      case "chan_subscribe":
        this.subscribe(args);
        return null;
      case "chan_want":
      case "chan_stop":
      case "chan_cancel":
        return null;
    }
    const own = this.world.commands[cmd];
    if (own) return own(args, this.world);
    if (cmd.startsWith("plugin:")) return pluginDefault(cmd, args);
    this.miss(`命令 ${cmd}`);
    return Promise.reject(`shots: 没有这条命令 ${cmd}`);
  }

  private chanCall(args: Record<string, unknown>): unknown {
    const origin = String(args.origin);
    const op = String(args.op);
    const handler = this.world.ops[op];
    if (!handler) {
      if (!this.world.quiet?.includes(op)) this.miss(`帧命令 ${op}`);
      return Promise.reject({ err: "Unsupported", body: [] });
    }
    const raw = args.payload as number[];
    const req = raw.length > 0 ? (JSON.parse(dec.decode(Uint8Array.from(raw))) as Record<string, unknown>) : {};
    try {
      const v = handler(origin, req, this.world);
      const delay = this.world.opDelayMs?.[op] ?? 0;
      const later = delay > 0 ? new Promise((r) => setTimeout(() => r(v), delay)) : Promise.resolve(v);
      return later.then(
        // 照真壳交原始字节（`tauri::ipc::Response` ⇒ 页里拿到 ArrayBuffer），不交数字数组：
        // 长会话整份读那一问有几 MB，数字数组那一形光假后端自己造就占掉页里几百 ms（性能台架量的是产品）
        (value) => enc.encode(JSON.stringify(TIMED_OPS.has(op) ? withTexts(value) : value)).buffer,
        (e: unknown) => Promise.reject(refusal(e, op)),
      );
    } catch (e) {
      return Promise.reject(refusal(e, op));
    }
  }

  private subscribe(args: Record<string, unknown>): void {
    const sub: Sub = { id: Number(args.id), origin: String(args.origin), kind: String(args.kind) };
    this.subs.set(sub.id, sub);
    // 订阅登记好之后才交格（与真句柄一样：返回之后格才可能到）。
    setTimeout(() => this.deliver(sub), 0);
  }

  private deliver(sub: Sub): void {
    if (this.world.unseenMachines.includes(sub.origin)) {
      this.send(sub, [{ t: "unseen", idx: 1, tag: "open", why: "Unreachable" }]);
      return;
    }
    if (this.world.closedMachines.includes(sub.origin) && sub.kind === "session-lines") {
      this.send(sub, [{ t: "closed_by_peer", body: JSON.stringify({ code: "busy", message: "这台机器上的会话太多，先关掉几个再连" }) }]);
      return;
    }
    if (sub.kind === "session-lines" || sub.kind.startsWith("session-lines/")) {
      const only = sub.kind.startsWith("session-lines/") ? sub.kind.slice("session-lines/".length) : null;
      this.sendFrames(sub, this.sessionFrames(sub.origin, only));
      if (this.world.droppedMachines?.includes(sub.origin)) {
        // 断了：壳那一侧的会话账本把这台的成品作废（会话流里一格 `unseen`：那台还活着的 / 可重连的一律状态不明），通道再说一声。
        setTimeout(() => {
          this.sendFrames(sub, [{ unseen: { origin: sub.origin } } as SessionStreamFrame]);
          this.send(sub, [{ t: "unseen", idx: 1, tag: "read", why: "Dropped" }]);
        }, 1500);
      }
    } else if (sub.kind.startsWith("terminal-screen/")) {
      // 终端实时画面：订上之后推一屏（就是那一刻的 `terminal-preview` 成品）；按场景随后停（画面中断 · 那台断开）。
      // 那一屏抓不到（场景让 `terminal-preview` 拒）⇒ 不推：真后端那边是 `terminal-follow` 那一问被拒，流上什么都不来。
      const mode = this.world.terminalLive ?? "live";
      let view: unknown;
      try {
        view = this.world.ops["terminal-preview"]?.(sub.origin, {}, this.world);
      } catch (e) {
        if (e instanceof Refuse) return;
        throw e;
      }
      setTimeout(() => this.send(sub, [{ t: "frame", seq: 0, body: JSON.stringify({ seq: 1, view }) }]), 200);
      if (mode === "lost") setTimeout(() => this.send(sub, [{ t: "frame", seq: 1, body: JSON.stringify({ end: "lost" }) }]), 500);
      if (mode === "offline") setTimeout(() => this.send(sub, [{ t: "unseen", idx: 1, tag: "read", why: "Dropped" }]), 500);
    } else if (sub.kind === "session-tap" || sub.kind === "accounts-changed" || sub.kind === "profiles-changed" || sub.kind === "session-tasks" || sub.kind === "quota-changed" || sub.kind === "plan-changed") {
      this.send(sub, [{ t: "seen", from: null }]);
    } else {
      this.miss(`订阅 ${sub.kind}`);
    }
  }

  /** 一台机器的会话流：成批那一段里先宣告、再交行，最后报「清单报完了」。 */
  sessionFrames(origin: string, only: string | null = null): SessionStreamFrame[] {
    const frames: SessionStreamFrame[] = [{ batch: "start" }];
    for (const s of this.world.sessions.filter((x) => x.origin === origin && (only === null || x.sid === only))) {
      frames.push({
        live: {
          session_id: s.sid,
          origin,
          background: s.background,
          attachable: true,
          cwd: s.cwd,
          project_dir: s.cwd,
          name: s.name,
        },
      });
      frames.push({ container: { session_id: s.sid, container: s.container } });
      const from = this.world.replayTail === undefined ? 0 : Math.max(0, s.records.length - this.world.replayTail);
      s.records.slice(from).forEach((message, i) => {
        frames.push({ line: { session_id: s.sid, cwd: s.cwd, path: `${s.cwd}/${s.sid}.jsonl`, seq: from + i, origin, message } });
      });
      if (s.runs.length > 0) frames.push({ runs: { session_id: s.sid, runs: s.runs, ended: [] } });
      frames.push({ activity: { session_id: s.sid, activity: s.activity, waiting_for: s.waitingFor } });
      if (s.ended) frames.push({ ended: { session_id: s.sid } });
      else if (s.idle) frames.push({ idle: { session_id: s.sid } });
    }
    this.listedOrigins.add(origin);
    frames.push({ listed: { origin, all: this.world.machines.every((m) => this.listedOrigins.has(m)) } });
    frames.push({ batch: "end" });
    return frames;
  }

  private sendFrames(sub: Sub, frames: SessionStreamFrame[]): void {
    this.send(
      sub,
      frames.map((f, seq) => ({ t: "frame", seq, body: JSON.stringify(f) })),
    );
  }

  private send(sub: Sub, items: unknown[]): void {
    void this.emit("chan-items", { sub: sub.id, items });
  }

  /** 场景在开页之后再推一格（例如活卡、某会话结束）。 */
  pushFrames(origin: string, frames: SessionStreamFrame[]): void {
    for (const sub of this.subs.values()) {
      if (sub.origin === origin && sub.kind.startsWith("session-lines")) this.sendFrames(sub, frames);
    }
  }

  /** 场景在开页之后推一格 `quota-changed`（`{"quota":true}` / `{"sid": …}`）。 */
  pushQuota(origin: string, body: unknown): void {
    for (const sub of this.subs.values()) {
      if (sub.origin === origin && sub.kind === "quota-changed") this.send(sub, [{ t: "frame", seq: 0, body: JSON.stringify(body) }]);
    }
  }

  pushTap(origin: string, payloads: unknown[]): void {
    for (const sub of this.subs.values()) {
      if (sub.origin === origin && sub.kind === "session-tap") {
        this.send(
          sub,
          payloads.map((p, seq) => ({ t: "frame", seq, body: JSON.stringify(p) })),
        );
      }
    }
  }
}

/** 替身那一份复制详情（那台后端写、monitor 转交的那几行）：钟停在夹具那一刻，机器与版本是合成的。 */
function detailOf(op: string, e: Refuse): string {
  const l = (k: Parameters<typeof copyText>[0]): string => copyText(k);
  return [
    `${l("detail.label.at")}：2026-10-08 14:35:50 +08:00`,
    `${l("detail.label.machine")}：Linux x86_64 · ${copyText("detail.value.backend", { build: "p9k-shots" })}`,
    `${l("detail.label.command")}：${op}`,
    `${l("detail.label.code")}：${e.code}`,
    ...(e.raw !== undefined ? [`${l("detail.label.raw")}：${e.raw}`] : []),
  ].join("\n");
}

function refusal(e: unknown, op = ""): { err: unknown; body: number[]; detail?: string } {
  // 场景要演通道那一跳的失败（够不着 · 期限到）：直接抛线上形状 `{err, body}`，原样交回。
  if (e !== null && typeof e === "object" && "err" in e && "body" in e) return e as { err: unknown; body: number[] };
  if (e instanceof Refuse) {
    const body = e.data === undefined ? { code: e.code, message: e.message } : { code: e.code, message: e.message, data: e.data };
    return { err: "Refused", body: Array.from(enc.encode(JSON.stringify(body))), detail: detailOf(op, e) };
  }
  return { err: "Refused", body: Array.from(enc.encode(JSON.stringify({ code: "shots", message: String(e) }))) };
}

/** 窗口 / 路径 / 对话框这几个插件：截图里不真开窗、不真选文件，给一个不出错的答。 */
function pluginDefault(cmd: string, args: Record<string, unknown>): unknown {
  if (cmd === "plugin:path|resolve_directory") return "/home/user";
  if (cmd === "plugin:path|join") return (args.paths as string[]).join("/");
  if (cmd === "plugin:window|scale_factor") return 1;
  if (cmd === "plugin:window|inner_size" || cmd === "plugin:window|outer_size") {
    return { width: window.innerWidth, height: window.innerHeight };
  }
  if (cmd.startsWith("plugin:window|is_")) return false;
  if (cmd === "plugin:dialog|open" || cmd === "plugin:dialog|save") return null;
  return null;
}
