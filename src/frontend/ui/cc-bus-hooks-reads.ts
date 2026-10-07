/**
 * cc-bus 那两条钩子在一台机器上装了没有（只读）：那台后端读它自己的 agent 配置里的 `settings.json`、出整份成品
 * （帧命令 `hooks-diag`，本机远端同一条 `chan.call(origin, …)`）。界面只按形状收、照态画；不读、不写那份文件。
 *
 * 成品的线上形状由跨语言金样 `tests/__fixtures__/hooks-diag.golden.json` 钉住（后端产出 == 金样 · {@link decodeHooksReport} 读同一份）。
 * `HookState` 是后端 `#[serde(tag = "kind", rename_all = "kebab-case")]` 的内部标记枚举 → 判别联合。
 */
import type { Origin } from "./ipc/origin";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, ReplyUnreadable, saidFrom } from "./ipc/chan-caller";
import { copyText } from "./copy-table";
import { exactKeys, isObj } from "./ipc/decode";

export type HookState =
  | { kind: "not-installed" }
  | { kind: "installed-via-path"; command: string }
  | { kind: "installed-at-path"; command: string; path: string }
  | { kind: "path-missing"; command: string; path: string }
  | { kind: "unknown"; command: string };
export type HooksDiagnosis = { session_start: HookState; stop: HookState; note: string };
/** `supported` = 那台跑得了 cc-bus（要 tmux）；`snippet` = 要合并进那份文件的内容（两条钩子直接指向那台装好的 cc-bus 里的脚本）；
 *  那台的 cc-bus 没装 / 跑不了 ⇒ `null`。 */
export type HooksReport = { supported: boolean; diagnosis: HooksDiagnosis; snippet: string | null; source: string };

/** 读一份 `settings.json` ＋ 几次 stat ＋ 回程。 */
const HOOKS_DIAG_BUDGET_MS = 30_000;

/**
 * `hooks-diag` 的成品 → {@link HooksReport}。**按形状严格收，不解释**：多一格 / 缺一格 / 类型不对 ⇒ 抛（两端契约对不上）。
 * 态的取值集只收已知五种：后端将来加第六态 ⇒ 这里抛「对不上」（不猜成哪一态）。
 */
export function decodeHooksReport(v: unknown): HooksReport {
  const bad = (what: string): never => {
    throw new ReplyUnreadable(`hooks-diag reply shape mismatch: ${what}`);
  };
  const obj = (x: unknown, what: string, keys: string[]): Record<string, unknown> => {
    if (!isObj(x)) return bad(`${what} is not an object`);
    if (!exactKeys(x, keys)) return bad(`${what} has keys ${Object.keys(x).join(",")}`);
    return x;
  };
  const str = (x: unknown, what: string): string => (typeof x === "string" ? x : bad(`${what} is not a string`));
  const state = (x: unknown, what: string): HookState => {
    const kind = (x as { kind?: unknown } | null)?.kind;
    switch (kind) {
      case "not-installed":
        obj(x, what, ["kind"]);
        return { kind };
      case "installed-via-path":
      case "unknown": {
        const o = obj(x, what, ["kind", "command"]);
        return { kind, command: str(o.command, `${what}.command`) };
      }
      case "installed-at-path":
      case "path-missing": {
        const o = obj(x, what, ["kind", "command", "path"]);
        return { kind, command: str(o.command, `${what}.command`), path: str(o.path, `${what}.path`) };
      }
      default:
        return bad(`${what}.kind is ${JSON.stringify(kind)}`);
    }
  };
  const top = obj(v, "reply", ["supported", "diagnosis", "snippet", "source"]);
  if (typeof top.supported !== "boolean") return bad("supported is not a boolean");
  const d = obj(top.diagnosis, "diagnosis", ["session_start", "stop", "note"]);
  return {
    supported: top.supported,
    diagnosis: {
      session_start: state(d.session_start, "session_start"),
      stop: state(d.stop, "stop"),
      note: str(d.note, "note"),
    },
    snippet: top.snippet === null ? null : str(top.snippet, "snippet"),
    source: str(top.source, "source"),
  };
}

/** 问 `origin` 那台后端要一份成品（本机逐字 `LOCAL_ORIGIN`）。问不出来 ⇒ 抛（带那台的原话）。 */
export async function fetchHooksReport(origin: Origin): Promise<HooksReport> {
  try {
    const body = jsonBody({});
    const budget = budgetWithin(HOOKS_DIAG_BUDGET_MS);
    return decodeHooksReport(readJson(await chan.call(origin, "hooks-diag", body, budget)));
  } catch (e) {
    throw new Error(saidFrom(e, origin));
  }
}

/** 一态 → 那句话 ＋ 语气。**`path-missing` 绝不能说成「已装」**；`unknown` 要中性（既不说已装也不说未装）。 */
export function describeState(st: HookState): { text: string; tone: "ok" | "bad" | "unknown" } {
  switch (st.kind) {
    case "not-installed":
      return { text: copyText("ccBusHooks.state.missing"), tone: "bad" };
    case "installed-via-path":
      return { text: copyText("ccBusHooks.state.onPath"), tone: "ok" };
    case "installed-at-path":
      return { text: copyText("ccBusHooks.state.explicit", { path: st.path }), tone: "ok" };
    case "path-missing":
      return { text: copyText("ccBusHooks.state.brokenPath", { path: st.path }), tone: "bad" };
    case "unknown":
      // 命令里出现了目标程序，但它包在 `sh -c` / `env` / `timeout` 之类里：猜「未装」和猜「已装」一样是猜。
      return { text: copyText("ccBusHooks.state.indirect", { command: st.command }), tone: "unknown" };
  }
}
