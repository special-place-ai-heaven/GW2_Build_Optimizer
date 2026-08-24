use chrono::NaiveDate;
use rand::RngCore;
use sha2::{Digest, Sha256};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 8 Crockford-base32 chars from 40 random bits. Shown to the player as `#A3F9K2QD`.
pub fn short_id() -> String {
    let mut bytes = [0u8; 5];
    rand::thread_rng().fill_bytes(&mut bytes);
    let mut acc: u64 = 0;
    for b in bytes {
        acc = (acc << 8) | b as u64;
    }
    (0..8)
        .rev()
        .map(|i| ALPHABET[((acc >> (i * 5)) & 31) as usize] as char)
        .collect()
}

/// sha256(ip || salt || day). Rotates daily so it cannot be joined across days.
pub fn ip_hash(ip: &str, salt: &str, day: NaiveDate) -> String {
    let mut h = Sha256::new();
    h.update(ip.as_bytes());
    h.update(salt.as_bytes());
    h.update(day.format("%Y-%m-%d").to_string().as_bytes());
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
