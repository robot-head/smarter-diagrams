// Embedded editors report their content height so the guide has one page scroll.
if (parent !== window) {
  let pending = false, previous = 0;
  const observer = new ResizeObserver(() => {
    if (pending) return;
    pending = true;
    requestAnimationFrame(() => {
      pending = false;
      const height = Math.ceil(document.body.getBoundingClientRect().bottom + scrollY + parseFloat(getComputedStyle(document.body).marginBottom));
      if (height !== previous) {
        previous = height;
        parent.postMessage({ type: 'smarter-diagrams:resize', height }, location.origin);
      }
    });
  });
  observer.observe(document.body);
  addEventListener('pagehide', () => observer.disconnect(), { once: true });
}
