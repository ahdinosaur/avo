/// Path escaping, like `systemd-escape --path`.
///
/// From https://github.com/lucab/libsystemd-rs/blob/b43fa5e3b5eca3e6aa16a6c2fad87220dc0ad7a0/src/unit.rs
///
/// Source: https://gitlab.archlinux.org/archlinux/vmexec/-/blob/main/src/utils.rs#L76-L98
pub fn escape_path(path: &str) -> String {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return "-".to_string();
    }

    let mut slash_seq = false;
    let parts: Vec<String> = trimmed
        .bytes()
        .filter(|b| {
            let is_slash = *b == b'/';
            let res = !(is_slash && slash_seq);
            slash_seq = is_slash;
            res
        })
        .enumerate()
        .map(|(n, b)| escape_byte(b, n))
        .collect();
    parts.join("")
}
