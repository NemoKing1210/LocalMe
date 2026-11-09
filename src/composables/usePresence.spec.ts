// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';

import { usePresence } from './usePresence';

/**
 * The sentence the list, the conversation header and the settings screen share: one place, so
 * the three cannot drift apart in wording or in plural form.
 */
describe('usePresence', () => {
  it('says the person is online while they are', () => {
    const presence = usePresence();

    expect(presence({ online: true, lastSeenMs: 1_000 }, 9_000)).toBe('online');
  });

  it('says a person who has never been seen has never been seen', () => {
    const presence = usePresence();

    expect(presence({ online: false, lastSeenMs: null }, 9_000)).toBe('never online');
  });

  it('ages the last-seen sentence against the clock it is given', () => {
    const presence = usePresence();
    const seen = 1_000;

    // The caller owns the ticking clock: the same row says different things a minute later.
    expect(presence({ online: false, lastSeenMs: seen }, seen + 30_000)).toBe(
      'last seen 30 seconds ago',
    );
    expect(presence({ online: false, lastSeenMs: seen }, seen + 3_600_000)).toBe(
      'last seen 1 hour ago',
    );
  });
});
