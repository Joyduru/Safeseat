// NEAR wallet libraries expect Node's Buffer in the browser.
import { Buffer } from "buffer";
globalThis.Buffer = globalThis.Buffer || Buffer;
globalThis.global = globalThis.global || globalThis;
// @near-js/accounts reads process.env at load time (via the Meteor wallet).
globalThis.process = globalThis.process || {
  env: {},
  browser: true,
  nextTick: (fn, ...args) => queueMicrotask(() => fn(...args)),
};
