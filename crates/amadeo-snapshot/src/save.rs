//! Reading a snapshot as a **save file**: the same bytes, read leniently (ADR 0069).
//!
//! # The rule the whole module turns on
//!
//! A save carries a fingerprint of the *shape* of everything in it (see [`crate::fingerprint`]), and
//! that fingerprint decides how this behaves:
//!
//! - **It matches the current build.** Nothing has changed shape, so every failure is a genuinely
//!   corrupt or hand-broken file rather than a version gap. This behaves exactly like
//!   [`crate::restore`] — hard errors, and the state hash enforced.
//! - **It differs.** The file was written by a build whose components had a different shape, so a
//!   missing field is expected rather than suspicious. Fields are defaulted, unknown names are
//!   dropped, everything is reported, and the recorded state hash is **not** enforced, because it
//!   describes a layout that no longer exists.
//!
//! That split is what lets one format serve both jobs honestly. The player who has not updated
//! keeps the full integrity check; leniency costs something only when it is actually needed. And it
//! means the strict path keeps being exercised by every ordinary load, rather than quietly rotting.
//!
//! # Who decides what a damaged save means
//!
//! This loads as much as it can and **returns a [`SaveReport`] saying what it could not**. It does
//! not decide whether that is acceptable, because it cannot: whether a player entity that lost a
//! component is a recoverable save or a ruined one is genre knowledge, and invariant I4 puts that
//! above the engine. A game that wants to refuse should read the report and refuse.

use crate::{RestoreError, Snapshot, fingerprint};
use amadeo_ecs::{ComponentRegistry, World};
use amadeo_reflect::{TypeKind, Value, default_value_for};
use std::collections::BTreeMap;

use crate::redirects::Redirects;

/// One field that the file did not have, filled in from its type's default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Defaulted {
    /// Which entity, as `index:generation`, or `None` for a resource.
    pub entity: Option<String>,
    /// The component or resource it belongs to.
    pub owner: String,
    /// The field's name.
    pub field: String,
}

/// Something in the file that this build could not place, and what was done about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dropped {
    /// Which entity, as `index:generation`, or `None` for a resource.
    pub entity: Option<String>,
    /// The component or resource it belongs to.
    pub owner: String,
    /// The field, when only one field was dropped rather than the whole thing.
    pub field: Option<String>,
    /// Why, in a sentence a person can act on.
    pub reason: String,
}

/// An old name that a redirect file mapped to a current one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirected {
    /// The name in the file.
    pub from: String,
    /// The name it was read as.
    pub to: String,
}

/// What a lenient restore had to do to make the file fit this build.
///
/// **Empty is the good answer.** Every entry is a place where the loaded world differs from the one
/// that was saved, and a defaulted field is a *silent gameplay change* — a save that comes back with
/// a new `battery: 0.0` reads as a bug in the game rather than as a consequence of the save
/// predating the field. That is why this is returned rather than logged, and why nothing here is
/// merely a warning count.
///
/// This is `asset_problems`, `SoundCache::failures` and `Animatable::missing` a fourth time: when
/// the engine survives something instead of refusing it, the report *is* the diagnosis.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveReport {
    /// Whether the file's layout matched this build's exactly.
    ///
    /// When true, nothing else here can be non-empty and the state hash was enforced.
    pub exact: bool,
    /// Whether the recorded state hash was checked. Follows `exact`, and is stated separately
    /// because it is the question somebody debugging a divergence will actually be asking.
    pub state_hash_checked: bool,
    /// Fields the file did not have.
    pub defaulted: Vec<Defaulted>,
    /// Things this build could not place.
    pub dropped: Vec<Dropped>,
    /// Names a redirect file mapped.
    pub redirected: Vec<Redirected>,
}

impl SaveReport {
    /// Whether the loaded world is exactly the world that was saved.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.defaulted.is_empty() && self.dropped.is_empty()
    }

    /// A human-readable summary, one line per thing that happened.
    ///
    /// Empty when [`SaveReport::is_clean`]. Written here rather than left to each caller so that two
    /// games cannot describe the same problem differently.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();

        for entry in &self.redirected {
            out.push(format!(
                "`{}` was read as `{}`, from the redirect file",
                entry.from, entry.to
            ));
        }
        for entry in &self.defaulted {
            let where_ = entry
                .entity
                .as_ref()
                .map_or_else(|| "resource".to_string(), |e| format!("entity {e}"));
            out.push(format!(
                "{where_}: `{}`.{} was not in the save and was set to its default. If that field \
                 matters to how the game plays, this save will resume differently",
                entry.owner, entry.field
            ));
        }
        for entry in &self.dropped {
            let where_ = entry
                .entity
                .as_ref()
                .map_or_else(|| "resource".to_string(), |e| format!("entity {e}"));
            let what = match &entry.field {
                Some(field) => format!("`{}`.{field}", entry.owner),
                None => format!("`{}`", entry.owner),
            };
            out.push(format!("{where_}: {what} was dropped -- {}", entry.reason));
        }

        out
    }
}

/// Puts a save back, filling in what a newer build expects and reporting every difference.
///
/// See the module docs for how this differs from [`crate::restore`], and when it does not differ at
/// all.
///
/// # Errors
///
/// [`RestoreError`], for the failures that leniency cannot explain away: a file whose entity slots
/// do not add up, and — **when the layout matches** — anything [`crate::restore`] would refuse,
/// including the state hash.
pub fn restore_save(
    world: &mut World,
    registry: &ComponentRegistry,
    snapshot: &Snapshot,
    redirects: &Redirects,
) -> Result<SaveReport, RestoreError> {
    // The layout question is asked once, before anything is rebuilt, because every decision below
    // depends on the answer.
    let exact = fingerprint(&snapshot.schema, world, registry) == snapshot.layout;

    if exact {
        // Nothing has changed shape, so there is nothing to be lenient about and being lenient
        // anyway would hide a genuinely broken file. One code path, taken by every ordinary load,
        // which is also what stops the strict path from rotting.
        crate::restore(world, registry, snapshot)?;
        return Ok(SaveReport {
            exact: true,
            state_hash_checked: true,
            ..SaveReport::default()
        });
    }

    let mut report = SaveReport {
        exact: false,
        state_hash_checked: false,
        ..SaveReport::default()
    };

    crate::validate_slots(snapshot)?;
    let live: Vec<_> = snapshot.entities.iter().map(|row| row.entity).collect();
    world.restore_entities(&live, &snapshot.free_slots);
    world.set_tick(snapshot.tick);

    for row in &snapshot.entities {
        let entity = row.entity.to_string();
        for (recorded_name, recorded_value) in &row.components {
            let name = redirect_name(recorded_name, redirects, &mut report);

            if !registry.contains(&name) {
                report.dropped.push(Dropped {
                    entity: Some(entity.clone()),
                    owner: name,
                    field: None,
                    reason: "this build has no component by that name. It was removed, or renamed \
                             without a redirect"
                        .to_string(),
                });
                continue;
            }

            let Some(info) = registry.info(&name) else {
                continue;
            };
            let reconciled = reconcile(
                recorded_value,
                &name,
                info,
                registry,
                redirects,
                Some(&entity),
                &mut report,
            );

            if let Err(error) = registry.insert(world, row.entity, &name, &reconciled) {
                report.dropped.push(Dropped {
                    entity: Some(entity.clone()),
                    owner: name,
                    field: None,
                    reason: format!("it would not rebuild: {error}"),
                });
            }
        }
    }

    for (recorded_name, recorded_value) in &snapshot.resources {
        let name = redirect_name(recorded_name, redirects, &mut report);

        let Some(info) = world.resource_schema(&name).cloned() else {
            // Matches `restore`'s existing behaviour for a resource a build does not have: dropping
            // a subsystem should not make old files unloadable. Reported rather than silent, which
            // is the part that changes here.
            report.dropped.push(Dropped {
                entity: None,
                owner: name,
                field: None,
                reason: "this build has no resource by that name".to_string(),
            });
            continue;
        };

        let reconciled = reconcile(
            recorded_value,
            &name,
            &info,
            registry,
            redirects,
            None,
            &mut report,
        );

        if let Err(error) = world.restore_resource(&name, &reconciled) {
            report.dropped.push(Dropped {
                entity: None,
                owner: name,
                field: None,
                reason: format!("it would not rebuild: {error}"),
            });
        }
    }

    Ok(report)
}

/// Follows a redirect and records it if it moved.
fn redirect_name(recorded: &str, redirects: &Redirects, report: &mut SaveReport) -> String {
    let name = redirects.component(recorded);
    if name != recorded {
        let entry = Redirected {
            from: recorded.to_string(),
            to: name.clone(),
        };
        // One line per rename, not one per entity that used it: a thousand redirected `Transform`s
        // is one fact, and a report nobody will read through is not a report.
        if !report.redirected.contains(&entry) {
            report.redirected.push(entry);
        }
    }
    name
}

/// Makes a recorded value fit a type this build has: rename fields, drop unknown ones, default
/// missing ones.
///
/// Only the **top level** is reconciled. A nested struct whose own shape changed is not repaired,
/// and that is deliberate rather than unfinished: ADR 0029's overrides reach an instance root and
/// nothing inside it, for the reason that a patch reaching arbitrarily deep is one nobody can
/// predict the result of. A nested change surfaces as the component failing to rebuild, which is
/// reported by name.
fn reconcile(
    recorded: &Value,
    owner: &str,
    info: &amadeo_reflect::TypeInfo,
    registry: &ComponentRegistry,
    redirects: &Redirects,
    entity: Option<&str>,
    report: &mut SaveReport,
) -> Value {
    let (Value::Struct(recorded_fields), TypeKind::Struct { fields }) = (recorded, &info.kind)
    else {
        // A component that reflects as a scalar or an enum has no fields to reconcile. It either
        // still fits or it does not, and the failure is reported by the caller.
        return recorded.clone();
    };

    let mut built: BTreeMap<String, Value> = BTreeMap::new();

    for (recorded_field, value) in recorded_fields {
        let field = redirects.field(owner, recorded_field);
        if fields.iter().any(|known| known.name == field) {
            built.insert(field, value.clone());
        } else {
            report.dropped.push(Dropped {
                entity: entity.map(str::to_string),
                owner: owner.to_string(),
                field: Some(field),
                reason: "the component no longer has that field".to_string(),
            });
        }
    }

    for field in fields {
        if built.contains_key(&field.name) {
            continue;
        }
        match default_value_for(&field.type_name, registry.types()) {
            Ok(value) => {
                built.insert(field.name.clone(), value);
                report.defaulted.push(Defaulted {
                    entity: entity.map(str::to_string),
                    owner: owner.to_string(),
                    field: field.name.clone(),
                });
            }
            Err(why) => {
                // Left absent on purpose. The component will fail to rebuild and be reported by
                // name, which is a better outcome than inventing a value: `default_value_for`
                // refuses exactly where a guess would have gameplay meaning.
                report.dropped.push(Dropped {
                    entity: entity.map(str::to_string),
                    owner: owner.to_string(),
                    field: Some(field.name.clone()),
                    reason: format!("it is missing from the save and has no default -- {why}"),
                });
            }
        }
    }

    Value::Struct(built)
}
