const { createHash } = require('node:crypto');

function samples(source, info = 'diagram') {
  const names = key => (info.match(new RegExp(`\\b${key}=([A-Za-z0-9_.,-]+)`))?.[1] || '').split(',').filter(Boolean);
  const views = names('views'), variants = names('variants'), focus = names('focus');
  return [null, ...variants].flatMap(variant => [null, ...views].flatMap(view => [null, ...focus].map(focusNode => ({
    source,
    view,
    variant,
    focus: focusNode,
    expectError: /\bexpect-error\b/.test(info),
    label: [views.length ? view ? `View: ${view}` : 'Canonical graph' : '', variants.length ? variant ? `Layout: ${variant}` : 'Default layout' : '', focus.length ? focusNode ? `Focus: ${focusNode} (1 hop)` : 'Full context' : ''].filter(Boolean).join(' · ') || 'Rendered output',
  }))));
}
function key(sample) {
  return createHash('sha256').update(JSON.stringify([sample.source.trim(), sample.view, sample.variant, sample.focus, sample.expectError])).digest('hex');
}
module.exports = { samples, key };
