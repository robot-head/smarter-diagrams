const fs = require('node:fs');
const { chromium } = require('playwright');

module.exports = function browserOptions(url, webgpu = false) {
  const target = new URL(url);
  const options = {
    executablePath: process.env.CHROMIUM_PATH || (fs.existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : chromium.executablePath()),
    headless: true,
    args: ['--no-sandbox', ...(webgpu ? ['--enable-unsafe-webgpu', '--use-angle=swiftshader'] : [])],
  };
  // Keep loopback tests local; public-site checks use the configured network
  // route. Credentials remain in memory and never appear in browser flags/logs.
  if (!['localhost', '127.0.0.1', '[::1]'].includes(target.hostname)) {
    const address = target.protocol === 'https:'
      ? process.env.HTTPS_PROXY || process.env.https_proxy || process.env.HTTP_PROXY || process.env.http_proxy
      : process.env.HTTP_PROXY || process.env.http_proxy;
    if (address) {
      const proxy = new URL(address);
      options.proxy = {
        server: `${proxy.protocol}//${proxy.host}`,
        bypass: process.env.NO_PROXY || process.env.no_proxy,
        username: decodeURIComponent(proxy.username),
        password: decodeURIComponent(proxy.password),
      };
    }
  }
  return options;
};
