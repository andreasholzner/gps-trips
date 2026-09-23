//! The server's integration tests, built as one binary (US-56) so the suite
//! links its dependencies once. Each module holds one story's acceptance
//! tests and keeps its `usN_` name, so `cargo test --test it us40` runs
//! just that story's (ADR-0012).

mod common;

mod us10_self_host;
mod us11_activity_type;
mod us12_staged_import;
mod us13_filter_trips;
mod us14_filter_by_region;
mod us15_edit_trip;
mod us19_auth;
mod us1_import;
mod us21_download_original_gpx;
mod us22_sync_candidates_api;
mod us25_sync_halts_on_failure;
mod us26_sync_blocks_concurrent_edits;
mod us2_photos;
mod us30_manual_photo_placement;
mod us31_trip_kind_import;
mod us32_trip_kind_tabs;
mod us33_tag_trips;
mod us34_bulk_tag_trips;
mod us35_komoot_privacy;
mod us36_qmapshack_export;
mod us37_qmapshack_resync;
mod us38_filter_by_tag;
mod us3_photo_map_placement;
mod us40_backup;
mod us41_spa_bundle;
mod us47_graceful_shutdown;
mod us4_photo_timestamp_interpolation;
mod us51_export_api;
mod us51_remote_export;
mod us54_downscale;
mod us5_thumbnails;
mod us62_trip_page;
mod us63_trip_list;
mod us66_find_trips_to_tidy;
mod us6_trip_list;
mod us7_trip_detail;
mod us9_delete_trip;
