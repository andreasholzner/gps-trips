//! What a page's map shows of its links (US-99): the ticked items, emptied
//! on a page of its own, and the item a page opens with ticked (US-103).

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::LinkOther;

use super::overlays::overlays_of;
use super::proposal::to_tick;
use super::{Group, Row, Shown};
use crate::api::{ApiClient, LinkEnd};
use crate::interop::Overlays;

/// What is ticked to be shown, emptied on a page of its own, and the
/// overlays it comes to, read from the archive whenever either the ticks or
/// the `listed` rows change — so an item whose link is removed leaves the
/// map too. `show` is ticked once, as soon as it is listed.
pub(super) fn use_shown(
    end: LinkEnd,
    mut overlays: Signal<Overlays>,
    show: Option<LinkOther>,
    listed: impl Fn() -> Vec<Group> + Clone + 'static,
) -> Signal<BTreeSet<Shown>> {
    let archive = use_context::<Signal<ApiClient>>();
    let mut shown = use_signal(BTreeSet::<Shown>::new);
    // What `show` was ticked for, so unticking it sticks.
    let mut ticked_for = use_signal(|| None::<(LinkEnd, LinkOther)>);
    use_effect(use_reactive!(|end| {
        let _ = end;
        shown.write().clear();
    }));
    let rows = listed.clone();
    use_effect(use_reactive!(|end, show| {
        let Some(other) = show else { return };
        if *ticked_for.peek() == Some((end, other)) {
            return;
        }
        if let Some(item) = to_tick(other, &rows()) {
            shown.write().insert(item);
            ticked_for.set(Some((end, other)));
        }
    }));
    let _read = use_resource(move || {
        let ticked: Vec<Row> = listed()
            .into_iter()
            .flat_map(|group| group.rows)
            .filter(|row| shown.read().contains(&row.shown))
            .collect();
        async move { overlays.set(overlays_of(&archive(), ticked).await) }
    });
    shown
}
