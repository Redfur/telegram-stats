import type { InputHTMLAttributes, LabelHTMLAttributes, ReactNode } from "react";
import styles from "./Input.module.css";

export function Input({ className, ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={[styles.input, className].filter(Boolean).join(" ")} {...rest} />;
}

export function InputLabel({ className, ...rest }: LabelHTMLAttributes<HTMLLabelElement> & { children: ReactNode }) {
  return <label className={[styles.label, className].filter(Boolean).join(" ")} {...rest} />;
}
