//! SHA-256, written out rather than pulled in.
//!
//! One digest is needed: the one a cosign bundle records for the artefact it
//! signed. Every SHA-256 crate in the registry brings a trait hierarchy, a
//! generic array crate and a feature matrix with it, and none of that earns
//! its place for sixty lines of arithmetic that has not changed since 2001.
//! The tests at the bottom run the NIST vectors and the three padding
//! boundaries with known answers, so a mistake here fails the build rather
//! than a release.

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// The digest of a byte string, as 32 bytes.
pub fn digest(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_be_bytes());

    for block in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (slot, add) in h.iter_mut().zip(v.iter()) {
            *slot = slot.wrapping_add(*add);
        }
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// The digest as lower case hex, which is how cosign and Rekor write it.
pub fn hex(data: &[u8]) -> String {
    let mut s = String::with_capacity(64);
    for byte in digest(data) {
        s.push(char::from_digit((byte >> 4) as u32, 16).expect("a nibble is a hex digit"));
        s.push(char::from_digit((byte & 0x0f) as u32, 16).expect("a nibble is a hex digit"));
    }
    s
}

/// The digest of a file, read in one go. A plugin asset is tens of megabytes
/// at the outside, so streaming it would buy nothing.
pub fn hex_file(path: &std::path::Path) -> std::io::Result<String> {
    Ok(hex(&std::fs::read(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nist_vectors() {
        assert_eq!(
            hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn the_padding_boundary_and_a_message_of_a_thousand_blocks() {
        // 55 bytes is the most that fits in one block with its padding, 56
        // forces a second, and 64 is exactly one block with a whole block of
        // padding after it. Those three are where a hand written pad goes
        // wrong, so all three carry a known answer rather than being compared
        // against each other.
        assert_eq!(
            hex(&[b'x'; 55]),
            "d5e285683cd4efc02d021a5c62014694958901005d6f71e89e0989fac77e4072"
        );
        assert_eq!(
            hex(&[b'x'; 56]),
            "04c26261370ee7541549d16dee320c723e3fd14671e66a099afe0a377c16888e"
        );
        assert_eq!(
            hex(&[b'x'; 64]),
            "7ce100971f64e7001e8fe5a51973ecdfe1ced42befe7ee8d5fd6219506b5393c"
        );
        // A thousand blocks, so the message length crosses a byte boundary in
        // the length field the padding carries.
        let long = [b'a'; 64_000];
        assert_eq!(
            hex(&long),
            "b79a5f9da7504a0ca606f2b54a81d5e28a8c924df8974416f320501652dcd6be"
        );
    }

    #[test]
    fn the_digest_of_a_file_is_the_digest_of_its_bytes() {
        let path = std::env::temp_dir().join(format!("gmx-sha-{}", std::process::id()));
        std::fs::write(&path, b"abc").expect("a file");
        assert_eq!(
            hex_file(&path).expect("it reads"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(&path);
    }
}
