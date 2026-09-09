-- Индексы на голых FK-колонках membership_events.person_id/
-- related_person_id и chat_events.actor_id (0011_timeline.sql завёл
-- только составные (chat_id, person_id) — бесполезные для запроса без
-- chat_id). Без них SQLite при удалении строки people (ON DELETE SET
-- NULL) гоняет полный скан membership_events/chat_events, чтобы найти
-- ссылающиеся строки — реконсиляция переимпорта (db/import.rs) удаляет
-- людей по одному в цикле, так что на реальном чате с большим числом
-- участников это превращало переимпорт в подвисание на этапе
-- "Finalizing" без какого-либо прогресса или ошибки.
CREATE INDEX idx_membership_events_person_id ON membership_events(person_id);
CREATE INDEX idx_membership_events_related_person_id ON membership_events(related_person_id);
CREATE INDEX idx_chat_events_actor_id ON chat_events(actor_id);
