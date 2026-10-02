import type { en } from './en';

/**
 * Every message key, derived from the English catalogue.
 *
 * `t()` takes this type, so a typo is a compile error in every component, template and test.
 */
export type MessageKey = keyof typeof en;

export type PluralCategory = 'zero' | 'one' | 'two' | 'few' | 'many' | 'other';

export type PluralKey = `${MessageKey}.${PluralCategory}`;

export type MessageCatalog = Record<MessageKey, string> & Partial<Record<PluralKey, string>>;

export type MessageParams = Readonly<Record<string, string | number>>;
