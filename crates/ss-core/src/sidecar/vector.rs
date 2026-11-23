//! Embedding vectors on the wire (contract § Transport: base64 of little-endian float32, L2-normalized)
//! and in the catalog (`embedding.vec`: the same bytes).

/// Standard base64 (RFC 4648, `+/`, `=` padding) to bytes. None on any invalid character or length.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a') as u32 + 26,
            b'0'..=b'9' => (c - b'0') as u32 + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    }
    let bytes = s.trim().as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (i, chunk) in bytes.chunks(4).enumerate() {
        let last = i + 1 == bytes.len() / 4;
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && !last) {
            return None;
        }
        let mut n = 0u32;
        for &c in &chunk[..4 - pad] {
            n = (n << 6) | val(c)?;
        }
        n <<= 6 * pad as u32;
        let b = n.to_be_bytes();
        out.extend_from_slice(&b[1..4 - pad]);
    }
    Some(out)
}

/// Standard base64 with padding (tests and fixtures).
pub fn base64_encode(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |a, (i, &b)| a | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Little-endian float32 bytes -> floats. None unless the length is exactly `dim * 4`.
pub fn floats_from_le(bytes: &[u8], dim: usize) -> Option<Vec<f32>> {
    (dim > 0 && bytes.len() == dim * 4)
        .then(|| bytes.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect())
}

/// Floats -> little-endian float32 bytes.
pub fn floats_to_le(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// A wire vector (base64 LE float32) of `dim` floats.
pub fn decode_vector(b64: &str, dim: usize) -> Result<Vec<f32>, String> {
    let bytes = base64_decode(b64).ok_or("vector is not valid base64")?;
    let v = floats_from_le(&bytes, dim).ok_or_else(|| format!("vector has {} bytes, expected dim {dim} x 4", bytes.len()))?;
    if v.iter().any(|f| !f.is_finite()) {
        return Err("vector holds a non-finite value".into());
    }
    Ok(v)
}

/// Scales `v` to unit length in place (cosine = dot product afterwards). A zero vector stays zero.
pub fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
}

pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trip_and_fixture() {
        for n in 0..20 {
            let bytes: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(base64_decode(&base64_encode(&bytes)).unwrap(), bytes, "len {n}");
        }
        // contracts/fixtures/sidecar/embed.text.response.json: four 0.5 floats.
        let v = decode_vector("AAAAPwAAAD8AAAA/AAAAPw==", 4).unwrap();
        assert_eq!(v, vec![0.5; 4]);
        assert!(base64_decode("AAA").is_none());
        assert!(base64_decode("A=AA").is_none());
        assert!(base64_decode("AA*A").is_none());
        assert!(decode_vector("AAAAPw==", 4).is_err());
    }
}
