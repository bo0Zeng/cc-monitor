/**
 * 配置文件那一页的合成答法（`profiles-*`）：一份照设计稿那台的清单（cc · cct 两条根，各号基于它们），
 * 按 `world.profiles` 换几种样子（写错一段 · TOML 写坏 · 没有配置文件 · 刚迁移过 · 手改过 · 存的时候被别处改过）。
 * 形状照金样 `tests/__fixtures__/profiles.golden.json`；值怎么说照后端那几句（文案表同一份字）。
 */
import { copyText } from "../../../src/frontend/ui/copy-table";
import { Refuse, type OpHandler, type World } from "./types";

const HOME = "/home/user";
const PATH = `${HOME}/.cc-monitor/profiles.toml`;

type Item = { key: string; slot: string; vals: string[]; line: number };
interface P {
  name: string;
  from: string | null;
  own: Item[];
  agent: string[];
}

const label = (slot: string): string =>
  ({
    account: copyText("beProfile.slot.account"),
    tmux: copyText("beProfile.slot.tmux"),
    cwdIf: copyText("beProfile.slot.cwdIf"),
    cwd: copyText("beProfile.slot.cwd"),
    agent: copyText("beProfile.slot.agent"),
    args: copyText("beProfile.slot.args"),
  })[slot] ?? slot;

const said = (key: string, vals: string[]): string =>
  key === "base"
    ? copyText("beProfile.val.base")
    : key === "ccm-tmux" && !vals.length
      ? copyText("beProfile.val.tmuxAuto")
      : key === "ccm-tmux"
        ? copyText("beProfile.val.tmuxNamed", { name: vals[0] })
        : key === "cwd-if"
          ? copyText("beProfile.val.cwdIf", { at: vals[0], to: vals[1] })
          : key === "cwd"
            ? copyText("beProfile.val.cwd", { dir: vals[0] })
            : vals.join(" ");

const item = (key: string, vals: string[], line: number): Item => ({
  key,
  slot: key === "account" || key === "base" ? "account" : key === "ccm-tmux" ? "tmux" : key === "cwd-if" ? "cwdIf" : key === "ccm-agent" ? "agent" : key,
  vals,
  line,
});

function book(): P[] {
  let line = 3;
  const p = (name: string, from: string | null, ...own: [string, string[]][]): P => {
    line += 4;
    return { name, from, own: own.map(([k, v], i) => item(k, v, line + i)), agent: [] };
  };
  return [
    p("cc", null, ["cwd-if", ["~", "~/projects/notes"]]),
    p("alphacc", "cc", ["account", ["z"]]),
    p("betacc", "cc", ["account", ["b"]]),
    p("gammacc", "cc", ["account", ["q"]]),
    p("cct", "cc", ["ccm-tmux", []]),
    p("alphacct", "cct", ["account", ["z"]]),
    p("betacct", "cct", ["account", ["b"]]),
    p("workcct", "cct", ["account", ["work"]]),
    p("workcc", "cc", ["account", ["work"]]),
    p("codex", null, ["ccm-agent", ["codex"]]),
    p("pcc", null, ["base", []], ["cwd", ["/srv/p"]]),
  ];
}

const isFn = (name: string): boolean => name === "cc" || name === "cct";

function summary(p: P): string {
  const parts = p.own.map((o) => {
    if (o.key === "ccm-tmux" && p.from) return copyText("beProfile.said.plus", { val: said(o.key, o.vals) });
    if (["base", "ccm-tmux", "cwd-if", "cwd"].includes(o.key)) return said(o.key, o.vals);
    return copyText("beProfile.said.slot", { label: label(o.slot), val: said(o.key, o.vals) });
  });
  if (!p.from && !p.own.some((o) => o.slot === "tmux")) parts.unshift(copyText("rsAccountAliases.said.hereTerminal"));
  return parts.join(copyText("rsAccountAliases.said.sep"));
}

function formOf(p: P): Record<string, unknown> {
  const f: Record<string, unknown> = { name: p.name, from: p.from, account: null, tmux: null, cwdIf: null, cwd: null, agent: null, args: null, launcher: null, tmuxSize: null, detach: false, busRegister: false, busNote: null };
  for (const o of p.own) {
    if (o.key === "account") f.account = { kind: "account", name: o.vals[0] };
    if (o.key === "base") f.account = { kind: "base" };
    if (o.key === "ccm-tmux") f.tmux = o.vals.length ? { mode: "fixed", name: o.vals[0] } : { mode: "auto", name: "" };
    if (o.key === "cwd-if") f.cwdIf = [{ at: o.vals[0], to: o.vals[1] }];
    if (o.key === "cwd") f.cwd = o.vals[0];
    if (o.key === "ccm-agent") f.agent = o.vals[0];
  }
  return f;
}

/** 表单 ⇒ 自己那几项（假后端只认表单上露出来的那几格）。 */
function ownOfForm(f: Record<string, unknown>): Item[] {
  const out: Item[] = [];
  const a = f.account as { kind: string; name?: string } | null;
  if (a?.kind === "account") out.push(item("account", [a.name ?? ""], 0));
  if (a?.kind === "base") out.push(item("base", [], 0));
  const t = f.tmux as { mode: string; name: string } | null;
  if (t) out.push(item("ccm-tmux", t.mode === "auto" ? [] : [t.name], 0));
  for (const c of (f.cwdIf as { at: string; to: string }[] | null) ?? []) out.push(item("cwd-if", [c.at, c.to], 0));
  if (typeof f.cwd === "string") out.push(item("cwd", [f.cwd], 0));
  if (typeof f.agent === "string") out.push(item("ccm-agent", [f.agent], 0));
  return out;
}

function state(w: World): string {
  return w.profiles ?? "normal";
}

function current(w: World): P[] {
  const b = book();
  if (state(w) === "broken") {
    const x = b.find((p) => p.name === "betacct")!;
    // 认不得的键不是一项（后端记成这一段的坏处，见 `problemOf`）。
    x.own = [];
  }
  return b;
}

const problemOf = (w: World, p: P, all: P[]): { line: number; message: string } | null => {
  if (state(w) !== "broken") return null;
  if (p.name === "betacct") return { line: 23, message: copyText("beProfile.chain.broken", { name: "betacct", e: copyText("beProfile.at.line", { line: "23", e: copyText("beProfile.value.unknown", { key: "tmux-sise" }) }) }) };
  void all;
  return null;
};

function row(w: World, p: P, all: P[]): Record<string, unknown> {
  const problem = problemOf(w, p, all);
  return {
    name: p.name,
    from: p.from,
    own: p.own,
    agent: p.agent,
    usable: problem === null,
    problem,
    kind: isFn(p.name) ? "function" : "link",
    functionWhy: isFn(p.name) ? copyText("rsShellDialect.posix.nameTakenPath", { name: p.name, cand: `/usr/bin/${p.name}` }) : null,
    functionLine: isFn(p.name) ? copyText("beProfile.function.at", { path: `${HOME}/.cc-monitor/aliases.sh`, line: `${p.name}() { ccm @${p.name} "$@"; }` }) : null,
    said: problem ? "" : summary(p),
    form: formOf(p),
    accountShape: p.own.length === 1 && p.own[0].key === "account" ? { account: p.own[0].vals[0], tmux: chain(all, p.name).some((q) => q.own.some((o) => o.slot === "tmux")) } : null,
  };
}

function chain(all: P[], name: string): P[] {
  const out: P[] = [];
  let cur = all.find((p) => p.name === name);
  while (cur && !out.includes(cur)) {
    out.unshift(cur);
    cur = cur.from ? all.find((p) => p.name === cur!.from) : undefined;
  }
  return out;
}

function rows(all: P[], name: string): Record<string, unknown>[] {
  const ch = chain(all, name);
  const out: Record<string, unknown>[] = [];
  ch.forEach((p, i) => {
    for (const o of p.own) {
      const later = ch.slice(i + 1).reverse().find((q) => q.own.some((x) => x.slot === o.slot) && o.slot !== "cwdIf");
      out.push({ key: o.key, slot: o.slot, label: label(o.slot), vals: o.vals, said: said(o.key, o.vals), from: p.name, overriddenBy: later?.name ?? null });
    }
  });
  return out;
}

const LINE: Record<string, string> = {
  betacct: "tmux new-session -s projects/notes-cc … ccm -- --cwd ~/projects/notes --account b --ccm-tmux",
  cc: "cd ~/projects/notes && exec claude",
};

export function profilesOps(): Record<string, OpHandler> {
  return {
    "profiles-read": (_o, _r, w) => {
      const all = current(w);
      const st = state(w);
      const empty = st === "empty";
      const seed = [
        { name: "cc", from: null, own: [], agent: [] },
        { name: "cct", from: "cc", own: [item("ccm-tmux", [], 3)], agent: [] },
      ];
      return {
        home: HOME,
        path: PATH,
        exists: !empty,
        fingerprint: empty ? null : "412-0123456789abcdef",
        modified: empty ? null : st === "edited" ? Math.floor(Date.now() / 1000) : 1791380000,
        fileProblem: st === "syntax" ? { line: 12, message: copyText("beProfile.file.syntax", { e: "invalid table header" }) } : null,
        profiles: empty || st === "syntax" ? [] : all.map((p) => row(w, p, all)),
        seed: empty ? seed.map((p) => row(w, p, seed)) : [],
        migrated: st === "migrated" ? { count: 11, path: PATH, skipped: [copyText("beProfile.migrate.restToCcm", { name: "cca" })] } : null,
        binDir: `${HOME}/.cc-monitor/bin`,
        tmux: true,
        accounts: ["b", "z", "q", "work"],
      };
    },
    "profiles-resolve": (_o, req, w) => {
      let all = current(w);
      const edit = req.edit as Record<string, unknown> | null;
      const name = String(edit?.name || req.name);
      if (edit) {
        const mine: P = { name, from: (edit.from as string | null) ?? null, own: ownOfForm(edit), agent: [] };
        all = [...all.filter((p) => p.name !== req.name), mine];
      }
      const ch = chain(all, name).map((p) => p.name);
      return { chain: ch, rows: rows(all, name), line: LINE[name] ?? `ccm @${name}`, lineError: null, problem: null };
    },
    "profiles-impact": (_o, req, w) => {
      const all = current(w);
      const c = (req.changes as Record<string, unknown>[])[0] ?? {};
      const target = String(c.was ?? c.name ?? "");
      const kids = all.filter((p) => p.from === target).map((p) => p.name);
      const form = c.form as Record<string, unknown> | undefined;
      const t = form?.tmux as { mode: string; name: string } | null | undefined;
      const after = c.op === "remove" ? "" : t && t.mode !== "auto" ? copyText("beProfile.val.tmuxNamed", { name: t.name }) : copyText("beProfile.val.tmuxAuto");
      const before = copyText("beProfile.val.tmuxAuto");
      if (before === after) return { affected: [] };
      return { affected: kids.map((name) => ({ name, changes: [{ slot: "tmux", label: label("tmux"), before, after }], problem: null })) };
    },
    "profiles-bases": (_o, req, w) => {
      const all = current(w);
      const name = String(req.name);
      const loops = (n: string): boolean => chain(all, n).some((p) => p.name === name) && n !== name;
      const out = all.filter((p) => p.name === name || !loops(p.name)).map((p) => ({ name: p.name, from: p.from, said: summary(p), selectable: p.name !== name }));
      if (!all.some((p) => p.name === name) && name) out.push({ name, from: null, said: "", selectable: false });
      return { bases: out };
    },
    "profiles-write": (_o, _r, w) => {
      if (state(w) === "stale") throw new Refuse("stale", copyText("rsAccountAliases.install.changedElsewhere", { path: PATH }));
      return { wrote: true, fingerprint: "412-0123456789abcdef", modified: 1791380000, reload: null };
    },
  };
}
