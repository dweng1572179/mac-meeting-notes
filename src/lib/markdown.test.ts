import { describe, expect, it } from 'vitest';
import { parseAnswerMarkdown, parseMeetingMarkdown, markdownInlines } from './markdown';

it('groups answer paragraphs and real lists without interpreting source HTML', () => {
  expect(parseAnswerMarkdown('## Water cycle\n\nWater moves\nbetween states.\n\n- Evaporation\n* Condensation\n\n3. Observe\n4. Compare\n\n<img src=x>')).toEqual([
    { kind: 'heading', text: 'Water cycle' },
    { kind: 'paragraph', text: 'Water moves between states.' },
    { kind: 'list', ordered: false, start: 1, items: ['Evaporation', 'Condensation'] },
    { kind: 'list', ordered: true, start: 3, items: ['Observe', 'Compare'] },
    { kind: 'paragraph', text: '<img src=x>' }
  ]);
});

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
