import type { HTMLAttributes } from "react";
import styles from "./Tag.module.css";

type Variant = "accent" | "neutral" | "outline";

interface TagProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: Variant;
}

export function Tag({ variant = "neutral", className, ...rest }: TagProps) {
  const variantClass = { accent: styles.accent, neutral: styles.neutral, outline: styles.outline }[variant];
  return <span className={[styles.tag, variantClass, className].filter(Boolean).join(" ")} {...rest} />;
}
