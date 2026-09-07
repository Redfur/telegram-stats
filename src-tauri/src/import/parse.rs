use std::cell::RefCell;
use std::fmt;
use std::io::Read;

use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use serde_json::{Map as JsonMap, Value as JsonValue};

use super::de_helpers::opt_i64_from_num_or_str;
use super::model::{
    ChatMeta, ImportError, Message, ParsedEvent, PersonRef, Reaction, ReactionActor, ServiceEvent,
};
use super::text::{RawTextPart, RawTextValue, TextContent};

fn person_from(display_name: Option<String>, id: Option<String>) -> Option<PersonRef> {
    id.map(|id| PersonRef { id, display_name })
}

#[derive(Debug, Deserialize)]
struct RawReactionActor {
    from: Option<String>,
    from_id: Option<String>,
    date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawReaction {
    #[serde(rename = "type")]
    kind: String,
    count: i64,
    #[serde(default)]
    emoji: Option<String>,
    #[serde(default)]
    document_id: Option<String>,
    #[serde(default)]
    recent: Vec<RawReactionActor>,
}

impl From<RawReaction> for Reaction {
    fn from(raw: RawReaction) -> Self {
        Reaction {
            kind: raw.kind,
            count: raw.count,
            emoji: raw.emoji,
            document_id: raw.document_id,
            recent: raw
                .recent
                .into_iter()
                .map(|a| ReactionActor {
                    who: PersonRef {
                        id: a.from_id.unwrap_or_default(),
                        display_name: a.from,
                    },
                    date: a.date,
                })
                .collect(),
        }
    }
}

/// Плоское объединение полей `message` и `service` (они почти не
/// пересекаются: `from`/`from_id` против `actor`/`actor_id`/`action`).
/// Всё, что не названо явно, падает в `extra` через `#[serde(flatten)]` —
/// парсер не типизирует то, что сейчас никто не потребляет, и не падает
/// на незнакомых полях (poll, contact_information, rich_message,
/// inline_bot_buttons, game_*, todo_list, ...).
#[derive(Debug, Deserialize)]
struct RawEvent {
    #[serde(rename = "type")]
    r#type: String,
    id: i64,
    #[serde(default, deserialize_with = "opt_i64_from_num_or_str")]
    date_unixtime: Option<i64>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    from_id: Option<String>,
    #[serde(default)]
    text: Option<RawTextValue>,
    #[serde(default)]
    text_entities: Option<Vec<RawTextPart>>,
    #[serde(default, deserialize_with = "opt_i64_from_num_or_str")]
    edited_unixtime: Option<i64>,
    #[serde(default)]
    reply_to_message_id: Option<i64>,
    #[serde(default)]
    forwarded_from: Option<String>,
    #[serde(default)]
    via_bot: Option<String>,
    #[serde(default)]
    media_type: Option<String>,
    #[serde(default)]
    mime_type: Option<String>,
    #[serde(default)]
    file_name: Option<String>,
    #[serde(default, deserialize_with = "opt_i64_from_num_or_str")]
    duration_seconds: Option<i64>,
    #[serde(default)]
    sticker_emoji: Option<String>,
    #[serde(default)]
    reactions: Option<Vec<RawReaction>>,
    #[serde(default)]
    actor: Option<String>,
    #[serde(default)]
    actor_id: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(flatten)]
    extra: JsonMap<String, JsonValue>,
}

impl RawEvent {
    fn into_event(self, index: usize) -> Result<ParsedEvent, ImportError> {
        match self.r#type.as_str() {
            "message" => {
                let text = TextContent::from_raw(self.text, self.text_entities);
                let from = person_from(self.from, self.from_id);
                let reactions = self
                    .reactions
                    .map(|list| list.into_iter().map(Reaction::from).collect());
                Ok(ParsedEvent::Message(Box::new(Message {
                    tg_id: self.id,
                    date_unixtime: self.date_unixtime,
                    from,
                    text,
                    edited_unixtime: self.edited_unixtime,
                    reply_to_tg_id: self.reply_to_message_id,
                    forwarded_from: self.forwarded_from,
                    via_bot: self.via_bot,
                    media_type: self.media_type,
                    mime_type: self.mime_type,
                    file_name: self.file_name,
                    duration_seconds: self.duration_seconds,
                    sticker_emoji: self.sticker_emoji,
                    reactions,
                    extra: self.extra,
                })))
            }
            "service" => {
                let action = self.action.ok_or(ImportError::MissingField {
                    index,
                    field: "action",
                })?;
                let actor = person_from(self.actor, self.actor_id);
                Ok(ParsedEvent::Service(ServiceEvent {
                    tg_id: self.id,
                    date_unixtime: self.date_unixtime,
                    actor,
                    action,
                    extra: self.extra,
                }))
            }
            other => Ok(ParsedEvent::Unrecognized {
                index,
                raw_type: other.to_string(),
                tg_id: self.id,
            }),
        }
    }
}

/// Стримит элементы поля `messages` по одному через `on_event`, никогда не
/// собирая `Vec` внутри парсера (docs/SPEC.md §6: нельзя грузить весь
/// экспорт в память).
struct MessagesSeed<'a, F> {
    on_event: &'a mut F,
    index: &'a mut usize,
    error_slot: &'a RefCell<Option<ImportError>>,
}

impl<'de, 'a, F> DeserializeSeed<'de> for MessagesSeed<'a, F>
where
    F: FnMut(ParsedEvent) -> Result<(), ImportError>,
{
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SeqVisitor<'a, F> {
            on_event: &'a mut F,
            index: &'a mut usize,
            error_slot: &'a RefCell<Option<ImportError>>,
        }

        impl<'de, 'a, F> Visitor<'de> for SeqVisitor<'a, F>
        where
            F: FnMut(ParsedEvent) -> Result<(), ImportError>,
        {
            type Value = ();

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "an array of Telegram export events")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                loop {
                    let raw = match seq.next_element::<RawEvent>() {
                        Ok(Some(raw)) => raw,
                        Ok(None) => break,
                        Err(e) => return Err(e),
                    };
                    let event = match raw.into_event(*self.index) {
                        Ok(ev) => ev,
                        Err(err) => {
                            *self.error_slot.borrow_mut() = Some(err);
                            return Err(de::Error::custom("import aborted"));
                        }
                    };
                    if let Err(err) = (self.on_event)(event) {
                        *self.error_slot.borrow_mut() = Some(err);
                        return Err(de::Error::custom("import aborted"));
                    }
                    *self.index += 1;
                }
                Ok(())
            }
        }

        deserializer.deserialize_seq(SeqVisitor {
            on_event: self.on_event,
            index: self.index,
            error_slot: self.error_slot,
        })
    }
}

struct ChatVisitor<'a, F> {
    on_event: &'a mut F,
    error_slot: &'a RefCell<Option<ImportError>>,
}

impl<'de, 'a, F> Visitor<'de> for ChatVisitor<'a, F>
where
    F: FnMut(ParsedEvent) -> Result<(), ImportError>,
{
    type Value = ChatMeta;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a Telegram export root object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut name: Option<String> = None;
        let mut chat_type: Option<String> = None;
        let mut tg_chat_id: Option<i64> = None;
        let mut index = 0usize;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "name" => name = map.next_value::<Option<String>>()?,
                "type" => chat_type = Some(map.next_value::<String>()?),
                "id" => tg_chat_id = map.next_value::<Option<i64>>()?,
                "messages" => {
                    map.next_value_seed(MessagesSeed {
                        on_event: self.on_event,
                        index: &mut index,
                        error_slot: self.error_slot,
                    })?;
                }
                _ => {
                    let _ = map.next_value::<de::IgnoredAny>()?;
                }
            }
        }

        Ok(ChatMeta {
            tg_chat_id,
            name,
            chat_type: chat_type.unwrap_or_default(),
        })
    }
}

/// Потоково разбирает `result.json`: читает корневой объект, вызывает
/// `on_event` на каждый элемент `messages` по мере чтения, никогда не
/// накапливая весь массив в памяти. Ошибка в одном событии останавливает
/// весь импорт (индекс попадёт в `ImportError`) — молчаливый пропуск
/// исказил бы статистику незаметно для пользователя.
pub fn parse_export<R, F>(reader: R, mut on_event: F) -> Result<ChatMeta, ImportError>
where
    R: Read,
    F: FnMut(ParsedEvent) -> Result<(), ImportError>,
{
    let error_slot = RefCell::new(None);
    let mut de = serde_json::Deserializer::from_reader(reader);
    let visitor = ChatVisitor {
        on_event: &mut on_event,
        error_slot: &error_slot,
    };

    let result = de.deserialize_map(visitor);

    if let Some(err) = error_slot.into_inner() {
        return Err(err);
    }

    result.map_err(|source| ImportError::Json {
        index: None,
        source,
    })
}
