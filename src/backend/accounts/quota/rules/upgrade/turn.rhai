
// ── 换谁、什么时候换（照权重排；同权照号列表；强制切换阈值；耗尽后启用 ＋ 启用延迟 ＋ 单次用量上限）──
// 权重由上面的 weight(a, left, mins) 算：a 是这个号的事实，left 是余量，mins 是离 5 小时重置的分钟数（窗口没开 ⇒ ()）。

fn is_fb(r) { r != () && r.fallback == true }

fn mins_of(a, now) {
  if a.resets.w5h == () { return (); }
  let m = (a.resets.w5h - now).to_float() / 60.0;
  if m < 1.0 { 1.0 } else { m }
}

// 两个号的权重几时交叉：f(x) 从 lo 到 hi 由假变真（单调），回第一次为真的 x（精到 step）；hi 处也不真 ⇒ ()。
// 只对这几份权重（离重置的分钟 · 余量的单调函数）成立，所以写在策略里，不在积木库。
fn cross_at(lo, hi, step, f) {
  if lo >= hi || !f.call(hi) { return (); }
  if f.call(lo) { return lo; }
  let a = lo;
  let b = hi;
  while b - a > step {
    let m = (a + b) / 2.0;
    if f.call(m) { b = m; } else { a = m; }
  }
  b
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

  // 权重与先后：耗尽后启用的号排最后。
  let left = #{};
  let mins = #{};
  let pri = #{};
  let main = [];
  let fb = [];
  for id in ids {
    left[id] = lib::left_of(s, r_of[id], a_of[id], lift, clock);
    mins[id] = mins_of(a_of[id], now);
    if !has[id] { continue; }
    pri[id] = weight(a_of[id], left[id], mins[id]).to_float();
    if is_fb(r_of[id]) { fb.push(id); } else { main.push(id); }
  }
  let next = lib::rank(main, pri, s.close);
  for id in lib::rank(fb, pri, s.close) { next.push(id); }

  // 强制切换阈值：在用的号有额度时，重置比它早、权重超出它这一比例的号直接换过去（只看先重置的，免得来回换）。
  let k = if s.force != () { 1.0 + s.force.to_float() / 100.0 } else { () };
  let why = #{};
  for id in next { why[id] = if is_fb(r_of[id]) { "其余账号无可用额度" } else { "权重最高" }; }
  let order = [];
  if !ids.contains(cur) || !has[cur] || is_fb(r_of[cur]) {
    order = next;
  } else {
    let take = ();
    if k != () && mins[cur] != () {
      for id in next {
        if id == cur || is_fb(r_of[id]) || mins[id] == () { continue; }
        if mins[id] < mins[cur] && pri[id] > pri[cur] * k { take = id; break; }
      }
    }
    if take != () { order.push(take); why[take] = "强制切换"; } else { why[cur] = "未达用量上限"; }
    order.push(cur);
    for id in next { if !order.contains(id) { order.push(id); } }
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

  // 叫醒：分段边界 · 启用延迟到点 · 要用的号读数到上限 · 单次用量上限那一读数 · 强制切换开着时两号权重交叉。
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
  if nxt != () && k != () && !is_fb(r_of[nxt]) && a_of[nxt].kind != "api" {
    let an = a_of[nxt];
    let ln = left[nxt];
    let mn = mins[nxt];
    let watch = false;
    let step = if mn == () { 0.01 } else { 1.0 };   // 要用的号窗口还没开：一用就开，开了再算
    for id in ids {
      if id == nxt || !has[id] || is_fb(r_of[id]) || mins[id] == () || (mn != () && mins[id] >= mn) { continue; }
      watch = true;
      if mn == () { continue; }
      let ai = a_of[id];
      let li = left[id];
      let mi = mins[id];
      // 时刻：两边余量照此刻，离重置的分钟都照走；x 分钟后它的权重超出要用的号这一比例
      let x = cross_at(0.0, mi - 1.0, 1.0, |x| weight(ai, li, mi - x).to_float() > weight(an, ln, mn - x).to_float() * k);
      if x != () && x > 0.0 { ats.push(now + (x * 60.0).to_int() + 1); }
      // 读数：时刻不走，要用的号再用掉多少就被它超出
      let pi = weight(ai, li, mi).to_float();
      let u = cross_at(0.0, ln, 0.05, |d| pi > weight(an, ln - d, mn).to_float() * k);
      if u != () {
        let half = u / 2.0;
        if half < step { step = if half > 0.05 { half } else { 0.05 }; }
      }
    }
    if watch { when.push(#{ acct: nxt, w: "5h", used_at_least: an.used.w5h + step }); }
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
