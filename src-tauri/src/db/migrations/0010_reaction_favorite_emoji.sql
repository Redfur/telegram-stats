-- Самая частая реакция, которую человек СТАВИТ — колонка "Чаще ставит" в
-- единой таблице "Обмен реакциями" (макет «Дашборд», ход t8/8a). Как и
-- 0009, это агрегат поверх reaction_actors, а не новая сырая таблица —
-- self-heal-пересчитываем (recompute_reaction_person_stats), но только
-- для чатов, у которых reaction_actors.kind уже заполнен миграцией 0009;
-- у более старых данных (kind IS NULL) останется NULL и здесь.
ALTER TABLE reaction_person_stats ADD COLUMN top_given_kind TEXT;
ALTER TABLE reaction_person_stats ADD COLUMN top_given_emoji TEXT;
ALTER TABLE reaction_person_stats ADD COLUMN top_given_document_id TEXT;
