//! What a scene file authors: a UI node, and the rectangle layout computes for it.

use amadeo_core::StableHash;
use amadeo_ecs::Component;
use amadeo_reflect::Reflect;

/// How a node lines up along one axis.
///
/// # Four names, used twice
///
/// The same four values answer two different questions, which is why they are one enum rather than
/// two: **where a node sits inside its parent** ([`Anchor`]), and **where a parent puts its children
/// across the flow direction** ([`UiNode::align_children`]). "Pinned to the left edge" and "lined up
/// along the left edge" are the same idea seen from either end.
///
/// Sixteen useful anchors out of four names, rather than sixteen variants somebody has to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum Align {
    /// The left edge, or the top edge. The default, because reading order starts there.
    #[default]
    Start,
    /// Centred on the axis.
    Centre,
    /// The right edge, or the bottom edge.
    End,
    /// Fills the axis, ignoring the node's own size on it.
    ///
    /// This is what makes a background panel or a full-width bar expressible without knowing the
    /// screen size — the margins become insets from the parent's edges.
    Stretch,
}

/// Where a node sits inside its parent.
///
/// One [`Align`] per axis. `Stretch` on both is a panel that fills its parent; `End` and `Start` is
/// the top-right corner; `Centre` and `Centre` is the middle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Anchor {
    /// Along X, where `Start` is the left edge.
    pub horizontal: Align,
    /// Along Y, where `Start` is the **top** edge.
    ///
    /// **Screen space, not world space.** +Y is downward here and the origin is the top-left corner,
    /// which is the opposite of the world convention in ADR 0018. UI is authored in the space it is
    /// drawn in, because "twenty pixels from the top" is what a person means; `render.describe`
    /// already performs the same flip once, deliberately, for the same reason.
    pub vertical: Align,
}

impl Anchor {
    /// Fills the parent on both axes.
    #[must_use]
    pub fn fill() -> Self {
        Self {
            horizontal: Align::Stretch,
            vertical: Align::Stretch,
        }
    }

    /// A corner or edge, spelled as two alignments.
    #[must_use]
    pub fn new(horizontal: Align, vertical: Align) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }
}

/// A measurement on all four sides, in pixels.
///
/// Used for both the space *outside* a node ([`UiNode::margin`]) and the space *inside* it before
/// its children ([`UiNode::padding`]) — the same distinction CSS makes, and for the same reason:
/// a gap belongs either to the thing or to the space around it, and conflating them makes nested
/// layouts impossible to reason about.
#[derive(Debug, Clone, Copy, PartialEq, Default, StableHash, Reflect)]
pub struct UiEdges {
    /// Left, in pixels.
    pub left: f32,
    /// Top, in pixels.
    pub top: f32,
    /// Right, in pixels.
    pub right: f32,
    /// Bottom, in pixels.
    pub bottom: f32,
}

impl UiEdges {
    /// The same measurement on all four sides.
    #[must_use]
    pub fn all(amount: f32) -> Self {
        Self {
            left: amount,
            top: amount,
            right: amount,
            bottom: amount,
        }
    }

    /// Horizontal then vertical, the way CSS shorthand reads.
    #[must_use]
    pub fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        }
    }
}

/// How a node arranges its children.
///
/// # Why this is on the parent
///
/// A child does not decide whether it is in a list; the list does. Putting flow on the parent is
/// what makes "a menu with five buttons" and "a menu with six" the same authored thing — which is
/// the entire reason a layout system exists rather than hand-computed positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum Flow {
    /// Children place themselves, each by its own [`Anchor`].
    ///
    /// The default, and what a HUD wants: a health bar belongs at a screen corner regardless of what
    /// else exists, and nothing should push it around.
    #[default]
    None,
    /// Children are laid left to right.
    Row,
    /// Children are laid top to bottom.
    Column,
}

impl Flow {
    /// Whether this flow arranges children at all.
    #[must_use]
    pub fn arranges(self) -> bool {
        !matches!(self, Flow::None)
    }
}

/// One element of the interface.
///
/// # A widget is an entity
///
/// ADR 0062, following ADR 0031's argument for the camera: a node is an entity with components, so
/// `world.query` sees it, `describe` reports it, a scene file authors it, and a snapshot restores it
/// — none of which needed anything built.
///
/// # This is the *authored* half only
///
/// Where a node ends up is [`ComputedRect`], which layout writes and nobody authors.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct UiNode {
    /// Where this node sits inside its parent.
    pub anchor: Anchor,
    /// Space outside the node, between it and its anchor.
    ///
    /// With a `Stretch` anchor these become insets from the parent's edges, which is how "a panel
    /// twenty pixels from every side" is spelled.
    pub margin: UiEdges,
    /// The node's own size in pixels, on axes it is not stretching.
    ///
    /// Ignored on an axis whose [`Align`] is `Stretch`, and ignored on the main axis when the parent
    /// is flowing and this node has a [`grow`](UiNode::grow) above zero.
    pub size: [f32; 2],
    /// Space inside the node, before its children.
    pub padding: UiEdges,
    /// How this node arranges its own children.
    pub flow: Flow,
    /// Space between children, in pixels. Only meaningful when [`flow`](UiNode::flow) arranges.
    pub gap: f32,
    /// Where children sit across the flow direction. Only meaningful when flowing.
    ///
    /// `Stretch` makes every child fill the cross axis, which is what a column of full-width buttons
    /// wants and is the single most common menu layout there is.
    pub align_children: Align,
    /// This node's share of the space left over along its parent's flow.
    ///
    /// Zero means "use my own size". Above zero, leftover space is divided between the growing
    /// children in proportion to their values — so two children with `1.0` split it evenly, and one
    /// with `2.0` beside one with `1.0` takes two thirds.
    ///
    /// The upper bound is arbitrary and exists only so an editor can draw a slider — proportions
    /// above a handful are indistinguishable from each other in a layout with a fixed amount of
    /// space to share.
    #[reflect(min = 0.0, max = 16.0)]
    pub grow: f32,
    /// Whether this node and its children are laid out and drawn at all.
    ///
    /// A field rather than removing the component, for `AudioSource::playing`'s reason: hiding and
    /// showing a menu must not move entities between archetypes, and a pause menu does exactly that
    /// on every keypress.
    pub visible: bool,
}

impl Default for UiNode {
    fn default() -> Self {
        Self {
            anchor: Anchor::default(),
            margin: UiEdges::default(),
            size: [0.0, 0.0],
            padding: UiEdges::default(),
            flow: Flow::None,
            gap: 0.0,
            align_children: Align::Start,
            grow: 0.0,
            visible: true,
        }
    }
}

impl UiNode {
    /// A node of a fixed size, anchored top-left.
    #[must_use]
    pub fn sized(width: f32, height: f32) -> Self {
        Self {
            size: [width, height],
            ..Self::default()
        }
    }

    /// A node that fills its parent — a background, or a full-screen root.
    #[must_use]
    pub fn full() -> Self {
        Self {
            anchor: Anchor::fill(),
            ..Self::default()
        }
    }

    /// A column of children, centred, filling its parent, with a gap between them. What a menu is.
    ///
    /// # Why it fills, and why that is the constructor's job
    ///
    /// A container has **no size of its own** in this model — there is no intrinsic sizing, by
    /// design (see `layout.rs`), so a flow node left at the default `size` of zero is a 0×0 box.
    /// Its children are then centred in nothing and land at negative coordinates, off the top-left
    /// of the screen.
    ///
    /// That is correct behaviour and a terrible first experience, and it is exactly what a
    /// constructor is for: `Flow::Column`, centred children, and an anchor that gives it something
    /// to be a column *in* are three decisions that always go together, so they are made together.
    /// Build the struct by hand to opt out.
    #[must_use]
    pub fn column(gap: f32) -> Self {
        Self {
            flow: Flow::Column,
            gap,
            align_children: Align::Centre,
            anchor: Anchor::fill(),
            ..Self::default()
        }
    }

    /// A row of children, centred, filling its parent, with a gap between them.
    ///
    /// Fills for the same reason [`UiNode::column`] does.
    #[must_use]
    pub fn row(gap: f32) -> Self {
        Self {
            flow: Flow::Row,
            gap,
            align_children: Align::Centre,
            anchor: Anchor::fill(),
            ..Self::default()
        }
    }
}

impl Component for UiNode {}

/// Where a node ended up, in screen pixels.
///
/// # Computed, never authored — and therefore not hashed
///
/// `GlobalTransform`'s arrangement exactly (ADR 0019), and here the reason is sharper than usual:
/// **this depends on the size of the window.** A game played at 1920×1080 and the same game played
/// at 1280×720 must produce the same state hash, and they would not if where a button landed were
/// part of the world's state.
///
/// So `Component::DERIVED` is true for it, layout overwrites it every frame, and nothing in a
/// `.scene` file spells it. It is still `Reflect` and still a `Component` — an agent can inspect it
/// and a draw pass can query it. Only the *hashing* is skipped, so invariant I8 is untouched.
#[derive(Debug, Clone, Copy, PartialEq, Default, StableHash, Reflect)]
pub struct ComputedRect {
    /// Distance from the left edge of the screen, in pixels.
    pub left: f32,
    /// Distance from the **top** edge of the screen, in pixels — screen space, +Y down.
    pub top: f32,
    /// Width in pixels.
    pub width: f32,
    /// Height in pixels.
    pub height: f32,
}

impl ComputedRect {
    /// A rectangle from its edges rather than its size.
    #[must_use]
    pub fn from_edges(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            width: (right - left).max(0.0),
            height: (bottom - top).max(0.0),
        }
    }

    /// The right edge.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.left + self.width
    }

    /// The bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.top + self.height
    }

    /// The point in the middle.
    #[must_use]
    pub fn centre(&self) -> [f32; 2] {
        [self.left + self.width * 0.5, self.top + self.height * 0.5]
    }

    /// This rectangle with `edges` taken off every side.
    ///
    /// Never inverts: taking 100 pixels of padding off a 50-pixel box gives an empty box rather than
    /// a negative one, because a negative rectangle propagates into every child and the symptom is
    /// widgets appearing on the wrong side of the screen.
    #[must_use]
    pub fn inset(&self, edges: UiEdges) -> Self {
        Self::from_edges(
            self.left + edges.left,
            self.top + edges.top,
            (self.right() - edges.right).max(self.left + edges.left),
            (self.bottom() - edges.bottom).max(self.top + edges.top),
        )
    }

    /// Whether a screen point falls inside. What a click test is built on.
    #[must_use]
    pub fn contains(&self, point: [f32; 2]) -> bool {
        point[0] >= self.left
            && point[0] <= self.right()
            && point[1] >= self.top
            && point[1] <= self.bottom()
    }
}

impl Component for ComputedRect {
    /// ADR 0019: computed, so it is excluded from the state hash. See the type's docs for why that
    /// matters more here than for most derived data — this one depends on the window size.
    const DERIVED: bool = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_reflect::TypeRegistry;

    #[test]
    fn a_node_defaults_to_visible_and_arranging_nothing() {
        let node = UiNode::default();
        assert!(node.visible);
        assert_eq!(node.flow, Flow::None);
        assert!(!node.flow.arranges());
        assert_eq!(node.grow, 0.0, "a node should have to ask to grow");
    }

    #[test]
    fn the_menu_constructor_cannot_get_its_pair_wrong() {
        // `column` exists so the two decisions that always go together cannot be made one at a
        // time: a column with `Align::Start` children is a menu whose buttons cling to the left.
        let menu = UiNode::column(8.0);
        assert_eq!(menu.flow, Flow::Column);
        assert_eq!(menu.align_children, Align::Centre);
        assert_eq!(menu.gap, 8.0);
        // **The third decision, and the one that was missing first time round.** A flow node has no
        // size of its own, so a column that does not fill something is a 0x0 box whose children are
        // centred in nothing and land at negative coordinates. Caught by a failing test rather than
        // by reasoning, which is why it is pinned here.
        assert_eq!(menu.anchor, Anchor::fill());
    }

    #[test]
    fn insetting_further_than_the_box_is_wide_gives_an_empty_box() {
        // **Not a negative one.** A negative rectangle propagates into every child, and the symptom
        // is widgets landing on the wrong side of the screen rather than an error.
        let small = ComputedRect {
            left: 10.0,
            top: 10.0,
            width: 50.0,
            height: 50.0,
        };
        let squeezed = small.inset(UiEdges::all(100.0));

        assert_eq!(squeezed.width, 0.0);
        assert_eq!(squeezed.height, 0.0);
        assert!(squeezed.left >= small.left && squeezed.top >= small.top);
    }

    #[test]
    fn a_rectangle_knows_what_is_inside_it() {
        let rect = ComputedRect {
            left: 10.0,
            top: 20.0,
            width: 100.0,
            height: 40.0,
        };
        assert_eq!(rect.right(), 110.0);
        assert_eq!(rect.bottom(), 60.0);
        assert_eq!(rect.centre(), [60.0, 40.0]);

        assert!(rect.contains([60.0, 40.0]));
        assert!(rect.contains([10.0, 20.0]), "the top-left corner is inside");
        assert!(!rect.contains([9.9, 40.0]));
        // Screen space: a point *above* the box has a **smaller** y. Getting this backwards makes
        // every click land on the wrong widget, which is why it is asserted rather than assumed.
        assert!(!rect.contains([60.0, 19.0]));
    }

    #[test]
    fn everything_authored_round_trips_through_the_value_tree() {
        // Invariant I8: if it cannot be reflected it cannot be serialised, inspected or edited — so
        // this is what says a scene file can author a menu at all.
        let mut registry = TypeRegistry::new();
        registry.register::<UiNode>().expect("registers");
        registry.register::<ComputedRect>().expect("registers");

        let node = UiNode {
            anchor: Anchor::new(Align::End, Align::Stretch),
            margin: UiEdges::symmetric(12.0, 4.0),
            ..UiNode::column(6.0)
        };
        assert_eq!(
            UiNode::from_value(&node.to_value()).expect("round trips"),
            node
        );
    }
}
