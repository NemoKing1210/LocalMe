import { describe, expect, it } from 'vitest';

import UnknownIcon from '~icons/vscode-icons/default-file';
import { fileIconFor } from './fileIcons';

/**
 * The mapping itself is data, and a test that pins "pdf shows the pdf picture" would only restate
 * the table. What is worth pinning is how the name is read: a wrong parse shows the wrong badge,
 * which is worse than showing none.
 */
describe('choosing a file icon', () => {
  it('reads the extension case-insensitively', () => {
    expect(fileIconFor('Report.PDF')).toBe(fileIconFor('report.pdf'));
    expect(fileIconFor('photo.JPG')).toBe(fileIconFor('photo.jpg'));
  });

  it('uses the last extension of a compound suffix', () => {
    expect(fileIconFor('archive.tar.gz')).toBe(fileIconFor('archive.gz'));
    expect(fileIconFor('bundle.min.js')).toBe(fileIconFor('app.js'));
  });

  it('falls back for a name with no extension, a dotfile, or a trailing dot', () => {
    for (const name of ['README', '.gitignore', 'notes.', '', '..']) {
      expect(fileIconFor(name), name).toBe(UnknownIcon);
    }
  });

  it('falls back for an extension this build does not know', () => {
    expect(fileIconFor('thing.qqq')).toBe(UnknownIcon);
  });

  it('separates a document from a picture from an executable', () => {
    // Three shapes a reader scans for; if any two collapsed to the same badge the list would stop
    // being readable at a glance, which is the whole reason the icons are here.
    const pdf = fileIconFor('a.pdf');
    const image = fileIconFor('a.png');
    const binary = fileIconFor('a.exe');
    expect(pdf).not.toBe(image);
    expect(pdf).not.toBe(binary);
    expect(image).not.toBe(binary);
  });
});
