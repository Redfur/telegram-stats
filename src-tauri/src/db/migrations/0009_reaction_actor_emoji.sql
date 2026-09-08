-- Разбивка "какие именно реакции ставит конкретный участник" (запрошено
-- после первого среза блока «Реакции») — `reaction_actors` до этой
-- миграции хранил только person_id, без типа реакции: этого хватало для
-- суммарного счётчика given_count, но не для "кто какой эмодзи ставит".
-- Как и остальные сырые таблицы реакций (0008), это не self-heal —
-- строки, вставленные до этой миграции, останутся с NULL здесь, разбивка
-- по ним недоступна без повторного импорта.
ALTER TABLE reaction_actors ADD COLUMN kind TEXT;
ALTER TABLE reaction_actors ADD COLUMN emoji TEXT;
ALTER TABLE reaction_actors ADD COLUMN document_id TEXT;

CREATE INDEX idx_reaction_actors_person_kind ON reaction_actors(chat_id, person_id, kind);
