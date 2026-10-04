/** A small abstracted suit-derived mark — two soft lobes tapering to a point,
 * on a short stem. Used sparingly as the system icon for "connector"/"source"
 * concepts; never a literal playing-card glyph, and never a page-wide motif. */
export default function SuitMark({
  size = 14,
  color = "currentColor",
}: {
  size?: number;
  color?: string;
  /** accepted for drop-in parity with lucide icons; this mark is a solid shape. */
  strokeWidth?: number;
}) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <path
        d="M8 1.5c2.6 2.6 5 4.9 5 7.3a3.4 3.4 0 0 1-5 3 3.4 3.4 0 0 1-5-3c0-2.4 2.4-4.7 5-7.3Z"
        fill={color}
      />
      <path d="M8 11.2v2.3M6.3 14.5h3.4" stroke={color} strokeWidth={1.1} strokeLinecap="round" />
    </svg>
  );
}
