// libsignal logs entire Signal sessions (including private keys) through the global
// console, independently of Baileys' logger. Silence it before importing the source
// libraries. The runner uses its own Console instance for deliberate safe output.
for (const method of ['log', 'info', 'warn', 'error', 'debug', 'trace', 'dir', 'table']) {
  console[method] = () => {};
}
