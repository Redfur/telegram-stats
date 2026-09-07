CREATE TABLE chats (
  id                INTEGER PRIMARY KEY,
  tg_chat_id        INTEGER,
  name              TEXT,
  chat_type         TEXT NOT NULL,
  source_filename   TEXT,
  imported_at       INTEGER NOT NULL,
  first_msg_ts      INTEGER,
  last_msg_ts       INTEGER,
  message_count     INTEGER NOT NULL DEFAULT 0,
  participant_count INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX idx_chats_tg_chat_id ON chats(tg_chat_id) WHERE tg_chat_id IS NOT NULL;

CREATE TABLE people (
  id           INTEGER PRIMARY KEY,
  chat_id      INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  tg_person_id TEXT NOT NULL,
  display_name TEXT,
  UNIQUE(chat_id, tg_person_id)
);

CREATE TABLE messages (
  id         INTEGER PRIMARY KEY,
  chat_id    INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  tg_msg_id  INTEGER NOT NULL,
  person_id  INTEGER REFERENCES people(id) ON DELETE SET NULL,
  ts         INTEGER,
  text_len   INTEGER NOT NULL DEFAULT 0,
  word_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_messages_chat_ts ON messages(chat_id, ts);
CREATE INDEX idx_messages_chat_person ON messages(chat_id, person_id);
