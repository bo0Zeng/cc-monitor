// @vitest-environment node
/**
 * 产品 TS 里「某件事只一个口」那几条由 eslint 守（`eslint.config.js` 的 `singleDoors` 表：`no-restricted-syntax` ·
 * `no-restricted-imports`，每项带它的家），`eslint .` 基线 0 ⇒ 别处写一处就红（`eslint-baseline.vitest.ts` 与 CI 那一步接）。
 * 本文件只证每一项真的会抓人、家里真的放过：用仓里那份真配置，往一份真文件的原文后面补一段（`lintText`，路径落在那份真文件上，
 * 不往盘上写）。
 *
 * 买不到：拆开拼（`"tmux new-" + "session -d"`）· 转一手再比（`const L = LOCAL_ORIGIN; x === L`）· 经 `any` 拼出来的
 * `JSON["pa" + "rse"]`；按语法树认，注释天然不算。
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { ESLint } from "eslint";
import { describe, expect, it } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { WALL_CEILING_MS } from "../../test-support/wall-ceiling";

const UI = join(REPO_ROOT, "src", "frontend", "ui");
const eslint = new ESLint({ cwd: REPO_ROOT });
const RULES = new Set(["no-restricted-syntax", "no-restricted-imports"]);

/** 那份真文件原文后面补 `extra`，照它的真路径 lint，回这两条规则报的话（去重）。 */
async function doorsHit(file: string, extra = ""): Promise<string[]> {
  const text = readFileSync(file, "utf8") + "\n" + extra + "\n";
  const [result] = await eslint.lintText(text, { filePath: file });
  const fatal = result.messages.filter((m) => m.fatal).map((m) => `(解析错) ${m.message}`);
  return [...new Set([...fatal, ...result.messages.filter((m) => RULES.has(m.ruleId ?? "")).map((m) => m.message)])];
}

const LEAF = join(UI, "last-seen.ts");
const COMMS = join(REPO_ROOT, "src", "comms", "inward", "chan.ts");
const fn = (body: string) => `export function _lintProbe(o: string, r: any): unknown { ${body} }`;

/** [说明, 文件, 补的那一段, 报错里该有的一截；null ⇒ 一句都不该报]。 */
const CASES: ReadonlyArray<readonly [string, string, string, string | null]> = [
  ["记录格式：链上的 parentUuid（读属性）", LEAF, fn("return r.parentUuid;"), "某一家的记录格式"],
  ["记录格式：stop_reason 作对象格", LEAF, fn("return { stop_reason: 1 };"), "某一家的记录格式"],
  ["记录格式：串里的 <synthetic>", LEAF, fn('return o === "<synthetic>";'), "某一家的记录格式"],
  ["记录格式：按会话文件名猜家 rollout-", LEAF, fn("return `rollout-${o}`;"), "某一家的记录格式"],
  ["记录格式：queue-operation 整串", LEAF, fn('return o === "queue-operation";'), "某一家的记录格式"],
  ["记录格式：「当前那一家」AGENT_PROFILE", LEAF, fn("return AGENT_PROFILE;"), "某一家的记录格式"],
  ["记录格式：画像表 AGENT_PROFILE_TABLE 不算", LEAF, fn("return AGENT_PROFILE_TABLE;"), null],
  ["记录格式：注释里写不算", LEAF, "// parentUuid · <synthetic> · ACTIVE_AGENT", null],
  ["起法意图里带启动命令", LEAF, fn("return { launcherOverride: o };"), "prefs.json"],
  ["JSON.parse 在别处", LEAF, fn("return JSON.parse(o);"), "JSON.parse"],
  ["JSON.parse 在线上字节的家", join(UI, "ipc", "decode.ts"), fn("return JSON.parse(o);"), null],
  ["JSON.parse 在本地存储的家", join(UI, "local-storage.ts"), fn("return JSON.parse(o);"), null],
  ["别处引 changedItems", LEAF, 'import { changedItems as ci } from "./changed-stream";\nexport const _ci = ci;', "changedItems"],
  ["视图引 bindEvents", join(UI, "views", "command-bar.ts"), 'import { bindEvents } from "../events.ts";\nexport const _b = bindEvents;', "bindEvents"],
  ["主窗口引 bindEvents", join(UI, "main.ts"), 'export { bindEvents as _b } from "./events";', null],
  ["串里拼 &&", LEAF, fn('return "cd x && exec y";'), "shell 串"],
  ["模板串里拼 tmux 命令（-u 可有可无）", LEAF, fn("return `tmux -u attach -t ${o}`;"), "shell 串"],
  ["界面以外的产品 TS 同样不拼", COMMS, fn("return `tmux send-keys -t ${o} Enter`;"), "shell 串"],
  ["代码里的 a && b 不算 · 给人看的话不算", LEAF, fn('return o && r ? "已拉起 tmux attach" : /[\'"]/.test(o) && "x";'), null],
  ["TS 起会话兜底一族的名字", LEAF, fn("return posixQuote(o);"), "兜底"],
  ["别处发 session-open", LEAF, fn('return callTyped(o, "session-open", r);'), "session-open.ts"],
  ["session-open 在它的家里发", join(UI, "session-open.ts"), fn('return callTyped(o, "session-open", r);'), null],
  ["别处问 sessions-where", LEAF, fn('return callTyped(o, "sessions-where", r);'), "sessions-where.ts"],
  ["界面拿 tmux 名单自己判", LEAF, fn("return listTmux(o);"), "自己判"],
  ["界面判起会话挑哪个号", LEAF, fn("return withAccount(o, null, r);"), "自己判"],
  ["线上 named 从别处出", LEAF, fn('return { kind: "named", name: o };'), "launch-account.ts"],
  ["线上 named 从它的家出", join(UI, "launch-account.ts"), fn('return { kind: "named", name: o };'), null],
  ["直比 LOCAL_ORIGIN", LEAF, fn("return o === LOCAL_ORIGIN;"), "是不是本机"],
  ["反着写、比 <local> 字面量", LEAF, fn('return "<local>" !== o;'), "是不是本机"],
  ["经属性访问比", LEAF, fn("return o == r.LOCAL_ORIGIN;"), "是不是本机"],
  ["case 上的 LOCAL_ORIGIN", LEAF, fn("switch (o) { case LOCAL_ORIGIN: return 1; default: return 0; }"), "是不是本机"],
  ["别名导入 LOCAL_ORIGIN", LEAF, 'import { LOCAL_ORIGIN as HERE } from "./ipc/origin";\nexport const _h = HERE;', "不起别名导入"],
  ["LOCAL_ORIGIN 当参数传不算", LEAF, fn("return r(LOCAL_ORIGIN, o);"), null],
  ["比本机在 origin.ts 里", join(UI, "ipc", "origin.ts"), fn("return o === LOCAL_ORIGIN;"), null],
  ["界面写会话状态转移", LEAF, fn("return nextState(o, r);"), "状态转移"],
  ["界面攒判活那一轴（串里也算）", LEAF, fn('return r["liveness"];'), "状态转移"],
  ["alert 当错误反馈", LEAF, fn('alert("失败了"); return 0;'), "alert"],
  ["window.alert 同样", LEAF, fn('window.alert("失败了"); return 0;'), "alert"],
  ["confirm 问要不要干，不算", LEAF, fn('return confirm("删掉？");'), null],
  ["在跑只读一格", LEAF, fn("return r.life.is;"), "在跑"],
  ["在跑只读一格（可缺链 · 下标）", LEAF, fn('return r?.life?.is ?? r.life["is"];'), "在跑"],
  ["在跑那一格在它的家里读", join(UI, "face-can.ts"), fn("return r.life.is;"), null],
  ["拿状态点推说不清", LEAF, fn('return r.dot === "unknown" ? 1 : 0;'), "状态点"],
  ["状态点当一格比、不推三元不算", LEAF, fn('return r.dot === "unknown";'), null],
  ["别处挂对话框角色", LEAF, fn('r.setAttribute("role", "alertdialog"); return 0;'), "kit/"],
  ["别处挂菜单项角色", LEAF, fn('r.setAttribute("role", "menuitemradio"); return 0;'), "kit/"],
  ["别处挂悬停提示角色（属性赋值）", LEAF, fn('r.role = "tooltip"; return 0;'), "kit/"],
  ["别处挂模态标记", LEAF, fn('r.setAttribute("aria-modal", "true"); return 0;'), "kit/"],
  ["别处建 toast 栈", LEAF, fn('r.className = "toast-stack"; return 0;'), "kit/"],
  ["别处在 document 上挂点外面关", LEAF, fn('document.addEventListener("pointerdown", r); return 0;'), "kit/"],
  ["界面以外的产品 TS 同样不建对话框", COMMS, fn('r.setAttribute("role", "dialog"); return 0;'), "kit/"],
  ["挂别的角色不算", LEAF, fn('r.setAttribute("role", "status"); return 0;'), null],
  ["对话框在 kit 里建", join(UI, "kit", "menu.ts"), fn('r.setAttribute("role", "menu"); document.addEventListener("pointerdown", r); return 0;'), null],
  ["设置页那套按钮类", LEAF, fn(`r.className = "${["settings", "btn"].join("-")} x"; return 0;`), "kit/button.ts"],
  ["本地 el() 函数声明", LEAF, fn("function el(t: string) { return t; } return el(o);"), "kit/el.ts"],
  ["本地 el() 箭头函数", LEAF, fn("const el = (t: string) => t; return el(o);"), "kit/el.ts"],
  ["叫 el 的变量不是建元素的函数，不算", LEAF, fn("const el = document.activeElement; return el;"), null],
  ["别处读 loggedIn", LEAF, fn("return r.loggedIn;"), "selectable"],
  ["别处按串读 authReady", LEAF, fn('return r["authReady"];'), "selectable"],
  ["别处解构 authKind", LEAF, fn("const { authKind: k } = r; return k;"), "selectable"],
  ["鉴权格在 accounts.ts 里", join(UI, "accounts.ts"), fn("return r.authKind;"), null],
];

describe("「只一个口」那几条由 eslint 守", () => {
  it("原样零报（下面几条报的是补上去的那一段）", async () => {
    for (const f of new Set(CASES.map((c) => c[1]))) expect(await doorsHit(f), f).toEqual([]);
  }, WALL_CEILING_MS);

  for (const [what, file, extra, want] of CASES) {
    it(`${what} ⇒ ${want === null ? "不报" : `报「…${want}…」`}`, async () => {
      const got = await doorsHit(file, extra);
      if (want === null) expect(got).toEqual([]);
      else {
        expect(got.length, `一句都没报：${extra}`).toBeGreaterThan(0);
        expect(got.every((m) => m.includes(want)), got.join("\n")).toBe(true);
      }
    }, WALL_CEILING_MS);
  }
});
