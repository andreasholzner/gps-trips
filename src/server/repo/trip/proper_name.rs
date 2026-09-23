//! What counts as a proper trip name (US-66): one that leads with the date
//! US-12 suggests and then says something. The trip list's "Unnamed" filter
//! keeps the trips whose names fall short of that.

use time::Date;

use super::DATE_FORMAT;

/// Whether `name` starts with a real `YYYY-MM-DD` calendar date and has some
/// text after it. A bare date, or a date followed only by whitespace, is what
/// is left when the owner accepted US-12's suggestion without typing a title,
/// so it is not a proper name; nor is a date that does not exist, which is a
/// typo rather than the prefix.
pub(super) fn has_proper_name(name: &str) -> bool {
    // `get` rather than slicing: a name whose tenth byte falls inside a
    // multi-byte character cannot start with a date, and must not panic.
    let (Some(date), Some(rest)) = (name.get(..10), name.get(10..)) else {
        return false;
    };
    Date::parse(date, DATE_FORMAT).is_ok() && !rest.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_followed_by_a_title_is_a_proper_name() {
        assert!(has_proper_name("2024-06-01 Oslo Hills Walk"));
        assert!(has_proper_name("2024-06-01Hike"));
    }

    #[test]
    fn a_bare_date_is_not_a_proper_name() {
        assert!(!has_proper_name("2024-06-01"));
        assert!(!has_proper_name("2024-06-01 "));
        assert!(!has_proper_name("2024-06-01 \t "));
    }

    #[test]
    fn a_name_without_a_leading_date_is_not_a_proper_name() {
        assert!(!has_proper_name("Evening ride"));
        assert!(!has_proper_name(" 2024-06-01 Leading space"));
        assert!(!has_proper_name("24-06-01 Short year"));
        assert!(!has_proper_name(""));
    }

    #[test]
    fn a_date_that_does_not_exist_is_not_a_proper_name() {
        assert!(!has_proper_name("2024-13-45 Impossible"));
        assert!(!has_proper_name("2023-02-29 Not a leap year"));
    }

    #[test]
    fn a_multi_byte_character_near_the_start_does_not_panic() {
        assert!(!has_proper_name("Tromsø tur 2024"));
        assert!(!has_proper_name("ååååå"));
    }
}
