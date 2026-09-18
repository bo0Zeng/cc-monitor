/**
 * K-R45 · `KR45D1`：把一个会话里「用户说过的每一句」挑出来，供「点一下跳过去」用。
 *
 * 〔用 09-10 逐字〕「**那个按照输入跳转是不是还没做? 即像是 deepseek 网页端一样可以跳转到
 * 某次用户输入**」「**要的就是跳过去就行**」。
 *
 * ## 为什么单独一个文件，而不是写进 `session-viewer.ts`
 *
 * `KR45D2`（乙 · 实时窗口）要做**同一件事**，而件里写死了「不许把甲的实现照抄一份过来 ——
 * 这个仓一整天在治的正是『一个东西两个住址』」。挑句子这一半是**纯函数、不碰 DOM、
 * 不碰渲染路子**，两条路的差别全在「怎么把它列出来 / 怎么跳」那一半。
 * ⇒ 先把可共用的那一半单独放好，乙那一拍直接 import，不用重写一份判定。
 *
 * 🔴 **订正（09-10 第二轮现打，件 `§5.2`）**：上面那句「乙直接 import 就行」**只对了一半**。
 * 这个**函数**确实共用得了；共用不了的是**它的入参** —— 本函数要的是「每条记录的 `message`」，
 * 而实时窗口那条路上**没有全量记录账本**：`tabs.ts` 的 `Tab` 31 个字段里，
 * 持有整条 payload 的只有 `window: TailWindow`（只收**没渲染**的那些，`takeTail` 一取就
 * `splice` 出账）与 `midBatchBuffer`（每批 flush 后置空）；已渲染那侧的 `RecordTimeline`
 * 条目是 `{seq, element, kind, toolGroup}`，**没有 `message`**。
 * ⇒ 一条已经上屏的 live 记录，它的文本在前端**零处留存**。乙那一拍要先在 `tabs.ts` 里
 * 立一个今天不存在的账本 —— 那不是「共用」，是新建。读数与分母全在件的 `§5.2`。
 *
 * ## 🔴 口径（分母写死在这里，判据的名字里也要带着它）
 *
 * 「一条用户输入」= 同时满足四条的 jsonl 记录：
 *   1. `type === "user"`
 *   2. `isMeta !== true` —— Claude Code 注入的 skill/command 展开 prompt、system-reminder、
 *      caveat 都带 `isMeta`，不是用户敲进去的（口径同 `cards/index.ts` 的 `rec.isMeta` 那一支）
 *   3. `isSidechain !== true` —— **子 agent 里的用户消息不算**。这是**选出来的**一条口径，
 *      不是漏掉的：这份清单要回答的是「**我**在这个会话里说过什么」，而子 agent 的
 *      prompt 是主线派下去的活，不是人敲的。判据的名字里带着这个选择。
 *   4. 抽出来的**纯文本** trim 之后非空 —— 工具结果回灌（`content` 全是 `tool_result` 块）
 *      抽不出文本，靠这一条排除。
 *
 * ## ⚠ 一条**已知的**、写出来别当没有的不等价
 *
 * 渲染那边（`cards/index.ts::renderMessage`）在本函数这四条之外**还会再剥一层**
 * `stripInternalNoise`（`<system-reminder>` / `<local-command-stdout>` /
 * `[Request interrupted by user]` 之类），剥空了整条 `skip`、**不建卡**。
 * 本函数**故意不复刻那一层**：复刻 = 第二个住址 = 必然漂。
 * ⇒ 后果是本清单可能**多出**极少数「渲染那边被剥空、因而没有卡」的条目
 *   （现实里最常见的一形是 ESC 打断留下的 `[Request interrupted by user]`）。
 * 🔴 那一形**不许静默**：跳过去落在一个不存在的卡上时，
 *   `session-viewer.ts::jumpToUserInput` 会把那一行**标出来**（见该函数）。
 *
 * 🔴 **根治的路不在本文件的写区里**：单一住址的正解是让 `cards/index.ts` 把
 * 「这条记录会不会建成一张用户卡」导出成一个判定（它已经为 `/compact` 那一格
 * 这么做过 —— `isCompactRecord`），两边共用。`src/cards/` 本轮在写区外 ⇒ 交回 PM。
 */

/** 本模块只认这几个字段；刻意收窄成最小形状，不手抄 `JsonlRecord` 的镜像。 */
interface UserInputRecordShape {
  type?: string;
  uuid?: string | null;
  timestamp?: string | null;
  isMeta?: boolean;
  isSidechain?: boolean;
  message?: { content?: unknown } | null;
}

/** 一条可点的清单项。`payloadIndex` 让调用方不必再查一次 uuid→下标。 */
export interface UserInputEntry {
  uuid: string;
  /** 摘要文本（已截断），列表上显示的就是它 */
  excerpt: string;
  timestamp: string;
  /**
   * 它在**来源那个数组**里的下标。
   * 查看器 = `payloads` 的下标（那份是全量的）；
   * 实时窗口 = 本 tab 账本里的位置（它是一条一条攒起来的，没有「全量数组」可言）。
   * ⚠ 两条路的分母不同 —— 这个字段只说「第几个」，不是跨路可比的坐标。
   */
  payloadIndex: number;
}

/** 清单上一条显示多少字。超出截断加省略号。 */
const EXCERPT_MAX = 80;

/**
 * 抽纯文本：`content` 是字符串就是它本身；是数组就把 `type:"text"` 的块拼起来。
 * 与 `cards/index.ts::extractText` 同形 —— 那一份没导出，而本轮写区够不着 `src/cards/`。
 * ⚠ 这是上面头注里那条「已知不等价」的**另一半**，别当它是新债：新债是**没申报**的那种。
 */
function plainText(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((b) =>
        b && typeof b === "object" && (b as { type?: string }).type === "text"
          ? String((b as { text?: unknown }).text ?? "")
          : "",
      )
      .filter(Boolean)
      .join("\n");
  }
  return "";
}

/**
 * 把多行/多空白压成一行，再按 `EXCERPT_MAX` 截断（清单一行一条，别把布局撑爆）。
 *
 * `设计/17 §2.1`：这里原本是**整条正文**过一遍 `/\s+/g`，只为取 80 个字 ——
 * 【现打】44 万字符 **11.56 ms**，是那一轮全部现打读数里最大的单条开销；
 * 而且它在窗口门控**之前**跑，收纳不建卡的记录也照付。
 * 改法（§2.1 逐字）：**先截断再折叠**，O(len) → O(1)。
 */
function toExcerpt(text: string): string {
  const head = text.length > EXCERPT_MAX * 8 ? text.slice(0, EXCERPT_MAX * 8) : text;
  let flat = head.replace(/\s+/g, " ").trim();
  // 边界：前缀里若几乎全是空白（640 个空格 + 正文），截过再折叠会少字。
  // 折叠后仍不够长就退回整条 ⇒ 摘要语义**逐字不变**。这一支只在病态输入上走。
  if (flat.length < EXCERPT_MAX && head.length < text.length) {
    flat = text.replace(/\s+/g, " ").trim();
  }
  return flat.length > EXCERPT_MAX ? `${flat.slice(0, EXCERPT_MAX)}…` : flat;
}

/**
 * 从一串 jsonl 记录里挑出**主线用户输入**，按原顺序返回。
 *
 * `records[i]` 给的是**每条记录的 `message` 字段**（即 `JsonlLinePayload.message`），
 * 不是整个 payload —— 这样乙（实时窗口）那条路不必先把自己的数据包成 payload 形状。
 *
 * 口径见头注 §「口径」。**没有 uuid 的一律不要**：没有 uuid 就跳不过去，
 * 列出来就是一条点了没反应的项。
 */
/**
 * 流式那条路的入口：**一条一条**喂。不算用户输入 ⇒ 返回 `null`。
 *
 * 实时窗口（`tabs.ts::onLine`）手上一次只有一条 payload，而且**过完就没了**
 * （已渲染的记录文本在前端零处留存 —— 读数与分母在件 `§5.2`）。
 * ⇒ 它只能一条一条判、攒进自己的账本。
 *
 * 🔴 **口径不在这里复述一份**：本函数**直接调** `collectUserInputs`，
 * 所以「一条用户输入算不算」永远只有头注那**一个**住址。
 * 只有 `payloadIndex` 是本函数补的 —— 它在账本里的位置，喂它的人才知道。
 */
export function toUserInputEntry(message: unknown, index: number): UserInputEntry | null {
  const [e] = collectUserInputs([message]);
  return e ? { ...e, payloadIndex: index } : null;
}

export function collectUserInputs(records: readonly unknown[]): UserInputEntry[] {
  const out: UserInputEntry[] = [];
  for (let i = 0; i < records.length; i++) {
    const rec = records[i] as UserInputRecordShape | null;
    if (!rec || rec.type !== "user") continue;
    if (rec.isMeta === true) continue;
    if (rec.isSidechain === true) continue;
    const uuid = rec.uuid;
    if (!uuid) continue;
    const text = plainText(rec.message?.content).trim();
    if (!text) continue;
    out.push({
      uuid,
      excerpt: toExcerpt(text),
      timestamp: rec.timestamp ?? "",
      payloadIndex: i,
    });
  }
  return out;
}
