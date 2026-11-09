import { useI18n } from '@/i18n';

/** The two fields a presence sentence is made of, wherever the row comes from. */
export interface PresenceSubject {
  readonly online: boolean;
  readonly lastSeenMs: number | null;
}

/**
 * One sentence for "is this person here".
 *
 * The people list, the conversation header and the settings screen all say the same thing, and a
 * copy each would be three places for the wording, the plural form or a new presence state to
 * drift. The caller passes `now`, because the ticking clock belongs to the component that draws
 * the sentence.
 */
export function usePresence(): (subject: PresenceSubject, now: number) => string {
  const i18n = useI18n();

  return (subject, now) => {
    if (subject.online) return i18n.t('users.online');
    if (subject.lastSeenMs === null) return i18n.t('users.neverSeen');
    return i18n.t('users.lastSeen', { relative: i18n.relative(subject.lastSeenMs, now) });
  };
}
