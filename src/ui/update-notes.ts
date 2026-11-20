export interface UpdateHighlight {
  title: string;
  detail: string;
}

interface UpdateSections {
  highlights: UpdateHighlight[];
  fixes: UpdateHighlight[];
}

function firstSentence(text: string): string {
  // Match through inline formatting, so punctuation inside a code span or
  // emphasis does not end the preview halfway through that span.
  const sentence =
    text.match(
      /^(?:`[^`\n]+`|\*\*[^*\n]+\*\*|[^。！？.!?]|[.!?](?!\s|$))*(?:[。！？.!?]|$)/,
    )?.[0] ?? text;
  if (sentence.length <= 220) return sentence;
  const clipped = Array.from(sentence.replace(/\*\*|`/g, ""))
    .slice(0, 219)
    .join("");
  const boundary = clipped.lastIndexOf(" ");
  return `${clipped.slice(0, boundary > 160 ? boundary : clipped.length)}…`;
}

function readBullets(content: string, allowPlainText = false): UpdateHighlight[] {
  const bullets: string[] = [];
  for (const line of content.split(/\r?\n/)) {
    if (line.startsWith("- ")) bullets.push(line.slice(2));
    else if (/^\s+\S/.test(line) && bullets.length) {
      bullets[bullets.length - 1] += ` ${line.trim()}`;
    }
  }
  if (allowPlainText && !bullets.length && content.trim() && !/^\s*[#<|]/.test(content)) {
    bullets.push(content.split(/\r?\n\s*\r?\n/)[0]);
  }
  return bullets.flatMap((bullet) => {
    const text = bullet
      .replace(/\s*\[#\d+\]\([^)]*\)/g, "")
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
      .replace(/\s*\(#\d+(?:,?\s*#\d+)*\)/g, "")
      .replace(/^[^\p{L}\p{N}*`<]+/u, "")
      .replace(/\s+/g, " ")
      .trim();
    if (!text) return [];
    const bold = text.match(/^\*\*(.+?)\*\*(?:\s+(.*)|$)/);
    return [{ title: bold?.[1] ?? "", detail: bold ? (bold[2] ?? "") : text }];
  });
}

function readUpdateSections(notes: string | null): UpdateSections {
  if (!notes?.trim()) return { highlights: [], fixes: [] };
  const sections = notes.split(/^##\s+/m);
  const readSection = (heading: RegExp) => {
    const section = sections.slice(1).find((part) => heading.test(part));
    return section ? readBullets(section.slice(section.indexOf("\n") + 1)) : [];
  };
  return {
    highlights:
      sections.length === 1
        ? readBullets(notes, true)
        : readSection(/^(?:(?:✨\s*)?Highlights|主要更新)\s*\r?\n/),
    fixes: readSection(/^(?:Fixes and improvements|修复与改进)\s*\r?\n/),
  };
}

export function getUpdateNotes(notes: string | null): {
  preview: UpdateHighlight[];
  more: UpdateHighlight[];
  fixes: UpdateHighlight[];
} {
  const { highlights, fixes } = readUpdateSections(notes);
  const primary = highlights.length ? highlights : fixes;
  const preview = primary.slice(0, 3).map(({ title, detail }) => ({
    title: firstSentence(title),
    detail: firstSentence(detail),
  }));
  const more = primary.flatMap((item, index) => {
    const shown = preview[index];
    if (!shown) return [item];
    if (shown.title === item.title && shown.detail === item.detail) return [];
    if (shown.title === item.title && item.detail.startsWith(shown.detail)) {
      return [{ title: item.title, detail: item.detail.slice(shown.detail.length).trimStart() }];
    }
    return [item];
  });
  return { preview, more, fixes: highlights.length ? fixes : [] };
}

/** Keep the existing string field and known headings for clients already in the field. */
export function formatUpdateFeedNotes(notes: string): string {
  const { highlights, fixes } = readUpdateSections(notes);
  return [["Highlights", highlights] as const, ["Fixes and improvements", fixes] as const]
    .filter(([, items]) => items.length)
    .map(([heading, items]) => {
      const list = items.map(({ title, detail }) =>
        `- ${title ? `**${title}** ` : ""}${detail}`.trimEnd(),
      );
      return [`## ${heading}`, ...list].join("\n");
    })
    .join("\n\n");
}
