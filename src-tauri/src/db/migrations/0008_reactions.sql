-- Реакции (docs/SPEC.md §9 «Социальная динамика»). В отличие от
-- message_type_* эти таблицы не выводимы из `messages` задним числом —
-- Telegram кладёт реакции только в `reactions` на самом сообщении в
-- экспорте, эта информация нигде больше не хранится, поэтому
-- message_reactions/reaction_actors/message_reaction_previews заполняются
-- один раз, во время потокового парсинга (db/import.rs), а не
-- пересчитываются из уже сохранённых messages, как остальной precompute
-- в проекте. reaction_emoji_stats/reaction_person_stats — обычный
-- precompute поверх них, самими можно пересчитать в любой момент.

ALTER TABLE chats ADD COLUMN reactions_ready INTEGER NOT NULL DEFAULT 0;
-- Чат прошёл актуальный импортер (reactions_ready=1), но сам экспорт был
-- сделан старой версией Telegram Desktop без поля reactions вовсе —
-- отличаем от "просто нет реакций" (docs/SPEC.md §7/§8.5): повторный
-- импорт того же файла тут не поможет, нужен более новый экспорт.
ALTER TABLE chats ADD COLUMN reactions_supported INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chats ADD COLUMN reaction_total INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chats ADD COLUMN reacted_message_count INTEGER NOT NULL DEFAULT 0;

-- Одна строка на (сообщение, тип реакции) — `count` авторитетный (из
-- экспорта), `known_actor_count` = len(recent). Telegram отдаёт только
-- недавних реагировавших в `recent`, не полный список — count >
-- known_actor_count значит, что для части реакций этого типа на этом
-- сообщении личность неизвестна (docs/SPEC.md §8.5).
CREATE TABLE message_reactions (
  id                INTEGER PRIMARY KEY,
  chat_id           INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  message_id        INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  kind              TEXT NOT NULL,
  emoji             TEXT,
  document_id       TEXT,
  count             INTEGER NOT NULL,
  known_actor_count INTEGER NOT NULL
);
CREATE INDEX idx_message_reactions_chat ON message_reactions(chat_id);
CREATE INDEX idx_message_reactions_message ON message_reactions(chat_id, message_id);

-- Один ряд на известного реагировавшего (из `recent`) — единственный
-- источник для "кто ставит", неполный по построению (см. выше). Не хранит
-- kind/emoji: этот срез не даёт разбивку "кто какой эмодзи ставит",
-- только счётчик по человеку (YAGNI, CLAUDE.md).
CREATE TABLE reaction_actors (
  id         INTEGER PRIMARY KEY,
  chat_id    INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  person_id  INTEGER REFERENCES people(id) ON DELETE SET NULL
);
CREATE INDEX idx_reaction_actors_chat ON reaction_actors(chat_id);
CREATE INDEX idx_reaction_actors_person ON reaction_actors(chat_id, person_id);

-- Текст сообщения нигде больше не хранится (messages — только text_len/
-- word_count, docs/SPEC.md §6 отклонение то же, что и у media-полей),
-- поэтому превью для "самых залайканных сообщений" снимается один раз во
-- время импорта, пока текст ещё доступен в памяти парсера — только для
-- сообщений с реакциями, не для всех подряд.
CREATE TABLE message_reaction_previews (
  chat_id      INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  message_id   INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  preview_text TEXT NOT NULL,
  PRIMARY KEY (chat_id, message_id)
);

-- Precompute — пересчитываются из message_reactions/reaction_actors,
-- обычный self-heal паттерн (metrics::precompute::with_write_lock).
CREATE TABLE reaction_emoji_stats (
  id          INTEGER PRIMARY KEY,
  chat_id     INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  kind        TEXT NOT NULL,
  emoji       TEXT,
  document_id TEXT,
  count       INTEGER NOT NULL
);
CREATE INDEX idx_reaction_emoji_stats_chat ON reaction_emoji_stats(chat_id);

CREATE TABLE reaction_person_stats (
  chat_id        INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  person_id      INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  given_count    INTEGER NOT NULL DEFAULT 0,
  received_count INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (chat_id, person_id)
);
