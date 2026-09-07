import type { ButtonHTMLAttributes } from "react";
import styles from "./Button.module.css";

type Variant = "primary" | "secondary" | "ghost";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
}

export function Button({ variant = "secondary", className, ...rest }: ButtonProps) {
  const variantClass = { primary: styles.primary, secondary: styles.secondary, ghost: styles.ghost }[variant];
  return <button className={[styles.btn, variantClass, className].filter(Boolean).join(" ")} {...rest} />;
}
