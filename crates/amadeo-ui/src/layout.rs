//! Turning authored nodes into rectangles — ADR 0062's layout pass.
//!
//! # The whole algorithm, in four sentences
//!
//! A root node is laid out against the screen. A node's children are laid out against its
//! **content box**, which is its rectangle minus its padding. If the node is not flowing, each child
//! places itself with its own anchor; if it is, children are stacked along the flow direction and
//! only the *cross* axis uses the anchor. Then recurse.
//!
//! That is one pass, top down, with no measurement step — which is the deliberate difference from
//! flexbox. Flexbox needs multiple passes because content can size its own container (a paragraph of
//! text decides how tall its box is). **Nothing here sizes its parent**, so the tree can be walked
//! once, and a person can follow it.
//!
//! # Why there is no wrapping and no intrinsic sizing
//!
//! Both are what turn a layout algorithm into a layout *engine*. Neither is needed by the interfaces
//! ADR 0062 lists — a title screen, a pause menu, a HUD — and adding either later is additive.
//! Adding them speculatively would be building the thing taffy already does better.

use crate::components::{Align, ComputedRect, Flow, UiEdges, UiNode};
use crate::theme::Theme;
use amadeo_ecs::{Entity, World};
use amadeo_transform::Parent;

/// How deep a UI tree may nest before the walk gives up.
///
/// A cycle in `Parent` is possible — nothing forbids it — and the honest failure is to stop rather
/// than to recurse forever. The same guard `propagate_transforms` uses, and the same number.
///
/// Shared with the draw pass, which walks the same tree upwards. Two numbers would let a node be
/// laid out by one and skipped by the other, and the symptom would be a widget that draws with a
/// rectangle nothing believes in.
pub(crate) const MAX_DEPTH: usize = 64;

/// Lays out every UI tree in the world against a screen of `width` by `height` pixels.
///
/// Writes a [`ComputedRect`] onto every visible node. **Invisible nodes and their descendants are
/// skipped entirely** — a hidden pause menu costs one comparison rather than a subtree of layout,
/// which is the whole reason `visible` is a field rather than a despawn.
///
/// # It reads and writes only derived state
///
/// `ComputedRect` is marked derived (ADR 0019), so nothing this function writes reaches the state
/// hash. That matters more here than elsewhere: layout depends on the **window size**, and a game
/// played at two resolutions must not produce two different worlds.
pub fn layout_ui(world: &mut World, width: f32, height: f32) {
    let screen = ComputedRect {
        left: 0.0,
        top: 0.0,
        width: width.max(0.0),
        height: height.max(0.0),
    };

    // Padding and gap are theme tokens (ADR 0064), so layout needs to know what they mean. Copied
    // out once rather than looked up per node: a `Theme` is small, and holding a borrow of it would
    // fight the world borrow the walk below needs.
    //
    // The built-in default when a game installed none — a theme cannot be missing, which is
    // `TextureCache`'s argument for its built-in placeholder.
    let theme = world.service::<Theme>().cloned().unwrap_or_default();

    // Collected before writing, because computing reads the whole world while writing needs it
    // mutably — `propagate_transforms` has the same shape for the same reason.
    let mut computed: Vec<(Entity, ComputedRect)> = Vec::new();

    for root in roots(world) {
        place(world, &theme, root, screen, &mut computed, 0);
    }

    for (entity, rect) in computed {
        world.insert(entity, rect);
    }
}

/// Every UI node with no UI node above it, in entity order.
///
/// A node whose `Parent` points at something that is *not* a UI node is a root: attaching a menu to
/// a gameplay entity is not a layout relationship, and treating it as one would lay the menu out
/// inside a rectangle that does not exist.
fn roots(world: &World) -> Vec<Entity> {
    world
        .entities()
        .into_iter()
        .filter(|entity| world.get::<UiNode>(*entity).is_some())
        .filter(|entity| match world.get::<Parent>(*entity) {
            Some(parent) => world.get::<UiNode>(parent.0).is_none(),
            None => true,
        })
        .collect()
}

/// This node's children, in **authored order**.
///
/// Entity order is spawn order is the order a scene file lists them, so a menu's buttons come out
/// the way they were written. That is not a happy accident to rely on quietly — a flow layout whose
/// order came from a hash map would shuffle a menu between runs, so it is stated here and asserted
/// by `a_column_keeps_the_order_the_scene_file_wrote`.
fn children(world: &World, parent: Entity) -> Vec<Entity> {
    world
        .entities()
        .into_iter()
        .filter(|entity| {
            world.get::<UiNode>(*entity).is_some()
                && world.get::<Parent>(*entity).map(|p| p.0) == Some(parent)
        })
        .collect()
}

/// Places one node inside `available`, then recurses into its children.
fn place(
    world: &World,
    theme: &Theme,
    entity: Entity,
    available: ComputedRect,
    out: &mut Vec<(Entity, ComputedRect)>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(node) = world.get::<UiNode>(entity).copied() else {
        return;
    };
    if !node.visible {
        return;
    }

    // The padding token becomes pixels here, and nowhere else in the walk.
    let padding = UiEdges::all(theme.space(node.padding));

    out.push((entity, available));
    arrange_children(
        world,
        theme,
        entity,
        node,
        available.inset(padding),
        out,
        depth,
    );
}

/// Lays out one node's children inside its content box.
fn arrange_children(
    world: &World,
    theme: &Theme,
    entity: Entity,
    node: UiNode,
    content: ComputedRect,
    out: &mut Vec<(Entity, ComputedRect)>,
    depth: usize,
) {
    let children = children(world, entity);
    if children.is_empty() {
        return;
    }

    if !node.flow.arranges() {
        // Each child places itself. This is the HUD case: nothing pushes anything else around.
        for child in children {
            let Some(style) = world.get::<UiNode>(child).copied() else {
                continue;
            };
            if !style.visible {
                continue;
            }
            place(
                world,
                theme,
                child,
                anchored(style, content),
                out,
                depth + 1,
            );
        }
        return;
    }

    flow_children(world, theme, node, content, &children, out, depth);
}

/// Stacks children along the flow direction, sharing out whatever space is left over.
#[allow(clippy::too_many_arguments)]
fn flow_children(
    world: &World,
    theme: &Theme,
    parent: UiNode,
    content: ComputedRect,
    children: &[Entity],
    out: &mut Vec<(Entity, ComputedRect)>,
    depth: usize,
) {
    let horizontal = parent.flow == Flow::Row;

    // Only visible children take part. An invisible one must not leave a gap where it would have
    // been — a menu that hides an option and keeps its space is a menu with a hole in it.
    let taking_part: Vec<(Entity, UiNode)> = children
        .iter()
        .filter_map(|child| world.get::<UiNode>(*child).copied().map(|s| (*child, s)))
        .filter(|(_, style)| style.visible)
        .collect();
    if taking_part.is_empty() {
        return;
    }

    let main_total = if horizontal {
        content.width
    } else {
        content.height
    };
    let gap = theme.space(parent.gap);
    let gaps = gap * (taking_part.len() - 1) as f32;

    // What the fixed-size children ask for, plus their margins along the flow.
    let mut fixed = 0.0;
    let mut grow_total = 0.0;
    for (_, style) in &taking_part {
        let outer = if horizontal {
            style.margin.left + style.margin.right
        } else {
            style.margin.top + style.margin.bottom
        };
        if style.grow > 0.0 {
            grow_total += style.grow;
            fixed += outer;
        } else {
            fixed += outer
                + if horizontal {
                    style.size[0]
                } else {
                    style.size[1]
                };
        }
    }

    // Never negative: children that together ask for more than there is simply overflow, which is
    // visible and diagnosable, where a negative share would place them in reverse.
    let leftover = (main_total - gaps - fixed).max(0.0);

    let mut cursor = if horizontal {
        content.left
    } else {
        content.top
    };
    for (child, style) in taking_part {
        let lead = if horizontal {
            style.margin.left
        } else {
            style.margin.top
        };
        let trail = if horizontal {
            style.margin.right
        } else {
            style.margin.bottom
        };

        let main_size = if style.grow > 0.0 && grow_total > 0.0 {
            leftover * (style.grow / grow_total)
        } else if horizontal {
            style.size[0]
        } else {
            style.size[1]
        };

        cursor += lead;
        let rect = if horizontal {
            let (top, height) = across(
                style,
                style.anchor.vertical,
                content.top,
                content.height,
                style.size[1],
                style.margin.top,
                style.margin.bottom,
                parent.align_children,
            );
            ComputedRect {
                left: cursor,
                top,
                width: main_size,
                height,
            }
        } else {
            let (left, width) = across(
                style,
                style.anchor.horizontal,
                content.left,
                content.width,
                style.size[0],
                style.margin.left,
                style.margin.right,
                parent.align_children,
            );
            ComputedRect {
                left,
                top: cursor,
                width,
                height: main_size,
            }
        };
        cursor += main_size + trail + gap;

        place(world, theme, child, rect, out, depth + 1);
    }
}

/// Where a flowing child sits across the flow direction, and how big it is there.
///
/// **The parent's `align_children` wins over the child's own anchor**, unless the child asks to
/// stretch. That is the rule every flow layout uses and it is worth stating: a column exists to line
/// its children up, so a child quietly opting out by its anchor would defeat the point — while
/// `Stretch` is an explicit request for a different *size*, not a different position, so it is
/// honoured.
#[allow(clippy::too_many_arguments)]
fn across(
    _style: UiNode,
    own: Align,
    start: f32,
    extent: f32,
    size: f32,
    lead: f32,
    trail: f32,
    from_parent: Align,
) -> (f32, f32) {
    let align = if own == Align::Stretch {
        Align::Stretch
    } else {
        from_parent
    };

    match align {
        Align::Stretch => (start + lead, (extent - lead - trail).max(0.0)),
        Align::Start => (start + lead, size),
        Align::Centre => (start + (extent - size) * 0.5, size),
        Align::End => (start + extent - trail - size, size),
    }
}

/// Where a non-flowing child sits, from its own anchor alone.
fn anchored(style: UiNode, content: ComputedRect) -> ComputedRect {
    let (left, width) = along(
        style.anchor.horizontal,
        content.left,
        content.width,
        style.size[0],
        style.margin.left,
        style.margin.right,
    );
    let (top, height) = along(
        style.anchor.vertical,
        content.top,
        content.height,
        style.size[1],
        style.margin.top,
        style.margin.bottom,
    );
    ComputedRect {
        left,
        top,
        width,
        height,
    }
}

/// One axis of an anchored placement: where it starts and how long it is.
///
/// `Stretch` is the only case that ignores the node's own size — the margins become insets, which is
/// how "twenty pixels from every edge" is spelled with no knowledge of the screen size.
fn along(align: Align, start: f32, extent: f32, size: f32, lead: f32, trail: f32) -> (f32, f32) {
    match align {
        Align::Stretch => (start + lead, (extent - lead - trail).max(0.0)),
        Align::Start => (start + lead, size),
        // Margins shift a centred node rather than sizing it, so `margin.left = 10` nudges it right
        // by ten. Subtracting the trail as well is what keeps "centred" symmetric when both are set.
        Align::Centre => (start + (extent - size) * 0.5 + lead - trail, size),
        Align::End => (start + extent - trail - size, size),
    }
}
