import { setupWalletSelector, actionCreators } from "@near-wallet-selector/core";
import { setupModal } from "@near-wallet-selector/modal-ui";
import { setupMyNearWallet } from "@near-wallet-selector/my-near-wallet";
import { setupMeteorWallet } from "@near-wallet-selector/meteor-wallet";
import "@near-wallet-selector/modal-ui/styles.css";

export const NETWORK = import.meta.env.VITE_NETWORK || "testnet";
// Defaults to the public demo contract on testnet, so the app works without a .env.
// Set VITE_CONTRACT_ID to use your own deployment.
const DEMO_CONTRACT = NETWORK === "testnet" ? "safeseat-demo-44511.testnet" : "";
export const CONTRACT_ID = import.meta.env.VITE_CONTRACT_ID || DEMO_CONTRACT;

const RPC = {
  testnet: ["https://test.rpc.fastnear.com", "https://rpc.testnet.near.org"],
  mainnet: ["https://free.rpc.fastnear.com", "https://rpc.mainnet.near.org"],
}[NETWORK];

export const EXPLORER = NETWORK === "mainnet" ? "https://nearblocks.io" : "https://testnet.nearblocks.io";

const TGAS = 1_000_000_000_000n;
const YOCTO_PER_NEAR = 10n ** 24n;

let selector;
let modal;

export async function initWallet(onChange) {
  selector = await setupWalletSelector({
    network: NETWORK,
    modules: [setupMyNearWallet(), setupMeteorWallet()],
  });
  modal = setupModal(selector, { contractId: CONTRACT_ID });
  selector.store.observable.subscribe((state) => {
    onChange(state.accounts.find((a) => a.active)?.accountId || null);
  });
  const accounts = selector.store.getState().accounts;
  return accounts.find((a) => a.active)?.accountId || null;
}

export function signIn() {
  modal.show();
}

export async function signOut() {
  const wallet = await selector.wallet();
  await wallet.signOut();
}

/** Read-only call straight to RPC. No wallet needed. */
export async function view(method, args = {}) {
  const body = {
    jsonrpc: "2.0",
    id: "safeseat",
    method: "query",
    params: {
      request_type: "call_function",
      finality: "optimistic",
      account_id: CONTRACT_ID,
      method_name: method,
      args_base64: btoa(JSON.stringify(args)),
    },
  };
  let lastError;
  for (const url of RPC) {
    try {
      const res = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      const json = await res.json();
      if (json.error) throw new Error(json.error.data || json.error.message);
      if (json.result?.error) throw new Error(json.result.error);
      const bytes = new Uint8Array(json.result.result);
      return JSON.parse(new TextDecoder().decode(bytes));
    } catch (e) {
      lastError = e;
    }
  }
  throw lastError;
}

/** State-changing call, signed by the user's wallet. */
export async function call(method, args = {}, { depositYocto = "0", tgas = 100 } = {}) {
  const wallet = await selector.wallet();
  return wallet.signAndSendTransaction({
    receiverId: CONTRACT_ID,
    actions: [
      actionCreators.functionCall(method, args, BigInt(tgas) * TGAS, BigInt(depositYocto)),
    ],
  });
}

/** "12.5" NEAR -> yocto string. Avoids floating point. */
export function toYocto(near) {
  const [whole, frac = ""] = String(near).trim().split(".");
  if (!/^\d+$/.test(whole || "0") || !/^\d*$/.test(frac)) throw new Error("Enter a number like 5 or 2.5");
  const fracPadded = (frac + "0".repeat(24)).slice(0, 24);
  return (BigInt(whole || "0") * YOCTO_PER_NEAR + BigInt(fracPadded)).toString();
}

/** yocto string -> "12.5" */
export function fromYocto(yocto) {
  const v = BigInt(yocto || "0");
  const whole = v / YOCTO_PER_NEAR;
  const frac = (v % YOCTO_PER_NEAR).toString().padStart(24, "0").slice(0, 4).replace(/0+$/, "");
  return frac ? `${whole}.${frac}` : whole.toString();
}
