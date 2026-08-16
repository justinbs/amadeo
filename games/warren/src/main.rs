//! **The Warren** — a first-person atmospheric horror slice, in progress.
//!
//! ```text
//! cargo run -p warren
//! ```
//!
//! WASD to walk, the mouse to look, F to take what is in front of you, Escape to pause.
//!
//! See the crate docs in `lib.rs` for what this is *for*. The short version: the first-person camera
//! rig has existed since session 17 and no game had ever used it, and the level you walk around in
//! is generated (ADR 0071) rather than authored.

use std::sync::Arc;
use std::time::Instant;

use amadeo_app::App;
use amadeo_app::{Stage, system};
use amadeo_audio::{Audio, KiraAudio};
use amadeo_camera::{LOOK, LOOK_X, LOOK_Y};
use amadeo_character::{MOVE_FORWARD, MOVE_RIGHT};
use amadeo_input::{InputDriver, LiveSource};
use amadeo_interaction::USE;
use amadeo_render::{RENDER_QUADS, Renderer, WgpuBackend, render_quads};
use amadeo_ui::{UI_CONFIRM, UI_NEXT, UI_PREVIOUS};
use warren::{PAUSE, Screen, build_headless, build_simulation};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, WindowEvent};
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
    // Near black. There is a ceiling, so this is only ever seen through a gap — but a bright clear
    // colour behind a dark interior reads as a hole in the wall rather than as nothing.
    renderer.clear_color = [0.01, 0.012, 0.015, 1.0];
    app.insert_service(renderer);

    app.add_system(Stage::Render, system(RENDER_QUADS, render_quads));
    Ok(app)
}

/// Which keys are held.
#[derive(Debug, Default)]
struct HeldKeys {
    forward: bool,
    back: bool,
    left: bool,
    right: bool,
    /// F, which takes whatever is in front of you.
    use_it: bool,
    /// Escape, which opens and closes the pause menu.
    pause: bool,
    /// Down and up through a menu, and Enter to choose.
    menu_next: bool,
    menu_previous: bool,
    menu_confirm: bool,
}

/// Pointer movement accumulated since the last frame.
#[derive(Debug, Default)]
struct Look {
    dx: f32,
    dy: f32,
}

/// Everything that exists only once a window has been created.
struct Running {
    window: Arc<Window>,
    app: App,
    last_frame: Instant,
}

#[derive(Default)]
struct Warren {
    running: Option<Running>,
    keys: HeldKeys,
    look: Look,
}

impl Warren {
    /// Pushes the current input state into the live source as action values.
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
                live.set_axis_from_keys(MOVE_FORWARD, keys.back, keys.forward);
                live.set_axis_from_keys(MOVE_RIGHT, keys.left, keys.right);
                live.set_button(USE, keys.use_it);
                live.set_button(PAUSE, keys.pause);
                live.set_button(UI_NEXT, keys.menu_next);
                live.set_button(UI_PREVIOUS, keys.menu_previous);
                live.set_button(UI_CONFIRM, keys.menu_confirm);

                // **`LOOK` is held permanently, and that is the first-person difference.** The
                // camera module gates the look axes on it so that a *third*-person game can keep a
                // free cursor until you grab the view (`games/scarp` holds the right button). Here
                // the view is your head, so there is nothing to grab and this is the one line that
                // says so. Forgetting it is silent: the axes arrive and nothing turns.
                live.set_button(LOOK, true);
                live.set_axis(LOOK_X, look.dx);
                live.set_axis(LOOK_Y, look.dy);
            });
    }

    /// Confines and hides the pointer, permanently — see `publish_input` for why there is no toggle.
    ///
    /// Failures are ignored deliberately. Grabbing is a request the platform may refuse, and a
    /// refusal means the pointer stays visible: annoying, and not a reason to take the game down.
    fn grab_pointer(&self) {
        let Some(running) = self.running.as_ref() else {
            return;
        };
        let window = &running.window;
        // `Confined` is what Windows supports and `Locked` is what X11 and Wayland support, so
        // trying both is one line rather than a platform branch.
        let _ = window
            .set_cursor_grab(CursorGrabMode::Confined)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked));
        window.set_cursor_visible(false);
    }
}

impl ApplicationHandler for Warren {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Amadeo — the Warren")
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

        // To **stderr**, not stdout: stdout is the agent protocol (ADR 0016).
        eprintln!("Amadeo — the Warren.");
        eprintln!("WASD to walk, the mouse to look, F to take what is in front of you.");
        eprintln!("Escape to pause; arrows and Enter to choose.");
        eprintln!("The torch is one room away. Find the key, then find the door.");

        self.running = Some(Running {
            window,
            app,
            last_frame: Instant::now(),
        });
        self.grab_pointer();
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
                    // **No longer an exit.** Escape opens the pause menu, and "quit" is a button on
                    // it — which is what M3's exit gate item 1 means by a complete game rather than
                    // a demo you close with the window chrome.
                    KeyCode::Escape => self.keys.pause = held,
                    KeyCode::KeyW => self.keys.forward = held,
                    KeyCode::KeyS => self.keys.back = held,
                    KeyCode::KeyA => self.keys.left = held,
                    KeyCode::KeyD => self.keys.right = held,
                    KeyCode::KeyF => self.keys.use_it = held,
                    // **The arrows drive the menu and not the character**, deliberately: a menu is
                    // up whenever they do anything, and a key that means two things depending on
                    // the screen is a key that eventually does the wrong one.
                    KeyCode::ArrowDown => self.keys.menu_next = held,
                    KeyCode::ArrowUp => self.keys.menu_previous = held,
                    KeyCode::Enter | KeyCode::Space => self.keys.menu_confirm = held,
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
                    let ticks = match running.app.advance_real_time(elapsed.as_nanos() as u64) {
                        Ok(ticks) => ticks,
                        Err(error) => {
                            eprintln!("simulation error: {error}");
                            event_loop.exit();
                            return;
                        }
                    };

                    // **Cleared only once a tick has actually read it**, which `games/scarp` learned
                    // the hard way. The loop runs uncapped, so most frames advance no tick at all —
                    // and zeroing regardless would throw away every mouse report that landed between
                    // two ticks, which is most of them. The symptom is a view that turns in jerks
                    // and feels like it is dropping input, because it is.
                    if ticks > 0 {
                        self.look = Look::default();
                    }

                    // **Between ticks, which is the whole point of it.** Saving, loading and
                    // starting over all touch a disk or replace a world, and a system doing either
                    // would put the state of a filesystem inside a deterministic simulation. The
                    // menu records what it wants; this is where it happens.
                    for said in warren::serve_requests(&mut running.app) {
                        eprintln!("{said}");
                    }

                    // Terminal, and read after the tick that could have set it so quitting takes
                    // effect on the frame it was chosen rather than the one after.
                    if running.app.world.resource::<Screen>() == Some(&Screen::Quitting) {
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

    /// Raw pointer movement, which is what steering a view wants.
    ///
    /// `DeviceEvent::MouseMotion` rather than `WindowEvent::CursorMoved`: the latter reports a
    /// *position inside the window*, so it stops changing the moment the pointer reaches an edge and
    /// the view stops turning. This reports how far the device moved, which has no edge to hit.
    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.look.dx += dx as f32;
            self.look.dy += dy as f32;
        }
    }
}

fn main() -> anyhow::Result<()> {
    // The agent handover, in one line. With `--amadeo-agent` this builds the headless world, answers
    // questions on stdin, and exits without ever opening a window (ADR 0016).
    if amadeo_app::serve_if_requested(&mut build_headless()?)? {
        return Ok(());
    }

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut Warren::default())?;
    Ok(())
}
