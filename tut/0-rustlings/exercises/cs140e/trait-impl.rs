// FIXME: Make me pass! Diff budget: 25 lines.

use std::cmp::PartialEq;

enum Duration {
    MilliSeconds(u64),
    Seconds(u32),
    Minutes(u16)
}
use Duration::*;

fn to_ms(d: &Duration) -> u64 {
    match d {
        MilliSeconds(ms) => *ms,
        Seconds(s) => (*s as u64) * 1000,
        Minutes(m) => (*m as u64) * 1000 * 60
    }
}

impl PartialEq for Duration {
    fn eq(&self, other: &Self) -> bool {
        to_ms(self) == to_ms(other)
    }
}

#[test]
fn traits() {
    assert_eq!(Seconds(120), Minutes(2));
    assert_eq!(Seconds(420), Minutes(7));
    assert_eq!(MilliSeconds(420000), Minutes(7));
    assert_eq!(MilliSeconds(43000), Seconds(43));
}
