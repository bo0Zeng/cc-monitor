// 引擎给策略的共用小函数（随引擎走，只读）。脚本里写 lib::名字(…) 调。
// 都是纯函数：同样的输入同样的结果；不碰文件、网络、钟。
//
// 约定的参数名（策略在 meta 里声明了，这几个小函数和合图才认得）：
//   每号  line   类型 line   用到多少换：数（两窗同一条）· #{"5h": n, "7d": n} · () ＝ 用满才换
//   每号  slots  类型 slots  按时段另算：[#{at: "HH:MM-HH:MM", line: n}]，line 0 ＝ 这段不用
//   整份  delay  类型 minutes 换号前延迟
//   整份  all_out 类型 choice ["stop", "go"] 都到上限后停下 / 接着用
// 小函数回的 why 是引擎的字（键以 @ 开头，字在 table.json 的 beRotation.lib.*）；describe_common 的片段键以 @d_ 开头（beRotation.desc.*）。

// "17:00-02:00" 含不含 clock（一天里第几分钟，配置的时区）；跨零点照算。
fn in_slot(at, clock) {
  let s = at.split("-");
  let a = hm(s[0]);
  let b = hm(s[1]);
  if a <= b { clock >= a && clock < b } else { clock >= a || clock < b }
}

fn hm(t) {
  let p = t.split(":");
  parse_int(p[0]) * 60 + parse_int(p[1])
}

// 此刻落在哪个时段：那一段 · ()
fn slot_now(a, ctx) {
  if type_of(a.params.slots) != "array" { return (); }
  for s in a.params.slots {
    if in_slot(s.at, ctx.clock) { return s; }
  }
  ()
}

// 此刻对这个号生效的线：#{w5h, w7d}（点；100 ＝ 用满才换；0 ＝ 这段不用）。
fn line_now(a, ctx) {
  let l = a.params.line;
  let s = slot_now(a, ctx);
  if s != () { l = s.line; }
  if l == () { return #{ w5h: 100, w7d: 100 }; }
  if type_of(l) == "map" { return #{ w5h: l["5h"] ?? 100, w7d: l["7d"] ?? 100 }; }
  #{ w5h: l, w7d: l }
}

// 离此刻的线还剩几点（两窗取小；没开的窗用量算 0）。
fn left(a, ctx) {
  let ln = line_now(a, ctx);
  let l5 = ln.w5h - a.w5h.used;
  let l7 = ln.w7d - a.w7d.used;
  if l5 < l7 { l5 } else { l7 }
}

// 能不能挑：事实（被拒 · 用满 · 超额在兜 · 要重新登录，引擎判的）＋ 线（这个号的 line · slots）。
// 回 #{ok, why, back, left}：why 是引擎的字，back 是几点能再挑（说不出 ⇒ ()）。
fn usable(a, ctx) {
  if !a.ok {
    return #{ ok: false, why: #{ k: "@" + a.why_not, at: a.back }, back: a.back, left: 0.0 };
  }
  if a.kind == "api" { return #{ ok: true, why: (), back: (), left: 100.0 }; }
  let ln = line_now(a, ctx);
  if ln.w5h == 0 {
    let s = slot_now(a, ctx);
    let end = hm(s.at.split("-")[1]);
    let back = ctx.now + ((end - ctx.clock + 1440) % 1440) * 60;
    return #{ ok: false, why: #{ k: "@off", at: back }, back: back, left: 0.0 };
  }
  if a.w5h.open && a.w5h.used >= ln.w5h {
    return #{ ok: false, why: #{ k: "@line", w: "5h", n: ln.w5h, at: a.w5h.resets }, back: a.w5h.resets, left: ln.w5h - a.w5h.used };
  }
  if a.w7d.open && a.w7d.used >= ln.w7d {
    return #{ ok: false, why: #{ k: "@line", w: "7d", n: ln.w7d, at: a.w7d.resets }, back: a.w7d.resets, left: ln.w7d - a.w7d.used };
  }
  #{ ok: true, why: (), back: (), left: left(a, ctx) }
}

// 每个号查一遍 usable：回 #{号: #{ok, why, back, left}}。
fn checks(ctx) {
  let c = #{};
  for a in ctx.accounts { c[a.id] = usable(a, ctx); }
  c
}

// 换号前延迟：有号排到在用的号上面，连着 mins 分都在上面才让它上去。
// 等的时候在用的号先顶着，那个号的 why 前面加一句「几点换过去」；备忘写进 out["@memo"].delay，下一判从 ctx.memo 读回。
fn delay(ctx, out, mins) {
  if mins == () || mins <= 0 { return out; }
  let cur = ctx.current;
  if !(cur in out) || out[cur].p == () { return out; }
  let top = ();
  let tp = ();
  for a in ctx.accounts {
    if !(a.id in out) || !a.ok { continue; }
    let p = out[a.id].p;
    if p == () { continue; }
    if tp == () || p > tp { top = a.id; tp = p; }
  }
  if top == () || top == cur || tp <= out[cur].p { return out; }
  let m = ctx.memo?.delay;
  let since = if m != () && m.to == top { m.since } else { ctx.now };
  if ctx.now - since >= mins * 60 { return out; }
  let at = since + mins * 60;
  out[cur] = #{ p: tp + 1, why: out[cur].why };
  out[top] = #{ p: tp, why: #{ k: "@pending", at: at, n: mins, then: out[top].why } };
  let mm = out["@memo"] ?? #{};
  mm.delay = #{ to: top, since: since };
  out["@memo"] = mm;
  out
}

// 都到上限后接着用：一个肯挑的号都没有时，把事实上还能用的号（没被拒 · 没用满 · 登录着）排上，在用的先。
fn keep_going(ctx, out) {
  for a in ctx.accounts {
    if a.ok && a.id in out && out[a.id].p != () { return out; }
  }
  for a in ctx.accounts {
    if !a.ok { continue; }
    let p = if a.id == ctx.current { 1 } else { 0 };
    out[a.id] = #{ p: p, why: #{ k: "@going" } };
  }
  out
}

// 都到上限后：照 all_out 参数。停下 ⇒ 不动（没号可挑，引擎停发到最早的 back）；接着用 ⇒ keep_going。
fn all_out(ctx, out) {
  if ctx.params.all_out == "go" { keep_going(ctx, out) } else { out }
}

// 描述里线 · 时段 · 延迟 · 都到上限后这几段（引擎的字）。accounts 是配置里的号，每个带 kind 与 params。
// 线：最多的那个数写一次（「到 90% 换」），别的号单写；按量号没有窗口，不写。
fn describe_common(p, accounts) {
  let out = [#{ k: "@d_n", n: accounts.len() }];
  let subs = accounts.filter(|a| a.kind != "api");
  let best = ();
  let best_n = 0;
  for a in subs {
    let n = subs.filter(|b| b.params.line == a.params.line).len();
    if n > best_n { best = a.params.line; best_n = n; }
  }
  if best_n > 0 { out.push(if best == () { "@d_full" } else { #{ k: "@d_line", n: best } }); }
  for a in subs {
    if a.params.line == best { continue; }
    out.push(if a.params.line == () { #{ k: "@d_acctFull", acct: a.id } } else { #{ k: "@d_acctLine", acct: a.id, n: a.params.line } });
  }
  for a in subs {
    if type_of(a.params.slots) != "array" { continue; }
    for s in a.params.slots {
      out.push(if s.line == 0 { #{ k: "@d_acctOff", acct: a.id, at: s.at } } else { #{ k: "@d_acctSlot", acct: a.id, at: s.at, n: s.line } });
    }
  }
  if p.delay != () && p.delay > 0 { out.push(#{ k: "@d_delay", n: p.delay }); }
  if p.all_out == "go" { out.push("@d_go"); } else if p.all_out == "stop" { out.push("@d_stop"); }
  out
}
