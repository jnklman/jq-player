import { describe, expect, it } from 'vitest';

import { isInsideDir } from './media';

describe('isInsideDir', () => {
  it('matches the folder and what is below it, not siblings', () => {
    expect(isInsideDir('/shows/a/media', '/shows/a/media')).toBe(true);
    expect(isInsideDir('/shows/a/media/sub', '/shows/a/media/')).toBe(true);
    expect(isInsideDir('/shows/a/media2', '/shows/a/media')).toBe(false);
  });

  it('ignores case and separators of Windows paths', () => {
    expect(isInsideDir('c:\\Shows\\A\\MEDIA\\x', 'C:\\shows\\a\\media')).toBe(true);
    expect(isInsideDir('C:/shows/a/media2', 'C:\\shows\\a\\media')).toBe(false);
  });
});
