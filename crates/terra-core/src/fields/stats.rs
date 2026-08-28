//! Summary statistics for a published aux channel.
//!
//! The evaluator publishes around forty named channels and, until now, nothing
//! could look at them. That made "why is my mask empty" a question you answered
//! with a debugger: the channel might be absent, present but all zero, present
//! but constant, or present and fine with the mask reading the wrong one. Those
//! are four different problems and they look identical from the viewport.
//!
//! These stats separate them, and they live in core rather than the UI so the
//! classification is testable without a window.

use crate::mask::MaskField;

/// What a channel looks like, in the terms that matter when a mask is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelShape {
    /// No samples at all.
    Empty,
    /// Every sample is the same value.
    Constant,
    /// Every sample is zero. A special case of `Constant` worth naming, because
    /// it is overwhelmingly the reason a mask reads as empty.
    AllZero,
    /// Genuine variation.
    Varying,
}

/// Summary of one published channel.
#[derive(Debug, Clone)]
pub struct ChannelStats {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub min: f32,
    pub max: f32,
    pub mean: f32,
    /// Fraction of samples greater than zero, in `[0, 1]`.
    pub coverage: f32,
    pub shape: ChannelShape,
    /// True when any sample is NaN or infinite. Never expected; worth surfacing
    /// loudly if it happens, because it poisons everything downstream.
    pub has_non_finite: bool,
}

impl ChannelStats {
    pub fn from_field(name: impl Into<String>, field: &MaskField) -> Self {
        let name = name.into();
        let data = field.data();
        let (w, h) = (field.metrics.width, field.metrics.height);

        if data.is_empty() {
            return Self {
                name,
                width: w,
                height: h,
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                coverage: 0.0,
                shape: ChannelShape::Empty,
                has_non_finite: false,
            };
        }

        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        let mut sum = 0.0f64;
        let mut positive = 0usize;
        let mut has_non_finite = false;
        for &v in data {
            if !v.is_finite() {
                has_non_finite = true;
                continue;
            }
            min = min.min(v);
            max = max.max(v);
            sum += v as f64;
            if v > 0.0 {
                positive += 1;
            }
        }
        // Every sample was non-finite: there is no range to report.
        if !min.is_finite() || !max.is_finite() {
            return Self {
                name,
                width: w,
                height: h,
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                coverage: 0.0,
                shape: ChannelShape::Empty,
                has_non_finite,
            };
        }

        let n = data.len();
        let mean = (sum / n as f64) as f32;
        let coverage = positive as f32 / n as f32;
        let span = max - min;
        let shape = if span > f32::EPSILON {
            ChannelShape::Varying
        } else if max.abs() <= f32::EPSILON {
            ChannelShape::AllZero
        } else {
            ChannelShape::Constant
        };

        Self {
            name,
            width: w,
            height: h,
            min,
            max,
            mean,
            coverage,
            shape,
            has_non_finite,
        }
    }

    /// One-line explanation of what this channel is, aimed at the question the
    /// panel exists to answer. `None` when the channel looks healthy and the
    /// numbers speak for themselves.
    pub fn diagnosis(&self) -> Option<&'static str> {
        if self.has_non_finite {
            return Some("contains NaN or infinity");
        }
        match self.shape {
            ChannelShape::Empty => Some("published but has no samples"),
            ChannelShape::AllZero => Some("all zero - nothing wrote to it"),
            ChannelShape::Constant => Some("constant - no variation to mask on"),
            ChannelShape::Varying => None,
        }
    }
}

/// Summarise every published channel, sorted by name so the list is stable
/// between frames and between runs.
pub fn summarise_channels<'a, I>(channels: I) -> Vec<ChannelStats>
where
    I: IntoIterator<Item = (&'a String, &'a MaskField)>,
{
    let mut out: Vec<ChannelStats> = channels
        .into_iter()
        .map(|(name, field)| ChannelStats::from_field(name.clone(), field))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightfield::HeightfieldMetrics;

    fn field(values: &[f32]) -> MaskField {
        let n = values.len() as u32;
        MaskField::from_raw(HeightfieldMetrics::new(n, 1, n as f32, 1.0), values)
    }

    #[test]
    fn all_zero_is_distinguished_from_constant() {
        assert_eq!(
            ChannelStats::from_field("z", &field(&[0.0, 0.0, 0.0])).shape,
            ChannelShape::AllZero
        );
        assert_eq!(
            ChannelStats::from_field("c", &field(&[0.5, 0.5, 0.5])).shape,
            ChannelShape::Constant
        );
    }

    /// The distinction the panel exists for: an absent channel, an all-zero
    /// channel and a healthy one must not read the same.
    #[test]
    fn diagnosis_separates_the_empty_mask_causes() {
        assert_eq!(
            ChannelStats::from_field("z", &field(&[0.0, 0.0])).diagnosis(),
            Some("all zero - nothing wrote to it")
        );
        assert_eq!(
            ChannelStats::from_field("c", &field(&[1.0, 1.0])).diagnosis(),
            Some("constant - no variation to mask on")
        );
        assert_eq!(
            ChannelStats::from_field("v", &field(&[0.0, 1.0])).diagnosis(),
            None
        );
    }

    #[test]
    fn stats_describe_the_range_and_coverage() {
        let s = ChannelStats::from_field("v", &field(&[0.0, 0.0, 1.0, 3.0]));
        assert_eq!(s.min, 0.0);
        assert_eq!(s.max, 3.0);
        assert!((s.mean - 1.0).abs() < 1e-6);
        assert!((s.coverage - 0.5).abs() < 1e-6, "two of four are positive");
        assert_eq!(s.shape, ChannelShape::Varying);
    }

    /// Non-finite samples must be flagged rather than silently poisoning the
    /// min/max, which is how they usually escape notice.
    #[test]
    fn non_finite_samples_are_flagged_without_wrecking_the_range() {
        let s = ChannelStats::from_field("bad", &field(&[0.0, f32::NAN, 2.0]));
        assert!(s.has_non_finite);
        assert_eq!(s.diagnosis(), Some("contains NaN or infinity"));
        assert_eq!(s.min, 0.0, "the finite samples must still describe a range");
        assert_eq!(s.max, 2.0);
    }

    #[test]
    fn an_all_non_finite_channel_does_not_report_an_infinite_range() {
        let s = ChannelStats::from_field("bad", &field(&[f32::NAN, f32::INFINITY]));
        assert!(s.has_non_finite);
        assert!(s.min.is_finite() && s.max.is_finite());
        assert_eq!(s.shape, ChannelShape::Empty);
    }

    #[test]
    fn summaries_are_sorted_for_a_stable_list() {
        use std::collections::HashMap;
        let mut m: HashMap<String, MaskField> = HashMap::new();
        for k in ["wetness", "hardness", "slope"] {
            m.insert(k.to_string(), field(&[0.0, 1.0]));
        }
        let stats = summarise_channels(m.iter());
        let names: Vec<&str> = stats.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["hardness", "slope", "wetness"]);
    }
}
