import type { en } from './en';

/**
 * Every message key, derived from the English catalogue.
 *
 * `t()` takes this type, so a typo is a compile error in every component, template and test.
 */
export type MessageKey = keyof typeof en;

/** CLDR plural categories, as reported by `Intl.PluralRules`. */
export type PluralCategory = 'zero' | 'one' | 'two' | 'few' | 'many' | 'other';

/**
 * A plural specialisation of a base key: `users.unread.one`, `users.unread.few`, …
 *
 * Only the categories a language actually needs have to exist, which is why these are
 * optional rather than required.
 */
export type PluralKey = `${MessageKey}.${PluralCategory}`;

/**
 * The shape a message catalogue must have.
 *
 * Every base key is mandatory; plural specialisations are optional and must be well-formed,
 * so `users.unread.few` in the Russian catalogue is accepted while `users.unread.some` is a
 * compile error. Keys that do not belong to any base key are rejected as excess properties.
 */
export type MessageCatalog = Record<MessageKey, string> & Partial<Record<PluralKey, string>>;

/** Values substituted into `{placeholders}`. */
export type MessageParams = Readonly<Record<string, string | number>>;
