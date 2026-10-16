//! 用户级 `PATH` 那几段 PowerShell：只动用户那一档、不踩 `setx` / `$env:PATH`、撤只摘自己那一格；状态与加 / 撤同一种相等。

use super::*;

const DIR: &str = r"C:\Users\u\.cc-monitor\bin";

/// 加 / 撤两段读 `HKCU\Environment` 的未展开原值与原类型、按原类型写回、写完广播；探那段只读用户那一档。
/// 地雷：`setx`（截断在 1024 字符、退出码照样 0）· 拿 `$env:PATH` 写回（机器级 ＋ 用户级拼起来的）· 写 `'Machine'` 那一档。
/// 撤：整格比（子串口径会把 `…\bin-old` 一起删掉）、不顺手滤空项（空项是「当前目录」）。
#[test]
fn the_user_path_scripts_edit_only_the_user_scope_and_never_via_setx() {
    let query = user_path_query("@@m@@");
    let add = user_path_add(DIR);
    let del = user_path_remove(DIR);
    assert!(
        query.contains("GetEnvironmentVariable('Path', 'User')"),
        "{query}"
    );
    for (which, cmd) in [("加", &add), ("撤", &del)] {
        for need in [
            "[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')",
            "GetValue('Path', $null, 'DoNotExpandEnvironmentNames')",
            "GetValueKind('Path')",
            "SetValue('Path', $n, $t)",
            SETTING_CHANGE_BROADCAST,
        ] {
            assert_eq!(
                cmd.matches(need).count(),
                1,
                "「{which}」少了 / 多了 `{need}`：\n{cmd}"
            );
        }
        for landmine in [
            "SetEnvironmentVariable",
            "GetEnvironmentVariable('Path', 'User')",
            "Where-Object { $_ }",
        ] {
            assert!(
                !cmd.contains(landmine),
                "「{which}」出现了 `{landmine}`：\n{cmd}"
            );
        }
    }
    for (which, cmd) in [("探", &query), ("加", &add), ("撤", &del)] {
        for landmine in ["setx", "'Machine'", "\"Machine\"", "$env:PATH"] {
            assert!(
                !cmd.contains(landmine),
                "「{which}」出现了 `{landmine}`：\n{cmd}"
            );
        }
    }
    assert!(
        add.contains("[Environment]::ExpandEnvironmentVariables($_) -eq $d"),
        "{add}"
    );
    assert!(
        del.contains("[Environment]::ExpandEnvironmentVariables($_) -ne $d"),
        "{del}"
    );
    for substr_op in ["-replace", "-like", "-notlike", "-match", ".Replace("] {
        assert!(
            !del.contains(substr_op),
            "「撤」用了子串口径 `{substr_op}`：\n{del}"
        );
    }
    // 目录按 PowerShell 单引号字面量放进去（含 `'` 的目录不断句）。
    assert!(user_path_add(r"C:\Users\o'k\.cc-monitor\bin")
        .contains(r"$d = 'C:\Users\o''k\.cc-monitor\bin'"));
}

/// 状态那一格与加 / 撤两段同一种相等：整格 · ASCII 大小写不敏感 · 不规范化；目录空恒不在。
#[test]
fn the_user_path_status_uses_the_same_equality_as_the_scripts() {
    assert!(user_path_has(&format!(r"C:\Windows;{DIR};C:\foo"), DIR));
    assert!(user_path_has(DIR, DIR));
    assert!(user_path_has(&format!(r"C:\Windows;{DIR}"), DIR));
    assert!(user_path_has(
        r"C:\Windows;c:\users\U\.cc-monitor\BIN;C:\foo",
        DIR
    ));
    assert!(!user_path_has(&format!("{DIR}-old"), DIR));
    assert!(!user_path_has(&format!(r"C:\Windows;{DIR}-old"), DIR));
    assert!(!user_path_has(&format!(r"{DIR}2;C:\x"), DIR));
    assert!(!user_path_has(&format!(r"{DIR}\"), DIR));
    assert!(!user_path_has(r"C:\a;;C:\b", ""));
    assert!(!user_path_has("", ""));
    assert!(!user_path_has("", DIR));
}

/// 探那段除赋值外只有「写字节 · 冲刷」两句：不经 PowerShell 的输出流（那一路按控制台代码页编码，非 ASCII 目录读回就坏）。
#[test]
fn the_user_path_query_writes_nothing_but_its_utf8_bytes() {
    let q = user_path_query("@@m@@");
    let statements: Vec<&str> = q
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !(l.starts_with('$') && l.contains(" = ")))
        .collect();
    assert_eq!(
        statements,
        ["$o.Write($b, 0, $b.Length)", "$o.Flush()"],
        "{q}"
    );
    assert_eq!(
        q.matches("[Text.Encoding]::UTF8.GetBytes(").count(),
        1,
        "{q}"
    );
}

/// 读回：成对标记中间那一段（空串也算读到）；标记不成对 ⇒ 没跑到底。
#[test]
fn the_user_path_reads_back_between_the_marks() {
    assert_eq!(
        between_marks("junk@@m@@C:\\A;C:\\B@@m@@", "@@m@@"),
        Some("C:\\A;C:\\B")
    );
    assert_eq!(between_marks("@@m@@@@m@@", "@@m@@"), Some(""));
    assert_eq!(between_marks("@@m@@C:\\A", "@@m@@"), None);
    assert_eq!(between_marks("", "@@m@@"), None);
}

/// 读数，**不在门禁**（要一个 PowerShell；`CCM_PWSH=<程序> cargo test -- --ignored wf1_`，那个程序收一个 `.ps1` 路径去跑）。
/// 用真 PowerShell 跑加 / 撤两段：注册表那一跳换成替身键，广播那三行换成计数。期望逐条手写：加 ⇒ 我们那一格恰好多一格
/// （原值以 `;` 收尾就插在那个空项之前）、类型不变、广播一次；再加 ⇒ 一个字节不动、不广播；撤 ⇒ 逐字回到原样。
/// 买不到：真注册表的读写与 `WM_SETTINGCHANGE` 真被新终端收到（要 Windows）。
#[test]
#[ignore = "要一个 PowerShell：CCM_PWSH=<收 .ps1 路径的程序> cargo test -- --ignored"]
fn wf1_real_powershell_add_then_remove_restores_the_user_path() {
    let Ok(pwsh) = std::env::var("CCM_PWSH") else {
        panic!("没给 CCM_PWSH");
    };
    let open = "$k = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')\n";
    let fake = |script: String| -> String {
        assert_eq!(script.matches(open).count(), 1, "锚不住开键那一行");
        assert_eq!(
            script.matches(SETTING_CHANGE_BROADCAST).count(),
            1,
            "锚不住广播那三行"
        );
        script
            .replace(open, "$k = $global:fake\n")
            .replace(SETTING_CHANGE_BROADCAST, "    $global:bc++\n")
    };
    let add = fake(user_path_add(DIR));
    let del = fake(user_path_remove(DIR));
    let ours = DIR;
    // (原值, 原类型；None = 不在) → 加之后的值（None = 已经在了、不动）· 撤之后的值（None = 不在）
    #[rustfmt::skip]
    let cases: &[(Option<(&str, &str)>, Option<String>, Option<&str>)] = &[
        (None, Some(ours.into()), None),
        (Some((r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;", "ExpandString")),
         Some(format!(r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;{ours};")),
         Some(r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;")),
        (Some((r"C:\A", "String")), Some(format!(r"C:\A;{ours}")), Some(r"C:\A")),
        (Some((r"C:\A;;C:\B", "ExpandString")), Some(format!(r"C:\A;;C:\B;{ours}")), Some(r"C:\A;;C:\B")),
        (Some((&format!("{ours}-old"), "ExpandString")), Some(format!("{ours}-old;{ours}")), Some(&format!("{ours}-old"))),
        (Some((r"C:\A;%USERPROFILE%\.cc-monitor\bin", "ExpandString")), None, Some(r"C:\A")),
    ];
    let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let mut ps = String::from(
        "$ErrorActionPreference = 'Stop'\n\
         $env:USERPROFILE = 'C:\\Users\\u'\n\
         class FakeKey {\n\
           [hashtable]$v = @{}; [hashtable]$kind = @{}\n\
           [object] GetValue([string]$n) { return $this.v[$n] }\n\
           [object] GetValue([string]$n, [object]$def, [object]$opt) { if ($this.v.ContainsKey($n)) { return $this.v[$n] }; return $def }\n\
           [object] GetValueKind([string]$n) { return $this.kind[$n] }\n\
           [void] SetValue([string]$n, [object]$val, [object]$t) { $this.v[$n] = [string]$val; $this.kind[$n] = [string]$t }\n\
           [void] DeleteValue([string]$n, [bool]$throw) { $this.v.Remove($n); $this.kind.Remove($n) }\n\
         }\n\
         function Snap { if ($global:fake.v.ContainsKey('Path')) { @($global:fake.v['Path'], $global:fake.kind['Path']) } else { @($null, $null) } }\n",
    );
    for (i, (orig, _, _)) in cases.iter().enumerate() {
        let seed = match orig {
            Some((v, t)) => format!(
                "$global:fake.v['Path'] = {}; $global:fake.kind['Path'] = {}\n",
                q(v),
                q(t)
            ),
            None => String::new(),
        };
        ps.push_str(&format!(
            "$global:fake = [FakeKey]::new(); $global:bc = 0\n{seed}\
             & {{\n{add}}}\n$a = Snap; $ab = $global:bc\n\
             & {{\n{add}}}\n$a2 = Snap; $ab2 = $global:bc - $ab\n\
             & {{\n{del}}}\n$r = Snap; $rb = $global:bc - $ab - $ab2\n\
             [pscustomobject]@{{ i = {i}; a = $a; ab = $ab; a2 = $a2; ab2 = $ab2; r = $r; rb = $rb }} | ConvertTo-Json -Compress -Depth 3\n"
        ));
    }
    let dir = std::env::temp_dir().join(format!("ccm-wf1-pwsh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建 tempdir");
    let file = dir.join("harness.ps1");
    std::fs::write(&file, &ps).unwrap();
    let out = std::process::Command::new(&pwsh).arg(&file).output();
    let _ = std::fs::remove_dir_all(&dir);
    let out = out.expect("起 CCM_PWSH");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "PowerShell 没跑完：{}\n{text}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).expect(l))
        .collect();
    assert_eq!(rows.len(), cases.len(), "{text}");
    for (row, (orig, after_add, after_del)) in rows.iter().zip(cases) {
        let (ov, ok) = orig.map_or((None, None), |(v, t)| {
            (Some(v.to_string()), Some(t.to_string()))
        });
        let kind = ok.clone().or(Some("ExpandString".into()));
        let pair = |v: &serde_json::Value| {
            (
                v[0].as_str().map(String::from),
                v[1].as_str().map(String::from),
            )
        };
        let want_add = match after_add {
            Some(v) => (Some(v.clone()), kind.clone()),
            None => (ov.clone(), ok.clone()),
        };
        assert_eq!(pair(&row["a"]), want_add, "加：{orig:?}");
        assert_eq!(
            row["ab"],
            u64::from(after_add.is_some()),
            "加的广播：{orig:?}"
        );
        assert_eq!(pair(&row["a2"]), want_add, "再加动了：{orig:?}");
        assert_eq!(row["ab2"], 0, "再加又广播：{orig:?}");
        let want_del = (after_del.map(String::from), after_del.and(kind.clone()));
        assert_eq!(pair(&row["r"]), want_del, "撤：{orig:?}");
        assert_eq!(row["rb"], 1, "撤的广播：{orig:?}");
    }
}
