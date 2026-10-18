//! Waveform extrema to SVG path commands.
use auditorylab::Extent;
use std::fmt::Write;
pub(crate) fn paths(bins: &[[Extent; 2]]) -> [String; 2] {
    let mut paths = [String::new(), String::new()];
    let count = f64::from(u32::try_from(bins.len()).unwrap_or(512).max(1));
    for (i, bin) in bins.iter().enumerate() {
        let x = f64::from(u32::try_from(i).unwrap_or(0)) * 1024.0 / count;
        for ear in 0..2 {
            let _ = write!(
                paths[ear],
                "{}{x:.3},{:.3} L{x:.3},{:.3} ",
                if i == 0 { "M" } else { "L" },
                (1.0 - bin[ear].max) * 50.0,
                (1.0 - bin[ear].min) * 50.0
            );
        }
    }
    paths
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_scale_preserves_both_polarities_and_ear_routes() {
        assert_eq!(paths(&[]), ["", ""]);
        let p = paths(&[[
            Extent {
                min: -1.0,
                max: 1.0,
            },
            Extent { min: 0.0, max: 0.0 },
        ]]);
        assert_eq!(p[0], "M0.000,0.000 L0.000,100.000 ");
        assert_eq!(p[1], "M0.000,50.000 L0.000,50.000 ");
    }
}
