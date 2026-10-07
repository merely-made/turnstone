// Copyright 2026 Mark Alan Boykin. SPDX-License-Identifier: MPL-2.0
(() => {
  const results = [];
  const check = (name, probe) => {
    try {
      if (!probe()) throw new Error('unexpected result');
      results.push(`${name}: pass`);
    } catch (error) {
      results.push(`${name}: failed (${String(error)})`);
    }
  };
  check('canonical normalization', () => 'e\u0301'.normalize('NFC') === '\u00e9');
  check('non-Latin normalization', () => '\u03b1\u0301'.normalize('NFC') === '\u03ac');
  check('ISO calendar date', () => {
    const format = new Intl.DateTimeFormat('en-US', {
      calendar: 'iso8601', timeZone: 'UTC', year: 'numeric', month: '2-digit', day: '2-digit',
    });
    const parts = Object.fromEntries(format.formatToParts(new Date(Date.UTC(2026, 9, 6)))
      .filter(part => part.type !== 'literal').map(part => [part.type, part.value]));
    return parts.year === '2026' && parts.month === '10' && parts.day === '06';
  });
  check('Buddhist calendar date', () => {
    const format = new Intl.DateTimeFormat('en-US', { calendar: 'buddhist', timeZone: 'UTC', year: 'numeric' });
    return format.formatToParts(new Date(Date.UTC(2026, 9, 6)))
      .find(part => part.type === 'year').value === '2569';
  });
  check('word segmentation', () => {
    const segments = [...new Intl.Segmenter('en', { granularity: 'word' }).segment('Turnstone browser')];
    return segments.filter(segment => segment.isWordLike).map(segment => segment.segment).join('|')
      === 'Turnstone|browser';
  });
  check('numeric collation', () => new Intl.Collator('en', { numeric: true }).compare('2', '10') < 0);
  check('Swedish collation', () => new Intl.Collator('sv').compare('z', '\u00e4') < 0);
  const passed = results.every(result => result.endsWith(': pass'));
  document.querySelector('#result').textContent = results.join('\n');
  document.body.style.background = passed ? '#176d35' : '#b52a17';
  document.title = `ICU bridge | intl=${passed ? 'pass' : 'failed'}`;
})();
