export type Platform = 'mac' | 'windows' | 'other';

export function platformFromUserAgent(userAgent: string): Platform {
  if (/Windows/i.test(userAgent)) return 'windows';
  if (/Macintosh|Mac OS X/i.test(userAgent)) return 'mac';
  return 'other';
}

export function currentPlatform(): Platform {
  return platformFromUserAgent(typeof navigator === 'undefined' ? '' : navigator.userAgent);
}

export const questionShortcut = (platform = currentPlatform()) => platform === 'mac' ? '⌘ Enter' : 'Ctrl Enter';
