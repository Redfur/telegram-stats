ALTER TABLE people ADD COLUMN merged_into INTEGER REFERENCES people(id);
ALTER TABLE people ADD COLUMN is_hidden INTEGER NOT NULL DEFAULT 0;

CREATE TABLE aliases (
  id INTEGER PRIMARY KEY,
  person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  first_seen INTEGER,
  last_seen INTEGER,
  UNIQUE(person_id, name)
);
