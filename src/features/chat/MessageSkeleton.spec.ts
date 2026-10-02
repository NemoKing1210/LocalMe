// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';

import MdSkeleton from '@/ui/MdSkeleton.vue';
import { mountView } from '@/test/mount';

import MessageSkeleton from './MessageSkeleton.vue';

describe('the message skeleton', () => {
  it('announces itself once to screen readers', async () => {
    const wrapper = await mountView(MessageSkeleton);

    expect(wrapper.find('[role="status"]').attributes('aria-label')).toBe('Loading…');
    expect(wrapper.findAll('[role="status"]')).toHaveLength(1);
  });

  it('draws five bubble rows, alternating incoming and outgoing', async () => {
    const wrapper = await mountView(MessageSkeleton);

    const rows = wrapper.findAll('.skeleton__row');
    expect(rows).toHaveLength(5);
    expect(rows.map((row) => row.classes().includes('skeleton__row--outgoing'))).toEqual([
      false,
      true,
      false,
      true,
      false,
    ]);
  });

  it('sizes each row like the message it stands in for', async () => {
    const wrapper = await mountView(MessageSkeleton);

    const bubbles = wrapper.findAllComponents(MdSkeleton);
    expect(bubbles).toHaveLength(5);
    expect(bubbles.map((bubble) => bubble.props('shape'))).toEqual([
      'bubble',
      'bubble',
      'bubble',
      'bubble',
      'bubble',
    ]);
    expect(bubbles.map((bubble) => bubble.props('width'))).toEqual([
      '58%',
      '42%',
      '34%',
      '64%',
      '48%',
    ]);
    expect(bubbles.map((bubble) => bubble.props('height'))).toEqual([
      '40px',
      '40px',
      '34px',
      '56px',
      '40px',
    ]);
  });
});
