/**
 * F10（剩余账号 UX）：`account_usage` IPC 的前端包装 + 去抖缓存。
 *
 * 缓存**不是** TTL 轮询——只是"同一次设置窗口/chip 菜单打开期间，别对同一个账号重复戳"的
 * 去抖（探测是较重操作：起隐藏会话+网络查询，几秒到十几秒）。没有 `setInterval`，没有后台
 * 定时任务；`force:true`（用户点"刷新用量"）忽略缓存强制重查。
 */
import { isValidConfigDir } from "./shell-quote.ts";
import { commands } from "./ipc/commands";
// F08：本机那条路的 origin 哨兵（ 的单一真相源，别在这里重定义）。
import { LOCAL_ORIGIN } from "./accounts.ts";

/**
 * `K-R101`/`R59`（2026-09-13）：**两态，没有第三态。**
 *
 * 🔴 生产路上**不再有解析** —— `parseUsageCapture` 已退役（墓碑住
 * `src/account-usage-parse.ts` 头部），本模块**不 import 它**。
 * `captured=true` ⇒ `screen`（那一屏原文，**空屏也算成功**）·
 * `captured=false` ⇒ `probe-failed`（那才是错误态）。
 *
 * ⚠ **`raw` 那个字段还在类型里 ≠ 原文到得了界面** —— 中途被谁丢掉照样是这个类型。
 * 「它真的到了展示那一层」由 [`usageScreenEl`] 与三个消费者的 DOM 判据钉。
 */
export type AccountUsageOutcome =
  | { status: "probe-failed"; error: string }
  | { status: "screen"; raw: string };

/**
 * 那一屏原文旁边那句提示。**`K-R101`/`R59`（2026-09-13）重写 —— 原文已经过时且在说假话。**
 *
 * 原文写着「格式已对照真机 /usage 抓屏验证（2026-07-31）……那种情况会显示成「认不出格式」
 * 并附原始屏」。那句话在 `S10` 时是诚实的，**但 `R59` 之后每一半都不成立**：
 * 生产路上**没有解析了**，也就没有「认不出格式」这个态；而「格式已验证」说的是**解析器**
 * 对那份冻结夹具的行为，与今天这条只抓屏的路**无关**。
 *
 * 今天这句话要说的是**另一件事**，而它是真的：抓早了会抓到半截的屏，
 * **而你自己看得出来**（这正是 `R58` 买到的鲁棒性 —— 把一个证不了的判据「解析得出」
 * 换成一个能证的判据「画面不动了」＋ 一个看得见的兜底「人」）。
 */
// 常量名沿用 `..._UNVERIFIED_...`（三处 import 着）——**只改文案不改名**：
// 改名是纯改动面，而这条提示的语义变化已经写在上面的注释里。
export const OK_USAGE_UNVERIFIED_CAVEAT =
  "这是探针抓到的那一屏**原文**，没有经过任何解析（R59〔用 09-13〕：解析层功能已退役）。" +
  "探针用「画面静止 3s」判定渲染完成，那是预算不是实测值——真 claude 若渲染中途卡顿超过 3s " +
  "会抓早；那种情况你会直接在这一屏上看出来（半截的画面），按「刷新」再抓一次即可。";

/**
 * `R58`〔用 09-13〕逐字「**即点击用量后把屏幕预览给我看**」的**唯一渲染处**。
 *
 * 三个消费者（chip · usage-view · accounts-section）共用这一份 ——
 * `K33`「所有命令只许有一处」在展示这一层的对应物：**原文怎么显示只有一个住址**，
 * 三处各写一份就会漂（其中一处漏掉 `pre-wrap` 就是「原文到了但读不了」）。
 *
 * 🔴 **空屏（`raw === ""`）也要渲染出东西来** —— 那是一次**成功**的抓屏
 * （`captured=true`），把它渲染成一片空白会让用户以为功能坏了。
 */
export function usageScreenEl(raw: string): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "usage-screen";
  const pre = document.createElement("pre");
  pre.className = "usage-screen-raw";
  // ★ `textContent` 而不是 `innerHTML` —— 这是**不受信的会话文本**
  //   （`capability_registry` 头注逐字点名过远端 capture-pane 这一族）。
  pre.textContent = raw;
  pre.title = OK_USAGE_UNVERIFIED_CAVEAT;
  // 等宽 + 保留空白（`R59` 逐字要求）。走内联样式而不是新 CSS 类：
  // 仓里既有惯例（`settings/diagnostics-section.ts:171` 等 149 处内联样式）。
  pre.style.whiteSpace = "pre-wrap";
  pre.style.fontFamily = "var(--font-mono, monospace)";
  pre.style.maxHeight = "40vh";
  pre.style.overflow = "auto";
  pre.style.margin = "6px 0";
  wrap.appendChild(pre);
  if (raw === "") {
    const note = document.createElement("div");
    note.className = "usage-screen-empty";
    note.textContent =
      "抓到的是空屏（0 个字符）——探针跑通了，屏上确实什么都没有。这不是失败；要重试按「刷新」。";
    wrap.appendChild(note);
  }
  return wrap;
}

interface CacheEntry {
  ts: number;
  outcome: AccountUsageOutcome;
}

const cache = new Map<string, CacheEntry>();
const cacheKey = (origin: string, accountName: string): string => `${origin}|${accountName}`;

/**
 * per-account 探测 plan 用量窗口%。`force` 忽略缓存（用户点"刷新用量"时传）。
 *
 * Z03：`configDir` 传 **`null`** = 探**账号 0**（载荷前缀是 `unset CLAUDE_CONFIG_DIR; `）。
 * **别传空串**——那是坏数据，下面的前置校验会 throw（被 catch 成 probe-failed）。
 */
export async function fetchAccountUsage(
  origin: string,
  accountName: string,
  configDir: string | null,
  opts?: { force?: boolean },
): Promise<AccountUsageOutcome> {
  const key = cacheKey(origin, accountName);
  if (!opts?.force) {
    const cached = cache.get(key);
    if (cached) return cached.outcome;
  }

  let outcome: AccountUsageOutcome;
  try {
    // U8c-2a：只报「哪个账号」，载荷由 Rust 内核（`backend::control::payload`）编译。
    // `configDir === null` 就是**账号 0** 的显式表态 —— 原样透给 Rust，不在这里做任何渲染。
    //
    // **configDir 的校验留在 TS，这是纵深防御不是重复**（同 `resolve_query.rs` 的 B2 纪律：
    // 「权威也保留本地校验」）。搬走的是**渲染**，不是**前置条件**：
    //   · 空串是坏数据（空值 ≠ 未设，账号 0 请传 null）；
    //   · 非法 configDir（引号 / 元字符 / 相对路径 / 路径穿越）⇒ **连问都不该问**。
    // Rust 侧也会各自再拒一道（`backend::control::payload::config_dir_command_safe`，而且用的是更严的并集），
    // 但那要多一次 IPC 往返，且既有 6 条测试逐字记着「探测不发起」。
    if (configDir === "") {
      throw new Error("用量探针需要显式 configDir（账号 0 请传 null，空串是坏数据）");
    }
    if (configDir !== null && !isValidConfigDir(configDir)) {
      throw new Error(`非法 CLAUDE_CONFIG_DIR（拒绝发起探测）: ${JSON.stringify(configDir)}`);
    }
    // F08：**本机走本机那条**（补平后两侧都有了）。
    // 🔴 `K-R104`（09-13）订正：上一版这里写「两条路的载荷逐字相同（Rust 侧同一个
    // `probe_command_for`……）—— 这里分的只是『谁去执行』：本机 `sh -c`，远端 SSH exec」。〔散文墓碑〕
    // **那两句今天都假了**：探针编排整条搬上后端帧面之后，Rust 侧两条命令**是同一个函数**，
    // 只差一个 origin（`<local>` 也是一个 origin），而**没有任何一侧再 `sh -c` 或起 SSH exec**。
    // ⇒ 这里分的只剩「IPC 名字」这一格，留着它是因为两条 tauri 命令的签名不同
    // （本机那条不收 origin）。同源那件事由 Rust 侧
    // `account_usage::tests::the_local_and_remote_probes_are_the_same_code_path` 钉住。
    const result =
      origin === LOCAL_ORIGIN
        ? await commands.account_usage_local({ accountName, configDir })
        : await commands.account_usage({ origin, accountName, configDir });
    // 🔴 `R59`：**这一行就是「生产路上零解析」的落点。**
    // `raw ?? ""` 里那个 `""` 不是防御性写法，是 `KR101D1` ③：
    // **抓到空屏也算成功** —— `captured=true` 时无论屏上有什么（包括什么都没有）
    // 都走 `screen`，把空屏判成失败是本条明令禁止的那一形。
    outcome = result.captured
      ? { status: "screen", raw: result.raw ?? "" }
      : { status: "probe-failed", error: result.error ?? "探测失败（原因未知）" };
  } catch (e) {
    outcome = { status: "probe-failed", error: String(e) };
  }

  cache.set(key, { ts: Date.now(), outcome });
  return outcome;
}

/** 设置面板"刷新"（账号列表本身）不该连带清用量缓存——两者语义分开（F10 计划 §5.2）。
 *  这个函数只在用户明确要求"重查用量"时调（如换了账号的登录态之后）。 */
export function invalidateAccountUsageCache(origin?: string, accountName?: string): void {
  if (!origin) {
    cache.clear();
    return;
  }
  if (!accountName) {
    for (const k of [...cache.keys()]) {
      if (k.startsWith(`${origin}|`)) cache.delete(k);
    }
    return;
  }
  cache.delete(cacheKey(origin, accountName));
}
