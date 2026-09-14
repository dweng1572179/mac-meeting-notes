export type MeetingMarkdownBlock = {
  kind: 'heading' | 'bullet' | 'paragraph';
  text: string;
};

export function parseMeetingMarkdown(markdown: string): MeetingMarkdownBlock[] {
  return markdown.split('\n').flatMap((line): MeetingMarkdownBlock[] => {
    if (!line.trim()) return [];
    if (line.startsWith('## ')) return [{ kind: 'heading', text: line.slice(3) }];
    if (line.startsWith('- ')) return [{ kind: 'bullet', text: line.slice(2) }];
    return [{ kind: 'paragraph', text: line }];
  });
}
