/** The signature mark: a twelve-tick dial, after the Horae — Eunomia's sisters
 * and the keepers of the hours and seasons. Used as the wordmark glyph, a
 * watermark behind the completion figure, and the shape of a checked item. */
// Fixed to 3dp: Math.cos/sin can differ in the last float digit between the
// server and browser JS engines, which is enough to fail SSR hydration.
const round = (n: number) => Math.round(n * 1000) / 1000;

export default function HourRing({ size = 20, ticks = 12, color = "currentColor" }: { size?: number; ticks?: number; color?: string }) {
  const r = size / 2;
  const tickInner = r * 0.66;
  const tickOuter = r * 0.86;
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} fill="none">
      <circle cx={r} cy={r} r={r - 1} stroke={color} strokeWidth={1.3} opacity={0.9} />
      {Array.from({ length: ticks }).map((_, i) => {
        const angle = (i / ticks) * Math.PI * 2 - Math.PI / 2;
        const x1 = round(r + Math.cos(angle) * tickInner);
        const y1 = round(r + Math.sin(angle) * tickInner);
        const x2 = round(r + Math.cos(angle) * tickOuter);
        const y2 = round(r + Math.sin(angle) * tickOuter);
        return <line key={i} x1={x1} y1={y1} x2={x2} y2={y2} stroke={color} strokeWidth={i % 3 === 0 ? 1.4 : 0.8} />;
      })}
      <circle cx={r} cy={r} r={1.4} fill={color} />
    </svg>
  );
}
