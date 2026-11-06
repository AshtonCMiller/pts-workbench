-- One row per person: an opaque ID and an encrypted, versioned blob. Nothing readable is stored.
CREATE TABLE IF NOT EXISTS stores (
  id TEXT PRIMARY KEY,          -- hash; can't be turned back into who it belongs to without guessing
  version INTEGER NOT NULL,     -- goes up by one on every save; used to catch two computers saving at once
  data TEXT NOT NULL,           -- encrypted on the computer before it is sent
  updated_at INTEGER NOT NULL
);