import { expect, it } from 'vitest';
import { platformFromUserAgent, questionShortcut } from './platform';
import { errorMessage } from './recovery';

it('uses the appropriate question shortcut without a native plugin', () => {
  expect(platformFromUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64)')).toBe('windows');
  expect(platformFromUserAgent('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)')).toBe('mac');
  expect(questionShortcut('windows')).toBe('Ctrl Enter');
  expect(questionShortcut('mac')).toBe('⌘ Enter');
});

it('keeps Windows microphone failures out of macOS System Settings', () => {
  const message = errorMessage({ code: 'microphone_permission', message: 'Access denied.' }, undefined, 'windows');
  expect(message).toContain('Microphone');
  expect(message).toContain('desktop apps');
  expect(message).not.toContain('System Settings');
  expect(errorMessage({ code: 'audio_permission', message: 'Access denied.' }, undefined, 'windows')).not.toContain('Screen & System Audio Recording');
});
