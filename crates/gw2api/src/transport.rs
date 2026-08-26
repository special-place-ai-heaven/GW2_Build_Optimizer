//! Shared HTTP transport helpers.

use std::io::Read;

/// Read a response body, stopping at `max_bytes`.
///
/// Accepts any `Read` source (`reqwest::blocking::Response` implements
/// `Read`). The caller picks the cap per endpoint — HTML pages, JSON
/// payloads, and model responses have very different sizes. Reads beyond
/// the cap are truncated, not an error; the caller's parser decides whether
/// what survived is usable.
pub fn read_body_capped<R: Read>(resp: R, max_bytes: u64) -> Result<Vec<u8>, std::io::Error> {
    let mut bytes = Vec::new();
    resp.take(max_bytes).read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_cap_passes_through() {
        let bytes = read_body_capped(std::io::Cursor::new(b"hello".to_vec()), 100).unwrap();
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn over_cap_truncates() {
        let bytes = read_body_capped(std::io::Cursor::new(vec![b'x'; 1000]), 10).unwrap();
        assert_eq!(bytes.len(), 10);
    }
}
