//! The error correction half of a QR code: Reed Solomon over GF(256) and
//! the interleaving of the blocks, as `qr.js` does it.

use super::qr::{raw_modules, BLOCKS_M, ECC_M};

pub fn interleave(ver: usize, data: &[u8]) -> Vec<u8> {
    let blocks = BLOCKS_M[ver - 1];
    let ecc_len = ECC_M[ver - 1];
    let raw = raw_modules(ver) / 8;
    let short = blocks - raw % blocks;
    let short_len = raw / blocks;
    let divisor = rs_divisor(ecc_len);
    let mut made: Vec<Vec<u8>> = Vec::new();
    let mut k = 0;
    for i in 0..blocks {
        let len = short_len - ecc_len + usize::from(i >= short);
        let mut dat = data[k..k + len].to_vec();
        k += len;
        let ecc = rs_remainder(&dat, &divisor);
        if i < short {
            dat.push(0);
        }
        dat.extend(ecc);
        made.push(dat);
    }
    let mut out = Vec::new();
    for i in 0..made[0].len() {
        for (j, block) in made.iter().enumerate() {
            if i != short_len - ecc_len || j >= short {
                out.push(block[i]);
            }
        }
    }
    out
}

fn gf_mul(x: u8, y: u8) -> u8 {
    let mut z: u32 = 0;
    for i in (0..8).rev() {
        z = (z << 1) ^ ((z >> 7) * 0x11d);
        z ^= ((u32::from(y) >> i) & 1) * u32::from(x);
    }
    (z & 0xff) as u8
}

fn rs_divisor(degree: usize) -> Vec<u8> {
    let mut result = vec![0u8; degree];
    result[degree - 1] = 1;
    let mut root = 1u8;
    for _ in 0..degree {
        for j in 0..degree {
            result[j] = gf_mul(result[j], root);
            if j + 1 < degree {
                result[j] ^= result[j + 1];
            }
        }
        root = gf_mul(root, 2);
    }
    result
}

fn rs_remainder(data: &[u8], divisor: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; divisor.len()];
    for b in data {
        let factor = b ^ result.remove(0);
        result.push(0);
        for (r, coef) in result.iter_mut().zip(divisor) {
            *r ^= gf_mul(*coef, factor);
        }
    }
    result
}
