// The existing maintained Baileys adapter emits its native QR to this private
// broker pipe. Only the authenticated Tangled proxy may expose the SVG in memory.
import { createRequire } from 'node:module';
import { ConnectorError, Destination } from '../whatsapp/core.mjs';
const require = createRequire(new URL('../whatsapp/package.json', import.meta.url));
const QRCode = require('qrcode-terminal/vendor/QRCode');
const level = require('qrcode-terminal/vendor/QRCode/QRErrorCorrectLevel');

export function qrSvg(value) {
  if (typeof value !== 'string' || value.length > 4096) throw new ConnectorError('Invalid native QR');
  const qr = new QRCode(-1, level.L);
  qr.addData(value);
  qr.make();
  const count = qr.getModuleCount();
  const size = count + 8;
  const cells = [];
  for (let y = 0; y < count; y++) {
    for (let x = 0; x < count; x++) {
      if (qr.isDark(y, x)) cells.push(`M${x + 4} ${y + 4}h1v1h-1z`);
    }
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}" shape-rendering="crispEdges"><path fill="white" d="M0 0h${size}v${size}H0z"/><path fill="black" d="${cells.join('')}"/></svg>`;
}

export function hostedDestination(vault, environment = process.env) {
  const binding = vault.load('hosted-binding');
  if (binding.connection_id !== environment.TANGLED_CONNECTION_ID) throw new ConnectorError('Hosted destination binding mismatch');
  return new Destination(environment.TANGLED_URL, binding.connection_id,
    () => vault.load('hosted-binding').token);
}
