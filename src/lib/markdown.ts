export type MeetingMarkdownBlock = {
  kind: 'heading' | 'bullet' | 'paragraph';
  text: string;
};

export function parseMeetingMarkdown(markdown: string): MeetingMarkdownBlock[] {
  return markdown.split('\n').flatMap((line): MeetingMarkdownBlock[] => {
    if (!line.trim()) return [];
    const heading = line.match(/^#{1,6}\s+(.+)$/u);
    if (heading) return [{ kind: 'heading', text: heading[1] }];
    if (line.startsWith('- ')) return [{ kind: 'bullet', text: line.slice(2) }];
    return [{ kind: 'paragraph', text: line }];
  });
}

export function markdownInlines(text: string) {
  const parts: { kind: 'text' | 'strong' | 'code'; text: string }[] = [];
  const pattern = /\*\*([^*]+)\*\*|__([^_]+)__|`([^`]+)`/gu;
  let cursor = 0;
  for (const match of text.matchAll(pattern)) {
    if (match.index > cursor) parts.push({ kind: 'text', text: text.slice(cursor, match.index) });
    parts.push({ kind: match[3] === undefined ? 'strong' : 'code', text: match[1] ?? match[2] ?? match[3] });
    cursor = match.index + match[0].length;
  }
  if (cursor < text.length) parts.push({ kind: 'text', text: text.slice(cursor) });
  return parts;
}
