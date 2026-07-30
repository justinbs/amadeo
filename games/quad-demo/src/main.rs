//! M0's exit gate: a quad you can move around a window, running on the real engine loop.
//!
//! Small on purpose. Its job is to prove the whole spine connects — keyboard to actions, actions to
//! simulation, simulation to GPU — using the same fixed-timestep loop, the same input abstraction,
//! and the same renderer that a real game would.
//!
//! # Where the windowing lives, and why
//!
//! The event loop is **here, in the game**, not in the engine. `amadeo-render` sits above
//! `amadeo-app` in the dependency order (invariant I6), so the engine cannot own a loop that drives
//! both. Keeping it in the binary also means no engine crate depends on winit, which is what lets
//! `cargo test --workspace` stay windowless and fast.
//!
//! ```text
//! cargo run -p quad-demo
//! ```
//!
//! WASD or the arrow keys to move. Escape to quit.

use std::sync::Arc;
use std::time::Instant;

use amadeo_app::{App, Stage, system};
use amadeo_core::{FIXED_DT, StableHash, StableHasher};
use amadeo_ecs::{Component, World};
use amadeo_input::{ActionId, InputDriver, InputState, LiveSource, SAMPLE_INPUT, sample_input};
use amadeo_render::{
    Camera2d, Quad, RENDER_QUADS, Renderer, Transform2d, WgpuBackend, render_quads,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

// --- Game logic ---
//
// Velocity is defined here rather than in an engine crate. Movement speed is a *game* concern, and
// invariant I4 keeps that kind of knowledge out of the core.

/// How fast something is moving, in world units per second.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Velocity {
    x: f32,
    y: f32,
}

impl StableHash for Velocity {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        self.x.stable_hash(hasher);
        self.y.stable_hash(hasher);
    }
}

impl Component for Velocity {}

/// Marks the entity the player controls.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Player;

impl StableHash for Player {
    fn stable_hash(&self, _hasher: &mut StableHasher) {}
}

impl Component for Player {}

/// Action names. Constants rather than literals so a typo is a compile error.
const MOVE_X: &str = "move_x";
const MOVE_Y: &str = "move_y";

/// World units per second at full stick deflection.
const MOVE_SPEED: f32 = 6.0;

/// Turns input actions into the player's velocity.
fn apply_input(world: &mut World) {
    let Some(input) = world.resource::<InputState>() else {
        return;
    };
    let x = input.axis(ActionId::new(MOVE_X)) * MOVE_SPEED;
    let y = input.axis(ActionId::new(MOVE_Y)) * MOVE_SPEED;

    world.for_each_pair_mut::<Velocity, Player>(|_entity, velocity, _player| {
        velocity.x = x;
        velocity.y = y;
    });
}

/// Moves everything by its velocity.
///
/// Uses `FIXED_DT`, never a measured frame time. That is what makes the same input produce the same
/// result regardless of frame rate (invariant I3).
fn integrate(world: &mut World) {
    world.for_each_pair_mut::<Transform2d, Velocity>(|_entity, transform, velocity| {
        transform.position[0] += velocity.x * FIXED_DT;
        transform.position[1] += velocity.y * FIXED_DT;
    });
}

/// Keeps the player inside the visible area, so it cannot be lost off-screen.
fn clamp_to_view(world: &mut World) {
    const LIMIT_X: f32 = 8.0;
    const LIMIT_Y: f32 = 4.5;

    world.for_each_pair_mut::<Transform2d, Player>(|_entity, transform, _player| {
        transform.position[0] = transform.position[0].clamp(-LIMIT_X, LIMIT_X);
        transform.position[1] = transform.position[1].clamp(-LIMIT_Y, LIMIT_Y);
    });
}

/// Builds the world: a player quad plus static markers to move against.
///
/// The palette is deliberate rather than default — a cool near-black ground, muted slate markers, and
/// a single warm amber for the thing you control, so the player reads instantly against everything
/// else.
fn build_app(backend: WgpuBackend) -> App {
    let mut app = App::new();

    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(LiveSource::new())),
    );
    app.insert_resource(Camera2d {
        center: [0.0, 0.0],
        height: 10.0,
    });

    let mut renderer = Renderer::new(Box::new(backend));
    renderer.clear_color = [0.043, 0.047, 0.055, 1.0];
    app.insert_service(renderer);

    app.add_system(Stage::PreSimulation, system(SAMPLE_INPUT, sample_input));
    app.add_system(Stage::Simulation, system("apply_input", apply_input));
    app.add_system(
        Stage::Simulation,
        system("integrate", integrate).after("apply_input"),
    );
    app.add_system(
        Stage::PostSimulation,
        system("clamp_to_view", clamp_to_view),
    );
    app.add_system(Stage::Render, system(RENDER_QUADS, render_quads));

    // Static markers, so movement is visible against something.
    for (x, y) in [
        (-6.0, 3.0),
        (6.0, 3.0),
        (-6.0, -3.0),
        (6.0, -3.0),
        (0.0, 0.0),
    ] {
        let marker = app.world.spawn();
        app.world.insert(marker, Transform2d::at(x, y));
        app.world.insert(
            marker,
            Quad::new(0.6, 0.6, [0.243, 0.286, 0.333, 1.0]).on_layer(0),
        );
    }

    // The player, on a higher layer so it always draws over the markers.
    let player = app.world.spawn();
    app.world.insert(player, Transform2d::at(0.0, 0.0));
    app.world.insert(player, Velocity { x: 0.0, y: 0.0 });
    app.world.insert(player, Player);
    app.world.insert(
        player,
        Quad::new(1.0, 1.0, [0.898, 0.588, 0.243, 1.0]).on_layer(10),
    );

    app
}

// --- Windowing ---

/// Which movement keys are currently held.
///
/// Tracked here rather than in the engine because the key-to-action mapping is platform knowledge.
/// `LiveSource` deals only in action names, so `amadeo-input` needs no winit dependency.
#[derive(Debug, Default)]
struct HeldKeys {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

/// Everything that exists only once a window has been created.
struct Running {
    window: Arc<Window>,
    app: App,
    /// When the previous frame was presented, for the real-time accumulator.
    last_frame: Instant,
}

/// The winit application handler.
#[derive(Default)]
struct QuadDemo {
    running: Option<Running>,
    keys: HeldKeys,
}

impl QuadDemo {
    /// Pushes the current key state into the live input source as action values.
    fn publish_input(&mut self) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let keys = &self.keys;

        running
            .app
            .world
            .with_service_taken::<InputDriver, ()>(|_world, driver| {
                let Some(live) = driver.source.as_any_mut().downcast_mut::<LiveSource>() else {
                    return;
                };
                // Opposed keys cancel, which is what players expect.
                live.set_axis_from_keys(MOVE_X, keys.left, keys.right);
                live.set_axis_from_keys(MOVE_Y, keys.down, keys.up);
            });
    }
}

impl ApplicationHandler for QuadDemo {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // `resumed` can fire more than once on some platforms; only build once.
        if self.running.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Amadeo — quad demo")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));

        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("could not create a window: {error}");
                event_loop.exit();
                return;
            }
        };

        let size = window.inner_size();
        // An `Arc<Window>` is what makes wgpu's surface `'static`, so it can live in a service
        // alongside the rest of the engine's state.
        let backend = match WgpuBackend::new(Arc::clone(&window), size.width, size.height) {
            Ok(backend) => backend,
            Err(error) => {
                eprintln!("could not start the renderer: {error}");
                event_loop.exit();
                return;
            }
        };

        println!("amadeo quad-demo — WASD or arrow keys to move, Escape to quit");

        self.running = Some(Running {
            window,
            app: build_app(backend),
            last_frame: Instant::now(),
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if let Some(running) = self.running.as_mut()
                    && let Some(renderer) = running.app.world.service_mut::<Renderer>()
                {
                    renderer.resize(size.width, size.height);
                }
            }

            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
                        ..
                    },
                ..
            } => {
                let pressed = state == ElementState::Pressed;
                match code {
                    KeyCode::Escape => {
                        event_loop.exit();
                        return;
                    }
                    KeyCode::KeyA | KeyCode::ArrowLeft => self.keys.left = pressed,
                    KeyCode::KeyD | KeyCode::ArrowRight => self.keys.right = pressed,
                    KeyCode::KeyW | KeyCode::ArrowUp => self.keys.up = pressed,
                    KeyCode::KeyS | KeyCode::ArrowDown => self.keys.down = pressed,
                    _ => return,
                }
                self.publish_input();
            }

            WindowEvent::RedrawRequested => {
                let Some(running) = self.running.as_mut() else {
                    return;
                };

                // Real elapsed time goes to the accumulator, which decides how many whole ticks to
                // run. It never reaches simulation code, which sees only FIXED_DT.
                let now = Instant::now();
                let elapsed = now.duration_since(running.last_frame);
                running.last_frame = now;

                if let Err(error) = running
                    .app
                    .advance_real_time(elapsed.as_nanos().min(u128::from(u64::MAX)) as u64)
                {
                    // A schedule error is a setup bug, not a transient condition, and it will repeat
                    // every frame. Fail loudly rather than spamming.
                    eprintln!("schedule error: {error}");
                    event_loop.exit();
                    return;
                }

                if let Err(error) = running.app.render() {
                    eprintln!("render schedule error: {error}");
                    event_loop.exit();
                    return;
                }

                // Ask for the next frame. Without this the loop renders once and stops.
                running.window.request_redraw();
            }

            _ => {}
        }
    }
}

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    // Poll rather than Wait: this is a game, so a frame is due even when no input arrives.
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut demo = QuadDemo::default();
    event_loop.run_app(&mut demo)?;
    Ok(())
}
