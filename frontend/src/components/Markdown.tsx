"use client";

import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

// Renders markdown to React elements (raw HTML in the text stays inert);
// links open in a new tab. Styled by `.md` in globals.css.
// `onDocLink` turns links to sibling docs ("agents.md#x") into in-app navigation.
export default function Markdown({ text, onDocLink }: { text: string; onDocLink?: (slug: string) => void }) {
  return (
    <div className="md">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ href = "", ...props }) => {
            const doc = onDocLink && href.match(/^([\w-]+)\.md(#.*)?$/);
            if (doc)
              return (
                <a
                  {...props}
                  href={`#${doc[1]}`}
                  onClick={(e) => {
                    e.preventDefault();
                    onDocLink(doc[1]);
                  }}
                />
              );
            return <a {...props} href={href} target="_blank" rel="noopener noreferrer" />;
          },
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}
