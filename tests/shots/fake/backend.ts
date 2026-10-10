/**
 * 页里的假适配层：站在壳的位置答产品代码发出的 Tauri 命令（`load_config` …）、帧命令（`chan_call` 里的 op）与订阅（`chan_subscribe`）。
 * `REAL_OPS` 那几条原样转给真后端（`../real/pool.mjs`：每台机器一个，读 `world.disk` 那几份原始文件）；其余的还是场景合成的。
 * 答不上的一律记进 `unhandled`（工具最后列出来），并按真后端「不认」那一形拒绝 —— 不悄悄给空。
 */
import type { SessionStreamFrame } from "../../../src/frontend/ui/generated/SessionStreamFrame";
import { Refuse, type World } from "./types";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { MachineDisk } from "../disk";

/**
 * 交给真后端答的帧命令（`real/pool.mjs` 每台机器起的那一个，读 `world.disk` 那几份原始文件）。场景不许再替它们写答（`world.ops` 里有就报）。
 * 其余的帧命令还是场景合成的；`tests/shots/fake/` 里不许写核心成品的字与判定（判据 `tests/shots/fake-no-core.vitest.ts`）。
 */
export const REAL_OPS = new Set([
  "accounts-list",
  "quota-read",
  "rotation-rules-read",
  "rotation-session-read",
  "rotation-session-set",
  "rotation-rule-save",
  "rotation-plan",
]);

/** 真后端够不着的那几台：只有 Linux 编出来的后端，Windows 那台的 `cfg(windows)` 分支（例：账号库「Windows 不支持多账号」）演不出来。 */
const WINDOWS_ONLY: Record<string, Set<string>> = { "win-laptop": new Set(["accounts-list"]) };

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
  /** 报过「清单报完了」的那几台（壳那一侧「各台都报完」那一拍按机器表算，这里照样算）。 */
  private readonly listedOrigins = new Set<string>();
  private screenSeq = 0;

  /** 真后端那一屋子起好了（`/__ccm/world` 答了）。 */
  private readonly ready: Promise<void>;
  private readonly key = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

  constructor(readonly world: World) {
    for (const op of REAL_OPS) {
      if (op in world.ops && !Object.values(WINDOWS_ONLY).some((x) => x.has(op))) this.miss(`场景替真后端答了 ${op}（改写 world.disk）`);
    }
    const machines: Record<string, MachineDisk> = {};
    for (const m of world.machines) machines[m] = world.disk[m] ?? { files: {}, live: [] };
    this.ready = fetch("/__ccm/world", { method: "POST", body: JSON.stringify({ key: this.key, machines }) }).then(async (r) => {
      if (!r.ok) throw new Error(`真后端起不来：${await r.text()}`);
    });
  }

  /** 问真后端那一台：应答帧原样翻成壳交给页里的那一形（成 ⇒ 字节；拒 ⇒ `{err: "Refused", body, detail}`）。 */
  private async real(origin: string, op: string, req: Record<string, unknown>): Promise<ArrayBuffer> {
    await this.ready;
    const r = await fetch("/__ccm/call", { method: "POST", body: JSON.stringify({ key: this.key, origin, op, args: req }) });
    const f = (await r.json()) as { ok: boolean; data?: unknown; code?: string; message?: string; detail?: string };
    if (f.ok) return enc.encode(JSON.stringify(f.data ?? null)).buffer;
    const body = f.data === undefined ? { code: f.code, message: f.message } : { code: f.code, message: f.message, data: f.data };
    throw { err: "Refused", body: Array.from(enc.encode(JSON.stringify(body))), detail: f.detail };
  }

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
    const raw = args.payload as number[];
    const req = raw.length > 0 ? (JSON.parse(dec.decode(Uint8Array.from(raw))) as Record<string, unknown>) : {};
    if (REAL_OPS.has(op) && !WINDOWS_ONLY[origin]?.has(op)) {
      const delay = this.world.opDelayMs?.[op] ?? 0;
      return new Promise((r) => setTimeout(r, delay)).then(() => this.real(origin, op, req));
    }
    const handler = this.world.ops[op];
    if (!handler) {
      if (!this.world.quiet?.includes(op)) this.miss(`帧命令 ${op}`);
      return Promise.reject({ err: "Unsupported", body: [] });
    }
    try {
      const v = handler(origin, req, this.world);
      const delay = this.world.opDelayMs?.[op] ?? 0;
      const later = delay > 0 ? new Promise((r) => setTimeout(() => r(v), delay)) : Promise.resolve(v);
      return later.then(
        // 照真壳交原始字节（`tauri::ipc::Response` ⇒ 页里拿到 ArrayBuffer），不交数字数组：
        // 长会话整份读那一问有几 MB，数字数组那一形光假后端自己造就占掉页里几百 ms（性能台架量的是产品）
        (value) => enc.encode(JSON.stringify(value)).buffer,
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
    } else if (sub.kind === "session-tap" || sub.kind.startsWith("changed/")) {
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
      s.records.slice(from).forEach((record, i) => {
        frames.push({ line: { session_id: s.sid, cwd: s.cwd, path: `${s.cwd}/${s.sid}.jsonl`, seq: from + i, origin, record } });
      });
      if (s.runs.length > 0) frames.push({ runs: { session_id: s.sid, runs: s.runs, ended: [] } });
      // 字与语气照后端 `wire::activity_cells` / `SessionFate::cells`（同一张文案表的 `beSession.*`）。
      const act = s.activity === null ? (["unclear", "now"] as const) : ({ working: ["working", "now"], needs_you: ["needsYou", "need"], idle: ["idle", "plain"], background_work: ["backgroundWork", "busy"] } as const)[s.activity];
      frames.push({ activity: { session_id: s.sid, activity: s.activity, activity_text: copyText(`beSession.activity.${act[0]}`), activity_tone: act[1], waiting_for: s.waitingFor } });
      if (s.ended) frames.push({ ended: { session_id: s.sid, text: copyText("sessionState.ended.name"), hint: copyText("sessionState.ended.tooltip"), tone: "plain" } });
      else if (s.idle) frames.push({ idle: { session_id: s.sid, text: copyText("sessionState.reconnectable.name"), hint: copyText("sessionState.reconnectable.tooltip"), tone: "plain" } });
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

  /** 场景在开页之后推一格 `changed/<topic>`（格体 `{key?, rev?, body?}`，同壳交的那一份）。 */
  pushChanged(origin: string, topic: string, cell: unknown): void {
    for (const sub of this.subs.values()) {
      if (sub.origin === origin && sub.kind === `changed/${topic}`) this.send(sub, [{ t: "frame", seq: 0, body: JSON.stringify(cell) }]);
    }
  }

  /** 性能台架：往那台所有终端实时画面的订阅上推一屏（`view` 同 `terminal-preview` 成品）。 */
  pushScreen(origin: string, view: unknown): void {
    this.screenSeq += 1;
    for (const sub of this.subs.values()) {
      if (sub.origin === origin && sub.kind.startsWith("terminal-screen/")) this.send(sub, [{ t: "frame", seq: this.screenSeq, body: JSON.stringify({ seq: this.screenSeq + 1, view }) }]);
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
