import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Skeleton } from "../../components/ui";
import { StatTile } from "../../components/StatTile/StatTile";
import { ActivityChart } from "../../components/ActivityChart/ActivityChart";
import { ParticipantsTable } from "../../components/ParticipantsTable/ParticipantsTable";
import { MessageTypesBlock } from "../../components/MessageTypesBlock/MessageTypesBlock";
import type { ActivitySeries, ChatTotals, TopParticipants } from "../../lib/types";
import styles from "./Dashboard.module.css";

interface DashboardProps {
  chatId: number;
  onReimport: () => void;
}

// Быстрое переключение между чатами в рельсе не должно бить invoke на
// каждый промежуточный chatId — только на тот, где пользователь
// задержался (ниже человеческого восприятия задержки клика, но выше
// типичного интервала между кликами при скролле по списку).
const SWITCH_DEBOUNCE_MS = 150;

// Разделы 00/01/02/03 макета «Дашборд» (вариант 2a: 00 сводка, 01
// активность, 02 кто писал) + ход t6 (03 типы сообщений) — остальные
// секции макета (хронология слов, рекорды) — следующий инкремент
// (docs/PLAN.md).
export function Dashboard({ chatId, onReimport }: DashboardProps) {
  const [totals, setTotals] = useState<ChatTotals | null>(null);
  const [activity, setActivity] = useState<ActivitySeries | null>(null);
  const [participants, setParticipants] = useState<TopParticipants | null>(null);

  useEffect(() => {
    let cancelled = false;
    setTotals(null);
    setActivity(null);
    setParticipants(null);
    const timer = setTimeout(() => {
      invoke<ChatTotals | null>("get_chat_totals", { chatId }).then((r) => {
        if (!cancelled) setTotals(r);
      });
      invoke<ActivitySeries | null>("get_activity_series", { chatId }).then((r) => {
        if (!cancelled) setActivity(r);
      });
      invoke<TopParticipants>("get_top_participants", { chatId }).then((r) => {
        if (!cancelled) setParticipants(r);
      });
    }, SWITCH_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [chatId]);

  const periodDays =
    totals?.first_msg_ts != null && totals.last_msg_ts != null
      ? Math.max(1, Math.round((totals.last_msg_ts - totals.first_msg_ts) / 86400))
      : null;
  const perDay = totals && periodDays ? Math.round(totals.message_count / periodDays) : null;

  return (
    <div className={styles.wrap}>
      <section>
        <div className={styles.sectionHead}>
          <span className="kick">00</span>
          <span className="sec">СВОДКА</span>
        </div>
        <div className={styles.tiles}>
          <StatTile
            kicker="Сообщений"
            value={totals?.message_count.toLocaleString("ru-RU")}
            loading={!totals}
          />
          <StatTile
            kicker="В день"
            value={perDay != null ? perDay.toLocaleString("ru-RU") : "—"}
            loading={!totals}
          />
          <StatTile
            kicker="Участников"
            value={totals?.participant_count.toLocaleString("ru-RU")}
            loading={!totals}
          />
        </div>
      </section>

      <section>
        <div className={styles.sectionHead}>
          <span className="kick">01</span>
          <span className="sec">АКТИВНОСТЬ</span>
          <span className={styles.caveat}>время — по UTC, часовой пояс исходного экспорта неизвестен</span>
        </div>
        <Card aria-busy={activity === null || undefined}>
          {activity === null ? (
            <Skeleton height={160} />
          ) : activity.buckets.length > 0 ? (
            <ActivityChart buckets={activity.buckets} granularity={activity.granularity} />
          ) : (
            <div className={styles.empty}>Недостаточно данных для графика</div>
          )}
        </Card>
      </section>

      <section>
        <div className={styles.sectionHead}>
          <span className="kick">02</span>
          <span className="sec">КТО ПИСАЛ</span>
        </div>
        {participants === null ? (
          <div className={styles.ratio} aria-busy="true">
            {[0, 1, 2].map((i) => (
              <div key={i} className={styles.ratioRow}>
                <Skeleton width="40%" />
                <Skeleton width="15%" />
              </div>
            ))}
          </div>
        ) : participants.degenerate ? (
          <div className={styles.ratio}>
            {participants.participants.map((p) => (
              <div key={p.person_id} className={styles.ratioRow}>
                <span>{p.display_name || "Без имени"}</span>
                <span className="num">{p.message_count.toLocaleString("ru-RU")}</span>
              </div>
            ))}
          </div>
        ) : (
          <ParticipantsTable participants={participants.participants} />
        )}
      </section>

      <section>
        <div className={styles.sectionHead}>
          <span className="kick">03</span>
          <span className="sec">ТИПЫ СООБЩЕНИЙ</span>
        </div>
        <MessageTypesBlock chatId={chatId} onReimport={onReimport} />
      </section>
    </div>
  );
}
