//! Shared HTTP transport helpers.

use std::io::Read;

/// Read a response body, stopping at `max_bytes`.
///
/// Accepts any `Read` source (`reqwest::blocking::Response` implements
/// `Read`). The caller picks the cap per endpoint — HTML pages, JSON
/// payloads, and model responses have very different sizes. A body over
/// the cap is an error (`InvalidData`), not a truncated buffer: a short
/// JSON parse would look like success.
pub fn read_body_capped<R: Read>(resp: R, max_bytes: u64) -> Result<Vec<u8>, std::io::Error> {
    let mut bytes = Vec::new();
    resp.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if (bytes.len() as u64) > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("response body exceeds {max_bytes} bytes"),
        ));
    }
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
    fn over_cap_is_error() {
        let err = read_body_capped(std::io::Cursor::new(vec![b'x'; 1000]), 10)
            .expect_err("truncated body must not succeed");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
