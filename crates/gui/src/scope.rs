//! Waveform extrema to SVG path commands.
use auditorylab_app::core::Extent;
use std::fmt::Write;
pub(crate) fn paths(bins: &[[Extent; 2]], limit: f64) -> [String; 2] {
    let mut paths = [String::new(), String::new()];
    let count = f64::from(u32::try_from(bins.len()).unwrap_or(512).max(1));
    for (i, bin) in bins.iter().enumerate() {
        let x = f64::from(u32::try_from(i).unwrap_or(0)) * 1024.0 / count;
        for ear in 0..2 {
            // Remove only interior points on an exactly horizontal span.
            // Every range, slope change and endpoint remains in the path.
            let extent = bin[ear];
            if i > 0
                && i + 1 < bins.len()
                && extent.min == extent.max
                && bins[i - 1][ear].min == extent.min
                && bins[i + 1][ear].max == extent.max
            {
                continue;
            }
            let _ = write!(
                paths[ear],
                "{}{x:.3},{:.3} L{x:.3},{:.3} ",
                if i == 0 { "M" } else { "L" },
                (1.0 - (bin[ear].max / limit).clamp(-1.0, 1.0)) * 50.0,
                (1.0 - (bin[ear].min / limit).clamp(-1.0, 1.0)) * 50.0
            );
        }
    }
    paths
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_fit_limit_preserves_asymmetric_ear_balance() {
        let b = [[
            Extent {
                min: -0.2,
                max: 0.2,
            },
            Extent {
                min: -0.02,
                max: 0.02,
            },
        ]];
        let fixed = paths(&b, 1.0);
        let fit = paths(&b, 0.2);
        assert_ne!(fixed, fit);
        assert_eq!(fit[0], "M0.000,0.000 L0.000,100.000 ");
        assert_eq!(fit[1], "M0.000,45.000 L0.000,55.000 ");
    }
    #[test]
    fn fixed_scale_preserves_both_polarities_and_ear_routes() {
        assert_eq!(paths(&[], 1.0), ["", ""]);
        let p = paths(
            &[[
                Extent {
                    min: -1.0,
                    max: 1.0,
                },
                Extent { min: 0.0, max: 0.0 },
            ]],
            1.0,
        );
        assert_eq!(p[0], "M0.000,0.000 L0.000,100.000 ");
        assert_eq!(p[1], "M0.000,50.000 L0.000,50.000 ");
    }
    #[test]
    fn flat_spans_keep_their_endpoints_and_each_ears_extrema() {
        let zero = Extent { min: 0.0, max: 0.0 };
        let mut bins = [[zero; 2]; 5];
        bins[2][1] = Extent {
            min: -1.0,
            max: 1.0,
        };
        let p = paths(&bins, 1.0);
        assert_eq!(
            p[0],
            "M0.000,50.000 L0.000,50.000 L819.200,50.000 L819.200,50.000 "
        );
        assert!(p[1].contains("L409.600,0.000 L409.600,100.000 "));
        assert!(p[1].contains("L204.800,50.000"));
        assert!(p[1].contains("L614.400,50.000"));
    }
    #[test]
    fn a_one_frame_pulse_keeps_the_preceding_closed_bin() {
        let zero = Extent { min: 0.0, max: 0.0 };
        let mut bins = [[zero; 2]; 5];
        bins[2][0] = Extent { min: 0.0, max: 1.0 };
        let p = paths(&bins, 1.0);
        assert_eq!(
            p[0],
            "M0.000,50.000 L0.000,50.000 L204.800,50.000 L204.800,50.000 L409.600,0.000 L409.600,50.000 L819.200,50.000 L819.200,50.000 "
        );
    }
    #[test]
    fn flat_pulse_plateaus_keep_both_transition_edges() {
        let bins = [0.0, 0.0, 0.5, 0.5, 0.5, 0.0, 0.0].map(|level| {
            [Extent {
                min: level,
                max: level,
            }; 2]
        });
        let p = paths(&bins, 1.0);
        for ear in p {
            assert!(ear.contains("L146.286,50.000"));
            assert!(ear.contains("L292.571,25.000"));
            assert!(!ear.contains("438.857"));
            assert!(ear.contains("L585.143,25.000"));
            assert!(ear.contains("L731.429,50.000"));
            assert!(ear.contains("L877.714,50.000"));
        }
    }
}
