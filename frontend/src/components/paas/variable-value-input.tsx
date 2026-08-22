"use client";

import { useMemo, useState } from "react";
import { Input } from "@/components/ui/input";

export interface LinkSuggestionSource {
  slug: string;
  keys: string[];
}

/**
 * Finds the reference the user is in the middle of typing, if any: text
 * after the last `$`, with a leading `{{` (or a lone `{`, mid-typing)
 * stripped. Returns `null` when there's nothing to suggest — no `$` at all,
 * or the last one is already part of a closed `${{...}}`.
 */
function activeQuery(value: string): string | null {
  const idx = value.lastIndexOf("$");
  if (idx === -1) return null;
  let after = value.slice(idx + 1);
  if (after.startsWith("{{")) {
    after = after.slice(2);
    if (after.includes("}}")) return null;
    return after;
  }
  if (after.startsWith("{")) return after.slice(1);
  return after;
}

/** Single-line value input with a `${{service.KEY}}` link autocomplete, triggered by typing `$`. */
export function VariableValueInput({
  value,
  onChange,
  suggestions,
  placeholder,
  id,
}: {
  value: string;
  onChange: (v: string) => void;
  suggestions: LinkSuggestionSource[];
  placeholder?: string;
  id?: string;
}) {
  const [focused, setFocused] = useState(false);

  const candidates = useMemo(
    () =>
      suggestions.flatMap((s) =>
        s.keys.map((k) => ({ slug: s.slug, key: k, ref: `\${{${s.slug}.${k}}}` })),
      ),
    [suggestions],
  );

  const query = activeQuery(value);
  const filtered =
    query === null
      ? []
      : candidates
          .filter((c) => `${c.slug}.${c.key}`.toLowerCase().includes(query.toLowerCase()))
          .slice(0, 8);

  const open = focused && query !== null && filtered.length > 0;

  return (
    <div className="relative flex-1">
      <Input
        id={id}
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onFocus={() => setFocused(true)}
        onBlur={() => setTimeout(() => setFocused(false), 100)}
      />
      {open && (
        <div className="absolute left-0 right-0 top-full z-20 mt-1 max-h-48 overflow-y-auto rounded-md border border-border bg-surface shadow-2xl">
          {filtered.map((c) => (
            <button
              key={c.ref}
              type="button"
              onMouseDown={(e) => {
                e.preventDefault();
                onChange(c.ref);
                setFocused(false);
              }}
              className="flex w-full flex-col items-start px-3 py-1.5 text-left hover:bg-surface-2"
            >
              <span className="font-mono text-xs text-ink">{c.ref}</span>
              <span className="text-[10px] text-muted">
                from <span className="text-primary">{c.slug}</span>
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
