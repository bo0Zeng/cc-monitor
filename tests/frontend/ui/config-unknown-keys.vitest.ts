/**
 * 🔴 **「配置里出现不认识的键」必须出声**，
 * 以及那一刀的由头 —— 落盘键 `forceLegacyLaunchRenderer` → `forceLaunchPayloadRenderer`。
 *
 * # 这一族守的是什么
 *
 * `P12` 逐字：「真正的毛病不是改名，是**未知键静默忽略** —— 那是本仓的头号病形
 *（『关掉了』与『过了』在终端上一模一样）。」
 * 又逐字：「**旧键出现时必须有一条会红的判据**，否则『出声』这件事本身没人守。」
 *
 * ⇒ 本文件就是那条。它**不 mock `src/frontend/ui/config.ts`** —— 只 mock 最外面那层 `ipc/commands`，
 * 于是 `loadConfig` / `getBehavior` / `setBehavior` 走的是**生产那条真链**。
 * （`判据不在执行链上就等于不存在`：把 `config.ts` mock 掉，就等于把被守的东西换成了替身。）
 *
 * # 射程：它盖不到什么
 *
 * - **不判用户那一侧真不真的显示出来**。那条在 `tests/frontend/ui/settings/unknown-keys-notice.vitest.ts`
 *   （真渲染设置面板，去 DOM 里找那个键名）。两条缺一不可：这里证明「数得出来」，
 *   那里证明「说出来了」。
 * - **不盖「一个新模块开始用 config.json 却没登记」**。测试里做目录遍历的文件要逐个登记理由
 *   （`tests/frontend/ui/scanning-guard-registry.vitest.ts` 的 `WALKERS`），
 *   所以下面的对拍是**逐个登记的主人**去核，不是全树扫。
 *   ⚠ 但那种漏登记**不是静默的**：漏了的键会当场被当成未知键、指名喊出来。
 *   吵，但不瞎 —— 失效方向是对的。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync, existsSync } from "node:fs";

const store = vi.hoisted(() => ({
  cfg: {} as Record<string, unknown>,
  /** 每次写完之后盘上的整份。 */
  saved: [] as Record<string, unknown>[],
  /** 每次写交来的补丁（写只交「改哪几条路径」）。 */
  edits: [] as (readonly { op: string; path: string[] }[])[],
}));

vi.mock("../../../src/frontend/ui/ipc/commands", async () => {
  const { applyConfigEdits } = await import("./config-patch-fake");
  return {
    commands: {
      load_config: vi.fn(async () => store.cfg),
      patch_config: vi.fn(async ({ edits }: { edits: Parameters<typeof applyConfigEdits>[1] }) => {
        store.edits.push(edits);
        store.cfg = JSON.parse(applyConfigEdits(JSON.stringify(store.cfg), edits)) as Record<string, unknown>;
        store.saved.push(store.cfg);
      }),
    },
  };
});

import {
  CONFIG_KEY_OWNERS,
  KNOWN_CONFIG_KEYS,
  loadConfig,
  unknownConfigKeys,
  unknownKeysIn,
  __resetUnknownConfigKeysForTests,
} from "../../../src/frontend/ui/config";
import { getBehavior, setBehavior } from "../../../src/frontend/ui/behavior";

/** 退役的那个旧名字。**逐字**写在这里 —— 本条判据的全部意义就是它出现时会红。 */
const RETIRED_KEY = "forceLegacyLaunchRenderer";
/** 它改名之后的名字也退役了（整个逃生口删了，理由在 `src/frontend/ui/behavior.ts`）⇒ 同样是未知键。 */
const RETIRED_KEY_2 = "forceLaunchPayloadRenderer";

const sorted = (xs: readonly string[]): string[] => [...xs].sort();

/**
 * 🔴 **config.json 顶层键的第二份普查 —— 刻意手写，刻意不从 `config.ts` 派生。**
 *
 * 它存在的唯一理由是**不让那条相等变成恒真**：如果这一份也从 `KNOWN_CONFIG_KEYS`
 * 生成，那么「登记表缩水」时两边会一起缩，判据照样绿 ——
 * 本工作区吃过这个亏（「恒等两侧同源会恒真」）。
 *
 * 这一份的来源是**现打的反向全扫**（2026-09-21）：`src/` 下每个 `import … from "./config"`
 * 的模块，逐个读它对 `cfg[…]` / `cfg.…` 的顶层存取。Rust 侧另读三个
 *（`claudeDir` / `showBgSessions` / `remote`），都已在列。
 */
const CENSUS: readonly string[] = [
  "theme", // src/frontend/ui/theme.ts
  "claudeDir", // src/frontend/ui/paths.ts
  "keybindings", // src/frontend/ui/keybindings/store.ts
  "accounts", // src/frontend/ui/accounts.ts
  // 〔条 66〕`backendPolicy` 退役（值搬到后端那台机器上）⇒ 这一行删掉，两份普查恒等地各少一键。
  "remote", // src/frontend/ui/remote-config.ts
  "tabBar", // src/frontend/ui/tab-bar-state.ts
  "tabCollections", // src/frontend/ui/tab-collections.ts
  "contextLimits", // src/frontend/ui/usage-hud.ts（只读）
  "autoFollowUserActive", // ── src/frontend/ui/behavior.ts 那一族 ──
  "bringMonitorToFrontOnUserActive",
  "showBgSessions",
  "resumeCommand",
  "localResumeCommand",
  "terminal", // src/frontend/ui/settings/terminal-row.ts（壳开终端时只读）
  "resumeCommandPresets",
  "resumeInTmux",
  "notifyTurnEnd",
  "notifyNeeds",
  // `forceLaunchPayloadRenderer` 退役 ⇒ 这一行删，两份普查恒等地各少一键。
  // 现打反扫 Rust 侧补一个：`src/frontend/shell/src/logging.rs::write_diagnostics_to_config` 写的 `diagnostics`。
  //   上面那句「Rust 侧另读三个」漏了它 ⇒ 存过一次诊断设置的用户，设置页「认不出的键」提示条会把它点名（假警报）。
  "diagnostics",
];

beforeEach(() => {
  store.cfg = {};
  store.saved = [];
  store.edits = [];
  __resetUnknownConfigKeysForTests();
});

describe("P12 ① 那个逃生口的两代落盘键都退役了（`forceLegacyLaunchRenderer` → `forceLaunchPayloadRenderer` → 删）", () => {
  it("★★ 两代旧名字都**不留别名**：盘上写 true 也不驱动任何行为（行为配置里根本没有这一格）", async () => {
    store.cfg = { [RETIRED_KEY]: true, [RETIRED_KEY_2]: true };
    const b = (await getBehavior()) as unknown as Record<string, unknown>;
    expect(
      [RETIRED_KEY, RETIRED_KEY_2].filter((k) => k in b),
      "退役键还在被当成开关读 —— `no-legacy-compat` 要的是**退役**，不是留一个没人管的开关。",
    ).toEqual([]);
  });

  it("★★ 写盘**只写行为那一族**（两向集合相等），两代退役键都不写", async () => {
    const before = await getBehavior();
    await setBehavior(before);
    expect(store.edits.length, "`setBehavior` 根本没写盘").toBe(1);
    // 看它**交了哪几条路径**（不是写完之后盘上有什么 —— 那里还有别人的键）。
    const written = sorted(store.edits[0]!.map((e) => e.path.join(".")));
    const owned = sorted(
      KNOWN_CONFIG_KEYS.filter((k) => CONFIG_KEY_OWNERS[k] === "src/frontend/ui/behavior.ts"),
    );
    expect(
      written,
      "`setBehavior` 写下的顶层键 ≠ 登记给 `src/frontend/ui/behavior.ts` 的那一族。\n" +
        "两个方向都要看：多写了 = 它在动不属于自己的键；少写了 = 登记表里有个键没人写，" +
        "而那种键会在下次读盘时被当成未知键喊出来。",
    ).toEqual(owned);
    for (const k of [RETIRED_KEY, RETIRED_KEY_2]) {
      expect(written, `写盘里还有退役键 \`${k}\` —— 退役只退了读的那一半`).not.toContain(k);
    }
  });

  it("写盘**不吞掉**用户手写的未知键（未知 ≠ 该删）", async () => {
    store.cfg = { [RETIRED_KEY]: true, 某个错字: 1 };
    await setBehavior(await getBehavior());
    expect(
      Object.keys(store.saved[0]),
      "保存设置时把认不出的键顺手删了 —— 用户手写的东西被我们悄悄吃掉，" +
        "这比静默忽略更糟：连让他自己发现的机会都没有了",
    ).toContain(RETIRED_KEY);
  });
});

describe("P12 ② 未知键要被数出来（`unknownKeysIn` / `loadConfig` 那一格）", () => {
  it("🔴 旧键出现 ⇒ 数得出来，而且逐字就是它（两代退役键都算）", () => {
    expect(
      unknownKeysIn({ [RETIRED_KEY]: true, theme: {}, [RETIRED_KEY_2]: false }),
      `盘上放了退役键而这里没全数出来 —— 「静默忽略」原样还在`,
    ).toEqual([RETIRED_KEY, RETIRED_KEY_2]);
  });

  it("🔴 旧键出现 ⇒ **读盘那一步**就记下来了（在执行链上，不是纯函数自娱自乐）", async () => {
    store.cfg = { theme: {}, [RETIRED_KEY]: true };
    await loadConfig();
    expect(
      unknownConfigKeys(),
      "`loadConfig()` 读完之后那格快照是空的 —— 提示条到时候会无话可说，" +
        "而「没话说」与「配置很干净」在屏幕上一模一样",
    ).toEqual([RETIRED_KEY]);
  });

  it("★ 反空真正控：把 [`CENSUS`] 那 18 个键整份喂进去 ⇒ 一个未知键都不许报", () => {
    const full: Record<string, unknown> = {};
    for (const k of CENSUS) full[k] = true;
    expect(
      unknownKeysIn(full),
      "真实用户会写的那 18 个键里，有的被当成了未知键 —— 这把尺子会对着正常配置喊，" +
        "而一把乱喊的尺子用不了几天就会被关掉",
    ).toEqual([]);
  });

  it("★ 空配置 / 非对象都不报（那不是「多写了一个键」）", () => {
    expect(unknownKeysIn({})).toEqual([]);
    expect(unknownKeysIn(null)).toEqual([]);
    expect(unknownKeysIn([1, 2])).toEqual([]);
    expect(unknownKeysIn(7)).toEqual([]);
  });

  it("多个未知键按**盘上的顺序**全报，不截断", () => {
    expect(
      unknownKeysIn({ zz: 1, theme: {}, aa: 2, bb: 3 }),
      "少报了一个 —— 用户改完被报出来的那个，剩下的仍然在静默失效",
    ).toEqual(["zz", "aa", "bb"]);
  });
});

describe("P12 ③ 登记表自己得是真的（否则上面每一条都在拿一张假名单对拍）", () => {
  it("★★ 登记表 == [`CENSUS`]（两向集合相等，**两份各自手写**）", () => {
    expect(
      sorted(KNOWN_CONFIG_KEYS),
      "`src/frontend/ui/config.ts` 的登记表与本文件这份独立普查对不上。\n" +
        "少了 ⇒ 那个键从今天起会被当成未知键，对着一份正常配置喊；\n" +
        "多了 ⇒ 表上挂着一个谁都不写也不读的名字，而它会让真正的错字蒙混过去。\n" +
        "⚠ 这两份**必须各自手写**：从同一个来源派生出来的「相等」恒真，" +
        "而恒真与真绿在终端上一模一样。",
    ).toEqual(sorted(CENSUS));
  });

  it("分母：登记了多少个主人", () => {
    const owners = new Set(Object.values(CONFIG_KEY_OWNERS));
    // 〔条 66〕10 → 9：`src/frontend/ui/backend-policy.ts` 不再是任何配置键的主人（`backendPolicy` 退役，
    //   「退出行为」那个值搬到后端所在那台机器上）。少的就是它这一个，别的主人一个没动。
    // 9 → 10：补上 `src/frontend/shell/src/logging.rs`（`diagnostics` 那一键的主人，Rust 写的）。多的就是它这一个。
    // 10 → 11：补上 `src/frontend/ui/local-machine-prefs.ts`（本机那一格恢复命令覆盖 `localResumeCommand` 的主人）。
    // 11 → 12：`terminal` 那一键（设置 → 通用 → 恢复 → 终端，`src/frontend/ui/settings/terminal-row.ts`）。
    expect(owners.size, `主人 ${owners.size} 个（现打 12）`).toBe(12);
  });

  it("★ 每个登记的主人文件真的在盘上，而且那个键名逐字出现在它里面", () => {
    const missing: string[] = [];
    for (const [key, owner] of Object.entries(CONFIG_KEY_OWNERS)) {
      if (!existsSync(owner)) {
        missing.push(`${key} → ${owner}（文件不在）`);
        continue;
      }
      // ⚠ 参数是变量不是字面量 —— 本仓 `scanning-guard-registry` 数的是
      //   「磁盘语料上的裸 `.includes("…")`」，那一格是棘轮且已顶格。
      if (!readFileSync(owner, "utf8").includes(key)) {
        missing.push(`${key} → ${owner}（文件里找不到这个键名）`);
      }
    }
    expect(
      missing,
      "登记表指着一个不存在的住址，或者主人那边已经把键名改了而表没跟上。\n" +
        "后果是实的：表里那个名字从此**没人写也没人读**，而用户盘上真正的那个键\n" +
        "会被当成未知键喊出来 —— 喊的是对的，但喊的理由是我们自己的表烂了。\n" +
        `逐条：\n${missing.join("\n")}`,
    ).toEqual([]);
  });

  it("★★ `src/frontend/ui/behavior.ts` 那一族：登记表 == 它自己 `const KEY_… = \"…\"` 的全集（两向）", () => {
    const src = readFileSync("src/frontend/ui/behavior.ts", "utf8");
    const declared = sorted([...src.matchAll(/const KEY_\w+ = "([^"]+)";/g)].map((m) => m[1]));
    const registered = sorted(
      KNOWN_CONFIG_KEYS.filter((k) => CONFIG_KEY_OWNERS[k] === "src/frontend/ui/behavior.ts"),
    );
    expect(declared.length, "一个 `const KEY_… =` 都没抽出来 —— 抽取器坏了，下面那条会空绿").toBeGreaterThan(0);
    expect(
      declared,
      "`behavior.ts` 声明的落盘键与 `config.ts` 登记的那一族对不上。\n" +
        "两个方向都要看：那边加了个键没登记 ⇒ 它自己会被当未知键喊；\n" +
        "这边登记了那边没有 ⇒ 表上挂着一个谁都不认的名字。",
    ).toEqual(registered);
  });
});
