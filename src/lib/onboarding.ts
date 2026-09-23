const setupKey = 'meeting-notes.setup-complete.v1';
type SetupStorage = Pick<Storage, 'getItem' | 'setItem'>;
const browserStorage = () => window.localStorage;

export function shouldOfferSetup(sessionCount: number, hasApiKey: boolean, storage: () => SetupStorage = browserStorage): boolean {
  if (sessionCount || hasApiKey) return false;
  try { return storage().getItem(setupKey) !== 'true'; }
  catch { return true; }
}

export function completeSetup(storage: () => SetupStorage = browserStorage): boolean {
  try { storage().setItem(setupKey, 'true'); return true; }
  catch { return false; }
}
