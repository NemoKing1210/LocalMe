// @vitest-environment happy-dom

import type { DOMWrapper, VueWrapper } from '@vue/test-utils';
import { createPinia, type Pinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';
import type { Router } from 'vue-router';

import { ROUTE } from '@/app/routes';
import * as ipc from '@/ipc';
import { CommandError } from '@/ipc';
import type { Profile, Settings } from '@/ipc';
import { usePeerStore } from '@/stores/peers';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import {
  makeDiagnostics,
  makeKnownDevice,
  makeLogsInfo,
  makeProfile,
  makeSettings,
} from '@/test/factories';
import { createTestRouter, flushPromises, mountView } from '@/test/mount';
import MdAvatar from '@/ui/MdAvatar.vue';
import MdBadge from '@/ui/MdBadge.vue';
import MdDialog from '@/ui/MdDialog.vue';
import MdListItem from '@/ui/MdListItem.vue';
import MdRadioGroup from '@/ui/MdRadioGroup.vue';
import MdSegmentedButton from '@/ui/MdSegmentedButton.vue';
import MdTextField from '@/ui/MdTextField.vue';

import SettingsView from './SettingsView.vue';

vi.mock('@/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/ipc')>();
  return {
    ...actual,
    ownProfile: vi.fn(),
    knownDevices: vi.fn(),
    diagnostics: vi.fn(),
    logsInfo: vi.fn(),
    updateSettings: vi.fn(),
    setNickname: vi.fn(),
    clearHistory: vi.fn(),
    clearLogs: vi.fn(),
    openLogsFolder: vi.fn(),
    restorePeer: vi.fn(),
    history: vi.fn(),
  };
});

const NOTIFICATION_TOGGLES: [string, Settings['notifications']][] = [
  ['Show notifications', { enabled: false, showText: true, sound: false }],
  ['Include the message text', { enabled: true, showText: false, sound: false }],
  ['Play a sound', { enabled: true, showText: true, sound: true }],
];

const SYSTEM_TOGGLES: [string, Settings['system']][] = [
  ['Start LocalMe when I sign in', { autostart: true, startMinimized: false, closeToTray: true }],
  ['Start minimised in the tray', { autostart: false, startMinimized: true, closeToTray: true }],
  [
    'Keep running in the tray when the window is closed',
    { autostart: false, startMinimized: false, closeToTray: false },
  ],
];

interface Mounted {
  readonly wrapper: VueWrapper;
  readonly router: Router;
  readonly pinia: Pinia;
}

function must<T>(value: T | undefined, what: string): T {
  if (value === undefined) throw new Error(`expected ${what} to exist`);
  return value;
}

function buttonByText(wrapper: VueWrapper, text: string): DOMWrapper<Element> {
  const found = wrapper.findAll('button').find((button) => button.text() === text);
  return must(found, `button "${text}"`);
}

function switchByLabel(wrapper: VueWrapper, label: string): DOMWrapper<Element> {
  const found = wrapper
    .findAll('button[role="switch"]')
    .find((button) => button.attributes('aria-label') === label);
  return must(found, `switch "${label}"`);
}

function segmentedButtonWith(wrapper: VueWrapper, value: string): VueWrapper {
  const found = wrapper.findAllComponents(MdSegmentedButton).find((component) => {
    const options = component.props('options') as readonly { readonly value: string }[];
    return options.some((option) => option.value === value);
  });
  return must(found, `segmented button offering "${value}"`);
}

function dialogWithHeadline(
  wrapper: VueWrapper,
  headline: string,
): VueWrapper<InstanceType<typeof MdDialog>> {
  const found = wrapper
    .findAllComponents(MdDialog)
    .find((dialog) => dialog.props('headline') === headline);
  return must(found, `dialog "${headline}"`);
}

function nicknameField(wrapper: VueWrapper): VueWrapper<InstanceType<typeof MdTextField>> {
  return must(wrapper.findComponent(MdTextField), 'the nickname field');
}

/** The document handed to the host by the last `updateSettings` call. */
function savedSettings(): Settings {
  const calls = vi.mocked(ipc.updateSettings).mock.calls;
  const last = calls.at(-1);
  if (!last) throw new Error('updateSettings was never called');
  return last[0];
}

async function mountSettings(overrides: Partial<Settings> = {}): Promise<Mounted> {
  const pinia = createPinia();
  // Applying the document before the mount is how the app starts the screen.
  useSettingsStore(pinia).apply(makeSettings(overrides));
  const router = createTestRouter();
  const wrapper = await mountView(SettingsView, { pinia, router, route: '/settings' });
  return { wrapper, router, pinia };
}

beforeEach(() => {
  vi.mocked(ipc.ownProfile).mockResolvedValue(makeProfile());
  vi.mocked(ipc.knownDevices).mockResolvedValue([]);
  vi.mocked(ipc.diagnostics).mockResolvedValue(makeDiagnostics());
  vi.mocked(ipc.logsInfo).mockResolvedValue(makeLogsInfo());
  vi.mocked(ipc.updateSettings).mockImplementation((next: Settings) => Promise.resolve(next));
  vi.mocked(ipc.setNickname).mockImplementation((value: string) =>
    Promise.resolve(makeProfile({ nickname: value })),
  );
  vi.mocked(ipc.clearHistory).mockResolvedValue(0);
  vi.mocked(ipc.clearLogs).mockResolvedValue(0);
  vi.mocked(ipc.openLogsFolder).mockResolvedValue(undefined);
  vi.mocked(ipc.restorePeer).mockResolvedValue(undefined);
  vi.mocked(ipc.history).mockResolvedValue([]);
  vi.spyOn(console, 'error').mockImplementation(() => {});
});

describe('the settings view', () => {
  describe('loading', () => {
    it('fills the profile group only once the profile has arrived', async () => {
      let resolveProfile: ((profile: Profile) => void) | undefined;
      vi.mocked(ipc.ownProfile).mockReturnValue(
        new Promise<Profile>((resolve) => {
          resolveProfile = resolve;
        }),
      );

      const { wrapper } = await mountSettings();
      expect(wrapper.findComponent(MdTextField).exists()).toBe(false);

      must(resolveProfile, 'the profile resolver')(makeProfile({ nickname: 'Ada' }));
      await flushPromises();

      expect(nicknameField(wrapper).props('modelValue')).toBe('Ada');
      expect(must(wrapper.findAllComponents(MdAvatar)[0], 'the avatar').props('seed')).toBe(
        'seed-self',
      );
    });

    it('renders the loaded devices, diagnostics and logs', async () => {
      vi.mocked(ipc.knownDevices).mockResolvedValue([makeKnownDevice({ nickname: 'Alice' })]);
      vi.mocked(ipc.diagnostics).mockResolvedValue(makeDiagnostics({ version: '1.2.3' }));

      const { wrapper } = await mountSettings();
      await flushPromises();

      expect(wrapper.text()).toContain('Alice');
      expect(wrapper.text()).toContain('Version 1.2.3');
      expect(wrapper.text()).toContain('protocolVersion');
      expect(wrapper.text()).toContain('47820');
      expect(wrapper.text()).toContain('windows');
      expect(wrapper.text()).toContain('/tmp/logs');
    });

    it('reports a failed profile read', async () => {
      vi.mocked(ipc.ownProfile).mockRejectedValue(new Error('boom'));
      const { pinia } = await mountSettings();
      await flushPromises();
      const ui = useUiStore(pinia);
      expect(ui.notice?.key).toBe('error.internal');
      expect(ui.notice?.tone).toBe('error');
    });

    it('reports a failed device read', async () => {
      vi.mocked(ipc.knownDevices).mockRejectedValue(new Error('boom'));
      const { pinia } = await mountSettings();
      await flushPromises();
      expect(useUiStore(pinia).notice?.key).toBe('error.internal');
    });

    it('reports a failed diagnostics read', async () => {
      vi.mocked(ipc.diagnostics).mockRejectedValue(new Error('boom'));
      const { pinia } = await mountSettings();
      await flushPromises();
      expect(useUiStore(pinia).notice?.key).toBe('error.internal');
    });

    it('reports a failed log read', async () => {
      vi.mocked(ipc.logsInfo).mockRejectedValue(new Error('boom'));
      const { pinia } = await mountSettings();
      await flushPromises();
      expect(useUiStore(pinia).notice?.key).toBe('settings.logsFailed');
    });
  });

  describe('nickname', () => {
    it('saves the trimmed nickname and refreshes the avatar preview', async () => {
      vi.mocked(ipc.setNickname).mockResolvedValue(
        makeProfile({ nickname: 'Ada', avatarSeed: 'seed-ada' }),
      );
      const { wrapper, pinia } = await mountSettings();
      await flushPromises();

      const field = nicknameField(wrapper);
      field.vm.$emit('update:modelValue', '  Ada  ');
      await nextTick();
      field.vm.$emit('submit');
      await flushPromises();

      expect(ipc.setNickname).toHaveBeenCalledWith('Ada');
      expect(nicknameField(wrapper).props('modelValue')).toBe('Ada');
      expect(must(wrapper.findAllComponents(MdAvatar)[0], 'the avatar').props('seed')).toBe(
        'seed-ada',
      );
      expect(useUiStore(pinia).notice).toBeNull();
    });

    it('does not call the host when the trimmed nickname is unchanged', async () => {
      const { wrapper } = await mountSettings();
      await flushPromises();

      const field = nicknameField(wrapper);
      field.vm.$emit('update:modelValue', '  Me  ');
      await nextTick();
      field.vm.$emit('submit');
      await flushPromises();

      expect(ipc.setNickname).not.toHaveBeenCalled();
      expect(nicknameField(wrapper).props('modelValue')).toBe('Me');
    });

    it('shows the host refusal for an empty nickname on the field', async () => {
      vi.mocked(ipc.setNickname).mockRejectedValueOnce(
        new CommandError({
          kind: 'invalid_input',
          field: 'nickname',
          message: 'must not be empty',
        }),
      );
      const { wrapper } = await mountSettings();
      await flushPromises();

      const field = nicknameField(wrapper);
      field.vm.$emit('update:modelValue', '   ');
      await nextTick();
      field.vm.$emit('submit');
      await flushPromises();

      expect(ipc.setNickname).toHaveBeenCalledWith('');
      expect(nicknameField(wrapper).props('errorText')).toBe('nickname: must not be empty');
    });

    it('falls back to the internal error text for a non-command failure', async () => {
      vi.mocked(ipc.setNickname).mockRejectedValueOnce(new Error('socket closed'));
      const { wrapper } = await mountSettings();
      await flushPromises();

      const field = nicknameField(wrapper);
      field.vm.$emit('update:modelValue', 'Ada');
      await nextTick();
      field.vm.$emit('submit');
      await flushPromises();

      expect(nicknameField(wrapper).props('errorText')).toBe(
        'Something went wrong. The technical details are in the log.',
      );
    });

    it('clears the error as soon as the value changes', async () => {
      vi.mocked(ipc.setNickname).mockRejectedValueOnce(
        new CommandError({ kind: 'internal', message: 'refused' }),
      );
      const { wrapper } = await mountSettings();
      await flushPromises();

      const field = nicknameField(wrapper);
      field.vm.$emit('update:modelValue', 'Ada');
      await nextTick();
      field.vm.$emit('submit');
      await flushPromises();
      expect(nicknameField(wrapper).props('errorText')).toBe('internal: refused');

      field.vm.$emit('update:modelValue', 'Ad');
      await nextTick();
      expect(nicknameField(wrapper).props('errorText')).toBeUndefined();
    });

    it('commits when the field loses focus', async () => {
      const { wrapper } = await mountSettings();
      await flushPromises();

      const field = nicknameField(wrapper);
      field.vm.$emit('update:modelValue', 'Ada');
      await nextTick();
      await wrapper.get('.settings__profile-field').trigger('focusout');
      await flushPromises();

      expect(ipc.setNickname).toHaveBeenCalledWith('Ada');
    });
  });

  describe('appearance', () => {
    it('saves the chosen theme and reflects it in the control', async () => {
      const { wrapper, pinia } = await mountSettings();
      await wrapper.get('[data-value="dark"]').trigger('click');
      await flushPromises();

      expect(savedSettings().appearance).toEqual({ theme: 'dark', accent: '#6750A4' });
      expect(useSettingsStore(pinia).theme).toBe('dark');
      expect(wrapper.get('[data-value="dark"]').classes()).toContain(
        'md-segmented__segment--selected',
      );
    });

    it('reflects the theme and locale already stored', async () => {
      const { wrapper } = await mountSettings({
        appearance: { theme: 'dark', accent: '#6750A4' },
        locale: 'ru',
      });
      await flushPromises();

      expect(wrapper.get('[data-value="dark"]').classes()).toContain(
        'md-segmented__segment--selected',
      );
      const russian = wrapper.get('input[value="ru"]').element as HTMLInputElement;
      expect(russian.checked).toBe(true);
      expect(wrapper.text()).toContain('Настройки');
    });

    it('ignores a theme value the host does not know', async () => {
      const { wrapper } = await mountSettings();
      segmentedButtonWith(wrapper, 'system').vm.$emit('update:modelValue', 'sepia');
      await nextTick();
      expect(ipc.updateSettings).not.toHaveBeenCalled();
    });

    it('saves a preset accent and marks it active', async () => {
      const { wrapper } = await mountSettings();
      await wrapper.get('button[aria-label="teal"]').trigger('click');
      await flushPromises();

      expect(savedSettings().appearance).toEqual({ theme: 'system', accent: '#006A6A' });
      expect(wrapper.get('button[aria-label="teal"]').attributes('aria-pressed')).toBe('true');
      expect(wrapper.get('button[aria-label="baseline"]').attributes('aria-pressed')).toBe('false');
    });

    it('saves a custom colour and marks it as custom', async () => {
      const { wrapper } = await mountSettings();
      const input = wrapper.get('input.settings__custom');
      (input.element as HTMLInputElement).value = '#123456';
      await input.trigger('change');
      await flushPromises();

      expect(savedSettings().appearance.accent).toBe('#123456');
      expect(wrapper.get('input.settings__custom').classes()).toContain('settings__custom--active');
    });

    it('surfaces a failed save without corrupting the stored document', async () => {
      vi.mocked(ipc.updateSettings).mockRejectedValueOnce(new Error('refused'));
      const { wrapper, pinia } = await mountSettings();
      await wrapper.get('[data-value="dark"]').trigger('click');
      await flushPromises();

      const ui = useUiStore(pinia);
      expect(ui.notice?.key).toBe('settings.saveFailed');
      expect(ui.notice?.tone).toBe('error');
      expect(useSettingsStore(pinia).document?.appearance.theme).toBe('system');
      expect(wrapper.get('[data-value="dark"]').classes()).not.toContain(
        'md-segmented__segment--selected',
      );
    });

    it('drops a second change while a save is still in flight', async () => {
      // Observed: `settings.save` returns early while `saving` is true, so a quick second toggle
      // is silently lost — the component neither queues nor coalesces the patch.
      let release: (() => void) | undefined;
      vi.mocked(ipc.updateSettings).mockImplementation(
        (next: Settings) =>
          new Promise<Settings>((resolve) => {
            release = () => {
              resolve(next);
            };
          }),
      );
      const { wrapper } = await mountSettings();

      await wrapper.get('[data-value="dark"]').trigger('click');
      await wrapper.get('[data-value="light"]').trigger('click');
      must(release, 'the updateSettings resolver')();
      await flushPromises();

      expect(ipc.updateSettings).toHaveBeenCalledTimes(1);
      expect(savedSettings().appearance.theme).toBe('dark');
    });
  });

  describe('language', () => {
    it('saves the locale and switches the visible strings', async () => {
      const { wrapper, pinia } = await mountSettings();
      const german = must(
        wrapper.findAll('input[type="radio"]').find((input) => input.attributes('value') === 'de'),
        'the German radio',
      );
      await german.trigger('change');
      await flushPromises();

      expect(savedSettings().locale).toBe('de');
      expect(useSettingsStore(pinia).locale).toBe('de');
      expect(wrapper.text()).toContain('Einstellungen');
      expect(wrapper.text()).toContain('Deutsch');
    });

    it('ignores a locale the host does not know', async () => {
      const { wrapper } = await mountSettings();
      must(wrapper.findComponent(MdRadioGroup), 'the language group').vm.$emit(
        'update:modelValue',
        'xx',
      );
      await nextTick();
      expect(ipc.updateSettings).not.toHaveBeenCalled();
    });
  });

  describe('notifications', () => {
    it.each(NOTIFICATION_TOGGLES)('saves %s on its own', async (label, expected) => {
      const { wrapper, pinia } = await mountSettings();
      await flushPromises();

      await switchByLabel(wrapper, label).trigger('click');
      await flushPromises();

      expect(savedSettings().notifications).toEqual(expected);
      expect(useSettingsStore(pinia).document?.notifications).toEqual(expected);
    });
  });

  describe('system', () => {
    it.each(SYSTEM_TOGGLES)('saves %s', async (label, expected) => {
      const { wrapper } = await mountSettings();
      await flushPromises();

      await switchByLabel(wrapper, label).trigger('click');
      await flushPromises();

      expect(savedSettings().system).toEqual(expected);
    });
  });

  describe('known devices', () => {
    it('lists devices with their status and last-seen support text', async () => {
      vi.mocked(ipc.knownDevices).mockResolvedValue([
        makeKnownDevice({ deviceId: 'device-a', nickname: 'Alice', messageCount: 3 }),
        makeKnownDevice({
          deviceId: 'device-b',
          nickname: 'Bob',
          forgotten: true,
          lastSeenMs: null,
        }),
      ]);
      const { wrapper } = await mountSettings();
      await flushPromises();

      const items = wrapper.findAllComponents(MdListItem);
      expect(items).toHaveLength(2);
      expect(must(items[0], 'the first device').props('headline')).toBe('Alice');
      expect(must(items[0], 'the first device').props('supporting')).toMatch(/^known · last seen /);
      expect(must(items[1], 'the second device').props('supporting')).toBe(
        'forgotten · never online',
      );
      expect(must(wrapper.findAllComponents(MdBadge)[0], 'the count badge').props('value')).toBe(3);
    });

    it('shows the empty state when no device has been seen', async () => {
      const { wrapper } = await mountSettings();
      await flushPromises();
      expect(wrapper.text()).toContain('No devices have been seen yet.');
    });

    it('restores a forgotten device and reloads the list', async () => {
      vi.mocked(ipc.knownDevices).mockResolvedValue([
        makeKnownDevice({ deviceId: 'device-b', nickname: 'Bob', forgotten: true }),
      ]);
      const { wrapper } = await mountSettings();
      await flushPromises();

      await wrapper
        .get('button[aria-label="Start receiving from this device again"]')
        .trigger('click');
      await flushPromises();

      expect(ipc.restorePeer).toHaveBeenCalledWith('device-b');
      expect(ipc.knownDevices).toHaveBeenCalledTimes(2);
    });

    it('reports a failed restore', async () => {
      vi.mocked(ipc.knownDevices).mockResolvedValue([
        makeKnownDevice({ deviceId: 'device-b', forgotten: true }),
      ]);
      vi.mocked(ipc.restorePeer).mockRejectedValueOnce(new Error('boom'));
      const { wrapper, pinia } = await mountSettings();
      await flushPromises();

      await wrapper
        .get('button[aria-label="Start receiving from this device again"]')
        .trigger('click');
      await flushPromises();

      expect(useUiStore(pinia).notice?.key).toBe('error.internal');
    });
  });

  describe('clearing history', () => {
    it('asks for confirmation first and does nothing on cancel', async () => {
      const { wrapper } = await mountSettings();
      await buttonByText(wrapper, 'Clear all message history').trigger('click');
      await nextTick();

      const dialog = dialogWithHeadline(wrapper, 'Delete every message?');
      expect(dialog.props('open')).toBe(true);
      expect(ipc.clearHistory).not.toHaveBeenCalled();

      await buttonByText(dialog, 'Cancel').trigger('click');
      await nextTick();
      expect(dialogWithHeadline(wrapper, 'Delete every message?').props('open')).toBe(false);
      expect(ipc.clearHistory).not.toHaveBeenCalled();
    });

    it('clears history, reopens the conversation and reloads devices on confirm', async () => {
      vi.mocked(ipc.clearHistory).mockResolvedValue(5);
      const { wrapper, pinia } = await mountSettings();
      const peers = usePeerStore(pinia);
      peers.select('device-a');
      await flushPromises();

      await buttonByText(wrapper, 'Clear all message history').trigger('click');
      await nextTick();
      await buttonByText(dialogWithHeadline(wrapper, 'Delete every message?'), 'Delete').trigger(
        'click',
      );
      await flushPromises();

      const ui = useUiStore(pinia);
      expect(ipc.clearHistory).toHaveBeenCalledTimes(1);
      expect(ipc.history).toHaveBeenCalledWith('device-a', null, 50);
      expect(ui.notice?.key).toBe('settings.historyCleared');
      expect(ui.notice?.params).toEqual({ count: 5 });
      expect(ipc.knownDevices).toHaveBeenCalledTimes(2);
    });
  });

  describe('logs', () => {
    it('renders the summary and directory after loading', async () => {
      const { wrapper } = await mountSettings();
      await flushPromises();
      expect(wrapper.text()).toMatch(/1 file · /);
      expect(wrapper.text()).toContain('/tmp/logs');
    });

    it('shows the empty summary and disables deletion with no files', async () => {
      vi.mocked(ipc.logsInfo).mockResolvedValue(makeLogsInfo({ files: [], totalBytes: 0 }));
      const { wrapper } = await mountSettings();
      await flushPromises();
      expect(wrapper.text()).toContain('No log files yet.');
      expect(buttonByText(wrapper, 'Delete log files').attributes('disabled')).toBeDefined();
    });

    it('saves the log level', async () => {
      const { wrapper } = await mountSettings();
      await wrapper.get('[data-value="debug"]').trigger('click');
      await flushPromises();
      expect(savedSettings().logging).toEqual({ level: 'debug', retentionDays: 14 });
    });

    it('ignores an unknown log level', async () => {
      const { wrapper } = await mountSettings();
      segmentedButtonWith(wrapper, 'debug').vm.$emit('update:modelValue', 'nope');
      await nextTick();
      expect(ipc.updateSettings).not.toHaveBeenCalled();
    });

    it('saves retention as a number and reloads the summary', async () => {
      const { wrapper } = await mountSettings();
      await flushPromises();

      await wrapper.get('[data-value="30"]').trigger('click');
      await flushPromises();

      expect(savedSettings().logging).toEqual({ level: 'info', retentionDays: 30 });
      expect(ipc.logsInfo).toHaveBeenCalledTimes(2);
    });

    it('ignores a non-numeric retention', async () => {
      const { wrapper } = await mountSettings();
      segmentedButtonWith(wrapper, '7').vm.$emit('update:modelValue', 'soon');
      await nextTick();
      expect(ipc.updateSettings).not.toHaveBeenCalled();
    });

    it('opens the log folder', async () => {
      const { wrapper } = await mountSettings();
      await buttonByText(wrapper, 'Open folder').trigger('click');
      await flushPromises();
      expect(ipc.openLogsFolder).toHaveBeenCalledTimes(1);
    });

    it('reports a failed folder open', async () => {
      vi.mocked(ipc.openLogsFolder).mockRejectedValueOnce(new Error('boom'));
      const { wrapper, pinia } = await mountSettings();
      await buttonByText(wrapper, 'Open folder').trigger('click');
      await flushPromises();
      expect(useUiStore(pinia).notice?.key).toBe('settings.logFolderFailed');
    });

    it('clears the logs only after confirmation and reloads them', async () => {
      vi.mocked(ipc.clearLogs).mockResolvedValue(1024);
      const { wrapper, pinia } = await mountSettings();
      await flushPromises();

      await buttonByText(wrapper, 'Delete log files').trigger('click');
      await nextTick();
      expect(ipc.clearLogs).not.toHaveBeenCalled();

      await buttonByText(dialogWithHeadline(wrapper, 'Delete every log file?'), 'Delete').trigger(
        'click',
      );
      await flushPromises();

      const ui = useUiStore(pinia);
      expect(ipc.clearLogs).toHaveBeenCalledTimes(1);
      expect(ui.notice?.key).toBe('settings.logsCleared');
      expect(ui.notice?.params?.['size']).toBe('1 kB');
      expect(ipc.logsInfo).toHaveBeenCalledTimes(2);
    });
  });

  describe('close', () => {
    it('replaces the settings route with the chat route when opened directly', async () => {
      const { wrapper, router } = await mountSettings();
      const back = vi.spyOn(router, 'back');
      const replace = vi.spyOn(router, 'replace');

      await wrapper.get('button[aria-label="Back to messages"]').trigger('click');

      expect(back).not.toHaveBeenCalled();
      expect(replace).toHaveBeenCalledWith({ name: ROUTE.chat });
    });

    it('goes back when there is an entry behind', async () => {
      const { wrapper, router } = await mountSettings();
      const back = vi.spyOn(router, 'back').mockImplementation(() => {});
      const replace = vi.spyOn(router, 'replace');
      const state = router.options.history.state as Record<string, unknown>;
      state['back'] = '/chat';

      await wrapper.get('button[aria-label="Back to messages"]').trigger('click');

      expect(back).toHaveBeenCalledTimes(1);
      expect(replace).not.toHaveBeenCalled();
    });
  });
});
