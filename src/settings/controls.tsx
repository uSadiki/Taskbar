import type { ReactNode } from "react";

/** A titled group of rows. */
export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mb-7">
      <h2 className="mb-2 text-[13px] font-semibold text-[var(--text-muted)]">{title}</h2>
      <div className="divide-y divide-[var(--border)] rounded-xl border border-[var(--border)] bg-[var(--surface-raised)]">
        {children}
      </div>
    </section>
  );
}

/** One setting: label and optional description on the left, control on the right. */
export function Row({
  label,
  description,
  children,
}: {
  label: string;
  description?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 px-4 py-3">
      <div className="min-w-0">
        <div className="font-medium">{label}</div>
        {description && (
          <div className="mt-0.5 text-[12.5px] leading-snug text-[var(--text-muted)]">{description}</div>
        )}
      </div>
      {children && <div className="flex-none">{children}</div>}
    </div>
  );
}

export function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="inline-flex rounded-lg bg-[var(--surface-sunken)] p-0.5"
    >
      {options.map((option) => {
        const selected = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={selected}
            onClick={() => onChange(option.value)}
            className={`rounded-md px-3 py-1.5 text-[13px] transition-colors focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--accent)] ${
              selected
                ? "bg-[var(--surface-raised)] font-semibold shadow-sm"
                : "text-[var(--text-muted)] hover:text-[var(--text)]"
            }`}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}

export function Toggle({
  label,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative h-6 w-11 rounded-full transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)] disabled:opacity-45 ${
        checked ? "bg-[var(--accent)]" : "bg-[var(--surface-sunken)]"
      }`}
    >
      <span
        className={`absolute top-0.5 left-0.5 h-5 w-5 rounded-full bg-white shadow transition-transform ${
          checked ? "translate-x-5" : ""
        }`}
      />
    </button>
  );
}

export function Notice({ children }: { children: ReactNode }) {
  return (
    <div
      role="status"
      className="mb-5 rounded-xl bg-[var(--warning-bg)] px-4 py-3 text-[13px] leading-snug text-[var(--warning-text)]"
    >
      {children}
    </div>
  );
}

export function Button({
  children,
  onClick,
  disabled,
  label,
}: {
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  label?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      className="rounded-lg border border-[var(--border)] bg-[var(--surface-raised)] px-3 py-1.5 text-[13px] font-medium hover:bg-[var(--surface-sunken)] focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--accent)] disabled:opacity-40"
    >
      {children}
    </button>
  );
}
