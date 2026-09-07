import { Card, CardKicker } from "../ui";
import styles from "./StatTile.module.css";

interface StatTileProps {
  kicker: string;
  value: string;
  caption?: string;
}

export function StatTile({ kicker, value, caption }: StatTileProps) {
  return (
    <Card className={styles.tile}>
      <CardKicker>{kicker}</CardKicker>
      <div className={["num", styles.value].join(" ")}>{value}</div>
      {caption && <div className={styles.caption}>{caption}</div>}
    </Card>
  );
}
