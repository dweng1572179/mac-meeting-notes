import { describe, expect, it } from 'vitest';
import { parseMeetingMarkdown } from './markdown';

describe('parseMeetingMarkdown', () => {
  it('returns safe headings and bullets without interpreting HTML', () => {
    expect(parseMeetingMarkdown('## Decisions\n- Pause deal\n<script>x</script>')).toEqual([
      { kind: 'heading', text: 'Decisions' },
      { kind: 'bullet', text: 'Pause deal' },
      { kind: 'paragraph', text: '<script>x</script>' },
    ]);
  });
});
