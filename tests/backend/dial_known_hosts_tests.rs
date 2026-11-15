//! 开终端那一行 `ssh -F` 用的那份配置的全文：主机钥匙那一格在前，之后照读用户与系统的 ssh 配置；路径照 ssh 配置的切词规矩写。

use super::*;

#[test]
fn the_terminal_config_names_our_known_hosts_first_then_includes_both_ssh_configs() {
    assert_eq!(
        config_text(b"/h/.cc-monitor/known_hosts", "~/.ssh/config", "/etc/ssh/ssh_config"),
        b"UserKnownHostsFile /h/.cc-monitor/known_hosts\nInclude ~/.ssh/config\nInclude /etc/ssh/ssh_config\n"
    );
    assert_eq!(
        config_text(br"C:\Users\O Neil\100%\known_hosts", "~/.ssh/config", r"C:\ProgramData\ssh\ssh_config"),
        b"UserKnownHostsFile \"C:\\Users\\O Neil\\100%%\\known_hosts\"\nInclude ~/.ssh/config\nInclude C:\\ProgramData\\ssh\\ssh_config\n"
    );
    // 钥匙库路径不是 UTF-8：照原字节写进去（ssh 读配置按字节切词）。
    assert_eq!(
        config_text(b"/h/u\xff/.cc-monitor/known_hosts", "~/.ssh/config", "/etc/ssh/ssh_config"),
        b"UserKnownHostsFile /h/u\xff/.cc-monitor/known_hosts\nInclude ~/.ssh/config\nInclude /etc/ssh/ssh_config\n"
    );
}
