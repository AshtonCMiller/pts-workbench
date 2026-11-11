-- One row per person: an opaque ID and an encrypted, versioned blob. Nothing readable is stored.
CREATE TABLE IF NOT EXISTS stores (
  id TEXT PRIMARY KEY,          -- hash; can't be turned back into who it belongs to without guessing
  version INTEGER NOT NULL,     -- goes up by one on every save; used to catch two computers saving at once
  data TEXT NOT NULL,           -- encrypted on the computer before it is sent
  updated_at INTEGER NOT NULL
);

-- Bug reports and feature ideas sent from the app ("Feedback"). Readable on purpose (it's the point),
-- so the app sends no VINs, ROs or customer details with them: the person's name, what they wrote,
-- the app version and which screen they were on.
CREATE TABLE IF NOT EXISTS reports (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  kind TEXT NOT NULL,             -- 'bug' or 'feature'
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',  -- who sent it (as shown in the app)
  reporter TEXT NOT NULL,         -- random ID of the computer that sent it, so people can see their own
  version TEXT NOT NULL DEFAULT '',
  platform TEXT NOT NULL DEFAULT '',
  screen TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'new',  -- new · planned · in-progress · done · wont-do
  reply TEXT NOT NULL DEFAULT ''       -- a note back to the person, shown in their list
);
CREATE INDEX IF NOT EXISTS reports_reporter ON reports (reporter, created_at);