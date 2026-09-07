//! Экзотические поля Telegram-экспорта, которые парсер намеренно не
//! моделирует (docs/PLAN.md ни одна метрика их пока не потребляет):
//! contact_information, todo_list, rich_message, inline_bot_buttons,
//! game_*. Здесь — не фикстуры, а маленькие инлайн-документы: проверяем
//! только "не падает и кладёт поле как есть в extra", без претензии на
//! точность формы (которую нельзя подтвердить по редактированной
//! разведке реального экспорта).

use telegram_stats_lib::import::{parse_export, ParsedEvent};

fn parse_single_message(doc: &str) -> telegram_stats_lib::import::Message {
    let mut found = None;
    parse_export(doc.as_bytes(), |event| {
        if let ParsedEvent::Message(m) = event {
            found = Some(*m);
        }
        Ok(())
    })
    .expect("document should parse without error");
    found.expect("document should contain one message event")
}

#[test]
fn contact_information_survives_in_extra() {
    let doc = r#"{
        "name": "t", "type": "personal_chat", "id": 1,
        "messages": [{
            "id": 1, "type": "message", "date_unixtime": "1700000000",
            "text": "",
            "contact_information": {"first_name": "A", "last_name": "B", "phone_number": "+10000000000"},
            "contact_vcard": "BEGIN:VCARD...", "contact_vcard_file_size": 100
        }]
    }"#;
    let m = parse_single_message(doc);
    assert!(m.extra.contains_key("contact_information"));
    assert!(m.extra.contains_key("contact_vcard"));
}

#[test]
fn todo_list_survives_in_extra() {
    let doc = r#"{
        "name": "t", "type": "private_group", "id": 1,
        "messages": [{
            "id": 1, "type": "message", "date_unixtime": "1700000000", "text": "",
            "todo_list": {"title": "Список дел", "others_can_append": true, "others_can_complete": true,
                          "answers": [{"text": "первое", "id": 1}]}
        }]
    }"#;
    let m = parse_single_message(doc);
    assert!(m.extra.contains_key("todo_list"));
}

#[test]
fn rich_message_nested_tables_survive_in_extra() {
    let doc = r#"{
        "name": "t", "type": "private_group", "id": 1,
        "messages": [{
            "id": 1, "type": "message", "date_unixtime": "1700000000", "text": "",
            "rich_message": {"rtl": false, "part": false, "blocks": [
                {"type": "table", "title": {"type": "plain", "text": {"type": "plain", "text": "Заголовок"}},
                 "bordered": true, "striped": false, "compact": false,
                 "rows": [{"cells": [{"text": {"type": "plain", "text": "1"}, "header": true, "align": "left", "vertical_align": "top"}]}]}
            ]}
        }]
    }"#;
    let m = parse_single_message(doc);
    assert!(m.extra.contains_key("rich_message"));
}

#[test]
fn inline_bot_buttons_survive_in_extra() {
    let doc = r#"{
        "name": "t", "type": "private_group", "id": 1,
        "messages": [{
            "id": 1, "type": "message", "date_unixtime": "1700000000", "text": "",
            "via_bot": "somebot",
            "inline_bot_buttons": [[{"type": "url", "text": "Open", "data": "https://example.invalid"}]]
        }]
    }"#;
    let m = parse_single_message(doc);
    assert!(m.extra.contains_key("inline_bot_buttons"));
    assert_eq!(m.via_bot.as_deref(), Some("somebot"));
}

#[test]
fn game_fields_survive_in_extra() {
    let doc = r#"{
        "name": "t", "type": "private_group", "id": 1,
        "messages": [{
            "id": 1, "type": "message", "date_unixtime": "1700000000", "text": "",
            "via_bot": "gamebot", "game_title": "Игра", "game_description": "описание",
            "game_link": "https://example.invalid/game"
        }]
    }"#;
    let m = parse_single_message(doc);
    assert!(m.extra.contains_key("game_title"));
    assert!(m.extra.contains_key("game_description"));
    assert!(m.extra.contains_key("game_link"));
}

#[test]
fn negative_message_id_is_not_an_error() {
    let doc = r#"{
        "name": "t", "type": "private_group", "id": 1,
        "messages": [{"id": -999932399, "type": "message", "date_unixtime": "1700000000", "text": "forwarded"}]
    }"#;
    let m = parse_single_message(doc);
    assert_eq!(m.tg_id, -999932399);
}

#[test]
fn date_unixtime_accepts_number_or_numeric_string() {
    let doc_str = r#"{"name":"t","type":"private_group","id":1,
        "messages":[{"id":1,"type":"message","date_unixtime":"1700000000","text":""}]}"#;
    let doc_num = r#"{"name":"t","type":"private_group","id":1,
        "messages":[{"id":1,"type":"message","date_unixtime":1700000000,"text":""}]}"#;
    assert_eq!(
        parse_single_message(doc_str).date_unixtime,
        Some(1700000000)
    );
    assert_eq!(
        parse_single_message(doc_num).date_unixtime,
        Some(1700000000)
    );
}

#[test]
fn unrecognized_top_level_event_type_does_not_abort_import() {
    let doc = r#"{
        "name": "t", "type": "private_group", "id": 1,
        "messages": [
            {"id": 1, "type": "message", "date_unixtime": "1700000000", "text": "hi"},
            {"id": 2, "type": "future_event_kind", "date_unixtime": "1700000001"}
        ]
    }"#;
    let mut events = Vec::new();
    parse_export(doc.as_bytes(), |e| {
        events.push(e);
        Ok(())
    })
    .expect("unknown event type should not abort the import");
    assert_eq!(events.len(), 2);
    assert!(matches!(events[1], ParsedEvent::Unrecognized { .. }));
}
