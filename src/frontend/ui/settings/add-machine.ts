/**
 * 「添加机器」对话框：两栏「从 ~/.ssh/config 选」·「自己填」。
 *
 * - 点「添加」之前一个字节都不写；写失败框不关、填的都在。
 * - 已在列表里的那台（地址 · 用户 · 端口相同）灰着、勾不了；多地址的那台可「按地址拆成 N 台」。
 * - 名字不许与已有机器、与这一框里勾的其它台重名；地址、用户必填；端口 1–65535。错在那一格下面说，有错时主按钮禁用。
 */
import { formDialog } from "../kit/dialog";
import { field, type FieldHandle } from "../kit/field";
import { fold } from "../kit/fold";
import { tabs } from "../kit/tabs";
import { checkbox } from "../kit/switch";
import { copyText } from "../copy-table";
import type { RemoteHostConfig } from "../remote-config";
import type { ImportGroup } from "../ssh-config-reads";

export interface AddMachineDeps {
  /** 列表里已有的那几台。 */
  existing: () => RemoteHostConfig[];
  /** 读 ~/.ssh/config（失败 ⇒ 抛）；每组带着后端判的「已在列表里」。 */
  groups: () => Promise<ImportGroup[]>;
  /** 写进去：成了 ⇒ `null`；没存上 ⇒ 一句原因。 */
  add: (cfgs: RemoteHostConfig[]) => Promise<string | null>;
}

const blank = (): RemoteHostConfig => ({
  label: "",
  host: "",
  port: 22,
  user: "",
  keyPath: "",
  hostKeyFingerprint: "",
  hostKeyPinnedAt: "",
  addresses: [],
  jump: "",
  resumeCommand: "",
  connect: true,
});

const nameOf = (h: RemoteHostConfig): string => h.label.trim() || h.host.trim();

interface SshRow {
  g: ImportGroup;
  cfg: RemoteHostConfig;
  inList: boolean;
  picked: boolean;
  split: boolean;
  name: FieldHandle | null;
}

/** 组 → 一台（成员多于一个时别名记在备用地址里）。 */
function groupCfg(g: ImportGroup): RemoteHostConfig {
  return { ...blank(), label: g.label, host: g.host, port: g.port || 22, user: g.user, keyPath: g.keyPath ?? "", addresses: g.addresses, jump: g.jump ?? "" };
}

/** 拆开：每个成员一台。 */
function memberCfgs(g: ImportGroup): RemoteHostConfig[] {
  return g.members.map((m) => ({ ...blank(), label: m.alias, host: m.host, port: m.port || 22, user: g.user, keyPath: g.keyPath ?? "", jump: m.proxyJump ?? "" }));
}

export function openAddMachine(deps: AddMachineDeps): Promise<boolean> {
  const body = document.createElement("div");
  body.className = "add-machine";
  let tab: "ssh" | "manual" = "ssh";
  const sshPane = document.createElement("div");
  sshPane.className = "add-machine-pane";
  const manualPane = document.createElement("div");
  manualPane.className = "add-machine-pane";
  manualPane.hidden = true;
  const strip = tabs<"ssh" | "manual">({
    items: [
      { key: "ssh", label: copyText("addMachine.tab.ssh") },
      { key: "manual", label: copyText("addMachine.tab.manual") },
    ],
    current: "ssh",
    label: copyText("addMachine.dialog.title"),
    onChange: (k) => {
      tab = k;
      sshPane.hidden = k !== "ssh";
      manualPane.hidden = k !== "manual";
      update();
    },
  });
  body.append(strip, sshPane, manualPane);

  // ---- 从 ~/.ssh/config 选 ----
  const rows: SshRow[] = [];
  const sshNote = document.createElement("div");
  sshNote.className = "add-machine-note";
  sshNote.textContent = copyText("addMachine.ssh.note");
  const sshAfter = document.createElement("div");
  sshAfter.className = "add-machine-note";
  sshAfter.textContent = copyText("addMachine.ssh.after");
  const list = document.createElement("div");
  list.className = "add-machine-list";
  sshPane.append(sshNote, list, sshAfter);

  const pickedCfgs = (): RemoteHostConfig[] => {
    const out: RemoteHostConfig[] = [];
    for (const r of rows) {
      if (!r.picked || r.inList) continue;
      if (r.split) out.push(...memberCfgs(r.g));
      else out.push({ ...r.cfg, label: (r.name?.input.value ?? r.cfg.label).trim() });
    }
    return out;
  };

  const renderSsh = (): void => {
    list.replaceChildren();
    for (const r of rows) {
      const item = document.createElement("div");
      item.className = "add-machine-row";
      if (r.inList) item.dataset.inList = "true";
      const box = checkbox("", r.picked && !r.inList, (on) => {
        r.picked = on;
        update();
      });
      const input = box.querySelector("input");
      if (input && r.inList) input.disabled = true;
      box.setAttribute("aria-label", r.cfg.label);
      const main = document.createElement("div");
      main.className = "add-machine-main";
      r.name = field({ label: copyText("addMachine.field.name"), value: r.cfg.label });
      r.name.input.addEventListener("input", update);
      if (r.inList || r.split) r.name.setDisabled(r.inList ? copyText("addMachine.ssh.inList") : null);
      const info = document.createElement("div");
      info.className = "add-machine-info";
      const parts = [r.g.host, r.g.user || copyText("remote.preview.noUser")];
      if (r.g.jump) parts.push(copyText("addMachine.ssh.jump", { jump: r.g.jump }));
      if (r.g.addresses.length) parts.push(copyText("addMachine.ssh.addresses", { n: r.g.addresses.length }));
      info.textContent = r.split
        ? copyText("addMachine.ssh.splitInto", { names: r.g.members.map((m) => m.alias).join(copyText("kit.text.sep")) })
        : parts.join(copyText("kit.text.sep"));
      main.append(r.name.root, info);
      if (r.inList) {
        const tag = document.createElement("span");
        tag.className = "add-machine-tag";
        tag.textContent = copyText("addMachine.ssh.inList");
        main.appendChild(tag);
      } else if (r.g.members.length > 1) {
        const link = document.createElement("button");
        link.type = "button";
        link.className = "add-machine-link";
        link.textContent = r.split
          ? copyText("addMachine.ssh.unsplit")
          : copyText("addMachine.ssh.split", { n: r.g.members.length });
        link.addEventListener("click", () => {
          r.split = !r.split;
          renderSsh();
          update();
        });
        main.appendChild(link);
      }
      item.append(box, main);
      list.appendChild(item);
    }
  };

  void deps
    .groups()
    .then((groups) => {
      for (const g of groups) {
        rows.push({ g, cfg: groupCfg(g), inList: g.inList, picked: !g.inList, split: false, name: null });
      }
      if (rows.length === 0) {
        sshNote.textContent = copyText("addMachine.ssh.none");
        strip.querySelector<HTMLButtonElement>('[data-key="manual"]')?.click();
      }
      renderSsh();
      update();
    })
    .catch(() => {
      sshNote.textContent = copyText("addMachine.ssh.none");
      strip.querySelector<HTMLButtonElement>('[data-key="manual"]')?.click();
    });

  // ---- 自己填 ----
  const grid = document.createElement("div");
  grid.className = "add-machine-grid";
  const fName = field({ label: copyText("addMachine.field.name"), help: copyText("addMachine.field.nameHelp") });
  const fHost = field({ label: copyText("addMachine.field.host"), help: copyText("machineCard.field.hostHint") });
  const fUser = field({ label: copyText("addMachine.field.user"), help: copyText("addMachine.field.userHelp") });
  const fPort = field({ label: copyText("addMachine.field.port"), value: "22", help: copyText("addMachine.field.portHelp") });
  const fKey = field({ label: copyText("addMachine.field.key"), help: copyText("addMachine.field.keyHelp") });
  fKey.root.classList.add("add-machine-wide");
  grid.append(fName.root, fHost.root, fUser.root, fPort.root, fKey.root);
  const moreBody = document.createElement("div");
  moreBody.className = "add-machine-grid";
  const fAddrs = field({ label: copyText("addMachine.field.addresses"), help: copyText("addMachine.field.addressesHelp"), multiline: true });
  const fJump = field({ label: copyText("addMachine.field.jump"), help: copyText("machineCard.field.jumpHint") });
  moreBody.append(fAddrs.root, fJump.root);
  const manualAfter = document.createElement("div");
  manualAfter.className = "add-machine-note";
  manualAfter.textContent = copyText("addMachine.ssh.after");
  manualPane.append(grid, fold({ title: copyText("machineCard.conn.more"), open: false, body: moreBody }), manualAfter);
  for (const f of [fName, fHost, fUser, fPort, fKey, fAddrs, fJump]) f.input.addEventListener("input", update);

  const manualCfg = (): RemoteHostConfig => ({
    ...blank(),
    label: fName.input.value.trim(),
    host: fHost.input.value.trim(),
    user: fUser.input.value.trim(),
    port: Number(fPort.input.value.trim()),
    keyPath: fKey.input.value.trim(),
    addresses: fAddrs.input.value.split("\n").map((x) => x.trim()).filter(Boolean),
    jump: fJump.input.value.trim(),
  });

  /** 这一框里要加的那几台，以及第一处错（`null` ⇒ 可以加）。 */
  const check = (): { cfgs: RemoteHostConfig[]; why: string | null } => {
    const taken = new Set(deps.existing().map(nameOf));
    if (tab === "manual") {
      const c = manualCfg();
      const portOk = Number.isInteger(c.port) && c.port >= 1 && c.port <= 65535;
      fPort.setError(fPort.input.value.trim() === "" || portOk ? null : copyText("addMachine.err.port"));
      const dup = nameOf(c) !== "" && taken.has(nameOf(c));
      fName.setError(dup ? copyText("addMachine.err.taken", { name: nameOf(c) }) : null);
      const why =
        c.host === "" ? copyText("addMachine.err.host")
        : c.user === "" ? copyText("addMachine.err.user")
        : !portOk ? copyText("addMachine.err.port")
        : dup ? copyText("addMachine.err.taken", { name: nameOf(c) })
        : null;
      return { cfgs: [c], why };
    }
    const cfgs = pickedCfgs();
    const seen = new Set<string>();
    let why: string | null = cfgs.length === 0 ? copyText("addMachine.err.none") : null;
    for (const r of rows) r.name?.setError(null);
    for (const c of cfgs) {
      const n = nameOf(c);
      if (n === "") why ??= copyText("addMachine.err.name");
      else if (taken.has(n) || seen.has(n)) {
        why ??= copyText("addMachine.err.taken", { name: n });
        rows.find((r) => !r.split && r.name?.input.value.trim() === n)?.name?.setError(copyText("addMachine.err.taken", { name: n }));
      }
      seen.add(n);
    }
    return { cfgs, why };
  };

  const handle = formDialog({
    title: copyText("addMachine.dialog.title"),
    action: copyText("addMachine.action.manual"),
    body,
    wide: true,
    blocked: () => check().why,
    dirty: () => tab === "manual" && (fHost.input.value !== "" || fName.input.value !== ""),
    submit: async () => {
      const { cfgs, why } = check();
      if (why) return why;
      return deps.add(cfgs);
    },
  });

  function update(): void {
    const { cfgs } = check();
    handle.setAction(
      tab === "manual" ? copyText("addMachine.action.manual") : copyText("addMachine.action.ssh", { n: cfgs.length }),
    );
    handle.refresh();
  }
  update();
  return handle.done;
}
