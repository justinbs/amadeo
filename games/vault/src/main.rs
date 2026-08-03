//! **The Vault** — M1's exit gate: a complete small 2D game.
//!
//! Collect six sigils without touching a warden or standing on a trap. Walls stop you; wardens do
//! not stop for anything.
//! The floor turns green when you win and red when you lose, because M1 has no text rendering and
//! will not until M3's UI system.
//!
//! ```text
//! cargo run -p vault
//! ```
//!
//! WASD or the arrow keys. Escape quits.
//!
//! # What this game is for
//!
//! It is the milestone's proof, not a product. `docs/05-roadmap.md` sets the bar: a complete small
//! 2D game — "player moves, enemies patrol, collision, a score, a win state" — **built entirely by
//! Claude with zero editor use**, authored through text files and RPC, and verified through
//! `inspect`, headless runs, and `render.describe` rather than by looking at it.
//!
//! So the interesting thing here is not the game. It is that every claim about the game can be
//! checked without eyes:
//!
//! ```text
//! amadeo call render.describe --package vault --ticks 120
//! amadeo query Sigil --package vault --ticks 600
//! amadeo status --package vault
//! ```
//!
//! # Where the level lives
//!
//! `scenes/vault.scene` — the player, both wardens with their patrol routes, the six sigils, the
//! floor, and the score readout. The sigils and traps are instances of the prefabs in
//! `assets/prefabs/`. The wall grid is the one exception and `level.rs` explains why: forty-four
//! tiles is a grid, and a grid wants a tilemap rather than forty-four prefab instances.

use std::sync::Arc;
use std::time::Instant;

use amadeo_app::App;
use amadeo_app::{Stage, system};
use amadeo_input::{InputDriver, LiveSource};
use amadeo_render::{RENDER_QUADS, Renderer, WgpuBackend, render_quads};
use vault::game::{MOVE_X, MOVE_Y, Phase, Run};
use vault::{build_headless, build_simulation};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// The windowed game: the shared simulation, plus live input and a GPU renderer.
fn build_app(backend: WgpuBackend) -> anyhow::Result<App> {
    let mut app = build_simulation()?;

    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(LiveSource::new())),
    );

    let mut renderer = Renderer::new(Box::new(backend));
    // Darker than the floor, so the arena reads as a lit room in a black surround rather than as a
    // rectangle floating on the same colour.
    renderer.clear_color = [0.02, 0.022, 0.035, 1.0];
    app.insert_service(renderer);

    app.add_system(Stage::Render, system(RENDER_QUADS, render_quads));
    Ok(app)
}

// --- Windowing ---

/// Which movement keys are held.
///
/// Here rather than in the engine because key-to-action mapping is platform knowledge; `LiveSource`
/// deals only in action names, which is what keeps `amadeo-input` free of a winit dependency.
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
    /// The phase reported last frame, so a change is announced once rather than every frame.
    announced: Phase,
}

/// The winit application handler.
#[derive(Default)]
struct Vault {
    running: Option<Running>,
    keys: HeldKeys,
}

impl Vault {
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

    /// Prints the outcome once, when it changes.
    ///
    /// To **stderr**, not stdout: stdout is the agent protocol (ADR 0016), and a game that prints
    /// there is reported as sending something that is not JSON.
    fn announce(&mut self) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let phase = running
            .app
            .world
            .resource::<Run>()
            .map_or(Phase::Playing, |run| run.phase);

        if phase == running.announced {
            return;
        }
        running.announced = phase;

        match phase {
            Phase::Won => eprintln!("All sigils collected. Escape to quit."),
            Phase::Lost => eprintln!("The run is over. Escape to quit."),
            Phase::Playing => {}
        }
    }
}

impl ApplicationHandler for Vault {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // `resumed` can fire more than once on some platforms; only build once.
        if self.running.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Amadeo — the Vault")
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
        let backend = match WgpuBackend::new(Arc::clone(&window), size.width, size.height) {
            Ok(backend) => backend,
            Err(error) => {
                eprintln!("could not start the renderer: {error}");
                event_loop.exit();
                return;
            }
        };

        let app = match build_app(backend) {
            Ok(app) => app,
            Err(error) => {
                eprintln!("could not build the game: {error:#}");
                event_loop.exit();
                return;
            }
        };

        eprintln!("Amadeo — the Vault. WASD or arrow keys, Escape to quit.");
        eprintln!("Collect all six sigils. Avoid the wardens and the floor traps.");

        self.running = Some(Running {
            window,
            app,
            last_frame: Instant::now(),
            announced: Phase::Playing,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
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
                let held = state == ElementState::Pressed;
                match code {
                    KeyCode::Escape => event_loop.exit(),
                    KeyCode::KeyA | KeyCode::ArrowLeft => self.keys.left = held,
                    KeyCode::KeyD | KeyCode::ArrowRight => self.keys.right = held,
                    KeyCode::KeyW | KeyCode::ArrowUp => self.keys.up = held,
                    KeyCode::KeyS | KeyCode::ArrowDown => self.keys.down = held,
                    _ => {}
                }
            }

            WindowEvent::RedrawRequested => {
                self.publish_input();

                if let Some(running) = self.running.as_mut() {
                    let now = Instant::now();
                    let elapsed = now.duration_since(running.last_frame);
                    running.last_frame = now;

                    // Real time in, whole ticks out. The accumulator is capped inside `App`, so a
                    // stall falls behind rather than spiralling.
                    if let Err(error) = running.app.advance_real_time(elapsed.as_nanos() as u64) {
                        eprintln!("simulation error: {error}");
                        event_loop.exit();
                        return;
                    }
                    if let Err(error) = running.app.render() {
                        eprintln!("render error: {error}");
                    }
                    running.window.request_redraw();
                }

                self.announce();
            }

            _ => {}
        }
    }
}

fn main() -> anyhow::Result<()> {
    // The agent handover, in one line. When `--amadeo-agent` is present this builds the headless
    // world, answers questions on stdin, and exits without ever opening a window (ADR 0016).
    if amadeo_app::serve_if_requested(&mut build_headless()?)? {
        return Ok(());
    }

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut vault = Vault::default();
    event_loop.run_app(&mut vault)?;
    Ok(())
}
