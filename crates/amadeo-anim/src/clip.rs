//! What a `.anim` file holds: tracks of keyframes, and how to sample one — ADR 0066.

use amadeo_core::StableHash;
use amadeo_ecs::Component;
use amadeo_reflect::Reflect;

/// How a track gets from one key to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum Interpolation {
    /// Hold the earlier key until the later one is reached.
    ///
    /// What a **flipbook** wants: a sprite showing frame 3 and then frame 4 was never showing
    /// frame 3.5, and a linear track over a tilesheet index produces exactly that — a frame nobody
    /// drew, for one tick, on every transition.
    Step,
    /// Move evenly between the two keys.
    ///
    /// The default, and what motion wants. Linear rather than any smoother curve because
    /// `+ - * /` is all IEEE 754 pins exactly (ADR 0044), and because easing is expressible by
    /// putting more keys in — which is a thing an author can see in the file.
    #[default]
    Linear,
}

/// One keyframe: a time, and the numbers the field holds at it.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct Key {
    /// Seconds from the start of the clip.
    ///
    /// No declared range: a clip's length is whatever it is, and a bound here would be a number
    /// somebody invented that an editor would then enforce.
    #[reflect(unit = "s")]
    pub time: f32,
    /// The value, as numbers.
    ///
    /// **The width is not declared and does not have to match anything here.** One number animates a
    /// scalar field, three a translation, four a colour — and which of those it is comes from the
    /// *target field's* own shape when the value is applied (ADR 0066 §2). That is what keeps the
    /// component's schema the single description of what its fields are, rather than restating it in
    /// every clip that touches one.
    pub value: Vec<f32>,
}

/// One field of one component, animated over time.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct Track {
    /// The component's canonical name, as `describe` reports it — `"Transform"`, `"PointLight"`.
    ///
    /// The canonical name rather than the Rust path, which is what ADR 0017 makes the identity of a
    /// component: moving a type between crates does not break a clip, and renaming one does.
    pub component: String,
    /// The field on it — `"translation"`, `"intensity"`.
    pub field: String,
    /// How to get from one key to the next.
    pub interpolation: Interpolation,
    /// The keys, in ascending time order.
    ///
    /// Not sorted on load, deliberately: `amadeo fmt` rewriting a file into a different order than
    /// the author wrote would break byte-stability against the thing they typed (I2). An
    /// out-of-order track is reported by [`AnimationClip::problems`] instead.
    pub keys: Vec<Key>,
}

/// A named piece of animation — what a `.anim` file is.
///
/// # It is a `Component`, and that is what makes the file work
///
/// `.material`, `.environment` and `.theme` are all scene files holding one component (ADR 0033,
/// 0034, 0064), and this is the fourth. `amadeo-anim` sits below `amadeo-scene`, so it cannot parse
/// its own asset — the app layer, which can see both crates, reads it. `amadeo fmt` and
/// `amadeo check` work on a `.anim` unchanged, for nothing.
#[derive(Debug, Clone, PartialEq, Default, StableHash, Reflect)]
pub struct AnimationClip {
    /// How long the clip runs, in seconds.
    ///
    /// Authored rather than derived from the last key, because the two are different things: a clip
    /// can hold a beat of stillness after its last movement, and a looping one that ended on its
    /// last key would have no gap between the end and the start.
    #[reflect(unit = "s")]
    pub duration: f32,
    /// The fields this clip animates.
    pub tracks: Vec<Track>,
}

impl Component for AnimationClip {}

impl Track {
    /// The numbers this track holds at `time`, or `None` if it has no keys.
    ///
    /// Clamps at both ends: before the first key it holds the first value, after the last it holds
    /// the last. That is what makes a clip shorter than another one on the same entity behave
    /// sensibly rather than snapping to zero.
    #[must_use]
    pub fn sample(&self, time: f32) -> Option<Vec<f32>> {
        let first = self.keys.first()?;
        let last = self.keys.last()?;

        if time <= first.time {
            return Some(first.value.clone());
        }
        if time >= last.time {
            return Some(last.value.clone());
        }

        // A linear scan. The keys of one track number in the tens, and a binary search over a list
        // that may not be sorted -- see `problems` -- would find the wrong segment silently where
        // this one at least behaves predictably.
        let index = self
            .keys
            .windows(2)
            .position(|pair| time >= pair[0].time && time < pair[1].time)?;
        let (from, to) = (&self.keys[index], &self.keys[index + 1]);

        if self.interpolation == Interpolation::Step {
            return Some(from.value.clone());
        }

        let span = to.time - from.time;
        // A zero-length span is two keys at the same time. Holding the earlier one is the answer
        // that does not divide by zero, and it is also what a deliberate hard cut means.
        if span <= 0.0 {
            return Some(from.value.clone());
        }
        let t = (time - from.time) / span;

        Some(
            from.value
                .iter()
                .zip(&to.value)
                .map(|(a, b)| a + (b - a) * t)
                // Widths that disagree between two keys are the author's mistake, and `problems`
                // names it. Zipping stops at the shorter one rather than reading past the end.
                .collect(),
        )
    }
}

impl AnimationClip {
    /// Everything wrong with this clip that can be seen without a world, as readable lines.
    ///
    /// # Why this exists rather than a load-time sort or a silent shrug
    ///
    /// Every fault here produces animation that *runs* and is subtly wrong — a track whose keys are
    /// out of order interpolates backwards through a segment, two keys of different widths lose a
    /// component, a clip shorter than its last key never reaches it. None of them throws, and all of
    /// them look like the clip having been authored badly rather than read badly, which is the
    /// hardest kind of bug to attribute.
    ///
    /// So they are reported by name. ADR 0060's rule: a subsystem that can produce something wrong
    /// must be able to say what.
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();

        if self.duration <= 0.0 {
            problems.push(format!(
                "the clip's duration is {}, so nothing in it will ever be reached",
                self.duration
            ));
        }

        for track in &self.tracks {
            let target = format!("{}.{}", track.component, track.field);

            if track.keys.is_empty() {
                problems.push(format!("track `{target}` has no keys"));
                continue;
            }

            for pair in track.keys.windows(2) {
                if pair[1].time < pair[0].time {
                    problems.push(format!(
                        "track `{target}` has a key at {}s after one at {}s; keys must be in \
                         ascending time order",
                        pair[1].time, pair[0].time
                    ));
                    break;
                }
            }

            let width = track.keys[0].value.len();
            if let Some(odd) = track.keys.iter().find(|key| key.value.len() != width) {
                problems.push(format!(
                    "track `{target}` has a key at {}s with {} numbers, where the first key has \
                     {width}; every key in a track must be the same width",
                    odd.time,
                    odd.value.len()
                ));
            }

            if let Some(last) = track.keys.last()
                && last.time > self.duration
            {
                problems.push(format!(
                    "track `{target}` has a key at {}s, past the clip's duration of {}s, so it is \
                     never reached",
                    last.time, self.duration
                ));
            }
        }

        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(interpolation: Interpolation, keys: &[(f32, &[f32])]) -> Track {
        Track {
            component: "Transform".to_string(),
            field: "translation".to_string(),
            interpolation,
            keys: keys
                .iter()
                .map(|(time, value)| Key {
                    time: *time,
                    value: value.to_vec(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_linear_track_moves_evenly_between_its_keys() {
        let track = track(
            Interpolation::Linear,
            &[(0.0, &[0.0, 0.0]), (2.0, &[10.0, 4.0])],
        );
        assert_eq!(track.sample(0.0), Some(vec![0.0, 0.0]));
        assert_eq!(track.sample(1.0), Some(vec![5.0, 2.0]));
        assert_eq!(track.sample(2.0), Some(vec![10.0, 4.0]));
    }

    #[test]
    fn a_step_track_holds_the_earlier_key() {
        // **What a flipbook needs.** A sprite showing frame 3 and then frame 4 was never showing
        // frame 3.5, and a linear track over a tilesheet index draws a frame nobody authored for
        // one tick on every transition.
        let track = track(Interpolation::Step, &[(0.0, &[3.0]), (1.0, &[4.0])]);
        assert_eq!(track.sample(0.9), Some(vec![3.0]));
        assert_eq!(track.sample(1.0), Some(vec![4.0]));
    }

    #[test]
    fn sampling_clamps_at_both_ends_rather_than_falling_to_zero() {
        let track = track(Interpolation::Linear, &[(1.0, &[5.0]), (2.0, &[7.0])]);
        assert_eq!(track.sample(0.0), Some(vec![5.0]));
        assert_eq!(track.sample(9.0), Some(vec![7.0]));
    }

    #[test]
    fn two_keys_at_the_same_time_are_a_hard_cut_rather_than_a_division_by_zero() {
        let track = track(
            Interpolation::Linear,
            &[(0.0, &[1.0]), (1.0, &[2.0]), (1.0, &[9.0]), (2.0, &[9.0])],
        );
        // Sampling inside the zero-length span holds the earlier value; nothing is NaN.
        let value = track.sample(1.0).expect("keys exist");
        assert!(value[0].is_finite(), "got {value:?}");
    }

    #[test]
    fn an_empty_track_samples_to_nothing() {
        assert_eq!(track(Interpolation::Linear, &[]).sample(0.0), None);
    }

    #[test]
    fn the_faults_that_run_anyway_are_all_reported() {
        // Every one of these produces animation that plays and is quietly wrong, which is the
        // hardest kind of defect to attribute — so each is named rather than shrugged at.
        let clip = AnimationClip {
            duration: 2.0,
            tracks: vec![
                track(Interpolation::Linear, &[(1.0, &[0.0]), (0.5, &[1.0])]),
                track(Interpolation::Linear, &[(0.0, &[0.0, 0.0]), (1.0, &[1.0])]),
                track(Interpolation::Linear, &[]),
                track(Interpolation::Linear, &[(0.0, &[0.0]), (99.0, &[1.0])]),
            ],
        };

        let problems = clip.problems();
        assert_eq!(problems.len(), 4, "got {problems:?}");
        assert!(problems[0].contains("ascending"), "{}", problems[0]);
        assert!(problems[1].contains("width"), "{}", problems[1]);
        assert!(problems[2].contains("no keys"), "{}", problems[2]);
        assert!(problems[3].contains("duration"), "{}", problems[3]);
    }

    #[test]
    fn a_clip_with_nothing_wrong_reports_nothing() {
        let clip = AnimationClip {
            duration: 2.0,
            tracks: vec![track(
                Interpolation::Linear,
                &[(0.0, &[0.0]), (2.0, &[1.0])],
            )],
        };
        assert!(clip.problems().is_empty(), "{:?}", clip.problems());
    }

    #[test]
    fn a_clip_round_trips_through_the_value_tree() {
        // Invariant I8, and what makes a `.anim` file possible at all — nested lists of structs,
        // which is ADR 0032's value nesting being used for the first time by a whole asset.
        let mut registry = amadeo_reflect::TypeRegistry::new();
        registry.register::<AnimationClip>().expect("registers");

        let clip = AnimationClip {
            duration: 1.5,
            tracks: vec![track(
                Interpolation::Step,
                &[(0.0, &[1.0, 2.0, 3.0]), (1.5, &[4.0, 5.0, 6.0])],
            )],
        };
        assert_eq!(
            AnimationClip::from_value(&clip.to_value()).expect("round trips"),
            clip
        );
    }
}
