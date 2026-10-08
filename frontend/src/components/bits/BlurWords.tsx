// Inspired by React Bits BlurText (reactbits.dev). Original implementation for Eunomia.
// Usage: <BlurWords text="Welcome to Eunomia" as="h1" className="page-title" /> for a rare first-run heading only.
// Each word settles from a 4px blur once on mount (420ms, 45ms stagger: a first-run moment, so longer than UI motion). Text and name are unchanged.
import { Fragment } from "react";
import "./bits.css";

type Tag = "h1" | "h2" | "h3" | "p" | "span" | "div";

export default function BlurWords({ text, as: As = "span", className }: { text: string; as?: Tag; className?: string }) {
  const words = text.split(" ");
  return (
    <As className={className}>
      {words.map((w, i) => (
        <Fragment key={i}>
          {i > 0 && " "}
          <span className="bits-word" style={{ ["--i" as string]: i }}>
            {w}
          </span>
        </Fragment>
      ))}
    </As>
  );
}
