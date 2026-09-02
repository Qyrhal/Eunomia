"use client";

function pad(n: number) {
  return String(n).padStart(2, "0");
}

// datetime-local wants "YYYY-MM-DDTHH:mm" in local time, with no timezone offset.
function toLocalInput(iso: string): string {
  if (!iso) return "";
  const d = new Date(iso);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function fromLocalInput(local: string): string {
  return local ? new Date(local).toISOString() : "";
}

function atTime(base: Date, hours: number, minutes = 0): Date {
  const d = new Date(base);
  d.setHours(hours, minutes, 0, 0);
  return d;
}

function daysFromNow(days: number): Date {
  return new Date(Date.now() + days * 86_400_000);
}

const QUICK_PICKS: { label: string; resolve: () => Date }[] = [
  { label: "Today", resolve: () => atTime(new Date(), 9) },
  { label: "Tomorrow", resolve: () => atTime(daysFromNow(1), 9) },
  {
    label: "This weekend",
    resolve: () => {
      const daysUntilSat = (6 - new Date().getDay() + 7) % 7 || 7;
      return atTime(daysFromNow(daysUntilSat), 10);
    },
  },
  { label: "Next week", resolve: () => atTime(daysFromNow(7), 9) },
];

export default function DueDatePicker({ value, onChange }: { value: string; onChange: (iso: string) => void }) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex flex-wrap gap-1.5">
        {QUICK_PICKS.map((q) => (
          <button
            key={q.label}
            type="button"
            onClick={() => onChange(q.resolve().toISOString())}
            className="px-2.5 py-1 text-[11.5px]"
            style={{ background: "var(--surface-2)", color: "var(--text-secondary)" }}
          >
            {q.label}
          </button>
        ))}
        {value && (
          <button
            type="button"
            onClick={() => onChange("")}
            className="px-2.5 py-1 text-[11.5px] underline"
            style={{ color: "var(--text-muted)" }}
          >
            Clear
          </button>
        )}
      </div>
      <div className="flex items-center gap-2 flex-wrap">
        <input
          type="datetime-local"
          value={toLocalInput(value)}
          onChange={(e) => onChange(fromLocalInput(e.target.value))}
          className="field px-2.5 py-1.5 text-[12.5px] font-mono"
        />
        {value && (
          <span className="text-[11.5px] font-mono" style={{ color: "var(--text-muted)" }}>
            {new Date(value).toLocaleString(undefined, {
              weekday: "short",
              month: "short",
              day: "numeric",
              hour: "numeric",
              minute: "2-digit",
            })}
          </span>
        )}
      </div>
    </div>
  );
}
