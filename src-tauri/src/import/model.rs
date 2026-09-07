use serde_json::{Map as JsonMap, Value as JsonValue};

use super::text::TextContent;

/// Метаданные чата из корня экспорта. `chat_type` — сырая строка из поля
/// `type`, никогда не валидируется: Telegram Desktop добавляет новые
/// значения между версиями (см. docs/SPEC.md §2).
#[derive(Debug, Clone)]
pub struct ChatMeta {
    pub tg_chat_id: Option<i64>,
    pub name: Option<String>,
    pub chat_type: String,
}

#[derive(Debug, Clone)]
pub enum ParsedEvent {
    // Boxed: Message is much larger than the other variants (clippy
    // large_enum_variant) — every element streamed through parse_export
    // would otherwise pay for the biggest variant's stack size.
    Message(Box<Message>),
    Service(ServiceEvent),
    /// `type` ни `"message"`, ни `"service"` — будущая версия формата.
    /// Не паникуем и не роняем импорт, просто передаём наверх как есть.
    Unrecognized {
        index: usize,
        raw_type: String,
        tg_id: i64,
    },
}

/// `id`/`actor_id`/`from_id` — единственная стабильная идентичность
/// человека (docs/SPEC.md §8.2). `display_name` — снимок отображаемого
/// имени на момент события, никогда не используется как ключ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonRef {
    pub id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Message {
    /// Может быть отрицательным в реальных экспортах — не признак ошибки.
    pub tg_id: i64,
    /// Канонический источник времени (docs/SPEC.md §8.4); `date` не
    /// сохраняется отдельно — не используется ни одной будущей метрикой.
    pub date_unixtime: Option<i64>,
    pub from: Option<PersonRef>,
    pub text: TextContent,
    pub edited_unixtime: Option<i64>,
    pub reply_to_tg_id: Option<i64>,
    pub forwarded_from: Option<String>,
    pub via_bot: Option<String>,
    pub media_type: Option<String>,
    pub mime_type: Option<String>,
    pub file_name: Option<String>,
    pub duration_seconds: Option<i64>,
    pub sticker_emoji: Option<String>,
    /// `None` — поля `reactions` в экспорте не было вовсе (старая версия
    /// Telegram Desktop). `Some(vec![])` — поле есть, но пусто. Это два
    /// разных состояния, парсер не имеет права их смешивать.
    pub reactions: Option<Vec<Reaction>>,
    /// Всё, что не смоделировано явно (`poll`, `contact_information`,
    /// `location_information`, `todo_list`, `rich_message`,
    /// `inline_bot_buttons`, `game_*`, `photo`, `width`/`height` и т.д.) —
    /// сохраняется как есть. Ни одна метрика в docs/PLAN.md пока это не
    /// потребляет, но парсер обязан не падать на этих полях.
    pub extra: JsonMap<String, JsonValue>,
}

#[derive(Debug, Clone)]
pub struct ServiceEvent {
    pub tg_id: i64,
    pub date_unixtime: Option<i64>,
    pub actor: Option<PersonRef>,
    /// Сырая строка action (в реальных данных встречено 14 разных
    /// значений). Не enum: ни одна метрика Фаз 4-9 пока не требует их
    /// различать поимённо — типизируем, когда появится первый потребитель.
    pub action: String,
    pub extra: JsonMap<String, JsonValue>,
}

#[derive(Debug, Clone)]
pub struct Reaction {
    /// "emoji" | "custom_emoji" | будущие значения — сырая строка.
    pub kind: String,
    pub count: i64,
    pub emoji: Option<String>,
    pub document_id: Option<String>,
    pub recent: Vec<ReactionActor>,
}

#[derive(Debug, Clone)]
pub struct ReactionActor {
    pub who: PersonRef,
    pub date: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("failed to read export file: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed export JSON near message index {index:?}: {source}")]
    Json {
        index: Option<usize>,
        #[source]
        source: serde_json::Error,
    },
    #[error("event at index {index} missing required field `{field}`")]
    MissingField { index: usize, field: &'static str },
    /// Колбэк `on_event` (например, запись в SQLite) отказал по своей
    /// причине — парсер её не типизирует, только пробрасывает наверх и
    /// останавливает импорт.
    #[error("import aborted by consumer: {0}")]
    Sink(String),
}
