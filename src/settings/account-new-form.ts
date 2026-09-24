/**
 * `设计/70 §4.4`（A2）：**新建账号 —— 一个表单问清楚，岔口在表单里。**
 *
 * # 它治的是什么
 *
 * `70 §4.2` 现打：加一个账号今天要走 8 步、跨 2 个顶层页，而且「加账号」那颗按钮是红的
 * （`.danger`）。五条病（`§4.3`）：
 * ① 加账号藏在「维护」折叠组里 —— 它是最常用的账号操作，不是维护；
 * ② 按钮是红色 —— 新建不是破坏性动作，红色留给「删账号」；
 * ③ 「哪个账号」被问了两次（apikey 那块自己还有一个账号下拉）；
 * ④ 「订阅还是第三方 apikey」要建完之后去另一块配；
 * ⑤ 下一步靠散文导航。
 *
 * 本模块治 ①②④，③ 由 `accounts-section.ts` 那一侧治（apikey 成了每个账号那一行的一格，
 * 整块里不再有账号下拉）。⑤ 归别名管理器那一路（`设计/71 §13`），这里只把命令名当成
 * 表单里的一行提示给出来，不再写「去另一个顶层页找」。
 *
 * # 岔口怎么接
 *
 * - **订阅**：弹终端跑 `cc-acct-iso add <名> --apply`，用户在那个终端里 `/login`。
 * - **第三方 apikey**：同一条命令建出账号目录；**key 在表单里就收下**，
 *   等这个号出现在账号列表里（知道了它的 configDir）时由 `accounts-section.ts`
 *   接着调 `write_relay_credentials_key` 写进 apikey 表 —— 两条命令在前端串起来，
 *   用户看到的是一次操作（`§4.4` 末段：「前提是后端不动」）。
 *
 * ⚠ 为什么不能当场就写 key：apikey 表按账号目录索引，而账号目录由那条终端命令建，
 *   前端**不许自己从名字推目录**（`KH2C1`：那条规则全仓只有 Rust 那一份）。
 *
 * # ★ key 的纪律（`KS6`）
 *
 * 本文件里每一处 `.value =` 赋值的右边都只许是空串 —— 输入框从不预填、交出去之前先清空。
 * 由 `accounts-section.vitest.ts` 里 `KS6` 那条源码扫描钉着（人群含本文件）。
 */
import { buildAcctIsoCmd, validateAcctName } from "./acct-deploy";
import { suggestAliasName } from "../launcher-diagnostics";

/** 接入方式 —— `§4.4` 那个岔口的两支。 */
export type AccountAccess = "subscription" | "apikey";

export type NewAccountRequest =
  | { name: string; access: "subscription"; credFile?: string }
  | { name: string; access: "apikey"; key: string };

/** 表单上给用户看的字。集中在一处，判据按这张表逐条对（不在断言里手抄第二份）。 */
export const NEW_ACCOUNT_COPY = {
  title: "新建账号",
  namePlaceholder: "账号名，如 b",
  accessLabel: "接入方式",
  subscription: "订阅",
  subscriptionHint: "创建后弹出终端，在那个终端里用这个号 /login。",
  apikey: "第三方 apikey",
  apikeyHint:
    "创建后弹出终端建好账号目录；这个号出现在下面的账号列表里时（终端跑完点「刷新」），" +
    "这把 key 自动写进 apikey 表。在那之前 key 只留在这个窗口里，关掉 monitor 就没了。",
  keyPlaceholder: "粘贴 API key",
  advanced: "高级：从旧凭据快照导入（免重登）",
  credPlaceholder: "旧凭据快照路径",
  previewHead: "将在终端里运行：",
  previewEmpty: "（填好账号名后显示将运行的命令）",
  cancel: "清空",
  create: "创建",
} as const;

let formSeq = 0;

/** 命令名那一行。规则只有一个住址（`suggestAliasName`），这里不抄第二份。 */
export function aliasHintFor(name: string): string {
  const alias = suggestAliasName(name);
  return alias ? `这个号的命令名会是 ${alias}` : "";
}

/**
 * 画出那张表单。`onCreate` 收到的是一份**已经校验过**的请求（名字合法、apikey 支有 key、
 * 快照路径能拼成命令）；不合法时「创建」是灰的，并且就算被绕过 `disabled` 点了也不交出去。
 */
export function renderNewAccountForm(
  onCreate: (req: NewAccountRequest) => void | Promise<void>,
): HTMLElement {
  const C = NEW_ACCOUNT_COPY;
  const box = document.createElement("div");
  box.className = "accounts-new";

  const title = document.createElement("div");
  title.className = "accounts-ne-title";
  title.textContent = C.title;
  box.appendChild(title);

  const nameIn = document.createElement("input");
  nameIn.type = "text";
  nameIn.className = "accounts-maint-name";
  nameIn.placeholder = C.namePlaceholder;
  box.appendChild(nameIn);
  const nameErr = document.createElement("div");
  nameErr.className = "accounts-maint-err";
  box.appendChild(nameErr);
  const aliasHint = document.createElement("div");
  aliasHint.className = "accounts-new-hint";
  box.appendChild(aliasHint);

  // ---- 岔口：订阅 / 第三方 apikey ----
  const access = document.createElement("div");
  access.className = "accounts-new-access";
  access.setAttribute("role", "radiogroup");
  access.setAttribute("aria-label", C.accessLabel);
  const radios = new Map<AccountAccess, HTMLInputElement>();
  // 每个表单一个组名：reload 重建表单时新旧两张（旧的正被摘下）也不会互相取消选中。
  const group = `accounts-new-access-${++formSeq}`;
  for (const [value, label] of [
    ["subscription", C.subscription],
    ["apikey", C.apikey],
  ] as const) {
    const row = document.createElement("label");
    row.className = "settings-row-checkbox";
    const r = document.createElement("input");
    r.type = "radio";
    r.name = group;
    r.value = value;
    r.checked = value === "subscription";
    radios.set(value, r);
    const span = document.createElement("span");
    span.className = "settings-checkbox-label";
    span.textContent = label;
    row.append(r, span);
    access.appendChild(row);
  }
  box.appendChild(access);
  const accessHint = document.createElement("div");
  accessHint.className = "accounts-new-hint";
  box.appendChild(accessHint);

  // ---- apikey 那一支（选了才显示）----
  const keyBox = document.createElement("div");
  keyBox.className = "accounts-new-key";
  const keyIn = document.createElement("input");
  keyIn.type = "password";
  keyIn.className = "accounts-maint-cred";
  keyIn.placeholder = C.keyPlaceholder;
  keyIn.autocomplete = "off";
  keyBox.appendChild(keyIn);
  box.appendChild(keyBox);

  // ---- 高级：凭据快照（只对订阅那一支有意义：快照里是订阅凭据）----
  const adv = document.createElement("details");
  adv.className = "accounts-new-adv";
  const advSum = document.createElement("summary");
  advSum.textContent = C.advanced;
  adv.appendChild(advSum);
  const credIn = document.createElement("input");
  credIn.type = "text";
  credIn.className = "accounts-maint-cred";
  credIn.placeholder = C.credPlaceholder;
  adv.appendChild(credIn);
  box.appendChild(adv);

  const preview = document.createElement("pre");
  preview.className = "accounts-wiz-preview";
  box.appendChild(preview);

  const btns = document.createElement("div");
  btns.className = "accounts-new-btns";
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.className = "settings-btn settings-btn-secondary";
  cancel.textContent = C.cancel;
  const create = document.createElement("button");
  create.type = "button";
  // ★ `§4.3` ②：**不是红色**。红色（`.danger`）留给删账号。
  create.className = "settings-btn settings-btn-primary";
  create.textContent = C.create;
  btns.append(cancel, create);
  box.appendChild(btns);

  const chosen = (): AccountAccess =>
    radios.get("apikey")!.checked ? "apikey" : "subscription";

  /** 当前表单能不能交；能交就给出那份请求。**判据与按钮共用这一个函数。** */
  const current = (): { req: NewAccountRequest; cmd: string } | { why: string } => {
    const name = nameIn.value.trim();
    const v = validateAcctName(name);
    if (!v.ok) return { why: name ? v.reason : "" };
    const credFile = chosen() === "subscription" ? credIn.value.trim() || undefined : undefined;
    const built = buildAcctIsoCmd({ kind: "add-apply", name, credFile });
    if (!built.ok) return { why: built.reason };
    if (chosen() === "apikey") {
      const key = keyIn.value.trim();
      if (!key) return { why: "" };
      return { req: { name, access: "apikey", key }, cmd: built.cmd };
    }
    return { req: { name, access: "subscription", credFile }, cmd: built.cmd };
  };

  const sync = (): void => {
    const apikey = chosen() === "apikey";
    keyBox.hidden = !apikey;
    adv.hidden = apikey;
    accessHint.textContent = apikey ? C.apikeyHint : C.subscriptionHint;
    aliasHint.textContent = aliasHintFor(nameIn.value.trim());
    const cur = current();
    if ("req" in cur) {
      nameErr.textContent = "";
      create.disabled = false;
      preview.textContent = `${C.previewHead}\n${cur.cmd}`;
    } else {
      nameErr.textContent = cur.why;
      create.disabled = true;
      // 名字合法时（只差 key）命令照样能预览 —— 命令本身与 key 无关。
      const name = nameIn.value.trim();
      const built = validateAcctName(name).ok
        ? buildAcctIsoCmd({ kind: "add-apply", name })
        : null;
      preview.textContent =
        built && built.ok ? `${C.previewHead}\n${built.cmd}` : C.previewEmpty;
    }
  };

  for (const el of [nameIn, keyIn, credIn]) el.addEventListener("input", sync);
  for (const r of radios.values()) r.addEventListener("change", sync);

  cancel.addEventListener("click", () => {
    nameIn.value = "";
    keyIn.value = "";
    credIn.value = "";
    sync();
  });
  create.addEventListener("click", () => {
    const cur = current();
    if (!("req" in cur)) return;
    // 先清空再交出去：明文在 DOM 里停留的时间越短越好（`KS6`）。
    keyIn.value = "";
    nameIn.value = "";
    credIn.value = "";
    sync();
    void onCreate(cur.req);
  });

  sync();
  return box;
}
