/**
 * 页里的假适配层：站在壳的位置答产品代码发出的 Tauri 命令（`load_config` …）、帧命令（`chan_call` 里的 op）与订阅（`chan_subscribe`）。
 * `REAL_OPS` 那几条与 `SHELL_KINDS` 那几种订阅原样交给壳的真代码（`../real/pool.mjs` 起的无头壳，后面每台机器一个真后端，读 `world.disk` 那几份原始文件）；
 * 其余的还是场景合成的。
 * 答不上的一律记进 `unhandled`（工具最后列出来），并按真后端「不认」那一形拒绝 —— 不悄悄给空。
 */
import { Refuse, type World } from "./types";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { diskOf } from "./world";

/**
 * 交给真后端答的帧命令（`real/pool.mjs` 每台机器起的那一个，读 `world.disk` 那几份原始文件）。场景不许再替它们写答（`world.ops` 里有就报）。
 * 其余的帧命令还是场景合成的；`tests/shots/fake/` 里不许写核心成品的字与判定（判据 `tests/shots/fake-no-core.vitest.ts`）。
 */
export const REAL_OPS = new Set([
  "accounts-list",
  "profiles-read",
  "profiles-resolve",
  "profiles-impact",
  "profiles-bases",
  "profiles-write",
  "quota-read",
  "rotation-rules-read",
  "rotation-session-read",
  "rotation-session-set",
  "rotation-rule-save",
  "rotation-plan",
  "history-page",
  "history-lines",
  "history-index",
  "history-user-inputs",
  "history-turns",
  "history-facts",
  "history-branch",
  "history-run",
]);

/**
 * 交给无头壳的订阅（壳的 `event_replay` 交格，那几台后端推）：会话流（`session-lines` 与独立窗口的 `session-lines/<sid>`：
 * 后端读盘上的原始记录 ＋ pidfile ＋ 私有 tmux 出宣告 · 行 · 红绿灯 · 容器 · 子运行 · 已结束 / 可重连，壳的会话账攒批、重放）· 那三种 `changed/<topic>`。
 * 其余几种还是这里合成：`session-tap`（中转抄出来的事件，台架没有真流量，场景自己推）· 其余几种 `changed/<topic>`（场景演换号那一下自己推，`pushChanged`；
 * 换号那几条帧命令还是合成的）· 终端画面。看不见 / 被关掉的那几台（`unseenMachines` · `closedMachines`）是连接那一跳的事，照旧这里演。
 */
export const SHELL_KINDS = new Set(["session-lines", "changed/accounts", "changed/profiles", "changed/tasks"]);
const shellKind = (kind: string): boolean => SHELL_KINDS.has(kind) || kind.startsWith("session-lines/");

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
  /** 交给无头壳的那几条订阅：编号 → 哪台、哪一种。 */
  private readonly shellSubs = new Map<number, Sub>();
  /** 开页之后台架还在那几台上演的事（已结束的收场 · 断了）：截图等它们都落定（[`settled`]）。 */
  private readonly pending: Promise<unknown>[] = [];
  /** 收到过清单的那几台（每台只演一次）。 */
  private readonly listedOrigins = new Set<string>();
  private screenSeq = 0;

  /** 真后端那一屋子起好了（`/__ccm/world` 答了）。 */
  private readonly ready: Promise<void>;
  private readonly key = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

  constructor(readonly world: World) {
    for (const op of REAL_OPS) {
      if (op in world.ops && !Object.values(WINDOWS_ONLY).some((x) => x.has(op))) this.miss(`场景替真后端答了 ${op}（改写 world.disk）`);
    }
    const machines = diskOf(world);
    this.ready = fetch("/__ccm/world", { method: "POST", body: JSON.stringify({ key: this.key, machines, now: Date.now() }) }).then(async (r) => {
      if (!r.ok) throw new Error(`真后端起不来：${await r.text()}`);
      // 无头壳交的格（同壳交给页的 `chan-items` 事件体），原样当事件发。
      new EventSource(`/__ccm/items?key=${encodeURIComponent(this.key)}`).onmessage = (m) => {
        const d = JSON.parse(m.data as string) as { sub: number; items: { t: string; body?: string }[] };
        this.watch(d.sub, d.items);
        void this.emit("chan-items", d);
      };
    });
  }

  /** 交给壳 `chan_call` 那一跳：成 ⇒ 原样字节；不成 ⇒ 壳交给页的那一形（`{err, body, detail}`，原样抛）。 */
  private async real(origin: string, op: string, req: Record<string, unknown>, view: unknown): Promise<ArrayBuffer> {
    await this.ready;
    const r = await fetch("/__ccm/call", { method: "POST", body: JSON.stringify({ key: this.key, origin, op, args: req, view: view ?? null }) });
    const f = (await r.json()) as { ok: boolean; body?: string; fail?: unknown };
    if (f.ok) return enc.encode(f.body ?? "null").buffer;
    throw f.fail;
  }

  /** 场景在开页之后改那台盘上的一份文件（相对家目录；例：存之前配置文件被别处改过）。 */
  async writeFile(origin: string, rel: string, text: string): Promise<void> {
    await this.ready;
    const r = await fetch("/__ccm/write", { method: "POST", body: JSON.stringify({ key: this.key, origin, rel, text }) });
    if (!r.ok) throw new Error(`改不了 ${origin} 上的 ${rel}：${await r.text()}`);
  }

  /** 订阅那几样交给无头壳。 */
  private tell(what: "sub" | "want" | "stop" | "ready" | "kill" | "end", body: Record<string, unknown>): Promise<void> {
    return this.ready.then(async () => {
      const r = await fetch(`/__ccm/${what}`, { method: "POST", body: JSON.stringify({ key: this.key, ...body }) });
      if (!r.ok) throw new Error(`/__ccm/${what}：${await r.text()}`);
    });
  }

  /** 主界面发 `frontend-ready`（就绪点：壳照它重放会话流，带上次所在的 tab）。 */
  frontendReady(payload: { prioritySid?: string | null } | null): void {
    void this.tell("ready", { priority_sid: payload?.prioritySid ?? null });
  }

  /**
   * 那台的会话流报完清单了（主界面那一条）：开页之后才演的那两样这时演 —— 已结束的那几个收场（[`SessionSpec.ended`]）·
   * 断了的那几台过一会儿后端被杀（[`World.droppedMachines`]：壳那一侧照真的那一套作废那台的成品、通道说断了）。
   */
  private watch(id: number, items: { t: string; body?: string }[]): void {
    const sub = this.shellSubs.get(id);
    if (!sub || sub.kind !== "session-lines" || this.listedOrigins.has(sub.origin)) return;
    if (!items.some((it) => it.t === "frame" && it.body?.startsWith('{"listed"'))) return;
    this.listedOrigins.add(sub.origin);
    this.pending.push(this.tell("end", { origin: sub.origin }));
    if (this.world.droppedMachines?.includes(sub.origin)) this.pending.push(new Promise((r) => setTimeout(r, 1500)).then(() => this.tell("kill", { origin: sub.origin })));
  }

  /** 开页之后台架在演的那几样都落定了（截图在这之后量、截）。 */
  async settled(): Promise<void> {
    await this.ready;
    for (let n = 0; n < this.pending.length; n = this.pending.length) await Promise.all(this.pending.slice(n));
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
        if (this.shellSubs.has(Number(args.id))) void this.tell("want", { id: Number(args.id), more: Number(args.more) });
        return null;
      case "chan_stop":
        if (this.shellSubs.delete(Number(args.id))) void this.tell("stop", { id: Number(args.id) });
        return null;
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
      return new Promise((r) => setTimeout(r, delay)).then(() => this.real(origin, op, req, args.view));
    }
    const handler = this.world.ops[op];
    if (!handler) {
      if (!this.world.quiet?.includes(op)) this.miss(`帧命令 ${op}`);
      return Promise.reject({ err: "Unsupported", body: [] });
    }
    try {
      const v = foldByView(op, handler(origin, req, this.world), args.view);
      // 不带声明的那一问（展开那一下按记录 id 取回全文）另有一档延时，让「读取中」那一形截得到。
      const delay = (args.view == null ? this.world.fullFetchDelayMs?.[op] : undefined) ?? this.world.opDelayMs?.[op] ?? 0;
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
    if (shellKind(sub.kind) && !this.world.unseenMachines.includes(sub.origin) && !this.world.closedMachines.includes(sub.origin)) {
      this.shellSubs.set(sub.id, sub);
      void this.tell("sub", { id: sub.id, origin: sub.origin, kind: sub.kind, want: Number(args.want ?? 0) });
      return;
    }
    this.subs.set(sub.id, sub);
    // 订阅登记好之后才交格（与真句柄一样：返回之后格才可能到）。
    setTimeout(() => this.deliver(sub), 0);
  }

  private deliver(sub: Sub): void {
    if (this.world.unseenMachines.includes(sub.origin)) {
      this.send(sub, [{ t: "unseen", idx: 1, tag: "open", why: "Unreachable" }]);
      return;
    }
    if (this.world.closedMachines.includes(sub.origin) && sub.kind.startsWith("session-lines")) {
      this.send(sub, [{ t: "closed_by_peer", body: JSON.stringify({ code: "busy", message: "这台机器上的会话太多，先关掉几个再连" }) }]);
      return;
    }
    if (sub.kind.startsWith("terminal-screen/")) {
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

  private send(sub: Sub, items: unknown[]): void {
    void this.emit("chan-items", { sub: sub.id, items });
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

/**
 * 照出口交的声明裁记录（真后端 `faces/project.rs` 那一处的小抄：只认桌面交的那几种路径 ——
 * `blocks[type=X].Y` · `results.*.Y`）。认不出的路径 ⇒ 抛（宁可这张图截失败，也不交一份没裁的冒充裁过）。
 */
function foldByView(op: string, v: unknown, view: unknown): unknown {
  const paths = (view as { omit?: { record?: string[] } } | null | undefined)?.omit?.record;
  if (!paths || (op !== "history-page" && op !== "history-lines")) return v;
  const lines = (v as { lines?: Array<{ record: Record<string, unknown> }> }).lines ?? [];
  const folded = lines.map((l) => ({ ...l, record: omitCells(l.record, paths) }));
  return { ...(v as object), lines: folded };
}

function omitCells(rec: Record<string, unknown>, paths: string[]): Record<string, unknown> {
  const out: Record<string, unknown> = { ...rec };
  for (const p of paths) {
    const block = /^blocks\[type=([a-z_]+)\]\.(\w+)$/.exec(p);
    const result = /^results\.\*\.(\w+)$/.exec(p);
    if (block && Array.isArray(out.blocks)) {
      out.blocks = (out.blocks as Array<Record<string, unknown>>).map((b) => {
        if (b.type !== block[1]) return b;
        const { [block[2]]: _gone, ...rest } = b;
        return rest;
      });
    } else if (result && out.results && typeof out.results === "object") {
      out.results = Object.fromEntries(
        Object.entries(out.results as Record<string, Record<string, unknown>>).map(([k, r]) => {
          const { [result[1]]: _gone, ...rest } = r;
          return [k, rest];
        }),
      );
    } else if (!block && !result) {
      throw new Error(`shots: 假后端认不出声明里的路径 ${p}`);
    }
  }
  return out;
}

