/** Eunomia's mark: three scattered points resolving into one -- the product
 * in miniature (disparate sources -> one connected memory), not a stock
 * network-node icon. Intentionally asymmetric, not a neat trefoil. */
export default function EunomiaMark({ size = 20, color = "var(--felt)" }: { size?: number; color?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden>
      <line x1="12" y1="12" x2="5" y2="7" stroke={color} strokeWidth="1.4" strokeLinecap="round" opacity="0.55" />
      <line x1="12" y1="12" x2="19" y2="9" stroke={color} strokeWidth="1.4" strokeLinecap="round" opacity="0.55" />
      <line x1="12" y1="12" x2="9.5" y2="19.5" stroke={color} strokeWidth="1.4" strokeLinecap="round" opacity="0.55" />
      <circle cx="5" cy="7" r="1.6" fill={color} opacity="0.55" />
      <circle cx="19" cy="9" r="1.3" fill={color} opacity="0.55" />
      <circle cx="9.5" cy="19.5" r="1.9" fill={color} opacity="0.55" />
      <circle cx="12" cy="12" r="3" fill={color} />
    </svg>
  );
}
