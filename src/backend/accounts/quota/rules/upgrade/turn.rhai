
// ── 换谁、什么时候换（照权重排；同权照号列表；强制切换阈值；耗尽后启用 ＋ 启用延迟 ＋ 单次用量上限）──
// 权重由上面的 weight(a, left, mins) 算：a 是这个号的事实，left 是余量，mins 是离 5 小时重置的分钟数（窗口没开 ⇒ ()）。

fn is_fb(r) { r != () && r.fallback == true }

fn mins_of(a, now) {
  if a.resets.w5h == () { return (); }
  let m = (a.resets.w5h - now).to_float() / 60.0;
  if m < 1.0 { 1.0 } else { m }
}

// x 分钟以后（离重置的分钟都照走）、号 u 的余量换成 lu 时，照规则排出来的先后：
//   权重高的在前（权重容差以内同权、照号列表）；耗尽后启用的号排最后；
//   在用的号有额度且不是耗尽后启用的 ⇒ 留着，除非开了强制切换、有号重置比它早且权重超出它这一比例（只看先重置的，免得来回换）。
// 这一判就是 x ＝ 0、不换余量；约下次再来问时拿同一个函数问「再过多久 · 再用掉多少就换」。
// 照方法调：this 是各号的事实（#{号: 事实}，几份里最大的那份，按引用传，二分里调几十次也不拷）；
// g ＝ #{ids, fb, has, left, mins, close, force}（fb · has · left · mins 都是 #{号: …}）。
fn order_of(g, cur, x, u, lu) {
  let pri = #{};
  let main = [];
  let fb = [];
  for id in g.ids {
    if !g.has[id] { continue; }
    let l = if id == u { lu } else { g.left[id] };
    let m = g.mins[id];
    if m != () && x > 0.0 { m -= x; if m < 1.0 { m = 1.0; } }
    pri[id] = weight(this[id], l, m).to_float();
    if g.fb[id] { fb.push(id); } else { main.push(id); }
  }
  let next = lib::rank(main, pri, g.close);
  for id in lib::rank(fb, pri, g.close) { next.push(id); }
  if !g.ids.contains(cur) || !g.has[cur] || g.fb[cur] { return #{ order: next, take: () }; }
  let k = if g.force != () { 1.0 + g.force.to_float() / 100.0 } else { () };
  let take = ();
  let mc = g.mins[cur];
  if k != () && mc != () {
    for id in next {
      if id == cur || g.fb[id] || g.mins[id] == () { continue; }
      if g.mins[id] < mc && pri[id] > pri[cur] * k { take = id; break; }
    }
  }
  let order = [];
  if take != () { order.push(take); }
  order.push(cur);
  for id in next { if !order.contains(id) { order.push(id); } }
  #{ order: order, take: take }
}

// 号 u 在用：x 分钟以后、它的余量换成 lu 时还排不排第一（不排 ⇒ 那时会换）。照方法调，this 同 order_of。
fn moved(g, u, x, lu) {
  let o = this.order_of(g, u, x, u, lu).order;
  o.len() == 0 || o[0] != u
}

fn decide(ctx) {
  let s = ctx.settings;
  let now = ctx.now;
  let clock = ctx.clock;
  let cur = ctx.current;
  let a_of = #{};
  for a in ctx.accounts { a_of[a.id] = a; }
  let ids = [];
  let r_of = #{};
  for r in s.accounts {
    let id = if r.start == true { ctx.start } else { r.id };
    if id == () || ids.contains(id) || a_of[id] == () { continue; }
    ids.push(id);
    r_of[id] = r;
  }

  // 有额度：先照上限看；一个都没有 ⇒ 软上限的号当用满为止再看。
  let has = #{};
  let lift = true;
  for id in ids {
    has[id] = lib::has_quota(s, r_of[id], a_of[id], false, clock);
    if has[id] { lift = false; }
  }
  if lift { for id in ids { has[id] = lib::has_quota(s, r_of[id], a_of[id], true, clock); } }

  // 耗尽后启用：其余的号都没额度持续满启用延迟，才算有额度；起点记在 memo。
  let memo = if ctx.memo != () { ctx.memo } else { #{} };
  let others = false;
  for id in ids { if !is_fb(r_of[id]) && has[id] { others = true; } }
  memo.outSince = lib::delay_start(memo.outSince, !others, now);
  if others { memo.stint = (); }
  let ready_at = ();
  for id in ids {
    if !is_fb(r_of[id]) || !has[id] || others { continue; }
    if !lib::delay_over(memo.outSince, r_of[id].delay, now) {
      has[id] = false;
      let t = memo.outSince + (if r_of[id].delay != () { r_of[id].delay } else { 0 }) * 60;
      if ready_at == () || t < ready_at { ready_at = t; }
    }
  }

  // 单次用量上限：每次换到耗尽后启用的号，从那一刻的读数起记它 5 小时用量涨了多少；涨够了这一段里不再用它。
  let stint = if memo.stint != () { memo.stint } else { #{} };
  let spent = #{};
  for id in ids {
    let r = r_of[id];
    if !is_fb(r) || r.stint == () || stint[id] == () { continue; }
    let e = stint[id];
    let a = a_of[id];
    if a.resets.w5h == e.reset { if a.used.w5h > e.last { e.rose += a.used.w5h - e.last; } } else { e.rose += a.used.w5h; }
    e.last = a.used.w5h;
    e.reset = a.resets.w5h;
    stint[id] = e;
    if e.rose >= r.stint.to_float() { has[id] = false; spent[id] = true; }
  }

  // 权重与先后（见 order_of）。
  let left = #{};
  let mins = #{};
  for id in ids {
    left[id] = lib::left_of(s, r_of[id], a_of[id], lift, clock);
    mins[id] = mins_of(a_of[id], now);
  }
  let fbs = #{};
  for id in ids { fbs[id] = is_fb(r_of[id]); }
  let g = #{ ids: ids, fb: fbs, has: has, left: left, mins: mins, close: s.close, force: s.force };
  let o = a_of.order_of(g, cur, 0.0, (), ());
  let order = o.order;
  let why = #{};
  for id in order { why[id] = if is_fb(r_of[id]) { "其余账号无可用额度" } else { "权重最高" }; }
  if ids.contains(cur) && has[cur] && !is_fb(r_of[cur]) {
    if o.take != () { why[o.take] = "强制切换"; } else { why[cur] = "未达用量上限"; }
  }
  for id in ids {
    if why[id] != () { continue; }
    let a = a_of[id];
    why[id] = if !a.sendable { "此刻发不出去" } else if spent[id] == true { "已达单次用量上限" } else if is_fb(r_of[id]) && lib::has_quota(s, r_of[id], a, lift, clock) { "启用延迟中" } else { "已达用量上限" };
  }

  // 换到耗尽后启用的号、它带单次用量上限又还没记账 ⇒ 从此刻的读数记起。
  let nxt = if order.len() > 0 { order[0] } else { () };
  if nxt != () && is_fb(r_of[nxt]) && r_of[nxt].stint != () && stint[nxt] == () {
    let a = a_of[nxt];
    stint[nxt] = #{ rose: 0.0, last: a.used.w5h, reset: a.resets.w5h };
  }
  memo.stint = if stint.len() > 0 { stint } else { () };

  // 叫醒：分段边界 · 启用延迟到点 · 要用的号读数到上限 · 单次用量上限那一读数 · 权重交叉、要用的号不再排第一的那一刻（时刻 · 读数各一）。
  let at = ();
  let when = [];
  let ats = [];
  for id in ids {
    for w in ["5h", "7d"] {
      let e = lib::next_edge(lib::segs_of(s, r_of[id], w), clock);
      if e != () { ats.push(now - now % 60 + e * 60); }
    }
  }
  ats.push(ready_at);
  if nxt != () && a_of[nxt].kind != "api" {
    let r = r_of[nxt];
    for w in ["5h", "7d"] { when.push(#{ acct: nxt, w: w, used_at_least: lib::cap_of(s, r, w, lift, clock) }); }
    if stint[nxt] != () && spent[nxt] != true {
      let e = stint[nxt];
      when.push(#{ acct: nxt, w: "5h", used_at_least: e.last + r.stint.to_float() - e.rose });
      if a_of[nxt].resets.w5h != () { ats.push(a_of[nxt].resets.w5h - 60); }
    }
  }
  // 权重会交叉的那一刻：下一判在用的是 nxt，问 order_of「再过多久 · 再用掉多少它就不排第一」。
  // 只在可能换时算：nxt 是耗尽后启用的（照权重和别的耗尽后启用的号排），或开了强制切换；且还有别的有额度的号。
  if nxt != () && order.len() > 1 && a_of[nxt].kind != "api" && (is_fb(r_of[nxt]) || s.force != ()) {
    let an = a_of[nxt];
    if mins[nxt] == () {
      // 要用的号窗口还没开：一用就开，开了再算
      when.push(#{ acct: nxt, w: "5h", used_at_least: an.used.w5h + 0.01 });
    } else if a_of.moved(g, nxt, 0.0, left[nxt]) {
      // 换过去下一判就又不排第一（权重容差比强制切换阈值大时会有）：下一秒就来问
      ats.push(now + 1);
    } else {
      // 权重随不随时间变（离重置的分钟进不进权重）
      let timed = false;
      for id in ids {
        if !has[id] || mins[id] == () { continue; }
        if weight(a_of[id], left[id], mins[id]).to_float() != weight(a_of[id], left[id], mins[id] + 1.0).to_float() { timed = true; }
      }
      // 按时刻：余量照此刻，离重置的分钟都照走；上界是有额度的号里最早重置的那一刻（重置本身是节点）
      if timed {
        let hi = ();
        for id in ids { if has[id] && mins[id] != () && (hi == () || mins[id] < hi) { hi = mins[id]; } }
        if a_of.moved(g, nxt, hi, left[nxt]) {
          let lo = 0.0;
          while hi - lo > 0.1 {
            let m = (lo + hi) / 2.0;
            if a_of.moved(g, nxt, m, left[nxt]) { hi = m; } else { lo = m; }
          }
          ats.push(now + (hi * 60.0).to_int() + 1);
        }
      }
      // 按读数：时刻不走，两窗各自算 nxt 这一窗再用掉多少就换（余量是两窗取小，各窗单独算才准）。
      // 权重不随时间变 ⇒ 直接约到那一读数；也随时间变 ⇒ 约到一半，到了再算，最后一截不到 0.1% 时约到那一读数。
      let r = r_of[nxt];
      let l5 = lib::cap_of(s, r, "5h", lift, clock) - an.used.w5h;
      let l7 = lib::cap_of(s, r, "7d", lift, clock) - an.used.w7d;
      for w in ["5h", "7d"] {
        let room = if w == "5h" { l5 } else { l7 };
        let keep = if w == "5h" { l7 } else { l5 };
        let hi = room - 0.01;
        if hi <= 0.0 { continue; }
        let lu = if room - hi < keep { room - hi } else { keep };
        if !a_of.moved(g, nxt, 0.0, lu) { continue; }
        let lo = 0.0;
        while hi - lo > 0.05 {
          let m = (lo + hi) / 2.0;
          let lu = if room - m < keep { room - m } else { keep };
          if a_of.moved(g, nxt, 0.0, lu) { hi = m; } else { lo = m; }
        }
        let h = if timed && hi > 0.1 { hi / 2.0 } else { hi };
        let u = if w == "5h" { an.used.w5h } else { an.used.w7d };
        when.push(#{ acct: nxt, w: w, used_at_least: u + h });
      }
    }
  }
  for t in ats { if t != () && t > now && (at == () || t < at) { at = t; } }

  let out = #{ order: order, why: why, wake: #{ at: at, when: when }, memo: memo };
  if order.len() == 0 {
    let live = [];
    for id in ids { if spent[id] != true { live.push(id); } }
    out.hold_until = lib::hold_from(s, r_of, a_of, live, now, clock, [at, ready_at]);
  }
  out
}
