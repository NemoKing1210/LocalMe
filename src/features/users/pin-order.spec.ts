import { describe, expect, it } from 'vitest';

import { makePeer } from '@/test/factories';

import { movePinned, pinnedDropIndex, pinnedIds } from './pin-order';

describe('pin-order', () => {
  it('collects only pinned ids in the order it is given them', () => {
    expect(
      pinnedIds([
        makePeer({ deviceId: 'a', pinned: true }),
        makePeer({ deviceId: 'b', pinned: false }),
        makePeer({ deviceId: 'c', pinned: true }),
      ]),
    ).toEqual(['a', 'c']);
  });

  it('moves an id to a position and leaves the other ids in order', () => {
    expect(movePinned(['a', 'b', 'c'], 'a', 2)).toEqual(['b', 'c', 'a']);
    expect(movePinned(['a', 'b', 'c'], 'c', 0)).toEqual(['c', 'a', 'b']);
  });

  it('clamps a target outside the list and ignores an unknown id', () => {
    expect(movePinned(['a', 'b'], 'a', 9)).toEqual(['b', 'a']);
    expect(movePinned(['a', 'b'], 'missing', 0)).toEqual(['a', 'b']);
  });

  it('turns a pointer position into a clamped row index', () => {
    expect(pinnedDropIndex(80, 0, 0, 3, 72)).toBe(1);
    expect(pinnedDropIndex(-40, 0, 0, 3, 72)).toBe(0);
    expect(pinnedDropIndex(1_000, 0, 0, 3, 72)).toBe(2);
    expect(pinnedDropIndex(0, 100, 172, 3, 72)).toBe(1);
  });
});
