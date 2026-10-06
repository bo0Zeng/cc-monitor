/**
 * 页里的假后端：答产品代码发出的 Tauri 命令（`load_config` …）、帧命令（`chan_call` 里的 op）与订阅（`chan_subscribe`）。
 * 答不上的一律记进 `unhandled`（工具最后列出来），并按真后端「不认」那一形拒绝 —— 不悄悄给空。
 */
import type { SessionStreamFrame } from "../../../src/frontend/ui/generated/SessionStreamFrame";
import { Refuse, type World } from "./types";

type Emit = (event: string, payload: unknown) => unknown;

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
      return Promise.resolve(v).then(
        (value) => Array.from(enc.encode(JSON.stringify(value))),
        (e: unknown) => Promise.reject(refusal(e)),
      );
    } catch (e) {
      return Promise.reject(refusal(e));
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
    } else if (sub.kind === "session-tap" || sub.kind === "accounts-changed" || sub.kind === "session-tasks" || sub.kind === "quota-changed") {
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
          kind: s.kind,
          attachable: true,
          cwd: s.cwd,
          project_dir: s.cwd,
          name: s.name,
        },
      });
      frames.push({ container: { session_id: s.sid, container: s.container } });
      s.records.forEach((message, seq) => {
        frames.push({ line: { session_id: s.sid, cwd: s.cwd, path: `${s.cwd}/${s.sid}.jsonl`, seq, origin, message } });
      });
      if (s.runs.length > 0) frames.push({ runs: { session_id: s.sid, runs: s.runs, ended: [] } });
      frames.push({ activity: { session_id: s.sid, status: s.status, waiting_for: s.waitingFor } });
      if (s.ended) frames.push({ ended: { session_id: s.sid } });
      else if (s.idle) frames.push({ idle: { session_id: s.sid } });
    }
    frames.push({ listed: { origin } });
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

function refusal(e: unknown): { err: unknown; body: number[] } {
  // 场景要演通道那一跳的失败（够不着 · 期限到）：直接抛线上形状 `{err, body}`，原样交回。
  if (e !== null && typeof e === "object" && "err" in e && "body" in e) return e as { err: unknown; body: number[] };
  if (e instanceof Refuse) {
    const body = e.data === undefined ? { code: e.code, message: e.message } : { code: e.code, message: e.message, data: e.data };
    return { err: "Refused", body: Array.from(enc.encode(JSON.stringify(body))) };
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
