import { Card, CardKicker, Skeleton } from "../ui";
import styles from "./StatTile.module.css";

interface StatTileProps {
  kicker: string;
  value?: string;
  caption?: string;
  loading?: boolean;
}

export function StatTile({ kicker, value, caption, loading }: StatTileProps) {
  return (
    <Card className={styles.tile} aria-busy={loading || undefined}>
      <CardKicker>{kicker}</CardKicker>
      {loading ? (
        <Skeleton width="60%" height="1.05em" className={styles.value} />
      ) : (
        <div className={["num", styles.value].join(" ")}>{value}</div>
      )}
      {caption && <div className={styles.caption}>{caption}</div>}
    </Card>
  );
}
