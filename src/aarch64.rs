#![cfg(target_arch = "aarch64")]

use crate::pattern::{PATTERNS, PATTERNS_ID};
use core::arch::aarch64::*;

#[target_feature(enable = "neon")]
pub(crate) unsafe fn inet_aton_impl(str: &[u8]) -> Option<u32> {
    // Always read 16 bytes from the input
    let input = vld1q_u8(str.as_ptr().cast());
    if str.len() > 15 {
        return None;
    }

    // locate dots
    let dotmask = {
        let dot = vdupq_n_u8(b'.');
        let t0 = vceqq_u8(input, dot);
        let mut dotmask = vmovmaskq_u8(t0) as u16;
        let mask = 1 << str.len();
        dotmask &= mask - 1;
        dotmask |= mask;
        dotmask
    };

    // build a hashcode
    let hashcode = ((6639 * u32::from(dotmask)) >> 13) as u8;

    // grab the index of the shuffle mask
    let id = PATTERNS_ID[hashcode as usize];
    if id >= 81 {
        return None;
    }
    let pat: *const u8 = &PATTERNS[id as usize][0];
    let pattern = vld1q_u8(pat.cast());
    // The value of the shuffle mask at a specific index points at the last digit,
    // we check that it matches the length of the input.
    let ascii0 = vdupq_n_u8(b'0');
    let t0 = input;
    let t1 = vshuffle_u8(t0, pattern);

    // check that leading digits of 2- 3- numbers are not zeros.
    {
        let eq0 = vceqq_u8(t1, ascii0);
        let res = {
            let mut mask = [0xff, 0, 0xff, 0, 0xff, 0, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            mask.reverse();
            let res = vandq_u64(vreinterpretq_u64_u8(eq0), vld1q_u64(mask.as_ptr()));
            !(vgetq_lane_u64(res, 0) | vgetq_lane_u64(res, 1))
        };
        if res == 0 {
            return None;
        }
    }
    // replace null values with '0'
    let t1b = {
        let mask = vreinterpretq_u8_s8(vshrq_n_s8(vreinterpretq_s8_u8(pattern), 7));
        vbslq_u8(mask, ascii0, t1)
    };

    // subtract '0'
    let t2 = vsubq_u8(t1b, ascii0);
    // check that everything was in the range '0' to '9'
    {
        let c9 = vdupq_n_u8(b'9' - b'0');
        let t2m = vmaxq_u8(t2, c9);
        let t2me = vceqq_u8(t2m, c9);
        let res = {
            let x = vreinterpretq_u64_u8(t2me);
            vgetq_lane_u64(x, 0) & vgetq_lane_u64(x, 1) == !0
        };
        if !res {
            return None;
        }
    }
    // We do the computation, the Mula way.
    let weights = vld1q_u8([1, 10, 1, 10, 1, 10, 1, 10, 100, 0, 100, 0, 100, 0, 100, 0].as_ptr());
    let t3 = vmaddubs_u8(t2, weights);
    let t4 = vextq_u8::<8>(vreinterpretq_u8_u16(t3), vreinterpretq_u8_u16(t3));
    let t5 = vaddq_u16(vreinterpretq_u16_u8(t4), t3);
    // Test that we don't overflow (over 255)
    let res = {
        let mut mask = [0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0, 0xff, 0, 0xff, 0, 0xff, 0];
        mask.reverse();
        let res = vandq_u64(vreinterpretq_u64_u16(t5), vld1q_u64(mask.as_ptr()));
        !(vgetq_lane_u64(res, 0) | vgetq_lane_u64(res, 1))
    };
    if res == 0 {
        return None;
    }
    // pack and we are done!
    let t6 = vcombine_u8(vqmovn_u16(t5), vqmovn_u16(t5));
    Some(vgetq_lane_u32(vreinterpretq_u32_u8(t6), 0))
}

// Use shifts to collect all of the sign bits.
// I'm not sure if this works on big endian, but big endian NEON is very
// rare.
unsafe fn vmovmaskq_u8(input: uint8x16_t) -> u32 {
    // Example input (half scale):
    // 0x89 FF 1D C0 00 10 99 33

    // Shift out everything but the sign bits
    // 0x01 01 00 01 00 00 01 00
    let high_bits = vreinterpretq_u16_u8(vshrq_n_u8(input, 7));

    // Merge the even lanes together with vsra. The '??' bytes are garbage.
    // vsri could also be used, but it is slightly slower on aarch64.
    // 0x??03 ??02 ??00 ??01
    let paired16 = vreinterpretq_u32_u16(vsraq_n_u16(high_bits, high_bits, 7));
    // Repeat with wider lanes.
    // 0x??????0B ??????04
    let paired32 = vreinterpretq_u64_u32(vsraq_n_u32(paired16, paired16, 14));
    // 0x??????????????4B
    let paired64 = vreinterpretq_u8_u64(vsraq_n_u64(paired32, paired32, 28));
    // Extract the low 8 bits from each lane and join.
    // 0x4B
    u32::from(vgetq_lane_u8(paired64, 0)) | (u32::from(vgetq_lane_u8(paired64, 8)) << 8)
}

unsafe fn vmaddubs_u8(a: uint8x16_t, b: uint8x16_t) -> uint16x8_t {
    let tl = vmulq_u16(vmovl_u8(vget_low_u8(a)), vmovl_u8(vget_low_u8(b)));
    let th = vmulq_u16(vmovl_u8(vget_high_u8(a)), vmovl_u8(vget_high_u8(b)));
    vqaddq_u16(vuzp1q_u16(tl, th), vuzp2q_u16(tl, th))
}

unsafe fn vshuffle_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
    let idx_masked = vandq_u8(b, vdupq_n_u8(0x8F));
    vqtbl1q_u8(a, idx_masked)
}
