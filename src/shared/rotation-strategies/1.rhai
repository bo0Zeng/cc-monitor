// 内置策略 1（缺省）：不随情况调。号的先后就是配置里写下的先后（ordered: true）。
// 今天界面上能调的每一样都是这份策略的参数：先后 · 每号用到多少换 · 按时段另算 · 用法（普通 / 优先 / 备用）·
// 每次最多连用 · 前面的号恢复后换回去 · 换号前延迟 · 用备用号前先等 · 都到上限后。
// 只算优先级：每个号一个数（大的先）或 ()（此刻不挑），外加一句为什么。挑号、热换、推演是引擎的事。
// 策略出错或超时时，引擎也照这一份排（参数取缺省，号的先后取配置里写下的先后）。

fn meta() {
  #{
    format: 1,
    ordered: true,
    params: [
      #{ k: "back",         type: "bool",    init: false, label: "p_back" },
      #{ k: "delay",        type: "minutes", init: 0,  min: 0, max: 120, label: "p_delay", hint: "h_delay" },
      #{ k: "standby_wait", type: "minutes", init: 40, min: 0, max: 240, label: "p_standby_wait" },
      #{ k: "all_out",      type: "choice",  of: ["stop", "go"], init: "stop", label: "p_all_out" },
    ],
    acct_params: [
      #{ k: "line",  type: "line",   init: 90, label: "p_line" },
      #{ k: "slots", type: "slots",  init: [], label: "p_slots" },
      #{ k: "mode",   type: "choice", of: ["normal", "first", "standby"], init: "normal", label: "p_mode" },
      #{ k: "stint", type: "points", init: (), min: 1, max: 100, label: "p_stint", hint: "h_stint" },
    ],
  }
}

fn texts() {
  #{
    zh: #{
      d_list: "按写下的先后用",
      d_back: "前面的号恢复就换回",
      d_keep: "前面的号恢复也不换回",
      d_first: "{acct} 优先",
      d_standby: "{acct} 备用",
      d_stint: "{acct} 每次最多连用 {n} 点",
      p_back: "前面的号恢复后换回去",
      p_delay: "换号前延迟（分）",
      h_delay: "有号排到在用的号上面，连着这么久才换",
      p_standby_wait: "用备用号前先等别的号恢复，最多（分）",
      p_all_out: "都到上限后",
      p_all_out_stop: "停下",
      p_all_out_go: "接着用",
      p_line: "用到多少换",
      p_slots: "按时段另算",
      p_mode: "用法",
      p_mode_normal: "普通",
      p_mode_first: "优先",
      p_mode_first_h: "一有额度就用它",
      p_mode_standby: "备用",
      p_mode_standby_h: "别的号一有额度就不用它",
      p_stint: "每次最多连用（点）",
      h_stint: "空着 ＝ 不限",
      w_order: "写在第 {n}",
      w_first: "优先 · 写在第 {n}",
      w_standby: "备用 · 写在第 {n}",
      w_wait: "备用 · 先等 {acct} {at} 恢复",
      w_keep: "还能用就不换（写在第 {n}）",
      w_stint: "这次已连用 {n} 点（上限 {m}），让给别的号",
    },
    en: #{
      d_list: "In the written order",
      d_back: "switch back when an earlier one recovers",
      d_keep: "stay even when an earlier one recovers",
      d_first: "{acct} first",
      d_standby: "{acct} standby",
      d_stint: "{acct} at most {n} pts per turn",
      p_back: "Switch back when an earlier one recovers",
      p_delay: "Delay before switching (min)",
      h_delay: "Switch only after another one has ranked above for this long",
      p_standby_wait: "Before a standby, wait for others up to (min)",
      p_all_out: "When all are at their limit",
      p_all_out_stop: "Stop",
      p_all_out_go: "Keep going",
      p_line: "Switch at",
      p_slots: "By time of day",
      p_mode: "Use",
      p_mode_normal: "Normal",
      p_mode_first: "First",
      p_mode_first_h: "use whenever it has quota",
      p_mode_standby: "Standby",
      p_mode_standby_h: "leave as soon as another has quota",
      p_stint: "Max points per turn",
      h_stint: "empty = no limit",
      w_order: "#{n} as written",
      w_first: "first · #{n} in order",
      w_standby: "standby · #{n} in order",
      w_wait: "standby · waiting for {acct} at {at}",
      w_keep: "stays while usable (was #{n})",
      w_stint: "used {n} pts this turn (max {m}), giving way",
    },
  }
}

fn describe(p, accounts) {
  let out = ["d_list", if p.back { "d_back" } else { "d_keep" }];
  for a in accounts {
    if a.params.mode == "first" { out.push(#{ k: "d_first", acct: a.id }); }
    if a.params.mode == "standby" { out.push(#{ k: "d_standby", acct: a.id }); }
    if a.params.stint != () { out.push(#{ k: "d_stint", acct: a.id, n: a.params.stint }); }
  }
  out + lib::describe_common(p, accounts)
}

fn priority(ctx) {
  let p = ctx.params;
  let ck = lib::checks(ctx);
  let out = #{};

  // 备用号要不要先等：别的（非备用）号都不能挑、有一个在 standby_wait 分内回来 ⇒ 先等它，备用号此刻不挑。
  let soon = ();
  let others_ok = false;
  for a in ctx.accounts {
    if a.params.mode == "standby" { continue; }
    let u = ck[a.id];
    if u.ok { others_ok = true; continue; }
    if u.back != () && u.back - ctx.now <= p.standby_wait * 60 && (soon == () || u.back < soon.at) {
      soon = #{ acct: a.id, at: u.back };
    }
  }

  let i = 0;
  for a in ctx.accounts {
    i += 1;
    let u = ck[a.id];
    if !u.ok { out[a.id] = #{ p: (), why: u.why, back: u.back }; continue; }
    let r = a.params.mode;
    if r == "first" {
      out[a.id] = #{ p: 3000 - i, why: #{ k: "w_first", n: i } };
    } else if r == "standby" {
      if soon != () && !others_ok {
        out[a.id] = #{ p: (), why: #{ k: "w_wait", acct: soon.acct, at: soon.at }, back: soon.at };
      } else {
        out[a.id] = #{ p: 100 - i, why: #{ k: "w_standby", n: i } };
      }
    } else {
      out[a.id] = #{ p: 1000 - i, why: #{ k: "w_order", n: i } };
    }
  }

  // 在用的号：这次连用到了它的上限 ⇒ 排最低；不换回 ⇒ 还能用就排在普通号上面（优先的号照样在它上面；备用号不粘）。
  let i = 0;
  for a in ctx.accounts {
    i += 1;
    if a.id != ctx.current || !ck[a.id].ok { continue; }
    let n = a.params.stint;
    if n != () && ctx.stint.used >= n {
      out[a.id] = #{ p: 0, why: #{ k: "w_stint", n: ctx.stint.used, m: n } };
    } else if !p.back && a.params.mode != "standby" {
      out[a.id] = #{ p: 2000, why: #{ k: "w_keep", n: i } };
    }
  }

  out = lib::delay(ctx, out, p.delay);
  lib::all_out(ctx, out)
}
