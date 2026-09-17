import { describe, expect, it } from 'vitest';
import { parseMeetingMarkdown, markdownInlines } from './markdown';

describe('parseMeetingMarkdown', () => {
  it('returns safe headings and bullets without interpreting HTML', () => {
    expect(parseMeetingMarkdown('## Decisions\n- Pause deal\n<script>x</script>')).toEqual([
      { kind: 'heading', text: 'Decisions' },
      { kind: 'bullet', text: 'Pause deal' },
      { kind: 'paragraph', text: '<script>x</script>' },
    ]);
  });

  it('recognizes heading levels and emphasis while keeping HTML as literal text', () => {
    expect(parseMeetingMarkdown('### Next steps')[0]).toEqual({ kind: 'heading', text: 'Next steps' });
    expect(markdownInlines('Review **the decision** and `<img onerror=alert(1)>`.')).toEqual([
      { kind: 'text', text: 'Review ' },
      { kind: 'strong', text: 'the decision' },
      { kind: 'text', text: ' and ' },
      { kind: 'code', text: '<img onerror=alert(1)>' },
      { kind: 'text', text: '.' }
    ]);
    expect(markdownInlines('Unclosed **words')).toEqual([{ kind: 'text', text: 'Unclosed **words' }]);
  });
});
