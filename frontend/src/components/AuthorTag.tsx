/** Figma-style multiplayer name tag: who wrote a memory, record or edit.
 * Pass the real author (an owner email, an agent/client name, or a source
 * key). Never invent one. Colour is stable per author name. */

const AUTHOR_COLORS = 6;

export function authorColor(name: string): string {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  return `var(--author-${(h % AUTHOR_COLORS) + 1})`;
}

export function authorLabel(name: string): string {
  return name.includes("@") ? name.split("@")[0] : name;
}

export function CursorGlyph({ color, size = 14 }: { color: string; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden style={{ flexShrink: 0 }}>
      <path d="M2 1.5 13.5 7 8.3 8.4 6 13.8Z" fill={color} stroke="var(--canvas)" strokeWidth="1" strokeLinejoin="round" />
    </svg>
  );
}

export default function AuthorTag({ name, cursor = false, title }: { name: string; cursor?: boolean; title?: string }) {
  const color = authorColor(name);
  const tag = (
    <span className="author-tag" style={{ ["--author" as string]: color }} title={title ?? name}>
      <span className="truncate">{authorLabel(name)}</span>
    </span>
  );
  if (!cursor) return tag;
  return (
    <span className="inline-flex items-start gap-0.5">
      <CursorGlyph color={color} />
      <span className="mt-2">{tag}</span>
    </span>
  );
}
