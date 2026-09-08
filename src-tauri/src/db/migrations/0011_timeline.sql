-- Таймлайн участников (дашборд, секция 05, docs/PLAN.md Фаза 3). Как и
-- реакции (0008_reactions.sql), это данные, доступные только во время
-- потокового парсинга — Telegram кладёт входы/выходы/групповые события
-- только в сервисные сообщения (`type: "service"`) самого экспорта и
-- нигде их не дублирует, поэтому membership_events/chat_events
-- заполняются один раз при импорте (db/import.rs), self-heal невозможен.
--
-- В отличие от реакций здесь нет отдельного "supported"-флага: сервисные
-- события с нужными action существуют в экспорте Telegram Desktop любой
-- актуальной версии, это не version-gated формат-фича. "Нечего
-- показывать" (личный чат, нет ни одного зафиксированного события)
-- определяется по факту отсутствия строк в этих таблицах, а не по
-- отдельному флагу — см. metrics/timeline.rs.
ALTER TABLE chats ADD COLUMN timeline_ready INTEGER NOT NULL DEFAULT 0;

-- Входы/выходы/удаления по человеку. `members[]` в исходном экспорте —
-- только отображаемые имена, без id (docs/SPEC.md §8.2 — from_id есть
-- только у `actor`, не у приглашённых/удалённых) — `person_id` резолвится
-- по точному совпадению имени среди уже известных участников чата;
-- неоднозначное или не найденное имя не отбрасывается, а остаётся с
-- `person_id = NULL` и сырым `unresolved_name` (честная деградация вместо
-- угадывания, см. metrics::timeline::TimelineSummary.unresolved_event_count).
CREATE TABLE membership_events (
  id                INTEGER PRIMARY KEY,
  chat_id           INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  person_id         INTEGER REFERENCES people(id) ON DELETE SET NULL,
  unresolved_name   TEXT,
  kind              TEXT NOT NULL, -- 'joined' | 'left' | 'removed'
  ts                INTEGER NOT NULL,
  related_person_id INTEGER REFERENCES people(id) ON DELETE SET NULL -- кто пригласил/удалил
);
CREATE INDEX idx_membership_events_chat ON membership_events(chat_id);
CREATE INDEX idx_membership_events_person ON membership_events(chat_id, person_id);

-- Групповые события (лента сверху таймлайна) — создание чата, смена
-- названия/фото, миграция в супергруппу.
CREATE TABLE chat_events (
  id       INTEGER PRIMARY KEY,
  chat_id  INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  kind     TEXT NOT NULL, -- 'created' | 'title_changed' | 'photo_changed' | 'migrated'
  ts       INTEGER NOT NULL,
  actor_id INTEGER REFERENCES people(id) ON DELETE SET NULL,
  detail   TEXT -- новое название для title_changed, иначе NULL
);
CREATE INDEX idx_chat_events_chat ON chat_events(chat_id);
