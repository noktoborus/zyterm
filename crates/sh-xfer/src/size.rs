//! A size as a listing writes it.

/// A size in the shortest form that still says it.
pub fn short_size(bytes: u64) -> String {
    const STEP: u64 = 1024;
    let units = ["B", "K", "M", "G", "T"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= STEP as f64 && unit + 1 < units.len() {
        value /= STEP as f64;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", units[0])
    } else {
        format!("{value:.1}{}", units[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_is_as_short_as_it_can_be() {
        assert_eq!(short_size(0), "0 B");
        assert_eq!(short_size(512), "512 B");
        assert_eq!(short_size(4096), "4.0K");
        assert_eq!(short_size(1_258_291), "1.2M");
    }

    #[test]
    fn every_step_has_a_letter_of_its_own_and_the_last_one_keeps_it() {
        assert_eq!(short_size(1024), "1.0K");
        assert_eq!(short_size(1024 * 1024), "1.0M");
        assert_eq!(short_size(1024 * 1024 * 1024), "1.0G");
        assert_eq!(short_size(1024_u64.pow(4)), "1.0T");
        assert_eq!(short_size(1024_u64.pow(5)), "1024.0T");
    }
}
