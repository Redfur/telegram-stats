// Зеркалит serde-структуры из src-tauri/src/db/mod.rs и
// src-tauri/src/metrics/queries.rs — имена полей snake_case как есть,
// без слоя трансформации между Rust и фронтом.

export interface ChatSummary {
  id: number;
  tg_chat_id: number | null;
  name: string | null;
  chat_type: string;
  message_count: number;
  participant_count: number;
  first_msg_ts: number | null;
  last_msg_ts: number | null;
  imported_at: number;
  source_file_size: number | null;
}

export interface ChatTotals {
  message_count: number;
  participant_count: number;
  word_count: number;
  char_count: number;
  first_msg_ts: number | null;
  last_msg_ts: number | null;
}

export type Granularity = "hour" | "day" | "week" | "month" | "year";

export interface ActivityBucket {
  bucket: string;
  count: number;
}

export interface ActivitySeries {
  granularity: Granularity;
  buckets: ActivityBucket[];
}

export interface ParticipantStat {
  person_id: number;
  display_name: string | null;
  message_count: number;
  avg_len: number;
}

export interface TopParticipants {
  degenerate: boolean;
  participants: ParticipantStat[];
}

// Зеркалит src-tauri/src/metrics/message_types.rs. 13 типов на этот срез,
// без реакций (см. docs/PLAN.md — своя таблица, отдельный будущий срез).
// "text" не эксклюзивен с медиа-типами — фото с подписью считается и в
// "photo", и в "text".
export type MessageTypeKind =
  | "text"
  | "photo"
  | "sticker"
  | "voice_message"
  | "video_message"
  | "video_file"
  | "animation"
  | "file"
  | "forwarded"
  | "edited"
  | "poll"
  | "location"
  | "contact";

export interface MessageTypeCount {
  kind: MessageTypeKind;
  count: number;
}

export interface MessageTypeOverview {
  /** `false` — чат импортирован до появления этой метрики, нужен переимпорт. */
  available: boolean;
  total_messages: number;
  counts: MessageTypeCount[];
}

// Гранулярность гистограммы блока. month — абсолютный календарный бакет
// (временной ряд). day_of_week/hour_of_day — распределения по всему
// периоду сразу ("в какой день недели"/"в какое время суток" обычно
// присылают), не временные ряды — bucket для day_of_week это "0".."6"
// (0 = понедельник), для hour_of_day — "0".."23".
export type TypeTimeView = "month" | "day_of_week" | "hour_of_day";

export interface TypeBucket {
  bucket: string;
  count: number;
}

// Гранулярность "темпа"/"пика" в карточке сводки — не то же самое, что
// TypeTimeView (что сейчас выбрано для гистограммы): это всегда
// календарный временной ряд, не зависит от того, что показывает график.
export type RateGranularity = "month" | "day";

export type MessageTypeSummaryStats =
  | {
      kind: "frequency";
      rate_granularity: RateGranularity;
      rate_value: number;
      peak: TypeBucket | null;
      top_sender: string | null;
    }
  | {
      kind: "duration";
      total_seconds: number;
      median_seconds: number;
      max_seconds: number;
    };

export type MessageTypeParticipantExtra =
  | { kind: "per_thousand"; value: number }
  | { kind: "total_minutes"; value: number };

export interface MessageTypeParticipantStat {
  person_id: number;
  display_name: string | null;
  count: number;
  share_of_own_pct: number;
  extra: MessageTypeParticipantExtra;
}

export interface MessageTypeDetail {
  kind: MessageTypeKind;
  total_count: number;
  pct_of_all: number;
  summary: MessageTypeSummaryStats;
  views_available: TypeTimeView[];
  selected_view: TypeTimeView;
  buckets: TypeBucket[];
  participants: MessageTypeParticipantStat[];
}

export interface PersonRow {
  id: number;
  display_name: string | null;
  aliases: string[];
  message_count: number;
  first_msg_ts: number | null;
  last_msg_ts: number | null;
  is_hidden: boolean;
}
