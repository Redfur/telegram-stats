import { Bar, BarChart, ResponsiveContainer, Tooltip, XAxis } from "recharts";
import { Segmented } from "../ui";
import type { TypeBucket, TypeTimeView } from "../../lib/types";
import { formatTypeBucketLabel } from "../../lib/formatTypeBucketLabel";
import styles from "./MessageTypesChart.module.css";

interface MessageTypesChartProps {
  buckets: TypeBucket[];
  view: TypeTimeView;
  viewsAvailable: TypeTimeView[];
  onViewChange: (view: TypeTimeView) => void;
}

const VIEW_OPTIONS: { value: TypeTimeView; label: string }[] = [
  { value: "month", label: "По месяцам" },
  { value: "day", label: "По дням" },
  { value: "hour_of_day", label: "По часам" },
];

export function MessageTypesChart({ buckets, view, viewsAvailable, onViewChange }: MessageTypesChartProps) {
  const options = VIEW_OPTIONS.filter((o) => viewsAvailable.includes(o.value));
  const data = buckets.map((b) => ({ label: formatTypeBucketLabel(b.bucket, view), count: b.count }));

  return (
    <div className={styles.wrap}>
      <div className={styles.head}>
        <span className={styles.kicker}>Когда отправляли</span>
        <Segmented options={options} value={view} onChange={onViewChange} className={styles.seg} />
      </div>
      {data.length > 0 ? (
        <ResponsiveContainer width="100%" height={140}>
          <BarChart data={data} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
            <XAxis
              dataKey="label"
              tick={{ fontSize: 10, fill: "var(--text-faint)" }}
              axisLine={{ stroke: "var(--border-soft)" }}
              tickLine={false}
              interval="preserveStartEnd"
            />
            <Tooltip
              contentStyle={{
                background: "var(--surface-raised)",
                border: "1px solid var(--border)",
                borderRadius: 0,
                fontSize: 12,
              }}
              labelStyle={{ color: "var(--text)" }}
            />
            <Bar dataKey="count" fill="var(--chart-fill)" stroke="var(--chart-line)" strokeWidth={1} />
          </BarChart>
        </ResponsiveContainer>
      ) : (
        <div className={styles.empty}>Недостаточно данных для графика</div>
      )}
    </div>
  );
}
