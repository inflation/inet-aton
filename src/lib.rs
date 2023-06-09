use crate::pattern::{PATTERNS, PATTERNS_ID};

#[target_feature(enable = "sse4.1")]
unsafe fn inet_aton_impl(str: &[u8]) -> Option<u32> {
    use std::arch::x86_64::*;

    // Always read 16 bytes from the input
    let input = _mm_loadu_si128(str.as_ptr().cast());
    if str.len() > 15 {
        return None;
    }

    // locate dots
    let dotmask = {
        let dot = _mm_set1_epi8(b'.' as i8);
        let t0 = _mm_cmpeq_epi8(input, dot);
        let mut dotmask = _mm_movemask_epi8(t0) as u16;
        let mask = 1 << str.len();
        dotmask &= mask - 1;
        dotmask |= mask;
        dotmask
    };

    // build a hashcode
    let hashcode = ((6639 * dotmask as u32) >> 13) as u8;

    // grab the index of the shuffle mask
    let id = PATTERNS_ID[hashcode as usize];
    if id >= 81 {
        return None;
    }
    let pat: *const u8 = &PATTERNS[id as usize][0];
    let pattern = _mm_loadu_si128(pat.cast());
    // The value of the shuffle mask at a specific index points at the last digit,
    // we check that it matches the length of the input.
    let ascii0 = _mm_set1_epi8(b'0' as i8);
    let t0 = input;
    let t1 = _mm_shuffle_epi8(t0, pattern);
    // check that leading digits of 2- 3- numbers are not zeros.
    {
        let eq0 = _mm_cmpeq_epi8(t1, ascii0);
        if _mm_testz_si128(
            eq0,
            _mm_set_epi8(-1, 0, -1, 0, -1, 0, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0),
        ) == 0
        {
            return None;
        }
    }
    // replace null values with '0'
    let t1b = _mm_blendv_epi8(t1, ascii0, pattern);

    // subtract '0'
    let t2 = _mm_sub_epi8(t1b, ascii0);
    // check that everything was in the range '0' to '9'
    {
        let c9 = _mm_set1_epi8((b'9' - b'0') as i8);
        let t2m = _mm_max_epu8(t2, c9);
        let t2me = _mm_cmpeq_epi8(t2m, c9);
        if _mm_test_all_ones(t2me) == 0 {
            return None;
        }
    }
    // We do the computation, the Mula way.
    let weights = _mm_setr_epi8(1, 10, 1, 10, 1, 10, 1, 10, 100, 0, 100, 0, 100, 0, 100, 0);
    let t3 = _mm_maddubs_epi16(t2, weights);
    let t4 = _mm_alignr_epi8(t3, t3, 8);
    let t5 = _mm_add_epi16(t4, t3);
    // Test that we don't overflow (over 255)
    if _mm_testz_si128(
        t5,
        _mm_set_epi8(0, 0, 0, 0, 0, 0, 0, 0, -1, 0, -1, 0, -1, 0, -1, 0),
    ) == 0
    {
        return None;
    }
    // pack and we are done!
    let t6 = _mm_packus_epi16(t5, t5);
    Some(_mm_cvtsi128_si32(t6) as u32)
}

#[must_use]
pub fn inet_aton(str: &[u8]) -> Option<u32> {
    unsafe { inet_aton_impl(str) }
}

mod pattern;

#[cfg(test)]
mod tests {

    use std::{net::Ipv4Addr, str::FromStr};

    use super::inet_aton;
    use proptest::prelude::*;

    const IPV4: &str =
        "((25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.){3}(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)";

    proptest! {
        #[test]
        fn test_ip(ip in IPV4) {
            let left = inet_aton(ip.as_bytes()).map(u32::swap_bytes);
            let right = Ipv4Addr::from_str(&ip).ok().map(Into::into);
            assert_eq!(left, right);
        }
    }
}
