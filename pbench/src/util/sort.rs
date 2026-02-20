//! Natural (numeric-aware) string comparison.
//!
//! [`NaturalCmp`] sorts `"bench_2"` before `"bench_10"` by comparing
//! embedded numeric subsequences by value rather than lexicographically.

use std::cmp::Ordering;
use std::iter::Peekable;
use std::str::Chars;

/// Natural string comparing utils that sorts numeric subsequences by value.
pub struct NaturalCmp;

impl NaturalCmp {
    pub(crate) fn compare(a: &str, b: &str) -> Ordering {
        let mut a_chars: Peekable<Chars<'_>> = a.chars().peekable();
        let mut b_chars: Peekable<Chars<'_>> = b.chars().peekable();

        loop {
            match (a_chars.peek(), b_chars.peek()) {
                (None, None) => return Ordering::Equal,

                (None, Some(_)) => return Ordering::Less,

                (Some(_), None) => return Ordering::Greater,

                (Some(&a_char), Some(&b_char)) => {
                    if a_char.is_ascii_digit() && b_char.is_ascii_digit() {
                        let a_num: u64 = Self::consume_number(&mut a_chars);
                        let b_num: u64 = Self::consume_number(&mut b_chars);
                        let cmp: Ordering = a_num.cmp(&b_num);

                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    } else {
                        let cmp: Ordering = a_char.cmp(&b_char);

                        if cmp != Ordering::Equal {
                            return cmp;
                        }

                        let _: Option<char> = a_chars.next();
                        let _: Option<char> = b_chars.next();
                    }
                }
            }
        }
    }

    /// Consume consecutive ASCII digits and parse them as `u64`.
    fn consume_number(chars: &mut Peekable<Chars<'_>>) -> u64 {
        let mut n: u64 = 0;

        while let Some(&ch) = chars.peek() {
            if ch.is_ascii_digit() {
                n = n
                    .saturating_mul(10)
                    .saturating_add(u64::from(ch as u32 - '0' as u32));

                let _: Option<char> = chars.next();
            } else {
                break;
            }
        }

        n
    }
}

#[cfg(test)]
mod unit_tests;
