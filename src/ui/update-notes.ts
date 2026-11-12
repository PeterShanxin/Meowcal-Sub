export interface UpdateHighlight {
  title: string;
  detail: string;
}

function firstSentence(text: string): string {
  const sentence = text.match(/^.*?(?:[.!?](?=\s|$)|$)/)?.[0] ?? "";
  if (sentence.length <= 220) return sentence;
  const clipped = sentence.slice(0, 219);
  const boundary = clipped.lastIndexOf(" ");
  return `${clipped.slice(0, boundary > 0 ? boundary : clipped.length)}…`;
}

/** Shared by release packaging and the UI, including clients reading older full notes. */
export function summarizeUpdateNotes(notes: string | null): UpdateHighlight[] {
  if (!notes?.trim()) return [];
  const sections = notes.split(/^##\s+/m);
  const section =
    sections.find((part) => /^(?:✨\s*)?Highlights\s*\r?\n/.test(part)) ??
    sections.find((part) => /^Fixes and improvements\s*\r?\n/.test(part));
  const content = section
    ? section.slice(section.indexOf("\n") + 1)
    : sections.length === 1
      ? notes
      : "";
  const bullets: string[] = [];
  for (const line of content.split(/\r?\n/)) {
    if (line.startsWith("- ")) bullets.push(line.slice(2));
    else if (/^\s+\S/.test(line) && bullets.length)
      bullets[bullets.length - 1] += ` ${line.trim()}`;
  }
  if (!bullets.length && content.trim() && !/^\s*[#<|]/.test(content)) {
    bullets.push(content.split(/\r?\n\s*\r?\n/)[0]);
  }
  return bullets.slice(0, 3).map((bullet) => {
    const text = bullet
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
      .replace(/\s*\(#\d+(?:,?\s*#\d+)*\)/g, "")
      .replace(/`/g, "")
      .replace(/^[^\p{L}\p{N}*<]+/u, "")
      .replace(/\s+/g, " ")
      .trim();
    const bold = text.match(/^\*\*(.+?)\*\*\s*(.*)$/);
    return bold
      ? { title: firstSentence(bold[1]), detail: firstSentence(bold[2]) }
      : { title: "", detail: firstSentence(text.replace(/\*\*/g, "")) };
  });
}
