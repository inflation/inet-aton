#[cfg(target_arch = "aarch64")]
use aarch64::inet_aton_impl;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use x86::inet_aton_impl;

#[must_use]
/// Convert a ASCII byte string of an IPv4 address to its integer representation.
pub fn inet_aton(str: &[u8]) -> Option<u32> {
    unsafe { inet_aton_impl(str) }
}

mod aarch64;
mod pattern;
mod x86;

#[cfg(test)]
mod tests {

    use std::{net::Ipv4Addr, str::FromStr};

    use super::inet_aton;
    use proptest::prelude::*;

    prop_compose! {
        fn ipv4_strategy()(v in prop::array::uniform4(any::<u8>())) -> String {
            v.map(|x| x.to_string()).join(".")
        }
    }

    proptest! {
        #[test]
        fn test_ip(ip in ipv4_strategy()) {
            let left = inet_aton(ip.as_bytes()).map(u32::swap_bytes);
            let right = Ipv4Addr::from_str(&ip).ok().map(Into::into);
            assert_eq!(left, right);
        }
    }
}
