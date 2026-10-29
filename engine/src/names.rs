use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// A persisted alias is never recomputed just because a sibling disappears.
pub fn alias(name: &str, id: &str, occupied: &HashSet<String>) -> String {
    let mut safe: String = name
        .chars()
        .map(|c| {
            if c < ' ' || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut units = 0;
    safe = safe
        .trim_end_matches([' ', '.'])
        .chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= 160
        })
        .collect();
    if safe.is_empty() {
        safe = "unnamed".into();
    }
    let stem = safe.split('.').next().unwrap_or("").to_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&stem.as_str())
    {
        safe.insert(0, '_');
    }
    if !occupied.contains(&safe.to_lowercase()) {
        return safe;
    }
    let hash = format!("{:x}", Sha256::digest(id.as_bytes()));
    for n in 8..=64 {
        let candidate = match safe.rsplit_once('.') {
            Some((stem, ext)) => format!("{stem}~{}.{ext}", &hash[..n]),
            None => format!("{safe}~{}", &hash[..n]),
        };
        if !occupied.contains(&candidate.to_lowercase()) {
            return candidate;
        }
    }
    unreachable!("SHA-256 alias collision for distinct Drive IDs")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sanitizes_reserved_and_invalid_names() {
        assert_eq!(alias("CON.txt", "1", &HashSet::new()), "_CON.txt");
        assert_eq!(alias("a:b?. ", "1", &HashSet::new()), "a_b_");
        assert_eq!(alias("..", "1", &HashSet::new()), "unnamed");
    }
    #[test]
    fn collisions_have_stable_suffixes() {
        let occupied = HashSet::from(["report.txt".into()]);
        let a = alias("Report.txt", "id1", &occupied);
        assert_ne!(a, "Report.txt");
        assert_eq!(a, alias("Report.txt", "id1", &occupied));
        assert!(a.ends_with(".txt"));
    }
}
