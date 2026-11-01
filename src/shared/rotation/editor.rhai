// 每号的用量上限、切入、切出照设置；智能切换关着时，用满或被拒才换号，几个号都能用时照号牌顺序。
// 切入：默认 ＝ 轮到它才用；强制切入 ＝ 有额度就切到它，几个强制切入的照号牌顺序、靠前的有额度就切回；延迟切入 ＝ 其余账号都用完才切到它，延迟时长内有账号恢复就先停发、到点用那个账号。
// 切出：默认 ＝ 到上限或被拒才换走；强制切出 ＝ 用着它时，其余账号余量合计达到切出条件（或一有额度）即换走，换走后排在其余账号后面。
// 智能切换：往后推演几种走法（留着 · 换到每一个能用的账号 · 给空着的账号此刻开窗），照几条用速路径各推一遍，比期望代价：卡住发不出的量 ＋ 换号重写提示缓存的一成 ＋ 用掉的周额度（周重置前用不完的不算）− 视野末还用得上的余量。
// 要换号时挑期望代价最小的，一样小的先用最先作废的、再照号牌；在用的账号还能用时，换过去少的超过换号代价的一成与两倍估计误差才换；空着的账号此刻开窗比不开少的超过一句话的代价，就发一句开窗。
// 量不到这个会话的用速时不推演：要换号时先用最先作废的那份额度（没开窗的、周上限卡着的放后面），在用的账号还能用就不提前换。
// 「现在用」选的账号与手动排的先后压过上面这些：在用的账号只在用不了时换。

/*--- rotation
{
  "format": 5,
  "settings": { "accounts": [], "smart": true },
  "form": [
    { "key": "caps", "kind": "daySegments", "scope": "each", "windows": ["5h", "7d"], "follow": true,
      "min": 0, "max": 100, "unit": "percent", "default": 90,
      "label": { "zh": "用量上限" }, "help": { "zh": "账号用量的上限，可按时段分段；0% 表示该时段停用；7 天未另设时同 5 小时" } },
    { "key": "in", "kind": "choice", "scope": "each", "default": "default",
      "choices": [
        { "v": "default", "label": { "zh": "默认" } },
        { "v": "force", "label": { "zh": "强制切入" }, "mark": { "glyph": "↺" } },
        { "v": "delay", "label": { "zh": "延迟切入" }, "mark": { "dashed": true } } ],
      "label": { "zh": "切入" },
      "help": { "zh": "默认：轮到它才用。强制切入：有额度就切到它；几个强制切入的账号都有额度时照号牌顺序，靠前的有额度就切回。延迟切入：其余账号都用完以后才切到它" } },
    { "key": "wait", "kind": "minutes", "scope": "each", "min": 0, "max": 300, "default": 0, "showIf": { "key": "in", "v": "delay" },
      "label": { "zh": "延迟" },
      "help": { "zh": "其余账号都用完时，这一时长内有账号恢复额度就先停发、到点用那个账号；没有就马上切入。0 表示用完就切" } },
    { "key": "out", "kind": "choice", "scope": "each", "default": "default",
      "choices": [
        { "v": "default", "label": { "zh": "默认" } },
        { "v": "force", "label": { "zh": "强制切出" }, "mark": { "glyph": "⇥" } } ],
      "label": { "zh": "切出" },
      "help": { "zh": "默认：用着它时，到上限或被拒才换走。强制切出：用着它时，满足切出条件即换走" } },
    { "key": "outTotal", "kind": "percent", "scope": "each", "min": 0, "max": 1000, "default": 400, "showIf": { "key": "out", "v": "force" },
      "nullable": { "zh": "一有额度" }, "lead": { "zh": "其余号余量合计达到" },
      "label": { "zh": "切出条件" },
      "help": { "zh": "其余账号余量之和，单位为百分点；余量 ＝ 上限 − 用量，两窗取小，此刻不可用的账号按 0 计" } },
    { "key": "smart", "kind": "toggle", "scope": "all", "default": true,
      "label": { "zh": "智能切换" }, "help": { "zh": "往后推演各个账号：要换号时挑卡住与代价最少的；在用的账号还能用时，现在换过去少卡的超过换号代价与估计误差才换；空着的账号此刻开窗能少卡就先开窗。额度宽裕时很少提前换" } }
  ]
}
---*/

// 一个号某一窗此刻的上限：那一窗没设 ⇒ 同 5 小时；都没设 ⇒ 90（声明里的默认）。
fn cap_of(r, w, clock) {
  let c = r.caps;
  if c != () && c[w] != () { return lib::cap_or(c[w], clock, 90.0); }
  if c != () && c["5h"] != () { return lib::cap_or(c["5h"], clock, 90.0); }
  90.0
}

// 用不了的号几点恢复（被拒到期 · 到上限那几窗的重置，取晚的；周窗只看有周上限的号）；说不出 ⇒ ()。
fn back_at(a, r, now, clock) {
  let t = now;
  if a.refusedUntil != () && a.refusedUntil > now { t = a.refusedUntil; }
  for w in [["5h", "w5h"], ["7d", "w7d"]] {
    if w[0] == "7d" && a.weekly != true { continue; }
    let c = cap_of(r, w[0], clock);
    if lib::used_of(a, w[1], now) < c { continue; }
    let at = a.resets[w[1]];
    if c <= 0.0 || at == () { return (); }
    if at > t { t = at; }
  }
  if t > now { t } else { () }
}

// 折合输入 token 照「万」写，一位小数（why 里的数）。
fn wan(x) { (x / 1000.0).round() / 10.0 }

// 推演过的两个号谁在前（[期望代价, 最先作废的键, 号牌]）：代价差过 1 tok 照代价，一样小的先最先作废、再照号牌。
fn before(x, y) {
  if (x[0] - y[0]).abs() > 1.0 { return if x[0] < y[0] { -1 } else { 1 }; }
  if x[1] != y[1] { return if x[1] < y[1] { -1 } else { 1 }; }
  x[2] - y[2]
}

fn decide(ctx) {
  let s = ctx.settings;
  let now = ctx.now;
  let clock = ctx.clock;
  let cur = ctx.current;
  let memo = if ctx.memo != () { ctx.memo } else { #{} };
  let smart = s.smart != false;
  let sess = if ctx.session != () { ctx.session } else { #{} };

  // 号列表（号牌顺序）里此刻有的号：have ＝ 号 ⇒ 事实，rows ＝ 号 ⇒ 它那一行，pos ＝ 号 ⇒ 第几个。
  // 下面的数据都按「号 ⇒ 一个数」摊平放：沙箱的表上限按嵌套合计，号多时整张表放不下。
  let have = #{};
  for a in ctx.accounts { have[a.id] = a; }
  let ids = [];
  let rows = #{};
  let pos = #{};
  for r in s.accounts {
    let id = if r.start == true { ctx.start } else { r.id };
    if id == () || have[id] == () || rows[id] != () { continue; }
    pos[id] = ids.len();
    ids.push(id);
    rows[id] = r;
  }

  // 每号此刻那一份额度（余量 · 作废时刻 · 周上限卡着 · 开没开窗 · 周剩的）· 有没有额度 · 切入。
  let p = #{}; let amt = #{}; let ok = #{}; let tier = #{};
  for id in ids {
    let a = have[id];
    let r = rows[id];
    p[id] = lib::piece(a, cap_of(r, "5h", clock), cap_of(r, "7d", clock), now);
    amt[id] = p[id].amt;
    ok[id] = a.sendable == true && amt[id] > 0.0;
    tier[id] = if r["in"] != () { r["in"] } else { "default" };
  }

  // 最先作废的先后键（越小越先）：开着窗的看离作废几秒，没开窗的第二档，周上限卡着的最后。
  let due = |id| { let q = p[id]; if q.exp == () { 1000000000 } else if q.wk { 3000000000 } else { q.exp - now } };
  // 挑号的先后：智能切换开 ⇒ 最先作废的在前，同一档照号牌（量得到用速时下面照推演重排）；关 ⇒ 照号牌。
  let rank = |list| {
    if !smart { return list; }
    let keys = [];
    for id in list { keys.push(due.call(id) * 1024 + pos[id]); }
    keys.sort();
    let out = [];
    for k in keys { out.push(ids[k % 1024]); }
    out
  };

  // 强制切出：其余（不是延迟切入的）号余量合计达到切出条件；没设 ⇒ 400（声明里的默认），空 ⇒ 一有额度。
  let out_now = |id| {
    let r = rows[id];
    if r["out"] != "force" { return false; }
    let total = 0.0;
    let any = false;
    for x in ids { if x != id && ok[x] && tier[x] != "delay" { total += amt[x]; any = true; } }
    if !("outTotal" in r) { return total >= 400.0; }
    if r.outTotal == () { any } else { total >= r.outTotal.to_float() }
  };

  // 能用的号：强制切入的照号牌在前，默认的其次，此刻该切出的排到它们后面；延迟切入的另放。
  let force = []; let norm = []; let outs = []; let late = [];
  for id in ids {
    if !ok[id] { continue; }
    if tier[id] == "delay" { late.push(id); }
    else if out_now.call(id) { outs.push(id); }
    else if tier[id] == "force" { force.push(id); }
    else { norm.push(id); }
  }
  norm = rank.call(norm);
  late = rank.call(late);
  let front = force + norm + outs;

  // 在用的号留不留。「现在用」选的 · 手动排了先后 ⇒ 在用的号只在用不了时换。
  let why = #{};
  let keep = false;
  let pinned = ctx.pinned == true || (ctx.temp != () && ctx.temp.len() > 0);
  if rows[cur] != () && ok[cur] {
    if pinned {
      keep = true;
      why[cur] = "手动选的账号";
    } else if out_now.call(cur) {
      keep = false;
    } else if tier[cur] == "delay" {
      if front.len() == 0 { keep = true; why[cur] = "其余账号无可用额度"; }
      else if memo.late == cur { keep = true; why[cur] = if rows[cur]["out"] == "force" { "未达切出条件，继续使用" } else { "切出为默认：到上限才换" }; }
    } else if force.len() == 0 || force[0] == cur {
      keep = true;
      why[cur] = "未达用量上限，继续使用";
    }
  }

  // 推演（智能切换开着、量得到本会话的用速、没有强制切入的号有额度）：lib::rollout 照几条用速路径把几种走法各推一遍、交原始量，
  //   lib::price 定价（卡住的 ＋ 换号重写的一成 ＋ 用掉的周额度 × 影子价 − 视野末的残值），回每种走法的期望代价与「和留着的差」的标准误。
  let pace = if sess.pace != () { sess.pace.to_float() } else { () };
  let can_roll = smart && pace != () && force.len() == 0;
  let eps = 0.1;           // 换号代价里不算额度的那一截占几成（同 lib::price）
  let sentence = 5000.0;   // 开窗发的那一句话（折合输入 token，取宽）
  let caps = #{};          // 号 ⇒ 两窗的分段上限（没设 ⇒ 90；7 天没设 ⇒ 同 5 小时）
  let lates = [];
  for id in ids {
    let c = rows[id].caps;
    let c5 = if c != () && c["5h"] != () { c["5h"] } else { 90.0 };
    caps[id] = #{ "5h": c5, "7d": if c != () && c["7d"] != () { c["7d"] } else { c5 } };
    if tier[id] == "delay" { lates.push(id); }
  }
  // 一次推得起几种走法：每种每条路径交回约 6 ＋ 9 × 号数个值，积木一次至多交 6000 个（超了先减路径，减到一条为止）。
  let room = (6000 - 8 * ids.len() - 8) / (6 + 9 * ids.len());
  if room < 2 { room = 2; }
  // 往后看两个 5 小时窗：此刻开着的每一窗都在里面作废、再来一轮。
  let roll = |c, walks| {
    let parts = lib::rollout(c, #{ caps: caps, dflt: 90.0, late: lates, walks: walks, horizon: 2 * 18000 });
    if parts == () { () } else { lib::price(parts, c) }
  };
  let jv = #{};            // 号 ⇒ 换到它的期望代价（推演过的）
  if can_roll && !keep && norm.len() > 1 {
    // 要换号：能用的都推（推不起的照最先作废排在后面），挑期望代价最小的；一样小（差不到 1 tok）的先用最先作废的、再照号牌。
    let cands = [];
    let rest = [];
    for id in norm { if cands.len() < room { cands.push(id); } else { rest.push(id); } }
    let pr = roll.call(ctx, cands);
    if pr != () {
      let keys = [];
      for i in 0..cands.len() { keys.push([pr[i].j, due.call(cands[i]), pos[cands[i]], cands[i]]); jv[cands[i]] = pr[i].j; }
      keys.sort(|x, y| before(x, y));
      norm = [];
      for k in keys { norm.push(k[3]); }
      norm += rest;
      front = force + norm + outs;
    }
  }

  let order = [];
  if keep { order.push(cur); }
  for id in front { if !order.contains(id) { order.push(id); } }
  let in_late = keep && tier[cur] == "delay";
  let wait_at = ();
  if front.len() == 0 && late.len() > 0 && !in_late {
    // 其余账号都用完：延迟时长内有号恢复 ⇒ 停到那一刻。
    let w = rows[late[0]].wait;
    if w == () { w = 0; }
    for id in ids {
      if w <= 0 || tier[id] == "delay" { continue; }
      let t = back_at(have[id], rows[id], now, clock);
      if t != () && t - now <= w * 60 && (wait_at == () || t < wait_at) { wait_at = t; }
    }
    if wait_at == () { order += late; }
  }
  if in_late { for id in late { if !order.contains(id) { order.push(id); } } }
  for id in ids {
    if why[id] != () { continue; }
    if order.contains(id) {
      why[id] = if tier[id] == "delay" { "其余账号无可用额度" } else if tier[id] == "force" { "强制切入" }
        else if jv[id] != () { `推演：换过去期望代价约 ${wan(jv[id])} 万` } else if smart { "最先作废" } else { "号牌顺序" };
    } else if ok[id] {
      why[id] = if tier[id] == "delay" { "其余账号用完才切入" } else { "满足切出条件" };
    } else if have[id].sendable == true {
      why[id] = "到用量上限";
    }
  }

  // 智能切换：在用的号还能用、用户没定住它 ⇒ 比「留着」与「换到每一个能用的号」（推得起几种比几种，照最先作废取）。
  //   换的条件：留着的期望代价 − 换过去的 ＞ 换号代价的一成与两倍标准误里大的那个（估不准的差不换，免得来回换）。
  let cands_seen = false;
  if can_roll && keep && !pinned {
    let cands = [];
    for id in norm { if id != cur && cands.len() < room - 1 { cands.push(id); } }
    if cands.len() > 0 {
      cands_seen = true;
      let pr = roll.call(ctx, [cur] + cands);
      if pr != () {
        let bi = 1;
        let kb = [pr[1].j, due.call(cands[0]), pos[cands[0]]];
        for i in 2..pr.len() {
          let ki = [pr[i].j, due.call(cands[i - 1]), pos[cands[i - 1]]];
          if before(ki, kb) < 0 { bi = i; kb = ki; }
        }
        let best = cands[bi - 1];
        let gain = -pr[bi].diff;
        let k = lib::switch_cost(sess, best, now);
        let noise = 2.0 * pr[bi].se;
        if gain > eps * k && gain > noise {
          why[best] = `智能切换：换过去少卡约 ${wan(gain)} 万 · 换号代价约 ${wan(k)} 万`;
          let o2 = [best];
          for id in order { if !o2.contains(id) { o2.push(id); } }
          order = o2;
        } else if gain <= 0.0 {
          why[cur] = `继续使用：换号不少卡（换号代价约 ${wan(k)} 万）`;
        } else if noise >= eps * k {
          why[cur] = `继续使用：换过去少卡约 ${wan(gain)} 万，在估计误差约 ${wan(noise)} 万以内`;
        } else {
          why[cur] = `继续使用：换过去少卡约 ${wan(gain)} 万，不抵换号代价的一成约 ${wan(eps * k)} 万`;
        }
      }
    }
  }

  // 记下延迟切入的号是不是其余都用完才切进来的（会话从它开、其余有额度 ⇒ 换走）。
  let first = if order.len() > 0 { order[0] } else { () };
  memo.late = if first != () && tier[first] == "delay" { first } else { () };

  // 开窗：号列表里此刻能用、5 小时窗关着的号，推演说「此刻给它开窗」比不开期望代价少的超过一句话的代价与两倍标准误 ⇒ 交回 warm，
  //   后端用它发一句、让它的 5 小时从此刻起算（饱和时空着的号每 5 小时少一份）。比的是接着用 first 的两种走法。
  let warm = [];
  if can_roll && first != () {
    let shut = [];
    for id in ids {
      let a = have[id];
      if id != first && ok[id] && !p[id].open && a.kind != "api" && a.off != true && shut.len() < room - 1 { shut.push(id); }
    }
    if shut.len() > 0 {
      let c = ctx;
      if first != cur { c.current = first; }
      let walks = [first];
      for id in shut { walks.push(#{ warm: id }); }
      let pr = roll.call(c, walks);
      if pr != () {
        for i in 0..shut.len() {
          let gain = -pr[i + 1].diff;
          if gain > sentence && gain > 2.0 * pr[i + 1].se {
            warm.push(shut[i]);
            why[shut[i]] = `开窗：此刻开窗少卡约 ${wan(gain)} 万`;
          }
        }
      }
    }
  }

  // 一个都不能用：停到延迟里等的那一刻，或最早有号恢复。
  let hold = ();
  if order.len() == 0 {
    hold = wait_at;
    if hold == () {
      for id in ids {
        let t = back_at(have[id], rows[id], now, clock);
        if t != () && (hold == () || t < hold) { hold = t; }
      }
    }
  }

  // 几时再来问：要用的号到上限的读数 · 分时段的边界 · 延迟切入的号在用时其余号被拒到期 · 智能切换有候选时每分钟 ·
  //   要用的号还没开窗时一用就开（开了才算得出它几时作废）。
  let when = [];
  let at = ();
  for id in ids {
    let r = rows[id];
    if id == first {
      when.push(#{ acct: id, w: "5h", used_at_least: cap_of(r, "5h", clock) });
      if have[id].weekly == true { when.push(#{ acct: id, w: "7d", used_at_least: cap_of(r, "7d", clock) }); }
      if smart && pace != () && !p[id].open { when.push(#{ acct: id, w: "5h", used_at_least: lib::used_of(have[id], "w5h", now) + 0.01 }); }
    }
    let t = ();
    if first != () && tier[first] == "delay" && tier[id] != "delay" { t = have[id].refusedUntil; }
    if r.caps != () {
      for w in ["5h", "7d"] {
        let e = lib::next_edge(r.caps[w], clock);
        if e != () && (t == () || now + e * 60 < t) { t = now + e * 60; }
      }
    }
    if t != () && t > now && (at == () || t < at) { at = t; }
  }
  if cands_seen && (at == () || now + 60 < at) { at = now + 60; }

  #{ order: order, why: why, hold_until: hold, wake: #{ at: at, when: when }, warm: warm, memo: memo }
}
