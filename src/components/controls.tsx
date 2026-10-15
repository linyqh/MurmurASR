import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { IconChevronUpDown } from "./icons";

export function Switch({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className="switch"
      disabled={disabled}
      onClick={() => onChange(!checked)}
    />
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label: ReactNode }[];
  onChange: (value: T) => void;
  label: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [thumb, setThumb] = useState({ left: 2, width: 0 });

  useLayoutEffect(() => {
    const index = options.findIndex((o) => o.value === value);
    const button = ref.current?.querySelectorAll("button")[index];
    if (button) setThumb({ left: button.offsetLeft, width: button.offsetWidth });
  }, [value, options]);

  return (
    <div className="segmented" role="radiogroup" aria-label={label} ref={ref}>
      <div className="thumb" style={{ left: thumb.left, width: thumb.width }} />
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          className={o.value === value ? "active" : ""}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Select({
  value,
  options,
  onChange,
  label,
  disabled,
}: {
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <span className="select">
      <select
        aria-label={label}
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <IconChevronUpDown size={11} className="chevron" />
    </span>
  );
}

export function Progress({ value, indeterminate }: { value: number; indeterminate?: boolean }) {
  return (
    <div
      className={`progress${indeterminate ? " indeterminate" : ""}`}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={indeterminate ? undefined : Math.round(value * 100)}
    >
      <div style={{ width: `${Math.max(0, Math.min(1, value)) * 100}%` }} />
    </div>
  );
}

export function Meter({ level, max = 5 }: { level: number; max?: number }) {
  return (
    <span className="meter" aria-label={`${level}/${max}`}>
      {Array.from({ length: max }, (_, i) => (
        <i key={i} className={i < level ? "on" : ""} />
      ))}
    </span>
  );
}

export function Sheet({ children, onClose }: { children: ReactNode; onClose: () => void }) {
  return (
    <div
      className="sheet-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      onKeyDown={(e) => e.key === "Escape" && onClose()}
    >
      <div className="sheet glass" role="dialog" aria-modal="true">
        {children}
      </div>
    </div>
  );
}
