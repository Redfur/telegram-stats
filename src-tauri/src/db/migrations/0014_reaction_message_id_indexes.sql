-- Тот же класс бага, что чинила 0013, но для другого DELETE. Реимпорт
-- (db/import.rs) чистит старые сообщения одним `DELETE FROM messages
-- WHERE chat_id = ?1`, а `delete_chat` удаляет чат целиком через
-- `ON DELETE CASCADE` от `chats` — в обоих случаях на каждое удаляемое
-- сообщение SQLite неявно проверяет потомков в message_reactions/
-- reaction_actors/message_reaction_previews (все три — `message_id
-- REFERENCES messages(id) ON DELETE CASCADE`, миграция 0008). У всех
-- трёх были только составные индексы, где ведущая колонка — chat_id
-- (`idx_message_reactions_message`, `idx_reaction_actors_person` и
-- PRIMARY KEY(chat_id, message_id) у previews) — для проверки по
-- голому message_id они бесполезны, ровно как `(chat_id, person_id)`
-- был бесполезен для проверки по голому person_id в 0013. На чате с
-- полноценной историей реакций (сотни тысяч сообщений) это превращало
-- и реимпорт, и обычное удаление чата в подвисание на десятки секунд
-- без ошибки и без реакции на «Отменить» (найдено на реальном
-- использовании, см. docs/PLAN.md).
CREATE INDEX idx_message_reactions_message_id ON message_reactions(message_id);
CREATE INDEX idx_reaction_actors_message_id ON reaction_actors(message_id);
CREATE INDEX idx_message_reaction_previews_message_id ON message_reaction_previews(message_id);
