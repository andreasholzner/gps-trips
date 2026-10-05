//! The activity drop-down: a list of its own, each entry the activity's
//! icon and name, since a native `select` cannot show an icon in its
//! options. It stands wherever one activity is picked — the trip list's
//! filter, the edit and import forms and the bulk panel.
//!
//! The native `select` is rendered beside it, with the chosen activity's
//! icon in front, and hidden by `activity.css`: a phone may be better served by
//! its own picker, and switching to it there is one media query, not a
//! rewrite.
//!
//! Open/closed is state, and a backdrop behind the open list closes it on a
//! click outside, as the header menu does (US-60).

use dioxus::prelude::*;
use trip_archive_types::ActivityType;

use crate::activity_icon::ActivityIcon;

/// One entry: the value the form keeps, what the entry says, and the
/// activity whose icon it shows, if any.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Choice {
    pub value: &'static str,
    pub label: &'static str,
    pub icon: Option<ActivityType>,
}

impl Choice {
    /// An entry that is no activity, such as "— any —": no icon.
    pub const fn plain(value: &'static str, label: &'static str) -> Self {
        Self {
            value,
            label,
            icon: None,
        }
    }

    /// An activity, by its wire value.
    pub fn activity(activity: ActivityType) -> Self {
        Self {
            value: activity.as_str(),
            label: activity.label(),
            icon: Some(activity),
        }
    }

    /// The unspecified activity, kept as `value`: the edit and import forms
    /// send it as an empty string.
    pub fn unspecified(value: &'static str) -> Self {
        Self {
            value,
            ..Self::activity(ActivityType::Unknown)
        }
    }
}

/// `leading`, then every activity in the order the pickers list them.
pub fn choices(leading: &[Choice]) -> Vec<Choice> {
    leading
        .iter()
        .copied()
        .chain(ActivityType::SELECTABLE.into_iter().map(Choice::activity))
        .collect()
}

/// The drop-down. `id` names the button that opens it; `label` is shown
/// above it unless `hide_label`, and names it either way. A `value` among
/// none of the `choices` shows the first.
#[component]
pub fn ActivitySelect(
    id: &'static str,
    label: &'static str,
    #[props(default)] hide_label: bool,
    value: String,
    choices: Vec<Choice>,
    onchange: EventHandler<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut active = use_signal(|| 0usize);
    let mut button = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let chosen = choices
        .iter()
        .position(|choice| choice.value == value)
        .unwrap_or(0);
    let current = choices[chosen];
    let count = choices.len();
    let label_id = format!("{id}-label");
    let list_id = format!("{id}-list");

    let refocus = move || {
        if let Some(button) = button() {
            spawn(async move {
                let _ = button.set_focus(true).await;
            });
        }
    };
    let mut choose = move |value: &'static str| {
        open.set(false);
        onchange.call(value.to_string());
        refocus();
    };
    let values: Vec<&'static str> = choices.iter().map(|choice| choice.value).collect();

    rsx! {
        div { class: "activity-select",
            span {
                id: "{label_id}",
                class: if hide_label { "visually-hidden" } else { "activity-select-label" },
                "{label}"
            }
            div {
                class: "activity-select-custom",
                // An open list takes Escape for itself: the overlay the edit
                // form sits in would close on it too.
                onkeydown: move |event| {
                    if event.key() == Key::Escape && open() {
                        event.stop_propagation();
                        open.set(false);
                        refocus();
                    }
                },
                button {
                    r#type: "button",
                    id: "{id}",
                    class: "activity-select-toggle",
                    "data-value": "{current.value}",
                    aria_haspopup: "listbox",
                    aria_expanded: "{open()}",
                    aria_controls: "{list_id}",
                    "aria-labelledby": "{label_id} {id}",
                    onmounted: move |event| button.set(Some(event.data())),
                    onclick: move |_| {
                        active.set(chosen);
                        open.toggle();
                    },
                    onkeydown: move |event| {
                        if matches!(event.key(), Key::ArrowDown | Key::ArrowUp) {
                            event.prevent_default();
                            active.set(chosen);
                            open.set(true);
                        }
                    },
                    ChoiceFace { choice: current }
                }
                if open() {
                    div { class: "activity-select-backdrop", onclick: move |_| open.set(false) }
                    ul {
                        id: "{list_id}",
                        class: "activity-select-list",
                        role: "listbox",
                        tabindex: "-1",
                        "aria-labelledby": "{label_id}",
                        "aria-activedescendant": "{id}-option-{active()}",
                        onmounted: move |event| async move {
                            let _ = event.data().set_focus(true).await;
                        },
                        onkeydown: move |event| {
                            let next = match event.key() {
                                Key::ArrowDown => (active() + 1) % count,
                                Key::ArrowUp => (active() + count - 1) % count,
                                Key::Home => 0,
                                Key::End => count - 1,
                                // Its default would press the button focus
                                // has just returned to, opening the list again.
                                Key::Enter => {
                                    event.prevent_default();
                                    return choose(values[active()]);
                                }
                                Key::Character(key) if key == " " => {
                                    event.prevent_default();
                                    return choose(values[active()]);
                                }
                                Key::Tab => return open.set(false),
                                _ => return,
                            };
                            event.prevent_default();
                            active.set(next);
                        },
                        for (index, choice) in choices.iter().copied().enumerate() {
                            li {
                                key: "{choice.value}",
                                id: "{id}-option-{index}",
                                role: "option",
                                "data-value": "{choice.value}",
                                "aria-selected": "{index == chosen}",
                                class: if index == active() { "active" },
                                onmousemove: move |_| active.set(index),
                                onclick: move |_| choose(choice.value),
                                ChoiceFace { choice }
                            }
                        }
                    }
                }
            }
            // The phone's own picker, hidden unless `activity.css` switches to it.
            div { class: "activity-select-native",
                ChoiceIcon { choice: current }
                select {
                    id: "{id}-native",
                    "aria-labelledby": "{label_id}",
                    value: "{current.value}",
                    onchange: move |event| onchange.call(event.value()),
                    for choice in choices.iter() {
                        option {
                            key: "{choice.value}",
                            value: choice.value,
                            selected: choice.value == current.value,
                            "{choice.label}"
                        }
                    }
                }
            }
        }
    }
}

/// An entry as the list and the closed drop-down show it.
#[component]
fn ChoiceFace(choice: Choice) -> Element {
    rsx! {
        ChoiceIcon { choice }
        span { class: "activity-select-text", "{choice.label}" }
    }
}

/// The entry's icon, or an empty space of its size, so the names line up.
#[component]
fn ChoiceIcon(choice: Choice) -> Element {
    match choice.icon {
        Some(activity) => rsx! { ActivityIcon { activity } },
        None => rsx! { span { class: "activity-icon", "aria-hidden": "true" } },
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    #[test]
    fn the_choices_are_the_leading_ones_then_every_activity_in_picker_order() {
        let list = choices(&[Choice::plain("", "— any —")]);

        assert_eq!(list.len(), 1 + ActivityType::SELECTABLE.len());
        assert_eq!(list[0], Choice::plain("", "— any —"));
        assert_eq!(list[1], Choice::activity(ActivityType::Hiking));
        assert_eq!(list[1].value, "hiking");
    }

    #[test]
    fn the_unspecified_entry_keeps_the_forms_value_and_has_its_icon() {
        let entry = Choice::unspecified("");

        assert_eq!(entry.value, "");
        assert_eq!(entry.label, ActivityType::Unknown.label());
        assert_eq!(entry.icon, Some(ActivityType::Unknown));
    }

    fn rendered(value: &'static str) -> String {
        render(move || {
            rsx! {
                ActivitySelect {
                    id: "pick",
                    label: "Activity",
                    value: value.to_string(),
                    choices: choices(&[Choice::plain("", "— any —")]),
                    onchange: |_| {},
                }
            }
        })
    }

    #[test]
    fn closed_it_shows_the_chosen_activitys_icon_and_name() {
        let html = rendered("kayaking");

        assert!(html.contains(r#"id="pick""#), "{html}");
        assert!(html.contains(r#"aria-expanded="false""#), "{html}");
        assert!(
            html.contains(r#"aria-labelledby="pick-label pick""#),
            "{html}"
        );
        assert!(html.contains(r#"data-value="kayaking""#), "{html}");
        assert!(html.contains(r#"title="Kayaking""#), "the icon: {html}");
        assert!(html.contains(">Kayaking</span>"), "the name: {html}");
        // The list is only there once opened.
        assert!(!html.contains(r#"role="listbox""#), "{html}");
    }

    #[test]
    fn an_entry_without_an_activity_keeps_the_icons_space_empty() {
        let html = rendered("");

        assert!(html.contains(">— any —</span>"), "{html}");
        assert!(
            html.contains(r#"<span class="activity-icon" aria-hidden="true"></span>"#),
            "{html}"
        );
    }

    #[test]
    fn the_native_select_is_there_for_phones_with_every_choice() {
        let html = rendered("cycling");

        assert!(html.contains(r#"id="pick-native""#), "{html}");
        assert_eq!(
            html.matches("<option").count(),
            1 + ActivityType::SELECTABLE.len(),
            "{html}"
        );
    }

    #[test]
    fn a_hidden_label_still_names_the_drop_down() {
        let html = render(|| {
            rsx! {
                ActivitySelect {
                    id: "bulk",
                    label: "Activity for selected trips",
                    hide_label: true,
                    value: String::new(),
                    choices: choices(&[Choice::plain("", "Choose…")]),
                    onchange: |_| {},
                }
            }
        });

        assert!(
            html.contains(r#"class="visually-hidden">Activity for selected trips<"#),
            "{html}"
        );
    }
}
