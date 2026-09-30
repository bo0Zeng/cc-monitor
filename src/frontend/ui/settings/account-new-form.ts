/**
 * `设计/70 §4.4`（A2）：**新建账号 —— 一个表单问清楚，岔口在表单里。**
 *
 * 表单只交意图：账号名 · 订阅还是第三方 API key · 订阅号要不要从一份旧凭据导入 · API 号的地址与 key · 是否设为默认。
 * 建目录、搭共享链接、写清单、放凭据或 key、补这个号的别名，全由那台机器的后端做（帧命令 `accounts-add`，
 * `src/frontend/ui/account-ops.ts`）。填表时每改一格问那台后端预演一次（不带 key），预演答得出来「创建」才亮；
 * 预演那几步原样上屏，别名名字也是它答的。
 *
 * # ★ key 的纪律（`KS6`）
 *
 * 本文件里每一处 `.value =` 赋值的右边都只许是空串 —— 输入框从不预填、交出去之前先清空。
 * 由 `accounts-section.vitest.ts` 里 `KS6` 那条源码扫描钉着（人群含本文件）。
 */
import { accountsAdd, validateAcctName, type AccountAddArgs } from "../account-ops";
import { copyText } from "../copy-table";
import { baseUrlIssue, type BaseUrlIssue } from "../generated/judgment-rules";
import { saidOfControl } from "../control-said";
import type { Origin } from "../ipc/origin";

/** 接入方式 —— `§4.4` 那个岔口的两支。 */
export type AccountAccess = "subscription" | "apikey";

/** 交给调用方的那一份请求（就是 `accounts-add` 的入参；API 号那一支带着 key）。 */
export type NewAccountRequest = AccountAddArgs;

/**
 * 〔第四波 ST2 · `70 §4.4` 线框里那一格〕Base URL 的**表单侧**那一句：留空合法（= 默认上游，D3 缺省）；
 * 「能不能用」〔DUP3 · J9〕读生成物 `baseUrlIssue`（规则住 `upstream_url_core::usable`，与后端写口 · 上游选择装表同一条），
 * 这里只按理由挑一句话 —— 让「创建」在填错时是灰的，不让用户建完号才发现端点没配上。
 */
export function checkBaseUrl(raw: string): { ok: true; value: string | undefined } | { ok: false; reason: string } {
  const s = raw.trim();
  if (s === "") return { ok: true, value: undefined };
  const issue = baseUrlIssue(s);
  if (issue === null) return { ok: true, value: s };
  return { ok: false, reason: baseUrlSaid(issue) };
}

/** 用不了的理由 ⇒ 表单那一句（三句一字没改）。 */
function baseUrlSaid(issue: BaseUrlIssue): string {
  switch (issue) {
    case "plaintextOffLoopback":
      return copyText("accountNewForm.baseUrl.plainHttp");
    case "badScheme":
      return copyText("accountNewForm.baseUrl.scheme");
    default:
      return copyText("accountNewForm.baseUrl.shape");
  }
}

/** 表单上给用户看的字。集中在一处，判据按这张表逐条对（不在断言里手抄第二份）。 */
// 〔CP2b〕取值器（getter）：用到时才取文 —— 模块顶层不留取文口调用（顶层有调用会让 Rollup 把这份挪进主窗也加载的共享 chunk）。
export const NEW_ACCOUNT_COPY = {
  get title() {
    return copyText("accountNewForm.form.title");
  },
  get namePlaceholder() {
    return copyText("accountNewForm.form.nameHint");
  },
  get accessLabel() {
    return copyText("accountNewForm.form.accessMode");
  },
  get subscription() {
    return copyText("accountNewForm.form.subscription");
  },
  get subscriptionHint() {
    return copyText("accountNewForm.form.subscriptionHint");
  },
  get apikey() {
    return copyText("accountNewForm.form.apikey");
  },
  get apikeyHint() {
    return copyText("accountNewForm.form.apikeyHint");
  },
  get keyPlaceholder() {
    return copyText("accountNewForm.form.key");
  },
  get baseUrlPlaceholder() {
    return copyText("accountNewForm.form.baseUrl");
  },
  get advanced() {
    return copyText("accountNewForm.form.advanced");
  },
  get credPlaceholder() {
    return copyText("accountNewForm.form.snapshot");
  },
  get makeDefault() {
    return copyText("accountNewForm.form.makeDefault");
  },
  get previewHead() {
    return copyText("accountNewForm.form.willRun");
  },
  get previewEmpty() {
    return copyText("accountNewForm.form.willRunEmpty");
  },
  get cancel() {
    return copyText("accountNewForm.form.clear");
  },
  get create() {
    return copyText("accountNewForm.form.create");
  },
} as const;

let formSeq = 0;

/** 命令名那一行：名字是那台后端预演时答的（`alias`），这里只套一句话。 */
export function aliasHintFor(alias: string | null): string {
  return alias ? copyText("accountNewForm.aliasHintFor.aliasHint", { alias }) : "";
}

/**
 * 画出那张表单。`onCreate` 收到的是一份**那台后端预演过**的请求（名字合法、API 号有 key 且地址可用）；
 * 不合法 / 预演没过时「创建」是灰的，并且就算被绕过 `disabled` 点了也不交出去。
 */
export function renderNewAccountForm(
  origin: Origin,
  onCreate: (req: NewAccountRequest) => void | Promise<void>,
  /**
   * 〔第三波 S3〕两支岔口下面那一行提示的**替换**。缺席 = 远端那一页的原话。
   * 本机那一页要换：本机在 Linux 上**不开终端窗口**（登录那一行复制给人自己跑），只换这两句。
   */
  hints?: { subscription: string; apikey: string },
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
  // 〔ST2 · `70 §4.4`〕线框里 apikey 那一支是两格：Base URL ＋ API key。
  const baseIn = document.createElement("input");
  baseIn.type = "text";
  baseIn.className = "accounts-maint-cred";
  baseIn.placeholder = C.baseUrlPlaceholder;
  baseIn.autocomplete = "off";
  baseIn.dataset.field = "base-url";
  keyBox.appendChild(baseIn);
  const keyIn = document.createElement("input");
  keyIn.type = "password";
  keyIn.className = "accounts-maint-cred";
  keyIn.placeholder = C.keyPlaceholder;
  keyIn.autocomplete = "off";
  keyBox.appendChild(keyIn);
  box.appendChild(keyBox);

  // ---- 高级：从旧凭据导入（只对订阅那一支有意义：快照里是订阅凭据）----
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

  // ---- 设为默认 ----
  const defRow = document.createElement("label");
  defRow.className = "settings-row-checkbox";
  const defIn = document.createElement("input");
  defIn.type = "checkbox";
  defIn.dataset.field = "is-default";
  const defSpan = document.createElement("span");
  defSpan.className = "settings-checkbox-label";
  defSpan.textContent = C.makeDefault;
  defRow.append(defIn, defSpan);
  box.appendChild(defRow);

  const preview = document.createElement("pre");
  preview.className = "accounts-wiz-preview";
  box.appendChild(preview);

  const btns = document.createElement("div");
  btns.className = "accounts-new-btns";
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.className = "settings-btn";
  cancel.textContent = C.cancel;
  const create = document.createElement("button");
  create.type = "button";
  // ★ `§4.3` ②：**不是红色**。红色（`.danger`）留给删账号。
  create.className = "settings-btn settings-btn-primary";
  create.textContent = C.create;
  btns.append(cancel, create);
  box.appendChild(btns);

  const chosen = (): AccountAccess => (radios.get("apikey")!.checked ? "apikey" : "subscription");

  /** 这一份输入的意图（不带 key）：预演问的就是它。名字不合法 / 地址用不了 ⇒ 说为什么。 */
  const intent = (): { args: AccountAddArgs } | { why: string } => {
    const name = nameIn.value.trim();
    const v = validateAcctName(name);
    if (!v.ok) return { why: name ? v.reason : "" };
    const args: AccountAddArgs = { name, kind: chosen() === "apikey" ? "api-key" : "subscription" };
    if (defIn.checked) args.isDefault = true;
    if (chosen() === "apikey") {
      const base = checkBaseUrl(baseIn.value);
      if (!base.ok) return { why: base.reason };
      if (base.value !== undefined) args.baseUrl = base.value;
    } else {
      const cred = credIn.value.trim();
      if (cred) args.credFile = cred;
    }
    return { args };
  };

  /** 那台后端对「这一份输入」预演过了 ⇒ `true`（只认最后一问）。「创建」要它为真。 */
  let dryOk = false;
  let asked = 0;

  /** 本侧那几格齐不齐（API 号还要有 key）。 */
  const filled = (): boolean => "args" in intent() && (chosen() !== "apikey" || keyIn.value.trim() !== "");

  const sync = (): void => {
    const apikey = chosen() === "apikey";
    keyBox.hidden = !apikey;
    adv.hidden = apikey;
    accessHint.textContent = apikey ? (hints?.apikey ?? C.apikeyHint) : (hints?.subscription ?? C.subscriptionHint);
    const cur = intent();
    nameErr.textContent = "args" in cur ? "" : cur.why;
    create.disabled = true;
    dryOk = false;
    const my = ++asked;
    if (!("args" in cur)) {
      preview.textContent = C.previewEmpty;
      aliasHint.textContent = "";
      return;
    }
    void accountsAdd(origin, { ...cur.args, dryRun: true }).then(
      (plan) => {
        if (my !== asked) return; // 输入又变了：这一问作废
        preview.textContent = [C.previewHead, ...plan.steps, ...plan.notes].join("\n");
        aliasHint.textContent = aliasHintFor(plan.alias);
        dryOk = true;
        create.disabled = !filled();
      },
      (e: unknown) => {
        if (my !== asked) return;
        preview.textContent = C.previewEmpty;
        aliasHint.textContent = "";
        nameErr.textContent = saidOfControl(e);
      },
    );
  };

  for (const el of [nameIn, baseIn, credIn]) el.addEventListener("input", sync);
  // key 那一格不进预演（预演不带 key）：只重算「创建」亮不亮。
  keyIn.addEventListener("input", () => {
    create.disabled = !(dryOk && filled());
  });
  defIn.addEventListener("change", sync);
  for (const r of radios.values()) r.addEventListener("change", sync);

  cancel.addEventListener("click", () => {
    nameIn.value = "";
    keyIn.value = "";
    baseIn.value = "";
    credIn.value = "";
    defIn.checked = false;
    sync();
  });
  create.addEventListener("click", () => {
    const cur = intent();
    if (!("args" in cur) || !dryOk || !filled()) return;
    const req: NewAccountRequest = { ...cur.args };
    if (chosen() === "apikey") req.key = keyIn.value.trim();
    // 先清空再交出去：明文在 DOM 里停留的时间越短越好（`KS6`）。
    keyIn.value = "";
    baseIn.value = "";
    nameIn.value = "";
    credIn.value = "";
    defIn.checked = false;
    sync();
    void onCreate(req);
  });

  sync();
  return box;
}
