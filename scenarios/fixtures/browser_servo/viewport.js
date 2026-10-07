// Copyright 2026 Mark Alan Boykin. SPDX-License-Identifier: MPL-2.0
(() => {
  let resizes = 0;
  const report = () => {
    const body = document.body.getBoundingClientRect();
    const metrics = `w=${innerWidth} h=${innerHeight} client=${document.documentElement.clientWidth} body=${Math.round(body.width)} r=${resizes}`;
    document.querySelector('#result').textContent = metrics;
    document.title = `Servo viewport | ${metrics}`;
  };
  window.addEventListener('resize', () => {
    resizes += 1;
    requestAnimationFrame(() => requestAnimationFrame(report));
  });
  requestAnimationFrame(report);
})();
