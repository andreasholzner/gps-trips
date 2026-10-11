-- US-86: no two collections share a name, whatever the case of the letters
-- A–Z (NOCASE folds no others) — the QMapShack export gives each a folder
-- by name (US-89). Enforced here, as tag names are, so every writer is held
-- to it; the repository turns the refusal into the owner's message.

CREATE UNIQUE INDEX IF NOT EXISTS idx_collection_name ON collection(name COLLATE NOCASE);
