//! **The Scarp** — a generated world you can walk on and dig into.
//!
//! ```text
//! cargo run -p scarp
//! ```
//!
//! WASD to walk, Q and E to turn, Space to jump, F to dig, Escape to quit.
//!
//! See the crate docs in `lib.rs` for what this is *for*. The short version: every piece of chunked
//! streaming was built and none of it had ever carried a player.

use std::sync::Arc;
use std::time::Instant;

use amadeo_app::App;
use amadeo_app::{Stage, system};
use amadeo_character::{JUMP, MOVE_FORWARD, MOVE_RIGHT, TURN};
use amadeo_input::{InputDriver, LiveSource};
use amadeo_render::{RENDER_QUADS, Renderer, WgpuBackend, render_quads};
use scarp::{DIG, LOOK, LOOK_X, LOOK_Y, build_headless, build_simulation};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

/// The windowed game: the shared simulation, plus live input and a GPU renderer.
fn build_app(backend: WgpuBackend) -> anyhow::Result<App> {
    let mut app = build_simulation()?;

    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(LiveSource::new())),
    );

    let mut renderer = Renderer::new(Box::new(backend));
    // A pale cold sky. Unlike the Atrium's, this one is most of the upper half of the screen, so it
    // is doing real work rather than filling gaps over a wall.
    renderer.clear_color = [0.46, 0.55, 0.65, 1.0];
    app.insert_service(renderer);

    app.add_system(Stage::Render, system(RENDER_QUADS, render_quads));
    Ok(app)
}

// --- Windowing ---

/// Which keys are held.
///
/// Here rather than in the engine because key-to-action mapping is platform knowledge: `LiveSource`
/// deals only in action *names*, which is what keeps `amadeo-input` free of a winit dependency.
#[derive(Debug, Default)]
struct HeldKeys {
    forward: bool,
    back: bool,
    left: bool,
    right: bool,
    turn_left: bool,
    turn_right: bool,
    jump: bool,
    dig: bool,
    /// The right mouse button, which gates turning the view.
    look: bool,
}

/// Mouse movement waiting to be handed to the simulation.
///
/// # Why it accumulates rather than being read when a tick asks
///
/// A mouse reports *displacement since the last report*, and those reports arrive on the window's
/// schedule rather than the simulation's. The event loop runs uncapped (`ControlFlow::Poll`), so
/// several frames can pass between two ticks — and if each frame simply overwrote an action's value,
/// every report but the last would be thrown away and the view would move a fraction of how far the
/// mouse did.
///
/// So movement is summed here and **cleared only once a tick has actually consumed it**, which
/// `advance_real_time`'s tick count is what makes knowable.
#[derive(Debug, Default)]
struct MouseLook {
    /// Pixels across the screen since the last tick ran.
    dx: f32,
    /// Pixels up and down the screen since the last tick ran.
    dy: f32,
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
struct Scarp {
    running: Option<Running>,
    keys: HeldKeys,
    look: MouseLook,
}

impl Scarp {
    /// Pushes the current key state into the live input source as action values.
    fn publish_input(&mut self) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let keys = &self.keys;
        let look = &self.look;

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
                live.set_button(DIG, keys.dig);

                // Pixels rather than a deflection, which is why these are set directly instead of
                // through `set_axis_from_keys`. A mouse is a displacement; a key is a rate.
                live.set_button(LOOK, keys.look);
                live.set_axis(LOOK_X, look.dx);
                live.set_axis(LOOK_Y, look.dy);
            });
    }

    /// Confines and hides the pointer while the right button is held, and lets it go afterwards.
    ///
    /// Without this the pointer wanders off the window mid-drag and the turn stops dead at the edge
    /// of the screen. The motion itself comes from `DeviceEvent`, which is raw and unaffected either
    /// way — this is purely so the cursor does not get in the way of what it is steering.
    ///
    /// Failures are ignored deliberately. Grabbing is a request the platform may refuse, and a
    /// refusal means the pointer stays visible — mildly annoying, and not a reason to take the game
    /// down.
    fn grab_pointer(&self, held: bool) {
        let Some(running) = self.running.as_ref() else {
            return;
        };
        let window = &running.window;

        if held {
            // `Confined` keeps it inside the window and is what Windows supports; `Locked` pins it in
            // place and is what X11 and Wayland support. Trying both means one line rather than a
            // platform branch.
            let _ = window
                .set_cursor_grab(CursorGrabMode::Confined)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked));
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
        }
        window.set_cursor_visible(!held);
    }
}

impl ApplicationHandler for Scarp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // `resumed` can fire more than once on some platforms; only build once.
        if self.running.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Amadeo — the Scarp")
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
                eprintln!("could not build the world: {error:#}");
                event_loop.exit();
                return;
            }
        };

        // To **stderr**, not stdout: stdout is the agent protocol (ADR 0016), and a game that prints
        // there is reported as sending something that is not JSON.
        eprintln!("Amadeo — the Scarp.");
        eprintln!("WASD to walk, Q and E to turn, Space to jump, F to dig, Escape to quit.");

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
                    KeyCode::KeyF => self.keys.dig = held,
                    _ => {}
                }
            }

            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state,
                ..
            } => {
                let held = state == ElementState::Pressed;
                self.keys.look = held;
                self.grab_pointer(held);
            }

            WindowEvent::RedrawRequested => {
                self.publish_input();

                if let Some(running) = self.running.as_mut() {
                    let now = Instant::now();
                    let elapsed = now.duration_since(running.last_frame);
                    running.last_frame = now;

                    // Real time in, whole ticks out. The accumulator is capped inside `App`, so a
                    // stall falls behind rather than spiralling — which matters more here than in
                    // the Atrium, since a burst of chunk colliders deliberately blocks the tick.
                    let ticks = match running.app.advance_real_time(elapsed.as_nanos() as u64) {
                        Ok(ticks) => ticks,
                        Err(error) => {
                            eprintln!("simulation error: {error}");
                            event_loop.exit();
                            return;
                        }
                    };

                    // **Cleared only once something has read it.** The loop runs uncapped, so most
                    // frames advance no tick at all — and zeroing regardless would throw away every
                    // mouse report that happened to land between two ticks, which is most of them.
                    if ticks > 0 {
                        self.look = MouseLook::default();
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

    /// Raw pointer movement, which is what steering a view wants.
    ///
    /// `DeviceEvent::MouseMotion` rather than `WindowEvent::CursorMoved`, and the difference matters:
    /// `CursorMoved` reports a *position inside the window*, so it stops changing the moment the
    /// pointer reaches an edge and the view stops turning with it. This reports how far the device
    /// moved, which has no edge to hit and is unaffected by pointer acceleration settings.
    ///
    /// Accumulated whether or not the button is held, and read only when it is — a couple of
    /// additions on an event that arrives anyway, against a branch that would have to be kept in step
    /// with the button state.
    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.look.dx += dx as f32;
            self.look.dy += dy as f32;
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
    let mut scarp = Scarp::default();
    event_loop.run_app(&mut scarp)?;
    Ok(())
}
