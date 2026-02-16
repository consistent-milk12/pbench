//! Utility structs.

/// Formatting utils
pub struct Formatter;

impl Formatter {
    /// Formats an `f64` to the given number of significant figures, stripping
    /// trailing zeros after the decimal point.
    pub(crate) fn format_f64(val: f64, sig_figs: usize) -> String {
        let mut s: String = val.to_string();

        if let Some(dot_index) = s.find('.') {
            let fract_digits: usize = sig_figs.saturating_sub(dot_index);

            if fract_digits == 0 {
                s.truncate(dot_index);
            } else {
                let fract_start: usize = dot_index + 1;
                let fract_end: usize = fract_start + fract_digits;

                if let Some(fract_str) = s.get(fract_start..fract_end) {
                    // Find offset from end before trailing zeros.
                    let pre_zero: Option<usize> = fract_str
                        .bytes()
                        .rev()
                        .enumerate()
                        .find_map(|(i, b): (usize, u8)| if b == b'0' { None } else { Some(i) });

                    if let Some(pre_zero) = pre_zero {
                        s.truncate(fract_end - pre_zero);
                    } else {
                        // All fractional digits are zero — drop the dot.
                        s.truncate(dot_index);
                    }
                }
            }
        }

        s
    }
}
