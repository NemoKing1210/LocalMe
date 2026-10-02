// @vitest-environment happy-dom
import { hostname } from '@tauri-apps/plugin-os';
import { createPinia, setActivePinia, type Pinia } from 'pinia';
import type { VueWrapper } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { translate } from '@/i18n';
import * as ipc from '@/ipc';
import type { Profile } from '@/ipc';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import { makeProfile, makeSettings } from '@/test/factories';
import { flushPromises, mountView } from '@/test/mount';
import MdAvatar from '@/ui/MdAvatar.vue';
import MdButton from '@/ui/MdButton.vue';
import MdTextField from '@/ui/MdTextField.vue';

vi.mock('@tauri-apps/plugin-os', () => ({ hostname: vi.fn() }));
// The settings store paints the theme through `@material/material-color-utilities`, whose
// extensionless ESM imports the test resolver cannot follow. Nothing under test reads it.
vi.mock('@/theme/useTheme', () => ({
  setAccentColor: vi.fn(),
  setThemeMode: vi.fn(),
}));
vi.mock('@/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/ipc')>()),
  ownProfile: vi.fn(),
  completeOnboarding: vi.fn(),
  getSettings: vi.fn(),
}));

import { CommandError } from '@/ipc';
import OnboardingView from './OnboardingView.vue';

/** Mirrors the limit the component and the core both use. */
const NICKNAME_MAX = 32;

function nicknameError(wrapper: VueWrapper): string | undefined {
  return wrapper.findComponent(MdTextField).props('errorText') as string | undefined;
}

async function enterNickname(wrapper: VueWrapper, value: string): Promise<void> {
  await wrapper.find('input').setValue(value);
  await flushPromises();
}

async function setup(): Promise<{ wrapper: VueWrapper; pinia: Pinia }> {
  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = await mountView(OnboardingView, { pinia });
  await flushPromises();
  return { wrapper, pinia };
}

beforeEach(() => {
  vi.mocked(hostname).mockResolvedValue('host-machine');
  vi.mocked(ipc.ownProfile).mockResolvedValue(makeProfile({ deviceId: 'device-self' }));
  vi.mocked(ipc.completeOnboarding).mockResolvedValue(makeProfile({ nickname: 'Alice' }));
  vi.mocked(ipc.getSettings).mockResolvedValue(makeSettings());
});

describe('the onboarding view', () => {
  it('seeds the nickname from the machine hostname and previews the announced avatar seed', async () => {
    const { wrapper } = await setup();

    expect(wrapper.findComponent(MdTextField).props('modelValue')).toBe('host-machine');
    const avatar = wrapper.findComponent(MdAvatar);
    expect(avatar.props('seed')).toBe('device-self:host-machine');
    expect(avatar.props('name')).toBe('host-machine');
    expect(nicknameError(wrapper)).toBeUndefined();
  });

  it('falls back to an empty nickname when the hostname cannot be read', async () => {
    vi.mocked(hostname).mockRejectedValue(new Error('no os plugin'));
    const { wrapper } = await setup();

    expect(wrapper.findComponent(MdTextField).props('modelValue')).toBe('');
    expect(wrapper.findComponent(MdAvatar).props('seed')).toBe('device-self:');
  });

  it('leaves the device out of the seed when the profile cannot be read', async () => {
    vi.mocked(ipc.ownProfile).mockRejectedValue(new Error('no host'));
    const { wrapper } = await setup();

    expect(wrapper.findComponent(MdAvatar).props('seed')).toBe(':host-machine');
  });

  it('rejects an empty nickname after whitespace is removed', async () => {
    const { wrapper } = await setup();
    await enterNickname(wrapper, '   ');

    expect(nicknameError(wrapper)).toBe(
      translate('onboarding.nickname.errorEmpty', { max: NICKNAME_MAX }),
    );
    expect(wrapper.findComponent(MdButton).props('disabled')).toBe(true);
  });

  it('rejects a nickname longer than the host limit', async () => {
    const { wrapper } = await setup();
    await enterNickname(wrapper, 'a'.repeat(NICKNAME_MAX + 1));

    expect(nicknameError(wrapper)).toBe(
      translate('onboarding.nickname.errorTooLong', { max: NICKNAME_MAX }),
    );
  });

  it('accepts exactly the limit, counted in code points', async () => {
    const { wrapper } = await setup();
    await enterNickname(wrapper, 'a'.repeat(NICKNAME_MAX));
    expect(nicknameError(wrapper)).toBeUndefined();

    // 32 emoji are 32 characters, not 64 UTF-16 units; one more tips it over.
    await enterNickname(wrapper, '😀'.repeat(NICKNAME_MAX));
    expect(nicknameError(wrapper)).toBeUndefined();
    expect(wrapper.findComponent(MdButton).props('disabled')).toBe(false);

    await enterNickname(wrapper, '😀'.repeat(NICKNAME_MAX + 1));
    expect(nicknameError(wrapper)).toBe(
      translate('onboarding.nickname.errorTooLong', { max: NICKNAME_MAX }),
    );
  });

  it('rejects a nickname containing a control character', async () => {
    const { wrapper } = await setup();
    await enterNickname(wrapper, 'Ann\u0007');

    expect(nicknameError(wrapper)).toBe(translate('onboarding.nickname.errorControl'));
    expect(wrapper.findComponent(MdButton).props('disabled')).toBe(true);
  });

  it('accepts a padded nickname and keeps the raw value in the preview', async () => {
    const { wrapper } = await setup();
    await enterNickname(wrapper, ' Alice ');

    expect(nicknameError(wrapper)).toBeUndefined();
    expect(wrapper.findComponent(MdAvatar).props('seed')).toBe('device-self: Alice ');
  });

  it('submits the trimmed nickname, reloads the settings, and leaves the field busy-free', async () => {
    const { wrapper } = await setup();
    await enterNickname(wrapper, ' Alice ');

    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(ipc.completeOnboarding).toHaveBeenCalledWith('Alice');
    expect(ipc.getSettings).toHaveBeenCalledTimes(1);
    expect(useSettingsStore().document?.onboarded).toBe(true);
    expect(wrapper.findComponent(MdButton).props('busy')).toBe(false);
  });

  it('submits when the text field reports Enter', async () => {
    const { wrapper } = await setup();
    await enterNickname(wrapper, 'Bob');

    await wrapper.find('input').trigger('keydown.enter');
    await flushPromises();

    expect(ipc.completeOnboarding).toHaveBeenCalledWith('Bob');
  });

  it('does not call the host while the nickname is invalid', async () => {
    const { wrapper } = await setup();
    await enterNickname(wrapper, '');

    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(ipc.completeOnboarding).not.toHaveBeenCalled();
    expect(ipc.getSettings).not.toHaveBeenCalled();
    expect(nicknameError(wrapper)).toBe(
      translate('onboarding.nickname.errorEmpty', { max: NICKNAME_MAX }),
    );
  });

  it('surfaces the host rejection and leaves the form usable', async () => {
    const { wrapper } = await setup();
    vi.mocked(ipc.completeOnboarding).mockRejectedValue(
      new CommandError({ kind: 'invalid_input', field: 'nickname', message: 'too rude' }),
    );
    await enterNickname(wrapper, 'Alice');

    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(nicknameError(wrapper)).toBe('nickname: too rude');
    expect(ipc.getSettings).not.toHaveBeenCalled();
    expect(wrapper.findComponent(MdButton).props('busy')).toBe(false);

    // Typing clears the host's message: the field is editable and submittable again.
    await enterNickname(wrapper, 'Alicia');
    expect(nicknameError(wrapper)).toBeUndefined();
    vi.mocked(ipc.completeOnboarding).mockResolvedValue(makeProfile({ nickname: 'Alicia' }));
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(ipc.completeOnboarding).toHaveBeenCalledTimes(2);
    expect(ipc.completeOnboarding).toHaveBeenLastCalledWith('Alicia');
  });

  it('reports an unexpected failure through the notice store', async () => {
    const { wrapper } = await setup();
    vi.mocked(ipc.completeOnboarding).mockRejectedValue(new Error('boom'));
    await enterNickname(wrapper, 'Alice');

    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(useUiStore().notice?.key).toBe('error.internal');
    expect(wrapper.findComponent(MdButton).props('busy')).toBe(false);
    expect(nicknameError(wrapper)).toBeUndefined();
  });

  it('ignores a second submit while the first is in flight', async () => {
    const { wrapper } = await setup();
    let resolve!: (value: Profile) => void;
    vi.mocked(ipc.completeOnboarding).mockReturnValue(
      new Promise<Profile>((done) => {
        resolve = done;
      }),
    );
    await enterNickname(wrapper, 'Alice');

    await wrapper.find('form').trigger('submit');
    await flushPromises();
    expect(wrapper.findComponent(MdButton).props('busy')).toBe(true);

    await wrapper.find('form').trigger('submit');
    await flushPromises();
    expect(ipc.completeOnboarding).toHaveBeenCalledTimes(1);

    resolve(makeProfile({ nickname: 'Alice' }));
    await flushPromises();
    expect(wrapper.findComponent(MdButton).props('busy')).toBe(false);
  });
});
