# SafeSeat

**Event tickets that hold their own refund.** Built for NEAR HACKATHON 2026.

Tracks: NFTs That Do Something, Real-World Money.

## The problem

When you buy a ticket today, your money goes straight to the organizer. If the show is cancelled, you chase a refund for weeks. If it sells out, scalpers resell at 5x. Fans carry all the risk.

## What SafeSeat does

Your money doesn't go to the organizer when you buy. It stays locked inside your ticket, which is an NFT on NEAR, until one of three things happens:

| What happens | Where the money goes |
| --- | --- |
| The show happens | After a safety window (set by the organizer, e.g. 24h after doors open), the organizer collects. |
| The show is cancelled | Every ticket refunds whoever holds it, in the same transaction. No forms. |
| A fan resells | The price can't go above the organizer's cap. The seller is paid, the organizer gets a royalty, and the locked refund moves to the new holder with the ticket. |

The organizer is paid for delivering the show, not for selling tickets.

## Try it in 2 minutes (do X, see Y)

1. Open the app and connect a testnet wallet (MyNearWallet or Meteor).
2. **Host a show**: set doors open 10 minutes from now, price 1 NEAR, cap 1.2 NEAR, and "hold money after doors open" to 5 minutes. Click **Create show**. You see it under **Shows**, holding 0 NEAR.
3. From a second account, click **Buy a ticket for 1 NEAR**. The stub now shows **1 NEAR held for fans**.
4. In **My tickets**, try to resell for 3 NEAR. The contract rejects it: *Price is above the resale cap*.
5. List it for 1.2 NEAR and buy it from a third account. The ticket moves, and so does its 1 NEAR refund.
6. As the organizer, click **Cancel show and refund everyone**. The current holder gets 1 NEAR back immediately. Check it on testnet.nearblocks.io.

(Or skip step 6, wait until the safety window ends, and click **Collect** as the organizer.)

## Run it yourself

You need [Rust](https://rustup.rs), [cargo-near](https://github.com/near/cargo-near), [near-cli-rs](https://github.com/near/near-cli-rs) and Node 18+.

```bash
# 1. Test the contract (14 unit tests)
cd contract
cargo test

# 2. Build the wasm
cargo near build non-reproducible-wasm

# 3. Create a free testnet account for the contract
near account create-account sponsor-by-faucet-service safeseat-YOURNAME.testnet \
  autogenerate-new-keypair save-to-keychain network-config testnet create

# 4. Deploy (no init call needed, the contract starts empty)
cargo near deploy build-non-reproducible-wasm safeseat-YOURNAME.testnet \
  without-init-call network-config testnet sign-with-keychain send

# 5. Run the app
cd ../frontend
cp .env.example .env        # set VITE_CONTRACT_ID=safeseat-YOURNAME.testnet
npm install
npm run dev                 # open http://localhost:5173
```

If a CLI command's syntax has changed, run it with `--help`; the interactive mode (`near` with no arguments) walks you through the same steps.

### Quick check from the terminal

```bash
C=safeseat-YOURNAME.testnet
near contract call-function as-read-only $C get_events json-args '{}' network-config testnet now
```

## How it uses NEAR

- **NEP-171 NFTs**: tickets implement `nft_token`, `nft_tokens_for_owner`, `nft_transfer` and `nft_metadata`, and emit standard `nft_mint` / `nft_transfer` events, so wallets and explorers can show them.
- **Contract-held escrow**: each ticket records the refund it holds. Refunds and payouts are plain NEAR transfers made by the contract, so no one, including the organizer, can move locked money outside the rules.
- **Named accounts** (`ada.testnet`) make gifting a ticket as easy as typing a friend's name.

## Contract methods

| Method | Who | What it does |
| --- | --- | --- |
| `create_event(name, venue, starts_at_ms, payout_delay_minutes, price, max_resale, royalty_bps, capacity)` | Organizer | Creates a show. Amounts are yoctoNEAR strings. Max 50 seats, max 20% royalty. |
| `buy_ticket(event_id)` + deposit | Fan | Mints a ticket; the price is locked in it. Extra deposit is returned. |
| `list_ticket(token_id, price)` / `unlist_ticket` | Holder | Resale, capped by the organizer. |
| `buy_resale(token_id)` + deposit | Fan | Pays seller and royalty; ticket and its refund move to buyer. |
| `nft_transfer(receiver_id, token_id)` + 1 yocto | Holder | Gift; the refund goes with it. |
| `cancel_event(event_id)` | Organizer | Refunds every current holder now. |
| `claim_payout(event_id)` | Anyone | After the safety window, sends locked money to the organizer. |
| `get_events`, `get_event`, `get_tickets_for_event`, `get_tickets_for_owner`, `get_ticket` | Anyone | Views. |
| `nft_token`, `nft_tokens_for_owner`, `nft_supply_for_owner`, `nft_metadata` | Anyone | Standard NEP-171 views for wallets and explorers. |

## Known limits (honest list)

- **The safety window is time-based.** If an organizer simply doesn't show up and doesn't cancel, fans are protected only until the window ends. Next step: let ticket holders flag "the show didn't happen" during the window, which freezes the payout for review.
- **Gifts can hide side payments.** `nft_transfer` is free, so two people could agree on a price off-chain. The cap stops public scalping, not private deals.
- **Max 50 seats per show** so that cancelling refunds everyone in one transaction. Bigger shows would refund in batches.
- **The contract pays storage** for tickets from its own balance. Keep a few testnet NEAR on the contract account.
- **Refund equals face value.** If you bought a resale ticket above face value, a cancellation refunds face value; the cap limits how much that can be.

## Next steps

- Each ticket as its own NEAR sub-account (e.g. `seat-12.lagos-live.safeseat.near`) holding its money directly.
- Pay in USDC, and accept any token from any chain with NEAR Intents.
- Fan dispute flag during the safety window.

## Built with

Rust, near-sdk 5, Vite, NEAR Wallet Selector (MyNearWallet, Meteor). Code written during the hackathon with help from Claude.

## License

MIT
