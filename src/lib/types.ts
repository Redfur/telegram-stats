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
