//! What lies under the pointer on a map whose lines and waypoints open a
//! page (US-104): a hint naming all of it, and a menu to choose one from.
//! Shared by every such map — the trip list's, the overview map and the
//! track map — as functions their scripts start with, so they cannot drift
//! apart.
//!
//! Rust decides what each thing is called, which icon leads its name and
//! what choosing it reports — or that it opens nothing; the script only
//! finds what lies under the pointer and draws the hint and the menu
//! (ADR-0025).

use super::waypoints::with_waypoints;
use crate::config::map_picking::HINT_ROWS;
use crate::waypoint::UNNAMED;

/// Defines, for a map's script:
///
/// - `waypointPicks(map, markers)`: `drawWaypoints`' markers, each
///   reporting its page if Rust says it opens one.
/// - `pickLine(hit, line, opens)` and `pickWaypoint(map, marker, opens)`:
///   one thing that can lie under the pointer. A line lies there within
///   half the width it takes the pointer at — where a click on it hit it
///   before (US-104) — and a waypoint where its marker is drawn. `opens` is
///   what choosing it reports, or `null` for what opens nothing.
/// - `setPicking(map, items, report)`: what lies on `map` now, in the order
///   the page lists it, and where a choice goes. Called again on a redraw;
///   the listeners are put on the map once.
/// - `pickIcon(icon)`: an activity's icon as nodes, for a hint or a menu.
///
/// A mouse's hover names everything under it, the first `HINT_ROWS` and an
/// ellipsis for more. A click with a mouse opens the one thing there is to
/// open at once, or a menu of them; a tap always opens the menu, listing
/// everything under the finger with what opens nothing as plain text — on a
/// map there is no sign that a tap leads anywhere. The menu closes on a
/// click elsewhere, on Escape and when the map moves.
///
/// Everything named is put in as text nodes, never markup: the names come
/// from GPX files and Komoot titles.
const PICKING_JS: &str = r##"
    // A marker's half-size: its icon is 16px across (`waypointIcon`).
    const MARKER_REACH = 8;

    function pickIcon(icon) {
      if (!icon) return null;
      const ns = "http://www.w3.org/2000/svg";
      const svg = document.createElementNS(ns, "svg");
      svg.setAttribute("viewBox", "0 0 24 24");
      for (const shape of icon.paths) {
        const path = document.createElementNS(ns, "path");
        path.setAttribute("d", shape);
        path.setAttribute("fill", icon.color);
        svg.appendChild(path);
      }
      const box = document.createElement("span");
      box.className = "pick-icon";
      box.appendChild(svg);
      return box;
    }

    function pickLine(hit, line, opens) {
      return {
        name: line.name,
        icon: () => pickIcon(line.icon),
        opens,
        hits: (point) => {
          const closest = hit.closestLayerPoint(point);
          return closest !== null && closest.distance <= hit.options.weight / 2;
        },
        point: (on) => hit.fire(on ? "mouseover" : "mouseout"),
      };
    }

    function pickWaypoint(map, marker, opens) {
      const waypoint = marker.waypoint;
      return {
        name: waypoint.name || UNNAMED,
        icon: () => {
          const box = document.createElement("span");
          box.className = `pick-icon ${waypoint.icon ? "waypoint-symbol" : "waypoint-mark"}`;
          box.appendChild(waypointSvg(waypoint.icon));
          return box;
        },
        opens,
        hits: (point) => {
          const at = map.latLngToLayerPoint(marker.getLatLng());
          return Math.abs(point.x - at.x) <= MARKER_REACH && Math.abs(point.y - at.y) <= MARKER_REACH;
        },
        point: () => {},
      };
    }

    function waypointPicks(map, markers) {
      return markers.getLayers().map((marker) => {
        const waypoint = marker.waypoint;
        return pickWaypoint(map, marker, waypoint.opens ? { open_waypoint: waypoint.id } : null);
      });
    }

    function pickRow(item, tag) {
      const row = document.createElement(tag);
      row.className = "pick-row";
      const icon = item.icon();
      if (icon) row.appendChild(icon);
      const name = document.createElement("span");
      name.textContent = item.name;
      row.appendChild(name);
      return row;
    }

    function setPicking(map, items, report) {
      const picking = (map.picking ||= installPicking(map));
      picking.close();
      picking.items = items;
      picking.report = report;
    }

    function installPicking(map) {
      const picking = { items: [], report: () => {} };
      const hint = L.tooltip({ direction: "right", offset: [14, 0], className: "pick-hint" });
      const picker = L.popup({
        autoPan: false,
        closeButton: false,
        // Closed here, so a click that closes it opens nothing.
        closeOnClick: false,
        className: "pick-menu",
      });
      let pointed = null;
      const unpoint = () => {
        pointed?.point(false);
        pointed = null;
      };
      const closeMenu = () => {
        unpoint();
        if (map.hasLayer(picker)) map.closePopup(picker);
      };
      picking.hideHint = () => map.closeTooltip(hint);
      picking.close = () => {
        closeMenu();
        picking.hideHint();
      };

      // Whether the last press or move came from a mouse: a finger's tap
      // ends in a click too, and a phone has no hover to show the hint.
      let mouse = true;
      const container = map.getContainer();
      for (const kind of ["pointerdown", "pointermove"]) {
        container.addEventListener(kind, (event) => (mouse = event.pointerType === "mouse"), true);
      }
      const under = (event) => picking.items.filter((item) => item.hits(event.layerPoint));
      const choose = (item) => {
        closeMenu();
        picking.report(item.opens);
      };

      const openMenu = (latlng, items) => {
        const list = document.createElement("ul");
        list.className = "pick-list";
        for (const item of items) {
          const entry = document.createElement("li");
          if (item.opens) {
            const button = pickRow(item, "button");
            button.type = "button";
            button.addEventListener("click", () => choose(item));
            button.addEventListener("mouseenter", () => {
              unpoint();
              pointed = item;
              item.point(true);
            });
            button.addEventListener("mouseleave", unpoint);
            entry.appendChild(button);
          } else {
            entry.appendChild(pickRow(item, "span"));
          }
          list.appendChild(entry);
        }
        map.closeTooltip(hint);
        // Never taller than the map: a longer menu scrolls.
        picker.options.maxHeight = Math.max(map.getSize().y - 48, 64);
        picker.setLatLng(latlng).setContent(list).openOn(map);
      };

      map.on("mousemove", (event) => {
        const items = mouse && !map.hasLayer(picker) ? under(event) : [];
        if (items.length === 0) {
          map.closeTooltip(hint);
          return;
        }
        const rows = document.createElement("div");
        rows.className = "pick-rows";
        for (const item of items.slice(0, HINT_ROWS)) rows.appendChild(pickRow(item, "div"));
        if (items.length > HINT_ROWS) {
          const more = document.createElement("div");
          more.textContent = "…";
          rows.appendChild(more);
        }
        hint.setContent(rows).setLatLng(event.latlng);
        if (!map.hasLayer(hint)) map.openTooltip(hint);
      });
      map.on("mouseout", () => map.closeTooltip(hint));
      map.on("click", (event) => {
        if (map.hasLayer(picker)) {
          closeMenu();
          return;
        }
        const items = under(event);
        const choosable = items.filter((item) => item.opens);
        if (!mouse) {
          if (items.length > 0) openMenu(event.latlng, items);
        } else if (choosable.length === 1) {
          choose(choosable[0]);
        } else if (choosable.length > 1) {
          openMenu(event.latlng, choosable);
        }
      });
      map.on("movestart", closeMenu);
      const onKey = (event) => {
        if (event.key === "Escape") closeMenu();
      };
      document.addEventListener("keydown", onKey);
      map.on("unload", () => document.removeEventListener("keydown", onKey));
      return picking;
    }
"##;

/// `script`, able to call the picking functions — and `drawWaypoints`,
/// whose markers they pick from. How many the hint names and what an
/// unnamed waypoint is called are Rust's, handed over as constants.
pub(super) fn with_picking(script: &str) -> String {
    with_waypoints(&format!(
        "const HINT_ROWS = {HINT_ROWS};\nconst UNNAMED = {UNNAMED:?};\n{PICKING_JS}\n{script}"
    ))
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us104_a_script_that_picks_can_call_the_functions_first() {
        let script = with_picking("setPicking(map, [], report);");

        assert!(script.find("function setPicking") < script.find("setPicking(map, [], report)"));
        assert!(script.contains("function drawWaypoints"), "{script}");
    }

    #[test]
    fn us104_the_hint_names_as_many_as_rust_says_and_unnamed_waypoints_as_the_pages_do() {
        let script = with_picking("");

        assert!(script.contains(&format!("const HINT_ROWS = {HINT_ROWS};")));
        assert!(script.contains(r#"const UNNAMED = "Unnamed waypoint";"#));
    }

    #[test]
    fn us104_names_and_icons_go_in_as_nodes_never_as_markup() {
        // The names come from GPX files and Komoot titles.
        assert!(PICKING_JS.contains("name.textContent = item.name"));
        assert!(!PICKING_JS.contains("innerHTML"));
        assert!(PICKING_JS.contains("createElementNS"));
    }

    #[test]
    fn us104_a_menu_that_closes_on_a_click_opens_nothing_with_it() {
        // Leaflet's own closing on a click comes before the click; it would
        // leave the click free to open the menu anew.
        assert!(PICKING_JS.contains("closeOnClick: false"));
    }
}
