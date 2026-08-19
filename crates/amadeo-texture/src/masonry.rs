//! Courses of stone, and the joints between them.
//!
//! # The defect this exists to end
//!
//! `games/atrium` generated its stone from a **4 × 4 square lattice in stack bond**: identical slab
//! sizes, joints running unbroken in both directions, repeating every tile. Engine gate review 12
//! called it the single biggest machine-made tell in the project and it was right — it was on the
//! floor, the walls, the pillars, the galleries, the ceilings and the plinth at once, so the room
//! read as a municipal swimming pool. Real ashlar has not looked like that since before the Romans.
//!
//! Three things separate masonry from a grid, and all three are here:
//!
//! - **A lap.** Each course's joints sit over the middle of the stones below, so no vertical joint
//!   runs through two courses. It is the whole reason a wall stands up.
//! - **Varied sizes.** Courses differ in height and slabs differ in width, because stone is cut from
//!   blocks and no two blocks are the same.
//! - **No joint at the tile seam.** A lattice whose cells start at `u = 0` puts a joint down the
//!   edge of every copy of the texture, which is a repeat you can count across a wall.
//!
//! # A wall is laid once, then sampled
//!
//! [`Courses`] is the *description* and [`Wall`] is the laid result. That split is not ceremony: a
//! course is placed **relative to the one below it**, which is how a mason guarantees a lap, and
//! resolving that per pixel would mean walking every course from the bottom for every texel.
//!
//! # How it tiles despite all of that
//!
//! Course heights and slab widths are drawn from a hash and then **normalised to sum to one**, so
//! the last one always closes the loop exactly. The first course is then rotated by an arbitrary
//! amount, which is what moves the seam joint off `u = 0`.
//!
//! Everything here is `+ - * /`, `floor` and integer hashing (ADR 0044), so a committed texture is
//! byte-identical on every machine.

use crate::noise::hash01;

/// How courses are offset against each other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bond {
    /// Every course starts at the same place, so vertical joints run the whole height.
    ///
    /// **Almost always wrong**, and it is here to be nameable rather than to be used: it is what
    /// tiling is, and a tiled wall is a real material. Reaching for it by accident is the defect
    /// this module was written for.
    Stack,
    /// Each course is offset from the one below by a fixed fraction of a slab.
    ///
    /// `0.5` is the common half-lap of brickwork. It still produces a *regular* pattern — every
    /// other course lines up exactly — which is honest for brick and too tidy for cut stone.
    Regular {
        /// How far each course steps on from the one below, as a fraction of a slab.
        lap: f32,
    },
    /// Each course's joints are placed over the middle of the stones below, jittered.
    ///
    /// What ashlar actually looks like, and the only variant whose lap is **guaranteed** rather than
    /// likely: a joint is positioned from its neighbours below rather than from a global offset, so
    /// no amount of size variation can let two line up. This is the one worth reaching for.
    Broken,
}

/// A wall of courses over the unit square, before it is laid.
#[derive(Debug, Clone)]
pub struct Courses {
    /// Which pattern this is. Change the seed and every slab in the wall is a different size.
    pub seed: u64,
    /// How many courses stack up the tile.
    pub rows: u32,
    /// How many slabs sit across one course.
    pub across: u32,
    /// How much course heights and slab widths may differ from the average, as a fraction.
    ///
    /// `0.0` gives a perfectly regular grid with a lap. `0.35` gives visibly hand-cut stone. Above
    /// about `0.6` the extremes start to read as a mistake rather than as variation.
    pub variation: f32,
    /// How wide a joint is, as a fraction of the tile.
    pub joint: f32,
    /// How courses line up against each other.
    pub bond: Bond,
}

/// Which slab a point landed on, and how deep in a joint it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stone {
    /// Which course, counting from the bottom of the tile.
    pub course: i64,
    /// Which slab within that course.
    pub slab: i64,
    /// `0.0` on an open face, rising to `1.0` in the middle of a joint.
    ///
    /// Ramped rather than a hard step, so a joint seen at a grazing angle does not alias into a
    /// staircase.
    pub joint: f32,
}

/// One laid course: where it sits, and where its vertical joints are.
#[derive(Debug, Clone)]
struct Course {
    /// Lower edge, in `0..1` up the tile.
    low: f32,
    /// Upper edge.
    high: f32,
    /// Vertical joint positions in `0..1`, ascending. A slab runs from one to the next, and the last
    /// wraps round to the first.
    joints: Vec<f32>,
}

/// A laid wall, ready to sample.
#[derive(Debug, Clone)]
pub struct Wall {
    seed: u64,
    joint: f32,
    courses: Vec<Course>,
}

impl Courses {
    /// Lays the wall: works out every course's height and every stone's width, once.
    #[must_use]
    pub fn lay(&self) -> Wall {
        let rows = self.rows.max(1);
        let across = self.across.max(1);

        // Course heights, drawn from the hash and normalised so they close at the top of the tile.
        let heights: Vec<f32> = (0..i64::from(rows))
            .map(|row| 1.0 + (hash01(self.seed ^ 0x00C0, row, 0) - 0.5) * 2.0 * self.variation)
            .collect();
        let total: f32 = heights.iter().sum();

        let mut courses: Vec<Course> = Vec::with_capacity(rows as usize);
        let mut edge = 0.0;
        for (row, height) in heights.iter().enumerate() {
            let share = height / total;
            let joints = self.joints_for(row, across, courses.last());
            courses.push(Course {
                low: edge,
                high: edge + share,
                joints,
            });
            edge += share;
        }

        Wall {
            seed: self.seed,
            joint: self.joint,
            courses,
        }
    }

    /// Where one course's vertical joints go.
    ///
    /// `below` is the course underneath, which is what [`Bond::Broken`] positions against.
    fn joints_for(&self, row: usize, across: u32, below: Option<&Course>) -> Vec<f32> {
        let row = row as i64;

        // Widths from the hash, normalised, then rotated by `offset` so the seam is not a joint.
        let independent = |offset: f32| -> Vec<f32> {
            let widths: Vec<f32> = (0..i64::from(across))
                .map(|slab| {
                    1.0 + (hash01(self.seed ^ 0x00C2, row, slab) - 0.5) * 2.0 * self.variation
                })
                .collect();
            let total: f32 = widths.iter().sum();
            let mut joints = Vec::with_capacity(widths.len());
            let mut edge = 0.0;
            for width in &widths {
                joints.push((edge + offset).rem_euclid(1.0));
                edge += width / total;
            }
            joints.sort_by(f32::total_cmp);
            joints
        };

        match self.bond {
            Bond::Stack => independent(0.0),
            Bond::Regular { lap } => {
                let slab = 1.0 / f32::from(u16::try_from(across).unwrap_or(1));
                let step = f32::from(u16::try_from(row.rem_euclid(1024)).unwrap_or(0));
                independent((step * lap * slab).rem_euclid(1.0))
            }
            Bond::Broken => {
                let Some(below) = below else {
                    // The first course has nothing to lap, so it is laid freely — offset by an
                    // arbitrary amount so the tile seam falls in the middle of a stone.
                    return independent(0.5 * hash01(self.seed ^ 0x00C4, 7, 0) + 0.25);
                };

                // **Each joint goes over the middle of the stone below it**, jittered within that
                // stone's middle half. That is what makes the lap a guarantee rather than a
                // probability: whatever the widths came out as, a new joint is at least a quarter of
                // a stone from either joint below it, and no amount of size variation can change it.
                let mut joints = Vec::with_capacity(below.joints.len());
                for (index, left) in below.joints.iter().enumerate() {
                    let right = below.joints[(index + 1) % below.joints.len()];
                    // Wrapped, because the last stone runs off the right edge and back on at the
                    // left. A raw subtraction is negative there, which would place the joint outside
                    // the stone entirely.
                    let span = (right - left).rem_euclid(1.0);
                    let jitter = 0.25 + hash01(self.seed ^ 0x00C5, row, index as i64) * 0.5;
                    joints.push((left + span * jitter).rem_euclid(1.0));
                }
                joints.sort_by(f32::total_cmp);
                joints
            }
        }
    }
}

impl Wall {
    /// Which stone covers `(u, v)`, and how close to a joint the point is.
    ///
    /// Anything outside `0..1` is wrapped, so a caller sampling a neighbour for a gradient does not
    /// have to handle the edge.
    #[must_use]
    pub fn at(&self, u: f32, v: f32) -> Stone {
        let u = u.rem_euclid(1.0);
        let v = v.rem_euclid(1.0);

        let index = self.course_at(v);
        let course = &self.courses[index];
        let height = (course.high - course.low).max(1e-6);
        // How far up its own course the point is, which is what makes the horizontal joint's ramp
        // the same width whatever height the course happens to be.
        let up = (v - course.low) / height;

        let (slab, left, width) = slab_at(&course.joints, u);
        let along = (u - left).rem_euclid(1.0) / width.max(1e-6);

        // Distance to the nearest edge in each direction, back in tile units. `min` of the two is
        // the distance to the nearest joint of any kind, which is what rounds the corner where four
        // stones meet rather than leaving a cross.
        let to_horizontal = up.min(1.0 - up) * height;
        let to_vertical = along.min(1.0 - along) * width;
        let edge = to_horizontal.min(to_vertical);

        let half = (self.joint * 0.5).max(1e-6);
        let joint = (1.0 - (edge / half).clamp(0.0, 1.0)).clamp(0.0, 1.0);

        Stone {
            course: index as i64,
            slab,
            joint,
        }
    }

    /// A stable number in `0..1` for one slab — its tone, its wear, whatever the caller wants.
    ///
    /// **Flat across a slab and discontinuous at the joint**, which is what "these are separate
    /// blocks" looks like. Noise cannot produce it: noise is continuous by construction, so a tone
    /// sampled from noise fades across a joint and reads as a stain rather than as a different
    /// stone.
    #[must_use]
    pub fn tone(&self, stone: Stone, salt: u64) -> f32 {
        hash01(self.seed ^ salt, stone.course, stone.slab)
    }

    /// How many courses this wall was laid in.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.courses.len()
    }

    /// Where one course's vertical joints sit, for a test or a diagnostic.
    #[must_use]
    pub fn joints(&self, course: usize) -> &[f32] {
        &self.courses[course].joints
    }

    /// One course's lower and upper edge.
    #[must_use]
    pub fn extent(&self, course: usize) -> (f32, f32) {
        (self.courses[course].low, self.courses[course].high)
    }

    /// How wide a joint is, as a fraction of the tile.
    #[must_use]
    pub fn joint_width(&self) -> f32 {
        self.joint
    }

    /// Which course covers `v`.
    fn course_at(&self, v: f32) -> usize {
        for (index, course) in self.courses.iter().enumerate() {
            if v < course.high {
                return index;
            }
        }
        self.courses.len() - 1
    }
}

/// Which slab of a course covers `u`: its index, its left edge, and its width.
///
/// The joints are ascending, so the slab is the one whose left joint is the last at or below `u` —
/// except below the first joint, which belongs to the **last** slab, because that one wraps round
/// the tile seam.
fn slab_at(joints: &[f32], u: f32) -> (i64, f32, f32) {
    if joints.is_empty() {
        return (0, 0.0, 1.0);
    }
    let mut index = joints.len() - 1;
    for (slot, joint) in joints.iter().enumerate() {
        if u < *joint {
            index = if slot == 0 {
                joints.len() - 1
            } else {
                slot - 1
            };
            break;
        }
    }
    let left = joints[index];
    let right = joints[(index + 1) % joints.len()];
    // **Wrapped, and this was a real defect.** A slab straddling the tile seam has `right < left`,
    // so a raw subtraction is negative — and clamping that to a tiny positive number made the
    // position within the slab enormous, which made the whole straddling stone read as joint. It
    // showed up as joints covering twenty-eight per cent of the tile instead of four.
    let width = (right - left).rem_euclid(1.0);
    (index as i64, left, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ashlar() -> Courses {
        Courses {
            seed: 0x51A9_3C7E,
            rows: 6,
            across: 4,
            variation: 0.35,
            joint: 0.008,
            bond: Bond::Broken,
        }
    }

    /// How close the nearest pair of joints in two courses comes.
    fn closest_joint(wall: &Wall, first: usize, second: usize) -> f32 {
        let mut closest = f32::MAX;
        for left in wall.joints(first) {
            for right in wall.joints(second) {
                // Wrapped, because a joint at 0.99 and one at 0.01 are two hundredths apart on a
                // surface that repeats, not ninety-eight.
                let gap = (left - right).abs();
                closest = closest.min(gap.min(1.0 - gap));
            }
        }
        closest
    }

    #[test]
    fn every_course_laps_the_one_below_it() {
        // **Review 12's close condition for this**, and the reason `Bond::Broken` positions each
        // joint from the stones below rather than from a global offset.
        //
        // A first attempt drew a fresh offset per course and stepped each on by a third to two
        // thirds of a slab. That sounds sufficient and is not: slab widths vary, so a joint's actual
        // position drifts from where the offset put it, and two courses came out 0.0035 apart — a
        // vertical line running through two courses, which is the grid this module exists to break.
        //
        // Placing a joint in the middle half of the stone below makes the lap arithmetic rather than
        // statistical: a quarter of a stone, whatever the widths turned out to be.
        // **Measured against the narrowest stone in the course below, not against the average**, and
        // that distinction is the guarantee stated exactly. A quarter of a *stone* is what the jitter
        // range promises, and stones vary — so a course whose narrowest stone is 0.16 wide laps by
        // 0.04, which is correct and is well under a quarter of the 0.25 average. Asserting against
        // the average would be asserting something the construction never claimed.
        let wall = ashlar().lay();
        for course in 1..wall.rows() {
            let joints = wall.joints(course - 1);
            let narrowest = (0..joints.len())
                .map(|index| (joints[(index + 1) % joints.len()] - joints[index]).rem_euclid(1.0))
                .fold(f32::MAX, f32::min);
            let gap = closest_joint(&wall, course - 1, course);
            assert!(
                gap > narrowest * 0.24,
                "courses {} and {course} have joints only {gap:.4} apart, under a quarter of the \
                 {narrowest:.4} narrowest stone below them — a vertical line running through two \
                 courses",
                course - 1
            );
        }
    }

    #[test]
    fn stack_bond_lines_every_course_up_and_that_is_the_control() {
        // The test above is worth nothing unless it can fail, and `Bond::Stack` is the arrangement
        // that must fail it: every course is laid from the same offset, so every course has a joint
        // at zero. Without this, an implementation whose courses had no joints at all would pass the
        // assertion above perfectly.
        let wall = Courses {
            bond: Bond::Stack,
            ..ashlar()
        }
        .lay();
        let gap = closest_joint(&wall, 0, 1);
        assert!(
            gap < 0.001,
            "in stack bond adjacent courses must line up — the closest pair was {gap:.4} apart, \
             which means the offset is coming from somewhere other than the bond"
        );
    }

    #[test]
    fn courses_and_slabs_both_vary_in_size() {
        // Variation is the second of the three things that separate stone from tile, and a
        // normalisation bug would silently make every course the same height while everything else
        // still worked.
        let wall = ashlar().lay();
        let heights: Vec<f32> = (0..wall.rows())
            .map(|course| {
                let (low, high) = wall.extent(course);
                high - low
            })
            .collect();
        let lowest = heights.iter().copied().fold(f32::MAX, f32::min);
        let highest = heights.iter().copied().fold(f32::MIN, f32::max);
        assert!(
            highest / lowest > 1.2,
            "course heights spanned {lowest:.4}..{highest:.4}, which is a regular grid"
        );

        // And slab widths within one course, for the same reason one level down.
        let joints = wall.joints(0);
        let mut widths: Vec<f32> = (0..joints.len())
            .map(|index| (joints[(index + 1) % joints.len()] - joints[index]).rem_euclid(1.0))
            .collect();
        widths.sort_by(f32::total_cmp);
        assert!(
            widths[widths.len() - 1] / widths[0] > 1.15,
            "slab widths in one course spanned {widths:?}, which is a regular grid one level down"
        );
    }

    #[test]
    fn the_whole_tile_is_covered_and_joints_are_a_small_share_of_it() {
        // Two failures in one assertion, both of which look like a broken material rather than a
        // broken lattice. It caught the real one: a slab straddling the tile seam came out with a
        // negative width, so the entire stone read as joint and joints covered 28% of the tile.
        let wall = ashlar().lay();
        let mut in_joint = 0u32;
        let samples: u32 = 256;
        for y in 0..samples {
            for x in 0..samples {
                let stone = wall.at(
                    f32::from(u16::try_from(x).unwrap_or(0)) / 256.0,
                    f32::from(u16::try_from(y).unwrap_or(0)) / 256.0,
                );
                assert!((0.0..=1.0).contains(&stone.joint));
                if stone.joint > 0.5 {
                    in_joint += 1;
                }
            }
        }
        let share = f64::from(in_joint) / f64::from(samples * samples);
        assert!(
            (0.002..0.15).contains(&share),
            "joints cover {:.1}% of the tile — expected a few per cent",
            share * 100.0
        );
    }

    #[test]
    fn a_slabs_tone_is_flat_across_it_and_changes_at_the_joint() {
        // The property that makes stone read as separate blocks. Sampled along one course: the tone
        // must be constant while the slab index holds and must move when it changes.
        let wall = ashlar().lay();
        let mut changes = 0;
        let mut previous = wall.at(0.0, 0.5);
        let mut previous_tone = wall.tone(previous, 0);
        for step in 1..2000u16 {
            let stone = wall.at(f32::from(step) / 2000.0, 0.5);
            let tone = wall.tone(stone, 0);
            if stone.slab == previous.slab {
                assert!(
                    (tone - previous_tone).abs() < 1e-6,
                    "tone drifted within one slab, which reads as a stain rather than as stone"
                );
            } else {
                changes += 1;
            }
            previous = stone;
            previous_tone = tone;
        }
        assert!(changes >= 2, "the scanline crossed only {changes} joints");
    }

    #[test]
    fn the_tile_seam_is_not_a_joint() {
        // The third of the three things, and the one that shows as a countable vertical line every
        // time the texture repeats across a wall. The first course is laid from an arbitrary offset
        // for exactly this reason, and every course above it inherits the shift.
        let wall = ashlar().lay();
        for course in 0..wall.rows() {
            for joint in wall.joints(course) {
                let from_seam = joint.min(1.0 - joint);
                assert!(
                    from_seam > wall.joint_width(),
                    "course {course} has a joint {from_seam:.4} from the tile seam, which puts a \
                     line down the edge of every copy of this texture"
                );
            }
        }
    }
}
