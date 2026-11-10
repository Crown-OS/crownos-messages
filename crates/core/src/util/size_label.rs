const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];

/// `bytes` as people read sizes: "820 bytes", "14 KB", "2.4 MB".
#[must_use]
pub fn size_label(bytes: u64) -> String {
    if bytes < 1000 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 1000.0 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    let name = UNITS.get(unit).copied().unwrap_or("TB");
    if value < 10.0 {
        format!("{value:.1} {name}")
    } else {
        format!("{value:.0} {name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(size_label(820), "820 bytes");
        assert_eq!(size_label(14_200), "14 KB");
        assert_eq!(size_label(2_400_000), "2.4 MB");
        assert_eq!(size_label(1_000_000_000_000_000), "1000 TB");
    }
}
