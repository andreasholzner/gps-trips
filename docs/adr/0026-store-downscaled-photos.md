# ADR-0026 — Store a size-bounded copy of each photo, not the original

## Status

Accepted

## Context

Photos arrive at camera resolution, several megabytes each. Stored as uploaded, next to the
generated thumbnail ([ADR-0007](./0007-blobstore-abstraction.md),
[ADR-0020](./0020-image-crate-for-thumbnails.md)), they would carry three costs once deployed
([ADR-0023](./0023-managed-scale-to-zero-hosting.md)): the volume is a single, snapshotted copy
that grows with every trip ([US-46](../requirements.md)); every photo is pulled by the backup
([US-40](../requirements.md)); and decoding a full-resolution photo during import competes for the
small machine's memory.

The archive's purpose is browsing trips — gallery, map popups, a photo viewed full-screen on a
phone or laptop — not preserving photos. The originals stay in the owner's own photo library.

## Decision

**For each photo the archive keeps a copy bounded by a maximum size, not the uploaded original**
([US-54](../requirements.md)).

- **On the server, at ingestion.** Every source — the import screen, adding photos to a trip, the
  Komoot sync — passes through the same ingestion path, so all photos are treated alike. Clients
  upload what they have.
- **Metadata comes from the untouched upload.** GPS position, capture time and orientation are
  read before the photo is resized, so placement ([US-3](../requirements.md),
  [US-4](../requirements.md)) does not depend on the stored copy.
- **Only what exceeds the bound is re-encoded.** A photo within it is stored byte-for-byte; a
  larger one is scaled down and re-encoded as JPEG, like its thumbnail (ADR-0020). Nothing is
  upscaled, and nothing is lossily re-compressed without being made smaller.
- **The stored copy keeps the original's EXIF verbatim, and its pixels are not re-oriented** — so
  its Orientation tag stays true and the file stays self-describing (capture time, GPS) in a
  backup or a restore.
- **Best-effort, never fatal.** Decoding is bounded by an allocation limit; a photo that cannot be
  decoded or exceeds the limit is stored as uploaded, the same stance
  [ADR-0017](./0017-kamadak-exif-for-gps-extraction.md) and ADR-0020 take for EXIF and thumbnails.

The bound, the JPEG quality and the allocation limit are configuration (`config::photo`), not part
of this decision.

### Alternatives considered

- **Resize in the SPA before upload.** Smaller uploads, but a browser canvas drops EXIF, so the
  metadata would have to travel separately; Komoot-synced photos never pass through the SPA, so
  the server would need the same step anyway.
- **Keep the original alongside a display copy.** Faster to view, but leaves the storage and backup
  problem exactly as it is.
- **Strip EXIF and bake in the orientation.** Marginally smaller, but a restored or backed-up photo
  would no longer carry its own capture time and position.

## Consequences

- The volume and each backup hold a fraction of what originals would need, and a photo opened on a
  phone loads correspondingly faster.
- **Irreversible per photo:** the archive cannot give back an original. Raising the bound later
  only affects photos imported afterwards; getting more detail for an earlier photo means
  re-importing it from its source.
- Uploads still arrive full-size, so the memory an import needs is bounded by the upload request
  size (client batching, the body cap), not by this decision.
- An oversized, non-JPEG photo becomes a JPEG, losing transparency or animation — acceptable for
  camera photos, for the same reason ADR-0020 gives for thumbnails.
- A photo that cannot be decoded, or exceeds the allocation limit, stays full-size; these are
  expected to be rare.
- Re-encoding costs CPU per imported photo; one decode serves both the stored copy and the
  thumbnail.
- The `BlobStore` abstraction (ADR-0007) is unchanged; what it holds as a photo's blob is now this
  copy.
