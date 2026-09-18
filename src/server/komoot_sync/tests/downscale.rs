//! A photo pulled by the Komoot sync is stored as the same size-bounded copy
//! as an uploaded one (US-54, ADR-0026): the sync reaches storage through
//! `ingest_photos` like every other source. Split out of the parent
//! `tests.rs` to keep that file under the repo's 500-line cap.

use super::*;
use crate::config::photo::MAX_DIMENSION;

#[tokio::test]
async fn us54_a_synced_photo_larger_than_the_bound_is_stored_downscaled() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    let (width, height) = (MAX_DIMENSION + 100, MAX_DIMENSION / 2);
    let photo = KomootPhoto {
        id: "p1".to_string(),
        src: "https://cdn.example/p1?width={width}&height={height}&crop={crop}".to_string(),
        location: None,
        width_px: width,
        height_px: height,
    };
    let resolved = crate::server::komoot::resolve_photo_url(&photo.src, width, height, false);

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![a_tour("999", "Mountain Loop", "mtb")],
        gpx: HashMap::from([("999".to_string(), SAMPLE_GPX.to_vec())]),
        photos: HashMap::from([("999".to_string(), vec![photo])]),
        photo_bytes: HashMap::from([(resolved, valid_jpeg_bytes(width, height))]),
        ..Default::default()
    });

    let summary = sync_selected_tours(&db.pool, &store, client, &recorded_sel(&["999"]))
        .await
        .unwrap();

    let (_, trip_id) = summary.imported.first().expect("tour must import");
    let stored = list_photos(&db.pool, *trip_id).await.unwrap().remove(0);
    let image = image::load_from_memory(&store.get(&stored.blob_key).unwrap()).unwrap();
    assert_eq!(image.width(), MAX_DIMENSION);
}
