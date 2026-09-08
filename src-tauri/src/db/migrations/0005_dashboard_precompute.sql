ALTER TABLE chats ADD COLUMN word_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chats ADD COLUMN char_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chats ADD COLUMN dashboard_ready INTEGER NOT NULL DEFAULT 0;

CREATE TABLE activity_buckets (
  chat_id INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  bucket  TEXT NOT NULL,
  count   INTEGER NOT NULL,
  PRIMARY KEY (chat_id, bucket)
);

CREATE TABLE person_message_stats (
  chat_id       INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  person_id     INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  message_count INTEGER NOT NULL,
  text_len_sum  INTEGER NOT NULL,
  PRIMARY KEY (chat_id, person_id)
);
