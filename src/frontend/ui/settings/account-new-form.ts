/**
 * 账号表上方就地展开的「新建账号」：名字 · 登录方式（订阅 / API key：地址 ＋ key）· 设为默认 · 导入已有登录。
 * 建目录、搭共享链接、写清单、放凭据或 key、补这个号的别名，全由那台机器的后端做（帧命令 `accounts-add`，
 * `src/frontend/ui/account-ops.ts`）。填表时每改一格问那台后端预演一次（不带 key）：预演过了主按钮才亮，
 * 「新增命令：workcc、workcct」是它答的别名名字，撞名 / 非法是它拒的那一句（与这台已有的号、已有的别名命令名都不许撞）。
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
import { field } from "../kit/field";
import { button, buttonRow, setButtonLabel } from "../kit/button";
import { checkbox, radio } from "../kit/switch";
import { confirmDialog, type ConfirmFn } from "../kit/dialog";
import type { Origin } from "../ipc/origin";

/** 接入方式 —— `§4.4` 那个岔口的两支。 */
export type AccountAccess = "subscription" | "apikey";

/** 交给调用方的那一份请求（就是 `accounts-add` 的入参；API 号那一支带着 key）。 */
export type NewAccountRequest = AccountAddArgs;

/**
 * 〔线框里那一格〕Base URL 的**表单侧**那一句：留空合法（= 默认上游，D3 缺省）；
 * 「能不能用」读生成物 `baseUrlIssue`（规则住 `upstream_url_core::usable`，与后端写口 · 上游选择装表同一条），
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

let formSeq = 0;

/** 命令名那一行：名字是那台后端预演时答的（`aliasNames`），这里只套一句话。 */
export function aliasHintFor(names: readonly string[]): string {
  return names.length ? copyText("acctNew.name.commands", { names: names.join(copyText("acctNew.name.commandSep")) }) : "";
}

export interface NewAccountForm {
  root: HTMLElement;
  /** 焦点落在名字框。 */
  focus(): void;
  /** Esc：空表单直接收；填过先问「放弃填写？」。回 `true` ＝ 收了。 */
  dismiss(): Promise<boolean>;
}

/**
 * 画出那张表单。`onCreate` 收到的是一份**那台后端预演过**的请求（名字合法、API 号有 key 且地址可用）；
 * 不合法 / 预演没过时主按钮是灰的，就算被绕过 `disabled` 点了也不交出去。`onCancel`：表单收起。
 */
export function renderNewAccountForm(
  origin: Origin,
  machine: string,
  hooks: { onCreate: (req: NewAccountRequest) => void | Promise<void>; onCancel: () => void; confirm?: ConfirmFn },
): NewAccountForm {
  const box = document.createElement("div");
  box.className = "acct-new";
  const title = document.createElement("div");
  title.className = "acct-new-title";
  title.textContent = copyText("acctNew.form.title");
  box.appendChild(title);

  const grid = document.createElement("div");
  grid.className = "acct-new-grid";
  const name = field({ label: copyText("acctNew.name.label"), placeholder: copyText("acctNew.name.example") });
  const nameIn = name.input as HTMLInputElement;
  const accessCol = document.createElement("div");
  accessCol.className = "acct-new-access";
  const accessLabel = document.createElement("div");
  accessLabel.className = "acct-new-label";
  accessLabel.textContent = copyText("acctNew.access.label");
  const radios = document.createElement("div");
  radios.className = "acct-new-radios";
  radios.setAttribute("role", "radiogroup");
  radios.setAttribute("aria-label", copyText("acctNew.access.label"));
  let access: AccountAccess = "subscription";
  const group = `acct-new-access-${++formSeq}`;
  const pick = (a: AccountAccess) => (): void => {
    access = a;
    sync();
  };
  radios.append(
    radio(group, copyText("acctNew.access.subscription"), true, pick("subscription")),
    radio(group, copyText("acctNew.access.apikey"), false, pick("apikey")),
  );
  accessCol.append(accessLabel, radios);
  const base = field({ label: copyText("acctNew.base.label"), placeholder: copyText("acctNew.base.example"), help: copyText("acctNew.base.help") });
  const baseIn = base.input as HTMLInputElement;
  baseIn.autocomplete = "off";
  baseIn.dataset.field = "base-url";
  const key = field({ label: copyText("acctNew.key.label"), help: copyText("acctNew.key.help", { machine }) });
  const keyIn = key.input as HTMLInputElement;
  keyIn.type = "password";
  keyIn.autocomplete = "off";
  // 地址 / key 两格各包一层（kit 输入框自己写了 display，不能直接拿 hidden 切它）。
  const baseCell = document.createElement("div");
  baseCell.className = "acct-new-cell";
  baseCell.appendChild(base.root);
  const keyCell = document.createElement("div");
  keyCell.className = "acct-new-cell";
  keyCell.appendChild(key.root);
  grid.append(name.root, accessCol, baseCell, keyCell);
  box.appendChild(grid);

  const extras = document.createElement("div");
  extras.className = "acct-new-extras";
  let makeDefault = false;
  const def = checkbox(copyText("acctNew.default.label"), false, (on) => {
    makeDefault = on;
    sync();
  });
  def.dataset.field = "is-default";
  const importToggle = document.createElement("button");
  importToggle.type = "button";
  importToggle.className = "acct-new-import";
  importToggle.setAttribute("aria-expanded", "false");
  importToggle.textContent = copyText("acctNew.import.toggle");
  extras.append(def, importToggle);
  box.appendChild(extras);
  const cred = field({ label: copyText("acctNew.import.label"), placeholder: copyText("acctNew.import.example") });
  const credIn = cred.input as HTMLInputElement;
  const credCell = document.createElement("div");
  credCell.className = "acct-new-cell";
  credCell.hidden = true;
  credCell.appendChild(cred.root);
  box.appendChild(credCell);
  importToggle.addEventListener("click", () => {
    credCell.hidden = !credCell.hidden;
    importToggle.setAttribute("aria-expanded", String(!credCell.hidden));
  });

  const cancel = button({ label: copyText("acctNew.form.cancel") });
  const create = button({ label: copyText("acctNew.form.createLogin"), kind: "primary" });
  box.appendChild(buttonRow(cancel, create));

  /** 这一份输入的意图（不带 key）：预演问的就是它。名字不合法 / 地址用不了 ⇒ 说为什么。 */
  const intent = (): { args: AccountAddArgs } | { why: string } => {
    const n = nameIn.value.trim();
    const v = validateAcctName(n);
    if (!v.ok) return { why: n ? v.reason : "" };
    const args: AccountAddArgs = { name: n, kind: access === "apikey" ? "api-key" : "subscription" };
    if (makeDefault) args.isDefault = true;
    if (access === "apikey") {
      const b = checkBaseUrl(baseIn.value);
      if (!b.ok) return { why: b.reason };
      if (b.value !== undefined) args.baseUrl = b.value;
    } else {
      const c = credIn.value.trim();
      if (c) args.credFile = c;
    }
    return { args };
  };

  /** 那台后端对「这一份输入」预演过了 ⇒ `true`（只认最后一问）。主按钮要它为真。 */
  let dryOk = false;
  let asked = 0;
  /** 本侧那几格齐不齐（API 号还要有 key）。 */
  const filled = (): boolean => "args" in intent() && (access !== "apikey" || keyIn.value.trim() !== "");
  const dirty = (): boolean => [nameIn, baseIn, keyIn, credIn].some((i) => i.value.trim() !== "");

  const sync = (): void => {
    const apikey = access === "apikey";
    baseCell.hidden = !apikey;
    keyCell.hidden = !apikey;
    importToggle.hidden = apikey;
    if (apikey) credCell.hidden = true;
    setButtonLabel(create, apikey ? copyText("acctNew.form.create") : copyText("acctNew.form.createLogin"));
    const cur = intent();
    name.setError("args" in cur || cur.why === "" ? null : cur.why);
    create.disabled = true;
    dryOk = false;
    const my = ++asked;
    if (!("args" in cur)) {
      setNameHelp("");
      return;
    }
    void accountsAdd(origin, { ...cur.args, dryRun: true }).then(
      (plan) => {
        if (my !== asked) return; // 输入又变了：这一问作废
        setNameHelp(aliasHintFor(plan.aliasNames));
        dryOk = true;
        create.disabled = !filled();
      },
      (e: unknown) => {
        if (my !== asked) return;
        setNameHelp("");
        name.setError(saidOfControl(e));
      },
    );
  };
  const nameHelp = name.root.lastElementChild as HTMLElement;
  const setNameHelp = (t: string): void => {
    if (!name.root.dataset.error) nameHelp.textContent = t;
  };

  for (const el of [nameIn, baseIn, credIn]) el.addEventListener("input", sync);
  // key 那一格不进预演（预演不带 key）：只重算主按钮亮不亮。
  keyIn.addEventListener("input", () => {
    create.disabled = !(dryOk && filled());
  });
  nameIn.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter" && !create.disabled) {
      ev.preventDefault();
      create.click();
    }
  });

  const clear = (): void => {
    nameIn.value = "";
    keyIn.value = "";
    baseIn.value = "";
    credIn.value = "";
  };
  cancel.addEventListener("click", () => {
    clear();
    hooks.onCancel();
  });
  create.addEventListener("click", () => {
    const cur = intent();
    if (!("args" in cur) || !dryOk || !filled()) return;
    const req: NewAccountRequest = { ...cur.args };
    if (access === "apikey") req.key = keyIn.value.trim();
    // 先清空再交出去：明文在 DOM 里停留的时间越短越好（`KS6`）。
    clear();
    sync();
    void hooks.onCreate(req);
  });

  sync();
  return {
    root: box,
    focus: () => nameIn.focus(),
    dismiss: async () => {
      if (dirty()) {
        const ok = await (hooks.confirm ?? confirmDialog)({ title: copyText("acctNew.discard.title"), action: copyText("acctNew.discard.action") });
        if (!ok) return false;
      }
      clear();
      hooks.onCancel();
      return true;
    },
  };
}
