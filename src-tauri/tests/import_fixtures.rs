use std::fs::File;
use std::path::PathBuf;

use telegram_stats_lib::import::{parse_export, Message, ParsedEvent, ServiceEvent};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn parse_all(name: &str) -> (telegram_stats_lib::import::ChatMeta, Vec<ParsedEvent>) {
    let file = File::open(fixture(name)).expect("fixture file should exist");
    let mut events = Vec::new();
    let meta = parse_export(file, |event| {
        events.push(event);
        Ok(())
    })
    .expect("fixture should parse without error");
    (meta, events)
}

fn messages(events: &[ParsedEvent]) -> Vec<&Message> {
    events
        .iter()
        .filter_map(|e| match e {
            ParsedEvent::Message(m) => Some(m.as_ref()),
            _ => None,
        })
        .collect()
}

fn services(events: &[ParsedEvent]) -> Vec<&ServiceEvent> {
    events
        .iter()
        .filter_map(|e| match e {
            ParsedEvent::Service(s) => Some(s),
            _ => None,
        })
        .collect()
}

#[test]
fn three_day_chat_parses_edge_cases() {
    let (meta, events) = parse_all("three_day_chat.json");
    assert_eq!(meta.chat_type, "private_group");
    let msgs = messages(&events);
    assert_eq!(msgs.len(), 16);

    // text как строка
    let m1 = msgs.iter().find(|m| m.tg_id == 1).unwrap();
    assert_eq!(m1.text.plain, "Го завтра красить потолок?");

    // text как массив с типизированными кусками (ссылка)
    let m3 = msgs.iter().find(|m| m.tg_id == 3).unwrap();
    assert!(m3.text.plain.contains("тут"));
    assert!(m3
        .text
        .entities
        .iter()
        .any(|e| e.kind == "link" && e.href.is_some()));

    // ответ на сообщение
    let m8 = msgs.iter().find(|m| m.tg_id == 8).unwrap();
    assert_eq!(m8.reply_to_tg_id, Some(7));

    // отредактированное сообщение
    let m9 = msgs.iter().find(|m| m.tg_id == 9).unwrap();
    assert!(m9.edited_unixtime.is_some());

    // без text_entities вовсе — фолбэк на text (строка)
    let m13 = msgs.iter().find(|m| m.tg_id == 13).unwrap();
    assert_eq!(m13.text.plain, "ок, го в 11");
    assert_eq!(m13.text.entities.len(), 1);
    assert_eq!(m13.text.entities[0].kind, "plain");

    // сообщения без ключа reactions вовсе
    assert!(msgs.iter().all(|m| m.reactions.is_none()));
}

#[test]
fn personal_chat_has_exactly_two_participants_and_actor_based_service_events() {
    let (meta, events) = parse_all("personal_chat.json");
    assert_eq!(meta.chat_type, "personal_chat");
    let msgs = messages(&events);
    let mut ids: Vec<String> = msgs
        .iter()
        .filter_map(|m| m.from.as_ref().map(|p| p.id.clone()))
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 2);

    let svcs = services(&events);
    assert_eq!(svcs.len(), 2);
    for s in &svcs {
        assert!(s.actor.is_some(), "service event must use actor, not from");
        assert!(matches!(s.action.as_str(), "phone_call" | "group_call"));
        assert!(s.extra.contains_key("duration"));
    }
}

#[test]
fn group_50_members_covers_scale_and_exotic_fields() {
    let (meta, events) = parse_all("group_50_members.json");
    assert_eq!(
        meta.chat_type, "private_supergroup",
        "нестандартный тип не должен отклоняться парсером"
    );

    let msgs = messages(&events);
    let mut ids: Vec<String> = msgs
        .iter()
        .filter_map(|m| m.from.as_ref().map(|p| p.id.clone()))
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 50);

    // Deleted Account
    assert!(msgs.iter().any(
        |m| m.from.as_ref().and_then(|p| p.display_name.as_deref()) == Some("Deleted Account")
    ));

    // бот/канал среди участников
    assert!(msgs.iter().any(|m| m.via_bot.is_some()));

    // реакции: часть сообщений без поля вовсе, часть — с обоими видами
    assert!(msgs.iter().any(|m| m.reactions.is_none()));
    let with_reactions: Vec<_> = msgs.iter().filter(|m| m.reactions.is_some()).collect();
    assert!(!with_reactions.is_empty());
    let kinds: Vec<&str> = with_reactions
        .iter()
        .flat_map(|m| {
            m.reactions
                .as_ref()
                .unwrap()
                .iter()
                .map(|r| r.kind.as_str())
        })
        .collect();
    assert!(kinds.contains(&"emoji"));
    assert!(kinds.contains(&"custom_emoji"));

    // poll и location_information не смоделированы явно, но живы в extra
    assert!(msgs.iter().any(|m| m.extra.contains_key("poll")));
    assert!(msgs
        .iter()
        .any(|m| m.extra.contains_key("location_information")));

    // разнообразие сервисных действий
    let svcs = services(&events);
    let actions: Vec<&str> = svcs.iter().map(|s| s.action.as_str()).collect();
    for expected in [
        "create_group",
        "invite_members",
        "remove_members",
        "pin_message",
        "edit_group_photo",
        "migrate_to_supergroup",
    ] {
        assert!(
            actions.contains(&expected),
            "missing service action: {expected}"
        );
    }
    let invite = svcs.iter().find(|s| s.action == "invite_members").unwrap();
    assert!(invite
        .extra
        .get("members")
        .is_some_and(|v| v.as_array().is_some_and(|a| !a.is_empty())));
}

#[test]
fn no_reactions_old_export_never_invents_reactions() {
    let (meta, events) = parse_all("no_reactions_old_export.json");
    assert_eq!(meta.chat_type, "private_group");
    let msgs = messages(&events);
    assert_eq!(msgs.len(), 18);
    assert!(
        msgs.iter().all(|m| m.reactions.is_none()),
        "reactions must stay None when the field is absent from the export, never Some(vec![])"
    );
}

#[test]
fn english_chat_covers_media_and_entity_kinds() {
    let (_, events) = parse_all("english_chat.json");
    let msgs = messages(&events);

    assert!(msgs
        .iter()
        .any(|m| m.media_type.as_deref() == Some("sticker") && m.sticker_emoji.is_some()));
    assert!(msgs
        .iter()
        .any(|m| m.media_type.as_deref() == Some("voice_message") && m.duration_seconds.is_some()));
    assert!(msgs
        .iter()
        .any(|m| m.media_type.as_deref() == Some("video_file")
            && m.mime_type.is_some()
            && m.file_name.is_some()));

    let all_kinds: Vec<&str> = msgs
        .iter()
        .flat_map(|m| m.text.entities.iter().map(|e| e.kind.as_str()))
        .collect();
    for expected in ["hashtag", "spoiler", "code"] {
        assert!(
            all_kinds.contains(&expected),
            "missing text entity kind: {expected}"
        );
    }

    assert!(msgs.iter().any(|m| m.forwarded_from.is_some()));
}
