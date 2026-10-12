//! Correcting a waypoint's details (US-98) in a form over the screen, as a
//! collection track's are (US-86): its name, description, elevation, and
//! its symbol, chosen from those the archive has an icon for.

use dioxus::prelude::*;
use trip_archive_types::{Waypoint, WaypointEdit};

use crate::api::{self, ApiClient};
use crate::overlay::Overlay;
use crate::waypoint_icon::{symbol_groups, WaypointSymbol};

/// The edit form's fields, as the inputs hold them; empty for none.
#[derive(Clone, Debug, PartialEq)]
pub struct WaypointForm {
    pub name: String,
    pub description: String,
    pub symbol: String,
    /// Metres, as typed.
    pub ele: String,
}

impl WaypointForm {
    pub fn of(waypoint: &Waypoint) -> Self {
        Self {
            name: waypoint.name.clone().unwrap_or_default(),
            description: waypoint.description.clone().unwrap_or_default(),
            symbol: waypoint.symbol.clone().unwrap_or_default(),
            ele: waypoint.ele.map_or_else(String::new, |ele| ele.to_string()),
        }
    }
}

/// What to ask the archive to change: only what the owner altered, a field
/// emptied sent blank. Refused without a name — also for a waypoint its file
/// gave none.
pub fn changes(waypoint: &Waypoint, form: &WaypointForm) -> Result<WaypointEdit, &'static str> {
    let name = form.name.trim();
    if name.is_empty() {
        return Err("A waypoint needs a name.");
    }
    let opened_with = WaypointForm::of(waypoint);
    let changed = |now: &str, then: &str| (now != then).then(|| now.to_string());
    Ok(WaypointEdit {
        name: changed(name, &opened_with.name),
        description: changed(form.description.trim(), &opened_with.description),
        symbol: changed(&form.symbol, &opened_with.symbol),
        ele: changed(form.ele.trim(), &opened_with.ele),
        lat: None,
        lon: None,
    })
}

/// The waypoint's symbol as an option of its own, when none of the offered
/// ones is it: one the archive has no icon for, or one spelled otherwise
/// than Garmin does. It stays chosen until another is.
pub fn own_symbol(symbol: &str) -> Option<String> {
    let offered = symbol_groups()
        .into_iter()
        .any(|(_, symbols)| symbols.iter().any(|offered| offered == symbol));
    (!symbol.is_empty() && !offered).then(|| symbol.to_string())
}

/// Editing, folded away until asked for, over the screen.
#[component]
pub(super) fn EditWaypoint(waypoint: Waypoint, on_saved: EventHandler<()>) -> Element {
    let mut open = use_signal(|| false);
    // An editor left open would carry over to the next waypoint the router
    // shows through this same scope.
    let id = waypoint.id;
    use_effect(use_reactive!(|id| {
        let _ = id;
        open.set(false);
    }));

    rsx! {
        button {
            id: "edit-waypoint",
            r#type: "button",
            class: "quiet",
            onclick: move |_| open.set(true),
            "Edit waypoint"
        }
        if open() {
            Overlay { label: "Edit waypoint", on_close: move |_| open.set(false),
                div { class: "panel",
                    EditWaypointForm {
                        waypoint,
                        on_saved: move |_| {
                            open.set(false);
                            on_saved.call(());
                        },
                        on_cancel: move |_| open.set(false),
                    }
                }
            }
        }
    }
}

#[component]
fn EditWaypointForm(
    waypoint: Waypoint,
    on_saved: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut form = use_signal(|| WaypointForm::of(&waypoint));
    let mut error = use_signal(|| None::<String>);
    let own = waypoint.symbol.as_deref().and_then(own_symbol);
    let id = waypoint.id;

    rsx! {
        form {
            id: "edit-waypoint-form",
            onsubmit: move |event| {
                event.prevent_default();
                let edit = changes(&waypoint, &form.read());
                async move {
                    let edit = match edit {
                        Ok(edit) => edit,
                        Err(refused) => {
                            error.set(Some(refused.to_string()));
                            return;
                        }
                    };
                    if edit == WaypointEdit::default() {
                        on_cancel.call(());
                        return;
                    }
                    match api::edit_waypoint(&archive(), id, &edit).await {
                        Ok(()) => on_saved.call(()),
                        Err(err) => error.set(Some(err.to_string())),
                    }
                }
            },
            p {
                label { r#for: "waypoint-edit-name", "Name" }
                input {
                    id: "waypoint-edit-name",
                    r#type: "text",
                    value: "{form.read().name}",
                    oninput: move |event| form.write().name = event.value(),
                }
            }
            p {
                label { r#for: "waypoint-edit-description", "Description" }
                textarea {
                    id: "waypoint-edit-description",
                    value: "{form.read().description}",
                    oninput: move |event| form.write().description = event.value(),
                }
            }
            p {
                label { r#for: "waypoint-edit-ele", "Elevation (m)" }
                input {
                    id: "waypoint-edit-ele",
                    r#type: "number",
                    step: "any",
                    value: "{form.read().ele}",
                    oninput: move |event| form.write().ele = event.value(),
                }
            }
            SymbolChooser {
                value: form.read().symbol.clone(),
                own,
                onchange: move |symbol| form.write().symbol = symbol,
            }
            button { r#type: "submit", "Save" }
            button { r#type: "button", onclick: move |_| on_cancel.call(()), "Cancel" }
            if let Some(message) = error() {
                p { class: "error", "{message}" }
            }
        }
    }
}

/// The symbols to choose from, each with its icon: none, the waypoint's own
/// where it is not among them, then the archive's in their groups.
#[component]
pub fn SymbolChooser(
    value: String,
    own: Option<String>,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        fieldset { id: "waypoint-edit-symbol", class: "symbol-chooser",
            legend { "Symbol" }
            div { class: "symbol-options",
                SymbolOption { symbol: "", label: "None", checked: value.is_empty(), onchange }
                if let Some(own) = own {
                    SymbolOption {
                        symbol: own.clone(),
                        label: own.clone(),
                        checked: value == own,
                        onchange,
                    }
                }
            }
            for (group, symbols) in symbol_groups() {
                h3 { "{group}" }
                div { class: "symbol-options",
                    for symbol in symbols {
                        SymbolOption {
                            key: "{symbol}",
                            symbol: symbol.clone(),
                            label: symbol.clone(),
                            checked: value == symbol,
                            onchange,
                        }
                    }
                }
            }
        }
    }
}

/// One choice: a radio, the icon, the name.
#[component]
fn SymbolOption(
    symbol: String,
    label: String,
    checked: bool,
    onchange: EventHandler<String>,
) -> Element {
    let icon = (!symbol.is_empty()).then(|| symbol.clone());
    rsx! {
        label { class: "symbol-option",
            input {
                r#type: "radio",
                name: "waypoint-symbol",
                value: "{symbol}",
                checked,
                onchange: move |_| onchange.call(symbol.clone()),
            }
            WaypointSymbol { symbol: icon }
            "{label}"
        }
    }
}
