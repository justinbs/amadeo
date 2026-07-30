//! Q1 candidate D, gameplay half: the same enemy AI, compiled to WebAssembly.
//!
//! **This is the file the latency measurement perturbs.** An edit means recompiling this crate to
//! `wasm32-unknown-unknown` and handing the new module to the host, which keeps running.
//!
//! # Why there is no `unsafe` here
//!
//! The enemy records live in a `Vec` this module owns. The host writes into that `Vec`'s bytes
//! through wasmtime's bounds-checked memory API and calls [`enemy_ai`], which then reads its own
//! `Vec` through an ordinary safe borrow. Neither side needs a raw pointer dereference.
//!
//! That is the substantive difference from the cdylib candidate, where passing `&mut World` across
//! the boundary means the host must dereference a pointer whose layout nothing verifies. Here the
//! worst a malformed guest can do is compute wrong numbers — it cannot corrupt the host, because
//! WebAssembly linear memory *is* the sandbox.
//!
//! # Numbers are `f32`, and that is the point
//!
//! Everything below is `f32`, and WebAssembly's `f32` operations are IEEE-754 single precision,
//! specified exactly, with no permitted implementation variance for the operations used here. So
//! this computes bit-for-bit what the native Rust version computes — which is exactly what the
//! Luau candidate cannot do.

use std::cell::RefCell;

/// One enemy, laid out identically on both sides of the boundary.
///
/// `#[repr(C)]` because the host reconstructs these bytes field by field. Field *order* here is
/// load-bearing: reorder it without changing the host and the AI silently reads garbage.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct EnemyRecord {
    /// Current x position.
    pub x: f32,
    /// Current y position.
    pub y: f32,
    /// Patrol circuit centre, x.
    pub home_x: f32,
    /// Patrol circuit centre, y.
    pub home_y: f32,
    /// Last seen player position, x.
    pub last_x: f32,
    /// Last seen player position, y.
    pub last_y: f32,
    /// Output: velocity x.
    pub vx: f32,
    /// Output: velocity y.
    pub vy: f32,
    /// Behaviour state: 0 patrol, 1 pursue, 2 search.
    pub state: u32,
    /// Ticks remaining in search.
    pub timer: u32,
    /// Which patrol waypoint is being walked toward.
    pub waypoint: u32,
    /// Keeps the struct a round 48 bytes and its alignment obvious on both sides.
    pub padding: u32,
}

thread_local! {
    /// The enemy buffer. Allocated once by [`reserve`], then written and read by the host.
    static ENEMIES: RefCell<Vec<EnemyRecord>> = const { RefCell::new(Vec::new()) };
}

// The host supplies the engine's RNG. The script must never have its own — a second generator
// would advance independently of the simulation and void every replay assertion (invariant I3).
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn host_random(min: f32, max: f32) -> f32;
}

/// Draws from the engine's RNG.
///
/// # Safety of the call
///
/// A WebAssembly import cannot violate memory safety: it is a typed call into the host, and the
/// worst a wrong implementation can do is return an unexpected number. The `unsafe` is Rust's
/// blanket rule for `extern` calls, not a real hazard here — which is precisely the contrast with
/// the cdylib candidate, where the equivalent `unsafe` *is* a real hazard.
fn draw_random(min: f32, max: f32) -> f32 {
    unsafe { host_random(min, max) }
}

/// Sizes the enemy buffer and returns its address in linear memory.
///
/// Called once per instantiation. The `Vec` is never resized afterwards, so the address stays
/// valid for the life of the instance.
#[unsafe(no_mangle)]
pub extern "C" fn reserve(count: u32) -> u32 {
    ENEMIES.with(|cell| {
        let mut enemies = cell.borrow_mut();
        enemies.clear();
        enemies.resize(count as usize, EnemyRecord::default());
        enemies.as_ptr() as u32
    })
}

/// How many bytes one [`EnemyRecord`] occupies, so the host never hardcodes the stride.
#[unsafe(no_mangle)]
pub extern "C" fn record_size() -> u32 {
    size_of::<EnemyRecord>() as u32
}

/// A unit vector from `(fx, fy)` toward `(tx, ty)`, scaled by `speed`.
fn steer_toward(fx: f32, fy: f32, tx: f32, ty: f32, speed: f32) -> (f32, f32) {
    let dx = tx - fx;
    let dy = ty - fy;
    let length = (dx * dx + dy * dy).sqrt();
    if length < 1e-6 {
        return (0.0, 0.0);
    }
    (dx / length * speed, dy / length * speed)
}

/// The four patrol waypoints around a home position, in visit order.
fn waypoint(home_x: f32, home_y: f32, index: u32) -> (f32, f32) {
    const RADIUS: f32 = 2.0;
    match index % 4 {
        0 => (home_x + RADIUS, home_y),
        1 => (home_x, home_y + RADIUS),
        2 => (home_x - RADIUS, home_y),
        _ => (home_x, home_y - RADIUS),
    }
}

/// Runs the enemy AI over the whole buffer. Called once per tick.
///
/// Tunables arrive as arguments rather than through another shared buffer: they are few, they are
/// scalars, and a flat argument list is one less thing for the two sides to disagree about.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn enemy_ai(
    count: u32,
    player_x: f32,
    player_y: f32,
    sight_range: f32,
    lose_range: f32,
    patrol_speed: f32,
    pursue_speed: f32,
    search_ticks: u32,
    waypoint_radius: f32,
    search_jitter: f32,
) {
    ENEMIES.with(|cell| {
        let mut enemies = cell.borrow_mut();
        let sight_squared = sight_range * sight_range;
        let lose_squared = lose_range * lose_range;

        for enemy in enemies.iter_mut().take(count as usize) {
            let dx = player_x - enemy.x;
            let dy = player_y - enemy.y;
            let distance_squared = dx * dx + dy * dy;

            let mut vx = 0.0;
            let mut vy = 0.0;

            match enemy.state {
                0 => {
                    if distance_squared <= sight_squared {
                        enemy.state = 1;
                    } else {
                        let (tx, ty) = waypoint(enemy.home_x, enemy.home_y, enemy.waypoint);
                        let steered = steer_toward(enemy.x, enemy.y, tx, ty, patrol_speed);
                        vx = steered.0;
                        vy = steered.1;

                        let ax = tx - enemy.x;
                        let ay = ty - enemy.y;
                        if (ax * ax + ay * ay).sqrt() <= waypoint_radius {
                            enemy.waypoint = (enemy.waypoint + 1) % 4;
                        }
                    }
                }
                1 => {
                    if distance_squared > lose_squared {
                        // x then y, matching the specification's draw order exactly.
                        let jitter_x = draw_random(-search_jitter, search_jitter);
                        let jitter_y = draw_random(-search_jitter, search_jitter);
                        enemy.state = 2;
                        enemy.timer = search_ticks;
                        enemy.last_x = player_x + jitter_x;
                        enemy.last_y = player_y + jitter_y;
                    } else {
                        enemy.last_x = player_x;
                        enemy.last_y = player_y;
                        let steered =
                            steer_toward(enemy.x, enemy.y, player_x, player_y, pursue_speed);
                        vx = steered.0;
                        vy = steered.1;
                    }
                }
                _ => {
                    if distance_squared <= sight_squared {
                        enemy.state = 1;
                    } else {
                        let steered = steer_toward(
                            enemy.x,
                            enemy.y,
                            enemy.last_x,
                            enemy.last_y,
                            patrol_speed,
                        );
                        vx = steered.0;
                        vy = steered.1;
                        enemy.timer = enemy.timer.saturating_sub(1);
                        if enemy.timer == 0 {
                            enemy.state = 0;
                        }
                    }
                }
            }

            enemy.vx = vx;
            enemy.vy = vy;
        }
    });
}
