import { Area, AreaChart, ResponsiveContainer, Tooltip, XAxis } from "recharts";
import type { ActivityBucket, Granularity } from "../../lib/types";
import { formatBucketLabel } from "../../lib/formatBucketLabel";
import styles from "./ActivityChart.module.css";

interface ActivityChartProps {
  buckets: ActivityBucket[];
  granularity: Granularity;
}

export function ActivityChart({ buckets, granularity }: ActivityChartProps) {
  const data = buckets.map((b) => ({ label: formatBucketLabel(b.bucket, granularity), count: b.count }));

  return (
    <div className={styles.wrap}>
      <ResponsiveContainer width="100%" height={220}>
        <AreaChart data={data} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
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
          <Area type="monotone" dataKey="count" stroke="var(--chart-line)" fill="var(--chart-fill)" strokeWidth={2} />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
