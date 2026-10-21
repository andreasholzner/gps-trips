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
use crate::config::map_picking::{HINT_ROWS, LINE_REACH_PX};
use crate::waypoint::UNNAMED;

/// Defines, for a map's script:
///
/// - `waypointPicks(map, markers)`: `drawWaypoints`' markers, each
///   reporting its page if Rust says it opens one.
/// - `pickLine(path, line, opens, point)` and `pickWaypoint(map, marker,
///   opens)`: one thing that can lie under the pointer. A line lies there
///   within `LINE_REACH` of it, whatever its width or style, and a waypoint
///   where its marker is drawn. `opens` is what choosing it reports, or
///   `null` for what opens nothing; `point(on)` makes a line stand out, or
///   no longer, as the map does it.
/// - `setPicking(map, items, report)`: what lies on `map` now, in the order
///   the page lists it, and where a choice goes. Called again on a redraw;
///   the listeners are put on the map once.
/// - `pickIcon(icon)`: an activity's icon as nodes, for a hint or a menu.
///
/// A mouse's hover names everything under it, the first `HINT_ROWS` and an
/// ellipsis for more, and points at the first of it — the same reach for
/// the highlight as for the hint and a click. A click with a mouse opens the one thing there is to
/// open at once, or a menu of them; a tap always opens the menu, listing
/// everything under the finger with what opens nothing as plain text — on a
/// map there is no sign that a tap leads anywhere. A line given an `href`
/// opens it in a new tab on a Ctrl- or ⌘-click, on the map and in the menu
/// alike (US-114). The menu closes on a
/// click elsewhere, on Escape and when the map moves. Both stay within the
/// map's frame, wrapping long names and scrolling what still does not fit.
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

    function pickLine(path, line, opens, point = () => {}) {
      return {
        name: line.name,
        icon: () => pickIcon(line.icon),
        opens,
        href: line.href ?? null,
        hits: (at) => {
          const closest = path.closestLayerPoint(at);
          return closest !== null && closest.distance <= LINE_REACH;
        },
        point,
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
        href: waypoint.href ?? null,
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
      const container = map.getContainer();

      // The hint and the menu are boxes of the map's own, not Leaflet's
      // tooltip and popup: those open on one side of the point whether or
      // not they fit, and the map clips them. These are kept within the
      // map's frame and never larger than it — a long name wraps, and a menu
      // that still does not fit scrolls.
      const EDGE = 8;
      const GAP = 14;
      const box = (className) => {
        const el = document.createElement("div");
        el.className = className;
        el.hidden = true;
        container.appendChild(el);
        return el;
      };
      const hint = box("pick-hint");
      const menu = box("pick-menu");
      // A click, a drag or a wheel in the menu is the menu's, not the map's.
      L.DomEvent.disableClickPropagation(menu);
      L.DomEvent.disableScrollPropagation(menu);
      const clamp = (value, low, high) => Math.max(low, Math.min(value, high));
      // Beside `at`, on whichever side it fits, and moved in from the edges.
      const place = (el, at) => {
        const size = map.getSize();
        el.style.maxWidth = `${size.x - 2 * EDGE}px`;
        el.style.maxHeight = `${size.y - 2 * EDGE}px`;
        el.style.left = "0px";
        el.style.top = "0px";
        el.hidden = false;
        const width = el.offsetWidth;
        const height = el.offsetHeight;
        const right = at.x + GAP;
        const x = right + width <= size.x - EDGE ? right : at.x - GAP - width;
        el.style.left = `${clamp(x, EDGE, size.x - EDGE - width)}px`;
        el.style.top = `${clamp(at.y - height / 2, EDGE, size.y - EDGE - height)}px`;
      };
      const menuOpen = () => !menu.hidden;

      // What a menu entry points at, and what the mouse points at on the map.
      let pointed = null;
      const unpoint = () => {
        pointed?.point(false);
        pointed = null;
      };
      let hovered = null;
      const hover = (item) => {
        container.classList.toggle("pick-here", item !== null);
        if (hovered === item) return;
        hovered?.point(false);
        hovered = item;
        item?.point(true);
      };
      const closeMenu = () => {
        unpoint();
        menu.hidden = true;
        menu.replaceChildren();
      };
      picking.hideHint = () => {
        hover(null);
        hint.hidden = true;
        hint.replaceChildren();
      };
      picking.close = () => {
        closeMenu();
        picking.hideHint();
      };

      // Whether the last press or move came from a mouse: a finger's tap
      // ends in a click too, and a phone has no hover to show the hint.
      let mouse = true;
      for (const kind of ["pointerdown", "pointermove"]) {
        container.addEventListener(kind, (event) => (mouse = event.pointerType === "mouse"), true);
      }
      const under = (event) => picking.items.filter((item) => item.hits(event.layerPoint));
      // A Ctrl- or ⌘-click opens a page with an address in a new tab, as on
      // a link (US-114) — from here, while the browser still counts it as
      // the click, so it is not taken for a popup.
      const choose = (item, event) => {
        closeMenu();
        if (item.href && (event.ctrlKey || event.metaKey)) {
          window.open(item.href, "_blank", "noopener");
        } else {
          picking.report(item.opens);
        }
      };

      const openMenu = (at, items) => {
        const list = document.createElement("ul");
        list.className = "pick-list";
        for (const item of items) {
          const entry = document.createElement("li");
          if (item.opens) {
            const button = pickRow(item, "button");
            button.type = "button";
            button.addEventListener("click", (event) => choose(item, event));
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
        picking.hideHint();
        menu.replaceChildren(list);
        place(menu, at);
      };

      map.on("mousemove", (event) => {
        const items = mouse && !menuOpen() ? under(event) : [];
        if (items.length === 0) {
          picking.hideHint();
          return;
        }
        hover(items[0]);
        const rows = document.createElement("div");
        rows.className = "pick-rows";
        for (const item of items.slice(0, HINT_ROWS)) rows.appendChild(pickRow(item, "div"));
        if (items.length > HINT_ROWS) {
          const more = document.createElement("div");
          more.textContent = "…";
          rows.appendChild(more);
        }
        hint.replaceChildren(rows);
        place(hint, event.containerPoint);
      });
      map.on("mouseout", () => picking.hideHint());
      map.on("click", (event) => {
        // A click that closes the menu opens nothing with it.
        if (menuOpen()) {
          closeMenu();
          return;
        }
        const items = under(event);
        const choosable = items.filter((item) => item.opens);
        if (!mouse) {
          if (items.length > 0) openMenu(event.containerPoint, items);
        } else if (choosable.length === 1) {
          choose(choosable[0], event.originalEvent);
        } else if (choosable.length > 1) {
          openMenu(event.containerPoint, choosable);
        }
      });
      map.on("movestart", closeMenu);
      const onKey = (event) => {
        if (event.key === "Escape") closeMenu();
      };
      // A press anywhere off the map closes the menu too.
      const onPress = (event) => {
        if (menuOpen() && !container.contains(event.target)) closeMenu();
      };
      document.addEventListener("keydown", onKey);
      document.addEventListener("pointerdown", onPress);
      map.on("unload", () => {
        document.removeEventListener("keydown", onKey);
        document.removeEventListener("pointerdown", onPress);
        hint.remove();
        menu.remove();
      });
      return picking;
    }
"##;

/// `script`, able to call the picking functions — and `drawWaypoints`,
/// whose markers they pick from. How many the hint names and what an
/// unnamed waypoint is called are Rust's, handed over as constants.
pub(super) fn with_picking(script: &str) -> String {
    with_waypoints(&format!(
        "const HINT_ROWS = {HINT_ROWS};\nconst LINE_REACH = {LINE_REACH_PX:?};\nconst UNNAMED = {UNNAMED:?};\n{PICKING_JS}\n{script}"
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
    fn us104_every_line_is_as_easy_to_point_at_whatever_its_width_or_style() {
        // A solid recorded line is drawn thinner than a dashed plan is hit.
        let script = with_picking("");

        assert!(script.contains(&format!("const LINE_REACH = {LINE_REACH_PX:?};")));
        assert!(PICKING_JS.contains("closest.distance <= LINE_REACH"));
        assert!(!PICKING_JS.contains("options.weight"));
    }

    #[test]
    fn us104_names_and_icons_go_in_as_nodes_never_as_markup() {
        // The names come from GPX files and Komoot titles.
        assert!(PICKING_JS.contains("name.textContent = item.name"));
        assert!(!PICKING_JS.contains("innerHTML"));
        assert!(PICKING_JS.contains("createElementNS"));
    }

    #[test]
    fn us104_the_hint_and_the_menu_are_kept_within_the_map() {
        // Leaflet's tooltip and popup open on one side whether or not they
        // fit, and the map clips them.
        assert!(!PICKING_JS.contains("L.tooltip"));
        assert!(!PICKING_JS.contains("L.popup"));
        assert!(PICKING_JS.contains("el.style.maxWidth = `${size.x - 2 * EDGE}px`"));
        assert!(PICKING_JS.contains("el.style.maxHeight = `${size.y - 2 * EDGE}px`"));
    }

    #[test]
    fn us114_a_ctrl_or_cmd_click_opens_a_page_in_a_new_tab_from_the_click_itself() {
        // Opened while the click is still being handled, not after a round
        // trip through Rust, so the browser does not take it for a popup.
        assert!(PICKING_JS.contains("href: line.href ?? null"));
        assert!(PICKING_JS.contains("href: waypoint.href ?? null"));
        assert!(PICKING_JS.contains("event.ctrlKey || event.metaKey"));
        assert!(PICKING_JS.contains(r#"window.open(item.href, "_blank", "noopener")"#));
        assert!(PICKING_JS.contains("choose(choosable[0], event.originalEvent)"));
        assert!(PICKING_JS.contains("(event) => choose(item, event)"));
    }

    #[test]
    fn us104_a_click_in_the_menu_is_the_menus_not_the_maps() {
        assert!(PICKING_JS.contains("disableClickPropagation(menu)"));
        assert!(PICKING_JS.contains("disableScrollPropagation(menu)"));
    }
}
