import styles from "./Segmented.module.css";

interface SegmentedOption<T extends string> {
  value: T;
  label: string;
}

interface SegmentedProps<T extends string> {
  options: SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
  className?: string;
}

export function Segmented<T extends string>({ options, value, onChange, className }: SegmentedProps<T>) {
  return (
    <div className={[styles.seg, className].filter(Boolean).join(" ")} role="radiogroup">
      {options.map((opt) => (
        <button
          key={opt.value}
          type="button"
          role="radio"
          aria-checked={opt.value === value}
          className={[styles.opt, opt.value === value ? styles.on : ""].filter(Boolean).join(" ")}
          onClick={() => onChange(opt.value)}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}
