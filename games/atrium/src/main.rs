//! **The Atrium** — a lit 3D room with shadows and a character you walk around in.
//!
//! ```text
//! cargo run -p atrium
//! ```
//!
//! WASD to walk, Q and E to turn, Space to jump, Escape to quit.
//!
//! See the crate docs in `lib.rs` for what this is *for*. The short version: three parts of M2's
//! exit gate 1 were built and had never been seen together, and no test can answer whether shadows
//! look right — only whether they are darker in the pixel it samples.

use std::sync::Arc;
use std::time::Instant;

use amadeo_app::App;
use amadeo_app::{Stage, system};
use amadeo_audio::{Audio, KiraAudio};
use amadeo_character::{JUMP, MOVE_FORWARD, MOVE_RIGHT, TURN};
use amadeo_input::{InputDriver, LiveSource};
use amadeo_render::{RENDER_QUADS, Renderer, WgpuBackend, render_quads};
use atrium::{build_headless, build_simulation};
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

    // Sound, replacing the `NullAudio` that `build_simulation` installed for the headless path.
    //
    // **A missing device is not a reason to refuse to start.** A machine with no sound card, or one
    // whose device is held exclusively by something else, gets the null backend and a line on
    // stderr — a game with no audio is a game, where a game that will not open is not.
    match KiraAudio::new() {
        Ok(kira) => {
            app.insert_service(Audio::new(Box::new(kira)));
        }
        Err(error) => eprintln!("continuing without sound: {error}"),
    }

    let mut renderer = Renderer::new(Box::new(backend));
    // A dim blue-grey, so anything past the walls reads as sky rather than as a hole. It is only
    // ever visible over the tops of the walls, which is exactly where a flat black would look wrong.
    renderer.clear_color = [0.09, 0.11, 0.16, 1.0];
    app.insert_service(renderer);

    app.add_system(Stage::Render, system(RENDER_QUADS, render_quads));
    Ok(app)
}

// --- Windowing ---

/// Which movement keys are held.
///
/// Here rather than in the engine because key-to-action mapping is platform knowledge; `LiveSource`
/// deals only in action *names*, which is what keeps `amadeo-input` free of a winit dependency —
/// and what lets `modules/amadeo-character` read `move_forward` without knowing a keyboard exists.
#[derive(Debug, Default)]
struct HeldKeys {
    forward: bool,
    back: bool,
    left: bool,
    right: bool,
    turn_left: bool,
    turn_right: bool,
    jump: bool,
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
struct Atrium {
    running: Option<Running>,
    keys: HeldKeys,
}

impl Atrium {
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
                live.set_axis_from_keys(MOVE_FORWARD, keys.back, keys.forward);
                live.set_axis_from_keys(MOVE_RIGHT, keys.left, keys.right);
                live.set_axis_from_keys(TURN, keys.turn_right, keys.turn_left);
                live.set_button(JUMP, keys.jump);
            });
    }
}

impl ApplicationHandler for Atrium {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // `resumed` can fire more than once on some platforms; only build once.
        if self.running.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Amadeo — the Atrium")
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
                eprintln!("could not build the room: {error:#}");
                event_loop.exit();
                return;
            }
        };

        // To **stderr**, not stdout: stdout is the agent protocol (ADR 0016), and a game that prints
        // there is reported as sending something that is not JSON.
        eprintln!("Amadeo — the Atrium.");
        eprintln!("WASD to walk, Q and E to turn, Space to jump, Escape to quit.");

        self.running = Some(Running {
            window,
            app,
            last_frame: Instant::now(),
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
                    KeyCode::KeyW | KeyCode::ArrowUp => self.keys.forward = held,
                    KeyCode::KeyS | KeyCode::ArrowDown => self.keys.back = held,
                    KeyCode::KeyA => self.keys.left = held,
                    KeyCode::KeyD => self.keys.right = held,
                    KeyCode::KeyQ | KeyCode::ArrowLeft => self.keys.turn_left = held,
                    KeyCode::KeyE | KeyCode::ArrowRight => self.keys.turn_right = held,
                    KeyCode::Space => self.keys.jump = held,
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
    let mut atrium = Atrium::default();
    event_loop.run_app(&mut atrium)?;
    Ok(())
}
