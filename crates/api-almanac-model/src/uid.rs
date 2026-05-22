use rand::Rng;

/// Generate a stable, compact request UID: 8 uppercase alphanumeric characters [A-Z0-9].
/// Provides ~2.8 trillion distinct values with no external dependencies.
pub fn generate_uid() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    (0..8)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn uid_is_8_chars_uppercase_alphanumeric() {
        let uid = generate_uid();
        assert_eq!(uid.len(), 8);
        assert!(uid.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
    }

    #[test]
    fn uid_generation_produces_unique_values() {
        let uids: HashSet<String> = (0..1000).map(|_| generate_uid()).collect();
        assert_eq!(uids.len(), 1000, "expected 1000 distinct UIDs in 1000 attempts");
    }
}
