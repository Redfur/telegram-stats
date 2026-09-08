CREATE TABLE message_type_counts (
  chat_id INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  kind    TEXT NOT NULL,
  count   INTEGER NOT NULL,
  PRIMARY KEY (chat_id, kind)
);

CREATE TABLE message_type_time_buckets (
  chat_id INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  kind    TEXT NOT NULL,
  view    TEXT NOT NULL,
  bucket  TEXT NOT NULL,
  count   INTEGER NOT NULL,
  PRIMARY KEY (chat_id, kind, view, bucket)
);

CREATE TABLE message_type_durations (
  chat_id        INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  kind           TEXT NOT NULL,
  total_seconds  INTEGER NOT NULL,
  median_seconds REAL NOT NULL,
  max_seconds    INTEGER NOT NULL,
  PRIMARY KEY (chat_id, kind)
);

CREATE TABLE message_type_participant_stats (
  chat_id          INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  kind             TEXT NOT NULL,
  person_id        INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  count            INTEGER NOT NULL,
  duration_seconds INTEGER NOT NULL,
  PRIMARY KEY (chat_id, kind, person_id)
);
