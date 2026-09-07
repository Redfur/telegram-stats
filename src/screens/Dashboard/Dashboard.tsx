import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card } from "../../components/ui";
import { StatTile } from "../../components/StatTile/StatTile";
import { ActivityChart } from "../../components/ActivityChart/ActivityChart";
import { ParticipantsTable } from "../../components/ParticipantsTable/ParticipantsTable";
import type { ActivitySeries, ChatTotals, TopParticipants } from "../../lib/types";
import styles from "./Dashboard.module.css";

interface DashboardProps {
  chatId: number;
}

// Разделы 00/01/02 макета «Дашборд» (вариант 2a) — минимальный набор
// метрик из MVP-среза, остальные секции макета (03 хронология слов,
// 04 рекорды) — следующий инкремент (docs/PLAN.md).
export function Dashboard({ chatId }: DashboardProps) {
  const [totals, setTotals] = useState<ChatTotals | null>(null);
  const [activity, setActivity] = useState<ActivitySeries | null>(null);
  const [participants, setParticipants] = useState<TopParticipants | null>(null);

  useEffect(() => {
    let cancelled = false;
    setTotals(null);
    setActivity(null);
    setParticipants(null);
    invoke<ChatTotals | null>("get_chat_totals", { chatId }).then((r) => {
      if (!cancelled) setTotals(r);
    });
    invoke<ActivitySeries | null>("get_activity_series", { chatId }).then((r) => {
      if (!cancelled) setActivity(r);
    });
    invoke<TopParticipants>("get_top_participants", { chatId }).then((r) => {
      if (!cancelled) setParticipants(r);
    });
    return () => {
      cancelled = true;
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
        {totals && (
          <div className={styles.tiles}>
            <StatTile kicker="Сообщений" value={totals.message_count.toLocaleString("ru-RU")} />
            <StatTile kicker="В день" value={perDay != null ? perDay.toLocaleString("ru-RU") : "—"} />
            <StatTile kicker="Участников" value={totals.participant_count.toLocaleString("ru-RU")} />
          </div>
        )}
      </section>

      <section>
        <div className={styles.sectionHead}>
          <span className="kick">01</span>
          <span className="sec">АКТИВНОСТЬ</span>
          <span className={styles.caveat}>время — по UTC, часовой пояс исходного экспорта неизвестен</span>
        </div>
        <Card>
          {activity && activity.buckets.length > 0 ? (
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
        {participants &&
          (participants.degenerate ? (
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
          ))}
      </section>
    </div>
  );
}
