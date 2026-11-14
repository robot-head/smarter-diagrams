const { createHash } = require('node:crypto');

function samples(rawSource, info = 'diagram') {
  const source = String(rawSource).replace(/\r\n/g, '\n');
  const names = key => (info.match(new RegExp(`\\b${key}=([A-Za-z0-9_.,-]+)`))?.[1] || '').split(',').filter(Boolean);
  const steps = names('steps'), views = names('views'), variants = names('variants'), focus = names('focus');
  if (steps.length) {
    return steps.map((step, i) => ({
      source,
      step,
      view: null,
      variant: null,
      focus: null,
      expectError: /\bexpect-error\b/.test(info),
      label: `Step ${i + 1}/${steps.length}: ${step}`,
    }));
  }
  return [null, ...variants].flatMap(variant => [null, ...views].flatMap(view => [null, ...focus].map(focusNode => ({
    source,
    step: null,
    view,
    variant,
    focus: focusNode,
    expectError: /\bexpect-error\b/.test(info),
    label: [views.length ? view ? `View: ${view}` : 'Canonical graph' : '', variants.length ? variant ? `Layout: ${variant}` : 'Default layout' : '', focus.length ? focusNode ? `Focus: ${focusNode} (1 hop)` : 'Full context' : ''].filter(Boolean).join(' · ') || 'Rendered output',
  }))));
}
function key(sample) {
  return createHash('sha256').update(JSON.stringify([String(sample.source).replace(/\r\n/g, '\n').trim(), sample.step || null, sample.view, sample.variant, sample.focus, sample.expectError])).digest('hex');
}
module.exports = { samples, key };
