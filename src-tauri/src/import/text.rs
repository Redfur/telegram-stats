use serde::Deserialize;

/// Плоский текст (для подсчёта слов/символов) плюс структурные "куски"
/// (для метрик, которым нужны ссылки/упоминания и т.п. — Фазы 5-6).
/// Строится по docs/SPEC.md §8.1: `text_entities` — приоритетный источник
/// (голые строки там всегда представлены типизированными объектами вида
/// `{type:"plain", text}`), поле `text` — фолбэк на случай, если
/// `text_entities` в экспорте нет вовсе.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextContent {
    pub plain: String,
    pub entities: Vec<TextEntity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEntity {
    pub kind: String,
    pub text: String,
    pub href: Option<String>,
}

/// Сырое значение поля `text`: JSON `null` разбирается самим `Option<T>`
/// на уровне вызывающего поля, сюда попадает только строка или массив.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RawTextValue {
    Str(String),
    Arr(Vec<RawTextPart>),
}

/// Элемент массива `text` ИЛИ `text_entities`: либо голая строка (только
/// в `text`, всегда трактуется как обычный текст), либо типизированный
/// объект `{type, text, href?}`. Неизвестные поля объекта (`user_id` у
/// `mention_name` и т.п.) serde по умолчанию игнорирует.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RawTextPart {
    Plain(String),
    Entity {
        #[serde(rename = "type")]
        kind: String,
        #[serde(default)]
        text: String,
        #[serde(default)]
        href: Option<String>,
    },
}

impl From<RawTextPart> for TextEntity {
    fn from(part: RawTextPart) -> Self {
        match part {
            RawTextPart::Plain(text) => TextEntity {
                kind: "plain".to_string(),
                text,
                href: None,
            },
            RawTextPart::Entity { kind, text, href } => TextEntity { kind, text, href },
        }
    }
}

impl TextContent {
    pub fn from_raw(
        text: Option<RawTextValue>,
        text_entities: Option<Vec<RawTextPart>>,
    ) -> TextContent {
        if let Some(entities) = text_entities {
            return TextContent::from_parts(entities);
        }
        match text {
            None => TextContent::default(),
            Some(RawTextValue::Str(s)) => {
                if s.is_empty() {
                    TextContent::default()
                } else {
                    TextContent {
                        plain: s.clone(),
                        entities: vec![TextEntity {
                            kind: "plain".to_string(),
                            text: s,
                            href: None,
                        }],
                    }
                }
            }
            Some(RawTextValue::Arr(parts)) => TextContent::from_parts(parts),
        }
    }

    fn from_parts(parts: Vec<RawTextPart>) -> TextContent {
        let mut plain = String::new();
        let mut entities = Vec::with_capacity(parts.len());
        for part in parts {
            let entity: TextEntity = part.into();
            plain.push_str(&entity.text);
            entities.push(entity);
        }
        TextContent { plain, entities }
    }
}
