# Trip Archive — Requirements & User Stories

## Purpose

A personal, **single-user, self-hosted** archive for organizing trips (GPS tracks + photos)
and browsing them on a map. It replaces **komoot's organization features only**.

- **In scope:** importing GPS tracks, attaching photos, placing photos on a map, browsing
  trips with stats, reliving a trip (map + elevation + gallery).
- **Out of scope (stays in komoot):** recording tracks, route planning, discovery/highlights,
  social features.

The overarching driver: *"self-host the whole thing, so I own my data."* and a learning opportunity for rust and geo data

## Actors

- **The owner** — the single user who owns and runs the instance, and the only one who signs in.
- **A share's recipient** — anyone holding a share link (US-53). They have no account and need no
  password, and they see only the trips the share names, read-only — their stats, map, elevation
  profile, photos and GPX — until the share expires or the owner stops it (US-69).

## Scheduling

Tracks progress on work that spans several dependent stories: each step names the stories it
covers and why it comes where it does. Only what is not yet done is listed: a step leaves this
table when its stories ship. Each story's own state is recorded with the story and in the
[story index](#story-index) below.

| Step | Stories | Why it sits here |
|:-----|---------|------------------|

## Story index

**State:** ✅ done · 🚧 in progress · 📋 planned · ⏳ backlog (not planned but not descoped) · 🚫 de-scoped

> ✅ means the capability works today. Where a story has a UI, that UI is the Dioxus SPA:
> US-44 moved the last screen the server-rendered proof of concept still owned, and
> [ADR-0024](./adr/0024-dioxus-ui-web-and-android.md)'s legacy UI is gone. No story here was
> re-opened or duplicated by that move.

Stories are grouped by the part of the archive they are about; a story's ID never changes.

| ID                                                            | State  | Story                                        | Area                                                                |
|---------------------------------------------------------------|--------|----------------------------------------------|---------------------------------------------------------------------|
| [US-1](#us-1--import-a-gpx-file)                              | ✅     | Import a GPX file                            | [Import](#import)                                                   |
| [US-2](#us-2--attach-photos-to-a-trip)                        | ✅     | Attach photos to a trip                      | [Import](#import)                                                   |
| [US-3](#us-3--place-geotagged-photos)                         | ✅     | Place geotagged photos                       | [Photos](#photos)                                                   |
| [US-4](#us-4--place-photos-by-timestamp)                      | ✅     | Place photos by timestamp                    | [Photos](#photos)                                                   |
| [US-5](#us-5--generate-thumbnails)                            | ✅     | Generate thumbnails                          | [Photos](#photos)                                                   |
| [US-6](#us-6--trip-list-with-summary-stats)                   | ✅     | Trip list with summary stats                 | [Trip list & map](#trip-list--map)                                  |
| [US-7](#us-7--trip-detail-page)                               | ✅     | Trip detail page                             | [Trip page](#trip-page)                                             |
| [US-8](#us-8--computed-trip-stats)                            | ✅     | Computed trip stats                          | [Trip page](#trip-page)                                             |
| [US-9](#us-9--delete-a-trip)                                  | ✅     | Delete a trip                                | [Trip page](#trip-page)                                             |
| [US-10](#us-10--self-hosting)                                 | ✅     | Self-hosting                                 | [Hosting & backup](#hosting--backup)                                |
| [US-11](#us-11--activity-type-at-import)                      | ✅     | Activity type at import                      | [Import](#import)                                                   |
| [US-12](#us-12--suggested-trip-name-at-import)                | ✅     | Suggested trip name at import                | [Import](#import)                                                   |
| [US-13](#us-13--filter-the-trip-list)                         | ✅     | Filter the trip list                         | [Trip list & map](#trip-list--map)                                  |
| [US-14](#us-14--filter-by-region)                             | ✅     | Filter by region                             | [Trip list & map](#trip-list--map)                                  |
| [US-15](#us-15--edit-name-and-activity-type)                  | ✅     | Edit name and activity type                  | [Trip page](#trip-page)                                             |
| [US-16](#us-16--native-android-app)                           | ⏳     | Native Android app                           | [App & clients](#app--clients)                                      |
| [US-17](#us-17--photos-on-owncloud)                           | ⏳     | Photos on ownCloud                           | [Hosting & backup](#hosting--backup)                                |
| [US-18](#us-18--import-from-garmin-connect)                   | ⏳     | Import from Garmin Connect                   | [Import](#import)                                                   |
| [US-19](#us-19--password-protection)                          | ✅     | Password protection                          | [Access & security](#access--security)                              |
| [US-20](#us-20--push-edits-to-komoot)                         | ✅     | Push edits to Komoot                         | [Komoot sync](#komoot-sync)                                         |
| [US-21](#us-21--download-the-original-gpx)                    | ✅     | Download the original GPX                    | [Trip page](#trip-page)                                             |
| [US-22](#us-22--sync-now)                                     | ✅     | Sync now                                     | [Komoot sync](#komoot-sync)                                         |
| [US-23](#us-23--komoot-backfill)                              | ✅     | Komoot backfill                              | [Komoot sync](#komoot-sync)                                         |
| [US-24](#us-24--delete-on-komoot-too)                         | ✅     | Delete on Komoot too                         | [Komoot sync](#komoot-sync)                                         |
| [US-25](#us-25--stop-a-sync-at-the-first-failure)             | ✅     | Stop a sync at the first failure             | [Komoot sync](#komoot-sync)                                         |
| [US-26](#us-26--block-edits-during-a-sync)                    | ✅     | Block edits during a sync                    | [Komoot sync](#komoot-sync)                                         |
| [US-27](#us-27--komoot-integration-check)                     | ✅     | Komoot integration check                     | [Komoot sync](#komoot-sync)                                         |
| [US-28](#us-28--upload-photos-to-komoot)                      | 🚫     | Upload photos to Komoot                      | [Komoot sync](#komoot-sync)                                         |
| [US-29](#us-29--planned-trips-from-komoot)                    | ✅     | Planned trips from Komoot                    | [Komoot sync](#komoot-sync)                                         |
| [US-30](#us-30--place-photos-manually)                        | ✅     | Place photos manually                        | [Photos](#photos)                                                   |
| [US-31](#us-31--import-as-recorded-or-planned)                | ✅     | Import as recorded or planned                | [Import](#import)                                                   |
| [US-32](#us-32--recorded-and-planned-tabs)                    | ✅     | Recorded and planned tabs                    | [Trip list & map](#trip-list--map)                                  |
| [US-33](#us-33--tag-trips)                                    | ✅     | Tag trips                                    | [Tags](#tags)                                                       |
| [US-34](#us-34--tag-many-trips-at-once)                       | ✅     | Tag many trips at once                       | [Tags](#tags)                                                       |
| [US-35](#us-35--komoot-privacy-status)                        | ✅     | Komoot privacy status                        | [Komoot sync](#komoot-sync)                                         |
| [US-36](#us-36--qmapshack-export)                             | ✅     | QMapShack export                             | [QMapShack export](#qmapshack-export)                               |
| [US-37](#us-37--re-runnable-qmapshack-export)                 | ✅     | Re-runnable QMapShack export                 | [QMapShack export](#qmapshack-export)                               |
| [US-38](#us-38--filter-by-tag)                                | ✅     | Filter by tag                                | [Tags](#tags)                                                       |
| [US-39](#us-39--validate-the-qmapshack-config)                | ✅     | Validate the QMapShack config                | [QMapShack export](#qmapshack-export)                               |
| [US-40](#us-40--consistent-backup)                            | ✅     | Consistent backup                            | [Hosting & backup](#hosting--backup)                                |
| [US-41](#us-41--trip-list-in-the-spa)                         | ✅     | Trip list in the SPA                         | [UI migration to Dioxus (history)](#ui-migration-to-dioxus-history) |
| [US-42](#us-42--trip-page-in-the-spa)                         | ✅     | Trip page in the SPA                         | [UI migration to Dioxus (history)](#ui-migration-to-dioxus-history) |
| [US-43](#us-43--import-in-the-spa)                            | ✅     | Import in the SPA                            | [UI migration to Dioxus (history)](#ui-migration-to-dioxus-history) |
| [US-44](#us-44--sync-screen-in-the-spa)                       | ✅     | Sync screen in the SPA                       | [UI migration to Dioxus (history)](#ui-migration-to-dioxus-history) |
| [US-45](#us-45--one-command-deploy)                           | ✅     | One-command deploy                           | [Hosting & backup](#hosting--backup)                                |
| [US-46](#us-46--persistent-storage)                           | ✅     | Persistent storage                           | [Hosting & backup](#hosting--backup)                                |
| [US-47](#us-47--scale-to-zero)                                | ✅     | Scale to zero                                | [Hosting & backup](#hosting--backup)                                |
| [US-48](#us-48--secrets-outside-the-repo)                     | ✅     | Secrets outside the repo                     | [Hosting & backup](#hosting--backup)                                |
| [US-49](#us-49--https-at-a-stable-address)                    | ✅     | HTTPS at a stable address                    | [Hosting & backup](#hosting--backup)                                |
| [US-50](#us-50--fill-the-deployed-instance)                   | ✅     | Fill the deployed instance                   | [Hosting & backup](#hosting--backup)                                |
| [US-51](#us-51--export-and-backfill-against-the-cloud)        | ✅     | Export and backfill against the cloud        | [Hosting & backup](#hosting--backup)                                |
| [US-52](#us-52--region-filter-in-the-spa)                     | ✅     | Region filter in the SPA                     | [UI migration to Dioxus (history)](#ui-migration-to-dioxus-history) |
| [US-53](#us-53--share-trips-by-link)                          | ✅     | Share trips by link                          | [Sharing](#sharing)                                                 |
| [US-54](#us-54--downscale-photos-on-import)                   | ✅     | Downscale photos on import                   | [Photos](#photos)                                                   |
| [US-55](#us-55--session-tokens-resist-offline-guessing)       | ✅     | Session tokens resist offline guessing       | [Access & security](#access--security)                              |
| [US-56](#us-56--one-integration-test-binary)                  | ✅     | One integration-test binary                  | [Development](#development)                                         |
| [US-57](#us-57--group-photos-taken-at-one-place)              | ✅     | Group photos taken at one place              | [Photos](#photos)                                                   |
| [US-58](#us-58--map-zoom-controls)                            | ✅     | Map zoom controls                            | [Maps](#maps)                                                       |
| [US-59](#us-59--elevation-profile-hover)                      | ✅     | Elevation profile hover                      | [Trip page](#trip-page)                                             |
| [US-60](#us-60--header-menu)                                  | ✅     | Header menu                                  | [App & clients](#app--clients)                                      |
| [US-61](#us-61--trip-counts-and-compact-filters)              | ✅     | Trip counts and compact filters              | [Trip list & map](#trip-list--map)                                  |
| [US-62](#us-62--trip-page-at-a-glance)                        | ✅     | Trip page at a glance                        | [Trip page](#trip-page)                                             |
| [US-63](#us-63--trip-list-map-paging-and-phone-layout)        | ✅     | Trip-list map, paging and phone layout       | [Trip list & map](#trip-list--map)                                  |
| [US-64](#us-64--photo-placement-across-timezone-borders)      | ✅     | Photo placement across timezone borders      | [Photos](#photos)                                                   |
| [US-65](#us-65--region-selection-by-touch)                    | ✅     | Region selection by touch                    | [Trip list & map](#trip-list--map)                                  |
| [US-66](#us-66--find-trips-needing-a-name-or-photo-placement) | ✅     | Find trips needing a name or photo placement | [Trip list & map](#trip-list--map)                                  |
| [US-67](#us-67--install-on-the-android-home-screen)           | ✅     | Install on the Android home screen           | [App & clients](#app--clients)                                      |
| [US-68](#us-68--show-the-running-version)                     | ✅     | Show the running version                     | [App & clients](#app--clients)                                      |
| [US-69](#us-69--manage-shares)                                | ✅     | Manage shares                                | [Sharing](#sharing)                                                 |
| [US-70](#us-70--access-log)                                   | ✅     | Access log                                   | [Sharing](#sharing)                                                 |
| [US-71](#us-71--bound-the-access-log)                         | ⏳     | Bound the access log                         | [Sharing](#sharing)                                                 |
| [US-72](#us-72--tell-shared-trips-apart)                      | ✅     | Tell shared trips apart                      | [Sharing](#sharing)                                                 |
| [US-73](#us-73--trip-lines-when-zoomed-in)                    | ✅     | Trip lines when zoomed in                    | [Trip list & map](#trip-list--map)                                  |
| [US-74](#us-74--place-based-name-suggestion)                  | 📋     | Place-based name suggestion                  | [Import](#import)                                                   |
| [US-75](#us-75--activity-colors-on-maps)                      | ✅     | Activity colors on maps                      | [Maps](#maps)                                                       |
| [US-76](#us-76--activity-type-suggestion)                     | 📋     | Activity type suggestion                     | [Import](#import)                                                   |
| [US-77](#us-77--statistics)                                   | ✅     | Statistics                                   | [Statistics](#statistics)                                           |
| [US-78](#us-78--tag-summary)                                  | ✅     | Tag summary                                  | [Statistics](#statistics)                                           |
| [US-79](#us-79--speed-and-incline-on-the-elevation-profile)   | ✅     | Speed and incline on the elevation profile   | [Trip page](#trip-page)                                             |
| [US-80](#us-80--average-speed)                                | ✅     | Average speed                                | [Trip page](#trip-page)                                             |
| [US-81](#us-81--climbing-rate)                                | ✅     | Climbing rate                                | [Trip page](#trip-page)                                             |
| [US-82](#us-82--share-a-summary)                              | ✅     | Share a summary                              | [Sharing](#sharing)                                                 |
| [US-83](#us-83--tags-page)                                    | ✅     | Tags page                                    | [Tags](#tags)                                                       |

### Maintaining this file

- A new story takes the next free ID — one above the highest in the table — and goes under the
  area it is about, in ID order, as `### US-N — Title`. It also gets a row in the table above.
- When a story's state changes, update the state icon both in the story and in the table.
- A story keeps its title once given: the table links to the story by its heading.
- A story whose criteria are not yet worked out carries **Notes** instead of **Acceptance
  criteria**; the label changes when the story is refined.

## Import

### US-1 — Import a GPX file

**Done ✅** — As the owner, I import a **GPX file** (exported from komoot or my recorder) so a trip
is archived outside komoot.

**Acceptance criteria:** Uploading a valid GPX creates a trip and takes the owner to its detail
view. Invalid/empty GPX is rejected with a clear error.

**Decisions:** US-1/US-8 → ADR-0004 (import handler), ADR-0003 (track storage)

### US-2 — Attach photos to a trip

**Done ✅** — As the owner, I **attach photos** to a trip so they are stored alongside the track.

**Acceptance criteria:** Photos uploaded with the import are stored and associated with the trip.
Photos can be added to a trip both during the gpx import and at a later time.

**Decisions:** US-2 → ADR-0004 (import + add-photos-later via the same pipeline)

### US-11 — Activity type at import

**Done ✅** — As the owner, when importing a **GPX file** I choose an activity type for the trip
(e.g. cycling, hiking, ...).

**Acceptance criteria:** The activity type is stored in the database and shown on the list over all
trips and on the trip detail page.

**Decisions:** US-11/US-15 → ADR-0008 (write API: import metadata + edit endpoint)

### US-12 — Suggested trip name at import

**Done ✅** — As the owner, when importing a **GPX file** I choose a name for the trip, with the name
field pre-filled with a suggested `YYYY-mm-dd` date prefix once the GPX is uploaded.

**Acceptance criteria:** The import is two requests, because the prefix has to be in the field
*while* the owner types and the date only exists once the track is read. `POST /api/import/staged`
parses the GPX and parks what it derived in `import_staging`, creating no trip; the confirm step
promotes that parse through the same insert path, so the file is parsed once. The suggestion leads
with the date whenever the track has one — `2024-06-01 Oslo Hills Walk`, or the bare `2024-06-01 `
to type after. It is only a suggestion: `resolve_name`'s precedence for a name left empty (the GPX
track's name, then `YYYY-MM-DD Imported Trip`) is unchanged. A parse the owner abandons is handed
back when they pick a different file, and swept after 24 h otherwise — navigating away sends
nothing, so the sweeper is what bounds the table. One that has expired is re-sent from the bytes the
screen still holds rather than sending them back to the file picker.

### US-18 — Import from Garmin Connect

**Planned 📋** — As the owner, I import trips from **Garmin Connect**.

**Notes:** Plugs into the same import pipeline as an alternate ingestion source.

### US-31 — Import as recorded or planned

**Done ✅** — as the owner, when importing a **GPX file** I can decide whether the file is imported
as a recorded or as a planned tour.

**Acceptance criteria:** The import view lets the owner choose Recorded or Planned, defaulting to
Recorded; the chosen `trip_kind` is stored on the trip and determines which list tab (US-32) it
appears under. An unrecognized value is rejected with 400. Komoot sync/backfill are unaffected
(still always `recorded`; US-29 will let Komoot-sourced planned trips in later).

### US-74 — Place-based name suggestion

**Planned 📋** — As the owner, when I import a trip or edit its name, I am offered a name that says
where the trip went, behind its date.

**Acceptance criteria:**

Extends US-12, whose suggestion puts the GPX track's name behind the date (`2024-06-01 Oslo Hills
Walk`); a GPX name is often a device default or empty, so the date is all the suggestion says. **The
part behind the date becomes the trip's places, read from its track** in the offline place-name
database of [ADR-0027](./adr/0027-offline-place-name-database.md). It covers Norway, Sweden,
Finland, Germany, Austria, Switzerland, northern Italy, Slovenia and the French Alps: OpenStreetMap
everywhere, supplemented by the national name registers of Norway (Kartverket), Sweden
(Lantmäteriet) and Finland (Maanmittauslaitos). **A trip that ends somewhere other than where it
started** reads `A - B`, A naming its start and B its end: `2019-09-07 Rysstad - Kilefjorden`. An
end is named after the most important place within reach of it rather than the nearest — a town
before a hamlet a few hundred metres closer, each kind of place with its own reach. A lake counts by
its shoreline, not its centre, so a trip ending on its shore is named after it. A hut or campsite
right at an end is where the trip stopped and gives its name — the name of the place it is named
after, if any (`Fjordbotn Camping` gives `Fjordbotn`). If neither end has a name in reach, the
trip's main places stand in their track order: `2024-07-27 Leirholtinden - Storsteinnestinden -
884`. **A round trip** — one that ends within a distance of its start set in `config.rs` — reads `A:
D1 - D2 - D3`, A naming its start as above and D1… its main places in the order the track passes
them. A round trip goes somewhere, so the place nearest its turning point weighs most (`2026-06-27
Tromsø: Kvaløyvågen`), and one main place far more important than the rest is named alone
(`2024-07-31 Langryggen: Hamperokken`). **Main places** are summits, passes, huts, lakes, bays,
glaciers and settlements the track passes close to, ranked by what the sources say about them — a
summit's prominence, a lake's area, a register's importance, a settlement's kind; farms and minor
localities never count. How many at most, the reaches, the round-trip distance and the dominance
ratio are set in `config.rs`.

**Names:** a summit known only by its height keeps it (`884`); a name given in several languages
contributes the one in the register's first language; a place named by two sources counts once. If
no place is found, or the place-name database is missing, the suggestion falls back to US-12's: the
date and the GPX name. A trip without timestamps (a planned one) has no date, so its places stand
alone. The suggestion is computed on the server, from the trip it already holds.

**Import:** the staged parse (US-12) returns this suggestion in the name field, where the GPX name
used to go. It stays a suggestion: `resolve_name`'s fallback for a name left empty is unchanged.
**Editing a name** (US-15): the edit form offers the suggestion for the trip as it is stored,
computed when the form opens, next to the name field; one click puts it in the field, and nothing is
replaced without that click — the owner's own name is never overwritten. This is what makes US-66's
"no proper name" trips quick to fix.

**Attribution:** the credits the sources require are shown in the app. **Tested** against real
tracks the owner provides for the purpose, each with the suggestion the owner accepted, over a
place-name fixture cut from the real database around them (ADR-0012).

**Not this story:** weighting kinds of places by the trip's activity type — a ski tour across a
plateau is not about the hut it passed at the end.

**Decisions:** US-74 → the ADR on the source of place names (to be proposed), ADR-0019 (the
precedent of an offline geographic lookup), ADR-0012 (the suggestion rule is unit-tested)

### US-76 — Activity type suggestion

**Planned 📋** — As the owner, when I import a trip or edit its activity type, I am offered an
activity type that fits the track, so I rarely have to pick one myself.

**Acceptance criteria:**

Extends US-11 and US-15. The suggestion is computed on the server from the stored or staged track,
by heuristics over what the track runs on, how steep it is and how fast it went; every threshold
below is set in `config.rs`. **What the track runs on** is read offline from OpenStreetMap — water,
roads and tracks, paths, and prepared ski trails — so, like US-74, no coordinate leaves the archive
(an extension of [ADR-0027](./adr/0027-offline-place-name-database.md)'s database, or a sibling of
it — to be proposed). **The rules,** first match wins: a winter trip — one whose start date falls in
the winter months set in `config.rs` — is told apart by its average moving speed: the faster ones →
*Cross-country skiing*, the slower ones → *Ski touring*; it comes first because a ski trip across
frozen lakes is not a kayak trip. A track mostly on water → *Kayaking*. A track mostly on roads or
good paths (tracks, cycleways, gravel) is told apart by its average moving speed: faster →
*Cycling*, slower → *Bikepacking*. A track mostly off roads, on small paths or none → *Hiking*, or
*Mountaineering* when it is steeper, judged by the share of the track above a gradient.

**Never suggested:** *Snowshoeing*, which the owner rarely does, and *Unknown* — a track the rules
cannot place (no recognisable majority, or the OSM data missing) gets no suggestion, and the field
stays as it would be without this story. A trip without timestamps (a planned one) has no speed:
where a rule needs one, it falls back to *Cycling* and *Ski touring*.

**Import:** the staged parse (US-12) returns the suggestion, and the activity selector starts on it;
the owner can still pick any activity, Snowshoeing included. **Editing** (US-15): the edit form
shows the suggestion for the trip as stored next to the activity selector; one click selects it, and
nothing changes without that click. **Tested** with unit tests over the rules, and against real
tracks of each activity the owner provides, with the activity the owner gave them (ADR-0012).

**Not this story:** feeding the suggested activity into US-74's name ranking.

**Decisions:** US-76 → ADR-0018 (the suggestion is one of the closed set of activities), the ADR on
the offline OSM way and water data (to be proposed; extends ADR-0027), ADR-0012 (the rules are
unit-tested)

## Komoot sync

### US-20 — Push edits to Komoot

**Done ✅** — As the owner, my edits to a trip's name and activity type are synced back to Komoot
when the trip originated there, so the archive and Komoot don't diverge.

**Acceptance criteria:** Editing a Komoot-sourced trip marks its link row `edit_pending` in the same
transaction as the edit. The next "Sync now" push phase calls Komoot's update-tour API with the
current name/activity_type and clears the flag on success. Trips not sourced from Komoot are
unaffected.

**Decisions:** US-20/22/23/24/25/26/27/29 → ADR-0021 (Komoot client, sync now, backfill, integration
check, planned-trip pull)

### US-22 — Sync now

**Done ✅** — As the owner, I trigger "Sync now" so newly recorded trips (via my Garmin GPS unit or
the Komoot app) are ingested without a manual GPX export/upload step.

**Acceptance criteria:** "Sync now" lists Komoot tours not yet in `trip_komoot_link` on a review
screen, none of them ticked, so the owner opts in per tour: a sync pulls whole tours with their
photos, and that is a decision rather than what a stray press of the button does (a full pull is one
click on the select-all box; the historical bulk import is US-23's own binary). This corrects the
original wording, which said "pre-checked" — the page never was, and US-44 kept the built behaviour
and fixed the sentence; each selected tour's GPX + photos are imported through the existing pipeline
and its link row inserted in the same transaction as the import. Already-linked tours are skipped
(anti-join dedup), and a failure halts the run before later selected tours are attempted.

**Decisions:** US-20/22/23/24/25/26/27/29 → ADR-0021 (Komoot client, sync now, backfill, integration
check, planned-trip pull)

### US-23 — Komoot backfill

**Done ✅** — As the owner, I run a one-off backfill so my historical trips already in Komoot
(including photos) end up in the archive without hand-importing each one.

**Acceptance criteria:** A separate CLI binary (`komoot_backfill`) imports every not-yet-linked
Komoot tour, incl. photos, through the existing pipeline. Re-running after an interruption is safe
(dedup via `komoot_tour_id`). No web UI interaction required. An `--interactive` flag asks for
confirmation before each Komoot request (mainly for testing against the real API). A `--limit N`
flag caps the number of tours synced in the run (mainly for testing and for bounding load on
Komoot); omitting it syncs all not-yet-linked tours.

**Decisions:** US-20/22/23/24/25/26/27/29 → ADR-0021 (Komoot client, sync now, backfill, integration
check, planned-trip pull)

### US-24 — Delete on Komoot too

**Done ✅** — As the owner, deleting a Komoot-sourced trip in the archive also deletes it on Komoot,
so I don't have to delete it twice.

**Acceptance criteria:** Deleting a Komoot-linked trip removes it from the archive immediately and
marks its link row `delete_pending`. The next "Sync now" push phase calls Komoot's delete-tour API
and removes the link row only on success.

**Decisions:**

- US-20/22/23/24/25/26/27/29 → ADR-0021 (Komoot client, sync now, backfill, integration check,
  planned-trip pull)
- US-24 → also ADR-0021's extension of US-9's delete flow (`delete_pending` / orphaned link row)

### US-25 — Stop a sync at the first failure

**Done ✅** — As the owner, a failed sync stops immediately and tells me what failed, so I don't
hammer Komoot with a string of doomed requests or lose track of what still needs attention.

**Acceptance criteria:** The first failed Komoot call in either the push or pull phase halts the
sync without attempting further items; a visible error names the specific trip/tour that failed.

**Decisions:** US-20/22/23/24/25/26/27/29 → ADR-0021 (Komoot client, sync now, backfill, integration
check, planned-trip pull)

### US-26 — Block edits during a sync

**Done ✅** — As the owner, editing or deleting a trip is blocked while a sync is running, so my
change can't race the sync's read of pending state.

**Acceptance criteria:** `PATCH`/`DELETE` requests made while a sync is in flight are rejected with
`409`. Only one sync runs at a time (single in-process flag).

**Decisions:** US-20/22/23/24/25/26/27/29 → ADR-0021 (Komoot client, sync now, backfill, integration
check, planned-trip pull)

### US-27 — Komoot integration check

**Done ✅** — As the owner, I can run a lightweight check to confirm the Komoot integration still
works, so I notice a break before it derails a real sync.

**Acceptance criteria:** A separate CLI binary (`komoot_check`) logs in and makes one cheap Komoot
call (e.g. list tours), reporting success or failure. No DB, `BlobStore`, or import pipeline
involved.

**Decisions:** US-20/22/23/24/25/26/27/29 → ADR-0021 (Komoot client, sync now, backfill, integration
check, planned-trip pull)

### US-28 — Upload photos to Komoot

**De-scoped 🚫** — As the owner, photos I attach to a trip in the archive are also uploaded to the
matching Komoot tour, so the two stay visually in sync.

**Reason:** No clear value, Komoot is no longer the main (or owning) platform, also difficult to
implement, as there is no easy Komoot-Api for uploading photos.

### US-29 — Planned trips from Komoot

**Done ✅** — As the owner, my planned (not-yet-recorded) trips in Komoot show up in the archive too,
so I can see upcoming plans alongside past ones.

**Acceptance criteria:** Komoot's planned routes (`type=tour_planned`) are pulled through the same
import pipeline and dedup as recorded tours and stored with `trip_kind = planned`, so they appear on
the list's Planned tab (US-32). Selectable per-tour on the "Sync now" review page (labeled by kind)
and bulk-importable via `komoot_backfill --planned`. Not read-only, but a planned route's `sport` is
never pushed back to Komoot, to avoid disturbing its route planning (ADR-0021).

**Decisions:** US-20/22/23/24/25/26/27/29 → ADR-0021 (Komoot client, sync now, backfill, integration
check, planned-trip pull)

### US-35 — Komoot privacy status

**Done ✅** — As the owner, for trips that have a linked Komoot trip, I can control the
privacy_status of the linked Komoot trip from the archive.

**Acceptance criteria:** The trip list shows a Privacy column (a dash for trips that never came from
Komoot); the detail view's edit form offers a Private/Public picker, rendered only for a linked
trip. The chosen value is stored on `trip_komoot_link` and pushed to Komoot by the next "Sync now"
push phase, like name/activity_type edits (US-20). A privacy Komoot reports that the archive can't
map is shown as "Unknown" and never pushed back; a privacy edit on an unlinked trip, or an
unrecognized value, 400s (ADR-0021).

### Background: determining the import pipeline

#### Status quo

- Main recording device: Garmin GPS unit
  - unit is integrated with Garmin Connect
  - Komoot integration with Garmin Connect
  - Edit name and add photos in Komoot
- Also: record directly via Komoot app on mobile
  - (no Garmin Connect involvement)
  - Edit name and add photos in Komoot app after recording
- Most my recorded tracks exist in Komoot
- Archive of old recorded tracks (pre-Komoot) in gpx format
- Archive of trip suggestions (e.g. from guide books) in gpx format
- A couple of planned trips in Komoot

#### Must have features

- All future recorded trips (both via Garmin GPS unit and Komoot mobile app) are ingested.
- Bulk-import of historical trips already in Komoot (incl. photos).
- Edits of trips (name and activity_type) are synced back to Komoot.

#### Nice to have features

- Attached photos are also uploaded to the corresponding trip in Komoot.
- Batch-import of old recorded tracks in gpx format, single run
- One-way sync for planned trips from Komoot

## Trip list & map

### US-6 — Trip list with summary stats

**Done ✅** — As the owner, I **browse a list of all trips** with summary stats so I can scan my
history.

**Acceptance criteria:** List shows each trip's name, date, distance, ascent, and duration; loads
without reading track geometry.

### US-13 — Filter the trip list

**Done ✅** — As the owner, I can filter the list of my trips by activity type, date interval,
distance and free search of the name.

**Acceptance criteria:** List shows only trips matching the selected filter criteria.

**Decisions:** US-13/US-14 → ADR-0011 (filtering, search & geographic queries on SQLite)

### US-14 — Filter by region

**Done ✅** — As the owner, I can filter the list of my trips by geographic region by selecting an
area in a map.

**Acceptance criteria:** The trip list's filters hold a collapsed "Region" map (Europe by default),
on which "Select area" lets the owner drag out a rectangle; only trips whose stored bounding box
overlaps it are listed. The rectangle travels as `bbox=minLon,minLat,maxLon,maxLat` (`GET
/api/trips`, ADR-0008), survives a Recorded/Planned tab switch, and is restored onto the map when
the list is loaded again. Matching is bbox-overlap on the `trip` columns, no PostGIS (ADR-0011) —
deliberately coarse, so a trip whose box overlaps the region but whose track never enters it is an
accepted false positive. A malformed bbox — not four numbers, out of coordinate range, or reversed
on either axis (i.e. a rectangle crossing the antimeridian, unsupported in v1) — 400s; a blank one
means "no region selected".

**Decisions:** US-13/US-14 → ADR-0011 (filtering, search & geographic queries on SQLite)

### US-32 — Recorded and planned tabs

**Done ✅** — As the owner, when I list all trips, planned and recorded trips are list separately.

**Acceptance criteria:** The trip list separates Recorded and Planned into two tabs, defaulting to
Recorded; switching tabs preserves any active filters (US-13). `trip.trip_kind` distinguishes the
two; every trip-creating path writes `recorded` until US-31 lets the owner choose `planned` at
import time.

### US-61 — Trip counts and compact filters

**Done ✅** — As the owner, the trip list tells me how many trips I have and does not push them off
the screen.

**Acceptance criteria:** Two numbers above the table: how many trips match the filters, and how many
the selected tab holds in all — "247 of 312 recorded trips" — so a narrowed list says what it
narrowed *from*. Both are counted from the rows the screen already reads, which is what keeps this a
screen change: the filtered number is the list it draws, and the total is a second read with only
`kind` set, refreshed when the tab changes rather than on every keystroke. The filter area becomes
one always-visible toolbar — the Recorded/Planned tabs, the name search and the activity — with
dates, distance, tags and the region map behind a "More filters" disclosure that stays open once
opened, so the table starts near the top of a desktop screen instead of below a stack of fieldsets.
Every filter keeps working exactly as it does now, URL included (US-52): this moves controls, it
does not change what they do, and a filter set in a shared link must still show its value when the
disclosure is opened. The bulk-tag panel (US-34) is not a filter and stays where it is, above the
table it acts on.

**Decisions:** US-60/US-61 rest on no decision: both move controls inside the SPA ADR-0024 chose,
and US-61's counts and filters keep the query semantics ADR-0011 fixed and the URL contract US-52
established.

### US-63 — Trip-list map, paging and phone layout

**Done ✅** — As the owner, the trip list shows me where the trips are, a page at a time, and works
on a phone.

**Acceptance criteria:**

**The map becomes the middle of the screen rather than a filter's fine print.** The region map
(US-52, carrying US-14) comes out of the "Region" disclosure and sits above the table at twice its
present height (`.region-map`, 320px → 640px), "Select area" and "Clear region" beside it.
Everything that made it a filter still works unchanged — a dragged rectangle narrows the list,
travels in the URL, and comes back from a shared link — it is simply no longer *only* that. Leaving
`<details>` also removes the reason `RegionFilter` waits for `opened` before mounting (Leaflet
cannot lay out inside a closed disclosure), and it means an ordinary visit now fetches OSM tiles,
which the collapsed panel deliberately avoided: that is what having the map in view costs, and it is
paid on purpose.

**What the map draws.** Every trip the filters match, so the shape of the list is visible as a
place: translucent marks that darken where trips pile up — a heat map by superposition, not by
plugin. `Leaflet.heat` would be a new vendored asset and a decision of its own, and what this needs
does not justify either. The mark is the *centre* of a trip's stored bounding box rather than the
box: a long tour's box spans hundreds of kilometres and says little about where the tour was, while
its centre answers "roughly where" — at the price of a sprawling trip being represented by a point
it may never have passed, which is why the track itself stays on the detail screen. The positions
are the missing piece: the bounding box is stored on every trip and `TripDetail` carries it, but
`TripSummary` — what the list reads — does not, so the list's response shape gains it (ADR-0015
decides field by field, and this field is already in the table). Which marks are drawn, and how
heavy each is, is computed in Rust and unit-tested; the script only draws (ADR-0025). Redrawn
whenever the filters change, on the same terms the table re-queries.

**Paging.** 50 rows to a page, paged in the client over the rows the screen already reads — the same
read US-61's counts come from, so nothing new is fetched and the heat map still has every matching
trip. Those counts stay and gain the owner's place in the list beside them. The page resets to the
first whenever the filters change (page 4 of a list narrowed to nine rows is a blank table) and
stays out of the URL: US-52's contract is about what the list *is*, and a page number is where the
owner is inside it. US-34's select-all covers the rows on the current page, which is what "cannot
select a trip the owner cannot see" already meant, and a selection survives paging, so trips from
several pages can be acted on together. Paging the *query* server-side is deliberately not this
story.

**The table on a phone.** US-41 put the table in an `overflow-x: auto` box so the page itself never
scrolls sideways — and the box never scrolls either: Pico's `:where(table) { width: 100% }` shrinks
the table to the box instead of letting it overflow, so eight columns are squeezed into slivers and
no scrollbar ever appears. The table gets the minimum width its columns actually need and its
numeric cells stop wrapping, so the box scrolls as it was meant to while the page still does not.
Asserted in the browser layer: a scrollbar is invisible to a rendered string (ADR-0012).

**One activity type for many trips.** The panel that stages tags for the selected trips (US-34)
gains a second action beside it: set the activity type of every selected trip, chosen from the same
closed set the edit form offers (ADR-0018). It differs from tagging in a way the screen has to
respect — a tag is added, an activity type *overwrites* one — so it says how many trips it will
change and asks before it does. One request rather than one per trip: a bulk endpoint mirroring
`POST /api/trips/tags`, applied in a single transaction, so a failure leaves the selection as it was
instead of half-changed. Two existing rules come along: a Komoot-linked trip's edit queues a change
the next sync pushes back (US-22), so this queues one for every linked trip in the selection, and
the request is refused with 409 while a sync is in flight, exactly as a single trip's `PATCH` is
(US-26).

**Decisions:** US-63 → ADR-0011 + US-52 (the region query and URL contract the map keeps while it
stops being only a filter), ADR-0015 (the bounding box joins `TripSummary`, the field decision this
needs), ADR-0025 (Rust decides the marks, the script draws them), ADR-0018 (the bulk activity type
stays a closed set), ADR-0021 + US-22/US-26 (a bulk edit queues a Komoot push per linked trip and is
refused during a sync)

### US-65 — Region selection by touch

**Done ✅** — As the owner, I select a region on the trip-list map by touch, not only with a mouse.

**Acceptance criteria:**

The region map (US-14/US-52, US-63) draws its rectangle from pointer events rather than mouse
events, so a finger, a mouse and a pen all draw through the same path.

**Arming.** "Select area" becomes a toggle the owner can back out of: while armed it says so
(`aria-pressed`, labelled "Cancel selection"), and tapping it again disarms. Rust holds that state
and the script follows it (ADR-0025) — the button is Dioxus's node, not the script's. While armed, a
one-finger drag draws, and everything else a touch could do to the map is off: Leaflet's panning,
its pinch zoom and the two-finger pan that comes with it, and the browser's own pinch of the page
(`touch-action: none`). A second finger is ignored. A finished rectangle becomes the region exactly
as a dragged one always has — the query, the URL, a reload — and disarms.

**What does not count as a rectangle.** A tap, or a drag under 5 px on either side on screen, is
thrown away and the map stays armed: today a click without moving sends a zero-size region, which
empties the list, and a finger makes that slip far likelier. A drag the browser cancels
(`pointercancel`) drops the half-drawn rectangle, puts back the one the filters hold, and stays
armed. A touch drag is asserted in the browser layer, beside the existing mouse drag as the
regression (ADR-0012).

**Decisions:** US-65 → ADR-0025 (the armed state is decided in Rust and crosses to the script over
the region map's existing channel; the script only reads pointers and draws), ADR-0012 (touch is a
real user event, so it is asserted in the browser layer), US-14/US-52 (the region query and URL
contract a drawn rectangle feeds, unchanged)

### US-66 — Find trips needing a name or photo placement

**Done ✅** — As the owner, I want to be able to quickly find trips, without a proper name or trips,
that have photos, that are not placed.

**Acceptance criteria:** Non-proper names are trip names that don't start with a date
("yyyy-mm-dd").

### US-73 — Trip lines when zoomed in

**Done ✅** — As the owner, the trip-list map shows the trips themselves once I zoom in far enough,
not only where they pile up.

**Acceptance criteria:** The trip-list map (US-63) draws every trip the filters match as a
translucent mark at its bounding box's centre — right for a continent, useless for a valley, where a
dozen marks sit on top of each other and say nothing about which way the trips went. **Past a zoom
threshold the map switches to lines**: each matching trip whose bounding box overlaps the visible
area is drawn as its track, the way a share's overview map draws them (US-53), in US-72's shades of
its activity's color, taken in list order over every trip the filters match, so panning does not
recolor a trip — named and highlighted on hover, and a click or tap opens that trip's detail screen.
Zooming back out past the threshold returns to the heat marks. The threshold is a constant in
`config.rs`, not a UI setting. The map has so far only reported a dragged rectangle back to Rust; it
now also reports its zoom and visible bounds when they settle, and Rust decides which lines to draw,
as it decides the marks today ([ADR-0025](./adr/0025-js-widget-interop-via-eval.md); US-52's spike
showed `eval` carries this). Tracks are fetched only for the trips in view, each at most once while
the list is open, so panning back does not fetch it again, and the map says so while they load; a
track that cannot be read is left off, as on the share's map. There is no cap on how many trips are
in view: past the threshold, every one of them is drawn, however dense the region. Tracks are drawn
as stored; whether a simplified form from the server is needed is left to real use to show.
Everything US-63 and US-65 made the map do still works in both modes: a change of filters redraws
it, and while "Select area" is armed the lines are not clickable, so a drag that starts on one still
draws the rectangle. The share's overview map is unchanged: it always draws lines.

**Decisions:** US-73 → ADR-0025 (the map reports its viewport; Rust decides marks or lines, the
script draws them), ADR-0012 (the browser layer for the switch at the threshold and a line's click)

## Trip page

### US-7 — Trip detail page

**Done ✅** — As the owner, a **trip detail page** lets me relive a trip.

**Acceptance criteria:** Shows the track on an OSM map, an elevation profile, and a photo gallery
with map markers. *(All four on the SPA's screen since US-42; the markers are US-3/US-4's.)*

**Decisions:** US-7 → ADR-0025 (Leaflet/OSM + uPlot, driven through `document::eval`; supersedes
ADR-0005/0006)

### US-8 — Computed trip stats

**Done ✅** — As the owner, trip **stats are computed automatically**, never entered by hand.

**Acceptance criteria:** Distance, ascent, descent, duration, and bounding box are derived from the
GPX at import.

**Decisions:** US-1/US-8 → ADR-0004 (import handler), ADR-0003 (track storage)

### US-9 — Delete a trip

**Done ✅** — As the owner, I can **delete a trip** (and its files) to fix mistakes.

**Acceptance criteria:** Deleting a trip removes its DB rows (cascade) and its photo blobs; no
orphaned files remain.

### US-15 — Edit name and activity type

**Done ✅** — As the owner, I can edit trip details (name and activity type) from the **trip detail
view** to correct mistakes.

**Acceptance criteria:** The new values for name and activity type are saved to the database.

**Decisions:** US-11/US-15 → ADR-0008 (write API: import metadata + edit endpoint)

### US-21 — Download the original GPX

**Done ✅** — As the owner, I can **download the original GPX file** I imported, from the trip detail
page, so I keep an untouched copy of the source.

**Acceptance criteria:** The exact uploaded GPX bytes are stored on import; the detail page offers a
download link; downloading returns the original file byte-for-byte with `Content-Type:
application/gpx+xml` and a sensible filename.

**Decisions:** US-21 → ADR-0003 (original GPX stored in DB with the track), ADR-0008 (download
endpoint)

### US-59 — Elevation profile hover

**Done ✅** — As the owner, hovering the elevation profile shows me the distance and elevation there
and marks that point on the track.

**Acceptance criteria:** Carries both halves of one interaction. uPlot's stock legend goes (`legend:
{ show: false }`): it is what offers the series toggle that hides the profile, and its `.u-marker`
square is what reads as a checkbox. The live readout it also provided is kept, re-rendered as
ordinary Dioxus markup from the hovered index through `format::km` and `format::metres`, so it
matches the stats above it. The hovered index round-trips through Rust — the chart sends it back,
Rust resolves it to a position and sends that to the map — the two-way `eval` interaction US-52's
spike already proved [ADR-0025](./adr/0025-js-widget-interop-via-eval.md) carries, so no amendment
falls out of this. The cursor follows a finger as well as a mouse: uPlot v1.6.31 binds mouse events
only, so `cursor.bind` is rebound to pointer events and the container claims the gesture in CSS, or
a drag along the chart is consumed as a page scroll and never reaches it. That claim is
`touch-action: pan-y` rather than the `none` this criterion first named: a vertical swipe still
scrolls the page, at the cost of a diagonal drag being taken as a scroll and dropping the readout
mid-gesture. A tap places the cursor too — uPlot positions it only on a move, so a tap would
otherwise read nothing — and a touch reading stays until the next gesture, because a touch pointer
is destroyed the moment the finger lifts and clearing it then would make the reading flash and
vanish. Dragging across the profile zooms into that range with a mouse, where a double-click
restores it, and does not with a finger, which has no double-click to undo it with.
`track::polyline` drops positions carrying fewer than two coordinates while `elevation_series` keeps
every entry, so chart index and polyline index are not the same point today; the map is handed an
index-aligned array of positions, asserted in `track.rs`'s own tests.

**Decisions:** US-57/US-59 → ADR-0025 (grouping and index-to-position resolution stay in Rust, where
they are unit-tested; US-59's chart-to-map round trip is the two-way interaction US-52 proved `eval`
carries). US-58 rests on no decision: it corrects a CSS collision between two vendored stylesheets.

### US-62 — Trip page at a glance

**Done ✅** — As the owner, the trip's page shows its numbers at a glance and lets me look at a photo
properly.

**Acceptance criteria:**

Five changes to one screen, all of them about what it spends its room on.

**The stats.** `dl.stats` keeps five entries — activity, distance, ascent, descent, duration — and
nothing else. The activity moves *into* the list (today it is a paragraph of its own above it, the
odd one out), the start *date* follows the trip's name in a muted tone rather than sitting among the
measurements, and only when the name does not already begin with one (`yyyy-mm-dd`, the prefix US-12
suggests) — nearly every name already carries the date, and repeating it beside itself is noise. No
clock time is shown there: that line says which day the trip was, and times are read where they
explain something, in the readout and the photo captions below — and "Photo timestamp timezone"
leaves the screen altogether. It was there so that a photo US-4 placed by interpolation in an odd
spot was explicable; what actually explains that is each photo's own capture time, which the gallery
now carries per thumbnail (below), so the name of the zone no longer has to stand in for it. From
768px — the width US-60's header already switches at — the list lays out as one horizontal row of
label-over-value pairs, and below it as a two-column grid rather than one pair per line. The markup
stays a `dl`: it is the right element for pairs, and the tests read it by its labels.

**The controls.** Edit, Add photos, Download original GPX and Delete are all occasional, and each
gets a full-width Pico primary button today, with the add-photos form permanently open besides. They
become one quiet row at the foot of the screen — a muted outline style written in `app.css`, because
the classless Pico build has no `.secondary` or `.outline` to borrow, so this is the same kind of
local correction that file already carries for Leaflet (US-58) — and adding photos goes behind a
button of its own, as the edit form already is, so no form is open until it is asked for. Delete
stays visually distinct from the other three, quiet but not disguised, and keeps its confirmation
(US-9).

**The tags.** The section becomes one line — the chips, and the field beside them — instead of a
heading, a paragraph and a form; the input and its `datalist` appear once adding is asked for. Every
rule US-33 fixed is untouched, the confirm-before-creating-a-new-tag step included.

**The readout.** US-59's "At cursor" gains the time of the hovered point. The server already writes
`properties.timestamps`, one per track point (US-4 reads them back to place photos);
`track::Properties` simply does not deserialize them, so `HoverPoint` carries the timestamp
alongside the distance and elevation, resolved in Rust like the rest (ADR-0025). Whether it reads as
a clock time or a date and a time is decided from the track itself: first and last timestamp on the
same date give `14:32`, different dates give `11 Jul 14:32`, so a trip that never crossed midnight
is not made to repeat its date on every reading. A point the GPX gave no time for is an empty string
in the blob and reads as a dash, not an invented time; a track whose timestamps are *all* empty
drops the element rather than showing a dash that can never fill. The time shown is local, and never
bare: the readout carries the offset it is in (`14:32 (+02:00)`), because a time with no zone on it
is worse than a time in the wrong one. It is local *to the point being hovered*, not to the trip:
the trip carries a single `tz_name`, guessed from its first coordinate
(`timezone::guess_timezone_from_track`), and one offset taken from that would label the far side of
a border wrongly — a tour from Finnmark into Finnish Lapland would read an hour behind, with
`(+02:00)` on points that were at +03:00. A trip that crosses a DST change overnight is the same
fault from the other direction. So what the track carries is not one offset but the points at which
the offset *changes*: when the track is served, each point's own zone is resolved from its
coordinate and its offset at that instant from the zone (`src/server/timezone.rs`, ADR-0019 — the
lookups US-4 already makes, one for geography and one for DST), and the served properties gain a
compact run of transitions — the first index's offset and every index where it changes, `[[0, 7200],
[1841, 10800]]`, which is a single entry for all but the trips this exists for. `HoverPoint` then
carries the offset in force at its own index, and the readout states it. Computed when served, not
stored: ADR-0003's blob stays verbatim, so there is no migration and nothing to backfill, and
because `tzf-rs`'s boundaries and `time-tz`'s tzdata are versioned independently of the archive, an
offset resolved now is the best one known now rather than one frozen at import. The cost is one
point-in-polygon lookup per track point per track read — milliseconds, on a request that already
ships thousands of coordinates; if it ever shows, offsets can be found by sampling and bisecting
around the changes instead, which is an optimisation and not a change to what is sent. A point whose
zone this build's tzdata cannot resolve falls back to the stored UTC instant (ADR-0009) and says so
— `14:32 UTC`, labelled — as does a trip with no usable timezone at all. The start date beside the
trip's name is the date where the trip started, not the one UTC was on — the same local date US-12's
name prefix is derived from (`timezone::local_date` in the trip's zone), so a name and a date beside
it can never disagree. `TripDetail` carries it ready-made, because that line is drawn before the
track has loaded; a trip with no usable timezone shows its UTC date.

**The photos.** Each thumbnail gets a caption under it: when the photo was taken, in the zone it was
taken in, through the same rendering as the readout above — which is what replaces the timezone row
the stats list loses, since a capture time per photo is the thing that actually explains where US-4
put it. Its offset is resolved from the photo's own position where it has one and from the trip's
zone where it does not, which is exactly the fallback US-4 already uses to place such a photo, so a
photo taken across a border is captioned in the zone it was taken in rather than the one the trip
started in. That capture time is read at import today (`location::read_capture_time`, ADR-0017) and
thrown away: `photo.created_at` is the insert time, not the shutter. So the `photo` table gains a
nullable capture time in RFC-3339 UTC (ADR-0009), set at ingestion and carried on `PhotoResponse`,
and a photo whose EXIF gave none simply has no caption — a dash under a thumbnail claims a missing
value where the honest thing is silence. Photos already stored can be backfilled without their
originals, because ADR-0026 keeps each stored copy's EXIF verbatim; until that runs, older photos
show no caption, which is the same thing a photo without the tag shows. Clicking or tapping a
gallery thumbnail opens that photo over the screen at the size the archive holds it
(`PhotoResponse.url` — ADR-0026's size-bounded copy; the untouched original was never uploaded), and
the owner browses from there: previous/next buttons, ArrowLeft/ArrowRight on a keyboard, a
horizontal swipe on a touch device, Escape or a close button to leave. The set being browsed is the
set it was opened from, which is the point of opening it from a marker as well: the gallery browses
the trip's photos in gallery order, while a photo tapped in a marker popup browses only the photos
that marker stands for — US-57's group — in the popup's order. Neither wraps; the buttons disable at
the ends, so it is clear where a set stops. The viewer is ordinary Dioxus markup and CSS, not a
lightbox library: it is an overlay, one photo and two buttons, and a new vendored asset would be out
of proportion to that. The popup is the part that costs something — it is built by the map's drawing
script, so a tap there has to cross back into Rust, the same two-way `eval` round trip US-59's hover
makes and the spike proved (ADR-0025, `docs/eval-two-way-spike.md`), over the channel the map
already holds open; the marker payload therefore carries each photo's full-size URL and its place in
the group, not just a thumbnail and a name. A horizontal drag must be claimed in CSS for the reason
US-59's chart taught, or it is taken as a page scroll before the viewer sees it, and the page behind
the viewer does not scroll while it is open. The viewer is not a route, so Back leaves the trip
rather than closing the overlay — accepted, and the first thing to revisit if it grates on Android
(US-16). Each layer asserts what it can: the set handed to the viewer and what it renders in this
crate's tests, the opening, the keys and the swipe in the browser layer (ADR-0012's 2026-08-26b
rule).

**Decisions:** US-62 → ADR-0025 (the hovered timestamp is resolved in Rust, and a popup's tap
crosses back over the channel US-59 already uses), ADR-0019 (per-point offsets come from the lookups
already isolated in `src/server/timezone.rs`, which is what lets a trip that crosses a zone border
or a DST change still read correctly), ADR-0017 (the capture time the gallery's captions show is the
one EXIF read already finds), ADR-0009 (stored in UTC, and the fallback when a trip has no
timezone), ADR-0026 (what "full resolution" means in an archive that stores a size-bounded copy),
ADR-0012 (which layer asserts the viewer's markup and which its keys and swipes)

### US-79 — Speed and incline on the elevation profile

**Done ✅** — As the owner, the elevation profile also shows how fast I went, and its readout
tells me the speed and the incline where I point, so I see where a trip was slow or steep.

**Acceptance criteria:** Extends US-59's chart and US-62's readout on the trip's page. **Speed** is
drawn in the same chart as a second series, in a color of its own, against a second y-axis on the
right in km/h, over the same distance axis; the elevation profile and its left axis are unchanged.
Speed is derived from the served track — distance and time between points — with no new field on the
server, and smoothed over a window set in `config.rs`, since GPS jitter between two points a second
apart reads as wild speeds. A pause reads as 0 km/h. A stretch whose points carry no time has no
speed and leaves a gap; a track with no timestamps at all (a planned trip) has no speed series and
no right axis, so the chart is the elevation profile it is today. Dragging to zoom (US-59) zooms
both series. **The readout** gains the speed (km/h) and the incline (%, positive uphill, negative
downhill) at the hovered point, beside the distance, elevation and time; the incline is smoothed
over a distance set in `config.rs`. A point with no speed reads as a dash, as a missing time does,
and a track without timestamps drops the speed from the readout altogether, as US-62 drops the time.
Both series are computed in Rust next to `elevation_series` and `hover_points` and unit-tested
there; the chart only draws them ([ADR-0025](./adr/0025-js-widget-interop-via-eval.md)).

**Decisions:** US-79 → ADR-0025 (speed and incline are computed in Rust; uPlot only draws the second
series and axis), ADR-0012 (the derived series are unit-tested)

### US-80 — Average speed

**Done ✅** — As the owner, I see the average speed I moved at, not counting breaks, so I can
tell how fast a trip actually went and compare trips and years by it.

**Acceptance criteria:** The average speed in motion is the distance covered while moving divided by
the moving time (US-77): only the stretches between consecutive timed points whose speed reaches the
activity's threshold count, so a break lowers neither figure. The moving distance is computed
alongside the moving time — at import, again when a trip's activity changes, and backfilled for
trips already stored. A trip without timestamps has no average speed. Shown in km/h.

**Trip page:** the stats list (US-62) gains an average speed entry right after distance; a trip
without timestamps leaves the entry out rather than showing a dash.

**Statistics:** average speed is a new measure on the Statistics screen (US-77). Over several trips
it is their moving distance added up divided by their moving time added up, never an average of
averages, and the activities together at the table's foot are figured the same way. A speed is not
a share of anything, so the share column is left out for it. The running total shows the average so
far by each date, and the comparison above it this year's average so far against last year's by the
same date.

**Summary:** the Summary screen (US-78) shows the average speed per activity, and for the activities
together, figured as on the Statistics screen. If this story is done first, that criterion moves into
US-78's figures.

**Shares:** a share's recipient sees the average speed as the owner does — in a shared trip's stats
(US-53) and on a shared summary (US-82).

**Decisions:** US-80 → ADR-0025 (Rust works out the speeds, the script draws the charts), ADR-0008
(the screens read their figures from the JSON API)

### US-81 — Climbing rate

**Done ✅** — As the owner, I see how fast I climbed — metres gained per hour on the trip's real
hills — so I can tell how strong a ride or hike went uphill and compare trips and years by it.

**Acceptance criteria:** **A climb** is a stretch of the track whose elevation, averaged over the
same distance US-79's incline is measured across, rises from a low point to a high point; it ends where the elevation falls more than a
set amount below its highest point so far, so a short dip or flat does not split one hill in two.
A climb counts only if it is significant for the trip's activity: on a bike ride (cycling,
bikepacking) it gains at least 30 m at an average gradient of at least 3 %, in cross-country skiing
and ski touring at least 15 m at 3 %, on a hike (hiking, mountaineering, snowshoeing) at least 75 m.
Other activities have no climbs. The thresholds and the dip are set per activity in `config.rs`, as
the moving-time thresholds are.
**The climbing rate** of a stretch is the height it gains divided by its moving time (US-77), in
m/h, so a break on the way up does not lower it. A trip's climbing rate is the height of all its
climbs added up divided by their moving time added up; a trip with no climb, or without timestamps,
has none. The trip's rate is computed alongside the moving time — at import, again when a trip's
activity changes, and backfilled for trips already stored. The climbs are found in Rust and
unit-tested there ([ADR-0025](./adr/0025-js-widget-interop-via-eval.md)).

**Trip page:** the stats list (US-62) gains a climbing rate entry right after ascent, left out when
the trip has none. Below the elevation profile, a list of the trip's climbs in track order, each
with its distance, height gained and climbing rate. In the elevation profile (US-59), each climb's
stretch is marked by a light background color below the elevation line; the speed series (US-79)
and the readout are unchanged. A trip without timestamps still lists and marks its climbs, each
climbing rate reading as a dash.

**Statistics:** climbing rate is a new measure on the Statistics screen (US-77). Over several trips
it is their climbs' height added up divided by their climbing time added up, never an average of
averages, and the activities together at the table's foot are figured the same way. It has no share
column, and the running total shows the rate so far by each date, as average speed does (US-80).

**Summary:** the Summary screen (US-78) shows the climbing rate per activity, and for the activities
together, figured as on the Statistics screen. If this story is done first, that criterion moves into
US-78's figures.

**Shares:** a share's recipient sees the climbing rate and the climbs as the owner does — in a
shared trip's stats and elevation profile (US-53) and on a shared summary (US-82).

**Decisions:** US-81 → ADR-0025 (Rust finds the climbs and works out the rates, the script draws the
charts and the marked stretches), ADR-0008 (the screens read their figures from the JSON API)

## Photos

### US-3 — Place geotagged photos

**Done ✅** — As the owner, photos with **EXIF GPS** appear on the map where they were taken.

**Acceptance criteria:** A geotagged photo shows a marker at its EXIF coordinates; `location_source =
exif`.

**Decisions:** US-3/US-4 → ADR-0009 (timezone normalization)

### US-4 — Place photos by timestamp

**Done ✅** — As the owner, photos **without GPS** are placed by matching their timestamp to the
track, so untagged shots still appear.

**Acceptance criteria:** A non-geotagged photo whose time falls within the track range gets an
interpolated position (`location_source = interpolated`). A photo outside the track time range is
left unplaced (`location_source = none`) and not shown on the map.

**Decisions:** US-3/US-4 → ADR-0009 (timezone normalization)

### US-5 — Generate thumbnails

**Done ✅** — As the owner, **thumbnails** are generated automatically on import so galleries and
maps load fast.

**Acceptance criteria:** Each photo has a generated thumbnail; originals are kept untouched; EXIF
orientation is honored.

**Decisions:** US-5 → ADR-0007 (BlobStore holds originals + thumbnails), ADR-0020 (image crate for
thumbnail generation)

### US-30 — Place photos manually

**Done ✅** — As the owner, I can manually place attached photos on a track location.

**Acceptance criteria:** Photos can be placed by selecting any point on the map — not only on the
track, which is shown as a guide when selecting a new location. Placing starts from the photo
viewer, so a photo that is on no marker is reachable from the gallery. Automatically determined
locations (exif, interpolated, or provided by Komoot) can be overwritten manually after a warning;
an unplaced or already manually placed photo is moved without one. A placed photo's
`location_source` is `manual`; there is no reverting to the automatic position.

**Decisions:** US-30 → ADR-0025 (the placing map reports each pick back over the channel US-52
proved `eval` carries; Rust decides what a pick means), ADR-0018 (`manual` joins the closed
`LocationSource` set), ADR-0008 (the `PATCH` that stores it)

### US-54 — Downscale photos on import

**Done ✅** — As the owner, photos are downscaled when they are imported, so the volume holds years
of trips and an import fits in the machine's memory.

**Acceptance criteria:** Every photo is stored as a size-bounded copy
([ADR-0026](./adr/0026-store-downscaled-photos.md)), whichever way it arrives — uploaded on the
import screen or to an existing trip (US-2, US-43), or pulled by the Komoot sync (US-22/US-23). A
photo larger than the bound is stored scaled down, as a JPEG that keeps the original's EXIF; one
within it is stored byte-for-byte as uploaded. GPS position, capture time and orientation are read
from the upload before it is resized, so placement on the map (US-3/US-4) and the photo's
orientation are unchanged. A photo the server cannot decode, or cannot decode within the machine's
memory, is stored as uploaded rather than failing the import. Adding photos to an existing trip
uploads them in batches, as the import screen does, so no single request approaches the upload cap.

**Decisions:** US-54 → ADR-0026 (store a size-bounded copy, not the original), ADR-0020 (the image
crate that already decodes every photo for its thumbnail), ADR-0007 (what the volume holds)

### US-57 — Group photos taken at one place

**Done ✅** — As the owner, I can reach every photo taken at the same place, not just the last one.

**Acceptance criteria:** `photo_markers` groups photos whose positions lie within **20 m** into one
marker, whose popup shows all of them and which says how many it stands for. Grouping is
seed-anchored: the first photo of a group fixes its position, and a later photo joins only if it is
within 20 m *of that seed* — deterministic in the order the API lists them, and it bounds a group at
20 m, where merging transitively would chain photos 15 m apart into a group far wider than the
threshold. It happens in Rust, where it is unit-tested without a browser
([ADR-0025](./adr/0025-js-widget-interop-via-eval.md): JS renders, Rust decides). Today's
one-`circleMarker`-per-photo draw stacks coincident markers so that only the topmost is clickable,
which US-4's interpolation produces routinely by snapping several photos onto a single track point.
Distance is an equirectangular approximation computed locally — sub-metre at this threshold — rather
than a reason to pull `geo` into the wasm crate, which does not depend on it.

**Decisions:** US-57/US-59 → ADR-0025 (grouping and index-to-position resolution stay in Rust, where
they are unit-tested; US-59's chart-to-map round trip is the two-way interaction US-52 proved `eval`
carries). US-58 rests on no decision: it corrects a CSS collision between two vendored stylesheets.

### US-64 — Photo placement across timezone borders

**Done ✅** — As the owner, a photo taken after I crossed a timezone border lands where I took it,
not an hour's walk away.

**Acceptance criteria:**

US-4 places a photo without GPS by resolving its EXIF wall clock to an instant and interpolating
that instant along the track. The resolution used one zone for the whole trip —
`placement::capture_time_to_utc` fell back to the trip's `tz_name`, guessed from its *first*
coordinate — so a photo taken after a border crossing was resolved an hour out and interpolated to
wherever the track was an hour earlier or later: on a ski tour from Finnmark into Finnish Lapland,
kilometres from where it was taken, and the archive stated it as confidently as any other placement.
US-62 needs the offsets in force along a track anyway, to say what time a hovered point was at, so
this story is that same knowledge applied where it changes stored data rather than a caption: one
function in `src/server/timezone.rs` — each point's zone from its coordinate (`tzf-rs`) and its
offset at that instant from the zone (`time-tz`), reduced to the indices where the offset changes
(ADR-0019) — computed once per trip and used by both. Whichever story lands first introduces it;
neither blocks the other.

**The placement itself stops guessing a zone and searches instead.** For each stretch of the track
over which the offset is constant, the photo's wall clock implies one candidate instant
(`naive.assume_offset(that offset)`), and that candidate counts only if it falls inside the stretch
it came from — which makes the answer the point on the track where the *local* clock read what the
camera wrote, rather than what one zone says about the whole trip. The stretches are contiguous, so
a trip that crosses nothing is one stretch spanning all of it and every such photo is placed
precisely where it is today: this changes the trips that cross something and no others. A westward
crossing repeats an hour of local time and so can produce two candidates; the earlier wins, the
tie-break `timezone::resolve_to_utc` already makes for the ambiguous hour of a DST fall-back. An
eastward crossing skips an hour, so a photo written inside the skipped hour matches no stretch; it
falls back to today's behaviour — the trip's own zone — which leaves such a photo no worse placed
than the archive placed it before. Nothing else about US-3/US-4's order of precedence moves: a known
location still wins, EXIF GPS still wins over any of this, and an embedded `OffsetTimeOriginal`
still short-circuits the search, because a photo that carries its own offset already names an
instant and has nothing to guess about.

**The owner stops being asked for a zone.** The import screen's "Photo timezone" field existed to
correct the guess placement leaned on; with the track answering that question, the field,
`ConfirmImport.timezone` and `import::resolve_timezone` all go, and a trip's `tz_name` is always the
zone its track starts in. The column keeps its other jobs — naming the trip's date in local terms,
the QMapShack export's `Timezone:` line, the last fallback above, and US-62's caption fallback for a
photo with no position of its own — so it is not removed, only stopped from deciding something it
cannot know. The one case the field still bought anything is a camera clock set to a zone the track
was never in, which US-30 answers per photo rather than per trip.

**Photos already placed keep their positions.** Re-placing them would mean reading every stored
copy's EXIF back (ADR-0026 keeps it verbatim) to recover capture times the `photo` row does not hold
until US-62 stores them; the archive's mis-placed photos are few, they are already visible on the
map, and US-30 moves any one of them exactly where it belongs.

**Tested where it can be reasoned about.** `tzf-rs` carries its boundary data in the binary and
`time-tz` its tzdata, so the unit tests use real coordinates either side of a real border
(Norway/Finland is the case this exists for) with no network and no fixture of their own (ADR-0012):
a photo on each side of the crossing, the repeated hour, the skipped hour, a trip sitting out a DST
change overnight, and — the one that matters most — a trip that crosses nothing, asserting the
position it gets today. The lookups are per track point and happen once per trip during an import,
not once per photo.

**Decisions:** US-64 → ADR-0019 (the geography and DST lookups it searches with, still behind
`src/server/timezone.rs`), ADR-0009 (placement still resolves to a stored UTC instant; only the
offset it resolves *with* changes), ADR-0017 (the EXIF capture time and embedded offset it starts
from), ADR-0012 (the border cases are unit-tested against bundled boundary data, with the
no-crossing trip as the regression)

## Tags

### US-33 — Tag trips

**Done ✅** — As the owner, I can **tag** trips on the trip detail view, so that I can organize my
trips in an easy way.

**Acceptance criteria:** Tags are simple strings (no spaces, no commas — the comma restriction added
by US-38 so a tag name can never collide with the trip list's comma-separated tag filter), using a
new tag creates the tag on-demand after confirmation. Tag names are normalized (trimmed, lowercased)
so casing doesn't create duplicates; the detail view suggests existing tags as the owner types.
Untagging a trip keeps the now-unused tag row around, suggestible again later.

### US-34 — Tag many trips at once

**Done ✅** — As the owner, I can select multiple trips in the trip list and assign tags to them, so
that I can organize my trips efficiently.

**Acceptance criteria:** The trip list lets the owner select any number of trips, individually or
all at once; with a selection active, one or more tag names can be staged (same suggestions and
new-tag confirmation as US-33) and then applied to every selected trip in one request. If the
selected trip ids include one that no longer exists, the whole request 404s and nothing is created
or linked (all-or-nothing); an invalid tag name 400s the same way US-33's does.

### US-38 — Filter by tag

**Done ✅** — As the owner, I can filter trips by tag in the trip list

**Acceptance criteria:** The trip list lets the owner choose any number of the known tags; only
trips that have all chosen tags are shown. Chosen tag names are joined into one comma-separated
`tags` query param (`GET /api/trips`, ADR-0011) — unambiguous because tag names can never contain a
comma (US-33); a malformed tag name (e.g. containing whitespace or a comma) 400s the same way
US-33/34 reject one on write, but a well-formed, merely nonexistent tag name just matches nothing.

### US-83 — Tags page

**Done ✅** — As the owner, I see every tag in one place, so I can reach a tag's summary, clear out
tags I no longer use and set up a tag before any trip carries it.

**Acceptance criteria:** A Tags screen, reached from the header menu (US-60) right after
"Statistics", lists every tag alphabetically — those no trip carries included — each with the
number of trips carrying it in parentheses after its name. A tag carrying at least one recorded trip
links to its summary (US-78) with that tag chosen; any other tag's summary would be empty, so its
name is plain text. A tag that an active summary share (US-82) names carries the common share icon
beside its name.

**Finding a tag:** a text field above the table narrows it to the tags whose names contain what is
typed, ignoring case, as the owner types. The filter lives in the URL, as the trip list's filters do
(US-52), so it can be bookmarked and survives a reload. The table shows 50 tags a page, with the
trip list's paging (US-63); changing the filter goes back to the first page.

**Deleting a tag:** each row can delete its tag, after a confirmation naming it, since it cannot be
undone. Only the tag goes: it is taken off every trip that carried it, and no trip is deleted or
otherwise changed. Once deleted, it is no longer suggested (US-33), offered as a filter (US-38) or
choosable on the Summary screen. **A shared tag leaves its share:** a summary share (US-82) that
names other tags as well is narrowed to the remaining ones and keeps working; one that names only
this tag is stopped, as stopping it on the Shares screen would (US-69). The confirmation says
explicitly what happens to each such share — by its label, or its tags without one — narrowed or
stopped.

**Creating a tag:** a field below the table creates a tag by name, carrying no trips yet, so it is
suggested when tagging later. The name is normalized and validated as US-33's are, and the screen
says why a name is refused; a name that already exists is not created twice, and the screen says it
exists. The new tag shows in the table at once.

**Decisions:** US-83 → ADR-0008 (the screen reads and changes the tags through the JSON API)

## Maps

### US-58 — Map zoom controls

**Done ✅** — As the owner, the maps' zoom controls look right.

**Acceptance criteria:** Pico's classless build styles `[role=button]` with `padding:
var(--pico-form-element-spacing-vertical) var(--pico-form-element-spacing-horizontal)` — 15px 20px.
Leaflet's zoom controls are `<a role="button">`, and Leaflet sets width, height and line-height but
never padding, so under the universal `box-sizing: border-box` that padding exceeds the 30 px button
and forces it to 42×32: the "+" sits hard against the right edge and the "−" is pushed out of the
box altogether. Leaflet puts that role on its popup close button and on every keyboard-reachable
marker too, so the same padding, border and corner radius reach US-57's cluster badge and the photo
popups' close button — this criterion first said `.leaflet-bar`, which named only where the damage
was noticed. `app.css` neutralises what Pico alone sets, inside `.leaflet-container`, alongside the
Pico corrections already there; Leaflet's own rules are more specific and keep the rest. Both maps
are affected — the detail screen's track map and the trip list's region map (US-52) — so the fix
belongs to the control rather than to either screen.

**Decisions:** US-57/US-59 → ADR-0025 (grouping and index-to-position resolution stay in Rust, where
they are unit-tested; US-59's chart-to-map round trip is the two-way interaction US-52 proved `eval`
carries). US-58 rests on no decision: it corrects a CSS collision between two vendored stylesheets.

### US-75 — Activity colors on maps

**Done ✅** — As the owner and as a share's recipient, the map shows each trip in its activity's
color, so I see at a glance what kind of trip a line or mark is.

**Acceptance criteria:** Every map draws its tracks in one blue today. **Each activity type gets a
main color**, and every map view uses it: the detail screen's track map, the photo-placement map, a
share's overview map (US-53), and the trip-list map — its heat marks (US-63) as well as its lines
(US-73). The colors follow the QMapShack export's families (US-36): hiking and mountaineering reds,
cycling and bikepacking blues, kayaking cyan, the ski activities and snowshoeing magentas, an
unspecified activity gray — so a trip looks alike in the archive and in QMapShack, except
snowshoeing, which the export keeps green. Activities that share a family are still told apart, and
every color stays readable on the OSM tiles. The mapping is one exhaustive function over
`ActivityType` in Rust, so a new activity cannot be added without a color, and the scripts only draw
what they are given ([ADR-0025](./adr/0025-js-widget-interop-via-eval.md)). A map showing more than
one activity has a small legend naming the colors of the activities it shows, and no others.
**US-72's per-trip colors become variations of the activity's color**: trips of the same activity on
a share's map get shades of that activity's color, stable for the share as before, and reused in
list order once a share holds more trips of one activity than there are shades. A shade stays closer
to its own activity's color than to any other activity's, so telling trips apart never blurs what
kind of trip each is. Photo markers, on the track map and on the photo-placement map, turn amber, a
color no activity uses. The ring marking the point hovered on the elevation chart takes the trip's
own color: it only ever appears on that one trip's map.

**Decisions:** US-75 → ADR-0018 (the activity is a closed enum, so the color mapping is exhaustive),
ADR-0025 (Rust decides the colors, the scripts draw them), ADR-0022 (the QMapShack colors the
families follow)

## Sharing

### US-53 — Share trips by link

**Done ✅** — As the owner, I give someone read-only access to a trip or a few, with the GPX
downloadable, without giving them my password.

**Acceptance criteria:** A share is an unguessable link scoped to the trips it names: read-only, no
account, no signup, revocable, optionally expiring. It reaches those trips' photos, tracks and GPX
and nothing else. [ADR-0010](./adr/0010-single-user-optional-auth.md)'s amendment resolves a
*principal* rather than a yes/no so that this needs no rewrite of the gate. **The link carries its
token in the path** (`/app/s/<token>` for the screens, `/s/<token>/api/…` and `/s/<token>/media/…`
for the data), not in a cookie: a share reaches only a read-only route set of its own, so no owner
handler needs to know shares exist, and the owner's cookie grants nothing there. An unknown, expired
or emptied share (every trip deleted) answers exactly like a route that does not exist. The owner
creates a share from the trip list's selection or from a trip's page, with an optional label — the
title the recipient sees — and an expiry of never, 1 month or 6 months. The recipient lands on the
shared trips' list with every track drawn on one map (a single trip opens directly), and sees each
trip's stats, map, elevation profile, photos and GPX download — never its tags, its Komoot link or
any control that changes it. The token is kept as-is, so the owner can copy a link again (US-69); a
database snapshot or backup (US-40) therefore holds every live link. Rotating the password does not
end shares; stopping one is US-69's.

**Decisions:** US-53 → ADR-0010's amendment (a share is a capability link resolving to a `Share`
principal, not a user account), ADR-0015 (the recipient's own response types), ADR-0025 (the
overview map: Rust decides the lines, the script draws them), ADR-0012 (the browser layer for the
link opening without a login and the map's clicks)

### US-69 — Manage shares

**Done ✅** — As the owner, I see which shares are active and can stop one, so a link I handed out
stops working when I want it to.

**Acceptance criteria:** Administers the shares US-53 creates. A list, behind the owner's login,
shows every active share: the trips it reaches, when it was created, and when it expires if it does.
Stopping a share takes effect on the next request: its link then answers exactly as an unknown link
does — no trips, photos, tracks or GPX, and nothing that says the share once existed. A stopped or
expired share leaves the list; stopping cannot be undone, and giving access again means creating a
new share. Only the owner's session can list or stop shares; a share's own link can do neither.

**Decisions:** US-69 → ADR-0010's amendment (stopping a share ends its `Share` principal; listing
and stopping are owner-only routes)

### US-70 — Access log

**Done ✅** — As the owner, I see how my archive is used — and above all how the links I shared are
used — so I know whether a link reached anyone and what they looked at.

**Acceptance criteria:** Every request is written to stdout as one line: method, path, status,
duration, who made it — the owner, a share (its id and label), an anonymous caller, or an unknown
link — and the caller's IP address and user agent. Every request except the bundle's content-hashed
files (`/app/assets/*`, which say nothing about use) is also stored in the database: time, method,
path, status, duration, who made it, the share's label, and the user agent — **never an IP
address**. A share's token never appears in either, nor does a cookie, an `Authorization` header, a
request body or the password: a path carries the token blanked out. Loading a share's page
(`/app/s/<token>`) counts as that share, not as an anonymous caller. The Shares screen (US-69)
shows, per active share, how often its link was opened, when last, and on which devices. Storing a
record never slows or fails the request it describes. Records are kept indefinitely (US-71 bounds
them if that ever matters), and a stopped share's records stay, label included — the share itself is
gone, so the record is the only place that label survives.

**Decisions:** US-70/US-71 → ADR-0010's amendment (the principal the gate resolves is what tells the
owner's requests from a share's), ADR-0002 (the records live in the same SQLite file, so the backup
and the volume snapshots carry them)

### US-71 — Bound the access log

**Planned 📋** — As the owner, the access log does not grow without bound, so the volume and my
backups stay small after years of use.

**Notes:** Rough: records older than a retention period set in `config.rs` are
deleted, on the way into the next write, the way abandoned staged imports are swept (US-12).
Possibly old records are first folded into monthly counts per share and user agent, so "opened 40
times in 2027" outlives the requests themselves. Only worth doing once US-70's table has grown
noticeably.

**Decisions:** US-70/US-71 → ADR-0010's amendment (the principal the gate resolves is what tells the
owner's requests from a share's), ADR-0002 (the records live in the same SQLite file, so the backup
and the volume snapshots carry them)

### US-72 — Tell shared trips apart

**Done ✅** — As the recipient of a share with several trips, I can tell the trips apart on the map,
so I know which line is which trip before opening it.

**Acceptance criteria:** Today every line on the overview map (US-53) is the same blue, and only a
hover tooltip names it; where tracks touch or overlap, which is which is guesswork, and on a phone
there is no hover. Each trip's line gets a color of its own, and the list below the map shows that
color beside the trip's name, so a line can be matched to its row without touching either. The
colors are shades of each trip's activity color (US-75), taken in list order and reused once a share
holds more trips of one activity than there are shades. Which color a trip gets is decided in Rust
alongside the rest of the line ([ADR-0025](./adr/0025-js-widget-interop-via-eval.md)) and is stable
for a given share, so it does not change between visits. Pointing at a row (hover, or focus)
highlights its line and brings it to the top; pointing at a line highlights its row. Clicking a line
still opens that trip. A single-trip share opens that trip directly, as now, and is unaffected. The
owner's own maps are out of scope.

**Decisions:** US-72 → ADR-0025 (Rust decides each line and its color, the script draws them),
ADR-0012 (the browser layer for the row↔line highlight)

### US-82 — Share a summary

**In progress ✅** — As the owner, I share a tag summary (US-78) by link, so someone I travelled with, or
told about the trip, can look back on the whole vacation — its figures, its map and every trip in it.

**Acceptance criteria:** The owner creates the share from the Summary screen, for the tags it shows,
with the optional label and expiry a trip share has (US-53); the label is the title the recipient
sees, and without one the tag names are. **The share names the tags, not trips:** it reaches the
recorded trips under them whenever the link is opened, so a trip tagged later appears to the
recipient and one untagged disappears. Planned trips never appear. Everything else about the link is
US-53's: unguessable, read-only, no account, the token in the path, revocable, and an unknown,
expired or stopped share answering exactly like a route that does not exist. A share whose tags hold
no recorded trips for the moment stays alive and says so, since tagging can fill it again.

**What the recipient sees:** the Summary screen for the shared tags, as the owner sees it — figures,
map and trip list, with the tags' names and colors — but without the tag search and without removing
a tag, so the summary cannot be widened or narrowed, and without the owner's menu. Clicking a line or
a trip opens it as a shared trip's page does (US-53): stats, map, elevation profile, photos and GPX,
never a control that changes it. The shared tags' names are the only tags the recipient sees: a
trip's other tags, its Komoot link and every trip outside the shared tags stay out of reach, its
photos and GPX included.

**Managing it:** the Shares screen (US-69) lists the share with its tags in place of its trips, and
stopping it works as stopping a trip share does. The access log (US-70) counts its link as that
share's.

**Decisions:** US-82 → ADR-0015 (the recipient's own response types), ADR-0025 (Rust adds the
figures up and decides the lines, the script draws them), ADR-0008 (the screen reads its trips from
the JSON API)

## Statistics

### US-77 — Statistics

**In progress ✅** — As the owner, I see statistics over my trips — what I did per activity and over
the years — so the archive tells me more than one trip at a time.

**Acceptance criteria:** One Statistics screen, reached from the header menu (US-60), counts recorded
trips only, since a planned trip (US-32) was never done. A trip without timestamps is counted
nowhere, and the screen says how many were left out.

**Controls:** the period (all years, or one year), the one measure shown at a time (distance,
ascent, moving time, trips, days out) and the activities (all, or any of them, from a dropdown of
checkboxes). They live in the URL, as the trip list's filters do (US-52), so a view can be bookmarked
and survives a reload.

**Totals:** the chosen measure per year (all years) or per month (one year). With all or several
activities, a table with a row per activity, a column per year or month, a total, and each
activity's share of the total; those activities together at its foot. The activity column stays in
view while the columns scroll. With one activity, a bar chart on that activity's own scale, so a
hike is not dwarfed by a ride.

**Running total:** the chosen measure added up day by day through the year, a line per year in a
color of its own, the chosen year (or else the current one) highlighted by a wider line. Pointing
at a line or its legend entry singles that year out. Above it, this year so far against last year
by the same date.

**Records:** the three longest trips, the three with the most ascent, and the three longest days —
the most distance started on one local date — best first, for the chosen period, overall and per
chosen activity, or for the one chosen activity alone. Each links to its trips.

**Definitions:** a trip counts in the year and month of its local start date. Days out are the
distinct local dates the trips cover, start to end, in each trip's timezone. Moving time is the time
between consecutive timed points whose speed reaches the activity's threshold; it is computed at
import, recomputed when a trip's activity changes, and backfilled for trips imported before it
existed. Per-tag figures are US-78's.

**Decisions:** US-77 → ADR-0025 (Rust adds the figures up, the script draws the charts), ADR-0008
(the screen reads its trips from the JSON API)

### US-78 — Tag summary

**Done ✅** — As the owner, I see a **tag summary** — what the trips under one tag add up to — so
a multi-day vacation or similar, grouped by a tag (US-33), can be looked back on as a whole.

**Acceptance criteria:** A Summary screen, reached from the header menu (US-60) as its second entry,
between "All trips" and "Statistics", counts recorded trips only, as US-77 does; a trip without
timestamps is counted nowhere, and the screen says how many were left out.

**Choosing tags:** a search box on the heading's row, on its right, offers the known tags as the
owner types (the suggestions US-33's field gives), and choosing one adds it to the summary; a chosen
tag can be removed again. One tag gives its summary, several put theirs side by side, so one
vacation can be compared with another.
The chosen tags live in the URL, as the trip list's filters do (US-52), so a summary can be
bookmarked and survives a reload. A tag with no recorded trips says so instead of showing zeros.
On a trip's detail page, each tag links to the Summary screen with that tag chosen.

**Figures:** per tag, the dates it spans — the local start date of its first trip to the local end
date of its last — and, per activity, the number of trips, days out, total distance, ascent, descent
and moving time, with those activities together beneath when a tag holds more than one; and its
longest day, with its date and distance, linking to its trips. Several tags show as a table with a
column per tag. Days out, longest day and moving time are US-77's.

**Map:** the summary shows every trip under the chosen tags on one map, as a share's overview map
draws them (US-53), zoomed so that all of them are in view. With several tags, each tag's trips are
drawn in a color of its own, shown beside the tag's name; a trip under more than one of them counts
in each tag's figures but is drawn once. Clicking a line opens that trip.

**Trips:** under the figures, every trip under the chosen tags is listed, grouped by tag in the order
chosen, oldest first: its name linking to it, its date, activity and distance, beside its line's
color on the map. A trip under more than one chosen tag is listed under each. With several tags,
each group is headed by its tag's name and color.

**Decisions:** US-78 → ADR-0025 (Rust adds the figures up and decides the lines, the script draws
them), ADR-0008 (the screen reads its trips from the JSON API)

## QMapShack export

The owner also wants to browse and visually compare their whole trip history, and reuse
existing track segments when planning new routes, in the desktop tool **QMapShack**.
QMapShack is a **side-tool only**: read access to the archive's data, no round-trip back in — a
route planned in QMapShack is still imported into Komoot the established way. See
[`docs/qmapshack.md`](./qmapshack.md) for the full discussion and feasibility spike,
[`docs/qmapshack-format.md`](./qmapshack-format.md) for the reverse-engineered database format,
and [ADR-0022](./adr/0022-qmapshack-export.md) for the resulting design.

### US-36 — QMapShack export

**Done ✅** — As the owner, I can export my whole trip archive to a QMapShack-compatible database,
organized into folders per a configuration I control, so I can browse and compare all my trips in
QMapShack without importing hundreds of files by hand.

**Acceptance criteria:** Running the export CLI binary against a configured QMapShack database path
writes every trip in the archive as a QMapShack track item, placed into folders per the owner's
folder-mapping configuration (at minimum activity type and year). The export checks the target
database's version and fails clearly, without writing anything, if it doesn't match what the
exporter was built for.

**Decisions:** US-36/37/39 → ADR-0022 (QMapShack export)

### US-37 — Re-runnable QMapShack export

**Done ✅** — As the owner, I can re-run the export at any time and have only the
trip-archive-derived items in the QMapShack database added, updated, or removed to match the
archive's current state, so the QMapShack copy stays in sync without me resetting it or ending up
with duplicates.

**Acceptance criteria:** A trip added since the last run appears as a new item on the next export. A
trip edited (US-15) or deleted (US-9) since the last run has its QMapShack item updated or removed
(moved to QMapShack's trash) accordingly. Items and folders not created by a previous export run —
e.g. the owner's own waypoints, routes, or reorganization done directly in QMapShack — are left
untouched. The sync is one-way only: nothing in the QMapShack database is ever read back into
trip-archive.

**Decisions:** US-36/37/39 → ADR-0022 (QMapShack export)

### US-39 — Validate the QMapShack config

**Done ✅** — As the owner, I want to be informed by the qmapshack-exporter if the config is
incomplete, before the start of an actual export.

**Acceptance criteria:** The qmapshack-exporter validates that all trip_types and activity_types
have a mapping in the config, before any trips are exported. If some configs are missing all missing
entries are listed in an error message.

**Decisions:** US-36/37/39 → ADR-0022 (QMapShack export)

## Access & security

### US-19 — Password protection

**Done ✅** — As the owner, my archive is reachable only by me, so putting it on the public internet
does not put my trips there.

**Acceptance criteria:**

A shared password, supplied as a platform secret (US-48), is the only way in: every route except the
login endpoint, the static SPA bundle and the bookmark redirects answers `401` with JSON and no
data, and a route added later is protected without anyone remembering to protect it. A correct
password opens a session that survives a reload, a redeploy, and the machine being stopped and
woken; a wrong one is rate-limited. Signing out ends the browser's session; changing the password
ends every session that exists, which is the only revocation there is. The SPA shows its own login
screen, not a browser dialog, and returns to the screen asked for. The mechanism is
[ADR-0010](./adr/0010-single-user-optional-auth.md)'s 2026-09-02 amendment; this story asserts the
behaviour, and stops at US-16, whose WebView loads media cross-origin.

**As built:** `TRIP_ARCHIVE_PASSWORD` with no development exemption — a missing or empty one refuses
the boot, which is US-48's criterion landing here because this story shipped first; a 90-day session
slid forward past its halfway mark; and a global fifteen-minute lockout after five consecutive
failures. `Authorization: Bearer` is accepted as a second form of the same token, ready for US-16.
No health endpoint is allowlisted — US-45 adds one if the platform asks for it.

**Decisions:** US-19 → ADR-0010 + its 2026-09-02 amendment (one shared secret, no accounts; session
cookie; a gate that resolves a principal), ADR-0023 (which promoted it to blocking), ADR-0008 (the
JSON 401 the SPA reads)

### US-55 — Session tokens resist offline guessing

**Done ✅** — As the owner, a leaked session token does not let anyone recover my password by
guessing offline.

**Acceptance criteria:** The session signing key is derived from the password with Argon2id under a
random salt the server generates on first boot and keeps in the data directory, never sending it
anywhere ([ADR-0010](./adr/0010-single-user-optional-auth.md)'s 2026-09-19 amendment). A leaked
token therefore allows no guessing of the password; should the salt leak too, every guess costs
Argon2id's configured work. Sessions survive restarts and redeploys, and changing the password still
ends every session. A missing, unreadable or corrupt salt is replaced on boot, which ends every
session once and loses nothing else; the salt is not part of the backup (US-40).

**Decisions:** US-55 → ADR-0010 + its 2026-09-02 amendment (the signing key derived from the
password) and its 2026-09-19 amendment (Argon2id under a salt the server keeps)

## App & clients

### US-16 — Native Android app

**Planned 📋** — As the owner, I access my archive from **Android**.

**Notes:** Deferred in favour of US-67, the installed web UI. A native Android app,
built from the same Dioxus source as the web UI (ADR-0024) and reading the JSON API; recording stays
in komoot, so no native sensors are needed. Feasibility confirmed on a real device 2026-08-26
(docs/dioxus-spike.md). Revisit if photos uploaded from the phone lose their GPS position, or if
uploads die while the phone is doing something else.

**Decisions:** US-16 → ADR-0024 (Android app built from the same Dioxus source as the web UI),
ADR-0008 (the JSON API it consumes)

### US-60 — Header menu

**Done ✅** — As the owner, the things I do occasionally live in one menu, not on the pages.

**Acceptance criteria:** "Import a trip", "Sync with Komoot", "Sign out" and a way back to "All
trips" move into a single menu in the app's header, reachable from every screen — today the first
two sit on the trip list (US-43/US-44), the third floats above the router, and the way home is a
link the detail screen carries of its own. On a narrow screen the menu is a burger that opens; from
a tablet's width up the same items sit inline in the header, where there is room for them. Sign out
stays web-only for the reason it already is: the Android build deliberately has none (US-16), so the
menu there is one item shorter rather than offering something that cannot work. The menu opens and
closes without a page load, closes when one of its items is chosen or the pointer goes elsewhere,
and is reachable by keyboard — it is the only way to those screens once the page links are gone. The
detail, import and sync screens each carried a "← All trips" link of their own — this criterion
first said only the detail screen did — and all three go: one way home, in the same place on every
screen, rather than the same action twice. The browser's Back button still restores a *filtered*
list, which no such link ever did.

**Decisions:** US-60/US-61 rest on no decision: both move controls inside the SPA ADR-0024 chose,
and US-61's counts and filters keep the query semantics ADR-0011 fixed and the URL contract US-52
established.

### US-67 — Install on the Android home screen

**Done ✅** — As the owner, I install the archive on my Android phone's home screen and open it like
an app.

**Acceptance criteria:** Chrome on Android offers to install the deployed instance (US-49). The
installed app has its own name and icon on the home screen and opens full screen, without the
browser's controls, on the trip list — or on the login screen once the session has ended. Signing in
there lasts as long as it does in the browser, and the system back button moves through the app's
history as it does in a tab. Offline use is out of scope: the installed app shows what the instance
holds, and is as current as its last deploy.

**Decisions:** US-67 → ADR-0023 (mobile access through the responsive web UI / PWA), ADR-0024 (the
SPA that is installed)

### US-68 — Show the running version

**Done ✅** — As the owner, I can see which version of the archive I'm running, and whether my open
page is older than the server, so I know if a deploy has reached me.

**Acceptance criteria:** The trip list shows the SPA's version in small type in its top-right
corner: the deployed commit's date (UTC) and short hash, e.g. `2026-09-24 · e45c25a`. The version is
fixed into the SPA when it is built. The deploy command works it out from the commit being deployed,
since the image is built without `.git`. A build that isn't given one, such as a local run, shows
`dev`. The server reports its own version through the JSON API, behind the same login as every other
route. When the trip list loads, it compares the two versions. If they match, the version is gray,
the same as the "Clear filters" button text. If they differ, it is dark red, and a reload fixes it,
since the app shell is never cached (US-67). If the server's version can't be fetched, the version
stays gray, because an outdated client can't be shown. On a phone, the version stays out of the way
of the list's controls.

**Decisions:** US-68 → ADR-0023 (deployed instance, deploy command), ADR-0024 (the SPA that carries
its own version), ADR-0008 (the JSON endpoint reporting the server's version)

## Hosting & backup

[ADR-0023](./adr/0023-managed-scale-to-zero-hosting.md) ended ADR-0014's deferral: the archive is
deployed to a managed scale-to-zero platform, initially **Fly.io** — a container around the
existing binary, the data directory on a persistent volume, TLS and ingress from the platform, and
the machine stopped between uses.
`docs/deployment.md` describes the deployed instance alongside the laptop run.

The stories below are what stands between that and a deployment the owner can rely on. **US-19**
(shared-password auth) has landed: ADR-0023 promoted it from optional to blocking, and it was the
one thing that had to exist before the archive could be reachable at all. The other prerequisite,
**US-40** (consistent
backup export), is what makes a single unreplicated volume survivable; it stays a future story
because its retrieval shape is still open. Nothing here is Fly-specific by design; ADR-0023 chose a
plain container plus volume precisely so it ports.

### US-10 — Self-hosting

**Done ✅** — As the owner, I **self-host** the whole thing on my own machine.

**Acceptance criteria:** Single deployable binary + static assets; all data under a configurable
data directory (`TRIP_ARCHIVE_DATA_DIR`); no external services required.

**Decisions:** US-10 → ADR-0002 (local SQLite), ADR-0010 (self-hosted, optional auth), ADR-0016
(assets relative to executable)

### US-17 — Photos on ownCloud

**Planned 📋** — As the owner, my photos live on my private **ownCloud** instance.

**Notes:** Photos move to an `OwnCloudWebDav` storage backend; the SQLite DB (incl.
tracks) stays local.

**Decisions:** US-17 → ADR-0007 (BlobStore enables ownCloud), ADR-0002 (DB stays local)

### US-40 — Consistent backup

**Done ✅** — As the owner, I can retrieve a **consistent backup** of the database and blob store
from the deployed instance, so my existing borg jobs (external disk + remote cloud storage) can
archive it like any other directory.

**Acceptance criteria:** A `backup` command on the laptop pulls the deployed archive into a
directory laid out like a data directory — `trip-archive.db` plus `photos/` — which the borg jobs
archive and a restore can use as `TRIP_ARCHIVE_DATA_DIR`. The database arrives as a snapshot the
server takes with `VACUUM INTO`, never as a copy of the live file, and the photos are exactly those
the snapshot names — so the two match even when the archive changes during a run. Photos never
change once stored, so a run fetches only those the directory does not hold yet, and removes those
the snapshot no longer names; borg's history keeps them. A run that fails at any point leaves the
previous backup complete and consistent; at most, photos already fetched for the next one wait
beside it. The snapshot sits behind the shared password like every other route (US-19); the command
asks for the password on every run, or takes it from a password command such as `kwallet-query`, and
stores it nowhere. The archive URL, the backup directory and the password command sit in a config
file on the laptop, since none of them changes between runs. The directory is on an external disk
mounted only for the backup, so nothing else can rely on it (see US-51).

**Decisions:** US-40 → ADR-0023 (backup export of DB + blobs), ADR-0007 (blobs behind `BlobStore`)

### US-45 — One-command deploy

**Done ✅** — As the owner, I deploy the archive with one command, so shipping a change is not a
manual ritual.

**Acceptance criteria:** A container image built from the repo carries the release binary and its
adjacent `public/` assets ([ADR-0016](./adr/0016-assets-relative-to-executable.md)), and one deploy
command releases it. The listen address becomes configurable: `config::server::BIND_ADDR` was
hard-coded to `127.0.0.1:3000`, which serves nothing from inside a container — it must bind
`0.0.0.0` on a configurable port. The embedded migrations (`sqlx::migrate!`) run on boot, so a fresh
volume becomes a working archive with no manual step. `docs/deployment.md` gains the deployed path
alongside the laptop-local one it documents today.

**Decisions:** US-45/46/47/48/49/50 → ADR-0023 (managed scale-to-zero hosting), ADR-0002 + ADR-0007
(what the volume holds), ADR-0016 (binary + adjacent assets as the deployable unit)

### US-46 — Persistent storage

**Done ✅** — As the owner, the deployed archive keeps its database and photos on storage that
survives redeploys, restarts, and the machine being stopped.

**Acceptance criteria:** `TRIP_ARCHIVE_DATA_DIR` points at a mounted persistent volume holding both
the SQLite database and the photo blobs ([ADR-0002](./adr/0002-sqlite-local-disk.md),
[ADR-0007](./adr/0007-blobstore-abstraction.md) unchanged). Deploying a new image keeps every trip
and photo; so does stopping and waking the machine. The volume is a single copy on one host with no
redundancy, so snapshots are enabled and their restore path is walked through by hand once, before
the instance is relied on.

**Decisions:** US-45/46/47/48/49/50 → ADR-0023 (managed scale-to-zero hosting), ADR-0002 + ADR-0007
(what the volume holds), ADR-0016 (binary + adjacent assets as the deployable unit)

### US-47 — Scale to zero

**Done ✅** — As the owner, the instance sleeps when I am not using it and wakes on my next request,
costing me nothing worse than a slow first request.

**Acceptance criteria:** Auto-stop on idle and auto-start on request are enabled; the first request
after an idle period succeeds rather than erroring, in roughly a second. The server shuts down
gracefully on `SIGTERM`: in-flight requests finish and the SQLite pool closes, so the WAL is
checkpointed rather than left to recovery on the next boot. Being stopped mid-import or mid-sync
leaves no partial trip (the import transaction already guarantees this) and nothing wedged — US-26's
sync lock is in-process and dies with the process, so an interrupted sync never blocks the next one.

**Decisions:** US-45/46/47/48/49/50 → ADR-0023 (managed scale-to-zero hosting), ADR-0002 + ADR-0007
(what the volume holds), ADR-0016 (binary + adjacent assets as the deployable unit)

### US-48 — Secrets outside the repo

**Done ✅** — As the owner, I configure the deployed instance without putting secrets in the repo or
in the image.

**Acceptance criteria:** The Komoot credentials and US-19's shared password are supplied as platform
secrets and injected as the environment variables the app already reads; none of them appear in the
image, in the deploy config, or in the repo. Missing Komoot credentials still boot the app,
unchanged from today. A missing or empty shared password does the opposite — the server refuses to
start rather than serving the archive unauthenticated; that refusal shipped with US-19, so what
remains here is supplying the secret to the deployed instance.

**Decisions:** US-45/46/47/48/49/50 → ADR-0023 (managed scale-to-zero hosting), ADR-0002 + ADR-0007
(what the volume holds), ADR-0016 (binary + adjacent assets as the deployable unit)

### US-49 — HTTPS at a stable address

**Done ✅** — As the owner, my archive answers only over HTTPS at an address that does not change, so
I can bookmark it on my phone.

**Acceptance criteria:** The instance is reachable at a stable hostname with platform-provided TLS;
plain HTTP redirects to HTTPS and no route serves content over HTTP. This is also what unblocks the
Android app, whose WebView serves the SPA from `https://dioxus.localhost` and therefore blocks
plain-HTTP thumbnails as mixed content ([ADR-0024](./adr/0024-dioxus-ui-web-and-android.md)). With
US-19, an unauthenticated request reaches a login and never data.

**Decisions:** US-45/46/47/48/49/50 → ADR-0023 (managed scale-to-zero hosting), ADR-0002 + ADR-0007
(what the volume holds), ADR-0016 (binary + adjacent assets as the deployable unit)

### US-50 — Fill the deployed instance

**Done ✅** — As the owner, I fill the deployed instance once, so I am not starting from an empty
archive.

**Acceptance criteria:** There is no real data on a laptop instance yet, so this is the archive's
first fill rather than a migration. Before it, the volume the deployment was tried on is replaced by
a fresh one, so nothing from testing survives. A procedure copies a data directory — a local one, or
a US-40 backup restored by borg — onto a fresh volume; it is how a backup is restored, and it is
walked through once with the first backup of the filled archive, after which the deployed instance
reports the same trip and photo counts as before and photos and thumbnails render.

**Decisions:** US-45/46/47/48/49/50 → ADR-0023 (managed scale-to-zero hosting), ADR-0002 + ADR-0007
(what the volume holds), ADR-0016 (binary + adjacent assets as the deployable unit)

### US-51 — Export and backfill against the cloud

**Done ✅** — As the owner, I can still run the QMapShack export and the Komoot backfill once the
archive lives in the cloud.

**Acceptance criteria:** `qmapshack_export` (US-36/37/39) reads the archive through its HTTP API and
never opens a database file ([ADR-0022](./adr/0022-qmapshack-export.md)'s 2026-09-19 amendment), so
it runs on the laptop, where QMapShack is, against the deployed archive. The archive URL (https
only, as for US-40) and an optional password command sit in the export's config file beside the
target database and the folder mapping, which is found in the owner's config directory unless
another is named, as for `backup`.

**Decisions:** US-51 → ADR-0022 + its 2026-09-19 amendment (the exporter reads the archive through
the HTTP API), ADR-0021 + its 2026-09-19 amendment (the backfill runs inside the instance), ADR-0010
(the shared password the exporter signs in with)

Deliberately *not* stories here: object storage for photo blobs is a cost decision, not a
functional one (the volume works, it just bills per GB) and belongs to **US-17**, whose current
wording names ownCloud as the only backend. Diagnostic logging needs nothing new either — the app
writes `tracing` output to stdout where the platform collects it; the access log that tells the
owner how the archive is *used* is a story of its own, US-70.

## Development

### US-56 — One integration-test binary

**Done ✅** — As a developer, the test suite builds as one integration-test binary, so a build links
once and `target/` stays small.

**Acceptance criteria:** Each file under `tests/` is its own binary today, and each links every
dependency — 31 links per build, and `target/` had grown to 87 GB before the first `cargo clean`.
The files become modules of a single `tests/it/main.rs`, keeping their `usN_…` names so a story's
acceptance tests are still found by its ID (ADR-0012); `tests/common` becomes a sibling module
rather than being compiled into every binary. `cargo test --workspace` runs the same tests, and one
story's tests still run on their own by filter (`cargo test --test it us40`). Debug info for
dependencies is already off and stale artifacts are pruned with `cargo sweep`
([docs/development.md](./development.md)); this is the remaining, larger step.

**Decisions:** US-56 → ADR-0012 (the test layers, and the story IDs that name their tests)

## UI migration to Dioxus (history)

The UI serving `/` today is the server-rendered proof of concept — it was always meant to be
replaced, and the stories above are ✅ because that PoC satisfies them. The Dioxus SPA
([ADR-0024](./adr/0024-dioxus-ui-web-and-android.md)) re-implements the same capabilities on two
platforms; it adds none, so the stories above are neither re-opened nor duplicated. What follows
schedules the move one screen at a time, because a screen — not a story — is the unit of work: the
trip-list screen alone carries seven stories' criteria between them.

The stories below state no acceptance criteria of their own. Each carries the criteria of the
stories it lists, and all four share one completion rule, from
[ADR-0012](./adr/0012-tdd-test-strategy.md)'s migration rule:

> The SPA screen satisfies the listed stories' acceptance criteria; the acceptance assertions that
> ride on the corresponding server-rendered page have moved to screen-level tests of the SPA; only
> then is that page deleted.

Until a screen's story is done, its server-rendered page stays and remains how the owner does that
work. Nothing the PoC never implemented is a migration: **US-30** and **US-12** are built in the
SPA directly, first time, and were never added to the PoC — US-12 shipped that way alongside
US-43, on the same import screen, and is listed under [Import](#import).

### US-41 — Trip list in the SPA

**Done ✅** — As the owner, I browse, filter and tag my trips in the SPA, so the trip list works the
same on my laptop and my phone.

**Acceptance criteria:** Carries the acceptance criteria of US-6, US-13, US-32, US-34 and US-38,
plus US-35's Privacy column. US-14's region filter is deliberately *not* here — see US-52. The
completion rule above applies to those criteria, but the server-rendered list page was not deleted
here: the region filter rode on that same page, so it stayed until US-52 had moved US-14's
assertions too.

**Decisions:** US-41/42/43/44/52 → ADR-0024 (the SPA these screens are built in), ADR-0012 (the
migration rule and the test layers that carry the criteria over), ADR-0025 (US-42's map and chart;
and US-52, which tests that ADR's own revisit trigger)

### US-42 — Trip page in the SPA

**Done ✅** — As the owner, I relive and edit a single trip in the SPA.

**Acceptance criteria:** Carries US-7, US-3, US-4, US-9, US-15, US-21 and US-33, plus US-35's
Private/Public picker. The map and the elevation profile reach their libraries through
`document::eval` ([ADR-0025](./adr/0025-js-widget-interop-via-eval.md)), so the browser layer covers
them, not `dioxus-ssr` ([ADR-0012](./adr/0012-tdd-test-strategy.md)) — as does US-3/US-4's photo
marker, drawn as a circle because Leaflet's default pin is an image file no bundle here ships. The
server-rendered detail page is deleted, and `public/vendor` with it: the SPA bundles its own Leaflet
and uPlot via `asset!`, so ADR-0025's duplicated-libraries consequence is resolved. Two API changes
fell out of it. `GET /api/trips/:id` was in ADR-0008's listed surface but had never been implemented
— only the page had read a single trip. And `POST /api/trips/:id/photos` answers 204 instead of
redirecting to that page: a browser `fetch` cannot decline to follow a redirect, so the SPA was
reading the status of whatever it landed on as the upload's. The import redirects to
`/app/trips/{id}` now, and `/trips/:id` redirects there too, so a bookmark made before the page was
deleted still reaches the trip.

**Decisions:** US-41/42/43/44/52 → ADR-0024 (the SPA these screens are built in), ADR-0012 (the
migration rule and the test layers that carry the criteria over), ADR-0025 (US-42's map and chart;
and US-52, which tests that ADR's own revisit trigger)

### US-43 — Import in the SPA

**Done ✅** — As the owner, I import a GPX file with its photos from the SPA.

**Acceptance criteria:** Carries US-1, US-2, US-11 and US-31, and ships together with US-12's
redesign below — the same screen, so building it twice made no sense. `POST /api/import` is
unchanged and still serves one-shot clients ([ADR-0004](./adr/0004-import-via-axum-multipart.md));
the screen drives the two-phase pair instead. Photos are chosen in one multi-select dialog and
uploaded in batches behind it, so a large import reports "6 of 12" rather than freezing — which cost
photos their place in the import transaction ([ADR-0004's 2026-09-01
amendment](./adr/0004-import-via-axum-multipart.md#amendment-2026-09-01--photos-may-land-after-the-trip-when-the-ui-reports-progress)):
a failed batch leaves a real trip carrying fewer photos, said plainly, with the rest addable from
the trip page. The server-rendered import form is deleted and `/import` redirects to the screen.

**Decisions:** US-41/42/43/44/52 → ADR-0024 (the SPA these screens are built in), ADR-0012 (the
migration rule and the test layers that carry the criteria over), ADR-0025 (US-42's map and chart;
and US-52, which tests that ADR's own revisit trigger)

### US-44 — Sync screen in the SPA

**Done ✅** — As the owner, I run "Sync now" and review which Komoot tours to pull, from the SPA.

**Acceptance criteria:** Carries US-22 and US-29, plus US-25's naming of the failed trip/tour and
US-26's `409` surfaced as a readable message. US-20, US-23, US-24 and US-27 are server- or CLI-side
and need no screen — but the two *queues* got one: the screen says how many edits (US-20) and
deletes (US-24) the run will push before it pulls anything, which the old page only did for edits.
The review page computed its list server-side, so `GET /api/komoot/sync` is new (ADR-0008); the sync
`POST` is unchanged, and its request/response pair moved to `crates/types` where
[ADR-0015](./adr/0015-db-model-response-type-separation.md)'s amendment had already placed it. No
browser-layer test, deliberately: the Playwright suite runs the real binary with no Komoot
credentials and no seam to point the client at a stand-in, so covering the ticking there would mean
a production config knob existing for tests alone ([ADR-0012](./adr/0012-tdd-test-strategy.md)'s
2026-08-26b rule bounds that layer rather than requiring it). **The last server-rendered page is
deleted**, and with it `src/server/render/` and the `/static` mount that served its script —
`/komoot/sync` redirects to the screen, and the SPA is the only UI.

**Decisions:** US-41/42/43/44/52 → ADR-0024 (the SPA these screens are built in), ADR-0012 (the
migration rule and the test layers that carry the criteria over), ADR-0025 (US-42's map and chart;
and US-52, which tests that ADR's own revisit trigger)

### US-52 — Region filter in the SPA

**Done ✅** — As the owner, I filter the trip list by geographic region in the SPA, by dragging a
rectangle on a map.

**Acceptance criteria:** Carries US-14's acceptance criteria unchanged. Split out of US-41 because
it is the one screen element that needs the map to report *back* into Rust state — a dragged
rectangle becomes a `bbox` that survives a tab switch and is restored on the next load.
[ADR-0025](./adr/0025-js-widget-interop-via-eval.md) names exactly this as its own revisit trigger:
`document::eval` "suits *draw this*", and a map that reports drags and viewport changes back "would
strain it". So this story started with a spike of that interaction rather than implementation:
`eval` carried it, so no ADR-0025 amendment was needed
([docs/eval-two-way-spike.md](./eval-two-way-spike.md)). Doing it here, early, also de-risks US-30,
which needs the same two-way map. Its filters now live in the SPA's URL, so any narrowed list is
bookmarkable and survives a reload — which is what restores the rectangle. The server-rendered list
page is deleted; `/` redirects to the SPA, and the SPA linked out to the import and Komoot pages the
PoC still owned at the time (both are its own screens since US-43 and US-44). `public/vendor` stayed
until US-42 deleted the detail page, its last consumer.

**Decisions:** US-41/42/43/44/52 → ADR-0024 (the SPA these screens are built in), ADR-0012 (the
migration rule and the test layers that carry the criteria over), ADR-0025 (US-42's map and chart;
and US-52, which tests that ADR's own revisit trigger)

US-41 and US-42 are the two screens the spike already built
([docs/dioxus-spike.md](./dioxus-spike.md)); US-43 and US-44 were new. All of them are done: the
SPA owns every path the owner has, no page is rendered on the server any more, and each path one
of those pages answered redirects into the screen that replaced it.

One thing is known to bite, already recorded in
[ADR-0024](./adr/0024-dioxus-ui-web-and-android.md) rather than restated here as criteria: the
Android build needs HTTPS before thumbnails load at all. The response-type question that sat
alongside it was settled by [ADR-0015](./adr/0015-db-model-response-type-separation.md)'s
amendment and landed with US-42, which moved `PhotoResponse` into the shared crate — the first
shape both sides of the API needed.

## Non-functional requirements

- **Self-contained:** no external API keys (OSM raster tiles, no key); single binary + assets.
- **Data ownership & portability:** all state under one data dir; SQLite DB is a single-file backup.
- **Performance at personal scale:** designed for one user's trips (hundreds), each track up to
  tens of thousands of points; list views must not load track geometry.
- **Reliability:** imports are transactional — a failed import leaves no partial trip. The trip and
  its track always commit together; the photos the import screen uploads in batches after that
  (US-43) can partly land, which is reported as such and repaired by adding the rest to the trip
  that exists (US-2).
- **Maintainability:** server-only code isolated from the WASM client; small JS surface for maps/charts.

## Further reading

See `adr/` folder for the full Architecture Decision Records and
[`architecture.md`](./architecture.md) for the C4 diagrams. The original
[`initial_plan.md`](./initial_plan.md) is kept as a frozen historical snapshot.
