/** A ring chart: one SVG arc out of `value`/`max`, with an optional big
 * numeral + label centered inside. Every ring here encodes a real metric —
 * there's no decorative use of this component. */
export default function StatRing({
  value,
  max,
  size = 96,
  strokeWidth = 8,
  color = "var(--signal)",
  trackColor = "var(--surface-raised)",
  valueLabel,
  label,
  ariaLabel,
}: {
  value: number;
  max: number;
  size?: number;
  strokeWidth?: number;
  color?: string;
  trackColor?: string;
  valueLabel?: string;
  label?: string;
  ariaLabel: string;
}) {
  const fraction = max > 0 ? Math.max(0, Math.min(1, value / max)) : 0;
  const r = size / 2 - strokeWidth / 2;
  const c = size / 2;
  const dash = Math.round(fraction * 1000) / 10; // 0-100, 1dp

  return (
    <div
      role="img"
      aria-label={ariaLabel}
      style={{ width: size, height: size, position: "relative", display: "inline-flex" }}
    >
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden="true">
        <circle cx={c} cy={c} r={r} fill="none" stroke={trackColor} strokeWidth={strokeWidth} />
        {dash > 0 && (
          <circle
            className="stat-ring-arc"
            cx={c}
            cy={c}
            r={r}
            fill="none"
            stroke={color}
            strokeWidth={strokeWidth}
            strokeLinecap="round"
            pathLength={100}
            strokeDasharray={`${dash} ${100 - dash}`}
            transform={`rotate(-90 ${c} ${c})`}
          />
        )}
      </svg>
      {(valueLabel || label) && (
        <div
          aria-hidden="true"
          style={{
            position: "absolute",
            inset: 0,
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            textAlign: "center",
            padding: strokeWidth,
          }}
        >
          {valueLabel && (
            <span
              className="font-display"
              style={{ color: "var(--ink)", fontWeight: 600, lineHeight: 1, fontSize: size * 0.22 }}
            >
              {valueLabel}
            </span>
          )}
          {label && (
            <span
              className="font-mono"
              style={{ color: "var(--ink-dim)", fontSize: Math.max(9, size * 0.075), marginTop: 4 }}
            >
              {label}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
