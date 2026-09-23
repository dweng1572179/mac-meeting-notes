import { expect, it } from 'vitest';
import { completeSetup, shouldOfferSetup } from './onboarding';

it('offers optional setup only to an empty profile without a key or prior dismissal', () => {
  const values = new Map<string, string>();
  const storage = () => ({ getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } });
  expect(shouldOfferSetup(0, false, storage)).toBe(true);
  expect(shouldOfferSetup(1, false, storage)).toBe(false);
  expect(shouldOfferSetup(0, true, storage)).toBe(false);
  expect(completeSetup(storage)).toBe(true);
  expect(shouldOfferSetup(0, false, storage)).toBe(false);
});

it('does not break local use when browser storage is unavailable', () => {
  const unavailable = () => { throw new Error('Storage unavailable'); };
  expect(shouldOfferSetup(0, false, unavailable)).toBe(true);
  expect(shouldOfferSetup(2, false, unavailable)).toBe(false);
  expect(completeSetup(unavailable)).toBe(false);
});
