import "./polyfills.js";
import "./style.css";
import {
  CONTRACT_ID, EXPLORER, NETWORK,
  initWallet, signIn, signOut, view, call, toYocto, fromYocto,
} from "./near.js";

const app = document.getElementById("app");
const state = {
  account: null,
  tab: "shows",
  events: [],
  ticketsByEvent: {},
  myTickets: [],
  nftMeta: {},
  nftEvent: null,
  loading: true,
  error: "",
  toast: "",
};

const esc = (s) =>
  String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

const when = (ms) =>
  new Date(ms).toLocaleString(undefined, { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

const STATUS = { OnSale: "On sale", Cancelled: "Cancelled, refunded", PaidOut: "Happened, paid out" };

/** Tickets the signed-in account holds for one show, by seat. */
const ticketsFor = (eventId) =>
  state.myTickets.filter((t) => t.event_id === eventId).sort((a, b) => a.seat - b.seat);

// -------------------------------------------------------------------
// Data
// -------------------------------------------------------------------

async function load() {
  if (!CONTRACT_ID) {
    state.loading = false;
    state.error = "Set VITE_CONTRACT_ID in frontend/.env to your deployed contract, then restart the dev server.";
    return render();
  }
  try {
    state.events = (await view("get_events", { from_index: 0, limit: 50 })).reverse();
    const lists = await Promise.all(state.events.map((e) => view("get_tickets_for_event", { event_id: e.id })));
    state.ticketsByEvent = Object.fromEntries(state.events.map((e, i) => [e.id, lists[i]]));
    if (state.account) {
      const [mine, tokens] = await Promise.all([
        view("get_tickets_for_owner", { account_id: state.account }),
        view("nft_tokens_for_owner", { account_id: state.account, from_index: "0", limit: 200 }),
      ]);
      state.myTickets = mine;
      state.nftMeta = Object.fromEntries(tokens.map((t) => [t.token_id, t.metadata]));
    } else {
      state.myTickets = [];
      state.nftMeta = {};
    }
    if (state.nftEvent !== null && !ticketsFor(state.nftEvent).length) state.nftEvent = null;
    state.error = "";
  } catch (e) {
    state.error = `Couldn't read the contract ${CONTRACT_ID} on ${NETWORK}. Check it's deployed. (${e.message})`;
  }
  state.loading = false;
  render();
}

async function act(label, fn) {
  if (!state.account) return signIn();
  try {
    state.toast = `${label}… confirm in your wallet`;
    render();
    await fn();
    state.toast = `${label}: done`;
    await load();
  } catch (e) {
    state.toast = `${label} didn't go through: ${e.message || e}`;
    render();
  }
  setTimeout(() => { state.toast = ""; render(); }, 5000);
}

// -------------------------------------------------------------------
// Views
// -------------------------------------------------------------------

function header() {
  return `
  <header class="top">
    <a class="brand" href="./" aria-label="SafeSeat home">
      <span class="brand-mark" aria-hidden="true"></span>SafeSeat
    </a>
    ${state.account
      ? `<button class="ghost" data-action="signout" title="Sign out">${esc(state.account)}</button>`
      : `<button class="primary" data-action="signin">Connect wallet</button>`}
  </header>`;
}

function hero() {
  return `
  <section class="hero">
    <h1>Your ticket holds your money until the show happens.</h1>
    <p>Cancelled? Every ticket refunds itself, instantly. Reselling? Never above the price the organizer allows.</p>
  </section>`;
}

function tabs() {
  const t = (id, label) =>
    `<button role="tab" aria-selected="${state.tab === id}" data-tab="${id}">${label}</button>`;
  return `<nav class="tabs" role="tablist">${t("shows", "Shows")}${t("mine", "My tickets")}${t("host", "Host a show")}</nav>`;
}

function eventCard(e) {
  const now = Date.now();
  const tickets = state.ticketsByEvent[e.id] || [];
  const isOrganizer = state.account === e.organizer;
  const started = now >= e.starts_at_ms;
  const canBuy = e.status === "OnSale" && !started && e.sold < e.capacity;
  const held = tickets.reduce((s, t) => s + BigInt(t.locked), 0n);
  const resales = tickets.filter((t) => t.listed_for && t.owner_id !== state.account);
  const payoutReady = now >= e.payout_after_ms;
  const mineHere = ticketsFor(e.id).length;

  return `
  <article class="show status-${e.status}">
    <div class="show-main">
      <h3>${esc(e.name)}</h3>
      <p class="meta">${esc(e.venue)}<br>${when(e.starts_at_ms)}</p>
      <dl class="facts">
        <div><dt>Ticket</dt><dd>${fromYocto(e.price)} NEAR</dd></div>
        <div><dt>Resale cap</dt><dd>${fromYocto(e.max_resale)} NEAR</dd></div>
        <div><dt>Seats sold</dt><dd>${e.sold} of ${e.capacity}</dd></div>
      </dl>
      <p class="status">${STATUS[e.status]}${e.status === "OnSale" && started ? ", doors open" : ""}</p>
      <div class="actions">
        ${canBuy ? `<button class="primary" data-action="buy" data-id="${e.id}" data-price="${e.price}">Buy a ticket for ${fromYocto(e.price)} NEAR</button>` : ""}
        ${e.status === "OnSale" && !started && e.sold >= e.capacity ? `<span class="soldout">Sold out</span>` : ""}
        ${mineHere ? `<button class="secondary" data-action="nfts" data-id="${e.id}">View your ${mineHere === 1 ? "NFT" : `${mineHere} NFTs`}</button>` : ""}
      </div>
      ${resales.length ? `
        <div class="resales">
          <h4>Resale tickets</h4>
          ${resales.map((t) => `
            <div class="resale-row">
              <span>Seat ${t.seat}</span>
              <button class="secondary" data-action="buy-resale" data-token="${esc(t.token_id)}" data-price="${t.listed_for}">Buy for ${fromYocto(t.listed_for)} NEAR</button>
            </div>`).join("")}
        </div>` : ""}
      ${isOrganizer && e.status === "OnSale" ? `
        <div class="organizer">
          <h4>You're hosting this</h4>
          <button class="danger" data-action="cancel" data-id="${e.id}">Cancel show and refund everyone</button>
          <button class="secondary" data-action="payout" data-id="${e.id}" ${payoutReady ? "" : "disabled"}>
            ${payoutReady ? `Collect ${fromYocto(held)} NEAR` : `Collect after ${when(e.payout_after_ms)}`}
          </button>
        </div>` : ""}
    </div>
    <aside class="stub" aria-label="Money held by tickets">
      ${e.status === "Cancelled"
        ? `<span class="stub-amount">Refunded</span><span class="stub-unit">to every ticket holder</span>`
        : e.status === "PaidOut"
        ? `<span class="stub-amount">Paid out</span><span class="stub-unit">to the organizer</span>`
        : `<span class="stub-amount">${fromYocto(held)}</span><span class="stub-unit">NEAR held for fans</span>`}
    </aside>
  </article>`;
}

function shows() {
  if (state.loading) return `<p class="empty">Loading shows…</p>`;
  if (!state.events.length)
    return `<div class="empty"><p>No shows yet.</p><button class="primary" data-tab="host">Host the first one</button></div>`;
  return state.events.map(eventCard).join("");
}

/** Deterministic artwork for a ticket NFT, drawn from the show and seat. */
function nftArt(e, t) {
  const hue = (e.id * 67 + 210) % 360;
  const sat = e.status === "OnSale" ? 72 : 8;
  const name = e.name.length > 22 ? `${e.name.slice(0, 21)}…` : e.name;
  const gid = `g-${String(t.token_id).replace(/[^a-zA-Z0-9-]/g, "_")}`;
  const font = (f, size, weight, op = 1) =>
    `fill="#fff" opacity="${op}" font-family="${f}, system-ui" font-size="${size}" font-weight="${weight}"`;
  return `
  <svg class="nft-art" viewBox="0 0 300 300" role="img" aria-label="Ticket artwork: ${esc(e.name)}, seat ${t.seat}">
    <defs>
      <linearGradient id="${gid}" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="hsl(${hue} ${sat}% 52%)"/>
        <stop offset="1" stop-color="hsl(${(hue + 50) % 360} ${sat}% 24%)"/>
      </linearGradient>
    </defs>
    <rect width="300" height="300" fill="url(#${gid})"/>
    <circle cx="250" cy="60" r="90" fill="#fff" opacity="0.08"/>
    <circle cx="40" cy="280" r="70" fill="#fff" opacity="0.06"/>
    <line x1="0" y1="214" x2="300" y2="214" stroke="#fff" stroke-opacity="0.45" stroke-width="2" stroke-dasharray="7 7"/>
    <circle cx="0" cy="214" r="14" fill="#000" opacity="0.25"/>
    <circle cx="300" cy="214" r="14" fill="#000" opacity="0.25"/>
    <text x="24" y="44" ${font("Atkinson Hyperlegible", 15, 700, 0.8)} letter-spacing="3">SAFESEAT · SEAT</text>
    <text x="24" y="152" ${font("Bricolage Grotesque", 104, 800)}>${t.seat}</text>
    <text x="24" y="190" ${font("Bricolage Grotesque", 22, 800)}>${esc(name)}</text>
    <text x="24" y="250" ${font("Atkinson Hyperlegible", 14, 400, 0.85)}>${esc(when(e.starts_at_ms))}</text>
    <text x="24" y="274" ${font("Atkinson Hyperlegible", 14, 400, 0.85)}>${e.status === "OnSale" ? `${fromYocto(t.locked)} NEAR held` : STATUS[e.status]}</text>
  </svg>`;
}

/** One ticket NFT: artwork, on-chain identity, and what the holder can do with it. */
function nftCard(e, t) {
  const open = e.status === "OnSale" && Date.now() < e.starts_at_ms;
  const meta = state.nftMeta[t.token_id];
  return `
  <article class="nft">
    ${nftArt(e, t)}
    <div class="nft-body">
      <h3>${esc(meta?.title || `${e.name} - Seat ${t.seat}`)}</h3>
      <dl class="nft-facts">
        <div><dt>Token ID</dt><dd><code>${esc(t.token_id)}</code></dd></div>
        <div><dt>Refund held</dt><dd>${e.status === "OnSale" ? `${fromYocto(t.locked)} NEAR` : STATUS[e.status]}</dd></div>
      </dl>
      ${open ? `
        ${t.listed_for
          ? `<p class="status">Listed for ${fromYocto(t.listed_for)} NEAR</p>
             <div class="actions"><button class="secondary" data-action="unlist" data-token="${esc(t.token_id)}">Take it off sale</button></div>`
          : `<form class="inline" data-form="list" data-token="${esc(t.token_id)}">
               <label>Resell for (max ${fromYocto(e.max_resale)} NEAR)
                 <input name="price" inputmode="decimal" placeholder="${fromYocto(e.price)}" required>
               </label>
               <button class="secondary">List for resale</button>
             </form>`}
        <form class="inline" data-form="gift" data-token="${esc(t.token_id)}">
          <label>Give to a friend
            <input name="to" placeholder="friend.testnet" autocomplete="off" required>
          </label>
          <button class="secondary">Send ticket</button>
        </form>` : ""}
      <p class="fine"><a href="${EXPLORER}/nft-token/${CONTRACT_ID}/${encodeURIComponent(t.token_id)}" target="_blank" rel="noopener">See this NFT on the explorer</a></p>
    </div>
  </article>`;
}

/** Gallery of every ticket NFT the user holds for the selected show. */
function nftDialog() {
  if (state.nftEvent === null) return "";
  const e = state.events.find((x) => x.id === state.nftEvent);
  const tickets = ticketsFor(state.nftEvent);
  if (!e || !tickets.length) return "";
  const n = tickets.length;
  return `
  <dialog class="nft-dialog" aria-labelledby="nft-title">
    <div class="nft-head">
      <div>
        <h2 id="nft-title">${esc(e.name)}</h2>
        <p class="meta">You hold ${n} ticket ${n === 1 ? "NFT" : "NFTs"} for this show</p>
      </div>
      <button class="ghost" data-action="close-nfts">Close</button>
    </div>
    <div class="nft-grid">${tickets.map((t) => nftCard(e, t)).join("")}</div>
  </dialog>`;
}

/** One card per show the user has booked, with how many tickets they hold. */
function myShowCard(e) {
  const tickets = ticketsFor(e.id);
  if (!tickets.length) return "";
  const n = tickets.length;
  const held = tickets.reduce((s, t) => s + BigInt(t.locked), 0n);
  const stubText =
    e.status === "Cancelled" ? ["Refunded", "to your wallet"]
    : e.status === "PaidOut" ? [n === 1 ? "Admit one" : `Admit ${n}`, "show happened"]
    : [fromYocto(held), "NEAR held for you"];
  return `
  <article class="show ticket status-${e.status}">
    <div class="show-main">
      <h3>${esc(e.name)}</h3>
      <p class="meta">${esc(e.venue)}<br>${when(e.starts_at_ms)}</p>
      <dl class="facts">
        <div><dt>Your tickets</dt><dd>${n}</dd></div>
        <div><dt>${n === 1 ? "Seat" : "Seats"}</dt><dd>${tickets.map((t) => t.seat).join(", ")}</dd></div>
      </dl>
      <div class="actions">
        <button class="primary" data-action="nfts" data-id="${e.id}">View your ${n === 1 ? "NFT" : `${n} NFTs`}</button>
      </div>
    </div>
    <aside class="stub"><span class="stub-amount">${stubText[0]}</span><span class="stub-unit">${stubText[1]}</span></aside>
  </article>`;
}

function mine() {
  if (!state.account)
    return `<div class="empty"><p>Connect your wallet to see your tickets.</p><button class="primary" data-action="signin">Connect wallet</button></div>`;
  if (!state.myTickets.length)
    return `<div class="empty"><p>You don't have tickets yet.</p><button class="primary" data-tab="shows">Browse shows</button></div>`;
  return state.events.map(myShowCard).join("");
}

function host() {
  const soon = new Date(Date.now() + 30 * 60_000);
  const local = new Date(soon.getTime() - soon.getTimezoneOffset() * 60_000).toISOString().slice(0, 16);
  return `
  <form class="host" data-form="create">
    <label>Show name<input name="name" maxlength="80" required placeholder="Lagos Live"></label>
    <label>Venue<input name="venue" maxlength="80" required placeholder="Eko Hall"></label>
    <label>Doors open<input name="starts" type="datetime-local" value="${local}" required></label>
    <div class="row">
      <label>Ticket price (NEAR)<input name="price" inputmode="decimal" value="1" required></label>
      <label>Resale cap (NEAR)<input name="cap" inputmode="decimal" value="1.2" required></label>
    </div>
    <div class="row">
      <label>Seats (1 to 50)<input name="capacity" type="number" min="1" max="50" value="20" required></label>
      <label>Your resale cut (%)<input name="royalty" type="number" min="0" max="20" step="0.5" value="5" required></label>
    </div>
    <label>Hold money after doors open for (minutes)
      <input name="delay" type="number" min="0" value="1440" required>
      <span class="hint">Fans are protected until this ends. 1440 is one day. Use 5 for a quick demo.</span>
    </label>
    <button class="primary">Create show</button>
  </form>`;
}

function render() {
  const body = state.tab === "shows" ? shows() : state.tab === "mine" ? mine() : host();
  app.innerHTML = `
    ${header()}
    <main>
      ${hero()}
      ${tabs()}
      ${state.error ? `<p class="error" role="alert">${esc(state.error)}</p>` : ""}
      <section class="panel">${body}</section>
    </main>
    <footer class="foot">Running on NEAR ${NETWORK}. Contract <a href="${EXPLORER}/address/${CONTRACT_ID}" target="_blank" rel="noopener">${esc(CONTRACT_ID || "not set")}</a></footer>
    ${nftDialog()}
    ${state.toast ? `<div class="toast" role="status">${esc(state.toast)}</div>` : ""}`;
  const dialog = app.querySelector(".nft-dialog");
  if (dialog) {
    dialog.addEventListener("close", () => {
      if (state.nftEvent === null) return;
      state.nftEvent = null;
      render();
    });
    // Clicking the dimmed backdrop closes the gallery.
    dialog.addEventListener("click", (ev) => { if (ev.target === dialog) dialog.close(); });
    dialog.showModal();
  }
}

// -------------------------------------------------------------------
// Events
// -------------------------------------------------------------------

app.addEventListener("click", (ev) => {
  const el = ev.target.closest("[data-tab],[data-action]");
  if (!el) return;
  if (el.dataset.tab) {
    state.tab = el.dataset.tab;
    return render();
  }
  const { action, id, price, token } = el.dataset;
  if (action === "signin") return signIn();
  if (action === "signout") return signOut();
  if (action === "nfts") {
    state.nftEvent = Number(id);
    return render();
  }
  if (action === "close-nfts") return el.closest("dialog").close();
  if (action === "buy")
    return act("Buying ticket", () => call("buy_ticket", { event_id: Number(id) }, { depositYocto: price }));
  if (action === "buy-resale")
    return act("Buying resale ticket", () => call("buy_resale", { token_id: token }, { depositYocto: price }));
  if (action === "unlist")
    return act("Taking ticket off sale", () => call("unlist_ticket", { token_id: token }));
  if (action === "cancel") {
    if (!confirm("Cancel this show? Every ticket holder is refunded right away. This can't be undone.")) return;
    return act("Cancelling and refunding", () => call("cancel_event", { event_id: Number(id) }, { tgas: 300 }));
  }
  if (action === "payout")
    return act("Collecting payout", () => call("claim_payout", { event_id: Number(id) }, { tgas: 300 }));
});

app.addEventListener("submit", (ev) => {
  ev.preventDefault();
  const form = ev.target;
  const f = Object.fromEntries(new FormData(form));
  const kind = form.dataset.form;
  try {
    if (kind === "create") {
      const args = {
        name: f.name.trim(),
        venue: f.venue.trim(),
        starts_at_ms: new Date(f.starts).getTime(),
        payout_delay_minutes: Number(f.delay),
        price: toYocto(f.price),
        max_resale: toYocto(f.cap),
        royalty_bps: Math.round(Number(f.royalty) * 100),
        capacity: Number(f.capacity),
      };
      return act("Creating show", async () => {
        await call("create_event", args);
        state.tab = "shows";
      });
    }
    if (kind === "list")
      return act("Listing ticket", () => call("list_ticket", { token_id: form.dataset.token, price: toYocto(f.price) }));
    if (kind === "gift")
      return act("Sending ticket", () =>
        call("nft_transfer", { receiver_id: f.to.trim(), token_id: form.dataset.token, memo: "gift" }, { depositYocto: "1" }));
  } catch (e) {
    state.toast = e.message;
    render();
  }
});

// -------------------------------------------------------------------
// Start
// -------------------------------------------------------------------

render();
initWallet((account) => {
  if (account !== state.account) {
    state.account = account;
    load();
  }
}).then((account) => {
  state.account = account;
  load();
}).catch((e) => {
  state.error = `Wallet setup failed: ${e.message}`;
  load();
});
