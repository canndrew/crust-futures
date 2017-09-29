use std::net::IpAddr;
use std::str::FromStr;

#[test]
fn test() {
    assert_eq!(ip!(1.2.3.4), IpAddr::from_str("1.2.3.4").unwrap());
}

