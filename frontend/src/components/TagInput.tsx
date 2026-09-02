"use client";

import { useId, useState } from "react";
import { X } from "lucide-react";

export default function TagInput({
  value,
  onChange,
  suggestions = [],
}: {
  value: string[];
  onChange: (tags: string[]) => void;
  suggestions?: string[];
}) {
  const [draft, setDraft] = useState("");
  const listId = useId();

  function commit() {
    const name = draft.trim();
    if (name && !value.includes(name)) onChange([...value, name]);
    setDraft("");
  }

  function remove(name: string) {
    onChange(value.filter((t) => t !== name));
  }

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {value.map((t) => (
        <span
          key={t}
          className="flex items-center gap-1 px-2 py-0.5 text-[11.5px]"
          style={{ background: "var(--surface-2)", color: "var(--text-secondary)" }}
        >
          #{t}
          <button type="button" onClick={() => remove(t)} aria-label={`Remove ${t} tag`}>
            <X size={10} />
          </button>
        </span>
      ))}
      <input
        list={listId}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === ",") {
            e.preventDefault();
            commit();
          } else if (e.key === "Backspace" && !draft && value.length) {
            remove(value[value.length - 1]);
          }
        }}
        onBlur={commit}
        placeholder={value.length ? "" : "Add tag…"}
        className="field px-2 py-1 text-[12px] w-24"
      />
      <datalist id={listId}>
        {suggestions.map((s) => (
          <option key={s} value={s} />
        ))}
      </datalist>
    </div>
  );
}
