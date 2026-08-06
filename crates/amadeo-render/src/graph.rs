//! The render graph: the plan for one frame.
//!
//! # What a render graph is, and what this one is not
//!
//! A **pass** is one step of drawing a frame — "draw what camera 0 sees", "blur the bright parts",
//! "put the finished picture on the screen". A **resource** is an image a pass reads or writes. A
//! **transient** is a resource that exists only within one frame: scratch paper, not a saved file.
//!
//! A render graph is the *declaration* of those passes and which resources each one touches. From
//! that it derives the order they must run in — if one pass writes an image another reads, the
//! writer goes first — rather than that order being spelled out by hand in a long function where
//! moving two lines silently draws the wrong picture.
//!
//! **This graph does no drawing.** It is a plan, and it knows nothing about wgpu. The backend
//! executes it. That split is why a graph bug is catchable with no GPU at all, which invariant I7
//! asks of every subsystem, and it is why [`NullBackend`](crate::NullBackend) can report the pass
//! structure of a frame it never drew.
//!
//! # It is deliberately not a public extension surface — ADR 0034
//!
//! Nothing outside this crate can name a pass, order one, or add one. That is a decision rather than
//! an omission: `RenderBackend` isolating rendering completely is what made ADR 0018, ADR 0023 and
//! ADR 0031 cheap to decide, and ADR 0031 could prove in a three-row table that an entire render
//! restructuring contributed nothing to simulation state. A public graph gives that up permanently,
//! and Bevy — the one engine that made its graph public — has rewritten that public API repeatedly.
//!
//! Games configure a *look* through reflected data instead (ADR 0034's `Environment`), which the
//! schema, the scene format, `amadeo check` and the agent protocol all already handle.
//!
//! # What it does not do yet
//!
//! Frostbite's original frame graph existed largely to insert GPU synchronisation automatically and
//! to overlap the memory of transients whose lifetimes do not collide. **wgpu already does the
//! first**, which is one of its main reasons to exist over raw Vulkan or DX12, and its safe API has
//! no way to overlap two textures' memory — so reuse here means handing the same whole texture to a
//! later transient with an identical description, which is what [`Plan::lifetimes`] exists to make
//! safe.

use std::collections::{BTreeMap, BTreeSet};

/// The frame's final destination: a window's surface, or the texture an offscreen backend owns.
///
/// Reserved rather than declared — no pass may create it, and exactly one should write it.
pub(crate) const DESTINATION: &str = "destination";

/// The image every camera draws into, before anything reaches the screen.
///
/// High dynamic range — see [`TargetFormat::Hdr16`].
pub(crate) const SCENE: &str = "scene";

/// The finished, displayable image, after the post pass and before it reaches the destination.
pub(crate) const OUTPUT: &str = "output";

/// How far away each pixel is, so nearer geometry hides further geometry.
///
/// Declared only when a frame actually holds a 3D camera — see [`frame_graph`].
pub(crate) const DEPTH: &str = "depth";

/// The pixel format of a transient image.
///
/// One variant, deliberately, exactly as [`PixelFormat`](amadeo_image::PixelFormat) shipped with one
/// under ADR 0026: **the tag is the load-bearing part.** Adding a high-dynamic-range variant when
/// tonemapping lands is then a new variant plus a new producer, rather than a change to every pass,
/// every allocation site, and every test that asserts on a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TargetFormat {
    /// Eight bits per channel, red-green-blue-alpha, sRGB encoded.
    ///
    /// # A transient does *not* inherit the destination's format, and that is the point
    ///
    /// A window's surface is commonly **B**GRA while an offscreen target is RGBA — the surface
    /// format is whatever the adapter offers, not something the engine picks. If a transient copied
    /// that, the finished picture would sit in memory with its red and blue channels swapped on one
    /// path and not the other, and every capture would have to know which. The captured image would
    /// stop being evidence about the renderer that ships, which is the entire reason capture exists.
    ///
    /// So the graph fixes the format it works in, and the present pass is the one place the
    /// destination's own format is met. The hardware does that conversion while writing.
    Srgb8,
    /// Sixteen-bit floating point per channel, linear — **high dynamic range**.
    ///
    /// What the cameras draw into, so that a pixel can be *brighter than white*. Eight-bit sRGB
    /// cannot represent that: everything is clamped into 0..1, which leaves bloom with no bright
    /// parts to isolate and tonemapping with nothing to compress. Both effects exist precisely to
    /// handle values above the display range, so without this they would be elaborate ways of doing
    /// nothing.
    ///
    /// This is the variant [`TargetFormat`] was written expecting, and adding it cost a match arm —
    /// which is what ADR 0026's format-tag argument predicted when the same shape was used for
    /// decoded images.
    Hdr16,
    /// A depth buffer: how far away each pixel is, so nearer geometry hides further geometry.
    ///
    /// **Not a colour image**, and the difference is load-bearing in one place — nothing samples it
    /// yet, so a pooled depth texture carries no bind group. See `PooledTexture` in the wgpu
    /// backend, where building one against the colour layout would fail at *creation* rather than at
    /// draw, and therefore look like an allocation bug rather than a layout one.
    ///
    /// Shadow maps have their own variant below; fog is what will eventually read this one.
    Depth32,
    /// A depth buffer that a later pass **samples** — a shadow map (ADR 0038).
    ///
    /// # Why this is a separate variant rather than a flag on [`TargetFormat::Depth32`]
    ///
    /// The two are the same wgpu format and differ in what they are *for*, which turns out to be the
    /// thing that matters:
    ///
    /// - A shadow map needs `TEXTURE_BINDING`, and asking for usages nothing needs is not free —
    ///   some backends pick a less efficient memory layout to satisfy them. The scene depth buffer is
    ///   only ever attached, so it should keep asking for nothing extra.
    /// - `assign_transients` reuses one physical texture for two transients whose descriptions match.
    ///   Matching on the format means a shadow map and a scene depth buffer can never be handed the
    ///   same texture even at identical sizes, which they otherwise could — and one of them would
    ///   then be missing the usage it needs.
    ///
    /// The same argument the enum's own doc makes: the tag is the load-bearing part.
    ShadowMap32,
}

/// An image that exists only for the duration of one frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Transient {
    /// What passes call it.
    pub name: String,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// What kind of image it is.
    pub format: TargetFormat,
}

/// What work a pass performs.
///
/// The graph does none of it — this is what the backend matches on when executing the plan, and it
/// is the whole of the vocabulary the graph has about *drawing*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PassKind {
    /// Draw what one camera sees, selected by its index into [`FrameData::views`](crate::FrameData).
    View {
        /// Which view.
        index: usize,
        /// Whether this pass clears its target first.
        ///
        /// Only the first view clears. Later ones load what is already there, which is what makes a
        /// HUD camera compose over a world camera rather than erase it (ADR 0031).
        clears: bool,
    },
    /// Fill the target with the clear colour and draw nothing.
    ///
    /// What a world with **no camera** produces. Without it the previous frame's image would
    /// persist, so "no camera" would look like "frozen" rather than "empty" — and a world under
    /// construction genuinely has no camera yet (ADR 0031).
    Clear,
    /// Apply the camera's [`Environment`](crate::Environment): exposure, tonemap, grade, vignette.
    ///
    /// Reads the high-dynamic-range scene image and writes a displayable one. This is the step that
    /// turns "brighter than white" into pixels, so everything after it is in display range and
    /// everything before it is not.
    Post,
    /// Draw the scene from a light's point of view, keeping only depth — a shadow map (ADR 0038).
    ///
    /// The only pass in this engine with **no colour attachment at all**: nothing is being painted,
    /// only measured. What it writes is how far the light can see before something blocks it, which
    /// the view pass then compares each pixel against.
    Shadow {
        /// Which view's light this belongs to, indexing [`FrameData::views`](crate::FrameData).
        view: usize,
    },
    /// Put a finished image onto the destination.
    ///
    /// A full-screen draw rather than a texture copy, because a surface texture is not guaranteed to
    /// accept a copy — and because this is where tonemapping goes when it arrives, so the pass has
    /// to be a shader either way.
    Present,
}

/// One declared step of a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pass {
    /// A unique name, used in diagnostics and in what a backend reports back.
    pub label: String,
    /// Resources this pass reads.
    pub reads: Vec<String>,
    /// Resources this pass writes.
    pub writes: Vec<String>,
    /// The depth buffer this pass tests and writes against, if it has one.
    ///
    /// # Why this is its own field rather than another entry in `writes`
    ///
    /// A depth attachment is written, but it is also *state the pass tests against* — it is not an
    /// image any later pass reads, and it is bound in a different place in a render pass descriptor.
    /// Folding it into `writes` would make the ordering rules apply to it (harmless) while giving
    /// every consumer of `writes` a resource it has to specially exclude (not harmless).
    ///
    /// Only the 3D view passes have one. The 2D passes and the full-screen passes leave it `None`,
    /// which is what keeps the sprite path provably untouched by any of this: a pipeline declaring
    /// no depth state cannot be used in a pass that has a depth attachment, so "sometimes attached"
    /// has to be a real distinction rather than a convenience.
    pub depth: Option<String>,
    /// What it does.
    pub kind: PassKind,
}

/// What can be wrong with a declared graph.
///
/// Every one of these is a programming error inside this crate rather than anything content can
/// cause — the graph is not a public surface (ADR 0034). They are typed and actionable anyway,
/// because the alternative is a wrong picture with nothing to read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum GraphError {
    /// Two passes share a label.
    #[error(
        "two passes in the render graph are both labelled `{label}`; labels name a pass in every \
         diagnostic, so they have to be unique"
    )]
    DuplicateLabel {
        /// The repeated label.
        label: String,
    },

    /// A pass named a resource that was never declared.
    #[error(
        "render pass `{pass}` names the resource `{resource}`, which is neither a declared \
         transient nor the frame destination; declare it with `RenderGraph::transient` first"
    )]
    UnknownResource {
        /// The pass that named it.
        pass: String,
        /// The name it used.
        resource: String,
    },

    /// A pass reads something nothing produces.
    #[error(
        "render pass `{pass}` reads `{resource}`, which no pass writes; it would sample an \
         uninitialised image"
    )]
    NeverWritten {
        /// The reading pass.
        pass: String,
        /// What it wanted.
        resource: String,
    },

    /// The dependencies form a loop.
    ///
    /// Reported with the whole chain rather than one pass, for the same reason ADR 0029 reports a
    /// prefab cycle that way: one name in a loop tells you nothing about how to break it.
    #[error("the render graph has a cycle: {chain}")]
    Cycle {
        /// The passes in the loop, joined by ` -> `.
        chain: String,
    },
}

/// When a transient is first written and last touched, as positions in the execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Lifetime {
    /// Position of the first pass that writes it.
    pub first: usize,
    /// Position of the last pass that reads or writes it.
    pub last: usize,
}

impl Lifetime {
    /// Whether two transients are both live at any point, and so cannot share one texture.
    #[cfg_attr(
        not(feature = "gpu"),
        allow(
            dead_code,
            reason = "only the wgpu backend allocates textures; the null backend compiles the \
                      graph to check it and draws nothing"
        )
    )]
    pub(crate) fn overlaps(&self, other: &Lifetime) -> bool {
        self.first <= other.last && other.first <= self.last
    }
}

/// A validated graph, with an execution order and transient lifetimes worked out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    /// Pass indices, in the order they must run.
    order: Vec<usize>,
    /// Per transient name, when it is live.
    lifetimes: BTreeMap<String, Lifetime>,
    /// What the pass writing the destination reads, if there is exactly one such read.
    ///
    /// This is what `capture` reads back. A window's surface cannot be read, so "the finished image"
    /// has to be the transient that was about to be copied onto it — and deriving that from the
    /// graph rather than hardcoding a name means post-processing can be inserted before the present
    /// pass without capture quietly returning the pre-effect picture.
    destination_source: Option<String>,
}

/// Only the wgpu backend allocates or reads back textures, so a build without it uses the plan for
/// its pass order alone. Kept out of the `gpu` feature deliberately: the *checking* this module does
/// is what a headless run needs, and gating it would put a whole class of bug beyond CI's reach.
#[cfg_attr(not(feature = "gpu"), allow(dead_code))]
impl Plan {
    /// Pass indices in execution order.
    pub(crate) fn order(&self) -> &[usize] {
        &self.order
    }

    /// When each transient is live, by name.
    pub(crate) fn lifetimes(&self) -> &BTreeMap<String, Lifetime> {
        &self.lifetimes
    }

    /// The transient holding the finished image, which is what a capture reads back.
    pub(crate) fn destination_source(&self) -> Option<&str> {
        self.destination_source.as_deref()
    }
}

/// The declared passes and transients of one frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RenderGraph {
    passes: Vec<Pass>,
    transients: Vec<Transient>,
}

impl RenderGraph {
    /// An empty graph.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Declares an image that lives for this frame only.
    pub(crate) fn transient(&mut self, name: &str, width: u32, height: u32, format: TargetFormat) {
        self.transients.push(Transient {
            name: name.to_string(),
            width,
            height,
            format,
        });
    }

    /// Declares a pass and what it touches.
    pub(crate) fn pass(&mut self, label: &str, kind: PassKind, reads: &[&str], writes: &[&str]) {
        self.passes.push(Pass {
            label: label.to_string(),
            reads: reads.iter().map(|name| (*name).to_string()).collect(),
            writes: writes.iter().map(|name| (*name).to_string()).collect(),
            depth: None,
            kind,
        });
    }

    /// Gives the pass declared most recently a depth attachment.
    ///
    /// Separate from [`RenderGraph::pass`] rather than a sixth argument, because only the 3D view
    /// passes have one and every other call site would read `None`.
    pub(crate) fn with_depth(&mut self, name: &str) {
        if let Some(pass) = self.passes.last_mut() {
            pass.depth = Some(name.to_string());
        }
    }

    /// The declared passes, in declaration order.
    pub(crate) fn passes(&self) -> &[Pass] {
        &self.passes
    }

    /// The declared transients.
    ///
    /// Only the wgpu backend has anything to allocate for them — see the note on `impl Plan`.
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    pub(crate) fn transients(&self) -> &[Transient] {
        &self.transients
    }

    /// Checks the graph and works out the order its passes must run in.
    ///
    /// # Ordering rules
    ///
    /// 1. A pass that **writes** a resource runs before every pass that **reads** it.
    /// 2. Two passes writing the *same* resource run in **declaration order**. That is what makes
    ///    the per-camera passes compose: view 1 loads what view 0 left, so their order is meaningful
    ///    rather than incidental.
    /// 3. Passes with no relationship keep their declaration order.
    ///
    /// Rule 3 is where this differs from `amadeo-app`'s `Schedule`, which breaks ties
    /// **alphabetically** so that registration order cannot influence a result. That rule is right
    /// there and wrong here: a schedule's registration order is accidental, while a graph's
    /// declaration order is the order the frame is composed in, and rule 2 already depends on it.
    /// Both are deterministic, which is what actually matters — two runs of the same frame produce
    /// the same picture.
    ///
    /// # Errors
    ///
    /// [`GraphError`] for a duplicate label, an undeclared resource, a read nothing writes, or a
    /// cycle.
    pub(crate) fn compile(&self) -> Result<Plan, GraphError> {
        let mut labels = BTreeSet::new();
        for pass in &self.passes {
            if !labels.insert(pass.label.as_str()) {
                return Err(GraphError::DuplicateLabel {
                    label: pass.label.clone(),
                });
            }
        }

        let declared: BTreeSet<&str> = self
            .transients
            .iter()
            .map(|transient| transient.name.as_str())
            .chain(std::iter::once(DESTINATION))
            .collect();

        for pass in &self.passes {
            // The depth attachment is checked alongside reads and writes, because "declared" is the
            // same question for it — it is only the *ordering* rules it stays out of.
            for resource in pass
                .reads
                .iter()
                .chain(pass.writes.iter())
                .chain(pass.depth.iter())
            {
                if !declared.contains(resource.as_str()) {
                    return Err(GraphError::UnknownResource {
                        pass: pass.label.clone(),
                        resource: resource.clone(),
                    });
                }
            }
        }

        // Who writes what, in declaration order. Used for both edge kinds below.
        let mut writers: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (index, pass) in self.passes.iter().enumerate() {
            for resource in &pass.writes {
                writers.entry(resource.as_str()).or_default().push(index);
            }
        }

        for pass in &self.passes {
            for resource in &pass.reads {
                if !writers.contains_key(resource.as_str()) {
                    return Err(GraphError::NeverWritten {
                        pass: pass.label.clone(),
                        resource: resource.clone(),
                    });
                }
            }
        }

        // `edges[a]` holds the passes that must run after `a`. A set rather than a list, so the same
        // dependency declared two ways is counted once — otherwise the in-degree below never reaches
        // zero and a perfectly good graph reports a cycle.
        let mut edges: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); self.passes.len()];

        // Rule 1: every writer of something this pass reads comes first.
        for (index, pass) in self.passes.iter().enumerate() {
            for resource in &pass.reads {
                for &writer in &writers[resource.as_str()] {
                    if writer != index {
                        edges[writer].insert(index);
                    }
                }
            }
        }

        // Rule 2: consecutive writers of one resource keep their declaration order.
        for indices in writers.values() {
            for pair in indices.windows(2) {
                edges[pair[0]].insert(pair[1]);
            }
        }

        let mut in_degree = vec![0usize; self.passes.len()];
        for successors in &edges {
            for &successor in successors {
                in_degree[successor] += 1;
            }
        }

        // Kahn's algorithm, always taking the *lowest-numbered* pass that is ready. Scanning for it
        // rather than keeping a heap is what gives rule 3 — among passes nothing orders, the one
        // declared first goes first.
        let mut order = Vec::with_capacity(self.passes.len());
        let mut done = vec![false; self.passes.len()];
        for _ in 0..self.passes.len() {
            let Some(next) =
                (0..self.passes.len()).find(|&index| !done[index] && in_degree[index] == 0)
            else {
                break;
            };
            done[next] = true;
            order.push(next);
            for &successor in &edges[next] {
                in_degree[successor] -= 1;
            }
        }

        if order.len() != self.passes.len() {
            return Err(GraphError::Cycle {
                chain: self.describe_cycle(&edges, &done),
            });
        }

        // A transient's lifetime runs from the first pass that writes it to the last that touches
        // it, measured in positions along the execution order rather than declaration order —
        // because reuse is about what is live *while the frame runs*.
        let mut lifetimes: BTreeMap<String, Lifetime> = BTreeMap::new();
        for (position, &index) in order.iter().enumerate() {
            let pass = &self.passes[index];
            // A depth attachment is written by the pass that uses it, so it is live exactly as a
            // colour target is — and it needs a lifetime for the same reason: without one the
            // backend has nothing to allocate against and the pass would run with no depth buffer.
            for resource in pass.writes.iter().chain(pass.depth.iter()) {
                if resource == DESTINATION {
                    continue;
                }
                lifetimes
                    .entry(resource.clone())
                    .and_modify(|life| life.last = life.last.max(position))
                    .or_insert(Lifetime {
                        first: position,
                        last: position,
                    });
            }
            for resource in &pass.reads {
                if let Some(life) = lifetimes.get_mut(resource) {
                    life.last = life.last.max(position);
                }
            }
        }

        // What the present pass is about to put on screen — see `Plan::destination_source`. Exactly
        // one read, because "the finished image" is not a meaningful phrase otherwise.
        let destination_source = self
            .passes
            .iter()
            .find(|pass| pass.writes.iter().any(|name| name == DESTINATION))
            .filter(|pass| pass.reads.len() == 1)
            .map(|pass| pass.reads[0].clone());

        Ok(Plan {
            order,
            lifetimes,
            destination_source,
        })
    }

    /// Walks the unfinished part of the graph until a pass repeats, and names the loop.
    fn describe_cycle(&self, edges: &[BTreeSet<usize>], done: &[bool]) -> String {
        let Some(start) = (0..self.passes.len()).find(|&index| !done[index]) else {
            return "<empty>".to_string();
        };

        let mut walk = vec![start];
        let mut seen = BTreeSet::new();
        seen.insert(start);
        let mut current = start;

        // Every step follows an edge into the unfinished part, and that part is finite, so this
        // terminates: either a pass repeats or the walk runs out of successors.
        while let Some(&next) = edges[current].iter().find(|&&candidate| !done[candidate]) {
            walk.push(next);
            // The second sighting closes the loop, and is where the chain stops growing.
            if !seen.insert(next) {
                break;
            }
            current = next;
        }

        walk.into_iter()
            .map(|index| self.passes[index].label.as_str())
            .collect::<Vec<_>>()
            .join(" -> ")
    }
}

/// The graph one frame needs, derived from what the world produced.
///
/// # Why the cameras draw into a transient rather than straight at the destination
///
/// Two reasons, and the second one is the concrete payoff of this whole module.
///
/// It is **where post-processing goes**. A pass that blurs the bright parts of the picture has to
/// read the picture, and a pass cannot read the image it is writing. Composing effects means an
/// off-screen image to hand between them, so it has to exist before the first effect does.
///
/// And it is **what gives a windowed run `capture`**. A window's surface image cannot be read back —
/// it is not created with `COPY_SRC` and wgpu does not let you ask for one — which is why capture
/// used to need [`WgpuBackend::offscreen`](crate::WgpuBackend::offscreen) and a windowed backend
/// could only refuse. A transient *can* be read back, so the finished picture is now readable with
/// or without a window; the two paths differ by the final copy alone, and `RenderBackend::capture`
/// says which side of it each one reads.
/// # Why there is a separate `output` image rather than posting straight onto the destination
///
/// Folding the post pass into the present pass would save one full-screen copy per frame. It would
/// also take **windowed capture away again**: a window's image cannot be read back, so the finished
/// picture has to exist somewhere readable, and after the post pass is the only place it is both
/// finished and displayable. `scene` is not an answer — it is high dynamic range, so reading it back
/// would hand out pixels nothing can display without redoing the tonemap in Rust, which is two
/// copies of a curve that would drift.
///
/// One full-screen copy is a real cost and a small one. If it ever shows up in a profile, the fix is
/// to fold the two passes together and give windowed capture its own on-demand pass instead — the
/// graph already expresses that, and it is a local change.
pub(crate) fn frame_graph(frame: &crate::FrameData, width: u32, height: u32) -> RenderGraph {
    let mut graph = RenderGraph::new();
    graph.transient(SCENE, width, height, TargetFormat::Hdr16);
    graph.transient(OUTPUT, width, height, TargetFormat::Srgb8);

    // A depth buffer exists only if something needs one. Declaring it unconditionally would cost a
    // full-screen texture for every 2D game in the engine, and every one of the target games that
    // is 2D would pay it for nothing.
    let any_3d = frame.views.iter().any(|view| {
        matches!(
            view.camera.projection,
            crate::Projection::Perspective { .. }
        )
    });
    if any_3d {
        graph.transient(DEPTH, width, height, TargetFormat::Depth32);
    }

    if frame.views.is_empty() {
        graph.pass("clear", PassKind::Clear, &[], &[SCENE]);
    } else {
        for (index, view) in frame.views.iter().enumerate() {
            // A shadow map is declared only when a light in this view actually casts one, on the
            // same reasoning as the depth buffer above: a game with no shadows should allocate no
            // shadow map and run no extra pass over its geometry.
            let shadow = view.lights.iter().find_map(|light| light.shadow);
            let shadow_name = format!("shadow {index}");
            if let Some(shadow) = shadow {
                graph.transient(
                    &shadow_name,
                    shadow.resolution,
                    shadow.resolution,
                    TargetFormat::ShadowMap32,
                );
                // Written by the shadow pass and read by the view pass, which is what puts them in
                // that order -- the dependency is declared rather than the order being asserted.
                graph.pass(
                    &format!("shadow {index}"),
                    PassKind::Shadow { view: index },
                    &[],
                    &[shadow_name.as_str()],
                );
            }

            // Reading the shadow map is what orders this pass after the one that draws it.
            let reads: Vec<&str> = match shadow {
                Some(_) => vec![shadow_name.as_str()],
                None => Vec::new(),
            };
            graph.pass(
                &format!("view {index}"),
                PassKind::View {
                    index,
                    // Only the first camera clears; later ones load what is already there, which is
                    // what makes a HUD camera compose over a world camera rather than erase it.
                    clears: index == 0,
                },
                &reads,
                &[SCENE],
            );
            // Only a 3D view gets depth. A pipeline declaring no depth state cannot be used in a
            // pass that has a depth attachment, so this is what keeps the 2D passes untouched by
            // any of it rather than merely unaffected in practice.
            if matches!(
                view.camera.projection,
                crate::Projection::Perspective { .. }
            ) {
                graph.with_depth(DEPTH);
            }
        }
    }

    // Always present, even when the environment does nothing, so that a frame has **one shape**
    // rather than two. A conditional pass would mean the common path and the tested path could
    // differ, which is the sort of saving that costs more than it returns.
    graph.pass("post", PassKind::Post, &[SCENE], &[OUTPUT]);
    graph.pass("present", PassKind::Present, &[OUTPUT], &[DESTINATION]);
    graph
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Camera, FrameData, View};

    /// A graph shaped like a real frame: two cameras into one image, then present.
    fn two_views() -> RenderGraph {
        let mut graph = RenderGraph::new();
        graph.transient("scene", 64, 64, TargetFormat::Srgb8);
        graph.pass(
            "view 0",
            PassKind::View {
                index: 0,
                clears: true,
            },
            &[],
            &["scene"],
        );
        graph.pass(
            "view 1",
            PassKind::View {
                index: 1,
                clears: false,
            },
            &[],
            &["scene"],
        );
        graph.pass("present", PassKind::Present, &["scene"], &[DESTINATION]);
        graph
    }

    fn labels(graph: &RenderGraph, plan: &Plan) -> Vec<String> {
        plan.order()
            .iter()
            .map(|&index| graph.passes()[index].label.clone())
            .collect()
    }

    #[test]
    fn a_reader_runs_after_its_writer() {
        // Declared backwards on purpose: the order has to come from the dependency, not the order
        // the passes were added in.
        let mut graph = RenderGraph::new();
        graph.transient("scene", 8, 8, TargetFormat::Srgb8);
        graph.pass("present", PassKind::Present, &["scene"], &[DESTINATION]);
        graph.pass(
            "view 0",
            PassKind::View {
                index: 0,
                clears: true,
            },
            &[],
            &["scene"],
        );

        let plan = graph.compile().expect("a valid graph");
        assert_eq!(labels(&graph, &plan), ["view 0", "present"]);
    }

    #[test]
    fn two_writers_of_one_image_keep_their_declaration_order() {
        // The rule that makes a HUD camera compose over a world camera rather than race it.
        let graph = two_views();
        let plan = graph.compile().expect("a valid graph");
        assert_eq!(labels(&graph, &plan), ["view 0", "view 1", "present"]);
    }

    #[test]
    fn compiling_the_same_graph_twice_gives_the_same_order() {
        let graph = two_views();
        let first = graph.compile().expect("valid");
        let second = graph.compile().expect("valid");
        assert_eq!(first.order(), second.order());
    }

    #[test]
    fn an_undeclared_resource_is_named_in_the_error() {
        let mut graph = RenderGraph::new();
        graph.pass("present", PassKind::Present, &["blooom"], &[DESTINATION]);

        let error = graph.compile().expect_err("`blooom` was never declared");
        let message = error.to_string();
        assert!(message.contains("present"), "{message}");
        assert!(message.contains("blooom"), "{message}");
        // Says what to do about it, per the project's error-message standard.
        assert!(message.contains("RenderGraph::transient"), "{message}");
    }

    #[test]
    fn reading_an_image_nothing_writes_is_refused() {
        // Declared, so it exists — but nothing fills it, so sampling it reads whatever was in that
        // memory. Silent garbage is the worst available outcome, so this is an error.
        let mut graph = RenderGraph::new();
        graph.transient("scene", 8, 8, TargetFormat::Srgb8);
        graph.pass("present", PassKind::Present, &["scene"], &[DESTINATION]);

        let error = graph.compile().expect_err("nothing writes `scene`");
        assert!(matches!(error, GraphError::NeverWritten { .. }), "{error}");
        assert!(error.to_string().contains("uninitialised"), "{error}");
    }

    #[test]
    fn a_cycle_is_reported_with_its_whole_chain() {
        // One name in a loop tells you nothing about how to break it — ADR 0029 reports prefab
        // cycles the same way, for the same reason.
        let mut graph = RenderGraph::new();
        graph.transient("a", 8, 8, TargetFormat::Srgb8);
        graph.transient("b", 8, 8, TargetFormat::Srgb8);
        graph.pass("first", PassKind::Present, &["b"], &["a"]);
        graph.pass("second", PassKind::Present, &["a"], &["b"]);

        let error = graph
            .compile()
            .expect_err("first and second need each other");
        let message = error.to_string();
        assert!(message.contains("first"), "{message}");
        assert!(message.contains("second"), "{message}");
        assert!(
            message.contains("->"),
            "the chain should be shown: {message}"
        );
    }

    #[test]
    fn duplicate_labels_are_refused() {
        let mut graph = RenderGraph::new();
        graph.transient("scene", 8, 8, TargetFormat::Srgb8);
        graph.pass(
            "view",
            PassKind::View {
                index: 0,
                clears: true,
            },
            &[],
            &["scene"],
        );
        graph.pass(
            "view",
            PassKind::View {
                index: 1,
                clears: false,
            },
            &[],
            &["scene"],
        );

        let error = graph.compile().expect_err("two passes named `view`");
        assert!(
            matches!(error, GraphError::DuplicateLabel { .. }),
            "{error}"
        );
    }

    #[test]
    fn a_transients_lifetime_spans_its_writer_and_its_last_reader() {
        let graph = two_views();
        let plan = graph.compile().expect("valid");
        let life = plan.lifetimes()["scene"];
        // Written at position 0, still read by `present` at position 2.
        assert_eq!(life.first, 0);
        assert_eq!(life.last, 2);
        // The destination is not a transient, so it has no lifetime to track.
        assert!(!plan.lifetimes().contains_key(DESTINATION));
    }

    #[test]
    fn transients_that_do_not_overlap_could_share_one_texture() {
        // The property a future reuse step depends on. Asserted now because getting it wrong later
        // means two passes writing the same memory and a picture nobody can explain.
        let mut graph = RenderGraph::new();
        graph.transient("first", 8, 8, TargetFormat::Srgb8);
        graph.transient("second", 8, 8, TargetFormat::Srgb8);
        graph.pass(
            "draw",
            PassKind::View {
                index: 0,
                clears: true,
            },
            &[],
            &["first"],
        );
        graph.pass("copy", PassKind::Present, &["first"], &["second"]);
        graph.pass("present", PassKind::Present, &["second"], &[DESTINATION]);

        let plan = graph.compile().expect("valid");
        let first = plan.lifetimes()["first"];
        let second = plan.lifetimes()["second"];
        // `first` is live over passes 0..=1 and `second` over 1..=2, so they *do* overlap and must
        // stay separate. The interesting assertion is that the graph knows it.
        assert!(first.overlaps(&second));

        let disjoint = Lifetime { first: 3, last: 4 };
        assert!(!first.overlaps(&disjoint));
    }

    #[test]
    fn the_plan_knows_what_capture_should_read() {
        let graph = two_views();
        let plan = graph.compile().expect("valid");
        // Not hardcoded anywhere: it is whatever the present pass was about to put on screen, so
        // inserting post-processing before it moves this automatically.
        assert_eq!(plan.destination_source(), Some("scene"));
    }

    fn frame_with_views(count: usize) -> FrameData {
        FrameData {
            views: (0..count)
                .map(|_| View {
                    camera: Camera::default(),
                    environment: crate::Environment::default(),
                    eye: [0.0, 0.0],
                    eye_matrix: amadeo_transform::Mat4::IDENTITY,
                    quads: Vec::new(),
                    batches: Vec::new(),
                    meshes: Vec::new(),
                    lights: Vec::new(),
                })
                .collect(),
            ..FrameData::default()
        }
    }

    #[test]
    fn every_camera_gets_a_pass_and_only_the_first_clears() {
        let frame = frame_with_views(3);
        let graph = frame_graph(&frame, 320, 200);
        let plan = graph.compile().expect("a frame graph is always valid");

        assert_eq!(
            labels(&graph, &plan),
            ["view 0", "view 1", "view 2", "post", "present"]
        );

        let clears: Vec<bool> = graph
            .passes()
            .iter()
            .filter_map(|pass| match pass.kind {
                PassKind::View { clears, .. } => Some(clears),
                _ => None,
            })
            .collect();
        assert_eq!(clears, [true, false, false]);
    }

    #[test]
    fn a_world_with_no_camera_still_gets_a_clearing_pass() {
        // Otherwise the previous frame would persist and "no camera" would look like "frozen".
        let frame = frame_with_views(0);
        let graph = frame_graph(&frame, 320, 200);
        let plan = graph.compile().expect("a frame graph is always valid");

        assert_eq!(labels(&graph, &plan), ["clear", "post", "present"]);
    }

    #[test]
    fn the_transients_match_the_destination_size() {
        // A viewport rectangle is a fraction of the target, so a transient that did not match the
        // destination would put every camera in the wrong place by a scale factor.
        let frame = frame_with_views(1);
        let graph = frame_graph(&frame, 320, 200);
        for transient in graph.transients() {
            assert_eq!((transient.width, transient.height), (320, 200));
        }
    }

    #[test]
    fn the_cameras_draw_in_high_dynamic_range_and_the_post_pass_brings_it_down() {
        // The arrangement every effect depends on: bloom needs values above the display range to
        // isolate and tonemapping exists to compress them, so an 8-bit scene target would make both
        // elaborate ways of doing nothing.
        let frame = frame_with_views(1);
        let graph = frame_graph(&frame, 64, 64);

        let scene = graph
            .transients()
            .iter()
            .find(|transient| transient.name == SCENE)
            .expect("declared");
        let output = graph
            .transients()
            .iter()
            .find(|transient| transient.name == OUTPUT)
            .expect("declared");

        assert_eq!(scene.format, TargetFormat::Hdr16);
        assert_eq!(output.format, TargetFormat::Srgb8);
    }

    /// A frame holding one perspective camera.
    fn frame_in_3d() -> FrameData {
        let mut frame = frame_with_views(1);
        frame.views[0].camera = Camera::perspective(60.0);
        frame
    }

    #[test]
    fn a_2d_frame_declares_no_depth_buffer() {
        // A full-screen depth texture for every 2D game would be a real cost paid for nothing, and
        // three of the eight target games are 2D.
        let frame = frame_with_views(1);
        let graph = frame_graph(&frame, 64, 64);

        assert!(graph.transients().iter().all(|t| t.name != DEPTH));
        assert!(
            graph.passes().iter().all(|pass| pass.depth.is_none()),
            "a 2D pass must not have a depth attachment: a pipeline declaring no depth state \
             cannot be used in a pass that has one"
        );
    }

    #[test]
    fn a_3d_frame_declares_one_and_attaches_it_to_the_view_pass() {
        let frame = frame_in_3d();
        let graph = frame_graph(&frame, 64, 64);

        let depth = graph
            .transients()
            .iter()
            .find(|t| t.name == DEPTH)
            .expect("a 3D frame needs somewhere to put depth");
        assert_eq!(depth.format, TargetFormat::Depth32);
        assert_eq!((depth.width, depth.height), (64, 64));

        // The view pass has it; the full-screen passes do not.
        let with_depth: Vec<&str> = graph
            .passes()
            .iter()
            .filter(|pass| pass.depth.is_some())
            .map(|pass| pass.label.as_str())
            .collect();
        assert_eq!(with_depth, ["view 0"]);
    }

    #[test]
    fn a_depth_attachment_gets_a_lifetime_so_it_is_actually_allocated() {
        // Without one the backend has nothing to allocate against and the pass would run with no
        // depth buffer at all — which is not an error anywhere, just wrong pictures.
        let frame = frame_in_3d();
        let graph = frame_graph(&frame, 64, 64);
        let plan = graph.compile().expect("valid");

        assert!(
            plan.lifetimes().contains_key(DEPTH),
            "lifetimes: {:?}",
            plan.lifetimes().keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_undeclared_depth_attachment_is_refused_like_any_other_resource() {
        let mut graph = RenderGraph::new();
        graph.transient("scene", 8, 8, TargetFormat::Hdr16);
        graph.pass(
            "view 0",
            PassKind::View {
                index: 0,
                clears: true,
            },
            &[],
            &["scene"],
        );
        graph.with_depth("depth_nobody_declared");

        let error = graph.compile().expect_err("depth was never declared");
        assert!(
            matches!(error, GraphError::UnknownResource { .. }),
            "{error}"
        );
        assert!(
            error.to_string().contains("depth_nobody_declared"),
            "{error}"
        );
    }

    #[test]
    fn capture_reads_the_displayable_image_rather_than_the_hdr_one() {
        // If this ever came back as `scene`, a windowed capture would hand out pixels nothing can
        // display — and it would do it silently, since the values are perfectly valid floats.
        let frame = frame_with_views(1);
        let graph = frame_graph(&frame, 64, 64);
        let plan = graph.compile().expect("a frame graph is always valid");
        assert_eq!(plan.destination_source(), Some(OUTPUT));
    }
    /// A 3D frame whose light casts a shadow.
    fn frame_with_a_shadow() -> FrameData {
        let mut frame = frame_in_3d();
        frame.views[0].lights = vec![crate::LightData {
            direction: [0.0, -1.0, 0.0],
            colour: [1.0, 1.0, 1.0],
            shadow: Some(crate::ShadowData {
                view_projection: amadeo_transform::Mat4::IDENTITY,
                resolution: 512,
                bias: 0.001,
            }),
        }];
        frame
    }

    #[test]
    fn a_frame_with_no_shadow_casting_light_declares_no_shadow_map() {
        // The same rule the depth buffer follows: a game that never asked for shadows must not
        // allocate a shadow map or run an extra pass over all its geometry.
        let frame = frame_in_3d();
        let graph = frame_graph(&frame, 64, 64);

        assert!(
            graph
                .transients()
                .iter()
                .all(|t| t.format != TargetFormat::ShadowMap32)
        );
        assert!(
            graph
                .passes()
                .iter()
                .all(|pass| !matches!(pass.kind, PassKind::Shadow { .. }))
        );
    }

    #[test]
    fn a_shadow_map_is_declared_at_the_resolution_the_light_asked_for() {
        let frame = frame_with_a_shadow();
        let graph = frame_graph(&frame, 64, 64);

        let map = graph
            .transients()
            .iter()
            .find(|t| t.format == TargetFormat::ShadowMap32)
            .expect("a casting light needs somewhere to put its depths");
        // Square, and at the light's resolution rather than the window's -- a shadow map is not a
        // full-screen image and sizing it like one would tie shadow quality to window size.
        assert_eq!((map.width, map.height), (512, 512));
    }

    #[test]
    fn the_shadow_pass_is_ordered_before_the_view_that_reads_it() {
        // **The property the whole graph exists for**, and the one that would be a mystery to debug
        // if it were merely asserted by writing the passes in the right order. The view pass reads
        // the shadow map, the shadow pass writes it, and the order falls out of that -- so it is
        // checkable here with no GPU, which is exactly what ADR 0034 said an internal graph buys.
        let frame = frame_with_a_shadow();
        let graph = frame_graph(&frame, 64, 64);
        let plan = graph.compile().expect("a valid frame");

        let position = |predicate: &dyn Fn(&Pass) -> bool| {
            plan.order()
                .iter()
                .position(|&index| predicate(&graph.passes()[index]))
                .expect("present")
        };

        let shadow = position(&|pass| matches!(pass.kind, PassKind::Shadow { .. }));
        let view = position(&|pass| matches!(pass.kind, PassKind::View { .. }));
        assert!(
            shadow < view,
            "the shadow map has to be drawn before it is sampled; got shadow at {shadow}, view at {view}"
        );
    }

    #[test]
    fn a_shadow_map_and_the_scene_depth_buffer_never_share_a_texture() {
        // They are the same wgpu format and differ in the usages they need, so a pool that matched
        // them would hand one of them a texture missing `TEXTURE_BINDING`. The distinct format tag
        // is what prevents it -- `assign_transients` matches on (width, height, format).
        let mut frame = frame_with_a_shadow();
        // Force the awkward case: a shadow map exactly the size of the window's depth buffer.
        frame.views[0].lights[0].shadow = Some(crate::ShadowData {
            view_projection: amadeo_transform::Mat4::IDENTITY,
            resolution: 64,
            bias: 0.001,
        });
        let graph = frame_graph(&frame, 64, 64);

        let depth = graph
            .transients()
            .iter()
            .find(|t| t.name == DEPTH)
            .expect("declared");
        let map = graph
            .transients()
            .iter()
            .find(|t| t.format == TargetFormat::ShadowMap32)
            .expect("declared");

        assert_eq!((depth.width, depth.height), (map.width, map.height));
        assert_ne!(
            depth.format, map.format,
            "same size, so only the format tag can keep them from sharing one texture"
        );
    }
}
