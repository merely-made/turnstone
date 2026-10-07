// Copyright 2026 Mark Alan Boykin. SPDX-License-Identifier: MPL-2.0
(() => {
  const query = new URLSearchParams(location.search);
  const result = document.querySelector('#result');
  if (result) {
    const page = query.get('page');
    const text = query.get('text');
    const clicks = query.get('clicks');
    document.documentElement.dataset.page = page;
    document.querySelector('#heading').textContent = `Servo ${page} native result`;
    result.textContent = `Page ${page}\nActual input: ${text}\nActual button clicks: ${clicks}`;
    document.title = `Servo ${page} | result=${text} | clicks=${clicks}`;
    return;
  }
  const page = document.documentElement.dataset.page;
  const entry = document.querySelector('#entry');
  let clicks = 0;
  entry.addEventListener('input', () => {
    document.title = `Servo ${page} | text=${entry.value} | clicks=${clicks}`;
  });
  document.querySelector('#activate').addEventListener('click', () => {
    clicks += 1;
    const target = new URL('result.html', location.href);
    target.searchParams.set('page', page);
    target.searchParams.set('text', entry.value);
    target.searchParams.set('clicks', String(clicks));
    location.href = target.href;
  });
})();
