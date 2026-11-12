/* Composants du design system Volta, portés en React + TypeScript. */
import { useEffect, useId, useRef, type ReactNode, type ButtonHTMLAttributes, type KeyboardEvent } from "react";
import { X } from "lucide-react";

const cx = (...c: (string | false | null | undefined)[]) => c.filter(Boolean).join(" ");

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "outline" | "ghost" | "danger";
  size?: "md" | "sm";
  block?: boolean;
  icon?: ReactNode;
};
export function Button({ variant = "primary", size, block, icon, className, children, ...rest }: ButtonProps) {
  const isBlock = block ?? (variant === "primary" || variant === "outline");
  return (
    <button type="button" {...rest} className={cx("vt-btn", `vt-btn-${variant}`, size === "sm" && "vt-btn-sm", isBlock && "vt-btn-block", className)}>
      {icon}
      {children}
    </button>
  );
}
export const ButtonGroup = ({ children }: { children: ReactNode }) => (
  <div className="vt-btn-group" role="group">{children}</div>
);

export function StatTile(p: { label: string; value: ReactNode; unit?: string; note?: ReactNode; tone?: "good" | "bad" }) {
  return (
    <div className="vt-stat">
      <div className="vt-stat-label">{p.label}</div>
      <div className="vt-stat-value">
        {p.value}
        {p.unit && <span className="vt-stat-unit">{p.unit}</span>}
      </div>
      {p.note && <div className={cx("vt-stat-note", p.tone === "good" && "is-good", p.tone === "bad" && "is-bad")}>{p.note}</div>}
    </div>
  );
}

export const List = ({ children }: { children: ReactNode }) => <div className="vt-list">{children}</div>;
export function ListRow(p: { title: ReactNode; subtitle?: ReactNode; trailing?: ReactNode; leading?: ReactNode; onClick?: () => void }) {
  const onKey = (e: KeyboardEvent) => {
    if (p.onClick && (e.key === "Enter" || e.key === " ")) {
      e.preventDefault();
      p.onClick();
    }
  };
  return (
    <div className="vt-li" onClick={p.onClick} onKeyDown={onKey} role={p.onClick ? "button" : undefined} tabIndex={p.onClick ? 0 : undefined}>
      {p.leading}
      <div className="vt-li-main">
        <div className="vt-li-title">{p.title}</div>
        {p.subtitle && <div className="vt-li-sub">{p.subtitle}</div>}
      </div>
      {p.trailing != null && <div className="vt-li-trail">{p.trailing}</div>}
    </div>
  );
}

export function Segmented<T extends string>(p: {
  options: { value: T; label: string }[];
  value: T;
  onChange: (v: T) => void;
  label: string;
  hint?: string;
  hideLabel?: boolean;
}) {
  const seg = (
    <div className="vt-seg" role="group" aria-label={p.label}>
      {p.options.map((o) => (
        <button key={o.value} type="button" aria-pressed={o.value === p.value} onClick={() => p.onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
  if (p.hideLabel) return seg;
  return (
    <div className="vt-seg-field">
      <div className="vt-seg-title">{p.label}</div>
      {p.hint && <div className="vt-seg-hint">{p.hint}</div>}
      {seg}
    </div>
  );
}

export const Badge = ({ tone, children }: { tone?: "electric" | "fuel" | "charge"; children: ReactNode }) => (
  <span className={cx("vt-badge", tone && `vt-badge-${tone}`)}>{children}</span>
);

export function Field(p: { label: string; hint?: ReactNode; children: (id: string) => ReactNode }) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id}>{p.label}</label>
      {p.children(id)}
      {p.hint && <div className="hint">{p.hint}</div>}
    </div>
  );
}

/** Feuille modale : monte du bas sur mobile, centrée sur ordinateur. Échap ferme. */
export function Sheet(p: { title: string; onClose: () => void; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    ref.current?.querySelector<HTMLElement>("input, select, textarea, button")?.focus();
    const onKey = (e: globalThis.KeyboardEvent) => e.key === "Escape" && p.onClose();
    window.addEventListener("keydown", onKey);
    document.body.style.overflow = "hidden";
    return () => {
      window.removeEventListener("keydown", onKey);
      document.body.style.overflow = "";
      prev?.focus();
    };
  }, [p.onClose]);
  return (
    <div className="sheet-backdrop" onMouseDown={(e) => e.target === e.currentTarget && p.onClose()}>
      <div className="sheet" role="dialog" aria-modal="true" aria-label={p.title} ref={ref}>
        <div className="sheet-grip" />
        <div className="sheet-head">
          <h2>{p.title}</h2>
          <button className="iconbtn" onClick={p.onClose} aria-label="Fermer">
            <X size={22} strokeWidth={1.5} />
          </button>
        </div>
        {p.children}
      </div>
    </div>
  );
}
