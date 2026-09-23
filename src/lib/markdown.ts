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

export type AnswerMarkdownBlock =
  | { kind: 'heading' | 'paragraph'; text: string }
  | { kind: 'list'; ordered: boolean; start: number; items: string[] };

export function parseAnswerMarkdown(markdown: string): AnswerMarkdownBlock[] {
  const blocks: AnswerMarkdownBlock[] = [];
  let paragraph: string[] = [];
  let list: Extract<AnswerMarkdownBlock, { kind: 'list' }> | undefined;
  const flush = () => {
    if (paragraph.length) blocks.push({ kind: 'paragraph', text: paragraph.join(' ') });
    paragraph = [];
    list = undefined;
  };
  for (const line of markdown.split('\n')) {
    const text = line.trim();
    if (!text) { flush(); continue; }
    const heading = text.match(/^#{1,6}\s+(.+)$/u);
    if (heading) { flush(); blocks.push({ kind: 'heading', text: heading[1] }); continue; }
    const item = text.match(/^(?:([-*+])\s+|(\d+)[.)]\s+)(.+)$/u);
    if (item) {
      const ordered = item[2] !== undefined;
      if (!list || list.ordered !== ordered) {
        flush();
        list = { kind: 'list', ordered, start: ordered ? Number(item[2]) : 1, items: [] };
        blocks.push(list);
      }
      list.items.push(item[3]);
    } else if (list && /^\s{2,}\S/u.test(line)) {
      list.items[list.items.length - 1] += ` ${text}`;
    } else {
      list = undefined;
      paragraph.push(text);
    }
  }
  flush();
  return blocks;
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
